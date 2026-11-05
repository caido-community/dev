//! Native orchestration around executable JavaScript configuration and builds.
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{adapter, filesystem, package};

pub(crate) fn preflight(cwd: &Path, config: &Value) -> Result<(), String> {
    let root = std::fs::canonicalize(cwd)
        .map_err(|error| format!("Cannot resolve project root '{}': {error}", cwd.display()))?;
    let plugins = config
        .get("plugins")
        .and_then(Value::as_array)
        .ok_or("Expected config.plugins array")?;
    let roots = plugins
        .iter()
        .map(|plugin| {
            plugin
                .get("root")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .ok_or_else(|| "Expected plugin.root string".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    filesystem::check_output(&root, &roots)
}

pub(crate) fn load_cancellable(
    cwd: &Path,
    config: Option<&str>,
    cancellation: &adapter::Cancellation,
) -> Result<Value, String> {
    let response = adapter::invoke_cancellable(cwd, config, false, None, cancellation)?;
    Ok(response.config)
}

pub(crate) fn build(cwd: &Path, config: Option<&str>) -> Result<Value, String> {
    build_cancellable(cwd, config, &adapter::Cancellation::default())
}

pub(crate) fn build_cancellable(
    cwd: &Path,
    config: Option<&str>,
    cancellation: &adapter::Cancellation,
) -> Result<Value, String> {
    let expected = load_cancellable(cwd, config, cancellation)?;
    build_loaded_cancellable(cwd, config, &expected, cancellation)
}

pub(crate) fn build_loaded_cancellable(
    cwd: &Path,
    config: Option<&str>,
    expected: &Value,
    cancellation: &adapter::Cancellation,
) -> Result<Value, String> {
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
