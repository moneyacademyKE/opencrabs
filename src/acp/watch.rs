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
//! Rows are not insert-once: the store creates the assistant row at turn
//! start and STREAMS content into it in place (`content = content || ?` per
//! partial update), stamping `duration_secs` only at turn end. So "has
//! content" does not mean finished — a mirror that ships the first partial
//! update burns the watermark past the rest of the answer (moe's lost
//! digest: Telegram showed the full reply, MonoCode got only the preamble).
//! The fresh window stops before any row lacking its duration stamp (see
//! [`is_in_flight`]), and a staleness guard settles rows from cancelled or
//! crashed turns that never receive one.
//!
//! Echo suppression: rows written by the ACP child's OWN turn already
//! reached the client live through the turn bridge, so re-emitting them
//! would duplicate. While `active_cancel` is set the mirror sleeps; on the
//! busy→idle edge it resyncs the watermark silently, swallowing its own
//! writes. (A foreign row landing inside that window is swallowed too —
//! a rare, bounded race, accepted for v1.)

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
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

/// An assistant row is abandoned (cancelled turn, crashed writer) once it
/// has gone this long without a duration stamp; at that point the mirror
/// treats it as settled rather than pinning the window forever. Generous
/// on purpose: a too-small window's only cost is mirroring a long turn's
/// partial content early, and its completion then re-emits nothing.
pub const STALE_TURN_SECS: i64 = 900;

/// An assistant row whose turn is still writing. The store streams content
/// into the row in place and stamps `duration_secs` only at turn end, so a
/// row WITH content but NO duration is mid-write — mirroring it now ships
/// the preamble and burns the watermark past the rest of the answer
/// (moe's digest: Telegram got the full text, MonoCode got one sentence).
/// Rows older than the staleness guard (cancelled turns, and the legacy
/// rows written before the stamp existed) are treated as settled.
pub fn is_in_flight(m: &Message, now: DateTime<Utc>) -> bool {
    m.role == "assistant"
        && m.duration_secs.is_none()
        && (now - m.created_at).num_seconds() < STALE_TURN_SECS
}

/// The last sequence a mirror may treat as settled: history minus any
/// window starting at an in-flight assistant row. Seeds the load path and
/// bounds every poll.
pub fn settled_watermark(history: &[Message]) -> i32 {
    settled_watermark_at(history, Utc::now())
}

/// Clock-explicit variant for tests and callers that already have one.
pub fn settled_watermark_at(history: &[Message], now: DateTime<Utc>) -> i32 {
    let upto = history
        .iter()
        .position(|m| is_in_flight(m, now))
        .unwrap_or(history.len());
    history[..upto].last().map_or(0, |m| m.sequence)
}

/// The pure mirror core: given the session's ordered history and the last
/// sequence the client has seen, return the updates to emit and the new
/// watermark. History is sequence-ordered (the load replay depends on it),
/// so unseen rows are a suffix — `partition_point` finds the window with
/// zero clones. The window stops before the first in-flight assistant row:
/// rows behind an unfinished one stay pending so everything emits in
/// order, exactly once, when the writer completes.
pub fn mirror_updates(history: &[Message], watermark: i32) -> (Vec<Value>, i32) {
    mirror_updates_at(history, watermark, Utc::now())
}

/// Clock-explicit variant for tests and the daemon-side watcher.
pub fn mirror_updates_at(
    history: &[Message],
    watermark: i32,
    now: DateTime<Utc>,
) -> (Vec<Value>, i32) {
    let idx = history.partition_point(|m| m.sequence <= watermark);
    let fresh = &history[idx..];
    let upto = fresh
        .iter()
        .position(|m| is_in_flight(m, now))
        .unwrap_or(fresh.len());
    let settled = &fresh[..upto];
    let new_mark = settled.last().map_or(watermark, |m| m.sequence);
    // Mirrors ship final text only (owner directive 2026-10-06): thought
    // chunks belong to the client's own session/load replay, not to
    // cross-surface sync.
    let updates = protocol::replay_updates(settled)
        .into_iter()
        .filter(|u| u["sessionUpdate"].as_str() != Some("agent_thought_chunk"))
        .collect();
    (updates, new_mark)
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
            // Swallow our own writes (they streamed live). Settled-only, so
            // a foreign turn still in flight at this edge stays pending for
            // the next tick instead of being swallowed half-written.
            watermark = settled_watermark(&history).max(watermark);
            continue;
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
