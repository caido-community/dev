use std::sync::Arc;

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
use serde_json::{json, Value};
use tokio::sync::{broadcast, watch, RwLock};

use super::SHUTDOWN_TIMEOUT;

#[derive(Clone)]
pub(super) struct ServerState {
    pub current: Arc<RwLock<Published>>,
    pub messages: broadcast::Sender<String>,
    shutdown: watch::Receiver<bool>,
    pub port: u16,
}

#[derive(Default)]
pub(super) struct Published {
    pub package_id: String,
    // Retain the last successful package even if a subsequent assembly fails.
    pub zip: Option<Vec<u8>>,
}

impl ServerState {
    pub fn new(package_id: String, port: u16, shutdown: watch::Receiver<bool>) -> Self {
        let (messages, _) = broadcast::channel(64);
        Self {
            current: Arc::new(RwLock::new(Published {
                package_id,
                zip: None,
            })),
            messages,
            shutdown,
            port,
        }
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/plugin_package.zip", get(download))
            .fallback(get(upgrade))
            .with_state(self.clone())
    }
}

fn connected(package_id: &str, port: u16) -> Value {
    json!({"kind": "connected", "packageId": package_id, "downloadUrl": download_url(port)})
}

pub(super) fn rebuild(port: u16) -> Value {
    json!({"kind": "rebuild", "downloadUrl": download_url(port)})
}

fn error_message(error: &str) -> Value {
    json!({"kind": "error", "error": error})
}

fn download_url(port: u16) -> String {
    format!("http://localhost:{port}/plugin_package.zip")
}

pub(super) fn report(state: &ServerState, error: &str) {
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
                    // A slow client can fetch the retained package rather than stale notifications.
                    Err(broadcast::error::RecvError::Lagged(_)) => rebuild(state.port).to_string(),
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if !send(&mut socket, message).await { break; }
            }
        }
    }
    let _ = tokio::time::timeout(SHUTDOWN_TIMEOUT, socket.send(Message::Close(None))).await;
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
