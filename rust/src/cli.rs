use std::sync::OnceLock;

use clap::{Args, Parser, Subcommand};
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
    command: CliCommand,
}

#[derive(Subcommand)]
pub enum CliCommand {
    #[command(about = "Build the Caido plugin")]
    Build(PathOptions),
    #[command(about = "Start development server and watch for changes")]
    Watch(PathOptions),
}

#[derive(Args, Clone)]
pub struct PathOptions {
    #[arg(value_name = "path")]
    pub path: Option<String>,
    #[arg(
        short = 'c',
        long,
        value_name = "path",
        help = "Path to caido.config.ts file"
    )]
    pub config: Option<String>,
}

pub(crate) fn parse(args: Vec<String>) -> Result<CliCommand, clap::Error> {
    Cli::try_parse_from(std::iter::once("caido-dev".to_owned()).chain(args)).map(|cli| cli.command)
}

#[cfg(test)]
#[path = "cli/tests.rs"]
mod tests;
