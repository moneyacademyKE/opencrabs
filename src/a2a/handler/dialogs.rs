//! Dialog data (Mini App parity Phase C): skills browser, profiles
//! (read-only), and the Mission Control report — the server half of the
//! TUI's SkillsList / Profiles / MissionControl dialogs.
//!
//! Each handler delegates to the SAME source the TUI dialog reads:
//! `skills::load_all_skills`, `config::profile::list_profiles`, and the
//! mission-control services behind `MissionControlReportTool` (same
//! `render_markdown`, so the phone report and the agent-sent report are
//! byte-identical).

use serde_json::{Value, json};

use crate::a2a::types::{JsonRpcResponse, error_codes};
use crate::services::ServiceContext;

fn err(id: Value, code: i64, msg: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse::error(id, code, msg)
}

/// `agent/skills` — every loaded skill with its slash name and whether
/// invoking it passes a review gate (the client confirms first).
pub async fn handle_skills(id: Value, _params: Value) -> JsonRpcResponse {
    let skills: Vec<Value> = crate::brain::skills::load_all_skills()
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "slash": s.slash_name,
                "description": s.description,
                "reviewGate": s.review_gate,
            })
        })
        .collect();
    JsonRpcResponse::success(id, json!({ "skills": skills }))
}

/// `agent/profiles` — read-only profile list. Creating, switching, or
/// migrating profiles changes which config/home the host runs under:
/// an operator act at the host, deliberately not a phone mutation
/// (spec exclusions). The active profile is flagged.
pub async fn handle_profiles(id: Value, _params: Value) -> JsonRpcResponse {
    match crate::config::profile::list_profiles() {
        Ok(profiles) => {
            let active = crate::config::profile::active_profile()
                .map(str::to_string)
                .unwrap_or_else(|| "default".to_string());
            let rows: Vec<Value> = profiles
                .iter()
                .map(|p| {
                    json!({
                        "name": p.name,
                        "description": p.description,
                        "createdAt": p.created_at,
                        "lastUsed": p.last_used,
                        "active": active == p.name,
                    })
                })
                .collect();
            JsonRpcResponse::success(id, json!({ "profiles": rows }))
        }
        Err(e) => err(
            id,
            error_codes::INTERNAL_ERROR,
            format!("profiles failed: {e}"),
        ),
    }
}

/// `agent/mission-control` — the shareable Mission Control report:
/// analytics + activity + inbox + schedule, rendered by the same
/// function the `mission_control_report` tool sends to chats.
pub async fn handle_mission_control(
    id: Value,
    _params: Value,
    ctx: ServiceContext,
) -> JsonRpcResponse {
    use crate::brain::mission_control::{
        TimeWindow, activity_service, analytics_service, inbox_service, schedule_service,
    };
    let pool = ctx.pool();
    let analytics = analytics_service::summary(pool.clone(), TimeWindow::All).await;
    let activity = activity_service::recent(5);
    let inbox = inbox_service::list();
    let schedule = schedule_service::list(pool).await;
    let report = crate::brain::tools::mission_control_report::render_markdown(
        &analytics, &activity, &inbox, &schedule,
    );
    JsonRpcResponse::success(id, json!({ "report": report }))
}
