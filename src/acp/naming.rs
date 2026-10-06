//! Session naming for ACP surfaces.
//!
//! Birth titles follow the channel convention (`Telegram: …`, `Discord: …`):
//! a surface prefix the auto-title machinery recognizes as a default, so the
//! first real turn upgrades it to a model-written title — plus enough context
//! (client name, working directory) to tell fresh sessions apart in a picker
//! until that happens.

use std::path::Path;

use serde_json::Value;

use super::state::ServerState;

/// Hard cap on client-supplied names: titles stay readable and a weird
/// client cannot mint a 10 KB session row.
const CLIENT_NAME_CAP: usize = 40;

/// The client's self-identification from `initialize` params
/// (`clientInfo.name`, falling back to `clientInfo.title`).
pub fn client_name_from_params(params: &Value) -> Option<String> {
    let info = params.get("clientInfo")?;
    let raw = info
        .get("name")
        .or_else(|| info.get("title"))
        .and_then(Value::as_str)?
        .trim();
    if raw.is_empty() {
        return None;
    }
    Some(raw.chars().take(CLIENT_NAME_CAP).collect())
}

/// Remember the client's self-identification for later birth titles.
pub fn capture_client(state: &ServerState, params: &Value) {
    if let Some(name) = client_name_from_params(params) {
        *state.client_name.lock().unwrap_or_else(|e| e.into_inner()) = Some(name);
    }
}

/// Compose a session's birth title. Shapes, richest first:
/// `ACP: <client> · <dir>`, `ACP: <client>`, `ACP: <dir>`, bare `ACP`.
/// Every shape is an auto-title default (see `is_default_channel_title`),
/// so the first turn upgrades it.
pub fn birth_title(client: Option<&str>, cwd: Option<&str>) -> String {
    let client = client.map(str::trim).filter(|s| !s.is_empty());
    let dir = cwd
        .map(Path::new)
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().trim().to_string())
        .filter(|s| !s.is_empty());
    match (client, dir) {
        (Some(c), Some(d)) => format!("ACP: {c} · {d}"),
        (Some(c), None) => format!("ACP: {c}"),
        (None, Some(d)) => format!("ACP: {d}"),
        (None, None) => "ACP".to_string(),
    }
}

/// `birth_title` with the client name captured at `initialize`.
pub fn birth_title_for(state: &ServerState, cwd: Option<&str>) -> String {
    let client = state.client_name.lock().unwrap_or_else(|e| e.into_inner());
    birth_title(client.as_deref(), cwd)
}
