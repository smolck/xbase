mod bin;
mod handler;
mod service;
mod simulator;

use crate::constants::DAEMON_STATE;
use crate::device::Device;
use crate::nvim::Logger;
use crate::state::State;
use crate::Error;
use crate::{RequestHandler, Result};
use async_trait::async_trait;
use process_stream::Process;
use tokio::sync::MutexGuard;
use xbase_proto::{BuildSettings, Client, RunRequest};

pub use service::RunService;
pub use {bin::*, simulator::*};

#[async_trait::async_trait]
pub trait Runner {
    /// Run Project
    async fn run<'a>(&self, logger: &mut Logger<'a>) -> Result<Process>;
}

#[async_trait]
impl RequestHandler for RunRequest {
    async fn handle(self) -> Result<()>
    where
        Self: Sized + std::fmt::Debug,
    {
        let (title, sep) = crate::util::handler_log_content("Run", &self.client);
        log::info!("{sep}");
        log::info!("{title}");
        log::trace!("\n\n{:#?}\n", &self);
        log::info!("{sep}");

        let ref key = self.to_string();
        let state = DAEMON_STATE.clone();
        let ref mut state = state.lock().await;

        if self.ops.is_once() {
            // TODO(run): might want to keep track of ran services
            RunService::new(state, self).await?;
            return Ok(());
        }

        let client = self.client.clone();
        if self.ops.is_watch() {
            let watcher = state.watcher.get(&self.client.root)?;
            if watcher.contains_key(key) {
                state
                    .clients
                    .get(&self.client.pid)?
                    .echo_err("Already watching with {key}!!")
                    .await?;
            } else {
                let pid = self.client.pid.to_owned();
                let run_service = RunService::new(state, self).await?;
                let watcher = state.watcher.get_mut(&client.root)?;
                watcher.add(run_service)?;
                state.clients.get(&pid)?.set_watching(true).await?;
            }
        } else {
            log::info!("[{}] stopping .....", &self.settings.method.format_for_log_info());
            let watcher = state.watcher.get_mut(&self.client.root)?;
            let listener = watcher.remove(&self.to_string())?;
            state
                .clients
                .get(&self.client.pid)?
                .set_watching(false)
                .await?;
            listener.discard(state).await?;
        }

        state.sync_client_state().await?;

        log::info!("{sep}",);
        log::info!("{sep}",);

        Ok(())
    }
}

async fn get_runner<'a>(
    state: &'a MutexGuard<'_, State>,
    client: &Client,
    settings: &BuildSettings,
    device: Option<&Device>,
    is_once: bool,
) -> Result<process_stream::Process> {
    let root = &client.root;
    let nvim = state.clients.get(&client.pid)?;

    let logger = &mut nvim.logger();

    if !is_once {
        logger.open_win().await?;
        logger.set_running(false).await?;
    }

    let (runner, stream, args) = state.projects.get(root)?.get_runner(&settings, device)?;

    logger.set_title(format!("Build:{}", settings.method.scheme_or_target()));

    // TODO(smolck): Better naming
    let log_info_info = settings.method.format_for_log_info();

    log::info!("[{}] building .....", log_info_info);

    let success = logger.consume_build_logs(stream, true, !is_once).await?;
    if !success {
        let msg = format!("[{}] failed to be built", log_info_info);
        logger.nvim.echo_err(&msg).await?;
        log::error!("[{}] failed to be built", log_info_info);
        log::error!("[ran: 'xcodebuild {}']", args.join(" "));
        return Err(Error::Build(msg));
    } else {
        log::info!("[{}] built successfully", log_info_info);
    }

    logger.set_title(format!("Run:{}", settings.method.scheme_or_target()));
    logger.set_running(true).await?;

    let process = runner.run(logger).await?;
    log::info!("[{}] running .....", log_info_info);

    Ok(process)
}
