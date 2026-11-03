//! `agent/skills`, `agent/profiles`, `agent/mission-control` handlers
//! (Mini App parity Phase C): the data behind the TUI's SkillsList,
//! Profiles, and MissionControl dialogs.

use serde_json::{Value, json};

use crate::a2a::handler;
use crate::a2a::types::JsonRpcRequest;

struct Rig {
    ctx: crate::services::ServiceContext,
    agent: std::sync::Arc<crate::brain::agent::service::AgentService>,
}

impl Rig {
    async fn new() -> Self {
        use crate::a2a::test_helpers::helpers;
        // Same shared context for agent + dispatch (production shape).
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
async fn skills_lists_loaded_skills_with_slash_and_gate() {
    let rig = Rig::new().await;
    let body = ok(&rig.rpc("agent/skills", json!({})).await);
    let skills = body["skills"].as_array().expect("skills array");
    assert!(
        !skills.is_empty(),
        "built-in skills ship with the binary; the browser must list them"
    );
    for s in skills {
        assert!(s["slash"].as_str().unwrap().starts_with('/'));
        assert!(s["description"].is_string());
        assert!(s["reviewGate"].is_boolean());
    }
}

#[tokio::test]
async fn profiles_lists_default_and_flags_activity() {
    let rig = Rig::new().await;
    let body = ok(&rig.rpc("agent/profiles", json!({})).await);
    let profiles = body["profiles"].as_array().expect("profiles array");
    assert!(!profiles.is_empty(), "the default profile always exists");
    assert_eq!(profiles[0]["name"].as_str(), Some("default"));
    let actives = profiles.iter().filter(|p| p["active"] == true).count();
    assert_eq!(actives, 1, "exactly one active profile is flagged");
}

#[tokio::test]
async fn mission_control_renders_a_report() {
    let rig = Rig::new().await;
    let body = ok(&rig.rpc("agent/mission-control", json!({})).await);
    let report = body["report"].as_str().expect("report string");
    assert!(!report.trim().is_empty(), "report is not empty");
}

#[tokio::test]
async fn dialog_methods_are_read_scoped() {
    for m in ["agent/skills", "agent/profiles", "agent/mission-control"] {
        assert_eq!(
            crate::remote::scopes::scope_for_method(m),
            Some(crate::remote::scopes::SCOPE_READ),
            "{m} must be read-scoped"
        );
    }
}
