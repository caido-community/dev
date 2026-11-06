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
    use clap::error::ErrorKind;
    use cli::CliCommand;
    let command = match cli::parse(args) {
        Ok(command) => command,
        Err(error) => {
            match error.kind() {
                ErrorKind::DisplayHelp => print!("{error}"),
                ErrorKind::DisplayVersion => println!("{}", cli::version()),
                _ => {
                    eprintln!("{error}");
                    return 2;
                }
            }
            return 0;
        }
    };
    let result = match command {
        CliCommand::Build(options) => {
            let cwd = std::path::Path::new(options.path.as_deref().unwrap_or("."));
            build::build(cwd, options.config.as_deref()).map(|_| ())
        }
        CliCommand::Watch(options) => watch::run(options),
    };
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}
