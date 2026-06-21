use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ClangdConfig {
    #[serde(default)]
    pub compile_flags: CompileFlagsSection,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CompileFlagsSection {
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
    pub compilation_database: Option<CompilationDatabaseValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CompilationDatabaseValue {
    Single(String),
    List(Vec<String>),
}

impl CompilationDatabaseValue {
    pub fn as_path(&self) -> Option<&str> {
        match self {
            Self::Single(value) => Some(value.as_str()),
            Self::List(values) => values.first().map(String::as_str),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ClangdFragment {
    #[serde(default)]
    compile_flags: CompileFlagsSection,
}

pub fn parse_clangd_file(path: &Path) -> Result<ClangdConfig> {
    if !path.exists() {
        return Ok(ClangdConfig::default());
    }

    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;

    merge_clangd_fragments(&contents)
}

fn merge_clangd_fragments(contents: &str) -> Result<ClangdConfig> {
    let mut merged = ClangdConfig::default();

    for fragment in contents.split("\n---\n") {
        let trimmed = fragment.trim();
        if trimmed.is_empty() {
            continue;
        }

        let fragment: ClangdFragment = serde_yaml::from_str(trimmed)
            .with_context(|| format!("parse .clangd fragment: {trimmed}"))?;

        if fragment.compile_flags.compilation_database.is_some() {
            merged.compile_flags.compilation_database =
                fragment.compile_flags.compilation_database.clone();
        }

        merged.compile_flags.add.extend(fragment.compile_flags.add);
        merged.compile_flags.remove.extend(fragment.compile_flags.remove);
    }

    merged.compile_flags.add = dedupe_preserve_order(merged.compile_flags.add);
    merged.compile_flags.remove = dedupe_preserve_order(merged.compile_flags.remove);

    Ok(merged)
}

pub fn injected_args_from_clangd(clangd_path: &Path) -> Result<Vec<String>> {
    let config = parse_clangd_file(clangd_path)?;
    Ok(build_injected_args(&config, clangd_path.parent()))
}

pub fn build_injected_args(config: &ClangdConfig, project_root: Option<&Path>) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(db) = &config.compile_flags.compilation_database {
        if let Some(path) = db.as_path() {
            if path != "Ancestors" && path != "None" {
                let resolved = resolve_config_path(path, project_root);
                args.push(format!("--compile-commands-dir={}", resolved.display()));
            }
        }
    }

    for flag in &config.compile_flags.add {
        if !args.iter().any(|existing| existing == flag) {
            args.push(flag.clone());
        }
    }

    args
}

fn resolve_config_path(path: &str, project_root: Option<&Path>) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        return candidate;
    }

    project_root
        .map(|root| root.join(&candidate))
        .unwrap_or(candidate)
}

fn dedupe_preserve_order(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_compile_flags_and_database() {
        let yaml = r#"
CompileFlags:
  CompilationDatabase: build
  Add: [-Wall, -Wextra]
  Remove: [-Werror]
"#;
        let config = merge_clangd_fragments(yaml).unwrap();
        assert_eq!(
            config.compile_flags.compilation_database.as_ref().unwrap().as_path(),
            Some("build")
        );
        assert_eq!(config.compile_flags.add, vec!["-Wall", "-Wextra"]);
        assert_eq!(config.compile_flags.remove, vec!["-Werror"]);
    }

    #[test]
    fn build_injected_args_resolves_relative_database() {
        let config = ClangdConfig {
            compile_flags: CompileFlagsSection {
                add: vec!["-std=c++20".to_string()],
                remove: vec![],
                compilation_database: Some(CompilationDatabaseValue::Single("build".to_string())),
            },
        };

        let args = build_injected_args(&config, Some(Path::new("/proj")));
        assert!(args.iter().any(|arg| {
            arg.starts_with("--compile-commands-dir=") && arg.ends_with("build")
        }));
        assert!(args.contains(&"-std=c++20".to_string()));
    }
}
