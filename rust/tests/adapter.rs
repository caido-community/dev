#[path = "../src/adapter.rs"]
mod adapter;

use std::path::PathBuf;

struct Environment {
    adapter: Option<std::ffi::OsString>,
    node: Option<std::ffi::OsString>,
    directory: PathBuf,
}

impl Drop for Environment {
    fn drop(&mut self) {
        for (key, value) in [
            ("CAIDO_DEV_ADAPTER", &self.adapter),
            ("CAIDO_DEV_NODE", &self.node),
        ] {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn subprocess_protocol_and_failures() {
    let directory = std::env::temp_dir().join(format!("caido-adapter-rust-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let environment = Environment {
        adapter: std::env::var_os("CAIDO_DEV_ADAPTER"),
        node: std::env::var_os("CAIDO_DEV_NODE"),
        directory,
    };
    let script = environment.directory.join("adapter.cjs");
    std::env::set_var("CAIDO_DEV_ADAPTER", &script);
    std::env::set_var("CAIDO_DEV_NODE", "node");
    std::fs::write(
        &script,
        r#"
        let input = '';
        process.stdin.on('data', chunk => input += chunk);
        process.stdin.on('end', () => {
            const request = JSON.parse(input);
            console.error('adapter diagnostic on stderr');
            console.log(JSON.stringify({version: 1, config: request, buildOutputs: [], assetMatches: {}}));
        });
    "#,
    )
    .unwrap();
    let response = adapter::invoke(&environment.directory, Some("./custom.ts"), false).unwrap();
    assert_eq!(response.config["version"], 1);
    assert_eq!(response.config["operation"], "load");
    assert_eq!(response.config["config"], "./custom.ts");
    assert_eq!(
        response.config["cwd"],
        environment.directory.to_str().unwrap()
    );
    let response = adapter::invoke(&environment.directory, None, true).unwrap();
    assert_eq!(response.config["operation"], "build");
    assert!(response.config.get("config").is_none());

    for source in [
        "console.log('contamination'); console.log(JSON.stringify({version:1,config:{},buildOutputs:[]}));",
        "console.log(JSON.stringify({version:2,config:{},buildOutputs:[]}));",
        "console.log('not json');",
        "process.exit(7);",
        "process.kill(process.pid, 'SIGTERM');",
    ] {
        std::fs::write(&script, source).unwrap();
        assert!(adapter::invoke(&environment.directory, None, true).is_err(), "accepted {source}");
    }
    std::env::set_var("CAIDO_DEV_NODE", environment.directory.join("missing-node"));
    assert!(adapter::invoke(&environment.directory, None, false)
        .unwrap_err()
        .contains("Failed to start Node adapter"));
}
