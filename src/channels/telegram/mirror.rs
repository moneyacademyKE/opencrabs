//! Daemon-side mirror: sessions bound to Telegram topics can also be driven
//! from other surfaces (MonoCode over ACP, CLI one-shots, cron). Rows those
//! surfaces write never reach the topic — the daemon only posts turns IT
//! runs — so the topic goes stale (moe typed in MonoCode, the Telegram
//! thread showed nothing; 2026-10-06). This watcher polls bound sessions
//! and posts rows written while the daemon is idle for that session.
//!
//! Own-turn suppression: while the daemon runs a turn on a session, its rows
//! stream to the topic live; the watcher sleeps, then resyncs past them on
//! the busy→idle edge so they never double-post. A foreign row landing
//! inside that window is swallowed with them (rare, bounded, v1).
//!
//! Settled rule shared with the ACP mirror: an assistant row is final only
//! once `duration_secs` is stamped (or it outlives the staleness guard), so
//! a mid-turn partial never posts and the completed answer posts once.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use teloxide::types::{ChatId, MessageId, ThreadId};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::acp::watch::is_in_flight;
use crate::db::SessionBindingRepository;
use crate::db::models::Message;
use crate::services::MessageService;

use super::TelegramState;
use super::send::message_in_thread;

/// Poll period. Human-paced external turns tolerate seconds; the per-session
/// read is one indexed range query, so 3s stays cheap.
pub const POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Telegram's message cap is 4096; leave headroom for the prefix.
const MAX_BODY_CHARS: usize = 3900;
/// External user rows post as context, not content: a preview is enough.
const USER_PREVIEW_CHARS: usize = 300;
/// Rows mirrored per session per tick; the next tick continues the rest.
const BATCH_SIZE: usize = 50;

/// The postable text of an assistant row: reasoning stripped, whitespace
/// trimmed. Empty means "nothing a human should read" (reasoning-only turn).
pub fn visible_text(content: &str) -> String {
    use crate::tui::app::reasoning_split::{Segment, split_segments};
    let mut out = String::new();
    for segment in split_segments(content) {
        if let Segment::Text(t) = segment {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(t.trim());
        }
    }
    out.trim().to_string()
}

/// Unicode-safe truncation with an ellipsis marker.
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("\n…");
    out
}

/// An external user row posts as a one-line context marker.
pub fn format_user_row(content: &str) -> Option<String> {
    let text = content.trim();
    if text.is_empty() {
        return None;
    }
    Some(format!("🖥 {}", truncate(text, USER_PREVIEW_CHARS)))
}

/// An external assistant row posts as the session's own voice (reasoning
/// already lives behind the TUI's fold; the topic gets the answer).
pub fn format_assistant_row(content: &str) -> Option<String> {
    let text = visible_text(content);
    if text.is_empty() {
        return None;
    }
    Some(truncate(&text, MAX_BODY_CHARS))
}

/// The pure selection core: given rows already past `watermark` (the
/// incremental DB read does that filtering), return the post bodies in
/// order and the new watermark. The window stops before the first in-flight
/// assistant row so a mid-turn partial never posts and the completed answer
/// arrives exactly once, in order. Tool/system rows stay off the topic but
/// still advance the watermark.
pub fn select_posts(rows: &[Message], watermark: i32, now: DateTime<Utc>) -> (Vec<String>, i32) {
    let upto = rows
        .iter()
        .position(|m| is_in_flight(m, now))
        .unwrap_or(rows.len());
    let settled = &rows[..upto];
    let mut bodies = Vec::new();
    for m in settled {
        let body = match m.role.as_str() {
            "user" => format_user_row(&m.content),
            "assistant" => format_assistant_row(&m.content),
            _ => None,
        };
        if let Some(b) = body {
            bodies.push(b);
        }
    }
    let new_mark = settled.last().map_or(watermark, |m| m.sequence);
    (bodies, new_mark)
}

/// The watcher loop. One task, all telegram-bound sessions: the binding set
/// is re-read every tick so re-binds take effect without a restart. First
/// sight of a session seeds the watermark at the current tail — the mirror
/// is for NEW activity, never a history dump (restart-safe by construction).
pub async fn run_telegram_mirror(
    state: Arc<TelegramState>,
    messages: MessageService,
    bindings: SessionBindingRepository,
    cancel: CancellationToken,
) {
    let mut watermarks: HashMap<Uuid, i32> = HashMap::new();
    let mut busy: HashSet<Uuid> = HashSet::new();
    let mut tick = tokio::time::interval(POLL_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tick.tick() => {}
        }
        let Some(bot) = state.bot().await else {
            continue;
        };
        let Ok(rows) = bindings.all_for_channel("telegram").await else {
            continue;
        };
        for b in rows {
            let (Ok(sid), Ok(chat)) = (Uuid::parse_str(&b.session_id), b.chat_id.parse::<i64>())
            else {
                continue;
            };
            if state.is_turn_active(sid) {
                busy.insert(sid); // daemon's own turn streams to the topic live
                continue;
            }
            if busy.remove(&sid) {
                // Busy→idle edge: resync past the daemon's own rows so they
                // never double-post.
                if let Ok(Some(last)) = messages.get_last_message(sid).await {
                    watermarks
                        .entry(sid)
                        .and_modify(|w| *w = (*w).max(last.sequence))
                        .or_insert(last.sequence);
                }
                continue;
            }
            let Some(&watermark) = watermarks.get(&sid) else {
                let seed = messages
                    .get_last_message(sid)
                    .await
                    .ok()
                    .flatten()
                    .map_or(0, |m| m.sequence);
                watermarks.insert(sid, seed);
                continue;
            };
            let Ok(fresh) = messages
                .list_messages_after_sequence(sid, watermark, BATCH_SIZE)
                .await
            else {
                continue;
            };
            let (posts, new_mark) = select_posts(&fresh, watermark, Utc::now());
            for body in posts {
                let thread = b.thread_id.map(|t| ThreadId(MessageId(t)));
                if let Err(e) = message_in_thread(&bot, ChatId(chat), thread, body).await {
                    tracing::debug!("telegram mirror send failed for {sid}: {e}");
                }
            }
            watermarks.insert(sid, new_mark);
        }
    }
}
