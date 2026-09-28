//! The prompt bridge: one `session/prompt` request drives one full tool-loop
//! turn, translating in both directions —
//!
//! - loop `ProgressEvent`s -> outbound `session/update` notifications
//!   (message chunks, thought chunks, tool-call lifecycle, usage);
//! - approval-gated tools -> outbound `session/request_permission` calls,
//!   whose client answer becomes the loop's `(approved, always)` pair;
//! - `session/cancel` -> the turn's `CancellationToken`, surfacing as
//!   `stopReason: "cancelled"`.
//!
//! Constraint that shapes the code: `ProgressCallback` is a synchronous
//! `Fn`, so the tool-call-id pairing map is a plain `std::sync::Mutex` —
//! held for microseconds, never across an await. `ApprovalCallback` returns
//! a future, so the permission round-trip is free to await the client.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex as StdMutex};

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::brain::agent::{ApprovalCallback, ProgressCallback, ProgressEvent, ToolApprovalInfo};
use crate::brain::provider::StopReason;

use super::protocol::{self, permission_options, permission_outcome, session_update, text_chunk};
use super::server::{ServerState, SessionState};
use super::transport::TransportHandle;

/// Run one turn to completion and answer the `session/prompt` request.
/// The cancel token is pre-claimed by `session_prompt` under the session's
/// active_cancel lock; run_turn only consumes and releases it.
pub async fn run_turn(
    state: Arc<ServerState>,
    session: Arc<SessionState>,
    request_id: Value,
    text: String,
    cancel: CancellationToken,
) {
    let acp_session_id = session.id.to_string();

    // Shared between the progress and approval callbacks: a permission ask
    // pre-mints the id its tool call will adopt, so the ask and the gated
    // tool render as one transcript entry instead of two.
    let pairing = Arc::new(CallIdPairing::new());
    let progress = progress_callback(
        state.handle.clone(),
        acp_session_id.clone(),
        state.agent.clone(),
        pairing.clone(),
    );
    let mode = *session.mode.lock().await;
    let approval = approval_callback(
        state.handle.clone(),
        acp_session_id,
        mode,
        cancel.clone(),
        pairing,
    );

    let model = session.model.lock().await.clone();
    let result = state
        .agent
        .send_message_with_tools_and_callback(
            session.id,
            text,
            model,
            Some(cancel.clone()),
            Some(approval),
            Some(progress),
            "acp",
            None,
            None,
        )
        .await;

    *session.active_cancel.lock().await = None;

    match result {
        Ok(response) => {
            let stop = if cancel.is_cancelled() {
                "cancelled"
            } else {
                stop_reason(response.stop_reason)
            };
            state
                .handle
                .respond(request_id, json!({ "stopReason": stop }));
        }
        Err(_) if cancel.is_cancelled() => {
            // A cancelled turn may surface as an error from the loop; the
            // client only cares that the turn ended by cancel.
            state
                .handle
                .respond(request_id, json!({ "stopReason": "cancelled" }));
        }
        Err(e) => {
            tracing::warn!("acp turn failed: {e:#}");
            state.handle.respond_error(
                request_id,
                protocol::INTERNAL_ERROR,
                crate::brain::agent::format_user_error(&e),
            );
        }
    }
}

/// ACP stop reasons from the provider's.
pub(crate) fn stop_reason(reason: Option<StopReason>) -> &'static str {
    match reason {
        Some(StopReason::MaxTokens) => "max_tokens",
        // StopSequence: the model hit a stop token — a natural end. Kept
        // explicit because "stop_sequence" is NOT an ACP stopReason value.
        Some(StopReason::StopSequence) => "end_turn",
        // EndTurn/ToolUse/None: the turn completed — ToolUse means the loop
        // ended after tool execution, which from the client's seat is a
        // finished turn.
        _ => "end_turn",
    }
}

/// Whether an `IntermediateText` round aggregate repeats the text live
/// streaming already delivered this round. Whitespace is ignored on both
/// sides: providers disagree about trailing newlines between the delta
/// stream and the round summary. An empty aggregate is never a duplicate —
/// emitting it would only add noise.
pub(crate) fn round_text_is_duplicate(streamed: &str, aggregate: &str) -> bool {
    !aggregate.trim().is_empty() && streamed.trim() == aggregate.trim()
}

/// Pairing state shared by the progress and approval callbacks. ACP clients
/// merge transcript blocks by `toolCallId`, so a permission request and the
/// tool call it gates must carry the SAME id — minting a fresh id per ask is
/// what made every gated tool render as two transcript rows (round 7
/// dogfood: tool card + orphaned approval block).
///
/// The ask fires before the tool starts (approval precedes execution), so
/// the flow is: `mint_for_ask` when the client is asked; on approval the
/// tool's `ToolStarted` adopts the pending id via `consume_for_start`; a
/// deny retracts it so the next same-name call does not adopt a dead id;
/// `ToolCompleted` pops the started id FIFO. FIFO per tool name is the only
/// honest pairing the events support.
pub(crate) struct CallIdPairing {
    pending: StdMutex<HashMap<String, VecDeque<String>>>,
    open: StdMutex<HashMap<String, VecDeque<String>>>,
}

impl Default for CallIdPairing {
    fn default() -> Self {
        Self::new()
    }
}

impl CallIdPairing {
    pub(crate) fn new() -> Self {
        Self {
            pending: StdMutex::new(HashMap::new()),
            open: StdMutex::new(HashMap::new()),
        }
    }

    pub(crate) fn mint_for_ask(&self, tool: &str) -> String {
        let id = Uuid::new_v4().to_string();
        if let Ok(mut pending) = self.pending.lock() {
            pending
                .entry(tool.to_string())
                .or_default()
                .push_back(id.clone());
        }
        id
    }

    /// A denied or failed ask: the tool never starts, so its pre-minted id
    /// must not leak into the next same-name call.
    pub(crate) fn retract_for_deny(&self, tool: &str) {
        if let Ok(mut pending) = self.pending.lock() {
            if let Some(queue) = pending.get_mut(tool) {
                queue.pop_back();
            }
        }
    }

    /// `ToolStarted` adopts a pending permission id when one exists (the
    /// gated case — the client already rendered the ask under that id),
    /// else mints fresh.
    pub(crate) fn consume_for_start(&self, tool: &str) -> String {
        let adopted = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.get_mut(tool).and_then(VecDeque::pop_front));
        let id = adopted.unwrap_or_else(|| Uuid::new_v4().to_string());
        if let Ok(mut open) = self.open.lock() {
            open.entry(tool.to_string())
                .or_default()
                .push_back(id.clone());
        }
        id
    }

    /// `ToolCompleted` pops the started id, FIFO per tool name.
    pub(crate) fn complete(&self, tool: &str) -> String {
        self.open
            .lock()
            .ok()
            .and_then(|mut open| open.get_mut(tool).and_then(VecDeque::pop_front))
            .unwrap_or_else(|| Uuid::new_v4().to_string())
    }
}

/// Map loop progress to `session/update` notifications.
///
/// Tool-call ids are invented here: `ProgressEvent` carries tool names but
/// no call ids, so each `ToolStarted` mints a UUID and the matching
/// `ToolCompleted` pops it FIFO per tool name. Parallel same-name tools pair
/// in start order, which is the only honest pairing the events support.
fn progress_callback(
    handle: TransportHandle,
    acp_session_id: String,
    agent: Arc<crate::brain::agent::AgentService>,
    pairing: Arc<CallIdPairing>,
) -> ProgressCallback {
    // Text streamed since the last round boundary. The loop emits live
    // deltas via `StreamingChunk` and then the round's full text again via
    // `IntermediateText` — both map to `agent_message_chunk`, so without a
    // guard every streamed answer renders twice on the client. Same rule as
    // the WhatsApp handler's pre-send dedup: skip the aggregate when it
    // repeats what streaming already delivered; emit it when it carries
    // text the stream did not (CLI providers stream nothing).
    let streamed: Arc<StdMutex<String>> = Arc::new(StdMutex::new(String::new()));

    Arc::new(move |session_id, event| {
        let update = match event {
            ProgressEvent::StreamingChunk { text } => {
                if let Ok(mut buf) = streamed.lock() {
                    buf.push_str(&text);
                }
                Some(text_chunk("agent_message_chunk", &text))
            }
            ProgressEvent::IntermediateText { text, .. } => {
                let duplicate = streamed
                    .lock()
                    .map(|mut buf| {
                        let dup = round_text_is_duplicate(&buf, &text);
                        buf.clear();
                        dup
                    })
                    .unwrap_or(false);
                if duplicate {
                    None
                } else {
                    Some(text_chunk("agent_message_chunk", &text))
                }
            }
            ProgressEvent::ReasoningChunk { text } => {
                Some(text_chunk("agent_thought_chunk", &text))
            }
            ProgressEvent::ToolStarted {
                tool_name,
                tool_input,
            } => {
                // A tool call ends the text round; round 2's stream starts
                // fresh so its aggregate dedups against its own chunks.
                if let Ok(mut buf) = streamed.lock() {
                    buf.clear();
                }
                let call_id = pairing.consume_for_start(&tool_name);
                Some(json!({
                    "sessionUpdate": "tool_call",
                    "toolCallId": call_id,
                    "title": tool_name,
                    "kind": protocol::tool_kind(&tool_name),
                    "status": "in_progress",
                    "rawInput": tool_input,
                }))
            }
            ProgressEvent::ToolCompleted {
                tool_name,
                success,
                summary,
                ..
            } => {
                let call_id = pairing.complete(&tool_name);
                Some(json!({
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": call_id,
                    "status": if success { "completed" } else { "failed" },
                    "rawOutput": summary,
                }))
            }
            ProgressEvent::TokenCount(used) => Some(json!({
                "sessionUpdate": "usage",
                "usage": {
                    "used": used,
                    "size": agent.context_limit_for_session(session_id),
                },
            })),
            // Everything else (compaction notices, retry ticker, provider
            // switches, suggestions, stream strips) has no ACP vocabulary —
            // logged by the loop already, not re-broadcast here.
            _ => None,
        };

        if let Some(update) = update {
            handle.send(session_update(&acp_session_id, update));
        }
    })
}

/// Backstop for a client that drops `session/request_permission` without
/// answering: generous enough for a human reading the prompt, finite enough
/// that a wedged client cannot stall the agent forever.
const PERMISSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Route approval-gated tools according to the session's mode. Only
/// `supervised` (and non-edit kinds under `auto-accept-edits`) reach the
/// client as `session/request_permission`; `plan` denies mutations outright
/// so the agent learns the boundary from the denial instead of a silent
/// client-side veto. Any transport failure denies — a client that cannot
/// answer must never become an approval. The round-trip also races
/// `session/cancel` and the permission timeout, both resolving to a deny.
fn approval_callback(
    handle: TransportHandle,
    acp_session_id: String,
    mode: protocol::AcpMode,
    cancel: CancellationToken,
    pairing: Arc<CallIdPairing>,
) -> ApprovalCallback {
    Arc::new(move |info: ToolApprovalInfo| {
        let handle = handle.clone();
        let acp_session_id = acp_session_id.clone();
        let cancel = cancel.clone();
        let pairing = pairing.clone();
        Box::pin(async move {
            let kind = protocol::tool_kind(&info.tool_name);
            match mode {
                protocol::AcpMode::Plan => {
                    tracing::info!("acp plan mode: denied {} without asking", info.tool_name);
                    return Ok((false, false));
                }
                protocol::AcpMode::Auto | protocol::AcpMode::FullAccess => {
                    return Ok((true, false));
                }
                protocol::AcpMode::AutoAcceptEdits if kind == "edit" => {
                    return Ok((true, false));
                }
                _ => {}
            }
            // Pre-mint the id the gated tool call will adopt on approval, so
            // the permission ask and the tool share one transcript entry
            // (clients merge blocks by toolCallId).
            let tool_name = info.tool_name.clone();
            let call_id = pairing.mint_for_ask(&tool_name);
            let params = json!({
                "sessionId": acp_session_id,
                "toolCall": {
                    "toolCallId": call_id,
                    "title": tool_name.clone(),
                    "kind": kind,
                    "rawInput": info.tool_input,
                },
                "options": permission_options(),
            });
            let call = handle.call(protocol::SESSION_REQUEST_PERMISSION, params);
            let outcome = tokio::select! {
                res = call => res,
                _ = cancel.cancelled() => {
                    Err(anyhow::anyhow!("turn cancelled before the client answered"))
                }
                _ = tokio::time::sleep(PERMISSION_TIMEOUT) => {
                    Err(anyhow::anyhow!(
                        "no permission response within {PERMISSION_TIMEOUT:?}, denying"
                    ))
                }
            };
            match outcome {
                Ok(result) => {
                    let (approved, remember) = permission_outcome(&result);
                    // A denied tool never starts: retract the pre-minted id
                    // so the next same-name call does not adopt it.
                    if !approved {
                        pairing.retract_for_deny(&tool_name);
                    }
                    Ok((approved, remember))
                }
                Err(e) => {
                    pairing.retract_for_deny(&tool_name);
                    tracing::warn!("acp permission request failed, denying: {e:#}");
                    Ok((false, false))
                }
            }
        })
    })
}
