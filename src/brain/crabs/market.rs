//! Crab market — the remote index and the update checker.
//!
//! The market is a git repo (default `moneyacademyKE/crab-market`) read
//! over plain GitHub raw: `index.toml` plus one dir per crab. No server,
//! no API, no auth.
//!
//! [`check_updates`] is REPORT-ONLY — parity with the Tier 1 bb
//! checker: it never applies anything, it names what moved. Git sources
//! are checked with `git ls-remote` (one call, no clone); local sources
//! by re-hashing the local `crab.toml`. A moved pin is no longer
//! automatic drift: the crab's recorded file set is content-hashed
//! upstream vs on disk (one shallow clone, only when the pin moved), so
//! unrelated market commits don't drift every installed crab.

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
    #[serde(default)]
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
        let status = status_for(rec, &market, home).await;
        report.push(UpdateReport {
            crab: rec.crab.clone(),
            installed_version: rec.version.clone(),
            status,
        });
    }
    report
}

async fn status_for(rec: &CrabRecord, market: &[MarketEntry], home: &Path) -> UpdateStatus {
    // Market-published pin wins when the crab is listed there. Index
    // pins are PER-CRAB bookkeeping (set at that crab's publish), so the
    // compare is already content-truthful — no clone needed.
    if let Some(pin) = market
        .iter()
        .find(|e| e.name == rec.crab)
        .and_then(|e| e.pin.as_ref())
    {
        return if *pin == rec.pin {
            UpdateStatus::UpToDate
        } else {
            UpdateStatus::Drift {
                current: pin.clone(),
            }
        };
    }
    match rec.source.kind.as_str() {
        "git" => {
            let Some(url) = &rec.source.url else {
                return UpdateStatus::Unknown("git source without url".into());
            };
            match remote_head(url) {
                Ok(head) => pin_status(rec, &head, home),
                Err(e) => UpdateStatus::Unknown(e),
            }
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

/// A moved pin is no longer automatically drift — that was the HEAD-pin
/// quirk (any market commit drifted every installed crab). Content
/// identity decides: identical bytes upstream vs installed = UpToDate.
fn pin_status(rec: &CrabRecord, pin: &str, home: &Path) -> UpdateStatus {
    if pin == rec.pin {
        return UpdateStatus::UpToDate;
    }
    match content_matches_upstream(rec, home) {
        Ok(true) => UpdateStatus::UpToDate,
        Ok(false) => UpdateStatus::Drift {
            current: pin.to_string(),
        },
        Err(e) => UpdateStatus::Unknown(e),
    }
}

/// Shallow-clone the source repo and compare the recorded file set's
/// content hash upstream vs on disk. One clone per mismatched crab, only
/// when the pin actually moved.
fn content_matches_upstream(rec: &CrabRecord, home: &Path) -> Result<bool, String> {
    let Some(url) = &rec.source.url else {
        return Err("git source without url".into());
    };
    let (repo, subdir) = super::content::split_repo_fragment(url);
    let tmp = std::env::temp_dir().join(format!("crab-check-{}", uuid::Uuid::new_v4()));
    let out = std::process::Command::new("git")
        .args(["clone", "--depth", "50", &repo, &tmp.to_string_lossy()])
        .output()
        .map_err(|e| format!("git clone failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let base = match &subdir {
        Some(s) => tmp.join(s),
        None => tmp.clone(),
    };
    let installed = super::content::content_sha(&rec.files, home);
    let upstream = super::content::content_sha(&rec.files, &base);
    let _ = std::fs::remove_dir_all(&tmp);
    match (installed, upstream) {
        (Some(a), Some(b)) => Ok(a == b),
        // upstream no longer ships a recorded file — that IS a change
        (Some(_), None) => Ok(false),
        (None, _) => Err("installed file missing — cannot verify".into()),
    }
}

/// Remote HEAD sha via `git ls-remote <url> HEAD` — no clone.
fn remote_head(url: &str) -> Result<String, String> {
    // Ledger urls may carry a #subdir fragment (market monorepo) — ls-remote wants the bare repo.
    let (repo, _) = super::content::split_repo_fragment(url);
    let out = std::process::Command::new("git")
        .args(["ls-remote", repo.as_str(), "HEAD"])
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
    use std::path::PathBuf;

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
        let status = status_for(&rec, &market, &home).await;
        assert_eq!(
            status,
            UpdateStatus::Drift {
                current: "newpin".into()
            }
        );
        let up_to_date = vec![entry("competitor-watch", "0.1.0", Some("oldpin"))];
        assert_eq!(
            status_for(&rec, &up_to_date, &home).await,
            UpdateStatus::UpToDate
        );
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
        let home = std::env::temp_dir().join(format!("crab-home-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(status_for(&rec, &[], &home).await, UpdateStatus::UpToDate);
        // edit the pack → drift
        std::fs::write(
            pack.join("crab.toml"),
            "name = \"a\"\nversion = \"0.2.0\"\ndescription = \"x\"\n\n[[skills]]\npath = \"skills/a\"\n",
        )
        .unwrap();
        assert!(matches!(
            status_for(&rec, &[], &home).await,
            UpdateStatus::Drift { .. }
        ));
    }

    /// Run a git command in `repo`, asserting success.
    fn git_run(repo: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn git_head(repo: &Path) -> String {
        let out = std::process::Command::new("git")
            .args(["-C", &repo.to_string_lossy(), "rev-parse", "HEAD"])
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Stage everything and commit; returns the new HEAD sha.
    fn commit_all(repo: &Path) -> String {
        let id = ["-c", "user.email=t@t", "-c", "user.name=t"];
        let mut add = id.to_vec();
        add.push("add");
        add.push("-A");
        git_run(repo, &add);
        let mut ci = id.to_vec();
        ci.extend(["commit", "-qm", "c-next"]);
        git_run(repo, &ci);
        git_head(repo)
    }

    /// A real local git repo with the pack at `crabs/a`, committed once.
    fn git_market_fixture() -> (PathBuf, String) {
        let repo = std::env::temp_dir().join(format!("crab-mkt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(repo.join("crabs/a/skills/a")).unwrap();
        std::fs::write(
            repo.join("crabs/a/crab.toml"),
            "name = \"a\"\nversion = \"0.1.0\"\ndescription = \"x\"\n\n[[skills]]\npath = \"skills/a\"\n",
        )
        .unwrap();
        std::fs::write(
            repo.join("crabs/a/skills/a/SKILL.md"),
            "---\nname: a\ndescription: x\n---\nbody",
        )
        .unwrap();
        git_run(&repo, &["init", "-q"]);
        commit_all(&repo);
        let sha1 = git_head(&repo);
        (repo, sha1)
    }

    #[tokio::test]
    async fn moved_pin_identical_content_is_up_to_date_real_drift_still_drifts() {
        let (repo, sha1) = git_market_fixture();
        // installed copy: identical bytes, pinned at the first commit
        let home = std::env::temp_dir().join(format!("crab-home-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(home.join("skills/a")).unwrap();
        std::fs::write(
            home.join("skills/a/SKILL.md"),
            "---\nname: a\ndescription: x\n---\nbody",
        )
        .unwrap();
        let rec = CrabRecord::new(
            "a",
            "0.1.0",
            &sha1,
            super::super::ledger::CrabSource::git(format!("{}#crabs/a", repo.display())),
            vec!["skills/a/SKILL.md".into()],
        );
        // fast path: HEAD == pin
        assert_eq!(status_for(&rec, &[], &home).await, UpdateStatus::UpToDate);

        // THE QUIRK: unrelated upstream commit moves HEAD, pack unchanged
        std::fs::write(repo.join("README.md"), "someone else published").unwrap();
        let sha2 = commit_all(&repo);
        assert_ne!(sha2, sha1);
        // old behavior: Drift (forced the hand re-pin of 10 crabs).
        // new behavior: content identical → UpToDate.
        assert_eq!(status_for(&rec, &[], &home).await, UpdateStatus::UpToDate);

        // REAL drift: upstream changes this crab's bytes
        std::fs::write(
            repo.join("crabs/a/skills/a/SKILL.md"),
            "---\nname: a\ndescription: x\n---\nbody v2",
        )
        .unwrap();
        let _ = commit_all(&repo);
        assert!(matches!(
            status_for(&rec, &[], &home).await,
            UpdateStatus::Drift { .. }
        ));
    }

    #[test]
    fn parse_index_rejects_malformed() {
        // not TOML at all
        assert!(parse_index("this is not toml {{{").is_err());
        // valid TOML, entry missing a required field (no `name`)
        let missing_name = r#"
[[crabs]]
version = "0.1.0"
description = "d"
"#;
        let err = parse_index(missing_name).unwrap_err();
        assert!(err.contains("parse failed"), "unexpected error: {err}");
    }

    #[test]
    fn parse_index_optional_fields_default_to_none() {
        let raw = r#"
[[crabs]]
name = "minimal"
version = "0.1.0"
description = "no category, no pin"
repo = "https://example.com/market"
"#;
        let entries = parse_index(raw).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "minimal");
        assert!(entries[0].pin.is_none());
        assert!(entries[0].category.is_none());
    }

    #[test]
    fn empty_index_parses_to_no_entries() {
        // a fresh market with zero crabs: legal, not an error
        let entries = parse_index("").unwrap();
        assert!(entries.is_empty());
    }
}
