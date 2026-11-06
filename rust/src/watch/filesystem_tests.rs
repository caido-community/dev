use super::*;
use crate::watch::{
    config::{config_path, watch_files, WatchConfig},
    worker::{Scheduler, DEBOUNCE},
};
use serde_json::json;
use std::time::Duration;
use tokio::time::Instant;

fn roots(cwd: &Path, root: &str) -> Vec<PathBuf> {
    let metadata =
        serde_json::from_value(json!({"id":"test", "plugins":[{"id":"plugin", "root":root}]}))
            .unwrap();
    WatchConfig::resolve(cwd, metadata).unwrap().roots
}

#[test]
fn roots_and_metadata_filter_events_without_output_loops() {
    let cwd = std::env::current_dir().unwrap();
    let files = watch_files(&cwd, Some("config/custom.ts"));
    let roots = roots(&cwd, "frontend");
    assert!(relevant(&cwd.join("config/custom.ts"), &files, &roots));
    assert!(relevant(&cwd.join("config"), &files, &roots));
    assert!(relevant(&cwd.join("package.json"), &files, &roots));
    assert!(relevant(&cwd.join("README.md"), &files, &roots));
    assert!(relevant(&cwd.join("frontend/new/file.ts"), &files, &roots));
    assert!(!relevant(
        &cwd.join("frontend/dist/output.js"),
        &files,
        &roots
    ));
    assert!(!relevant(
        &cwd.join("frontend/node_modules/dependency.js"),
        &files,
        &roots
    ));
    assert!(!relevant(&cwd.join("unrelated.ts"), &files, &roots));
}

#[test]
fn paths_are_caller_relative_and_ignores_are_root_relative() {
    let caller = std::env::current_dir().unwrap();
    let project = caller.join("dist/node_modules/project");
    let custom = config_path(&project, Some("./custom.ts")).unwrap();
    assert_eq!(Path::new(&custom), caller.join("custom.ts"));
    let files = watch_files(&project, Some(&custom));
    let roots = roots(&project, "./frontend/../frontend");
    assert!(relevant(Path::new(&custom), &files, &roots));
    assert!(relevant(&project.join("README.md"), &files, &roots));
    assert!(relevant(&project.join("frontend/index.ts"), &files, &roots));
    assert!(!relevant(
        &project.join("frontend/dist/output.js"),
        &files,
        &roots
    ));
}

#[tokio::test]
async fn real_notifications_recover_nested_config_and_missing_root() {
    use crate::filesystem::tests::TempDir;
    let project = TempDir::new();
    let file = project.0.join("new/deep/custom.ts");
    let files = vec![file.clone()];
    let root = project.0.join("plugins/missing");
    let roots = vec![root.clone()];
    let (tx, mut rx) = mpsc::channel(1);
    let mut watcher = make_watcher(&files, &roots, tx.clone()).unwrap();
    std::fs::create_dir(project.0.join("new")).unwrap();
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    // Failed reloads must refresh registration, even with no valid config.
    let next_watcher = make_watcher(&files, &roots, tx.clone()).unwrap();
    drop(watcher);
    watcher = next_watcher;
    while rx.try_recv().is_ok() {}
    std::fs::create_dir(project.0.join("new/deep")).unwrap();
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let next_watcher = make_watcher(&files, &roots, tx.clone()).unwrap();
    drop(watcher);
    watcher = next_watcher;
    while rx.try_recv().is_ok() {}
    std::fs::write(&file, "config").unwrap();
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    std::fs::create_dir_all(&root).unwrap();
    let next_watcher = make_watcher(&files, &roots, tx).unwrap();
    drop(watcher);
    watcher = next_watcher;
    while rx.try_recv().is_ok() {}
    // Candidate roots registered before builds retain edits during that work.
    let mut scheduler = Scheduler::default();
    scheduler.start();
    std::fs::write(root.join("index.ts"), "changed during build").unwrap();
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let now = Instant::now();
    scheduler.changed(now);
    scheduler.finished();
    assert!(scheduler.ready(now + DEBOUNCE));
    drop(watcher);
}
