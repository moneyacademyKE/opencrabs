//! Telegram Mini App `initData` authentication — Phase 3's second door.
//!
//! Telegram signs every Mini App launch payload with a key derived from the
//! bot's own token: `secret = HMAC-SHA256("WebAppData", bot_token)` (the
//! constant string as HMAC **key**, the bot token as data — per the official
//! docs' PHP example), and the payload's `hash` field is
//! `HMAC-SHA256(secret, data_check_string)` where
//! `data_check_string` is every field **except `hash`** — `signature` stays in
//! it — sorted by key and joined with `\n`. We verify that signature, require
//! the launch to
//! be fresh ([`MAX_AGE_SECS`]), and check the Telegram user id against
//! `[a2a].telegram_users` — an allowlist that **defaults to empty**, i.e. the
//! door ships CLOSED until the owner lists user ids.
//!
//! All three checks pass → we mint a normal scoped session token via
//! [`PairingStore::issue_session`]: same tokens, same scopes, same revocation
//! as the pair-code door. One auth surface, two entrances.
//!
//! Failures are cheap (one HMAC) and indistinguishable in shape (401), so a
//! per-client fixed-window limiter rides in front purely as flood control,
//! sharing the pairing limiter's state with a distinct key prefix.

use anyhow::{Context, Result, bail, ensure};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::a2a::server::A2aState;
use crate::remote::now_secs;
use crate::remote::pairing::PairingStore;

/// `auth_date` must be within this window — a launch payload is not a
/// long-lived credential.
pub const MAX_AGE_SECS: i64 = 24 * 60 * 60;

/// Scopes granted to an allowlisted Telegram user's Mini App session. The
/// allowlist is the owner's explicit trust decision; the app needs send to
/// drive the agent and read to poll it.
pub const TELEGRAM_SESSION_SCOPES: [&str; 2] = ["a2a:read", "a2a:send"];

/// Limiter key prefix so initData attempts share the pairing limiter's
/// discipline without eating the pair endpoint's own budget.
const LIMITER_PREFIX: &str = "tg-initdata:";

#[derive(Debug, Deserialize)]
pub struct InitDataRequest {
    pub init_data: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitDataResponse {
    pub token: String,
    pub scopes: Vec<String>,
    pub expires_at: u64,
    pub user_id: i64,
    pub username: Option<String>,
}

/// The Telegram user a validated payload identifies.
#[derive(Debug, PartialEq, Eq)]
pub struct TelegramUser {
    pub id: i64,
    pub username: Option<String>,
}

// ─── validation ──────────────────────────────────────────────────────────

/// HMAC-SHA256 without pulling a new dependency: sha2 only, textbook ipad/opad.
pub(crate) fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let (mut ipad, mut opad) = ([0x36u8; 64], [0x5cu8; 64]);
    for i in 0..64 {
        ipad[i] ^= block[i];
        opad[i] ^= block[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner);
    let out = outer.finalize();
    let mut mac = [0u8; 32];
    mac.copy_from_slice(&out);
    mac
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Constant-time equality: no early exit on the first differing byte.
pub(crate) fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

fn hex_val(b: u8) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => bail!("invalid hex digit in percent escape"),
    }
}

fn urldecode(s: &str) -> Result<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                ensure!(i + 2 < bytes.len(), "truncated percent escape in init_data");
                let hi = hex_val(bytes[i + 1])?;
                let lo = hex_val(bytes[i + 2])?;
                out.push(hi << 4 | lo);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).context("init_data field is not UTF-8")
}

/// Validate a Telegram Mini App `initData` query string against the bot's
/// token: signature, freshness, and the embedded user.
pub fn validate_init_data(
    init_data: &str,
    bot_token: &str,
    max_age_secs: i64,
) -> Result<TelegramUser> {
    // 1. Parse the query string into decoded (key, value) pairs.
    let mut fields: Vec<(String, String)> = Vec::new();
    for pair in init_data.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').context("malformed init_data pair")?;
        fields.push((urldecode(k)?, urldecode(v)?));
    }
    ensure!(!fields.is_empty(), "empty init_data");

    // 2. data_check_string: dual-mode verification.
    //    Official Bot API 8.0 specification excludes both `hash` and `signature`
    //    from data_check_string for HMAC calculation. Some implementations or
    //    legacy clients may include `signature` (excluding only `hash`).
    //    We compute both candidates and accept if either passes constant-time comparison.
    let hash = fields
        .iter()
        .find(|(k, _)| k == "hash")
        .map(|(_, v)| v.clone())
        .context("init_data missing hash")?;

    let mut check_with_sig: Vec<String> = fields
        .iter()
        .filter(|(k, _)| k != "hash")
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    check_with_sig.sort();
    let data_check_with_sig = check_with_sig.join("\n");

    let mut check_no_sig: Vec<String> = fields
        .iter()
        .filter(|(k, _)| k != "hash" && k != "signature")
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    check_no_sig.sort();
    let data_check_no_sig = check_no_sig.join("\n");

    // 3. HMAC chain: secret from the bot token, check both candidate hashes.
    let secret = hmac_sha256(b"WebAppData", bot_token.as_bytes());
    let expected_with_sig = hex(&hmac_sha256(&secret, data_check_with_sig.as_bytes()));
    let expected_no_sig = hex(&hmac_sha256(&secret, data_check_no_sig.as_bytes()));
    ensure!(
        ct_eq(expected_no_sig.as_bytes(), hash.as_bytes())
            || ct_eq(expected_with_sig.as_bytes(), hash.as_bytes()),
        "initData signature mismatch"
    );

    // 4. Freshness: auth_date within the window.
    let auth_date: i64 = fields
        .iter()
        .find(|(k, _)| k == "auth_date")
        .and_then(|(_, v)| v.parse().ok())
        .context("init_data missing or malformed auth_date")?;
    let now = i64::try_from(now_secs()).unwrap_or(0);
    ensure!(
        now - auth_date <= max_age_secs && auth_date <= now + 300,
        "initData is stale (auth_date outside the freshness window)"
    );

    // 5. The user the payload identifies.
    let user_raw = fields
        .iter()
        .find(|(k, _)| k == "user")
        .map(|(_, v)| v.clone())
        .context("init_data missing user")?;
    let user: serde_json::Value =
        serde_json::from_str(&user_raw).context("init_data user field is not JSON")?;
    let id = user
        .get("id")
        .and_then(|v| v.as_i64())
        .context("init_data user missing id")?;
    let username = user
        .get("username")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    Ok(TelegramUser { id, username })
}

// ─── endpoint ────────────────────────────────────────────────────────────

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}

/// POST /a2a/telegram/init-data — exchange a signed Telegram launch payload
/// for a scoped session token. Door ships closed: no bot token or empty
/// allowlist means 401, always.
pub async fn handle_init_data(
    State(state): State<A2aState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<InitDataRequest>,
) -> Response {
    let key = format!(
        "{LIMITER_PREFIX}{}",
        crate::a2a::pairing_http::client_key(&headers)
    );
    {
        let mut map = state.pair_hits.lock().unwrap_or_else(|e| e.into_inner());
        if crate::a2a::pairing_http::bump(&mut map, &key, now_secs()).is_err() {
            return crate::a2a::pairing_http::rate_limited(
                60,
                "too many initData attempts, slow down",
            );
        }
    }

    let Some(bot_token) = state.telegram_token.as_deref() else {
        return unauthorized("telegram auth is not configured on this host");
    };
    if state.telegram_users.is_empty() {
        return unauthorized(
            "telegram auth allowlist is empty — add user ids under [a2a].telegram_users",
        );
    }

    let user = match validate_init_data(&body.init_data, bot_token, MAX_AGE_SECS) {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!("initData rejected: {e:#}");
            return unauthorized("invalid or expired initData");
        }
    };
    if !state.telegram_users.contains(&user.id) {
        tracing::warn!(user_id = user.id, "initData user not allowlisted");
        return unauthorized("user is not allowlisted");
    }

    let label = if body.label.trim().is_empty() {
        match &user.username {
            Some(uname) => format!("telegram:{uname}"),
            None => format!("telegram:{}", user.id),
        }
    } else {
        body.label.trim().to_string()
    };
    let scopes: Vec<String> = TELEGRAM_SESSION_SCOPES
        .iter()
        .map(|s| s.to_string())
        .collect();
    match state.pairing.issue_session(&label, scopes) {
        Ok((token, session)) => {
            tracing::info!(user_id = user.id, session = %session.id, "A2A telegram initData: session issued");
            (
                StatusCode::OK,
                Json(InitDataResponse {
                    token,
                    scopes: session.scopes,
                    expires_at: session.expires_at,
                    user_id: user.id,
                    username: user.username,
                }),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("session mint failure: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "session store unavailable" })),
            )
                .into_response()
        }
    }
}

// ─── bot token loading (host side) ───────────────────────────────────────

/// Load the Telegram bot token from `keys.toml` `[channels.telegram].token`.
/// Absent → None → the initData door stays closed.
pub fn load_bot_token() -> Option<String> {
    let path = crate::config::opencrabs_home().join("keys.toml");
    let raw = std::fs::read_to_string(path).ok()?;
    let value: toml::Value = toml::from_str(&raw).ok()?;
    value
        .get("channels")?
        .get("telegram")?
        .get("token")?
        .as_str()
        .filter(|t| !t.trim().is_empty())
        .map(str::to_string)
}

/// Convenience for the host: the pairing store the A2A gateway itself uses.
pub fn host_pairing_store() -> PairingStore {
    PairingStore::new(PairingStore::default_path())
}
