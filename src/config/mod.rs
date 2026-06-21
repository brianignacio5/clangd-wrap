pub mod clangd_yaml;
pub mod project;

pub use clangd_yaml::{build_injected_args, injected_args_from_clangd, parse_clangd_file, ClangdConfig};
pub use project::{discover_project, ProjectPaths, CONFIG_FILE_NAMES};

use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct WrapperConfig {
    pub clangd_path: String,
    pub log_level: String,
    pub watch_root: PathBuf,
}

impl WrapperConfig {
    pub fn from_env() -> Result<Self> {
        let watch_root = std::env::var("CLANGD_WRAP_WATCH_ROOT")
            .map(PathBuf::from)
            .or_else(|_| std::env::current_dir())
            .context("resolve watch root")?;

        Ok(Self {
            clangd_path: std::env::var("CLANGD_PATH").unwrap_or_else(|_| "clangd".to_string()),
            log_level: std::env::var("CLANGD_WRAP_LOG").unwrap_or_else(|_| "error".to_string()),
            watch_root,
        })
    }
}
