use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::config::WrapperConfig;

pub struct ClangdProcess {
    pub child: Child,
    pub stdin: Arc<Mutex<ChildStdin>>,
    pub stdout: Arc<Mutex<ChildStdout>>,
    stderr_task: JoinHandle<()>,
}

impl ClangdProcess {
    pub async fn spawn(wrapper: &WrapperConfig, args: &[String]) -> Result<Self> {
        spawn_clangd(wrapper, args).await
    }

    pub async fn kill(&mut self) -> Result<()> {
        if let Err(err) = self.child.start_kill() {
            tracing::debug!(error = %err, "start_kill failed");
        }
        let _ = self.child.wait().await;
        Ok(())
    }
}

impl Drop for ClangdProcess {
    fn drop(&mut self) {
        self.stderr_task.abort();
    }
}

pub async fn spawn_clangd(wrapper: &WrapperConfig, args: &[String]) -> Result<ClangdProcess> {
    tracing::info!(
        clangd = %wrapper.clangd_path,
        args = ?args,
        "spawning clangd"
    );

    let mut command = Command::new(&wrapper.clangd_path);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("spawn clangd at {}", wrapper.clangd_path))?;

    let stdin = child
        .stdin
        .take()
        .context("take clangd stdin")?;
    let stdout = child
        .stdout
        .take()
        .context("take clangd stdout")?;
    let stderr = child
        .stderr
        .take()
        .context("take clangd stderr")?;

    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(target: "clangd", "{line}");
        }
    });

    Ok(ClangdProcess {
        child,
        stdin: Arc::new(Mutex::new(stdin)),
        stdout: Arc::new(Mutex::new(stdout)),
        stderr_task,
    })
}
