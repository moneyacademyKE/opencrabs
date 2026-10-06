//! The mirror core: watermark arithmetic and replay-shape reuse. The poller
//! loop's DB/timing behavior is proven on the wire (two-process probe), not
//! mocked here — these tests pin the pure part.

use crate::acp::watch::{mirror_updates, settled_watermark};
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

/// A row whose turn completed: the store stamps `duration_secs` at turn end.
fn done(role: &str, content: &str, seq: i32, thinking: Option<&str>) -> Message {
    let mut m = msg(role, content, seq, thinking);
    m.duration_secs = Some(1);
    m
}

/// A row from before the duration stamp existed, or from a cancelled turn:
/// never stamped, so it settles by age via the staleness guard.
fn old_undated(content: &str, seq: i32) -> Message {
    let mut m = msg("assistant", content, seq, None);
    m.created_at = chrono::Utc::now() - chrono::Duration::hours(1);
    m
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
        done("assistant", "daemon replied", 3, None),
    ];
    let (updates, mark) = mirror_updates(&history, 1);
    assert_eq!(
        kinds(&updates),
        vec!["user_message_chunk", "agent_message_chunk"]
    );
    assert_eq!(updates[0]["content"]["text"], "from telegram");
    assert_eq!(updates[1]["content"]["text"], "daemon replied");
    assert_eq!(mark, 3);
}

#[test]
fn mirror_drops_thinking_ships_final_text_only() {
    let history = vec![done("assistant", "answer", 5, Some("pondering"))];
    let (updates, mark) = mirror_updates(&history, 4);
    assert_eq!(kinds(&updates), vec!["agent_message_chunk"]);
    assert_eq!(mark, 5);
}

#[test]
fn blank_rows_advance_the_watermark_without_chunks() {
    let history = vec![msg("user", "", 8, None), done("assistant", "real", 9, None)];
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

#[test]
fn probe_assistant_row_with_inline_reasoning_mirrors() {
    // The EXACT bytes the wire probe persisted (session 2f3f3a3c): a
    // well-formed reasoning block plus visible text. The live mirror sent
    // only the user chunk for this pair (trace: `emit updates=1 wm 0->2`),
    // so this test pins where the loss happens: pure mapper or live path.
    let content = "<!-- reasoning -->\nThe user wants me to reply with exactly: MIRROR-OK 247518\n\nSimple. Just reply exactly that.\n<!-- /reasoning -->\n\nMIRROR-OK 247518\n\n";
    let history = vec![
        msg("user", "Reply with exactly: MIRROR-OK 247518", 1, None),
        done("assistant", content, 2, None),
    ];
    let (updates, mark) = mirror_updates(&history, 0);
    println!("UPDATES={} kinds={:?}", updates.len(), kinds(&updates));
    assert_eq!(mark, 2);
    assert!(
        updates.len() >= 2,
        "assistant row produced no updates: {updates:?}"
    );
    assert!(kinds(&updates).contains(&"agent_message_chunk"));
}

#[test]
fn persisted_reasoning_row_mirrors_as_thought_plus_message() {
    // Regression (mirror probe, session 2f3f3a3c): a REAL persisted assistant
    // row — reasoning block at position 0, answer trailing — crossed the wire
    // as zero chunks while the user row beside it mirrored fine. The exact
    // bytes from the database must mirror as thought + message.
    let rows = vec![
        msg("user", "Reply with exactly: MIRROR-OK 247518", 1, None),
        done(
            "assistant",
            "<!-- reasoning -->\nThe user wants me to reply with exactly: MIRROR-OK 247518\n\nSimple. Just reply exactly that.\n<!-- /reasoning -->\n\nMIRROR-OK 247518\n\n",
            2,
            None,
        ),
    ];
    let (updates, mark) = mirror_updates(&rows, 0);
    assert_eq!(mark, 2);
    let kinds: Vec<&str> = updates
        .iter()
        .map(|u| u["sessionUpdate"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["user_message_chunk", "agent_message_chunk"]);
    assert!(
        updates[1]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("MIRROR-OK")
    );
}

#[test]
fn in_flight_assistant_row_is_held_until_filled() {
    // The store inserts the assistant row EMPTY at turn start and fills it
    // IN PLACE at turn end. Mirroring past it early burns the watermark and
    // loses the answer forever (session 2f3f3a3c: one emit, then silence).
    let (updates, mark) = mirror_updates(
        &[
            msg("user", "do the thing", 1, None),
            msg("assistant", "", 2, None),
        ],
        0,
    );
    assert_eq!(updates.len(), 1, "only the user row may mirror mid-turn");
    assert_eq!(mark, 1, "watermark must stop before the unfinished row");

    // Turn end fills the content AND stamps the duration.
    let mut filled = msg("assistant", "", 2, None);
    filled.content = "<!-- reasoning -->\nthink\n<!-- /reasoning -->\n\nthe answer".to_string();
    filled.duration_secs = Some(9);
    let (updates, mark) = mirror_updates(&[msg("user", "do the thing", 1, None), filled], 1);
    let kinds: Vec<&str> = updates
        .iter()
        .map(|u| u["sessionUpdate"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["agent_message_chunk"]);
    assert_eq!(mark, 2);
}

#[test]
fn completed_empty_assistant_row_advances_the_watermark() {
    // A turn that produced no visible text still FINISHES: duration_secs is
    // stamped with the final content, marking the row settled.
    let mut done = msg("assistant", "", 2, None);
    done.duration_secs = Some(7);
    let (updates, mark) = mirror_updates(&[msg("user", "hi", 1, None), done], 0);
    assert_eq!(updates.len(), 1);
    assert_eq!(mark, 2);
}

#[test]
fn rows_behind_an_in_flight_one_wait_their_turn() {
    // A queued foreign turn must not jump the unfinished row, or it emits
    // twice when the window catches up.
    let history = vec![
        msg("user", "first", 1, None),
        msg("assistant", "", 2, None),
        msg("user", "second", 3, None),
    ];
    let (updates, mark) = mirror_updates(&history, 0);
    assert_eq!(updates.len(), 1);
    assert_eq!(mark, 1);
    let (updates, _) = mirror_updates(&history, 1);
    assert!(
        updates.is_empty(),
        "rows after an in-flight row stay pending"
    );
}

#[test]
fn settled_watermark_skips_a_trailing_in_flight_row() {
    // Load-time seeding mid-turn: the answer still arrives via the mirror
    // once the writer finishes.
    let history = vec![msg("user", "first", 1, None), msg("assistant", "", 2, None)];
    assert_eq!(settled_watermark(&history), 1);
    let mut done = msg("assistant", "", 2, None);
    done.duration_secs = Some(3);
    let settled = vec![msg("user", "first", 1, None), done];
    assert_eq!(settled_watermark(&settled), 2);
    assert_eq!(settled_watermark(&[]), 0);
}

#[test]
fn streamed_partial_content_is_held_until_turn_end() {
    // The digest incident: Telegram's assistant row carries REAL content
    // mid-turn (the store streams appends in place) while duration_secs
    // stays NULL. Shipping it early burned the watermark and the tail —
    // the actual digest — never reached MonoCode.
    let (updates, mark) = mirror_updates(
        &[
            msg("user", "give me the digest", 1, None),
            msg("assistant", "On it: pulling fresh headlines now.", 2, None),
        ],
        0,
    );
    assert_eq!(
        kinds(&updates),
        vec!["user_message_chunk"],
        "mid-turn partial content must not mirror"
    );
    assert_eq!(mark, 1);

    let mut finished = msg("assistant", "", 2, None);
    finished.content =
        "On it: pulling fresh headlines now.\n\n• headline one\n• headline two".to_string();
    finished.duration_secs = Some(42);
    let (updates, mark) =
        mirror_updates(&[msg("user", "give me the digest", 1, None), finished], 1);
    assert_eq!(kinds(&updates), vec!["agent_message_chunk"]);
    assert!(
        updates[0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("headline two"),
        "the final content arrives whole, exactly once"
    );
    assert_eq!(mark, 2);
}

#[test]
fn legacy_undated_row_is_settled_by_age() {
    // Rows written before duration_secs existed (and rows from cancelled
    // turns) never receive the stamp; the staleness guard settles them so
    // they mirror instead of pinning the window forever.
    let old = old_undated("an old completed answer", 2);
    let (updates, mark) = mirror_updates(&[msg("user", "q", 1, None), old], 0);
    assert_eq!(
        kinds(&updates),
        vec!["user_message_chunk", "agent_message_chunk"]
    );
    assert_eq!(mark, 2);
}
