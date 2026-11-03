//! `sessions/*` JSON-RPC handlers (Mini App parity Phase A): the gateway
//! exposes the same SessionService/MessageService data the TUI renders —
//! list, create (+ bound A2A context), history, rename, archive-delete.

use serde_json::{Value, json};

use crate::a2a::handler;
use crate::a2a::types::{JsonRpcRequest, error_codes};
use crate::services::{MessageService, SessionService};

async fn rpc(method: &str, params: Value) -> crate::a2a::types::JsonRpcResponse {
    use crate::a2a::test_helpers::helpers;
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: method.to_string(),
        params,
        id: json!(1),
    };
    handler::dispatch(
        req,
        handler::new_task_store(),
        handler::new_cancel_store(),
        helpers::placeholder_agent_service().await,
        helpers::placeholder_service_context().await,
    )
    .await
}

/// Dispatch against one shared context so state persists across calls.
struct Rig {
    ctx: crate::services::ServiceContext,
    agent: std::sync::Arc<crate::brain::agent::service::AgentService>,
}

impl Rig {
    async fn new() -> Self {
        use crate::a2a::test_helpers::helpers;
        Self {
            ctx: helpers::placeholder_service_context().await,
            agent: helpers::placeholder_agent_service().await,
        }
    }

    async fn rpc(&self, method: &str, params: Value) -> crate::a2a::types::JsonRpcResponse {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: json!(1),
        };
        handler::dispatch(
            req,
            handler::new_task_store(),
            handler::new_cancel_store(),
            self.agent.clone(),
            self.ctx.clone(),
        )
        .await
    }
}

fn ok(resp: &crate::a2a::types::JsonRpcResponse) -> Value {
    assert!(
        resp.error.is_none(),
        "expected success, got error: {:?}",
        resp.error
    );
    resp.result.clone().expect("result present")
}

#[tokio::test]
async fn create_then_list_shows_the_conversation_with_context() {
    let rig = Rig::new().await;
    let created = ok(&rig.rpc("sessions/create", json!({ "title": "parity probe" })).await);
    let session_id = created["id"].as_str().expect("session id").to_string();
    let context_id = created["contextId"]
        .as_str()
        .expect("create binds an A2A context")
        .to_string();
    assert!(!context_id.is_empty());

    let listed = ok(&rig.rpc("sessions/list", json!({})).await);
    let sessions = listed["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["id"].as_str(), Some(session_id.as_str()));
    assert_eq!(sessions[0]["title"].as_str(), Some("parity probe"));
}

#[tokio::test]
async fn history_returns_messages_oldest_first_and_capped() {
    let rig = Rig::new().await;
    let sessions = SessionService::new(rig.ctx.clone());
    let session = sessions.create_session(Some("h".into())).await.unwrap();
    let messages = MessageService::new(rig.ctx.clone());
    messages
        .create_message(session.id, "user".into(), "first".into())
        .await
        .unwrap();
    messages
        .create_message(session.id, "assistant".into(), "second".into())
        .await
        .unwrap();
    messages
        .create_message(session.id, "user".into(), "x".repeat(5_000))
        .await
        .unwrap();

    let history = ok(
        &rig.rpc(
            "sessions/history",
            json!({ "sessionId": session.id.to_string() }),
        )
        .await,
    );
    let items = history["messages"].as_array().expect("messages array");
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["content"].as_str(), Some("first"));
    assert_eq!(items[1]["role"].as_str(), Some("assistant"));
    assert!(
        items[2]["content"].as_str().unwrap().chars().count() <= 4_001,
        "oversized message truncated"
    );
    assert!(items[2]["content"].as_str().unwrap().ends_with('…'));
}

#[tokio::test]
async fn history_of_missing_session_is_not_found_not_empty() {
    let rig = Rig::new().await;
    let missing = uuid::Uuid::new_v4();
    let resp = rig
        .rpc("sessions/history", json!({ "sessionId": missing.to_string() }))
        .await;
    let err = resp.error.expect("error expected");
    assert_eq!(err.code, error_codes::TASK_NOT_FOUND);
}

#[tokio::test]
async fn rename_and_delete_archive_the_conversation() {
    let rig = Rig::new().await;
    let created = ok(&rig.rpc("sessions/create", json!({})).await);
    let id = created["id"].as_str().unwrap().to_string();

    let renamed = ok(
        &rig.rpc(
            "sessions/rename",
            json!({ "sessionId": id, "title": "renamed from phone" }),
        )
        .await,
    );
    assert_eq!(renamed["ok"], true);

    let listed = ok(&rig.rpc("sessions/list", json!({})).await);
    assert_eq!(
        listed["sessions"][0]["title"].as_str(),
        Some("renamed from phone")
    );

    let deleted = ok(&rig.rpc("sessions/delete", json!({ "sessionId": id })).await);
    assert_eq!(deleted["ok"], true);

    // Archived: gone from the default list, still retrievable when asked.
    let listed = ok(&rig.rpc("sessions/list", json!({})).await);
    assert_eq!(listed["sessions"].as_array().unwrap().len(), 0);
    let with_archived = ok(
        &rig.rpc("sessions/list", json!({ "includeArchived": true }))
            .await,
    );
    assert_eq!(with_archived["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(with_archived["sessions"][0]["archived"], true);
}

#[tokio::test]
async fn bad_session_id_is_invalid_params() {
    let resp = rpc("sessions/history", json!({ "sessionId": "not-a-uuid" })).await;
    let err = resp.error.expect("error expected");
    assert_eq!(err.code, error_codes::INVALID_PARAMS);
}

#[tokio::test]
async fn list_limit_is_clamped() {
    let rig = Rig::new().await;
    let sessions = SessionService::new(rig.ctx.clone());
    for i in 0..3 {
        sessions
            .create_session(Some(format!("s{i}")))
            .await
            .unwrap();
    }
    let listed = ok(&rig.rpc("sessions/list", json!({ "limit": 2 })).await);
    assert_eq!(listed["sessions"].as_array().unwrap().len(), 2);
}
