use std::sync::OnceLock;

use clap::{error::ErrorKind, Args, Parser, Subcommand};
use serde::Serialize;
use serde_json::Value;

pub fn version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let package: Value = serde_json::from_str(include_str!("../../package.json"))
            .expect("package.json must contain valid JSON");
        package["version"]
            .as_str()
            .expect("package.json must contain a version")
            .to_owned()
    })
}

#[derive(Parser)]
#[command(
    name = "caido-dev",
    about = "Development tools for building Caido plugins",
    version = version(),
    propagate_version = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand, Serialize)]
#[serde(tag = "command", rename_all = "lowercase")]
pub enum CliCommand {
    #[command(about = "Build the Caido plugin")]
    Build(PathOptions),
    #[command(about = "Start development server and watch for changes")]
    Watch(PathOptions),
}

#[derive(Args, Serialize, Clone)]
pub struct PathOptions {
    #[arg(value_name = "path")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[command(flatten)]
    #[serde(flatten)]
    config: ConfigOptions,
}

impl PathOptions {
    pub(crate) fn config_path(&self) -> Option<&str> {
        self.config.config.as_deref()
    }
}

#[derive(Args, Serialize, Clone)]
pub struct ConfigOptions {
    #[arg(
        short = 'c',
        long,
        value_name = "path",
        help = "Path to caido.config.ts file"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum CliResponse {
    Command(CliCommand),
    Display(DisplayResponse),
}

#[derive(Serialize)]
#[serde(tag = "command", rename_all = "lowercase")]
pub(crate) enum DisplayResponse {
    Help { help: String },
    Version,
}

pub(crate) fn parse(args: Vec<String>) -> Result<CliResponse, String> {
    let cli = match Cli::try_parse_from(std::iter::once("caido-dev".to_owned()).chain(args)) {
        Ok(cli) => cli,
        Err(error) => {
            return match error.kind() {
                ErrorKind::DisplayHelp => Ok(CliResponse::Display(DisplayResponse::Help {
                    help: error.to_string(),
                })),
                ErrorKind::DisplayVersion => Ok(CliResponse::Display(DisplayResponse::Version)),
                _ => Err(error.to_string()),
            };
        }
    };
    let command = cli.command.ok_or("Expected a command")?;
    Ok(CliResponse::Command(command))
}

#[cfg(test)]
mod tests {
    use super::{parse, CliResponse, DisplayResponse};

    #[test]
    fn displays_help_and_version() {
        assert!(matches!(
            parse(vec!["--help".into()]),
            Ok(CliResponse::Display(DisplayResponse::Help { .. }))
        ));
        assert!(matches!(
            parse(vec!["--version".into()]),
            Ok(CliResponse::Display(DisplayResponse::Version))
        ));
    }

    #[test]
    fn parses_build_and_watch_options() {
        assert!(matches!(
            parse(vec!["build".into(), "-c".into(), "custom.ts".into()]),
            Ok(CliResponse::Command(_))
        ));
        assert!(matches!(
            parse(vec!["watch".into()]),
            Ok(CliResponse::Command(_))
        ));
    }

    #[test]
    fn rejects_unknown_subcommands() {
        assert!(parse(vec!["unknown".into()]).is_err());
        assert!(parse(vec![]).is_err());
        assert!(parse(vec!["build".into(), "--config".into()]).is_err());
    }
}
