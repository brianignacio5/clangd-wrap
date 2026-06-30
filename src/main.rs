mod cli;
mod clangd;
mod config;
mod lsp;
mod tasks;
mod watcher;

use std::sync::Arc;

use anyhow::Context;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use crate::config::WrapperConfig;
use crate::lsp::proxy::Proxy;
use crate::tasks::default_pipeline;
use crate::watcher::ConfigWatcher;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let parsed = cli::parse_cli();
    let wrapper_config = WrapperConfig::load(parsed.config.as_deref())?;
    let user_args = parsed.clangd_args;

    init_tracing(&wrapper_config);

    let project = config::discover_project(&wrapper_config.watch_root, &user_args)?;

    tracing::debug!(
        watch_root = %wrapper_config.watch_root.display(),
        clangd = %wrapper_config.clangd_path,
        user_arg_count = user_args.len(),
        "starting clangd-wrap"
    );

    let restart_tasks = default_pipeline();
    let shared = Arc::new(Mutex::new(lsp::session::SharedState::new(
        user_args.clone(),
        restart_tasks,
        wrapper_config.clone(),
        project.clone(),
    )));

    let (restart_tx, mut restart_rx) = tokio::sync::mpsc::channel(8);
    let _watcher = ConfigWatcher::new(project.clone(), restart_tx)?;

    let mut proxy = Proxy::new(shared.clone());
    proxy.spawn().await.context("spawn LSP proxy")?;

    loop {
        tokio::select! {
            changed = restart_rx.recv() => {
                let Some(event) = changed else { break; };
                let mut state = shared.lock().await;
                if let Err(err) = clangd::handle_config_change(&mut state, event).await {
                    tracing::error!(error = %err, "restart failed");
                }
            }
            result = proxy.wait_until_done() => {
                result.context("proxy exited")?;
                break;
            }
        }
    }

    Ok(())
}

fn init_tracing(config: &WrapperConfig) {
    let filter = EnvFilter::try_new(&config.log_level)
        .unwrap_or_else(|_| EnvFilter::new("error"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}
