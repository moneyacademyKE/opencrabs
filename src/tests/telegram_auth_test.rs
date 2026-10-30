//! telegram_auth tests: HMAC validation vectors + the initData door's
//! three closed states and its one open one.
//!
//! Payloads are crafted exactly the way Telegram signs them: a urlencoded
//! query string whose `hash` is HMAC-SHA256(secret, data_check_string) over
//! every other field, sorted, joined with newlines.

use std::path::Path;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use crate::a2a::handler;
use crate::a2a::pairing_http::new_pair_hits;
use crate::a2a::server::{A2aState, build_router};
use crate::remote::now_secs;
use crate::remote::pairing::PairingStore;
use crate::remote::telegram_auth::{MAX_AGE_SECS, hex, hmac_sha256, validate_init_data};

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn signed_init_data(bot_token: &str, user_id: i64, auth_date: i64) -> String {
    let user = format!("{{\"id\":{user_id},\"username\":\"moe\"}}");
    // Telegram signs the DECODED key=value list; only the wire form is urlencoded.
    let mut fields = vec![
        ("auth_date".to_string(), auth_date.to_string()),
        ("query_id".to_string(), "AATe4Y0".to_string()),
        ("user".to_string(), user),
    ];
    fields.sort();
    let check = fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");
    let secret = hmac_sha256(bot_token.as_bytes(), b"WebAppData");
    let hash = hex(&hmac_sha256(&secret, check.as_bytes()));
    fields.push(("hash".to_string(), hash));
    fields
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn now() -> i64 {
    i64::try_from(now_secs()).unwrap_or(0)
}

#[test]
fn validates_a_wellformed_payload() {
    let data = signed_init_data("test-bot-token", 42, now());
    let user = validate_init_data(&data, "test-bot-token", MAX_AGE_SECS).unwrap();
    assert_eq!(user.id, 42);
    assert_eq!(user.username.as_deref(), Some("moe"));
}

#[test]
fn rejects_a_wrong_bot_token() {
    let data = signed_init_data("real-token", 42, now());
    assert!(validate_init_data(&data, "other-token", MAX_AGE_SECS).is_err());
}

#[test]
fn rejects_a_tampered_payload() {
    let data = signed_init_data("t", 42, now());
    let tampered = data.replace("AATe4Y0", "AATe4Y1");
    assert!(validate_init_data(&tampered, "t", MAX_AGE_SECS).is_err());
}

#[test]
fn rejects_a_stale_payload() {
    let stale = now() - MAX_AGE_SECS - 60;
    let data = signed_init_data("t", 42, stale);
    assert!(validate_init_data(&data, "t", MAX_AGE_SECS).is_err());
}

#[test]
fn rejects_a_future_dated_payload() {
    let future = now() + 3_600;
    let data = signed_init_data("t", 42, future);
    assert!(validate_init_data(&data, "t", MAX_AGE_SECS).is_err());
}

#[test]
fn rejects_a_missing_hash() {
    assert!(validate_init_data("auth_date=1&user=%7B%7D", "t", MAX_AGE_SECS).is_err());
}

#[test]
fn issue_session_creates_a_verifiable_token() {
    let dir = tempfile::tempdir().unwrap();
    let store = PairingStore::new(dir.path().join("pairing.json"));
    let (token, session) = store
        .issue_session("telegram:moe", vec!["a2a:read".into(), "a2a:send".into()])
        .unwrap();
    assert!(token.starts_with("ocpair_"));
    assert_eq!(session.scopes.len(), 2);
    let verified = store.verify_token(&token).expect("round trip verifies");
    assert_eq!(verified.id, session.id);
}

// ── endpoint integration: the door ships closed, opens for the allowlist

async fn state_with(bot_token: Option<&str>, telegram_users: Vec<i64>, dir: &Path) -> A2aState {
    use crate::a2a::test_helpers::helpers;
    A2aState {
        task_store: handler::new_task_store(),
        cancel_store: handler::new_cancel_store(),
        host: "127.0.0.1".to_string(),
        port: 18790,
        agent_service: helpers::placeholder_agent_service().await,
        service_context: helpers::placeholder_service_context().await,
        api_key: Some("k".into()),
        pairing: PairingStore::new(dir.join("pairing.json")),
        pair_hits: new_pair_hits(),
        remote_state_dir: dir.to_path_buf(),
        telegram_token: bot_token.map(str::to_string),
        telegram_users,
    }
}

async fn post_init_data(app: Router, init_data: &str) -> (StatusCode, String) {
    let resp = app
        .oneshot(
            Request::post("/a2a/telegram/init-data")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "init_data": init_data }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn door_closed_without_bot_token() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_router(state_with(None, vec![42], dir.path()).await, &[]);
    let (status, body) = post_init_data(app, &signed_init_data("t", 42, now())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body.contains("not configured"));
}

#[tokio::test]
async fn door_closed_with_empty_allowlist() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_router(state_with(Some("t"), vec![], dir.path()).await, &[]);
    let (status, body) = post_init_data(app, &signed_init_data("t", 42, now())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body.contains("allowlist"));
}

#[tokio::test]
async fn door_closed_for_unlisted_user() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_router(state_with(Some("t"), vec![7], dir.path()).await, &[]);
    let (status, _) = post_init_data(app, &signed_init_data("t", 42, now())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn allowlisted_user_gets_a_session_token() {
    let dir = tempfile::tempdir().unwrap();
    let state = state_with(Some("test-bot-token"), vec![42], dir.path()).await;
    let app = build_router(state.clone(), &[]);
    let (status, body) = post_init_data(app, &signed_init_data("test-bot-token", 42, now())).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    let token = parsed["token"].as_str().unwrap();
    let session = state.pairing.verify_token(token).expect("token verifies");
    assert!(session.scopes.contains(&"a2a:send".to_string()));
    assert!(session.scopes.contains(&"a2a:read".to_string()));
}

#[tokio::test]
async fn garbage_payload_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let app = build_router(state_with(Some("t"), vec![42], dir.path()).await, &[]);
    let (status, _) = post_init_data(app, "hash=deadbeef&auth_date=1").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
