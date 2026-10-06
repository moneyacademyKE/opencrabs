//! Telegram daemon-side mirror: the pure selection/formatting core.
//! The poll loop's DB/send behavior is proven live (moe's MonoCode turns
//! landing in the bound topic); these tests pin the pure part.

use crate::channels::telegram::mirror::{
    format_assistant_row, format_user_row, select_posts, visible_text,
};
use crate::db::models::Message;

fn msg(role: &str, content: &str, seq: i32) -> Message {
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
        thinking: None,
        duration_secs: None,
    }
}

/// A completed assistant row (duration stamped at turn end).
fn done(content: &str, seq: i32) -> Message {
    let mut m = msg("assistant", content, seq);
    m.duration_secs = Some(3);
    m
}

#[test]
fn visible_text_strips_reasoning_blocks() {
    let content = "<!-- reasoning -->\nprivate deliberation\n<!-- /reasoning -->\n\nthe answer";
    assert_eq!(visible_text(content), "the answer");
}

#[test]
fn visible_text_passes_plain_content_through() {
    assert_eq!(
        visible_text("just text, no markers"),
        "just text, no markers"
    );
}

#[test]
fn visible_text_of_reasoning_only_is_empty() {
    let content = "<!-- reasoning -->\nonly thoughts\n<!-- /reasoning -->";
    assert_eq!(visible_text(content), "");
}

#[test]
fn user_row_posts_as_prefixed_context() {
    let body = format_user_row("do the thing").expect("user rows post");
    assert!(body.starts_with("🖥 "), "external provenance marker: {body}");
    assert!(body.contains("do the thing"));
}

#[test]
fn user_row_blank_posts_nothing() {
    assert!(format_user_row("   ").is_none());
}

#[test]
fn assistant_row_posts_visible_text_only() {
    let content = "<!-- reasoning -->\nthinking\n<!-- /reasoning -->\n\nvisible answer";
    let body = format_assistant_row(content).expect("assistant rows post");
    assert_eq!(body, "visible answer");
}

#[test]
fn assistant_reasoning_only_posts_nothing_but_still_settles() {
    let content = "<!-- reasoning -->\nthinking\n<!-- /reasoning -->";
    assert!(format_assistant_row(content).is_none());
}

#[test]
fn long_bodies_are_truncated_char_safely() {
    let long = "x".repeat(5000);
    let body = format_assistant_row(&long).expect("long rows still post");
    assert!(body.chars().count() <= 3902, "3900 chars + ellipsis marker");
    assert!(body.ends_with('…'));
}

#[test]
fn select_posts_emits_settled_rows_in_order() {
    let now = chrono::Utc::now();
    let rows = vec![msg("user", "external prompt", 5), done("the reply", 6)];
    let (bodies, mark) = select_posts(&rows, 4, now);
    assert_eq!(bodies.len(), 2);
    assert!(bodies[0].starts_with("🖥 "));
    assert_eq!(bodies[1], "the reply");
    assert_eq!(mark, 6);
}

#[test]
fn select_posts_holds_mid_turn_partial_rows() {
    // The digest incident's other direction: an external writer's assistant
    // row streams content with duration NULL — nothing posts until stamped.
    let now = chrono::Utc::now();
    let rows = vec![
        msg("user", "from monocode", 2),
        msg("assistant", "partial preamble", 3),
    ];
    let (bodies, mark) = select_posts(&rows, 1, now);
    assert_eq!(bodies.len(), 1, "only the user row may post mid-turn");
    assert_eq!(mark, 2, "watermark stops before the unfinished row");
}

#[test]
fn select_posts_settles_stale_undated_rows_by_age() {
    let now = chrono::Utc::now();
    let mut old = msg("assistant", "ancient reply", 3);
    old.created_at = now - chrono::Duration::hours(2);
    let (bodies, mark) = select_posts(&[msg("user", "q", 2), old], 1, now);
    assert_eq!(bodies.len(), 2);
    assert_eq!(mark, 3);
}

#[test]
fn select_posts_skips_tool_and_system_roles_but_advances() {
    let now = chrono::Utc::now();
    let rows = vec![
        msg("tool", "tool output", 7),
        done("visible", 8),
        msg("system", "housekeeping", 9),
    ];
    let (bodies, mark) = select_posts(&rows, 6, now);
    assert_eq!(bodies, vec!["visible".to_string()]);
    assert_eq!(mark, 9, "silent roles still advance the watermark");
}

#[test]
fn select_posts_empty_window_keeps_watermark() {
    let (bodies, mark) = select_posts(&[], 11, chrono::Utc::now());
    assert!(bodies.is_empty());
    assert_eq!(mark, 11);
}
