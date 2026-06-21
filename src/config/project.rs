use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub const CONFIG_FILE_NAMES: &[&str] = &[
    "compile_commands.json",
    "compile_flags.txt",
    ".clangd",
];

/// Known project-relative paths to watch in addition to root-level files.
pub const NESTED_WATCH_PATHS: &[&str] = &["build/compile_commands.json"];

#[derive(Debug, Clone)]
pub struct ProjectPaths {
    pub root: PathBuf,
    pub compile_commands: PathBuf,
    pub build_compile_commands: PathBuf,
    pub compile_flags: PathBuf,
    pub clangd_path: PathBuf,
    pub compile_commands_dir: Option<PathBuf>,
    pub watch_paths: Vec<PathBuf>,
}

impl ProjectPaths {
    pub fn resolve(root: PathBuf, user_args: &[String]) -> Self {
        let compile_commands_dir = parse_compile_commands_dir(user_args);
        let compile_commands = compile_commands_dir
            .as_ref()
            .map(|dir| dir.join("compile_commands.json"))
            .unwrap_or_else(|| root.join("compile_commands.json"));

        let mut watch_paths = vec![
            root.join("compile_commands.json"),
            root.join("build/compile_commands.json"),
            root.join("compile_flags.txt"),
            root.join(".clangd"),
        ];

        if let Some(dir) = &compile_commands_dir {
            watch_paths.push(dir.join("compile_commands.json"));
        }

        watch_paths.sort();
        watch_paths.dedup();

        Self {
            root: root.clone(),
            compile_commands,
            build_compile_commands: root.join("build/compile_commands.json"),
            compile_flags: root.join("compile_flags.txt"),
            clangd_path: root.join(".clangd"),
            compile_commands_dir,
            watch_paths,
        }
    }
}

pub fn discover_project(watch_root: &Path, user_args: &[String]) -> Result<ProjectPaths> {
    let root = watch_root
        .canonicalize()
        .or_else(|_| Ok::<_, std::io::Error>(watch_root.to_path_buf()))
        .context("resolve project root")?;

    Ok(ProjectPaths::resolve(root, user_args))
}

fn parse_compile_commands_dir(user_args: &[String]) -> Option<PathBuf> {
    for arg in user_args {
        if let Some(dir) = arg.strip_prefix("--compile-commands-dir=") {
            return Some(PathBuf::from(dir));
        }
        if arg == "--compile-commands-dir" {
            continue;
        }
    }

    for (idx, arg) in user_args.iter().enumerate() {
        if arg == "--compile-commands-dir" {
            return user_args.get(idx + 1).map(PathBuf::from);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_compile_commands_dir_flag() {
        let args = vec![
            "--background-index".to_string(),
            "--compile-commands-dir=build/debug".to_string(),
        ];
        let dir = parse_compile_commands_dir(&args).unwrap();
        assert_eq!(dir, PathBuf::from("build/debug"));
    }

    #[test]
    fn parse_compile_commands_dir_separate() {
        let args = vec![
            "--compile-commands-dir".to_string(),
            "/tmp/build".to_string(),
        ];
        let dir = parse_compile_commands_dir(&args).unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/build"));
    }
}
