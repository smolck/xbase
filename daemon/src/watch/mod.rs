mod event;
mod serialize;

pub use event::{Event, EventKind};

use crate::compile::ensure_server_support;
use crate::{constants::DAEMON_STATE, state::State, Result};
use async_trait::async_trait;
use log::{error, info, trace};
use notify::{Config, RecommendedWatcher, RecursiveMode::Recursive, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tokio::sync::mpsc::channel;
use tokio::{sync::MutexGuard, task::JoinHandle};
use xbase_proto::{Client, IntoResult};

#[derive(derive_deref_rs::Deref)]
pub struct WatchService {
    #[deref]
    pub listeners: HashMap<String, Box<(dyn Watchable + Send + Sync + 'static)>>,
    pub handler: JoinHandle<Result<()>>,
}

pub struct InternalState {
    debounce: Arc<Mutex<SystemTime>>,
    last_path: Arc<Mutex<PathBuf>>,
}

/// Trait to make an object react to filesystem changes.
///
/// ToString is required in order to store watchable in HashMap
#[async_trait]
pub trait Watchable: ToString + Send + Sync + 'static {
    /// Trigger Restart of Watchable.
    async fn trigger(&self, state: &MutexGuard<State>, event: &Event) -> Result<()>;

    /// A function that controls whether a a Watchable should restart
    async fn should_trigger(&self, state: &MutexGuard<State>, event: &Event) -> bool;

    /// A function that controls whether a watchable should be dropped
    async fn should_discard(&self, state: &MutexGuard<State>, event: &Event) -> bool;

    /// Drop watchable for watching a given file system
    async fn discard(&self, state: &MutexGuard<State>) -> Result<()>;
}

impl WatchService {
    pub async fn new(client: Client, ignore_pattern: Vec<String>) -> Result<Self> {
        let listeners = Default::default();

        async fn try_to_recompile<'a>(
            event: &Event,
            client: &Client,
            state: &mut MutexGuard<'a, State>,
        ) -> Result<()> {
            let recompiled = event.is_create_event()
                || event.is_remove_event()
                || event.is_content_update_event()
                || event.is_rename_event() && !event.is_seen();

            if recompiled {
                let ensure = ensure_server_support(state, client, Some(event)).await;
                match ensure {
                    Err(err) => {
                        log::error!("Ensure server support Errored!! {err:?} ");
                    }
                    Ok(true) => {
                        let ref name = client.abbrev_root();
                        state
                            .clients
                            .echo_msg(&client.root, name, "new compilation database generated ✅")
                            .await;
                        info!("[{name}] recompiled successfully");
                    }
                    _ => (),
                }
            };

            Ok(())
        }

        let handler = tokio::spawn(async move {
            let mut discards = vec![];
            let ref root = client.root;
            let internal_state = InternalState::default();

            let (tx, mut rx) = channel::<notify::Event>(1);
            let mut w = <RecommendedWatcher as Watcher>::new(move |res| {
                if let Ok(event) = res {
                    tx.blocking_send(event).unwrap()
                }
            })
            .map_err(|e| crate::Error::Unexpected(e.to_string()))?;
            w.watch(&client.root, Recursive)
                .map_err(|e| crate::Error::Unexpected(e.to_string()))?;
            w.configure(Config::NoticeEvents(true))
                .map_err(|e| crate::Error::Unexpected(e.to_string()))?;

            let ignore_pattern = ignore_pattern
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<&str>>();

            let ignore = wax::any::<wax::Glob, _>(ignore_pattern).unwrap();

            while let Some(event) = rx.recv().await {
                let ref event = match Event::new(&ignore, &internal_state, event) {
                    Some(e) => e,
                    None => continue,
                };

                // IGNORE EVENTS OF RENAME FOR PATHS THAT NO LONGER EXISTS
                if !event.path().exists() && event.is_rename_event() {
                    log::debug!("{} [ignored]", event);
                    continue;
                }

                let state = DAEMON_STATE.clone();
                let ref mut state = state.lock().await;

                try_to_recompile(event, &client, state).await?;

                let watcher = match state.watcher.get(root) {
                    Ok(w) => w,
                    Err(err) => {
                        error!(r#"Unable to get watcher for {root:?}: {err}"#);
                        info!(r#"Dropping watcher for {root:?}: {err}"#);
                        break;
                    }
                };

                for (key, listener) in watcher.listeners.iter() {
                    if listener.should_discard(state, event).await {
                        if let Err(err) = listener.discard(state).await {
                            error!(" discard errored for `{key}`!: {err}");
                        }
                        discards.push(key.to_string());
                    } else if listener.should_trigger(state, event).await {
                        if let Err(err) = listener.trigger(state, event).await {
                            error!("trigger errored for `{key}`!: {err}");
                        }
                    }
                }
                let watcher = state.watcher.get_mut(root).unwrap();

                for key in discards.iter() {
                    info!("[{key:?}] discarded");
                    watcher.listeners.remove(key);
                }

                discards.clear();
                internal_state.update_debounce();

                info!("{event} consumed successfully");
            }

            info!("Dropped {:?}!!", client.root);

            Ok(())
        });

        Ok(Self { handler, listeners })
    }

    pub fn add<W: Watchable>(&mut self, watchable: W) -> Result<()> {
        let key = watchable.to_string();
        info!(r#"Add: {key:?}"#);

        let other = self.listeners.insert(key, Box::new(watchable));
        if let Some(watchable) = other {
            let key = watchable.to_string();
            error!("Watchable with `{key}` already exists!")
        }

        Ok(())
    }

    pub fn remove(&mut self, key: &String) -> Result<Box<dyn Watchable>> {
        info!("Remove: `{key}`");
        let item = self.listeners.remove(key).into_result("Watchable", key)?;
        Ok(item)
    }
}

impl Default for InternalState {
    fn default() -> Self {
        Self {
            debounce: Arc::new(Mutex::new(SystemTime::now())),
            last_path: Default::default(),
        }
    }
}

impl InternalState {
    pub fn update_debounce(&self) {
        let mut debounce = self.debounce.lock().unwrap();
        *debounce = SystemTime::now();
        trace!("Debounce updated!!!");
    }

    pub fn last_run(&self) -> u128 {
        self.debounce.lock().unwrap().elapsed().unwrap().as_millis()
    }

    /// Get a reference to the internal state's last path.
    #[must_use]
    pub fn last_path(&self) -> Arc<Mutex<PathBuf>> {
        self.last_path.clone()
    }
}
