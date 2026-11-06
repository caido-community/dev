use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

fn ignored(path: &Path) -> bool {
    path.components().any(
        |part| matches!(part, Component::Normal(name) if name == "dist" || name == "node_modules"),
    )
}

pub(super) fn relevant(path: &Path, files: &[PathBuf], roots: &[PathBuf]) -> bool {
    files
        .iter()
        .any(|file| file == path || file.starts_with(path))
        || roots.iter().any(|root| {
            root.starts_with(path)
                || path
                    .strip_prefix(root)
                    .is_ok_and(|relative| !ignored(relative))
        })
}

pub(super) fn make_watcher(
    files: &[PathBuf],
    roots: &[PathBuf],
    events: mpsc::Sender<()>,
) -> Result<RecommendedWatcher, String> {
    let watched_files = files.to_vec();
    let watched_roots = roots.to_vec();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        match event {
            Ok(event)
                if !matches!(event.kind, EventKind::Access(_))
                    && event
                        .paths
                        .iter()
                        .any(|path| relevant(path, &watched_files, &watched_roots)) =>
            {
                // A full channel already represents pending changes; memory stays bounded.
                let _ = events.try_send(());
            }
            Err(error) => {
                eprintln!("Filesystem watcher error: {error}");
                let _ = events.try_send(());
            }
            _ => {}
        }
    })
    .map_err(|error| format!("Cannot create filesystem watcher: {error}"))?;
    let mut registrations = BTreeSet::new();
    for file in files {
        let mut parent = file.parent().unwrap_or(file.as_path()).to_path_buf();
        while !parent.is_dir() {
            if !parent.pop() {
                return Err(format!("No existing parent for {}", file.display()));
            }
        }
        registrations.insert((parent, false));
    }
    for root in roots {
        let mut existing = root.clone();
        while !existing.is_dir() {
            if !existing.pop() {
                return Err(format!("No existing parent for {}", root.display()));
            }
        }
        if let Some(parent) = existing.parent() {
            registrations.insert((parent.to_path_buf(), false));
        }
        registrations.insert((existing, true));
    }
    // Prefer recursive registration when a path is also a metadata parent.
    for (path, recursive) in &registrations {
        if !recursive && registrations.contains(&(path.clone(), true)) {
            continue;
        }
        watcher
            .watch(
                path,
                if *recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                },
            )
            .map_err(|error| format!("Cannot watch {}: {error}", path.display()))?;
    }
    Ok(watcher)
}

#[cfg(test)]
#[path = "filesystem_tests.rs"]
mod tests;
