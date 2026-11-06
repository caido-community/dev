use std::{path::Path, time::Duration};

use crate::package::PackageConfig;
use tokio::{task::JoinHandle, time::Instant};

use crate::{adapter::Cancellation, build};

pub(super) const DEBOUNCE: Duration = Duration::from_millis(200);

#[derive(Default)]
pub(super) struct Scheduler {
    pub deadline: Option<Instant>,
    pub active: bool,
}

impl Scheduler {
    pub fn changed(&mut self, now: Instant) {
        self.deadline = Some(now + DEBOUNCE);
    }

    pub fn ready(&self, now: Instant) -> bool {
        !self.active && self.deadline.is_some_and(|deadline| deadline <= now)
    }

    pub fn start(&mut self) {
        self.active = true;
        self.deadline = None;
    }

    pub fn finished(&mut self) {
        self.active = false;
    }
}

pub(super) enum Cycle {
    Loaded(Result<super::config::WatchConfig, String>),
    Built(Result<Vec<u8>, String>),
}

pub(super) fn load(
    cwd: &Path,
    path: Option<&str>,
    cancellation: &Cancellation,
) -> Result<super::config::WatchConfig, String> {
    super::config::WatchConfig::resolve(cwd, build::load_cancellable(cwd, path, cancellation)?)
}

pub(super) fn build_cycle(
    cwd: &Path,
    config_path: Option<&str>,
    config: &PackageConfig,
    cancellation: &Cancellation,
) -> Cycle {
    Cycle::Built((|| {
        build::build_loaded_cancellable(cwd, config_path, config, cancellation)?;
        cancellation.check()?;
        std::fs::read(cwd.join("dist").join("plugin_package.zip"))
            .map_err(|error| format!("Cannot read built plugin package: {error}"))
    })())
}

pub(super) async fn cancel_and_join<T: Send + 'static>(
    cancellation: &Cancellation,
    active: Option<JoinHandle<T>>,
) -> Result<(), String> {
    cancellation.cancel();
    // spawn_blocking cannot be aborted once running. Await its child cleanup
    // and any assembly already in progress, rather than detaching the worker.
    if let Some(task) = active {
        task.await
            .map_err(|error| format!("Watch worker failed during shutdown: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
