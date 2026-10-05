//! The mirror core: watermark arithmetic and replay-shape reuse. The poller
//! loop's DB/timing behavior is proven on the wire (two-process probe), not
//! mocked here — these tests pin the pure part.

use crate::acp::watch::mirror_updates;
use crate::db::models::Message;
use serde_json::Value;

fn msg(role: &str, content: &str, seq: i32, thinking: Option<&str>) -> Message {
    Message {
        id: uuid::Uuid::new_v4(),
        session_id: uuid::Uuid::new_v4(),
        role: role.to_string(),
        content: content.to_string(),
        sequence: seq,
        created_at: chrono::Utc::now(),
        token_count: None,
        cost: None,
        input_tokens: None,
        cache_creation_tokens: None,
        cache_read_tokens: None,
        thinking: thinking.map(str::to_string),
        duration_secs: None,
    }
}

fn kinds(updates: &[Value]) -> Vec<&str> {
    updates
        .iter()
        .map(|u| u["sessionUpdate"].as_str().unwrap())
        .collect()
}

#[test]
fn empty_history_mirrors_nothing() {
    let (updates, mark) = mirror_updates(&[], 7);
    assert!(updates.is_empty());
    assert_eq!(mark, 7);
}

#[test]
fn rows_at_or_below_the_watermark_are_silent() {
    let history = vec![
        msg("user", "old", 1, None),
        msg("assistant", "stale", 2, None),
    ];
    let (updates, mark) = mirror_updates(&history, 2);
    assert!(updates.is_empty(), "already-seen rows must not re-emit");
    assert_eq!(mark, 2);
}

#[test]
fn fresh_suffix_mirrors_through_the_replay_shapes() {
    let history = vec![
        msg("user", "seen", 1, None),
        msg("user", "from telegram", 2, None),
        msg("assistant", "daemon replied", 3, None),
    ];
    let (updates, mark) = mirror_updates(&history, 1);
    assert_eq!(kinds(&updates), vec!["user_message_chunk", "agent_message_chunk"]);
    assert_eq!(updates[0]["content"]["text"], "from telegram");
    assert_eq!(updates[1]["content"]["text"], "daemon replied");
    assert_eq!(mark, 3);
}

#[test]
fn assistant_thinking_rides_along() {
    let history = vec![msg("assistant", "answer", 5, Some("pondering"))];
    let (updates, mark) = mirror_updates(&history, 4);
    assert_eq!(kinds(&updates), vec!["agent_thought_chunk", "agent_message_chunk"]);
    assert_eq!(mark, 5);
}

#[test]
fn blank_rows_advance_the_watermark_without_chunks() {
    let history = vec![
        msg("user", "", 8, None),
        msg("assistant", "real", 9, None),
    ];
    let (updates, mark) = mirror_updates(&history, 7);
    assert_eq!(kinds(&updates), vec!["agent_message_chunk"]);
    assert_eq!(mark, 9, "the blank row is consumed, never re-offered");
}

#[test]
fn watermark_never_regresses_on_empty_suffix() {
    let history = vec![msg("user", "x", 3, None)];
    let (updates, mark) = mirror_updates(&history, 10);
    assert!(updates.is_empty());
    assert_eq!(mark, 10, "a stale history read must not rewind the mirror");
}
