use super::{
    package_files, package_plan, prepare, ExpandedAsset, PackageFile, PackageInput, PackageSources,
    PreparedPackage,
};
use crate::filesystem::{absolute, check_output, checked_source};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Assemble all bytes before touching the last successful output. Messages are
/// returned to the caller so CLI/watch logging remains outside package assembly.
pub(crate) fn assemble(
    cwd: &Path,
    config: Value,
    build_outputs: Vec<Value>,
    asset_matches: HashMap<String, Vec<String>>,
) -> Result<Vec<String>, String> {
    let original_root = absolute(cwd)?;
    let root = fs::canonicalize(&original_root)
        .map_err(|e| format!("Cannot resolve project root: {e}"))?;
    let plugin_roots = config
        .get("plugins")
        .and_then(Value::as_array)
        .ok_or("Expected config.plugins array")?
        .iter()
        .map(|plugin| {
            plugin
                .get("root")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .ok_or_else(|| "Expected plugin.root string".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    check_output(&root, &plugin_roots)?;
    let plan = package_plan(PackageInput {
        config,
        build_outputs,
        manifest: None,
    })?;
    let readme = crate::readme::bundle(&root)?;
    let manifest = serde_json::to_string_pretty(&plan.manifest).map_err(|e| e.to_string())?;
    let mut sources = PackageSources::new(plan.copies)?;
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
            sources.expand(&asset, expanded.clone())?;
        }
    }
    let mut contents = HashMap::new();
    let mut files = package_files(manifest, &readme);
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
    let package = prepare(files, readme)?;
    write_package(&root, &plugin_roots, &package)?;
    Ok(package.messages)
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
    package: &PreparedPackage,
) -> Result<(), String> {
    let stage = Stage::new(root)?;
    let staged_dist = stage.0.join("next");
    let package_dir = staged_dist.join("plugin_package");
    fs::create_dir_all(&package_dir).map_err(|e| e.to_string())?;
    for file in &package.files {
        if !super::safe_package_path(&file.path) {
            return Err(format!("Invalid package destination '{}'", file.path));
        }
        let path = package_dir.join(&file.path);
        fs::create_dir_all(path.parent().ok_or("Missing destination parent")?)
            .map_err(|e| e.to_string())?;
        fs::write(path, &file.bytes).map_err(|e| format!("Cannot stage package file: {e}"))?;
    }
    fs::write(staged_dist.join("plugin_package.zip"), &package.zip)
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
mod tests {
    use super::*;
    use crate::filesystem::tests::TempDir;
    use serde_json::json;

    fn fixture() -> TempDir {
        let root = TempDir::new();
        fs::write(root.0.join("README.md"), "# Plugin\n").unwrap();
        fs::create_dir(root.0.join("plugin")).unwrap();
        fs::write(root.0.join("plugin/index.js"), "export default 1;").unwrap();
        root
    }
    fn config() -> Value {
        json!({"id":"package", "name":"Package", "version":"1.0.0", "plugins":[{"id":"plugin","root":"plugin"}]})
    }
    fn output(assets: Value) -> Value {
        json!({"kind":"backend","id":"plugin","fileName":"plugin/index.js","assets":assets})
    }

    #[test]
    fn manifest_defaults_and_referenced_ids_match_packaging_contract() {
        let plan = |output: Value| {
            super::super::package_plan(super::super::PackageInput {
                config: config(),
                build_outputs: vec![output],
                manifest: None,
            })
        };
        for kind in ["frontend", "backend"] {
            for name in [None, Some(Value::Null), Some(json!(""))] {
                let mut output =
                    json!({"kind":kind,"id":"plugin","fileName":"plugin/index.js","assets":[]});
                if let Some(name) = &name {
                    output["name"] = name.clone();
                }
                let manifest = plan(output).unwrap().manifest;
                assert_eq!(
                    manifest["plugins"][0]["name"],
                    if name == Some(json!("")) {
                        ""
                    } else {
                        "plugin"
                    }
                );
                assert!(manifest["plugins"][0].get("style").is_none());
            }
        }
        let mut frontend = json!({"kind":"frontend","id":"plugin","fileName":"plugin/index.js","cssFileName":"","backendId":"","assets":[]});
        let manifest = plan(frontend.clone()).unwrap().manifest;
        assert!(manifest["plugins"][0].get("style").is_none());
        assert!(manifest["plugins"][0]["backend"].is_null());
        frontend["backendId"] = json!("../escape");
        assert!(plan(frontend).is_err());
        let backend = json!({"kind":"backend","id":"plugin","fileName":"plugin/index.js","backend":{"id":"../escape"},"assets":[]});
        assert!(plan(backend).is_err());
    }

    #[test]
    fn supplied_manifest_fields_order_and_referenced_id_validation_are_preserved() {
        let supplied = json!({"extension":{"nested":true},"plugins":[{"extra":"retained","id":"supplied","backendId":""}],"id":"package","name":"Supplied"});
        let plan = |config: Value, manifest: Value| {
            super::super::package_plan(super::super::PackageInput {
                config,
                build_outputs: vec![],
                manifest: Some(manifest),
            })
        };
        assert_eq!(
            serde_json::to_string(&plan(config(), supplied.clone()).unwrap().manifest).unwrap(),
            serde_json::to_string(&supplied).unwrap()
        );
        let mut invalid_config = config();
        invalid_config["plugins"][0]["backendId"] = json!("../escape");
        assert!(plan(invalid_config, supplied.clone()).is_err());
        for key in ["id", "backendId"] {
            let mut invalid = supplied.clone();
            invalid["plugins"][0][key] = json!("../escape");
            assert!(plan(config(), invalid).is_err());
        }
    }

    #[test]
    fn asset_destinations_reject_unsafe_case_and_file_directory_collisions() {
        for paths in [
            vec!["same.txt", "same.txt"],
            vec!["same.txt", "SAME.txt"],
            vec!["folder", "folder/file.txt"],
            vec!["folder/file.txt", "folder"],
            vec!["Folder", "folder/file.txt"],
            vec!["Folder/file.txt", "folder"],
            vec!["../escape.txt"],
            vec!["/absolute.txt"],
        ] {
            let mut sources = super::super::PackageSources::new(vec![]).unwrap();
            let mut rejected = false;
            for path in paths {
                if sources.reserve(path).is_err() {
                    rejected = true;
                    break;
                }
            }
            assert!(rejected);
        }
    }

    #[test]
    fn assembles_layout_and_replaces_previous_output() {
        let root = fixture();
        fs::create_dir(root.0.join("dist")).unwrap();
        fs::write(root.0.join("dist/sentinel"), b"old").unwrap();
        assemble(&root.0, config(), vec![output(json!([]))], HashMap::new()).unwrap();
        assert!(!root.0.join("dist/sentinel").exists());
        assert_eq!(
            fs::read(root.0.join("dist/plugin_package/plugin/index.js")).unwrap(),
            b"export default 1;"
        );
        assert!(root.0.join("dist/plugin_package.zip").is_file());
        let manifest: Value = serde_json::from_slice(
            &fs::read(root.0.join("dist/plugin_package/manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["plugins"][0]["entrypoint"], "plugin/index.js");
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 3);
    }

    #[test]
    fn preflight_failure_preserves_previous_output() {
        let root = fixture();
        fs::create_dir(root.0.join("dist")).unwrap();
        fs::write(root.0.join("dist/sentinel"), b"old").unwrap();
        let mut missing = output(json!([]));
        missing["fileName"] = json!("plugin/missing.js");
        assert!(assemble(&root.0, config(), vec![missing], HashMap::new()).is_err());
        assert_eq!(fs::read(root.0.join("dist/sentinel")).unwrap(), b"old");
        assert!(assemble(
            &root.0,
            config(),
            vec![output(json!(["{a,b}.txt"]))],
            HashMap::new(),
        )
        .unwrap_err()
        .contains("Missing asset match metadata"));
        assert_eq!(fs::read(root.0.join("dist/sentinel")).unwrap(), b"old");
    }

    #[test]
    fn directory_glob_semantics_and_destination_collisions() {
        let root = fixture();
        fs::create_dir_all(root.0.join("assets/nested")).unwrap();
        fs::write(root.0.join("assets/nested/a.txt"), b"asset").unwrap();
        fs::write(root.0.join("assets/.hidden"), b"hidden").unwrap();
        let assets = root.0.join("assets").to_str().unwrap().to_owned();
        let files = expand_assets(&root.0, std::slice::from_ref(&assets)).unwrap();
        assert_eq!(
            files
                .iter()
                .map(|file| file.relative.as_str())
                .collect::<Vec<_>>(),
            [".hidden", "nested/a.txt"]
        );
        let files = expand_assets(
            &root.0,
            &[
                root.0.join("assets/nested").to_str().unwrap().to_owned(),
                root.0
                    .join("assets/nested/a.txt")
                    .to_str()
                    .unwrap()
                    .to_owned(),
            ],
        )
        .unwrap();
        assert_eq!(files.len(), 1); // Directory traversal and direct match target the same copy.
        assert!(files.iter().any(|file| file.relative == "a.txt"));
        assert!(assemble(
            &root.0,
            config(),
            vec![output(json!(["assets", "assets"]))],
            HashMap::from([("assets".to_owned(), vec![assets])]),
        )
        .unwrap_err()
        .contains("Duplicate package destination"));
    }

    #[test]
    fn replacement_rechecks_output() {
        let root = fixture();
        fs::write(root.0.join("dist"), b"sentinel").unwrap();
        let package = PreparedPackage {
            files: vec![],
            zip: vec![],
            messages: vec![],
        };
        assert!(write_package(&root.0, &[], &package).is_err());
        assert_eq!(fs::read(root.0.join("dist")).unwrap(), b"sentinel");
    }

    #[test]
    fn node_patterns_use_supplied_matches_and_allow_explicit_empty_results() {
        let root = fixture();
        fs::write(root.0.join("a.txt"), b"asset").unwrap();
        let patterns = ["*.{txt,png}", "@(one|two)", "!(excluded)", "a\\*"];
        for pattern in patterns {
            assemble(
                &root.0,
                config(),
                vec![output(json!([pattern]))],
                HashMap::from([(
                    pattern.to_owned(),
                    vec![root.0.join("a.txt").to_str().unwrap().to_owned()],
                )]),
            )
            .unwrap();
            assert_eq!(
                fs::read(root.0.join("dist/plugin_package/plugin/assets/a.txt")).unwrap(),
                b"asset"
            );
        }
        assemble(
            &root.0,
            config(),
            vec![output(json!(["missing/*"]))],
            HashMap::from([("missing/*".to_owned(), vec![])]),
        )
        .unwrap();
    }

    #[test]
    fn unsafe_asset_matches_preserve_previous_output() {
        let root = fixture();
        let outside = fixture();
        fs::create_dir(root.0.join("dist")).unwrap();
        fs::write(root.0.join("dist/sentinel"), b"old").unwrap();
        let mut matches = vec![
            "plugin/index.js".to_owned(),
            outside
                .0
                .join("plugin/index.js")
                .to_str()
                .unwrap()
                .to_owned(),
            root.0.join("../outside.txt").to_str().unwrap().to_owned(),
        ];
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(root.0.join("plugin/index.js"), root.0.join("linked.js")).unwrap();
            symlink(&outside.0, root.0.join("linked-parent")).unwrap();
            matches.push(root.0.join("linked.js").to_str().unwrap().to_owned());
            matches.push(
                root.0
                    .join("linked-parent/plugin/index.js")
                    .to_str()
                    .unwrap()
                    .to_owned(),
            );
            fs::create_dir(root.0.join("assets")).unwrap();
            symlink(
                root.0.join("plugin/index.js"),
                root.0.join("assets/linked.js"),
            )
            .unwrap();
            matches.push(root.0.join("assets").to_str().unwrap().to_owned());
        }
        for source in matches {
            assert!(
                assemble(
                    &root.0,
                    config(),
                    vec![output(json!(["assets/*"]))],
                    HashMap::from([("assets/*".to_owned(), vec![source.clone()])]),
                )
                .is_err(),
                "{source}"
            );
            assert_eq!(fs::read(root.0.join("dist/sentinel")).unwrap(), b"old");
        }
    }
}
