//! Executable tool gates: declarative pre-execution policy (`gates.toml`).
//!
//! A gates file encodes a house constitution as data — first matching gate
//! decides *before* the approval layer:
//!
//! - `deny` refuses the call before execution (reason surfaced to model/user)
//! - `allow` pre-clears the approval prompt for exactly the matched shape
//! - `prompt` forces the interactive prompt even under auto-approve flags
//! - no match ⇒ the existing approval policy applies, unchanged
//!
//! Fail-closed: malformed TOML, a bad regex, or an unknown decision string
//! denies every gated call until the file is fixed — a broken constitution
//! must never widen permissions. A *missing* gates file means the feature
//! is off (`NoMatch`), so behavior is identical for profiles that never
//! opt in.
//!
//! `args-regex` matches the newline-joined string leaves of the tool input
//! JSON — a bash `command` value therefore appears verbatim, unescaped.

use crate::config::types::opencrabs_home;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Verdict for a single tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    /// Refuse before execution; carries the deciding gate and its reason.
    Deny { gate: String, reason: String },
    /// Pre-clear the approval prompt for this exact call shape.
    Allow,
    /// Force the interactive approval prompt (never auto-approved).
    Prompt,
    /// No gate matched; the existing approval policy applies.
    NoMatch,
}

#[derive(Debug, Deserialize)]
struct GateRule {
    name: String,
    /// Exact tool name this gate applies to (e.g. `bash`, `edit`).
    tool: String,
    /// Optional regex over the input's string leaves.
    #[serde(rename = "args-regex", default)]
    args_regex: Option<String>,
    /// `deny` | `allow` | `prompt`
    decision: String,
    /// Human-facing reason surfaced on deny.
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GatesFile {
    #[serde(default)]
    gate: Vec<GateRule>,
}

/// Path of the gates file for the active profile.
pub fn gates_path() -> PathBuf {
    opencrabs_home().join("gates.toml")
}

/// Evaluate the active profile's `gates.toml` for one tool call.
pub fn evaluate(tool_name: &str, tool_input: &Value) -> GateDecision {
    evaluate_file(&gates_path(), tool_name, tool_input)
}

/// Evaluate an explicit gates file. Missing file ⇒ `NoMatch` (feature off).
pub fn evaluate_file(path: &Path, tool_name: &str, tool_input: &Value) -> GateDecision {
    match std::fs::read_to_string(path) {
        Ok(src) => evaluate_str(&src, tool_name, tool_input),
        Err(_) => GateDecision::NoMatch,
    }
}

/// Evaluate raw gates TOML. Fail-closed on any parse, compile, or decision
/// error: the returned `Deny` names the offending gate so the owner can fix
/// the file fast.
pub fn evaluate_str(toml_src: &str, tool_name: &str, tool_input: &Value) -> GateDecision {
    let file: GatesFile = match toml::from_str(toml_src) {
        Ok(f) => f,
        Err(e) => return fail_closed(format!("gates.toml parse error: {e}")),
    };
    let args_text = string_leaves(tool_input).join("\n");
    for rule in &file.gate {
        if rule.tool != tool_name {
            continue;
        }
        if let Some(pat) = &rule.args_regex {
            let re = match regex::Regex::new(pat) {
                Ok(re) => re,
                Err(e) => {
                    return fail_closed(format!(
                        "gate '{}' has an invalid args-regex: {e}",
                        rule.name
                    ));
                }
            };
            if !re.is_match(&args_text) {
                continue;
            }
        }
        return match rule.decision.as_str() {
            "deny" => GateDecision::Deny {
                gate: rule.name.clone(),
                reason: rule
                    .reason
                    .clone()
                    .unwrap_or_else(|| format!("denied by gate '{}'", rule.name)),
            },
            "allow" => GateDecision::Allow,
            "prompt" => GateDecision::Prompt,
            other => fail_closed(format!(
                "gate '{}' has an unknown decision '{other}'",
                rule.name
            )),
        };
    }
    GateDecision::NoMatch
}

/// Fail-closed verdict for an unusable gates configuration.
fn fail_closed(what: String) -> GateDecision {
    GateDecision::Deny {
        gate: "<gates>".to_string(),
        reason: format!("{what} — failing closed; fix gates.toml to restore execution"),
    }
}

/// All string leaves of a JSON value, depth-first, object keys sorted.
fn string_leaves(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items.iter().flat_map(string_leaves).collect(),
        Value::Object(map) => map.values().flat_map(string_leaves).collect(),
        _ => Vec::new(),
    }
}
