use clap::{error::ErrorKind, Arg, Command};
use serde_json::{json, Value};

fn command() -> Command {
    let mut command = Command::new("caido-dev")
        .about("Development tools for building Caido plugins")
        .version("1.0.0")
        .propagate_version(true)
        .arg_required_else_help(true);
    for (name, description) in [
        ("build", "Build the Caido plugin"),
        ("watch", "Start development server and watch for changes"),
        ("dev", "Start development server"),
    ] {
        let mut subcommand = Command::new(name).about(description).arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("path")
                .help("Path to caido.config.ts file"),
        );
        if name != "dev" {
            subcommand = subcommand.arg(Arg::new("path").value_name("path"));
        }
        command = command.subcommand(subcommand);
    }
    command
}

pub(crate) fn parse(input: &str) -> Result<String, String> {
    let args: Vec<String> =
        serde_json::from_str(input).map_err(|e| format!("Invalid CLI arguments: {e}"))?;
    let matches = match command()
        .try_get_matches_from(std::iter::once("caido-dev".to_owned()).chain(args))
    {
        Ok(matches) => matches,
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
    let (name, options) = matches.subcommand().ok_or("Expected a command")?;
    let mut result = json!({"command":name});
    if name != "dev" {
        if let Some(path) = options.get_one::<String>("path") {
            result["path"] = Value::from(path.clone());
        }
    }
    if let Some(config) = options.get_one::<String>("config") {
        result["config"] = Value::from(config.clone());
    }
    Ok(result.to_string())
}
