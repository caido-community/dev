use super::*;
use serde_json::json;
use std::path::Path;

#[tokio::test]
async fn runtime_watches_new_roots_before_build_and_recovers_invalid_and_missing_roots() {
    use crate::filesystem::tests::TempDir;
    let project = TempDir::new();
    let adapter = project.0.join("adapter.cjs");
    std::fs::write(&adapter, r#"
const fs = require('node:fs');
const path = require('node:path');
const request = JSON.parse(fs.readFileSync(0, 'utf8'));
const root = request.cwd;
fs.appendFileSync(path.join(root, 'requests'), request.operation + '\n');
const config = JSON.parse(fs.readFileSync(request.config, 'utf8'));
if (request.operation === 'build') {
  fs.appendFileSync(path.join(root, 'builds'), config.plugins[0].root + '\n');
  if (fs.existsSync(path.join(root, 'gate'))) {
    fs.writeFileSync(path.join(root, 'started'), 'yes');
    const wait = new Int32Array(new SharedArrayBuffer(4));
    const deadline = Date.now() + 8000;
    while (fs.existsSync(path.join(root, 'gate')) && Date.now() < deadline) Atomics.wait(wait, 0, 0, 10);
  }
}
console.log(JSON.stringify({version:1,config,buildOutputs:[],assetMatches:{}}));
"#).unwrap();
    struct Restore(Option<std::ffi::OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            match &self.0 {
                Some(value) => std::env::set_var("CAIDO_DEV_ADAPTER", value),
                None => std::env::remove_var("CAIDO_DEV_ADAPTER"),
            }
        }
    }
    let _restore = Restore(std::env::var_os("CAIDO_DEV_ADAPTER"));
    std::env::set_var("CAIDO_DEV_ADAPTER", &adapter);
    let config_file = project.0.join("caido.config.ts");
    let metadata = |root: &str| json!({"id":"test", "name":"Test", "version":"1.0.0", "author":{"name":"Test"}, "plugins":[{"kind":"workflow", "id":"flow", "name":"Flow", "root":root, "definition":"flow"}], "watch":{"port":0}});
    std::fs::create_dir(project.0.join("a")).unwrap();
    std::fs::create_dir(project.0.join("b")).unwrap();
    std::fs::write(&config_file, metadata("a").to_string()).unwrap();
    let response = cli::parse(vec!["watch".into(), project.0.to_str().unwrap().into()]).unwrap();
    let cli::CliResponse::Command(cli::CliCommand::Watch(options)) = response else {
        panic!("watch options")
    };
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let runtime = run_with_shutdown(options, async {
        let _ = stop_rx.await;
        Ok(())
    });
    let exercise = async {
        async fn count(path: &Path, minimum: usize) {
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if std::fs::read_to_string(path)
                        .unwrap_or_default()
                        .lines()
                        .count()
                        >= minimum
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .unwrap();
        }
        let builds = project.0.join("builds");
        count(&builds, 1).await;
        std::fs::write(project.0.join("gate"), "gate").unwrap();
        std::fs::write(&config_file, metadata("b").to_string()).unwrap();
        count(&builds, 2).await;
        std::fs::write(project.0.join("b/source.ts"), "edit during build").unwrap();
        std::fs::remove_file(project.0.join("gate")).unwrap();
        count(&builds, 3).await;
        std::fs::write(&config_file, "invalid JSON").unwrap();
        let requests = project.0.join("requests");
        let before = std::fs::read_to_string(&requests).unwrap().lines().count();
        count(&requests, before + 1).await;
        // Invalid reloads retain the previously valid root registration.
        let before = std::fs::read_to_string(&requests).unwrap().lines().count();
        std::fs::write(project.0.join("b/source.ts"), "old root still watched").unwrap();
        count(&requests, before + 1).await;
        let before = std::fs::read_to_string(&requests).unwrap().lines().count();
        std::fs::write(&config_file, metadata("missing/deep").to_string()).unwrap();
        count(&requests, before + 1).await;
        assert_eq!(std::fs::read_to_string(&builds).unwrap().lines().count(), 3);
        std::fs::create_dir_all(project.0.join("missing/deep")).unwrap();
        count(&builds, 4).await;
        let _ = stop_tx.send(());
    };
    let (result, ()) = tokio::join!(runtime, exercise);
    result.unwrap();
}
