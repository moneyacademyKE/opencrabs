#[test]
fn desktop_state_is_persisted_by_the_backend_contract() {
    let panes_rs = std::fs::read_to_string(backend_root().join("src/commands/panes.rs"))
        .expect("read panes.rs");
    assert!(panes_rs.contains("desktop-panes.toml"));
    assert!(panes_rs.contains("save_desktop_state"));
    assert!(panes_rs.contains("set_pane_session"));
    assert!(panes_rs.contains("remove_pane"));
}

#[test]
fn diagnostics_backend_redacts_and_bounds_log_output() {
    let diagnostics_rs =
        std::fs::read_to_string(backend_root().join("src/commands/diagnostics.rs"))
            .expect("read diagnostics.rs");
    assert!(diagnostics_rs.contains("MAX_LOG_BYTES"));
    assert!(diagnostics_rs.contains("sensitive_line"));
    assert!(diagnostics_rs.contains("api_key"));
}

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn backend_root() -> PathBuf {
    repo_root().join("src-tauri")
}

#[test]
fn desktop_contract_docs_exist() {
    let root = repo_root();
    assert!(root.join("desktop-command-contract.md").exists());
    assert!(root.join("docs/release-strategy.md").exists());
    assert!(root.join("docs/release-runbook.md").exists());
    assert!(root.join("docs/release-checklist.md").exists());
    assert!(root.join("docs/diagnostics.md").exists());
}

#[test]
fn frontend_metadata_matches_manual_update_policy() {
    let cargo_toml =
        std::fs::read_to_string(backend_root().join("Cargo.toml")).expect("read tauri cargo");
    assert!(cargo_toml.contains("auto_update_install = false"));
    assert!(cargo_toml.contains("distribution = \"signed-manual\""));
}

#[test]
fn updater_backend_is_honest_about_manual_install() {
    let update_rs = std::fs::read_to_string(backend_root().join("src/commands/update.rs"))
        .expect("read update.rs");
    assert!(update_rs.contains("intentionally unsupported"));
    assert!(update_rs.contains("signed release artifact"));
}

#[test]
fn bridge_has_non_wasm_guardrail() {
    let bridge_rs =
        std::fs::read_to_string(repo_root().join("src/bridge.rs")).expect("read bridge.rs");
    assert!(bridge_rs.contains("Desktop bridge is only available in the wasm frontend"));
}

#[test]
fn startup_promotes_chat_to_ready_before_secondary_panel_loads() {
    let app_rs = std::fs::read_to_string(repo_root().join("src/app.rs")).expect("read app.rs");
    let ready_at = app_rs
        .find("status.set(\"Ready\".to_string())")
        .expect("ready status transition");
    let secondary_load_at = app_rs
        .find("get_workspace_root")
        .expect("secondary workspace load");

    assert!(
        ready_at < secondary_load_at,
        "chat-critical readiness must precede secondary panel loading"
    );
    assert!(app_rs.contains("Some desktop panels are unavailable"));
    assert!(app_rs.contains("Files unavailable:"));
    assert!(app_rs.contains("Brain files unavailable:"));
}
