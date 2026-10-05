//! Cross-surface message mirror: keeps an attached ACP client current with
//! conversation activity driven from ANY other surface (Telegram, TUI, cron).
//!
//! The daemon process owns the live event stream for turns it runs; an ACP
//! child cannot subscribe to it. But every surface persists its messages to
//! the same SQLite history, so the mirror polls that history and re-emits
//! rows the client has not seen — through the SAME `replay_updates` mapper
//! the load path uses. One shape for history and live, no second vocabulary.
//!
//! Granularity is completed messages (one poll period behind the writer),
//! not token streaming: token parity would need a daemon→child event bus,
//! which does not exist. That is the honest ceiling of this design.
//!
//! Echo suppression: rows written by the ACP child's OWN turn already
//! reached the client live through the turn bridge, so re-emitting them
//! would duplicate. While `active_cancel` is set the mirror sleeps; on the
//! busy→idle edge it resyncs the watermark silently, swallowing its own
//! writes. (A foreign row landing inside that window is swallowed too —
//! a rare, bounded race, accepted for v1.)

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::protocol;
use super::state::{ServerState, SessionState};
use super::transport::TransportHandle;
use crate::db::models::Message;
use crate::services::MessageService;

/// Poll period. Message-level parity tolerates seconds; the query is one
/// indexed read per watched session, so 2s is cheap and feels live.
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The pure mirror core: given the session's ordered history and the last
/// sequence the client has seen, return the updates to emit and the new
/// watermark. History is sequence-ordered (the load replay depends on it),
/// so unseen rows are a suffix — `partition_point` finds the window with
/// zero clones.
pub fn mirror_updates(history: &[Message], watermark: i32) -> (Vec<Value>, i32) {
    let idx = history.partition_point(|m| m.sequence <= watermark);
    let fresh = &history[idx..];
    let new_mark = fresh.last().map_or(watermark, |m| m.sequence);
    (protocol::replay_updates(fresh), new_mark)
}

/// Server-side entry point: spawn the mirror for a freshly loaded session,
/// seeded to the last replayed row so nothing double-emits.
pub fn spawn_mirror(
    state: &Arc<ServerState>,
    session_state: &Arc<SessionState>,
    acp_session_id: &str,
    watermark: i32,
) {
    tokio::spawn(run_message_mirror(
        state.handle.clone(),
        state.messages.clone(),
        session_state.clone(),
        acp_session_id.to_string(),
        watermark,
        session_state.watch_cancel.clone(),
    ));
}

/// The poller. Lives as long as this attachment: the server cancels via
/// `cancel` when a reload supersedes the session state (a fresh watcher
/// takes over) — exactly one mirror per attached session.
pub async fn run_message_mirror(
    handle: TransportHandle,
    messages: MessageService,
    state: Arc<SessionState>,
    acp_session_id: String,
    mut watermark: i32,
    cancel: CancellationToken,
) {
    let mut tick = tokio::time::interval(POLL_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Was the previous tick inside our own turn? Drives the silent resync.
    let mut own_turn = false;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tick.tick() => {}
        }
        let Ok(history) = messages.list_messages_for_session(state.id).await else {
            continue;
        };
        if state.active_cancel.lock().await.is_some() {
            own_turn = true; // our turn in flight: its rows are already live
            continue;
        }
        if own_turn {
            own_turn = false;
            watermark = history.last().map_or(watermark, |m| m.sequence);
            continue; // swallow our own writes; mirror from here on
        }
        let (updates, new_mark) = mirror_updates(&history, watermark);
        if new_mark != watermark {
            watermark = new_mark;
            for update in updates {
                handle.send(protocol::session_update(&acp_session_id, update));
            }
        }
    }
}
