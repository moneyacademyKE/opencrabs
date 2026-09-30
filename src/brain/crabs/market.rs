//! Crab market — the remote index and the update checker.
//!
//! The market is a git repo (default `moneyacademyKE/crab-market`) read
//! over plain GitHub raw: `index.toml` plus one dir per crab. No server,
//! no API, no auth.
//!
//! [`check_updates`] is REPORT-ONLY — parity with the Tier 1 bb
//! checker: it never applies anything, it names what moved. Git sources
//! are checked with `git ls-remote` (one call, no clone); local sources
//! by re-hashing the local `crab.toml`.

use super::ledger::{self, CrabRecord};
use serde::Deserialize;
use std::path::Path;

/// Default remote index (raw GitHub). Overridable for tests and mirrors.
pub const DEFAULT_MARKET_INDEX: &str =
    "https://raw.githubusercontent.com/moneyacademyKE/crab-market/main/index.toml";

/// One entry in the market's `index.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
pub struct MarketEntry {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub category: Option<String>,
    /// Repo the crab lives in (install source).
    pub repo: String,
    /// Subdir within the repo when the crab isn't at the root (market
    /// monorepo: `crabs/<name>`). Install as `<repo>#<path>`.
    #[serde(default)]
    pub path: Option<String>,
    /// The commit this version was published at (drift reference).
    #[serde(default)]
    pub pin: Option<String>,
}

/// Wrapper for TOML deserialization: a top-level `[[crabs]]` array-of-tables
/// parses into a struct holding the vec, never a bare `Vec`.
#[derive(Debug, Deserialize)]
struct MarketIndex {
    crabs: Vec<MarketEntry>,
}

/// Parse an `index.toml` blob (top-level `[[crabs]]` array-of-tables).
pub fn parse_index(raw: &str) -> Result<Vec<MarketEntry>, String> {
    let entries: MarketIndex =
        toml::from_str(raw).map_err(|e| format!("market index parse failed: {e}"))?;
    Ok(entries.crabs)
}

/// Fetch and parse the remote index.
pub async fn fetch_index(index_url: &str) -> Result<Vec<MarketEntry>, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(index_url)
        .header("User-Agent", "opencrabs-crab")
        .send()
        .await
        .map_err(|e| format!("market index fetch failed ({index_url}): {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "market index fetch failed ({index_url}): HTTP {}",
            resp.status()
        ));
    }
    let raw = resp
        .text()
        .await
        .map_err(|e| format!("market index read failed: {e}"))?;
    parse_index(&raw)
}

/// Load an index from a URL or a local file path. URLs go through
/// [`fetch_index`]; anything else is read from disk — private markets,
/// offline development, mirrors.
pub async fn load_index(src: &str) -> Result<Vec<MarketEntry>, String> {
    if src.starts_with("http://") || src.starts_with("https://") {
        fetch_index(src).await
    } else {
        let raw = std::fs::read_to_string(src).map_err(|e| format!("read index {src}: {e}"))?;
        parse_index(&raw)
    }
}

/// Substring search over name + description (case-insensitive).
/// Empty query = all entries.
pub fn search<'a>(entries: &'a [MarketEntry], query: &str) -> Vec<&'a MarketEntry> {
    let q = query.to_ascii_lowercase();
    entries
        .iter()
        .filter(|e| {
            q.is_empty()
                || e.name.to_ascii_lowercase().contains(&q)
                || e.description.to_ascii_lowercase().contains(&q)
                || e.category
                    .as_deref()
                    .is_some_and(|c| c.to_ascii_lowercase().contains(&q))
        })
        .collect()
}

/// Drift status for one installed crab.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateStatus {
    /// Ledger pin matches the upstream head.
    UpToDate,
    /// Upstream moved: ledger pin → current upstream pin.
    Drift { current: String },
    /// Cannot determine (local source vanished, ls-remote failed, …).
    Unknown(String),
}

/// The update report for the whole ledger. Report-only by contract.
#[derive(Debug, Clone)]
pub struct UpdateReport {
    pub crab: String,
    pub installed_version: String,
    pub status: UpdateStatus,
}

/// Compare every ledger record against its upstream. Git sources use
/// `git ls-remote` (no clone); local sources re-hash the manifest;
/// market sources compare against the index pin.
pub async fn check_updates(home: &Path, index_url: &str) -> Vec<UpdateReport> {
    let records = ledger::load(home).unwrap_or_default();
    let market = load_index(index_url).await.ok().unwrap_or_default();
    let mut report = Vec::new();
    for rec in &records {
        let status = status_for(rec, &market).await;
        report.push(UpdateReport {
            crab: rec.crab.clone(),
            installed_version: rec.version.clone(),
            status,
        });
    }
    report
}

async fn status_for(rec: &CrabRecord, market: &[MarketEntry]) -> UpdateStatus {
    // Market-published pin wins when the crab is listed there.
    if let Some(entry) = market.iter().find(|e| e.name == rec.crab) {
        if let Some(pin) = &entry.pin {
            return if *pin == rec.pin {
                UpdateStatus::UpToDate
            } else {
                UpdateStatus::Drift {
                    current: pin.clone(),
                }
            };
        }
    }
    match rec.source.kind.as_str() {
        "git" => {
            let Some(url) = &rec.source.url else {
                return UpdateStatus::Unknown("git source without url".into());
            };
            remote_head(url)
                .map(|head| {
                    if head == rec.pin {
                        UpdateStatus::UpToDate
                    } else {
                        UpdateStatus::Drift { current: head }
                    }
                })
                .unwrap_or_else(UpdateStatus::Unknown)
        }
        "local" => {
            let Some(path) = &rec.source.url else {
                return UpdateStatus::Unknown("local source without path".into());
            };
            match std::fs::read_to_string(Path::new(path).join("crab.toml")) {
                Ok(raw) => {
                    let pin = super::install::hex_sha256(&raw);
                    if pin == rec.pin {
                        UpdateStatus::UpToDate
                    } else {
                        UpdateStatus::Drift { current: pin }
                    }
                }
                Err(e) => UpdateStatus::Unknown(format!("local source unreadable: {e}")),
            }
        }
        other => UpdateStatus::Unknown(format!("unknown source kind: {other}")),
    }
}

/// Remote HEAD sha via `git ls-remote <url> HEAD` — no clone.
fn remote_head(url: &str) -> Result<String, String> {
    // Ledger urls may carry a #subdir fragment (market monorepo) — ls-remote wants the bare repo.
    let repo = url.split('#').next().unwrap_or(url);
    let out = std::process::Command::new("git")
        .args(["ls-remote", repo, "HEAD"])
        .output()
        .map_err(|e| format!("git ls-remote failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-remote failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let sha = line.split_whitespace().next().unwrap_or("").to_string();
    if sha.len() < 7 {
        return Err("git ls-remote returned no sha".into());
    }
    Ok(sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, version: &str, pin: Option<&str>) -> MarketEntry {
        MarketEntry {
            name: name.into(),
            version: version.into(),
            description: format!("desc for {name}"),
            category: Some("monitor".into()),
            repo: "https://github.com/moneyacademyKE/crab-market".into(),
            path: None,
            pin: pin.map(str::to_string),
        }
    }

    #[test]
    fn parses_index_toml() {
        let raw = r#"
[[crabs]]
name = "competitor-watch"
version = "0.1.0"
description = "Daily page-diff"
category = "monitor"
repo = "https://github.com/moneyacademyKE/crab-market"
pin = "abc123"
"#;
        let entries = parse_index(raw).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pin.as_deref(), Some("abc123"));
    }

    #[test]
    fn search_matches_name_description_category() {
        let entries = vec![
            entry("competitor-watch", "0.1.0", None),
            entry("morning-paper", "0.1.0", None),
        ];
        assert_eq!(search(&entries, "competitor").len(), 1);
        assert_eq!(search(&entries, "desc for morning").len(), 1);
        assert_eq!(search(&entries, "monitor").len(), 2); // both categorized monitor
        assert_eq!(search(&entries, "").len(), 2);
        assert!(search(&entries, "nonexistent").is_empty());
    }

    #[tokio::test]
    async fn load_index_reads_local_file() {
        let dir = std::env::temp_dir().join(format!("crab-idx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("index.toml");
        std::fs::write(
            &path,
            "[[crabs]]\nname = \"a\"\nversion = \"0.1.0\"\ndescription = \"d\"\nrepo = \"r\"\n",
        )
        .unwrap();
        let entries = load_index(path.to_str().unwrap()).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "a");
        // missing file → clean error, no panic
        assert!(
            load_index(dir.join("nope.toml").to_str().unwrap())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn update_status_uses_market_pin_first() {
        let home = std::env::temp_dir().join(format!("crab-market-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        let rec = CrabRecord::new(
            "competitor-watch",
            "0.1.0",
            "oldpin",
            super::super::ledger::CrabSource::git("https://example.invalid/x"),
            vec![],
        );
        // market lists the crab with a pin → that decides, no network call
        let market = vec![entry("competitor-watch", "0.1.1", Some("newpin"))];
        let status = status_for(&rec, &market).await;
        assert_eq!(
            status,
            UpdateStatus::Drift {
                current: "newpin".into()
            }
        );
        let up_to_date = vec![entry("competitor-watch", "0.1.0", Some("oldpin"))];
        assert_eq!(status_for(&rec, &up_to_date).await, UpdateStatus::UpToDate);
    }

    #[tokio::test]
    async fn update_status_local_rehash() {
        let pack = std::env::temp_dir().join(format!("crab-pack-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(pack.join("skills/a")).unwrap();
        std::fs::write(
            pack.join("crab.toml"),
            "name = \"a\"\nversion = \"0.1.0\"\ndescription = \"x\"\n\n[[skills]]\npath = \"skills/a\"\n",
        )
        .unwrap();
        let pin = super::super::install::hex_sha256(
            &std::fs::read_to_string(pack.join("crab.toml")).unwrap(),
        );
        let rec = CrabRecord::new(
            "a",
            "0.1.0",
            &pin,
            super::super::ledger::CrabSource::local(pack.to_string_lossy().into_owned()),
            vec![],
        );
        assert_eq!(status_for(&rec, &[]).await, UpdateStatus::UpToDate);
        // edit the pack → drift
        std::fs::write(
            pack.join("crab.toml"),
            "name = \"a\"\nversion = \"0.2.0\"\ndescription = \"x\"\n\n[[skills]]\npath = \"skills/a\"\n",
        )
        .unwrap();
        assert!(matches!(
            status_for(&rec, &[]).await,
            UpdateStatus::Drift { .. }
        ));
    }
}
