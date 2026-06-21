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
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    clangd_args: Vec<String>,
}

/// Captures all CLI arguments for transparent pass-through to clangd.
pub fn parse_user_args() -> Vec<String> {
    Args::parse().clangd_args
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    #[test]
    fn clap_accepts_hyphen_values() {
        let args = vec![
            "clangd-wrap".to_string(),
            "--compile-commands-dir=build".to_string(),
            "-log=verbose".to_string(),
        ];
        let parsed = super::Args::try_parse_from(args).unwrap();
        assert_eq!(
            parsed.clangd_args,
            vec![
                "--compile-commands-dir=build".to_string(),
                "-log=verbose".to_string(),
            ]
        );
    }
}
