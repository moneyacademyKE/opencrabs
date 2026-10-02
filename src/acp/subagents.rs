//! Sub-agent surface for ACP clients: delegation renders as agent panels.
//!
//! Two halves. First, the spawn tool call classifies as an AGENT card —
//! upstream clients (MonoCode's `acpAgentInfo`) treat `other`-kind calls as
//! agent panels when the title starts with agent/task/subagent, so
//! `spawn_agent` becomes `subagent: {label}`. Second, sub-agents run
//! detached: their live progress exists only in the status file
//! (`WorkStatus::read`, the same channel `wait_agent` polls), so while a
//! spawn's transcript entry is open the turn polls that file and re-emits
//! each transition as a child tool call stamped
//! `_meta.parentToolCallId = <spawn call id>` — the marker upstream's
//! `AcpSubagents.route` needs to nest the steps under the parent card.
//!
//! No new bus is invented: the status file IS the child's progress channel.

use std::sync::{Arc, Mutex as StdMutex};

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::brain::agent::service::work_status::{WorkState, WorkStatus};

use super::protocol::{self, session_update};
use super::transport::TransportHandle;

/// How often an open spawn's status file is polled. Matches `wait_agent`'s
/// cadence; faster only burns reads on a file written per round.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// Extract the agent id from a spawn result. The tool's success text is
/// `Spawned sub-agent '{label}' with id: {8-hex id} …` (spawn.rs) — parsing
/// beats threading the id through a side channel the tool loop doesn't have.
pub(crate) fn spawned_agent_id(summary: &str) -> Option<String> {
    let marker = "with id: ";
    let idx = summary.find(marker)? + marker.len();
    let id: String = summary[idx..]
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect();
    (id.len() == 8).then_some(id)
}

/// Delegation calls render as agent cards: the title prefix is the upstream
/// classification contract. `None` keeps the plain tool name.
pub(crate) fn delegation_title(tool_name: &str, tool_input: &Value) -> Option<String> {
    if tool_name != "spawn_agent" {
        return None;
    }
    let label = tool_input
        .get("label")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    Some(match label {
        Some(label) => format!("subagent: {label}"),
        None => "subagent".to_string(),
    })
}

/// One watched child: the status-file transitions not yet emitted, keyed to
/// the spawn call id they must attribute to.
#[derive(Default)]
pub(crate) struct SpawnWatch {
    /// The spawn tool call's transcript id — every child step carries it as
    /// `_meta.parentToolCallId` so the client nests the steps under the
    /// spawn's agent card.
    parent_call_id: String,
    done: bool,
    signature: Option<(usize, Option<String>, Option<String>)>,
    open_child: Option<String>,
}

impl SpawnWatch {
    pub(crate) fn new(parent_call_id: String) -> Self {
        Self {
            parent_call_id,
            ..Default::default()
        }
    }
}

/// Turn-scoped watch registry. The progress callback (sync) registers; the
/// poller task (async) consumes. Notify wakes the poller the moment a spawn
/// lands instead of waiting out a tick.
#[derive(Default)]
pub(crate) struct SpawnWatches {
    watches: StdMutex<Vec<(String, SpawnWatch)>>,
    notify: tokio::sync::Notify,
}

impl SpawnWatches {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register(&self, agent_id: String, call_id: String) {
        if let Ok(mut watches) = self.watches.lock() {
            watches.push((agent_id, SpawnWatch::new(call_id)));
        }
        self.notify.notify_one();
    }

    /// True when every registered watch reached a terminal state (or none
    /// were ever registered).
    fn all_done(&self) -> bool {
        self.watches
            .lock()
            .map(|watches| watches.iter().all(|(_, w)| w.done))
            .unwrap_or(true)
    }

    /// Apply `f` to one watch in place; returns its emitted updates.
    fn with_updates<R>(&self, agent_id: &str, f: impl FnOnce(&mut SpawnWatch) -> R) -> Option<R> {
        self.watches
            .lock()
            .ok()?
            .iter_mut()
            .find(|(id, _)| id == agent_id)
            .map(|(_, w)| f(w))
    }
}

/// Map one status-file read to stamped child updates. Pure — the poller only
/// sends what this returns. A changed signature emits the new child step;
/// the previous open child is completed; a terminal state closes the watch.
pub(crate) fn child_step_updates(
    agent_id: &str,
    watch: &mut SpawnWatch,
    status: &WorkStatus,
) -> Vec<Value> {
    let progress = status.progress.as_ref();
    let signature = progress.map(|p| (p.iteration, p.last_tool.clone(), p.last_event.clone()));
    let terminal = status.state.is_terminal();

    let mut out = Vec::new();
    if !watch.done && signature.is_some() && signature != watch.signature {
        if let Some(previous) = watch.open_child.take() {
            out.push(child_update(&previous, "completed", watch));
        }
        let (iteration, last_tool, _) = signature.clone().unwrap_or_default();
        let child_id = format!("{agent_id}-i{iteration}");
        let title = last_tool
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .unwrap_or("working");
        let mut update = json!({
            "sessionUpdate": "tool_call",
            "toolCallId": child_id,
            "title": title,
            "kind": protocol::tool_kind(title),
            "status": "in_progress",
        });
        stamp(&mut update, watch);
        out.push(update);
        watch.open_child = Some(child_id);
        watch.signature = signature;
    }
    if terminal && !watch.done {
        if let Some(previous) = watch.open_child.take() {
            let state = if status.state == WorkState::Completed {
                "completed"
            } else {
                "failed"
            };
            out.push(child_update(&previous, state, watch));
        }
        watch.done = true;
    }
    out
}

fn child_update(child_id: &str, state: &str, watch: &SpawnWatch) -> Value {
    let mut update = json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": child_id,
        "status": state,
    });
    stamp(&mut update, watch);
    update
}

fn stamp(update: &mut Value, watch: &SpawnWatch) {
    if let Some(obj) = update.as_object_mut() {
        obj.insert(
            "_meta".to_string(),
            json!({ "parentToolCallId": watch.parent_call_id }),
        );
    }
}

/// Poll every open spawn's status file and push transitions as stamped
/// updates. Bounded by the turn's cancel token: a poller never outlives the
/// prompt that spawned the watch (the completion push reaches the next turn
/// through the normal queue).
pub(crate) async fn run_spawn_watches(
    handle: TransportHandle,
    acp_session_id: String,
    watches: Arc<SpawnWatches>,
    cancel: CancellationToken,
) {
    let mut tick = tokio::time::interval(POLL_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = watches.notify.notified() => {},
            _ = tick.tick() => {},
        }
        let ids: Vec<String> = watches
            .watches
            .lock()
            .map(|watches| watches.iter().map(|(agent, _)| agent.clone()).collect())
            .unwrap_or_default();
        for agent_id in ids {
            let Some(status) = WorkStatus::read(&agent_id) else {
                continue; // not written yet — the next tick re-reads
            };
            let updates = watches.with_updates(&agent_id, |watch| {
                child_step_updates(&agent_id, watch, &status)
            });
            for update in updates.into_iter().flatten() {
                handle.send(session_update(&acp_session_id, update));
            }
        }
        if watches.all_done() {
            break;
        }
    }
}
