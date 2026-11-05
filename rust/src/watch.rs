use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::{json, Value};
use tokio::{
    sync::{broadcast, mpsc, watch, RwLock},
    task::JoinHandle,
    time::Instant,
};

use crate::{adapter::Cancellation, build, cli};

const DEBOUNCE: Duration = Duration::from_millis(200);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Default)]
struct Scheduler {
    deadline: Option<Instant>,
    active: bool,
}

impl Scheduler {
    fn changed(&mut self, now: Instant) {
        self.deadline = Some(now + DEBOUNCE);
    }

    fn ready(&self, now: Instant) -> bool {
        !self.active && self.deadline.is_some_and(|deadline| deadline <= now)
    }

    fn start(&mut self) {
        self.active = true;
        self.deadline = None;
    }

    fn finished(&mut self) {
        self.active = false;
    }
}

#[derive(Clone)]
struct ServerState {
    current: Arc<RwLock<Published>>,
    messages: broadcast::Sender<String>,
    shutdown: watch::Receiver<bool>,
    port: u16,
}

#[derive(Default)]
struct Published {
    package_id: String,
    // Retain the last successful package even if a subsequent assembly fails.
    zip: Option<Vec<u8>>,
}

fn connected(package_id: &str, port: u16) -> Value {
    json!({"kind": "connected", "packageId": package_id, "downloadUrl": download_url(port)})
}

fn rebuild(port: u16) -> Value {
    json!({"kind": "rebuild", "downloadUrl": download_url(port)})
}

fn error_message(error: &str) -> Value {
    json!({"kind": "error", "error": error})
}

fn download_url(port: u16) -> String {
    format!("http://localhost:{port}/plugin_package.zip")
}

fn report(state: &ServerState, error: &str) {
    eprintln!("{error}");
    let _ = state.messages.send(error_message(error).to_string());
}

async fn download(State(state): State<ServerState>) -> Response {
    let current = state.current.read().await;
    match &current.zip {
        Some(bytes) => (
            [
                (header::CONTENT_TYPE, "application/zip"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"plugin_package.zip\"",
                ),
                (header::CACHE_CONTROL, "no-store"),
            ],
            Body::from(bytes.clone()),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "Plugin package not found").into_response(),
    }
}

async fn upgrade(ws: WebSocketUpgrade, State(state): State<ServerState>) -> Response {
    ws.on_upgrade(move |socket| client(socket, state))
}

async fn send(socket: &mut WebSocket, message: String) -> bool {
    matches!(
        tokio::time::timeout(SHUTDOWN_TIMEOUT, socket.send(Message::Text(message.into()))).await,
        Ok(Ok(()))
    )
}

async fn client(mut socket: WebSocket, mut state: ServerState) {
    let mut messages = state.messages.subscribe();
    let id = state.current.read().await.package_id.clone();
    if !send(&mut socket, connected(&id, state.port).to_string()).await {
        return;
    }
    loop {
        tokio::select! {
            _ = state.shutdown.changed() => break,
            incoming = socket.recv() => {
                match incoming {
                    None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                    _ => {}
                }
            }
            message = messages.recv() => {
                let message = match message {
                    Ok(message) => message,
                    // A slow client can fetch the latest retained package rather than stale notifications.
                    Err(broadcast::error::RecvError::Lagged(_)) => rebuild(state.port).to_string(),
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if !send(&mut socket, message).await { break; }
            }
        }
    }
    let _ = tokio::time::timeout(SHUTDOWN_TIMEOUT, socket.send(Message::Close(None))).await;
}

fn ignored(path: &Path) -> bool {
    path.components().any(
        |part| matches!(part, Component::Normal(name) if name == "dist" || name == "node_modules"),
    )
}

fn plugin_roots(cwd: &Path, config: &Value) -> Vec<PathBuf> {
    config["plugins"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|plugin| plugin["root"].as_str())
        .filter_map(|root| crate::filesystem::absolute(&cwd.join(root)).ok())
        .collect()
}

fn config_port(config: &Value) -> Result<u16, String> {
    let value = &config["watch"]["port"];
    if value.is_null() {
        return Ok(3000);
    }
    value
        .as_f64()
        .filter(|port| port.is_finite() && port.fract() == 0.0 && (0.0..=65535.0).contains(port))
        .map(|port| port as u16)
        .ok_or_else(|| format!("Invalid watch port {value}: expected an integer from 0 to 65535"))
}

fn config_path(cwd: &Path, custom: Option<&str>) -> Result<String, String> {
    let path = match custom {
        Some(custom) => crate::filesystem::absolute(Path::new(custom))?,
        None => cwd.join("caido.config.ts"),
    };
    path.into_os_string()
        .into_string()
        .map_err(|_| "Config path is not valid UTF-8".into())
}

fn watch_files(cwd: &Path, config_path: Option<&str>) -> Vec<PathBuf> {
    vec![
        cwd.join(config_path.unwrap_or("caido.config.ts")),
        cwd.join("package.json"),
        cwd.join("README.md"),
    ]
}

fn relevant(path: &Path, files: &[PathBuf], roots: &[PathBuf]) -> bool {
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

fn make_watcher(
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

enum Cycle {
    Loaded(Result<Value, String>),
    Built(Result<Vec<u8>, String>),
}

fn build_cycle(
    cwd: &Path,
    config_path: Option<&str>,
    config: &Value,
    cancellation: &Cancellation,
) -> Cycle {
    Cycle::Built((|| {
        build::build_loaded_cancellable(cwd, config_path, config, cancellation)?;
        cancellation.check()?;
        std::fs::read(cwd.join("dist").join("plugin_package.zip"))
            .map_err(|error| format!("Cannot read built plugin package: {error}"))
    })())
}

async fn shutdown_signal() -> Result<(), String> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate())
            .map_err(|error| format!("Cannot listen for SIGTERM: {error}"))?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result.map_err(|error| format!("Cannot listen for SIGINT: {error}")),
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .map_err(|error| format!("Cannot listen for shutdown: {error}"))
}

pub fn run(options: cli::PathOptions) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Cannot start watch runtime: {error}"))?;
    let result = runtime.block_on(run_async(options));
    // run_async cancels and joins blocking work before runtime shutdown.
    runtime.shutdown_timeout(SHUTDOWN_TIMEOUT);
    result
}

async fn run_async(options: cli::PathOptions) -> Result<(), String> {
    run_with_shutdown(options, shutdown_signal()).await
}

async fn run_with_shutdown(
    options: cli::PathOptions,
    signal: impl std::future::Future<Output = Result<(), String>>,
) -> Result<(), String> {
    let cwd = std::fs::canonicalize(options.path.as_deref().unwrap_or("."))
        .map_err(|error| format!("Cannot open project directory: {error}"))?;
    if !cwd.is_dir() {
        return Err(format!("{} is not a directory", cwd.display()));
    }
    let config_path = Some(config_path(&cwd, options.config_path())?);
    let files = watch_files(&cwd, config_path.as_deref());
    let (events_tx, mut events) = mpsc::channel(1);
    let mut watcher = make_watcher(&files, &[], events_tx.clone())?;
    tokio::pin!(signal);
    let initial_cwd = cwd.clone();
    let initial_path = config_path.clone();
    let cancellation = Cancellation::default();
    let initial_cancellation = cancellation.clone();
    let mut initial = tokio::task::spawn_blocking(move || {
        build::load_cancellable(&initial_cwd, initial_path.as_deref(), &initial_cancellation)
    });
    let initial = tokio::select! {
        biased;
        result = &mut signal => {
            cancel_and_join(&cancellation, Some(initial)).await?;
            return result;
        },
        result = &mut initial => result.map_err(|error| format!("Config task failed: {error}"))?,
    };
    let initial_error = initial.as_ref().err().cloned();
    let mut config = initial.ok();
    let requested_port = config
        .as_ref()
        .map(config_port)
        .transpose()?
        .unwrap_or(3000);
    let mut configured_port = requested_port;
    let roots = config
        .as_ref()
        .map(|config| plugin_roots(&cwd, config))
        .unwrap_or_default();
    if !roots.is_empty() {
        watcher = make_watcher(&files, &roots, events_tx.clone())?;
    }
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, requested_port))
        .await
        .map_err(|error| format!("Cannot bind watch server on port {requested_port}: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Cannot inspect watch server address: {error}"))?
        .port();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (messages, _) = broadcast::channel(64);
    let state = ServerState {
        current: Arc::new(RwLock::new(Published {
            package_id: config
                .as_ref()
                .and_then(|config| config["id"].as_str())
                .unwrap_or("")
                .to_owned(),
            zip: None,
        })),
        messages,
        shutdown: shutdown_rx.clone(),
        port,
    };
    let app = Router::new()
        .route("/plugin_package.zip", get(download))
        .fallback(get(upgrade))
        .with_state(state.clone());
    let mut server_shutdown = shutdown_rx;
    let mut server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = server_shutdown.changed().await;
            })
            .await
    });
    eprintln!("Development server running at http://localhost:{port}");
    if let Some(error) = initial_error {
        report(&state, &error);
    }
    let mut scheduler = Scheduler {
        deadline: Some(Instant::now()),
        active: false,
    };
    let mut active: Option<JoinHandle<Cycle>> = None;
    let mut port_notice = None;
    let outcome = loop {
        if scheduler.ready(Instant::now()) {
            scheduler.start();
            let cwd = cwd.clone();
            let config_path = config_path.clone();
            let cancellation = cancellation.clone();
            active = Some(tokio::task::spawn_blocking(move || {
                Cycle::Loaded(build::load_cancellable(
                    &cwd,
                    config_path.as_deref(),
                    &cancellation,
                ))
            }));
        }
        let deadline = scheduler.deadline.filter(|_| !scheduler.active);
        tokio::select! {
            biased;
            result = &mut signal => break result,
            result = &mut server => break Err(match result {
                Ok(Err(error)) => format!("Watch server failed on port {port}: {error}"),
                Ok(Ok(())) => "Watch server stopped unexpectedly".to_owned(),
                Err(error) => format!("Watch server task failed: {error}"),
            }),
            Some(()) = events.recv() => scheduler.changed(Instant::now()),
            _ = async { match deadline { Some(deadline) => tokio::time::sleep_until(deadline).await, None => std::future::pending().await } } => {},
            result = async { match active.as_mut() { Some(task) => task.await, None => std::future::pending().await } } => {
                active = None;
                scheduler.finished();
                let cycle = match result {
                    Ok(cycle) => cycle,
                    Err(error) => { report(&state, &format!("Build task failed: {error}")); continue; }
                };
                match cycle {
                    Cycle::Loaded(loaded) => {
                        let loaded = loaded.and_then(|next| config_port(&next).map(|_| next));
                        let candidate = loaded.as_ref().ok().or(config.as_ref());
                        let next_roots = candidate.map(|next| plugin_roots(&cwd, next)).unwrap_or_default();
                        // Refresh even after failed loads, advancing watches along
                        // newly created ancestors of a missing config file.
                        match make_watcher(&files, &next_roots, events_tx.clone()) {
                            Ok(next_watcher) => watcher = next_watcher,
                            Err(error) => { report(&state, &error); continue; }
                        }
                        match loaded {
                            Ok(next) => {
                                let next_port = config_port(&next).expect("validated port");
                                if next_port != configured_port && port_notice != Some(next_port) {
                                    report(&state, &format!("Watch port changed to {next_port}; restart watch to use the new port (still serving port {port})"));
                                    port_notice = Some(next_port);
                                }
                                configured_port = next_port;
                                state.current.write().await.package_id = next["id"].as_str().unwrap_or("").to_owned();
                                config = Some(next.clone());
                                // Keep the pending deadline: edits received during
                                // config evaluation still require a trailing cycle.
                                scheduler.active = true;
                                let cwd = cwd.clone();
                                let config_path = config_path.clone();
                                let cancellation = cancellation.clone();
                                active = Some(tokio::task::spawn_blocking(move || {
                                    build_cycle(&cwd, config_path.as_deref(), &next, &cancellation)
                                }));
                            }
                            Err(error) => report(&state, &error),
                        }
                    }
                    Cycle::Built(result) => {
                        // Build tools may replace roots; refresh registrations
                        // without scheduling an unconditional rebuild.
                        let roots = config.as_ref().map(|next| plugin_roots(&cwd, next)).unwrap_or_default();
                        match make_watcher(&files, &roots, events_tx.clone()) {
                            Ok(next_watcher) => watcher = next_watcher,
                            Err(error) => report(&state, &error),
                        }
                        match result {
                            Ok(zip) => {
                                state.current.write().await.zip = Some(zip);
                                let _ = state.messages.send(rebuild(port).to_string());
                            }
                            Err(error) => report(&state, &error),
                        }
                    }
                }
            }
        }
    };
    cancellation.cancel();
    drop(watcher);
    drop(config);
    let _ = shutdown_tx.send(true);
    if !server.is_finished()
        && tokio::time::timeout(SHUTDOWN_TIMEOUT, &mut server)
            .await
            .is_err()
    {
        server.abort();
    }
    cancel_and_join(&cancellation, active).await?;
    outcome
}

async fn cancel_and_join<T: Send + 'static>(
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
mod tests {
    use super::*;

    #[tokio::test]
    async fn shutdown_cancels_and_joins_blocking_worker_before_returning() {
        let cancellation = Cancellation::default();
        let worker_cancellation = cancellation.clone();
        let task = tokio::task::spawn_blocking(move || {
            while worker_cancellation.check().is_ok() {
                std::thread::yield_now();
            }
            assert!(worker_cancellation
                .assemble(|| -> Result<(), String> { panic!("assembly started after shutdown") })
                .is_err());
            42
        });
        tokio::time::timeout(
            Duration::from_secs(5),
            cancel_and_join(&cancellation, Some(task)),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(cancellation.check().is_err());
    }

    #[tokio::test]
    async fn shutdown_drains_assembly_already_in_progress() {
        let cancellation = Cancellation::default();
        let worker_cancellation = cancellation.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let committed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_committed = committed.clone();
        let task = tokio::task::spawn_blocking(move || {
            worker_cancellation
                .assemble(|| {
                    started_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    worker_committed.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                })
                .unwrap();
        });
        started_rx.await.unwrap();
        let shutdown = cancel_and_join(&cancellation, Some(task));
        tokio::pin!(shutdown);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut shutdown)
                .await
                .is_err()
        );
        assert!(cancellation.check().is_err());
        release_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), shutdown)
            .await
            .unwrap()
            .unwrap();
        assert!(committed.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn debounce_is_trailing_edge() {
        let now = Instant::now();
        let mut scheduler = Scheduler::default();
        scheduler.changed(now);
        scheduler.changed(now + Duration::from_millis(150));
        assert!(!scheduler.ready(now + Duration::from_millis(349)));
        assert!(scheduler.ready(now + Duration::from_millis(350)));
    }

    #[test]
    fn changes_during_build_produce_only_one_trailing_build() {
        let now = Instant::now();
        let mut scheduler = Scheduler::default();
        scheduler.start();
        for offset in 0..100 {
            scheduler.changed(now + Duration::from_millis(offset));
        }
        assert!(!scheduler.ready(now + Duration::from_secs(1)));
        scheduler.finished();
        assert!(scheduler.ready(now + Duration::from_secs(1)));
        scheduler.start();
        scheduler.finished();
        assert!(!scheduler.ready(now + Duration::from_secs(2)));
    }

    #[test]
    fn trailing_build_waits_for_latest_event() {
        let now = Instant::now();
        let mut scheduler = Scheduler::default();
        scheduler.start();
        scheduler.changed(now);
        scheduler.finished();
        assert!(!scheduler.ready(now + Duration::from_millis(199)));
        assert!(scheduler.ready(now + DEBOUNCE));
    }

    #[test]
    fn protocol_shapes_are_compatible() {
        assert_eq!(
            connected("test", 3000),
            json!({"kind":"connected", "packageId":"test", "downloadUrl":"http://localhost:3000/plugin_package.zip"})
        );
        assert_eq!(
            rebuild(4123),
            json!({"kind":"rebuild", "downloadUrl":"http://localhost:4123/plugin_package.zip"})
        );
        assert_eq!(
            error_message("failed"),
            json!({"kind":"error", "error":"failed"})
        );
    }

    #[test]
    fn roots_and_metadata_filter_events_without_output_loops() {
        let cwd = std::env::current_dir().unwrap();
        let files = watch_files(&cwd, Some("config/custom.ts"));
        let roots = plugin_roots(&cwd, &json!({"plugins":[{"root":"frontend"}]}));
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
        assert_eq!(config_port(&json!({})).unwrap(), 3000);
        assert_eq!(config_port(&json!({"watch":{"port":4567}})).unwrap(), 4567);
    }

    #[test]
    fn paths_are_caller_relative_and_ignores_are_root_relative() {
        let caller = std::env::current_dir().unwrap();
        let project = caller.join("dist/node_modules/project");
        let custom = config_path(&project, Some("./custom.ts")).unwrap();
        assert_eq!(Path::new(&custom), caller.join("custom.ts"));
        let files = watch_files(&project, Some(&custom));
        let roots = plugin_roots(
            &project,
            &json!({"plugins":[{"root":"./frontend/../frontend"}]}),
        );
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
    async fn ephemeral_port_is_advertised_and_invalid_ports_fail() {
        assert_eq!(config_port(&json!({"watch":{"port":0}})).unwrap(), 0);
        for value in [json!(-1), json!(1.5), json!(65536), json!("3000")] {
            assert!(config_port(&json!({"watch":{"port":value}})).is_err());
        }
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_ne!(port, 0);
        assert_eq!(
            connected("test", port)["downloadUrl"],
            format!("http://localhost:{port}/plugin_package.zip")
        );
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
        // A failed reload must refresh registration, even with no valid config.
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
        // Candidate roots are registered before build work starts, so edits
        // during that work remain queued for one trailing cycle.
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
        let response =
            cli::parse(vec!["watch".into(), project.0.to_str().unwrap().into()]).unwrap();
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

    #[tokio::test]
    async fn download_is_404_until_success_and_retains_published_bytes() {
        let (_, shutdown) = watch::channel(false);
        let (messages, _) = broadcast::channel(1);
        let state = ServerState {
            current: Arc::new(RwLock::new(Published::default())),
            messages,
            shutdown,
            port: 3000,
        };
        assert_eq!(
            download(State(state.clone())).await.status(),
            StatusCode::NOT_FOUND
        );
        state.current.write().await.zip = Some(vec![1, 2, 3]);
        let response = download(State(state.clone())).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/zip");
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 10)
                .await
                .unwrap()
                .as_ref(),
            &[1, 2, 3]
        );
        report(&state, "later build failed");
        assert_eq!(download(State(state)).await.status(), StatusCode::OK);
    }
}
