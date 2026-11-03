//! Agent-surface parity commands (Mini App Phase B): the TUI's
//! headless-safe slash commands as typed JSON-RPC methods.
//!
//! Each method delegates to the SAME code the TUI dispatcher calls —
//! `SLASH_COMMANDS` for the palette, `direct_model_switch` for models,
//! `usage::data` for usage, `AgentService::clear_context` and the
//! `[SYSTEM: …]` compaction trigger for context ops, and the
//! `plan_mode`/`goal` helpers for plan & goal — so the phone and the
//! terminal can never drift into two behaviours. Commands whose TUI
//! handling is a *prompt* (`/goal <text>`, skills, user commands) keep
//! working the way they always have: the client sends the text as a
//! normal `message/send` turn; `/goal` bare-usage and plan ops, which
//! the TUI answers locally without an LLM turn, get local handlers here.

use serde_json::{Value, json};
use uuid::Uuid;

use crate::a2a::types::{JsonRpcResponse, error_codes};
use crate::brain::agent::service::AgentService;
use crate::services::ServiceContext;
use std::sync::Arc;

fn err(id: Value, code: i64, msg: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse::error(id, code, msg)
}

fn parse_session_id(params: &Value) -> Result<Uuid, String> {
    let raw = params
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or("sessionId is required")?;
    Uuid::parse_str(raw).map_err(|_| format!("invalid sessionId: {raw}"))
}

fn opt_session_id(params: &Value) -> Option<Uuid> {
    params
        .get("sessionId")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// `agent/commands` — the parity-relevant command catalogue as data.
/// Built-in rows come straight from the TUI's `SLASH_COMMANDS` (single
/// source of truth, filtered to the set this surface can honour); user
/// commands and skills are loaded the same way the TUI help catalogue
/// loads them. Each row carries a `kind` so the client knows whether
/// tapping it runs locally (`local`), opens a panel (`panel`), or sends
/// the text as an agent turn (`prompt`).
pub async fn handle_commands(id: Value, _params: Value) -> JsonRpcResponse {
    // Commands answered by a local gateway handler (kind=local|panel).
    const LOCAL: &[(&str, &str)] = &[
        ("/new", "panel"),       // sessions/create via the chats view
        ("/sessions", "panel"),  // conversation list
        ("/clear", "local"),     // agent/clear-context
        ("/compact", "local"),   // agent/compact
        ("/help", "panel"),      // this catalogue, rendered
        ("/models", "panel"),    // agent/models + set-model
        ("/usage", "panel"),     // agent/usage
        ("/skills", "panel"),    // Phase C browser; rows below mark prompt skills
        ("/profiles", "panel"),  // Phase C
        ("/mission-control", "panel"), // Phase C
        ("/plan", "local"),      // agent/plan
        ("/show-plan", "local"), // agent/plan-show
        ("/execute", "local"),   // agent/plan-execute
        ("/discard", "local"),   // agent/plan-discard
        ("/stop", "local"),      // tasks/cancel (client-side)
        ("/rename", "local"),    // sessions/rename
        ("/theme", "local"),     // client-side CSS themes (Phase C)
    ];
    let is_local = |name: &str| LOCAL.iter().find(|(n, _)| *n == name);

    let mut rows: Vec<Value> = Vec::new();
    for cmd in crate::tui::app::state::SLASH_COMMANDS {
        if let Some((_, kind)) = is_local(cmd.name) {
            rows.push(json!({
                "name": cmd.name,
                "description": cmd.description,
                "section": "BuiltIn",
                "kind": kind,
            }));
        }
    }
    // Agent-dispatched built-ins the client sends as a turn: /goal (with
    // args) is the notable one; /goal bare is answered by agent/goal-check.
    rows.push(json!({
        "name": "/goal",
        "description": "Set/track an autonomous goal (/goal <text>, status, pause, resume, clear)",
        "section": "BuiltIn",
        "kind": "prompt",
    }));

    // Skills + user commands: same loaders the TUI help catalogue uses.
    // Disk reads happen here, once per palette open — never per frame/key.
    for skill in crate::brain::skills::load_all_skills() {
        rows.push(json!({
            "name": skill.slash_name,
            "description": skill.description,
            "section": "Skill",
            "kind": "prompt",
        }));
    }
    let brain_path = crate::brain::BrainLoader::resolve_path();
    let mut user_cmds = crate::brain::CommandLoader::from_brain_path(&brain_path).load();
    user_cmds.sort_by(|a, b| a.name.cmp(&b.name));
    for cmd in user_cmds {
        rows.push(json!({
            "name": cmd.name,
            "description": cmd.description,
            "section": "Custom",
            "kind": if cmd.action == "system" { "local-text" } else { "prompt" },
            "prompt": cmd.prompt,
        }));
    }
    JsonRpcResponse::success(id, json!({ "commands": rows }))
}

/// `agent/models` — current pair for the session + what's switchable.
/// Params: `{sessionId?}`. Model lists are the provider's live catalogue
/// when reachable, falling back to the supported set (both seams the TUI
/// provider selector already uses).
pub async fn handle_models(
    id: Value,
    params: Value,
    agent: Arc<AgentService>,
    _ctx: ServiceContext,
) -> JsonRpcResponse {
    let current = match opt_session_id(&params) {
        Some(sid) => {
            let provider = agent.provider_name_for_session(sid);
            let model = agent.provider_model_for_session(sid);
            json!({ "provider": provider, "model": model })
        }
        None => {
            let (provider, model) = crate::config::Config::load()
                .map(|c| {
                    let (p, m) = c.providers.active_provider_and_model();
                    (p, m)
                })
                .unwrap_or_else(|_| ("unknown".to_string(), "unknown".to_string()));
            json!({ "provider": provider, "model": model })
        }
    };
    let mut models = agent.fetch_models().await;
    if models.is_empty() {
        models = agent.supported_models();
    }
    let configured: Vec<Value> = crate::config::Config::load()
        .map(|c| {
            // Declared = a config section exists (is_declared), mirroring
            // the guard direct_model_switch applies. The config registry
            // walker itself is pub(crate); the canonical id/name pairs
            // live in utils::providers::KNOWN_PROVIDERS.
            crate::utils::providers::KNOWN_PROVIDERS
                .iter()
                .filter(|meta| c.providers.is_declared(meta.id))
                .map(|meta| json!({ "id": meta.id, "name": meta.display_name }))
                .collect()
        })
        .unwrap_or_default();
    JsonRpcResponse::success(
        id,
        json!({
            "current": current,
            "models": models,
            "providers": configured,
        }),
    )
}

/// `agent/set-model` — switch the session's provider/model pair.
/// Params: `{sessionId, model: "provider/model"}`. Delegates to the
/// channels/TUI shared `direct_model_switch` so validation and errors
/// read identically on every surface.
pub async fn handle_set_model(
    id: Value,
    params: Value,
    agent: Arc<AgentService>,
) -> JsonRpcResponse {
    let session_id = match parse_session_id(&params) {
        Ok(s) => s,
        Err(m) => return err(id, error_codes::INVALID_PARAMS, m),
    };
    let Some(model) = params.get("model").and_then(Value::as_str) else {
        return err(id, error_codes::INVALID_PARAMS, "model is required");
    };
    if crate::utils::provider_pair::parse_pair(model.trim()).is_err() {
        return err(
            id,
            error_codes::INVALID_PARAMS,
            "model must be a provider/model pair",
        );
    }
    let reply =
        crate::channels::commands::direct_model_switch(&agent, session_id, model).await;
    let ok = !reply.starts_with('⚠');
    JsonRpcResponse::success(id, json!({ "ok": ok, "reply": reply }))
}

/// `agent/usage` — usage dashboard data as JSON.
/// Params: `{period?: "today"|"week"|"month"|"all"}`.
pub async fn handle_usage(id: Value, params: Value, ctx: ServiceContext) -> JsonRpcResponse {
    use crate::usage::data::{DashboardData, Period};
    let period = match params.get("period").and_then(Value::as_str) {
        Some("week") => Period::Week,
        Some("month") => Period::Month,
        Some("all") => Period::AllTime,
        _ => Period::Today,
    };
    match DashboardData::fetch(&ctx.pool(), period).await {
        Ok(data) => JsonRpcResponse::success(
            id,
            json!({
                "period": period.label(),
                "summary": {
                    "totalTokens": data.summary.total_tokens,
                    "totalCost": data.summary.total_cost,
                    "sessionCount": data.summary.session_count,
                    "callCount": data.summary.call_count,
                },
                "models": data.models.iter().take(10).map(|m| json!({
                    "model": m.model,
                    "tokens": m.tokens,
                    "cost": m.cost,
                    "calls": m.calls,
                })).collect::<Vec<_>>(),
                "tools": data.tools.iter().take(10).map(|t| json!({
                    "tool": t.tool_name,
                    "calls": t.call_count,
                })).collect::<Vec<_>>(),
                "daily": data.daily.iter().take(14).map(|d| json!({
                    "date": d.date,
                    "tokens": d.tokens,
                    "cost": d.cost,
                })).collect::<Vec<_>>(),
            }),
        ),
        Err(e) => err(id, error_codes::INTERNAL_ERROR, format!("usage failed: {e}")),
    }
}

/// `agent/clear-context` — the TUI `/clear`: cut context at no cost,
/// history and title stay. Params: `{sessionId}`.
pub async fn handle_clear(
    id: Value,
    params: Value,
    agent: Arc<AgentService>,
) -> JsonRpcResponse {
    let session_id = match parse_session_id(&params) {
        Ok(s) => s,
        Err(m) => return err(id, error_codes::INVALID_PARAMS, m),
    };
    match agent.clear_context(session_id).await {
        Ok(receipt) => JsonRpcResponse::success(
            id,
            json!({ "ok": true, "reply": receipt.user_line() }),
        ),
        Err(e) => err(id, error_codes::INTERNAL_ERROR, format!("clear failed: {e}")),
    }
}

/// `agent/compact` — ask the agent to compact via the exact `[SYSTEM: …]`
/// trigger the TUI sends, as a normal turn. Params: `{sessionId}`.
/// Returns the prompt for the client to send (keeps one send path).
pub async fn handle_compact(id: Value, params: Value) -> JsonRpcResponse {
    if let Err(m) = parse_session_id(&params) {
        return err(id, error_codes::INVALID_PARAMS, m);
    }
    JsonRpcResponse::success(
        id,
        json!({
            "ok": true,
            "prompt": "[SYSTEM: Compact context now. Summarize this conversation for continuity.]",
            "reply": "Compaction requested — send the returned prompt as the next message, or just keep typing; context compacts on the next turn.",
        }),
    )
}

/// `agent/plan` — enter plan mode. Params: `{sessionId}`.
pub async fn handle_plan(id: Value, params: Value) -> JsonRpcResponse {
    let session_id = match parse_session_id(&params) {
        Ok(s) => s,
        Err(m) => return err(id, error_codes::INVALID_PARAMS, m),
    };
    let reply = crate::utils::plan_mode::enter_plan_mode(session_id).await;
    JsonRpcResponse::success(id, json!({ "ok": true, "reply": reply }))
}

/// `agent/plan-show` — plan summary text. Params: `{sessionId}`.
pub async fn handle_plan_show(id: Value, params: Value) -> JsonRpcResponse {
    let session_id = match parse_session_id(&params) {
        Ok(s) => s,
        Err(m) => return err(id, error_codes::INVALID_PARAMS, m),
    };
    let reply = crate::utils::plan_mode::show_plan(session_id).await;
    JsonRpcResponse::success(id, json!({ "ok": true, "reply": reply }))
}

/// `agent/plan-execute` — approve the live plan. On success the TUI
/// dispatches a seed turn; here the seed prompt is returned for the
/// client to send (single send path, visible in the conversation).
pub async fn handle_plan_execute(
    id: Value,
    params: Value,
    agent: Arc<AgentService>,
) -> JsonRpcResponse {
    let _ = agent; // kept in the signature for parity with dispatch needs
    let session_id = match parse_session_id(&params) {
        Ok(s) => s,
        Err(m) => return err(id, error_codes::INVALID_PARAMS, m),
    };
    use crate::utils::plan_mode::ApproveOutcome;
    match crate::utils::plan_mode::try_approve(
        session_id,
        crate::tui::plan::ApprovalSource::User,
    )
    .await
    {
        ApproveOutcome::SeedTurn { prompt } => JsonRpcResponse::success(
            id,
            json!({ "ok": true, "reply": "✅ Plan approved — starting now…", "prompt": prompt }),
        ),
        ApproveOutcome::Refused(reply) => {
            JsonRpcResponse::success(id, json!({ "ok": false, "reply": reply }))
        }
    }
}

/// `agent/plan-discard` — discard the live plan (no turn is running on
/// this surface's behalf; streaming turns are cancellable client-side).
pub async fn handle_plan_discard(
    id: Value,
    params: Value,
    ctx: ServiceContext,
) -> JsonRpcResponse {
    let session_id = match parse_session_id(&params) {
        Ok(s) => s,
        Err(m) => return err(id, error_codes::INVALID_PARAMS, m),
    };
    let reply = crate::utils::plan_mode::discard(session_id, &ctx).await;
    JsonRpcResponse::success(id, json!({ "ok": true, "reply": reply }))
}

/// `agent/goal-check` — bare `/goal` (and `/goal status` etc. are prompt
/// turns): answers the usage warning locally, exactly like the TUI,
/// without spending an LLM turn. Params: `{args}`.
pub async fn handle_goal_check(id: Value, params: Value) -> JsonRpcResponse {
    let args = params.get("args").and_then(Value::as_str).unwrap_or("");
    if crate::brain::goal::is_bare(args) {
        JsonRpcResponse::success(
            id,
            json!({
                "ok": true,
                "handled": true,
                "reply": crate::brain::goal::goal_usage_warning(),
            }),
        )
    } else {
        JsonRpcResponse::success(
            id,
            json!({
                "ok": true,
                "handled": false,
                "prompt": crate::brain::goal::goal_command_prompt(args),
            }),
        )
    }
}
