use std::path::PathBuf;

use clap::Parser;

/// Transparent clangd wrapper — all trailing arguments are forwarded to clangd.
#[derive(Parser, Debug)]
#[command(
    name = "clangd-wrap",
    disable_help_flag = true,
    disable_version_flag = true,
    trailing_var_arg = true,
    allow_hyphen_values = true
)]
struct Args {
    /// Wrapper configuration file (YAML).
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    clangd_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedCli {
    pub config: Option<PathBuf>,
    pub clangd_args: Vec<String>,
}

/// Captures wrapper config path and remaining CLI arguments for pass-through to clangd.
pub fn parse_cli() -> ParsedCli {
    let args = Args::parse();
    ParsedCli {
        config: args.config,
        clangd_args: args.clangd_args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn clap_accepts_hyphen_values() {
        let args = vec![
            "clangd-wrap".to_string(),
            "--compile-commands-dir=build".to_string(),
            "-log=verbose".to_string(),
        ];
        let parsed = Args::try_parse_from(args).unwrap();
        assert_eq!(
            parsed.clangd_args,
            vec![
                "--compile-commands-dir=build".to_string(),
                "-log=verbose".to_string(),
            ]
        );
        assert!(parsed.config.is_none());
    }

    #[test]
    fn clap_extracts_config_and_forwards_remaining_args() {
        let args = vec![
            "clangd-wrap".to_string(),
            "--config".to_string(),
            "cfg.yaml".to_string(),
            "--background-index".to_string(),
        ];
        let parsed = Args::try_parse_from(args).unwrap();
        assert_eq!(parsed.config, Some(PathBuf::from("cfg.yaml")));
        assert_eq!(parsed.clangd_args, vec!["--background-index".to_string()]);
    }

    #[test]
    fn clap_accepts_config_equals_form() {
        let args = vec![
            "clangd-wrap".to_string(),
            "--config=cfg.yaml".to_string(),
            "--clang-tidy".to_string(),
        ];
        let parsed = Args::try_parse_from(args).unwrap();
        assert_eq!(parsed.config, Some(PathBuf::from("cfg.yaml")));
        assert_eq!(parsed.clangd_args, vec!["--clang-tidy".to_string()]);
    }
}
