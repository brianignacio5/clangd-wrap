use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::config::ProjectPaths;

#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    pub path: PathBuf,
    pub hash: String,
    pub contents: Option<String>,
}

pub struct ConfigWatcher {
    _debouncer: Debouncer<RecommendedWatcher, FileIdMap>,
}

impl ConfigWatcher {
    pub fn new(project: ProjectPaths, restart_tx: mpsc::Sender<ConfigChangeEvent>) -> Result<Self> {
        let mut initial_hashes = HashMap::new();
        for path in &project.watch_paths {
            if path.exists() {
                if let Ok(hash) = hash_file(path) {
                    initial_hashes.insert(path.clone(), hash);
                }
            }
        }

        let hash_state = Arc::new(Mutex::new(initial_hashes));
        let watch_paths = project.watch_paths.clone();

        let mut debouncer = new_debouncer(
            Duration::from_millis(400),
            None,
            move |result: DebounceEventResult| {
                let Ok(events) = result else {
                    tracing::warn!("file watcher error");
                    return;
                };

                for event in events {
                    for path in &event.paths {
                        if !is_watched_path(path, &watch_paths) {
                            continue;
                        }

                        if should_ignore_file(path) {
                            continue;
                        }

                        let Ok(hash) = hash_file(path) else {
                            continue;
                        };

                        let mut hashes = hash_state.lock().expect("hash state lock");
                        if hashes.get(path) == Some(&hash) {
                            continue;
                        }
                        hashes.insert(path.clone(), hash.clone());

                        let contents = std::fs::read_to_string(path).ok();
                        let change = ConfigChangeEvent {
                            path: path.clone(),
                            hash,
                            contents,
                        };

                        if restart_tx.blocking_send(change).is_err() {
                            tracing::debug!("restart channel closed");
                            return;
                        }
                    }
                }
            },
        )
        .context("create file debouncer")?;

        if project.root.is_dir() {
            debouncer
                .watcher()
                .watch(&project.root, RecursiveMode::Recursive)
                .with_context(|| format!("watch {}", project.root.display()))?;
            debouncer
                .cache()
                .add_root(&project.root, RecursiveMode::Recursive);
        }

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}

fn is_watched_path(path: &Path, watch_paths: &[PathBuf]) -> bool {
    watch_paths.iter().any(|watched| watched == path)
}

fn should_ignore_file(path: &Path) -> bool {
    let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");

    if file_name == "compile_commands.json" {
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() == 0 {
                tracing::debug!(path = %path.display(), "ignoring empty compile_commands.json");
                return true;
            }
        }
    }

    false
}

pub fn hash_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_changes_when_contents_change() {
        let dir = std::env::temp_dir().join(format!("clangd-wrap-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("compile_commands.json");

        std::fs::write(&path, b"[]").unwrap();
        let first = hash_file(&path).unwrap();

        std::fs::write(&path, b"[{}]").unwrap();
        let second = hash_file(&path).unwrap();

        let _ = std::fs::remove_dir_all(&dir);
        assert_ne!(first, second);
    }

    #[test]
    fn ignore_empty_compile_commands() {
        let dir = std::env::temp_dir().join(format!("clangd-wrap-empty-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("compile_commands.json");
        std::fs::File::create(&path).unwrap();
        assert!(should_ignore_file(&path));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
