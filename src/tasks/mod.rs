use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing::info;

pub struct RestartContext {
    pub project_root: PathBuf,
    pub changed_path: PathBuf,
    pub file_hash: String,
    pub file_contents: Option<String>,
    pub user_args: Vec<String>,
}

pub trait RestartTask: Send + Sync {
    fn name(&self) -> &str;
    fn run(&self, ctx: &mut RestartContext) -> Result<()>;
}

pub struct LogChangeTask;

impl RestartTask for LogChangeTask {
    fn name(&self) -> &str {
        "log_change"
    }

    fn run(&self, ctx: &mut RestartContext) -> Result<()> {
        info!(
            changed = %ctx.changed_path.display(),
            hash = %ctx.file_hash,
            "project configuration changed"
        );
        Ok(())
    }
}

pub struct ValidateCompileCommandsTask;

impl RestartTask for ValidateCompileCommandsTask {
    fn name(&self) -> &str {
        "validate_compile_commands"
    }

    fn run(&self, ctx: &mut RestartContext) -> Result<()> {
        if !is_compile_commands_path(&ctx.changed_path) {
            return Ok(());
        }

        let Some(contents) = &ctx.file_contents else {
            tracing::warn!(
                path = %ctx.changed_path.display(),
                "compile_commands.json missing contents during validation"
            );
            return Ok(());
        };

        match serde_json::from_str::<serde_json::Value>(contents) {
            Ok(value) if value.is_array() => Ok(()),
            Ok(_) => {
                tracing::warn!(
                    path = %ctx.changed_path.display(),
                    "compile_commands.json root is not a JSON array"
                );
                Ok(())
            }
            Err(err) => {
                tracing::warn!(
                    path = %ctx.changed_path.display(),
                    error = %err,
                    "compile_commands.json failed JSON validation"
                );
                Ok(())
            }
        }
    }
}

pub fn default_pipeline() -> Vec<Box<dyn RestartTask>> {
    vec![
        Box::new(LogChangeTask),
        Box::new(ValidateCompileCommandsTask),
    ]
}

fn is_compile_commands_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "compile_commands.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn validate_compile_commands_accepts_array() {
        let mut ctx = RestartContext {
            project_root: PathBuf::from("."),
            changed_path: PathBuf::from("compile_commands.json"),
            file_hash: "abc".to_string(),
            file_contents: Some("[]".to_string()),
            user_args: vec![],
        };

        ValidateCompileCommandsTask.run(&mut ctx).unwrap();
    }
}
