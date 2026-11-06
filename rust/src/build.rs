//! Native orchestration around executable JavaScript configuration and builds.
use std::path::{Path, PathBuf};

use crate::{adapter, filesystem, package};
use package::PackageConfig;

pub(crate) fn preflight(cwd: &Path, config: &PackageConfig) -> Result<(), String> {
    let root = std::fs::canonicalize(cwd)
        .map_err(|error| format!("Cannot resolve project root '{}': {error}", cwd.display()))?;
    let roots: Vec<_> = config
        .plugins
        .iter()
        .map(|plugin| PathBuf::from(&plugin.root))
        .collect();
    filesystem::check_output(&root, &roots)
}

pub(crate) fn load_cancellable(
    cwd: &Path,
    config: Option<&str>,
    cancellation: &adapter::Cancellation,
) -> Result<PackageConfig, String> {
    let response = adapter::invoke_cancellable(cwd, config, false, None, cancellation)?;
    Ok(response.config)
}

pub(crate) fn build(cwd: &Path, config: Option<&str>) -> Result<PackageConfig, String> {
    let cancellation = adapter::Cancellation::default();
    let expected = load_cancellable(cwd, config, &cancellation)?;
    build_loaded_cancellable(cwd, config, &expected, &cancellation)
}

pub(crate) fn build_loaded_cancellable(
    cwd: &Path,
    config: Option<&str>,
    expected: &PackageConfig,
    cancellation: &adapter::Cancellation,
) -> Result<PackageConfig, String> {
    eprintln!("Building plugin package");
    // Check output policy before invoking Vite/tsup, not just before assembly.
    preflight(cwd, expected)?;
    let response = adapter::invoke_cancellable(cwd, config, true, Some(expected), cancellation)?;
    preflight(cwd, &response.config)?;
    for message in cancellation.assemble(|| {
        package::assemble(
            cwd,
            response.config.clone(),
            response.build_outputs,
            response.asset_matches,
        )
    })? {
        eprintln!("{message}");
    }
    eprintln!("Plugin package built successfully");
    Ok(response.config)
}
