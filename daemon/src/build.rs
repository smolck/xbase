use crate::constants::DAEMON_STATE;
use crate::state::State;
use crate::watch::{Event, Watchable};
use crate::RequestHandler;
use crate::Result;
use async_trait::async_trait;
use tokio::sync::MutexGuard;
use xbase_proto::BuildRequest;

#[async_trait]
impl RequestHandler for BuildRequest {
    async fn handle(self) -> Result<()>
    where
        Self: Sized + std::fmt::Debug,
    {
        let state = DAEMON_STATE.clone();
        let ref mut state = state.lock().await;

        let (title, sep) = crate::util::handler_log_content("Build", &self.client);
        log::info!("{sep}");
        log::info!("{title}");
        log::trace!("\n\n{:#?}\n", &self);
        log::info!("{sep}");

        if self.ops.is_once() {
            return self.trigger(state, &Event::default()).await;
        }

        if self.ops.is_watch() {
            state
                .clients
                .get(&self.client.pid)?
                .set_watching(true)
                .await?;
            state.watcher.get_mut(&self.client.root)?.add(self)?;
        } else {
            state
                .clients
                .get(&self.client.pid)?
                .set_watching(false)
                .await?;
            state
                .watcher
                .get_mut(&self.client.root)?
                .remove(&self.to_string())?;
        }

        state.sync_client_state().await?;

        Ok(())
    }
}

#[async_trait]
impl Watchable for BuildRequest {
    async fn trigger(&self, state: &MutexGuard<State>, _event: &Event) -> Result<()> {
        let is_once = self.ops.is_once();
        let root = &self.client.root;
        let (stream, args) = state.projects.get(root)?.build(&self.settings, None)?;
        let nvim = state.clients.get(&self.client.pid)?;
        let logger = &mut nvim.logger();

        if let xbase_proto::BuildMethod::WithTarget(ref target) = self.settings.method {
            logger.set_title(format!(
                "{}:{}",
                if is_once { "Build" } else { "Rebuild" },
                target
            ));

            log::info!("[target: {}] building .....", target);
            let success = logger.consume_build_logs(stream, false, !is_once).await?;
            if !success {
                let ref msg = format!("Failed: {} ", self.settings.to_string());
                nvim.echo_err(msg).await?;
                log::error!("[target: {}] failed to be built", target);
                log::error!("[ran: 'xcodebuild {}']", args.join(" "));
            } else {
                log::info!("[target: {}] built successfully", target);
            };
        } else {
            let scheme = self.settings.scheme().unwrap();
            logger.set_title(format!(
                "{}:{}",
                if is_once { "Build" } else { "Rebuild" },
                scheme
            ));

            log::info!("[scheme: {}] building .....", scheme);
            let success = logger.consume_build_logs(stream, false, !is_once).await?;
            if !success {
                let ref msg = format!("Failed: {} ", self.settings.to_string());
                nvim.echo_err(msg).await?;
                log::error!("[scheme: {}] failed to be built", scheme);
                log::error!("[ran: 'xcodebuild {}']", args.join(" "));
            } else {
                log::info!("[scheme: {}] built successfully", scheme);
            };
        }

        Ok(())
    }

    /// A function that controls whether a a Watchable should restart
    async fn should_trigger(&self, _state: &MutexGuard<State>, event: &Event) -> bool {
        event.is_content_update_event()
            || event.is_rename_event()
            || event.is_create_event()
            || event.is_remove_event()
            || !(event.path().exists() || event.is_seen())
    }

    /// A function that controls whether a watchable should be droped
    async fn should_discard(&self, _state: &MutexGuard<State>, _event: &Event) -> bool {
        false
    }

    /// Drop watchable for watching a given file system
    async fn discard(&self, _state: &MutexGuard<State>) -> Result<()> {
        Ok(())
    }
}
