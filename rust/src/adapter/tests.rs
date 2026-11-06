use super::*;

#[test]
fn cancellation_terminates_and_reaps_owned_node_child() {
    let path = std::env::temp_dir().join(format!(
        "caido-dev-cancel-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    let exchange = Exchange(path);
    let ready = exchange.0.join("ready");
    let cancellation = Cancellation::default();
    let worker_cancellation = cancellation.clone();
    let request = serde_json::json!({"ready":ready, "padding":"x".repeat(512 * 1024)});
    let worker = std::thread::spawn(move || {
        let mut command =
            Command::new(std::env::var_os("CAIDO_DEV_NODE").unwrap_or_else(|| "node".into()));
        command.arg("-e").arg(
            "let input='';process.stdin.on('data',c=>input+=c);process.stdin.on('end',()=>{const fs=require('node:fs');const r=JSON.parse(input);fs.writeSync(1,Buffer.alloc(1024*1024));fs.writeFileSync(r.ready,String(process.pid));setTimeout(()=>{},30000);});"
        );
        supervise(&mut command, &request, &worker_cancellation)
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !ready.exists() && !worker.is_finished() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let pid = std::fs::read_to_string(&ready);
    cancellation.cancel();
    let result = worker.join().unwrap();
    let pid = pid.expect("Node child did not become ready");
    assert!(result.unwrap_err().contains("cancelled"));
    #[cfg(unix)]
    assert!(
        !Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success(),
        "child is still alive or unreaped"
    );
    #[cfg(not(unix))]
    let _ = pid;
}

#[test]
fn cancellation_prevents_assembly_from_starting() {
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let mut committed = false;
    assert!(cancellation
        .assemble(|| {
            committed = true;
            Ok(())
        })
        .is_err());
    assert!(!committed);
}

#[test]
fn reports_nonzero_node_exit() {
    let mut command =
        Command::new(std::env::var_os("CAIDO_DEV_NODE").unwrap_or_else(|| "node".into()));
    command.args(["-e", "process.exit(7);"]);
    let error = supervise(&mut command, &json!({}), &Cancellation::default()).unwrap_err();
    assert!(error.contains("Node adapter failed"), "{error}");
    assert!(error.contains('7'), "{error}");
    assert!(error.contains("see stderr for diagnostics"), "{error}");
}

#[test]
fn reports_node_signal_termination() {
    let mut command =
        Command::new(std::env::var_os("CAIDO_DEV_NODE").unwrap_or_else(|| "node".into()));
    command.args(["-e", "process.kill(process.pid, 'SIGTERM');"]);
    let error = supervise(&mut command, &json!({}), &Cancellation::default()).unwrap_err();
    assert!(error.contains("Node adapter failed"), "{error}");
    #[cfg(unix)]
    assert!(error.contains("signal: 15"), "{error}");
    assert!(error.contains("see stderr for diagnostics"), "{error}");
}

#[test]
fn reports_node_start_failure() {
    let mut command = Command::new(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("Cargo.toml")
            .join("missing-node"),
    );
    let error = supervise(&mut command, &json!({}), &Cancellation::default()).unwrap_err();
    assert!(error.contains("Failed to start Node adapter"), "{error}");
}

#[test]
fn rejects_malformed_contaminated_and_incompatible_responses() {
    for input in [
        r#"{"version":2,"config":{},"buildOutputs":[]}"#,
        r#"{"version":1,"config":null,"buildOutputs":[]}"#,
        r#"{"version":1,"config":{},"buildOutputs":[null]}"#,
        r#"{"version":1,"config":{}}"#,
        "log\n{\"version\":1,\"config\":{},\"buildOutputs\":[]}",
        "{}{}",
    ] {
        assert!(
            parse_response(input.as_bytes()).is_err(),
            "accepted {input}"
        );
    }
}
