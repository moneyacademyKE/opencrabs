//! Sessions over the gateway: list / create / history / rename / delete.
//!
//! Phase A of Mini App ↔ TUI parity (spec:
//! `research/miniapp-parity-spec-2026-11-03.md`). The TUI's conversation
//! world rides `SessionService` + `MessageService`; these handlers expose
//! the same data to the Mini App client instead of inventing a parallel
//! store. A *session* here is the conversation itself; an A2A *task* is
//! one turn inside it, bound through `a2a_context_sessions` (#1159).
//!
//! Wire shapes are plain JSON objects (id/title/model/timestamps) — the
//! client never sees a `Session` row verbatim, and history content is
//! truncated at `HISTORY_MAX_CHARS` per message so one huge tool dump
//! cannot blow the WebView payload.

use serde_json::{Value, json};
use uuid::Uuid;

use crate::a2a::types::{JsonRpcResponse, error_codes};
use crate::db::repository::SessionListOptions;
use crate::services::{MessageService, ServiceContext, SessionService};

/// Default page size for `sessions/list` when the caller doesn't say.
const LIST_LIMIT: usize = 50;
/// Hard ceiling on one list page — a phone UI never pages past this.
const LIST_MAX: usize = 100;
/// Max messages returned by `sessions/history` (most recent, ascending).
const HISTORY_TAIL: usize = 100;
/// Per-message content ceiling in history replies.
const HISTORY_MAX_CHARS: usize = 4_000;

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

/// One session as the client renders it in the conversation list.
fn session_json(s: &crate::db::models::Session) -> Value {
    json!({
        "id": s.id.to_string(),
        "title": s.title,
        "model": s.model,
        "provider": s.provider_name,
        "createdAt": s.created_at.to_rfc3339(),
        "updatedAt": s.updated_at.to_rfc3339(),
        "tokenCount": s.token_count,
        "totalCost": s.total_cost,
        "archived": s.archived_at.is_some(),
    })
}

/// `sessions/list` — recent conversations, newest first.
/// Params: `{limit?, query?, includeArchived?}`.
pub async fn handle_list(id: Value, params: Value, ctx: ServiceContext) -> JsonRpcResponse {
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| (n as usize).clamp(1, LIST_MAX))
        .unwrap_or(LIST_LIMIT);
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .map(str::to_string);
    let include_archived = params
        .get("includeArchived")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let svc = SessionService::new(ctx);
    match svc
        .list_sessions(SessionListOptions {
            include_archived,
            limit: Some(limit),
            offset: 0,
            query,
            include_subagents: false,
        })
        .await
    {
        Ok(sessions) => {
            let items: Vec<Value> = sessions.iter().map(session_json).collect();
            JsonRpcResponse::success(id, json!({ "sessions": items }))
        }
        Err(e) => err(id, error_codes::INTERNAL_ERROR, format!("list failed: {e}")),
    }
}

/// `sessions/create` — a fresh conversation. Params: `{title?}`.
/// Returns the session plus a fresh A2A `contextId` bound to it, so the
/// client's next `message/send` continues *this* conversation (#1159).
pub async fn handle_create(id: Value, params: Value, ctx: ServiceContext) -> JsonRpcResponse {
    let title = params
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|t| !t.trim().is_empty());
    let pool = ctx.pool();
    let svc = SessionService::new(ctx);
    match svc.create_session(title).await {
        Ok(session) => {
            let context_id = Uuid::new_v4().to_string();
            crate::a2a::persistence::save_context_session(&pool, &context_id, session.id).await;
            let mut body = session_json(&session);
            body["contextId"] = json!(context_id);
            JsonRpcResponse::success(id, body)
        }
        Err(e) => err(
            id,
            error_codes::INTERNAL_ERROR,
            format!("create failed: {e}"),
        ),
    }
}

/// `sessions/history` — the tail of a conversation, oldest→newest.
/// Params: `{sessionId, limit?}`. Missing session answers TASK_NOT_FOUND
/// (the gateway's one "not found" code) rather than an empty list, so the
/// client can tell "deleted" apart from "quiet".
pub async fn handle_history(id: Value, params: Value, ctx: ServiceContext) -> JsonRpcResponse {
    let session_id = match parse_session_id(&params) {
        Ok(s) => s,
        Err(m) => return err(id, error_codes::INVALID_PARAMS, m),
    };
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| (n as usize).clamp(1, HISTORY_TAIL))
        .unwrap_or(HISTORY_TAIL);

    let sessions = SessionService::new(ctx.clone());
    match sessions.get_session(session_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return err(
                id,
                error_codes::TASK_NOT_FOUND,
                format!("Session not found: {session_id}"),
            );
        }
        Err(e) => {
            return err(id, error_codes::INTERNAL_ERROR, format!("lookup failed: {e}"));
        }
    }

    let messages = MessageService::new(ctx);
    match messages.list_messages_for_session(session_id).await {
        Ok(all) => {
            let tail = &all[all.len().saturating_sub(limit)..];
            let items: Vec<Value> = tail
                .iter()
                .map(|m| {
                    json!({
                        "id": m.id.to_string(),
                        "role": m.role,
                        "content": truncate(&m.content, HISTORY_MAX_CHARS),
                        "createdAt": m.created_at.to_rfc3339(),
                        "tokenCount": m.token_count,
                    })
                })
                .collect();
            JsonRpcResponse::success(id, json!({ "messages": items }))
        }
        Err(e) => err(
            id,
            error_codes::INTERNAL_ERROR,
            format!("history failed: {e}"),
        ),
    }
}

/// `sessions/rename` — set (or clear) a conversation title.
/// Params: `{sessionId, title}` (`null`/empty clears).
pub async fn handle_rename(id: Value, params: Value, ctx: ServiceContext) -> JsonRpcResponse {
    let session_id = match parse_session_id(&params) {
        Ok(s) => s,
        Err(m) => return err(id, error_codes::INVALID_PARAMS, m),
    };
    let title = params
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|t| !t.trim().is_empty());

    let svc = SessionService::new(ctx);
    if svc.get_session(session_id).await.ok().flatten().is_none() {
        return err(
            id,
            error_codes::TASK_NOT_FOUND,
            format!("Session not found: {session_id}"),
        );
    }
    match svc.update_session_title(session_id, title).await {
        Ok(()) => JsonRpcResponse::success(id, json!({ "ok": true })),
        Err(e) => err(
            id,
            error_codes::INTERNAL_ERROR,
            format!("rename failed: {e}"),
        ),
    }
}

/// `sessions/delete` — archive, not hard-delete: the TUI can still find
/// the conversation, credentials stay honest, and a mistaken phone tap is
/// undoable from the host. Params: `{sessionId}`.
pub async fn handle_delete(id: Value, params: Value, ctx: ServiceContext) -> JsonRpcResponse {
    let session_id = match parse_session_id(&params) {
        Ok(s) => s,
        Err(m) => return err(id, error_codes::INVALID_PARAMS, m),
    };
    let svc = SessionService::new(ctx);
    if svc.get_session(session_id).await.ok().flatten().is_none() {
        return err(
            id,
            error_codes::TASK_NOT_FOUND,
            format!("Session not found: {session_id}"),
        );
    }
    match svc.archive_session(session_id).await {
        Ok(()) => JsonRpcResponse::success(id, json!({ "ok": true })),
        Err(e) => err(
            id,
            error_codes::INTERNAL_ERROR,
            format!("delete failed: {e}"),
        ),
    }
}

/// Char-safe truncation with an ellipsis marker.
fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let cut: String = s.chars().take(max_chars).collect();
    format!("{cut}…")
}
