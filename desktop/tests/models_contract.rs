use opencrabs_desktop_ui::models::{
    ChannelStatus, DiagnosticsSnapshot, UpdateInfo, VoiceConfigInfo,
};
use serde_json::json;

#[test]
fn channel_status_json_round_trip_preserves_error_field() {
    let value = json!({
        "name": "telegram",
        "display_name": "Telegram Bot",
        "enabled": true,
        "alive": false,
        "error": "Missing credentials/config"
    });

    let parsed: ChannelStatus =
        serde_json::from_value(value.clone()).expect("channel status deserialize");
    assert_eq!(parsed.name, "telegram");
    assert_eq!(parsed.error.as_deref(), Some("Missing credentials/config"));

    let encoded = serde_json::to_value(parsed).expect("channel status serialize");
    assert_eq!(encoded, value);
}

#[test]
fn update_info_json_round_trip_matches_backend_shape() {
    let value = json!({
        "version": "0.2.0",
        "current_version": "0.1.0",
        "release_notes": "Tighter CSP and safer bridge",
        "date": "2026-07-25"
    });

    let parsed: UpdateInfo =
        serde_json::from_value(value.clone()).expect("update info deserialize");
    assert_eq!(parsed.version, "0.2.0");
    assert_eq!(parsed.current_version, "0.1.0");

    let encoded = serde_json::to_value(parsed).expect("update info serialize");
    assert_eq!(encoded, value);
}

#[test]
fn diagnostics_snapshot_json_round_trip_preserves_safe_log_tail() {
    let value = json!({
        "app_version": "0.1.0",
        "config_present": true,
        "database_present": false,
        "log_path": "/safe/log",
        "log_tail": ["request completed"],
        "notes": ["Log preview is redacted"]
    });

    let parsed: DiagnosticsSnapshot =
        serde_json::from_value(value.clone()).expect("diagnostics deserialize");
    assert_eq!(parsed.log_tail, vec!["request completed"]);
    assert!(!parsed.database_present);
    assert_eq!(
        serde_json::to_value(parsed).expect("diagnostics serialize"),
        value
    );
}

#[test]
fn voice_contract_declares_unsupported_state() {
    let value = json!({
        "stt_enabled": false,
        "stt_provider": "unavailable",
        "tts_enabled": false,
        "tts_provider": "unavailable",
        "status": "unsupported",
        "message": "Desktop voice controls are unavailable until local STT/TTS is wired into the native shell."
    });

    let parsed: VoiceConfigInfo =
        serde_json::from_value(value.clone()).expect("voice config deserialize");
    assert_eq!(parsed.status, "unsupported");
    assert!(!parsed.stt_enabled && !parsed.tts_enabled);
    assert_eq!(
        serde_json::to_value(parsed).expect("voice config serialize"),
        value
    );
}
