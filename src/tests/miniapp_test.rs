//! Mini App serving: the SPA shell and assets ride the public router —
//! same-origin, no Bearer, no secrets — and the shell carries the JSON-RPC
//! contract the client speaks.

use std::path::Path;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::a2a::handler;
use crate::a2a::pairing_http::new_pair_hits;
use crate::a2a::server::{A2aState, build_router};
use crate::remote::pairing::PairingStore;

async fn state_with(api_key: Option<&str>, dir: &Path) -> A2aState {
    use crate::a2a::test_helpers::helpers;
    A2aState {
        task_store: handler::new_task_store(),
        cancel_store: handler::new_cancel_store(),
        host: "127.0.0.1".to_string(),
        port: 18790,
        agent_service: helpers::placeholder_agent_service().await,
        service_context: helpers::placeholder_service_context().await,
        api_key: api_key.map(str::to_string),
        pairing: PairingStore::new(dir.join("pairing.json")),
        pair_hits: new_pair_hits(),
        remote_state_dir: dir.to_path_buf(),
        telegram_token: None,
        telegram_users: Vec::new(),
    }
}

async fn get_text(app: &Router, uri: &str) -> (StatusCode, String, Option<String>) {
    let resp = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap(), ct)
}

#[tokio::test]
async fn app_shell_serves_publicly_without_bearer() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_router(state_with(Some("k"), dir.path()).await, &[]);
    let (status, body, ct) = get_text(&app, "/a2a/app").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct.as_deref(), Some("text/html; charset=utf-8"));
    assert!(body.contains("OpenCrabs"), "the shell identifies the app");
    assert!(!body.contains("ocpair_"), "the shell embeds no secrets");
}

#[tokio::test]
async fn app_assets_serve_with_content_types() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_router(state_with(None, dir.path()).await, &[]);
    let (status, body, ct) = get_text(&app, "/a2a/app/app.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct.as_deref(), Some("application/javascript; charset=utf-8"));
    assert!(
        body.contains("message/send"),
        "the client speaks the JSON-RPC contract"
    );
    let (status, body, ct) = get_text(&app, "/a2a/app/style.css").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ct.as_deref(), Some("text/css; charset=utf-8"));
    assert!(body.contains("--bg"), "the stylesheet ships its tokens");
    assert!(
        body.contains("[hidden]"),
        "the stylesheet must enforce [hidden] with display none to prevent flex container leaks"
    );
}

#[tokio::test]
async fn beacon_endpoint_accepts_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_router(state_with(None, dir.path()).await, &[]);
    let (status, body, _) = get_text(&app, "/a2a/app/beacon?s=test&d=hello").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");
}

#[tokio::test]
async fn root_redirects_to_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_router(state_with(None, dir.path()).await, &[]);
    let resp = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        resp.headers().get("location").and_then(|v| v.to_str().ok()),
        Some("/a2a/app")
    );
}
