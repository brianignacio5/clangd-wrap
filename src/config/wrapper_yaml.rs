use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct WrapperYamlPartial {
    pub clangd_path: Option<String>,
    pub log_level: Option<String>,
    pub watch_root: Option<PathBuf>,
}

pub fn load_wrapper_yaml(path: &Path) -> Result<WrapperYamlPartial> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;

    serde_yaml::from_str(&contents).with_context(|| format!("parse {}", path.display()))
}
