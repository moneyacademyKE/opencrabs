//! Crab ledger — the pinning record of installed crabs, at
//! `~/.opencrabs/state/crabs.json`.
//!
//! Every `crab install` writes one [`CrabRecord`]: what was installed, at
//! which upstream pin, and exactly which files landed in the home. That
//! record is the source of truth for `crab remove` (deletes the recorded
//! files), `crab updates` (compares pins), and the drift gate on
//! reinstall (a changed upstream refuses until `--force` re-approves).
//!
//! Format: a JSON array, pretty-printed:
//!
//! ```json
//! [{"crab":"competitor-watch","version":"0.1.0","pin":"<sha>",
//!   "source":{"type":"git","url":"https://…"},
//!   "installed_at":"2026-09-22T05:00:00Z",
//!   "files":["skills/competitor-watch/SKILL.md"]}]
//! ```

use std::path::{Path, PathBuf};

/// Where one crab came from. `type` is "git" | "local" | "market".
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CrabSource {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl CrabSource {
    pub fn git(url: impl Into<String>) -> Self {
        Self { kind: "git".into(), url: Some(url.into()) }
    }

    pub fn local(path: impl Into<String>) -> Self {
        Self { kind: "local".into(), url: Some(path.into()) }
    }
}

/// One installed crab. `files` are relative to the home root — the exact
/// removal set, recorded at install time so removal never guesses.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CrabRecord {
    pub crab: String,
    pub version: String,
    /// Pinned upstream state: a git commit sha for git sources, or the
    /// sha256 of the local pack's `crab.toml` for local dirs.
    pub pin: String,
    pub source: CrabSource,
    /// RFC3339 UTC install timestamp.
    pub installed_at: String,
    pub files: Vec<String>,
}

impl CrabRecord {
    /// A record for a fresh install, timestamped now.
    pub fn new(
        crab: impl Into<String>,
        version: impl Into<String>,
        pin: impl Into<String>,
        source: CrabSource,
        files: Vec<String>,
    ) -> Self {
        Self {
            crab: crab.into(),
            version: version.into(),
            pin: pin.into(),
            source,
            installed_at: chrono::Utc::now().to_rfc3339(),
            files,
        }
    }
}

/// Ledger location: `<home>/state/crabs.json`.
pub fn ledger_path(home: &Path) -> PathBuf {
    home.join("state").join("crabs.json")
}

/// Load the ledger. A missing file is an empty ledger (nothing installed);
/// a malformed file is an error — silently swallowing it would drop the
/// pins and let a drift reinstall over the user's approval.
pub fn load(home: &Path) -> anyhow::Result<Vec<CrabRecord>> {
    let path = ledger_path(home);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    let records: Vec<CrabRecord> = serde_json::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("malformed crab ledger at {}: {e}", path.display()))?;
    Ok(records)
}

/// Persist the ledger atomically (unique temp file + rename, #911), with
/// the live-home write guard intact under `cfg(test)` (#1399).
pub fn save(home: &Path, records: &[CrabRecord]) -> anyhow::Result<()> {
    let path = ledger_path(home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(records)?;
    crate::config::types::io::atomic_write(&path, &raw)
        .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display()))?;
    Ok(())
}

/// Replace the record for `crab` in place, or append if absent.
pub fn upsert(records: &mut Vec<CrabRecord>, rec: CrabRecord) {
    match records.iter().position(|r| r.crab == rec.crab) {
        Some(i) => records[i] = rec,
        None => records.push(rec),
    }
}

/// Drop `crab`'s record, returning it (so callers can print what was
/// removed) or `None` when it wasn't installed.
pub fn remove(records: &mut Vec<CrabRecord>, crab: &str) -> Option<CrabRecord> {
    records.iter().position(|r| r.crab == crab).map(|i| records.remove(i))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home() -> PathBuf {
        let d = std::env::temp_dir().join(format!("crab-ledger-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample() -> CrabRecord {
        CrabRecord::new(
            "competitor-watch",
            "0.1.0",
            "abc123def456",
            CrabSource::git("https://github.com/moneyacademyKE/crab-market"),
            vec!["skills/competitor-watch/SKILL.md".into()],
        )
    }

    #[test]
    fn round_trips_through_disk() {
        let home = temp_home();
        crate::config::profile::with_home_override(home.clone(), || {
            let rec = sample();
            let records = vec![rec.clone()];
            save(&home, &records).unwrap();
            let loaded = load(&home).unwrap();
            assert_eq!(loaded.len(), 1);
            assert_eq!(loaded[0].crab, rec.crab);
            assert_eq!(loaded[0].files, rec.files);
            assert_eq!(loaded[0].source.kind, "git");
        });
    }

    #[test]
    fn missing_ledger_is_empty() {
        let home = temp_home();
        assert!(load(&home).unwrap().is_empty());
    }

    #[test]
    fn upsert_replaces_by_name() {
        let mut records = vec![sample()];
        let mut updated = sample();
        updated.version = "0.2.0".into();
        upsert(&mut records, updated);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].version, "0.2.0");
    }

    #[test]
    fn remove_returns_the_record() {
        let mut records = vec![sample(), CrabRecord::new(
            "morning-paper", "0.1.0", "sha", CrabSource::local("/tmp/x"), vec![])];
        let gone = remove(&mut records, "morning-paper").unwrap();
        assert_eq!(gone.crab, "morning-paper");
        assert_eq!(records.len(), 1);
        assert!(remove(&mut records, "nope").is_none());
    }

    #[test]
    fn malformed_ledger_is_an_error_not_a_reset() {
        let home = temp_home();
        let path = ledger_path(&home);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        let err = load(&home).unwrap_err().to_string();
        assert!(err.contains("malformed crab ledger"), "got: {err}");
    }
}
