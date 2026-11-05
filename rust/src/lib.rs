//! Native implementation of the Caido plugin development toolkit.
//!
//! The executable owns filesystem access, packaging and watch orchestration.
//! JavaScript configuration and the Vite/tsup ecosystem are invoked through
//! the versioned Node adapter rather than through WebAssembly bindings.

mod adapter;
mod archive;
mod build;
mod cli;
mod filesystem;
mod package;
mod readme;
mod readme_image;
mod watch;

/// Run the native command line, returning the documented process exit code.
pub fn run(args: Vec<String>) -> i32 {
    use cli::{CliCommand, CliResponse, DisplayResponse};
    let response = match cli::parse(args) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let result = match response {
        CliResponse::Display(DisplayResponse::Help { help }) => {
            print!("{help}");
            return 0;
        }
        CliResponse::Display(DisplayResponse::Version) => {
            println!("{}", cli::version());
            return 0;
        }
        CliResponse::Command(CliCommand::Build(options)) => {
            let cwd = std::path::Path::new(options.path.as_deref().unwrap_or("."));
            build::build(cwd, options.config_path()).map(|_| ())
        }
        CliResponse::Command(CliCommand::Watch(options)) => watch::run(options),
    };
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}
