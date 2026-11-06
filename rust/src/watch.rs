use std::time::Duration;

use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::Instant,
};

use self::{
    config::{config_path, watch_files},
    filesystem::make_watcher,
    server::{rebuild, report, ServerState},
    worker::{build_cycle, cancel_and_join, load, Cycle, Scheduler},
};
use crate::{adapter::Cancellation, cli};

mod config;
mod filesystem;
mod server;
mod worker;

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

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
    let result = runtime.block_on(run_with_shutdown(options, shutdown_signal()));
    // Blocking work is cancelled and joined before runtime shutdown.
    runtime.shutdown_timeout(SHUTDOWN_TIMEOUT);
    result
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
    let config_path = Some(config_path(&cwd, options.config.as_deref())?);
    let files = watch_files(&cwd, config_path.as_deref());
    let (events_tx, mut events) = mpsc::channel(1);
    let mut watcher = make_watcher(&files, &[], events_tx.clone())?;
    tokio::pin!(signal);
    let initial_cwd = cwd.clone();
    let initial_path = config_path.clone();
    let cancellation = Cancellation::default();
    let initial_cancellation = cancellation.clone();
    let mut initial = tokio::task::spawn_blocking(move || {
        load(&initial_cwd, initial_path.as_deref(), &initial_cancellation)
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
    let requested_port = config.as_ref().map_or(3000, |config| config.port);
    let mut configured_port = requested_port;
    let roots = config
        .as_ref()
        .map_or(&[][..], |config| config.roots.as_slice());
    if !roots.is_empty() {
        watcher = make_watcher(&files, roots, events_tx.clone())?;
    }
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, requested_port))
        .await
        .map_err(|error| format!("Cannot bind watch server on port {requested_port}: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Cannot inspect watch server address: {error}"))?
        .port();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let state = ServerState::new(
        config
            .as_ref()
            .map_or(String::new(), |config| config.id.clone()),
        port,
        shutdown_rx.clone(),
    );
    let app = state.router();
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
                Cycle::Loaded(load(&cwd, config_path.as_deref(), &cancellation))
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
                        let candidate = loaded.as_ref().ok().or(config.as_ref());
                        let roots = candidate.map_or(&[][..], |next| next.roots.as_slice());
                        // Refresh after failed loads too, advancing along newly created config ancestors.
                        match make_watcher(&files, roots, events_tx.clone()) {
                            Ok(next_watcher) => watcher = next_watcher,
                            Err(error) => { report(&state, &error); continue; }
                        }
                        match loaded {
                            Ok(next) => {
                                let next_port = next.port;
                                if next_port != configured_port && port_notice != Some(next_port) {
                                    report(&state, &format!("Watch port changed to {next_port}; restart watch to use the new port (still serving port {port})"));
                                    port_notice = Some(next_port);
                                }
                                configured_port = next_port;
                                state.current.write().await.package_id = next.id.clone();
                                let metadata = next.metadata.clone();
                                config = Some(next);
                                // Keep edits received during config evaluation queued for a trailing cycle.
                                scheduler.active = true;
                                let cwd = cwd.clone();
                                let config_path = config_path.clone();
                                let cancellation = cancellation.clone();
                                active = Some(tokio::task::spawn_blocking(move || {
                                    build_cycle(&cwd, config_path.as_deref(), &metadata, &cancellation)
                                }));
                            }
                            Err(error) => report(&state, &error),
                        }
                    }
                    Cycle::Built(result) => {
                        // Build tools may replace roots; refresh without forcing another build.
                        let roots = config.as_ref().map_or(&[][..], |next| next.roots.as_slice());
                        match make_watcher(&files, roots, events_tx.clone()) {
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

#[cfg(test)]
#[path = "watch/tests.rs"]
mod tests;
