use super::*;

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

#[tokio::test]
async fn ephemeral_port_is_advertised() {
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
async fn download_is_404_until_success_and_retains_published_bytes() {
    let (_, shutdown) = watch::channel(false);
    let state = ServerState::new(String::new(), 3000, shutdown);
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
