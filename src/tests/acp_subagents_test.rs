//! `src/acp/subagents.rs` — the delegation surface's pure core: spawn-id
//! parsing, agent-card titles, and status-file → stamped-child-step mapping.

use serde_json::json;

use crate::acp::subagents::{SpawnWatch, child_step_updates, delegation_title, spawned_agent_id};
use crate::brain::agent::service::work_status::{ProgressSnapshot, WorkState, WorkStatus};

fn watch() -> SpawnWatch {
    SpawnWatch::new("spawn-call-id".to_string())
}

fn status(state: WorkState, iteration: usize, last_tool: Option<&str>) -> WorkStatus {
    WorkStatus {
        id: "abc123de".to_string(),
        kind: Default::default(),
        session_id: "parent-session".to_string(),
        parent_session_id: None,
        label: "test".to_string(),
        task: "test task".to_string(),
        spawned_at: "2026-10-02T00:00:00Z".to_string(),
        state,
        progress: Some(ProgressSnapshot {
            iteration,
            last_tool: last_tool.map(str::to_string),
            last_event: Some(format!("round {iteration} done")),
            updated_at: None,
        }),
        finish: None,
    }
}

fn parent_of(update: &serde_json::Value) -> &serde_json::Value {
    update.get("_meta").expect("child step carries _meta")
}

#[test]
fn spawn_summary_yields_the_agent_id() {
    let summary = "Spawned sub-agent 'refactor-auth' with id: 4f7a91bc\nSession: x\nPrompt: y";
    assert_eq!(spawned_agent_id(summary).as_deref(), Some("4f7a91bc"));
}

#[test]
fn non_spawn_or_malformed_summaries_yield_nothing() {
    assert_eq!(
        spawned_agent_id("Spawned sub-agent 'x' with id: nothex"),
        None
    );
    assert_eq!(spawned_agent_id("error: provider down"), None);
}

#[test]
fn spawn_titles_classify_as_agent_cards_upstream() {
    let input = json!({ "label": "refactor-auth", "prompt": "do it" });
    assert_eq!(
        delegation_title("spawn_agent", &input).as_deref(),
        Some("subagent: refactor-auth")
    );
    // No label: the bare word still matches the ^(agent|task|subagent) regex.
    assert_eq!(
        delegation_title("spawn_agent", &json!({})).as_deref(),
        Some("subagent")
    );
    // Any other tool keeps its plain name.
    assert_eq!(delegation_title("bash", &input), None);
}

#[test]
fn first_progress_becomes_one_stamped_child_step() {
    let mut w = watch();
    let updates = child_step_updates(
        "abc123de",
        &mut w,
        &status(WorkState::Running, 1, Some("bash")),
    );
    assert_eq!(updates.len(), 1);
    let step = &updates[0];
    assert_eq!(step["sessionUpdate"], "tool_call");
    assert_eq!(step["toolCallId"], "abc123de-i1");
    assert_eq!(step["title"], "bash");
    assert_eq!(step["kind"], "execute");
    assert_eq!(
        parent_of(step),
        &json!({ "parentToolCallId": "spawn-call-id" })
    );
}

#[test]
fn an_unchanged_status_emits_nothing() {
    let mut w = watch();
    let s = status(WorkState::Running, 1, Some("bash"));
    assert_eq!(child_step_updates("abc123de", &mut w, &s).len(), 1);
    assert!(child_step_updates("abc123de", &mut w, &s).is_empty());
}

#[test]
fn a_new_iteration_completes_the_previous_child_step() {
    let mut w = watch();
    child_step_updates(
        "abc123de",
        &mut w,
        &status(WorkState::Running, 1, Some("bash")),
    );
    let updates = child_step_updates(
        "abc123de",
        &mut w,
        &status(WorkState::Running, 2, Some("grep")),
    );
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0]["sessionUpdate"], "tool_call_update");
    assert_eq!(updates[0]["toolCallId"], "abc123de-i1");
    assert_eq!(updates[0]["status"], "completed");
    assert_eq!(updates[1]["toolCallId"], "abc123de-i2");
    assert_eq!(updates[1]["title"], "grep");
}

#[test]
fn terminal_state_closes_the_watch_with_a_final_status() {
    let mut w = watch();
    child_step_updates(
        "abc123de",
        &mut w,
        &status(WorkState::Running, 3, Some("bash")),
    );
    let updates = child_step_updates(
        "abc123de",
        &mut w,
        &status(WorkState::Completed, 3, Some("bash")),
    );
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0]["sessionUpdate"], "tool_call_update");
    assert_eq!(updates[0]["status"], "completed");
    // Closed: further reads emit nothing.
    assert!(
        child_step_updates(
            "abc123de",
            &mut w,
            &status(WorkState::Completed, 3, Some("bash"))
        )
        .is_empty()
    );
}

#[test]
fn failed_and_interrupted_children_close_as_failed() {
    let mut w = watch();
    child_step_updates(
        "abc123de",
        &mut w,
        &status(WorkState::Running, 1, Some("bash")),
    );
    let updates = child_step_updates(
        "abc123de",
        &mut w,
        &status(WorkState::Failed, 1, Some("bash")),
    );
    assert_eq!(updates[0]["status"], "failed");

    let mut w = watch();
    child_step_updates(
        "abc123de",
        &mut w,
        &status(WorkState::Running, 1, Some("bash")),
    );
    let updates = child_step_updates(
        "abc123de",
        &mut w,
        &status(WorkState::Interrupted, 1, Some("bash")),
    );
    assert_eq!(updates[0]["status"], "failed");
}

#[test]
fn a_child_with_no_tool_yet_shows_as_working() {
    let mut w = watch();
    let updates = child_step_updates("abc123de", &mut w, &status(WorkState::Running, 1, None));
    assert_eq!(updates[0]["title"], "working");
}
