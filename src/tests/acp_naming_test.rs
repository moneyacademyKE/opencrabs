//! ACP session naming: client self-identification, birth-title composition,
//! and the degradation ladder when either ingredient is missing.

use serde_json::json;

use crate::acp::naming::{birth_title, client_name_from_params};

#[test]
fn client_name_prefers_name_then_title() {
    let named = json!({
        "protocolVersion": 1,
        "clientInfo": {"name": "MonoCode", "version": "0.6.0"}
    });
    assert_eq!(client_name_from_params(&named).as_deref(), Some("MonoCode"));

    let titled = json!({"clientInfo": {"title": "Zed"}});
    assert_eq!(client_name_from_params(&titled).as_deref(), Some("Zed"));

    // name wins over title when both are present
    let both = json!({"clientInfo": {"name": "MonoCode", "title": "ignored"}});
    assert_eq!(client_name_from_params(&both).as_deref(), Some("MonoCode"));
}

#[test]
fn client_name_caps_trims_and_rejects_blanks() {
    let long = json!({"clientInfo": {"name": "x".repeat(100)}});
    assert_eq!(client_name_from_params(&long).unwrap().chars().count(), 40);

    let padded = json!({"clientInfo": {"name": "  MonoCode  "}});
    assert_eq!(
        client_name_from_params(&padded).as_deref(),
        Some("MonoCode")
    );

    assert_eq!(
        client_name_from_params(&json!({"clientInfo": {"name": "   "}})),
        None
    );
    assert_eq!(client_name_from_params(&json!({"clientInfo": {}})), None);
    assert_eq!(client_name_from_params(&json!({})), None);
    assert_eq!(
        client_name_from_params(&json!({"clientInfo": {"name": 42}})),
        None
    );
}

#[test]
fn birth_title_composes_the_full_shape() {
    assert_eq!(
        birth_title(Some("MonoCode"), Some("/Users/moe/monocode")),
        "ACP: MonoCode · monocode"
    );
    // trailing slash still yields the basename
    assert_eq!(
        birth_title(Some("MonoCode"), Some("/srv/app/")),
        "ACP: MonoCode · app"
    );
}

#[test]
fn birth_title_degrades_gracefully() {
    assert_eq!(birth_title(Some("MonoCode"), None), "ACP: MonoCode");
    assert_eq!(birth_title(None, Some("/srv/app")), "ACP: app");
    assert_eq!(birth_title(None, None), "ACP");
    // blank client falls through to the dir-only shape
    assert_eq!(birth_title(Some("   "), Some("/x/y")), "ACP: y");
    // root has no basename: degrade, never dangle a separator
    assert_eq!(birth_title(None, Some("/")), "ACP");
    assert_eq!(birth_title(Some("Zed"), Some("/")), "ACP: Zed");
}

#[test]
fn birth_title_handles_unicode_dirs() {
    assert_eq!(
        birth_title(Some("Moe"), Some("/Users/moe/银行项目")),
        "ACP: Moe · 银行项目"
    );
    assert_eq!(
        birth_title(Some("Moe"), Some("/home/moe/café-au-lait")),
        "ACP: Moe · café-au-lait"
    );
}
