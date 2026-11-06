use super::*;
use crate::filesystem::tests::TempDir;
use serde_json::{json, Value};

fn fixture() -> TempDir {
    let root = TempDir::new();
    fs::write(root.0.join("README.md"), "# Plugin\n").unwrap();
    fs::create_dir(root.0.join("plugin")).unwrap();
    fs::write(root.0.join("plugin/index.js"), "export default 1;").unwrap();
    root
}
fn config() -> PackageConfig {
    serde_json::from_value(json!({"id":"package", "name":"Package", "version":"1.0.0", "plugins":[{"id":"plugin","root":"plugin"}]})).unwrap()
}
fn output(assets: Value) -> BuildOutput {
    serde_json::from_value(
        json!({"kind":"backend","id":"plugin","fileName":"plugin/index.js","assets":assets}),
    )
    .unwrap()
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
        let mut sources = super::super::PackageSources::new().unwrap();
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
fn successful_replacement_removes_previous_output_and_staging_directory() {
    let root = fixture();
    fs::create_dir(root.0.join("dist")).unwrap();
    fs::write(root.0.join("dist/sentinel"), b"old").unwrap();
    assemble(&root.0, config(), vec![output(json!([]))], HashMap::new()).unwrap();
    assert!(!root.0.join("dist/sentinel").exists());
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 3);
}

#[test]
fn missing_asset_metadata_preserves_previous_output() {
    let root = fixture();
    fs::create_dir(root.0.join("dist")).unwrap();
    fs::write(root.0.join("dist/sentinel"), b"old").unwrap();
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
    assert!(write_package(&root.0, &[], &[], &[]).is_err());
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
