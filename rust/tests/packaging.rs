use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "caido-packaging-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("plugin")).unwrap();
        fs::write(root.join("plugin/index.js"), "export default 1;").unwrap();
        fs::write(root.join("README.md"), "# Package\n\n![Image](image.png)\n").unwrap();
        fs::write(
            root.join("image.png"),
            include_bytes!("../../src/bundle/__fixtures__/images/red16x16.png"),
        )
        .unwrap();
        fs::write(root.join("adapter.cjs"), r#"
            const fs = require('node:fs');
            let input = '';
            process.stdin.on('data', chunk => input += chunk);
            process.stdin.on('end', () => {
                const request = JSON.parse(input);
                const fixture = JSON.parse(fs.readFileSync(require('node:path').join(request.cwd, 'metadata.json'), 'utf8'));
                if (request.operation !== 'load' && request.operation !== 'build') process.exit(8);
                console.log(JSON.stringify({version: 1, config: fixture.config,
                    buildOutputs: request.operation === 'build' ? fixture.buildOutputs : [],
                    assetMatches: Object.fromEntries(fixture.buildOutputs.flatMap(output => output.assets).map(pattern => [pattern, [require('node:path').join(request.cwd, pattern)]]))}));
            });
        "#).unwrap();
        let result = Self(root);
        result.metadata(json!({"id":"package","name":"Package","version":"1.0.0","plugins":[{"id":"plugin","root":"plugin"}]}), vec![json!({"id":"plugin","kind":"backend","fileName":"plugin/index.js","assets":[]})]);
        result
    }
    fn metadata(&self, config: Value, outputs: Vec<Value>) {
        fs::write(
            self.0.join("metadata.json"),
            serde_json::to_vec(&json!({"config":config,"buildOutputs":outputs})).unwrap(),
        )
        .unwrap();
    }
    fn build(&self) -> Output {
        self.build_at(&self.0)
    }
    fn build_at(&self, root: &std::path::Path) -> Output {
        Command::new(env!("CARGO_BIN_EXE_caido-dev"))
            .arg("build")
            .arg(root)
            .env("CAIDO_DEV_ADAPTER", self.0.join("adapter.cjs"))
            .env("CAIDO_DEV_NODE", "node")
            .output()
            .unwrap()
    }
    fn assert_success(&self) {
        let output = self.build();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn shared_sources_are_packaged_for_each_plugin_destination() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.0.join("assets")).unwrap();
    fs::write(fixture.0.join("assets/shared.txt"), b"shared asset").unwrap();
    fixture.metadata(
        json!({"id":"package","name":"Package","version":"1.0.0","plugins":[{"id":"first","root":"plugin"},{"id":"second","root":"plugin"}]}),
        ["first", "second"].into_iter().map(|id| json!({"id":id,"kind":"backend","fileName":"plugin/index.js","assets":["assets"]})).collect(),
    );
    fixture.assert_success();
    for id in ["first", "second"] {
        let root = fixture.0.join("dist/plugin_package").join(id);
        assert_eq!(
            fs::read(root.join("index.js")).unwrap(),
            b"export default 1;"
        );
        assert_eq!(
            fs::read(root.join("assets/shared.txt")).unwrap(),
            b"shared asset"
        );
    }
}

#[test]
fn native_cli_preflight_failures_preserve_last_package_and_return_runtime_status() {
    let fixture = Fixture::new();
    fixture.assert_success();
    let previous = fs::read(fixture.0.join("dist/plugin_package.zip")).unwrap();
    fs::write(fixture.0.join("dist/sentinel"), b"keep").unwrap();
    for source in [
        None,
        Some("![missing](missing.png)"),
        Some("![unsupported](image.svg)"),
    ] {
        match source {
            None => fs::remove_file(fixture.0.join("README.md")).unwrap(),
            Some(source) => fs::write(fixture.0.join("README.md"), source).unwrap(),
        }
        let output = fixture.build();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(fixture.0.join("dist/sentinel")).unwrap(), b"keep");
        assert_eq!(
            fs::read(fixture.0.join("dist/plugin_package.zip")).unwrap(),
            previous
        );
    }
    fs::write(fixture.0.join("README.md"), "# Package").unwrap();
    for (id, file) in [("plugin", "missing.js"), ("../escape", "plugin/index.js")] {
        fixture.metadata(json!({"id":"package","name":"Package","version":"1.0.0","plugins":[{"id":"plugin","root":"plugin"}]}), vec![json!({"id":id,"kind":"backend","fileName":file,"assets":[]})]);
        assert_eq!(fixture.build().status.code(), Some(1));
        assert_eq!(fs::read(fixture.0.join("dist/sentinel")).unwrap(), b"keep");
        assert_eq!(
            fs::read(fixture.0.join("dist/plugin_package.zip")).unwrap(),
            previous
        );
    }
    fixture.metadata(
        json!({"id":"package","plugins":[{"id":"plugin","root":"dist/plugin"}]}),
        vec![],
    );
    assert_eq!(fixture.build().status.code(), Some(1));
    assert_eq!(fs::read(fixture.0.join("dist/sentinel")).unwrap(), b"keep");
}

#[cfg(unix)]
#[test]
fn native_cli_accepts_project_alias_but_rejects_symlinked_output_and_sources() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let links = Fixture::new();
    let alias = links.0.join("alias");
    symlink(&fixture.0, &alias).unwrap();
    let output = fixture.build_at(&alias);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_file(fixture.0.join("image.png")).unwrap();
    symlink(links.0.join("image.png"), fixture.0.join("image.png")).unwrap();
    assert_eq!(fixture.build().status.code(), Some(1));
    fs::remove_dir_all(fixture.0.join("dist")).unwrap();
    fs::write(links.0.join("keep"), b"sentinel").unwrap();
    symlink(&links.0, fixture.0.join("dist")).unwrap();
    assert_eq!(fixture.build().status.code(), Some(1));
    assert_eq!(fs::read(links.0.join("keep")).unwrap(), b"sentinel");
}
