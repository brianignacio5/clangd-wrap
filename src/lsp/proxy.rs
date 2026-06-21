use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncWriteExt, stdout, Stdout};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::clangd::spawn_clangd;
use crate::lsp::framing::{read_message, write_message};
use crate::lsp::session::SharedState;

pub struct Proxy {
    shared: Arc<Mutex<SharedState>>,
    done: Arc<tokio::sync::Notify>,
    client_task: Option<JoinHandle<anyhow::Result<()>>>,
    clangd_task: Option<JoinHandle<anyhow::Result<()>>>,
}

impl Proxy {
    pub fn new(shared: Arc<Mutex<SharedState>>) -> Self {
        Self {
            shared,
            done: Arc::new(tokio::sync::Notify::new()),
            client_task: None,
            clangd_task: None,
        }
    }

    pub async fn spawn(&mut self) -> Result<()> {
        {
            let mut state = self.shared.lock().await;
            let args = state.merged_clangd_args();
            let wrapper = state.wrapper_config.clone();
            let process = spawn_clangd(&wrapper, &args)
                .await
                .context("spawn initial clangd")?;
            state.clangd = Some(process);
        }

        let shared = Arc::clone(&self.shared);
        let done = Arc::clone(&self.done);

        let client_task = tokio::spawn(async move { run_client_to_clangd(shared).await });
        let shared = Arc::clone(&self.shared);
        let clangd_task = tokio::spawn(async move { run_clangd_to_client(shared, done).await });

        self.client_task = Some(client_task);
        self.clangd_task = Some(clangd_task);
        Ok(())
    }

    pub async fn wait_until_done(&self) -> Result<()> {
        self.done.notified().await;
        Ok(())
    }
}

async fn run_client_to_clangd(shared: Arc<Mutex<SharedState>>) -> Result<()> {
    let mut stdin = tokio::io::stdin();

    loop {
        let message = read_message(&mut stdin)
            .await
            .context("read LSP message from client")?;

        let Some(message) = message else {
            break;
        };

        let maybe_stdin = {
            let mut state = shared.lock().await;
            state.observe_client_message(&message);

            if message.get("method") == Some(&Value::from("exit")) {
                if let Some(process) = state.clangd.as_ref() {
                    let stdin_handle = Arc::clone(&process.stdin);
                    drop(state);
                    let mut stdin_lock = stdin_handle.lock().await;
                    let _ = write_message(&mut *stdin_lock, &message).await;
                    let mut state = shared.lock().await;
                    if let Some(mut process) = state.clangd.take() {
                        let _ = process.kill().await;
                    }
                }
                break;
            }

            if state.restarting {
                None
            } else {
                state
                    .clangd
                    .as_ref()
                    .map(|process| Arc::clone(&process.stdin))
            }
        };

        let Some(stdin_handle) = maybe_stdin else {
            continue;
        };

        let mut stdin_lock = stdin_handle.lock().await;
        write_message(&mut *stdin_lock, &message)
            .await
            .context("forward client message to clangd")?;
    }

    Ok(())
}

async fn run_clangd_to_client(
    shared: Arc<Mutex<SharedState>>,
    done: Arc<tokio::sync::Notify>,
) -> Result<()> {
    let mut stdout_writer = stdout();

    loop {
        let stdout_handle = {
            let state = shared.lock().await;
            if state.restarting || state.clangd.is_none() {
                None
            } else {
                state.clangd.as_ref().map(|process| Arc::clone(&process.stdout))
            }
        };

        let Some(stdout_handle) = stdout_handle else {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            continue;
        };

        let message = {
            let mut stdout_lock = stdout_handle.lock().await;
            read_message(&mut *stdout_lock)
                .await
                .context("read LSP message from clangd")?
        };

        let Some(message) = message else {
            let restarting = shared.lock().await.restarting;
            if restarting {
                continue;
            }
            break;
        };

        write_message_to_stdout(&mut stdout_writer, &message)
            .await
            .context("forward clangd message to client")?;
    }

    done.notify_waiters();
    Ok(())
}

async fn write_message_to_stdout(writer: &mut Stdout, message: &Value) -> Result<()> {
    let frame = crate::lsp::framing::encode_message(message)?;
    writer
        .write_all(&frame)
        .await
        .context("write LSP frame to stdout")?;
    writer.flush().await.context("flush stdout")?;
    Ok(())
}
