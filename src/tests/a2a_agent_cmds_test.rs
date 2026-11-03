//! `agent/*` JSON-RPC handlers (Mini App parity Phase B): command
//! catalogue, models, usage, clear/compact, plan ops, goal gate — each
//! delegating to the same machinery the TUI dispatcher calls.

use serde_json::{Value, json};

use crate::a2a::handler;
use crate::a2a::types::{JsonRpcRequest, error_codes};

struct Rig {
    ctx: crate::services::ServiceContext,
    agent: std::sync::Arc<crate::brain::agent::service::AgentService>,
}

impl Rig {
    async fn new() -> Self {
        use crate::a2a::test_helpers::helpers;
        // The AgentService must be built over the SAME ServiceContext the
        // rig dispatches with — production shares one context between the
        // gateway and the agent (start_server / ui.rs). Building the agent
        // over a second placeholder context points session writes at one
        // in-memory DB and message writes at another (FK failure).
        let ctx = helpers::placeholder_service_context().await;
        let config = crate::config::Config::default();
        let agent = std::sync::Arc::new(
            crate::brain::agent::service::AgentService::new(
                std::sync::Arc::new(crate::brain::provider::PlaceholderProvider),
                ctx.clone(),
                &config,
            )
            .await,
        );
        Self { ctx, agent }
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
async fn commands_catalogue_carries_builtin_local_rows() {
    let rig = Rig::new().await;
    let body = ok(&rig.rpc("agent/commands", json!({})).await);
    let rows = body["commands"].as_array().expect("commands array");
    assert!(!rows.is_empty(), "catalogue is not empty");
    let by_name = |name: &str| {
        rows.iter()
            .find(|r| r["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("{name} in catalogue"))
    };
    // Local, gateway-answered commands from the TUI SLASH_COMMANDS table.
    assert_eq!(by_name("/help")["section"].as_str(), Some("BuiltIn"));
    assert_eq!(by_name("/clear")["kind"].as_str(), Some("local"));
    assert_eq!(by_name("/models")["kind"].as_str(), Some("panel"));
    assert_eq!(by_name("/usage")["kind"].as_str(), Some("panel"));
    assert_eq!(by_name("/plan")["kind"].as_str(), Some("local"));
    // /goal rides as a prompt turn (bare /goal is goal-check's job).
    assert_eq!(by_name("/goal")["kind"].as_str(), Some("prompt"));
    // Terminal-constitutive commands are NOT offered on this surface.
    assert!(
        rows.iter().all(|r| r["name"].as_str() != Some("/exit")),
        "/exit must not be offered remotely"
    );
    assert!(
        rows.iter().all(|r| r["name"].as_str() != Some("/restart")),
        "/restart must not be offered remotely"
    );
}

#[tokio::test]
async fn models_reports_current_pair_without_session() {
    let rig = Rig::new().await;
    let body = ok(&rig.rpc("agent/models", json!({})).await);
    assert!(body["current"]["provider"].is_string());
    assert!(body["models"].is_array());
    assert!(body["providers"].is_array());
}

#[tokio::test]
async fn usage_returns_dashboard_shape_on_empty_db() {
    let rig = Rig::new().await;
    let body = ok(&rig.rpc("agent/usage", json!({ "period": "week" })).await);
    assert_eq!(body["period"].as_str(), Some("Week"));
    assert_eq!(body["summary"]["totalTokens"].as_i64(), Some(0));
    assert!(body["models"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn compact_returns_the_system_trigger_prompt() {
    let rig = Rig::new().await;
    let created = ok(&rig.rpc("sessions/create", json!({})).await);
    let body = ok(
        &rig.rpc(
            "agent/compact",
            json!({ "sessionId": created["id"].as_str().unwrap() }),
        )
        .await,
    );
    assert!(
        body["prompt"]
            .as_str()
            .unwrap()
            .contains("Compact context now")
    );
}

#[tokio::test]
async fn goal_check_answers_bare_goal_locally() {
    let rig = Rig::new().await;
    let body = ok(&rig.rpc("agent/goal-check", json!({ "args": "" })).await);
    assert_eq!(body["handled"], true);
    assert!(
        body["reply"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("goal")
    );
    let body = ok(
        &rig.rpc("agent/goal-check", json!({ "args": " status" }))
            .await,
    );
    // Non-bare args become a prompt turn (goal_command_prompt).
    assert_eq!(body["handled"], false);
    assert!(body["prompt"].is_string());
}

#[tokio::test]
async fn plan_show_without_plan_is_a_clean_refusal() {
    let rig = Rig::new().await;
    let created = ok(&rig.rpc("sessions/create", json!({})).await);
    let body = ok(
        &rig.rpc(
            "agent/plan-show",
            json!({ "sessionId": created["id"].as_str().unwrap() }),
        )
        .await,
    );
    assert!(
        body["reply"]
            .as_str()
            .unwrap()
            .contains("No active plan")
    );
}

#[tokio::test]
async fn set_model_rejects_malformed_pair() {
    let rig = Rig::new().await;
    let created = ok(&rig.rpc("sessions/create", json!({})).await);
    let resp = rig
        .rpc(
            "agent/set-model",
            json!({
                "sessionId": created["id"].as_str().unwrap(),
                "model": "not-a-pair",
            }),
        )
        .await;
    let err = resp.error.expect("error expected");
    assert_eq!(err.code, error_codes::INVALID_PARAMS);
}

#[tokio::test]
async fn clear_context_requires_a_real_session() {
    let rig = Rig::new().await;
    let created = ok(&rig.rpc("sessions/create", json!({})).await);
    let body = ok(
        &rig.rpc(
            "agent/clear-context",
            json!({ "sessionId": created["id"].as_str().unwrap() }),
        )
        .await,
    );
    assert_eq!(body["ok"], true);
    assert!(body["reply"].is_string());
}
