use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::time::timeout;

use crate::config::parse_clangd_file;
use crate::lsp::framing::{read_message, write_message};
use crate::lsp::session::SharedState;
use crate::tasks::RestartContext;
use crate::watcher::ConfigChangeEvent;

use super::{spawn_clangd, ClangdProcess};

pub async fn handle_config_change(
    state: &mut SharedState,
    event: ConfigChangeEvent,
) -> Result<()> {
    if state.restarting {
        tracing::debug!("restart already in progress, skipping duplicate event");
        return Ok(());
    }

    state.restarting = true;
    let result = restart_clangd(state, event).await;
    state.restarting = false;
    result
}

async fn restart_clangd(state: &mut SharedState, event: ConfigChangeEvent) -> Result<()> {
    tracing::info!(path = %event.path.display(), "config changed, restarting clangd");

    if let Some(mut process) = state.clangd.take() {
        graceful_shutdown(&mut process, state).await?;
        process.kill().await?;
    }

    run_restart_tasks(state, &event).await?;

    let args = state.merged_clangd_args();
    let mut process = spawn_clangd(&state.wrapper_config, &args)
        .await
        .context("respawn clangd after config change")?;

    state.bump_restart_generation();
    replay_session(state, &mut process).await?;

    state.clangd = Some(process);
    Ok(())
}

async fn graceful_shutdown(process: &mut ClangdProcess, state: &mut SharedState) -> Result<()> {
    let shutdown_id = state.allocate_internal_id();
    let shutdown = json!({
        "jsonrpc": "2.0",
        "id": shutdown_id,
        "method": "shutdown",
        "params": null,
    });

    {
        let mut stdin = process.stdin.lock().await;
        write_message(&mut *stdin, &shutdown)
            .await
            .context("send shutdown to clangd")?;
    }

    let stdout = Arc::clone(&process.stdout);
    let wait = async move {
        loop {
            let message = {
                let mut stdout_lock = stdout.lock().await;
                read_message(&mut *stdout_lock)
                    .await
                    .context("read shutdown response")?
            };

            let Some(message) = message else {
                break;
            };

            if message.get("id") == Some(&Value::from(shutdown_id)) {
                break;
            }
        }
        Ok::<(), anyhow::Error>(())
    };

    if timeout(Duration::from_secs(5), wait).await.is_err() {
        tracing::warn!("timed out waiting for clangd shutdown response");
    }

    let exit = json!({
        "jsonrpc": "2.0",
        "method": "exit",
    });
    let mut stdin = process.stdin.lock().await;
    let _ = write_message(&mut *stdin, &exit).await;
    Ok(())
}

async fn run_restart_tasks(state: &mut SharedState, event: &ConfigChangeEvent) -> Result<()> {
    let clangd_config = parse_clangd_file(&state.project.clangd_path).unwrap_or_default();

    let mut ctx = RestartContext {
        project_root: state.project.root.clone(),
        changed_path: event.path.clone(),
        file_hash: event.hash.clone(),
        file_contents: event.contents.clone(),
        clangd_config,
        injected_args: state.injected_args.clone(),
        user_args: state.user_args.clone(),
    };

    for task in &state.restart_tasks {
        tracing::debug!(task = task.name(), "running restart task");
        task.run(&mut ctx)
            .with_context(|| format!("restart task `{}` failed", task.name()))?;
    }

    state.injected_args = ctx.injected_args;
    Ok(())
}

async fn replay_session(state: &mut SharedState, process: &mut ClangdProcess) -> Result<()> {
    let replay = state.replay_messages();

    for (idx, message) in replay.into_iter().enumerate() {
        if message.get("method") == Some(&Value::from("initialize")) {
            read_initialize_response(process, &message).await?;
            continue;
        }

        let mut stdin = process.stdin.lock().await;
        write_message(&mut *stdin, &message)
            .await
            .with_context(|| format!("replay message #{idx}"))?;
    }

    Ok(())
}

async fn read_initialize_response(
    process: &mut ClangdProcess,
    initialize: &Value,
) -> Result<Value> {
    {
        let mut stdin = process.stdin.lock().await;
        write_message(&mut *stdin, initialize)
            .await
            .context("send initialize replay")?;
    }

    let init_id = initialize.get("id").cloned();
    let stdout = Arc::clone(&process.stdout);

    loop {
        let message = {
            let mut stdout_lock = stdout.lock().await;
            read_message(&mut *stdout_lock)
                .await
                .context("read initialize response")?
        };

        let Some(message) = message else {
            anyhow::bail!("unexpected EOF while waiting for initialize response");
        };

        if init_id.as_ref() == message.get("id") {
            return Ok(message);
        }
    }
}
