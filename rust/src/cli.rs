use std::sync::OnceLock;

use clap::{error::ErrorKind, Args, Parser, Subcommand};
use serde::Serialize;
use serde_json::{json, Value};

fn version() -> &'static str {
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
enum CliCommand {
    #[command(about = "Build the Caido plugin")]
    Build(PathOptions),
    #[command(about = "Start development server and watch for changes")]
    Watch(PathOptions),
}

#[derive(Args, Serialize)]
struct PathOptions {
    #[arg(value_name = "path")]
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[command(flatten)]
    #[serde(flatten)]
    config: ConfigOptions,
}

#[derive(Args, Serialize)]
struct ConfigOptions {
    #[arg(
        short = 'c',
        long,
        value_name = "path",
        help = "Path to caido.config.ts file"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<String>,
}

pub(crate) fn parse(input: &str) -> Result<String, String> {
    let args: Vec<String> =
        serde_json::from_str(input).map_err(|e| format!("Invalid CLI arguments: {e}"))?;
    let cli = match Cli::try_parse_from(std::iter::once("caido-dev".to_owned()).chain(args)) {
        Ok(cli) => cli,
        Err(error) => {
            return match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    Ok(json!({"command":"help","help":error.to_string()}).to_string())
                }
                ErrorKind::DisplayVersion => Ok(json!({"command":"version"}).to_string()),
                _ => Err(error.to_string()),
            };
        }
    };
    let command = cli.command.ok_or("Expected a command")?;
    serde_json::to_string(&command).map_err(|error| error.to_string())
}
