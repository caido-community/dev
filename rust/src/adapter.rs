//! One-shot executable config/build bridge. Packaging and watch stay in Rust.
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

/// Cancellation is cooperative except for the directly owned adapter process.
/// Assembly already holding the gate is drained before shutdown completes.
#[derive(Clone, Default)]
pub(crate) struct Cancellation {
    cancelled: Arc<AtomicBool>,
    assembly: Arc<Mutex<()>>,
}

impl Cancellation {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::SeqCst) {
            Err("Build cancelled during watch shutdown".into())
        } else {
            Ok(())
        }
    }

    pub(crate) fn assemble<T>(
        &self,
        work: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let _guard = self.assembly.lock().map_err(|_| "Assembly lock poisoned")?;
        self.check()?;
        work()
    }
}

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        // Child::drop does not terminate or reap. Never leave the direct child
        // running when an I/O error or cancellation exits supervision early.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Exchange(PathBuf);

impl Drop for Exchange {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn supervise(
    command: &mut Command,
    request: &Value,
    cancellation: &Cancellation,
) -> Result<Vec<u8>, String> {
    cancellation.check()?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let exchange = loop {
        let path = std::env::temp_dir().join(format!(
            "caido-dev-adapter-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut directory = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        match directory.create(&path) {
            Ok(()) => break Exchange(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Cannot create adapter exchange directory: {error}")),
        }
    };
    // File-backed stdio avoids blocking writes and reads while supervising the
    // child, and does not wait for descendants that inherited its stdout pipe.
    let mut input = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(exchange.0.join("request"))
        .map_err(|error| error.to_string())?;
    serde_json::to_writer(&mut input, request).map_err(|error| error.to_string())?;
    input.write_all(b"\n").map_err(|error| error.to_string())?;
    input
        .seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut output = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(exchange.0.join("response"))
        .map_err(|error| error.to_string())?;
    command.stdin(Stdio::from(input)).stdout(Stdio::from(
        output.try_clone().map_err(|error| error.to_string())?,
    ));
    let spawned = cancellation.check().and_then(|()| {
        command
            .spawn()
            .map_err(|error| format!("Failed to start Node adapter: {error}"))
    });
    // Command retains configured handles after spawn; release them before the
    // exchange directory is removed (required on Windows).
    command.stdin(Stdio::null()).stdout(Stdio::null());
    let mut child = OwnedChild(spawned?);
    let status = loop {
        cancellation.check()?;
        if let Some(status) = child
            .0
            .try_wait()
            .map_err(|error| format!("Failed to wait for Node adapter: {error}"))?
        {
            break status;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    cancellation.check()?;
    if !status.success() {
        return Err(format!(
            "Node adapter failed ({status}); see stderr for diagnostics"
        ));
    }
    output
        .seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    output
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    cancellation.check()?;
    Ok(bytes)
}

use crate::package::{BuildOutput, PackageConfig};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterResponse {
    pub version: u32,
    pub config: PackageConfig,
    #[serde(rename = "buildOutputs")]
    pub build_outputs: Vec<BuildOutput>,
    #[serde(rename = "assetMatches")]
    pub asset_matches: std::collections::HashMap<String, Vec<String>>,
}

fn parse_response(stdout: &[u8]) -> Result<AdapterResponse, String> {
    let response: AdapterResponse = serde_json::from_slice(stdout).map_err(|error| {
        format!("Invalid Node adapter response (stdout must contain only JSON): {error}")
    })?;
    if response.version != 1 {
        return Err(format!(
            "Unsupported Node adapter response version: {}",
            response.version
        ));
    }
    Ok(response)
}

pub(crate) fn invoke_cancellable(
    cwd: &Path,
    config: Option<&str>,
    build: bool,
    expected_config: Option<&PackageConfig>,
    cancellation: &Cancellation,
) -> Result<AdapterResponse, String> {
    let adapter = std::env::var_os("CAIDO_DEV_ADAPTER")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../dist/adapter.js"));
    let node = std::env::var_os("CAIDO_DEV_NODE").unwrap_or_else(|| "node".into());
    let cwd = std::path::absolute(cwd)
        .map_err(|error| format!("Failed to resolve project path: {error}"))?;
    let request = json!({
        "version": 1,
        "operation": if build { "build" } else { "load" },
        "cwd": cwd,
    });
    let mut request = request;
    if let Some(config) = config {
        request["config"] = json!(config);
    }
    if let Some(expected) = expected_config {
        request["expectedConfig"] = json!(expected);
    }
    // Inherit the caller's process cwd, preserving jiti and custom config lookup.
    // Inherit stderr so build diagnostics stream live without mixing with JSON.
    let mut command = Command::new(&node);
    command.arg(&adapter).stderr(Stdio::inherit());
    let output = supervise(&mut command, &request, cancellation)?;
    parse_response(&output)
}

#[cfg(test)]
#[path = "adapter/tests.rs"]
mod tests;
