use super::{package_plan, BuildOutput, Copy, ExpandedAsset, PackageConfig, PackageFile};
use crate::filesystem::{absolute, check_output, checked_source};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Assemble all bytes before touching the last successful output. Messages are
/// returned to the caller so CLI/watch logging remains outside package assembly.
pub(crate) fn assemble(
    cwd: &Path,
    config: PackageConfig,
    build_outputs: Vec<BuildOutput>,
    asset_matches: HashMap<String, Vec<String>>,
) -> Result<Vec<String>, String> {
    let original_root = absolute(cwd)?;
    let root = fs::canonicalize(&original_root)
        .map_err(|e| format!("Cannot resolve project root: {e}"))?;
    let plugin_roots: Vec<_> = config
        .plugins
        .iter()
        .map(|plugin| PathBuf::from(&plugin.root))
        .collect();
    check_output(&root, &plugin_roots)?;
    let plan = package_plan(config, build_outputs)?;
    let readme = crate::readme::bundle(&root)?;
    let manifest = serde_json::to_string_pretty(&plan.manifest).map_err(|e| e.to_string())?;
    let mut sources = plan.sources;
    let mut expansions = HashMap::new();
    for asset in plan.assets {
        if !expansions.contains_key(&asset.pattern) {
            let matches = asset_matches.get(&asset.pattern).ok_or_else(|| {
                format!(
                    "Missing asset match metadata for pattern '{}'",
                    asset.pattern
                )
            })?;
            expansions.insert(
                asset.pattern.clone(),
                expand_assets(&original_root, matches)?,
            );
        }
        for expanded in &expansions[&asset.pattern] {
            sources.add(Copy {
                destination: format!("{}/{}", asset.destination, expanded.relative),
                source: expanded.source.clone(),
            })?;
        }
    }
    let mut contents = HashMap::new();
    let mut files = vec![
        PackageFile {
            path: "README.md".into(),
            bytes: readme.content.into_bytes(),
        },
        PackageFile {
            path: "manifest.json".into(),
            bytes: manifest.into_bytes(),
        },
    ];
    for copy in sources.copies {
        // Recheck each source even when its bytes have already been preloaded.
        let source = checked_source(&original_root, Path::new(&copy.source), false)?;
        if !contents.contains_key(&source) {
            if !fs::metadata(&source).map_err(|e| e.to_string())?.is_file() {
                return Err(format!("Unsupported package source: {}", source.display()));
            }
            let bytes = fs::read(&source)
                .map_err(|e| format!("Cannot read package source '{}': {e}", source.display()))?;
            contents.insert(source.clone(), bytes);
        }
        files.push(PackageFile {
            path: copy.destination,
            bytes: contents[&source].clone(),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let zip = crate::archive::create_files(&files)?;
    write_package(&root, &plugin_roots, &files, &zip)?;
    let mut messages = readme.messages;
    messages.extend(readme.warnings);
    Ok(messages)
}

fn expand_assets(root: &Path, matches: &[String]) -> Result<Vec<ExpandedAsset>, String> {
    let mut files = Vec::new();
    let mut collected = HashSet::new();
    for source in matches {
        let source = Path::new(source);
        if !source.is_absolute() {
            return Err(format!(
                "Asset match path must be absolute: {}",
                source.display()
            ));
        }
        let checked = checked_source(root, source, true)?;
        let metadata = fs::metadata(&checked).map_err(|e| e.to_string())?;
        let relative = if metadata.is_file() {
            source
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("Asset filename is not valid UTF-8")?
                .to_owned()
        } else {
            String::new()
        };
        collect(root, &source, &relative, &mut collected, &mut files)?;
    }
    Ok(files)
}

fn collect(
    root: &Path,
    source: &Path,
    relative: &str,
    collected: &mut HashSet<(PathBuf, String)>,
    files: &mut Vec<ExpandedAsset>,
) -> Result<(), String> {
    let file = checked_source(root, source, true)?;
    let metadata = fs::metadata(&file).map_err(|e| e.to_string())?;
    if metadata.is_dir() {
        let mut children = fs::read_dir(&file)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        children.sort_by_key(|child| child.file_name());
        for child in children {
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| "Asset filename is not valid UTF-8")?;
            let relative = if relative.is_empty() {
                name
            } else {
                format!("{relative}/{name}")
            };
            collect(root, &child.path(), &relative, collected, files)?;
        }
    } else if metadata.is_file() {
        if collected.insert((file.clone(), relative.to_owned())) {
            files.push(ExpandedAsset {
                source: file
                    .to_str()
                    .ok_or("Asset path is not valid UTF-8")?
                    .to_owned(),
                relative: relative.to_owned(),
            });
        }
    } else {
        return Err(format!("Unsupported package source: {}", source.display()));
    }
    Ok(())
}

static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

struct Stage(PathBuf);
impl Stage {
    fn new(root: &Path) -> Result<Self, String> {
        for _ in 0..100 {
            let path = root.join(format!(
                ".caido-package-{}-{}",
                std::process::id(),
                NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Cannot create package staging directory: {e}")),
            }
        }
        Err("Cannot allocate package staging directory".into())
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        // Only clean our private directory, never a path supplied by config.
        // Do not follow a replacement symlink or a changed project-root path.
        if let Ok(real) = fs::canonicalize(&self.0) {
            if real == self.0 {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
    }
}

fn write_package(
    root: &Path,
    plugin_roots: &[PathBuf],
    files: &[PackageFile],
    zip: &[u8],
) -> Result<(), String> {
    let stage = Stage::new(root)?;
    let staged_dist = stage.0.join("next");
    let package_dir = staged_dist.join("plugin_package");
    fs::create_dir_all(&package_dir).map_err(|e| e.to_string())?;
    for file in files {
        if !super::safe_package_path(&file.path) {
            return Err(format!("Invalid package destination '{}'", file.path));
        }
        let path = package_dir.join(&file.path);
        fs::create_dir_all(path.parent().ok_or("Missing destination parent")?)
            .map_err(|e| e.to_string())?;
        fs::write(path, &file.bytes).map_err(|e| format!("Cannot stage package file: {e}"))?;
    }
    fs::write(staged_dist.join("plugin_package.zip"), zip)
        .map_err(|e| format!("Cannot stage package ZIP: {e}"))?;
    check_output(root, plugin_roots)?;
    let dist = root.join("dist");
    let backup = stage.0.join("previous");
    let had_previous = match fs::rename(&dist, &backup) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(format!("Cannot preserve previous package output: {e}")),
    };
    if let Err(error) = fs::rename(&staged_dist, &dist) {
        if had_previous {
            if let Err(rollback) = fs::rename(&backup, &dist) {
                // Keep the backup available for recovery rather than dropping it.
                let location = backup.display().to_string();
                std::mem::forget(stage);
                return Err(format!("Cannot replace package output: {error}; rollback failed: {rollback}; previous output retained at {location}"));
            }
        }
        return Err(format!("Cannot replace package output: {error}"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "native/tests.rs"]
mod tests;
