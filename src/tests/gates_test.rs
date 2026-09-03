//! Executable tool gates: verdict matrix over the shipped default set plus
//! fail-closed properties. `gates.toml.example` is pinned by these tests —
//! the file that ships is the file that's tested.

use crate::utils::gates::{GateDecision, evaluate_file, evaluate_str};
use serde_json::json;
use std::path::PathBuf;

/// The shipped default constitution (repo root `gates.toml.example`).
const SHIPPED: &str = include_str!("../../gates.toml.example");

fn bash(cmd: &str) -> serde_json::Value {
    json!({ "command": cmd })
}

fn expect_deny(d: &GateDecision, want_gate: &str) {
    match d {
        GateDecision::Deny { gate, .. } => assert_eq!(gate, want_gate, "wrong gate: {d:?}"),
        other => panic!("expected Deny({want_gate}), got {other:?}"),
    }
}

// --- Shipped default set: the constitution matrix ---------------------------

#[test]
fn shipped_deny_python_and_pip() {
    for cmd in [
        "python x.py",
        "python3 -c 'print(1)'",
        "pip install x",
        "/usr/bin/python3 -V",
    ] {
        expect_deny(&evaluate_str(SHIPPED, "bash", &bash(cmd)), "deny-python");
    }
}

#[test]
fn shipped_deny_python_in_compound_commands() {
    for cmd in [
        "echo ok && python gen.py",
        "bb build | python check.py",
        "ls; pip3 install wheel",
    ] {
        expect_deny(&evaluate_str(SHIPPED, "bash", &bash(cmd)), "deny-python");
    }
}

#[test]
fn args_regex_sees_every_input_string_leaf() {
    // The regex matches any string leaf of the input, not just one field.
    let toml = r#"
[[gate]]
name = "no-secrets-in-writes"
tool = "edit"
args-regex = "python3"
decision = "deny"
"#;
    let input = json!({ "path": "a.txt", "content": "run python3 now" });
    expect_deny(&evaluate_str(toml, "edit", &input), "no-secrets-in-writes");
    // Non-string fields and non-matching strings stay clean.
    let clean = json!({ "path": "a.txt", "content": "run bb now", "count": 3 });
    assert_eq!(evaluate_str(toml, "edit", &clean), GateDecision::NoMatch);
}

#[test]
fn shipped_deny_python_word_not_substring() {
    // "conformist" contains no standalone python/pip; must not deny.
    assert_eq!(
        evaluate_str(SHIPPED, "bash", &bash("echo conformist typing")),
        GateDecision::NoMatch
    );
}

#[test]
fn shipped_deny_rm_direct_and_compound() {
    for cmd in [
        "rm -rf /tmp/x",
        "echo hi && rm important.db",
        "ls; rm -r dir",
    ] {
        expect_deny(&evaluate_str(SHIPPED, "bash", &bash(cmd)), "deny-rm");
    }
}

#[test]
fn shipped_deny_git_history_rewrites() {
    expect_deny(
        &evaluate_str(SHIPPED, "bash", &bash("git revert HEAD")),
        "deny-git-revert",
    );
    expect_deny(
        &evaluate_str(SHIPPED, "bash", &bash("git reset --hard HEAD~1")),
        "deny-git-reset-hard",
    );
    expect_deny(
        &evaluate_str(SHIPPED, "bash", &bash("git checkout abc123 -- .")),
        "deny-git-checkout-overwrite",
    );
}

#[test]
fn shipped_deny_force_and_main_pushes() {
    expect_deny(
        &evaluate_str(SHIPPED, "bash", &bash("git push --force origin x")),
        "deny-git-force-push",
    );
    expect_deny(
        &evaluate_str(SHIPPED, "bash", &bash("git push -f")),
        "deny-git-force-push",
    );
    expect_deny(
        &evaluate_str(SHIPPED, "bash", &bash("git push origin main")),
        "deny-git-push-main",
    );
    expect_deny(
        &evaluate_str(SHIPPED, "bash", &bash("git push main")),
        "deny-git-push-main",
    );
}

#[test]
fn shipped_feature_branch_push_prompts_not_denies() {
    // Irreversibles always prompt: a feature-branch push hits the
    // prompt gate, not a deny, and never auto-approves.
    assert_eq!(
        evaluate_str(SHIPPED, "bash", &bash("git push origin feat/tool-gates")),
        GateDecision::Prompt
    );
}

#[test]
fn shipped_allow_bb_scripting() {
    assert_eq!(
        evaluate_str(SHIPPED, "bash", &bash("bb test:all")),
        GateDecision::Allow
    );
    assert_eq!(
        evaluate_str(SHIPPED, "bash", &bash("bb -e '(+ 1 2)'")),
        GateDecision::Allow
    );
}

#[test]
fn shipped_read_only_tools_fall_through() {
    assert_eq!(
        evaluate_str(SHIPPED, "grep", &json!({"pattern": "rm", "path": "x"})),
        GateDecision::NoMatch
    );
}

#[test]
fn shipped_safe_git_work_is_untouched() {
    assert_eq!(
        evaluate_str(SHIPPED, "bash", &bash("git status --short")),
        GateDecision::NoMatch
    );
    assert_eq!(
        evaluate_str(SHIPPED, "bash", &bash("git commit -m 'one change'")),
        GateDecision::NoMatch
    );
}

// --- Semantics --------------------------------------------------------------

#[test]
fn first_matching_gate_wins() {
    let toml = r#"
[[gate]]
name = "first-allow"
tool = "bash"
args-regex = "^bb"
decision = "allow"

[[gate]]
name = "second-deny"
tool = "bash"
args-regex = "bb"
decision = "deny"
"#;
    assert_eq!(
        evaluate_str(toml, "bash", &bash("bb run")),
        GateDecision::Allow
    );
}

#[test]
fn prompt_gate_forces_interactive_approval() {
    let toml = r#"
[[gate]]
name = "careful"
tool = "bash"
args-regex = "deploy"
decision = "prompt"
"#;
    assert_eq!(
        evaluate_str(toml, "bash", &bash("deploy prod")),
        GateDecision::Prompt
    );
}

#[test]
fn tool_name_matches_exactly() {
    let toml = r#"
[[gate]]
name = "bash-only"
tool = "bash"
args-regex = "x"
decision = "deny"
"#;
    // A tool named `bashish` must not match the `bash` gate.
    assert_eq!(
        evaluate_str(toml, "bashish", &bash("x")),
        GateDecision::NoMatch
    );
}

// --- Fail-closed ------------------------------------------------------------

#[test]
fn malformed_toml_denies_everything() {
    let d = evaluate_str("this is [ not toml", "bash", &bash("ls"));
    match &d {
        GateDecision::Deny { reason, .. } => {
            assert!(
                reason.contains("parse error"),
                "reason should name the fault: {reason}"
            )
        }
        other => panic!("expected fail-closed Deny, got {other:?}"),
    }
}

#[test]
fn malformed_regex_denies_everything() {
    let toml = r#"
[[gate]]
name = "broken"
tool = "bash"
args-regex = "[unclosed"
decision = "deny"
"#;
    // Even a call the broken rule would never match is denied: fail closed.
    expect_deny(&evaluate_str(toml, "bash", &bash("ls")), "<gates>");
}

#[test]
fn unknown_decision_denies_everything() {
    let toml = r#"
[[gate]]
name = "typo"
tool = "bash"
decision = "maybe"
"#;
    expect_deny(&evaluate_str(toml, "bash", &bash("ls")), "<gates>");
}

#[test]
fn empty_gate_list_is_no_match() {
    assert_eq!(evaluate_str("", "bash", &bash("ls")), GateDecision::NoMatch);
}

#[test]
fn missing_gates_file_is_no_match() {
    let missing = PathBuf::from("/nonexistent/gates.toml");
    assert_eq!(
        evaluate_file(&missing, "bash", &bash("ls")),
        GateDecision::NoMatch
    );
}

#[test]
fn shipped_example_file_has_no_fail_closed_faults() {
    // Every probe must produce a real verdict: a `<gates>` deny means the
    // shipped file itself has a parse/compile/decision fault.
    for (tool, input) in [
        ("bash", bash("bb build")),
        ("bash", bash("git push origin feat/x")),
        ("edit", json!({ "path": "x" })),
    ] {
        match evaluate_str(SHIPPED, tool, &input) {
            GateDecision::Deny { gate, reason } if gate == "<gates>" => {
                panic!("shipped gates file has a fault: {reason}")
            }
            _ => {}
        }
    }
}
