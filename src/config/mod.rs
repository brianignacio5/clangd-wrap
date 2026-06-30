pub mod project;
pub mod wrapper_yaml;

pub use project::{discover_project, ProjectPaths, CONFIG_FILE_NAMES};

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use wrapper_yaml::load_wrapper_yaml;

#[derive(Debug, Clone)]
pub struct WrapperConfig {
    pub clangd_path: String,
    pub log_level: String,
    pub watch_root: PathBuf,
}

impl WrapperConfig {
    pub fn load(config_path: Option<&Path>) -> Result<Self> {
        let mut config = Self {
            clangd_path: "clangd".to_string(),
            log_level: "error".to_string(),
            watch_root: std::env::current_dir().context("resolve watch root")?,
        };

        if let Some(path) = config_path {
            let yaml = load_wrapper_yaml(path)?;
            if let Some(clangd_path) = yaml.clangd_path {
                config.clangd_path = clangd_path;
            }
            if let Some(log_level) = yaml.log_level {
                config.log_level = log_level;
            }
            if let Some(watch_root) = yaml.watch_root {
                config.watch_root = if watch_root.is_relative() {
                    std::env::current_dir()
                        .context("resolve watch root")?
                        .join(watch_root)
                } else {
                    watch_root
                };
            }
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn load_without_config_uses_defaults() {
        let config = WrapperConfig::load(None).unwrap();
        assert_eq!(config.clangd_path, "clangd");
        assert_eq!(config.log_level, "error");
        assert_eq!(config.watch_root, std::env::current_dir().unwrap());
    }

    #[test]
    fn load_partial_yaml_overlays_defaults() {
        let dir = std::env::temp_dir().join(format!("clangd-wrap-cfg-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wrap.yaml");
        fs::write(&path, "log_level: debug\n").unwrap();

        let config = WrapperConfig::load(Some(&path)).unwrap();
        assert_eq!(config.clangd_path, "clangd");
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.watch_root, std::env::current_dir().unwrap());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_full_yaml() {
        let dir = std::env::temp_dir().join(format!("clangd-wrap-cfg-full-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wrap.yaml");
        fs::write(
            &path,
            "clangd_path: /usr/bin/clangd\nlog_level: trace\nwatch_root: /tmp/project\n",
        )
        .unwrap();

        let config = WrapperConfig::load(Some(&path)).unwrap();
        assert_eq!(config.clangd_path, "/usr/bin/clangd");
        assert_eq!(config.log_level, "trace");
        assert_eq!(config.watch_root, PathBuf::from("/tmp/project"));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_invalid_yaml_fails() {
        let dir = std::env::temp_dir().join(format!("clangd-wrap-cfg-bad-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wrap.yaml");
        fs::write(&path, "log_level: [not, a, string]\nclangd_path: \n").unwrap();

        let err = WrapperConfig::load(Some(&path)).unwrap_err();
        assert!(err.to_string().contains("parse"));

        let _ = fs::remove_dir_all(dir);
    }
}
