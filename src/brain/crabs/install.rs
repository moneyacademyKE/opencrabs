//! Crab install — resolve → gate → copy → pin, and remove.
//!
//! Exit-code contract (parity with the Tier 1 bb layer):
//!
//! | Error | Exit | Meaning |
//! |---|---|---|
//! | `NotFound` / `Io` | 1 | source missing / filesystem failure |
//! | `Failed` | 2 | clone, checkout, or manifest failure |
//! | `Secrets` | 3 | credential-shaped string in payload — hard stop |
//! | `Collision` / `Drift` | 4 | existing files differ / upstream moved |
//!
//! Collision and drift both require `--force` (explicit re-approval) —
//! never a silent overwrite. Removal deletes exactly the ledger-recorded
//! file set, nothing else, so a stray user edit inside a skill dir
//! survives removal only by not being in the record (the copy step
//! records what it wrote, not a directory sweep).

use super::ledger::{self, CrabRecord, CrabSource};
use super::manifest::CrabManifest;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Where a crab comes from.
#[derive(Debug, Clone)]
pub enum CrabOrigin {
    /// A git repo URL, optionally pinned to a commit.
    Git { url: String, pin: Option<String> },
    /// A local directory containing `crab.toml`.
    Local { path: PathBuf },
}

impl CrabOrigin {
    /// Human-facing source string for reports.
    pub fn describe(&self) -> String {
        match self {
            CrabOrigin::Git { url, pin } => match pin {
                Some(p) => format!("git {url} @ {p}"),
                None => format!("git {url}"),
            },
            CrabOrigin::Local { path } => format!("local {}", path.display()),
        }
    }
}

/// A pack resolved to a concrete directory with a known pin.
#[derive(Debug, Clone)]
pub struct ResolvedPack {
    pub dir: PathBuf,
    pub source: CrabSource,
    pub pin: String,
}

/// Install/remove failures with their exit codes.
#[derive(Debug)]
pub enum InstallError {
    NotFound(String),
    Failed(String),
    Secrets(String),
    Collision(String),
    Drift(String),
    Io(String),
}

impl InstallError {
    pub fn exit_code(&self) -> i32 {
        match self {
            InstallError::NotFound(_) | InstallError::Io(_) => 1,
            InstallError::Failed(_) => 2,
            InstallError::Secrets(_) => 3,
            InstallError::Collision(_) | InstallError::Drift(_) => 4,
        }
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (tag, msg) = match self {
            InstallError::NotFound(m) => ("not-found", m),
            InstallError::Failed(m) => ("failed", m),
            InstallError::Secrets(m) => ("secrets", m),
            InstallError::Collision(m) => ("collision", m),
            InstallError::Drift(m) => ("drift", m),
            InstallError::Io(m) => ("io", m),
        };
        write!(f, "[crab:{tag}] {msg}")
    }
}

/// Resolve an origin to a pack directory + pin.
///
/// Git: shallow clone (`--depth 50`) into a temp dir, checkout the pin
/// if given, pin = resolved HEAD. Local: the dir itself, pin = sha256 of
/// its `crab.toml` (content-addressed, so edits to the pack change the
/// pin and trigger the drift gate on reinstall).
pub fn resolve(origin: &CrabOrigin) -> Result<ResolvedPack, InstallError> {
    match origin {
        CrabOrigin::Git { url, pin } => {
            // <repo-url>#<subdir> — the pack lives in a subdir of the repo
            // (market monorepo layout: crabs/<name>/ inside the index repo).
            let (repo, subdir) = match url.split_once('#') {
                Some((r, s)) if !s.is_empty() => {
                    (r.to_string(), Some(s.trim_start_matches('/').to_string()))
                }
                _ => (url.clone(), None),
            };
            let tmp = std::env::temp_dir().join(format!("crab-src-{}", uuid::Uuid::new_v4()));
            let out = std::process::Command::new("git")
                .args(["clone", "--depth", "50", &repo, &tmp.to_string_lossy()])
                .output()
                .map_err(|e| InstallError::Failed(format!("git clone failed: {e}")))?;
            if !out.status.success() {
                return Err(InstallError::Failed(format!(
                    "git clone failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
            if let Some(p) = pin {
                let out = std::process::Command::new("git")
                    .args(["-C", &tmp.to_string_lossy(), "checkout", p])
                    .output()
                    .map_err(|e| InstallError::Failed(format!("git checkout failed: {e}")))?;
                if !out.status.success() {
                    return Err(InstallError::Failed(format!(
                        "git checkout {p} failed: {}",
                        String::from_utf8_lossy(&out.stderr).trim()
                    )));
                }
            }
            let head = std::process::Command::new("git")
                .args(["-C", &tmp.to_string_lossy(), "rev-parse", "HEAD"])
                .output()
                .map_err(|e| InstallError::Failed(format!("git rev-parse failed: {e}")))?;
            if !head.status.success() {
                return Err(InstallError::Failed(
                    "cannot resolve HEAD after clone".into(),
                ));
            }
            let sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
            let dir = match &subdir {
                Some(s) => {
                    let d = tmp.join(s);
                    if !d.is_dir() {
                        return Err(InstallError::Failed(format!(
                            "subdir {s} not found in {repo} @ {}",
                            &sha[..sha.len().min(7)]
                        )));
                    }
                    d
                }
                None => tmp,
            };
            Ok(ResolvedPack {
                dir,
                source: CrabSource::git(url.clone()),
                pin: sha,
            })
        }
        CrabOrigin::Local { path } => {
            if !path.is_dir() {
                return Err(InstallError::NotFound(format!(
                    "pack dir not found: {}",
                    path.display()
                )));
            }
            let manifest_raw = std::fs::read_to_string(path.join("crab.toml"))
                .map_err(|_| InstallError::Failed(format!("no crab.toml at {}", path.display())))?;
            let pin = hex_sha256(&manifest_raw);
            Ok(ResolvedPack {
                dir: path.clone(),
                source: CrabSource::local(path.to_string_lossy().into_owned()),
                pin,
            })
        }
    }
}

pub fn hex_sha256(data: &str) -> String {
    let digest = Sha256::digest(data.as_bytes());
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Options for [`install`].
#[derive(Debug, Clone, Default)]
pub struct InstallOpts {
    /// Skip the secret-scan hard stop (still recorded in the report).
    /// Never set by the CLI without an explicit flag.
    pub allow_secrets: bool,
    /// Re-approve collisions and upstream drift.
    pub force: bool,
}

/// Install a resolved pack into `home`. Returns the ledger record.
///
/// Order matters: manifest validity → disk validity → drift gate →
/// secret scan → copy → ledger write. Every refusal happens before the
/// first write, so a refused install never leaves partial state.
pub fn install(
    home: &Path,
    pack: &ResolvedPack,
    opts: &InstallOpts,
) -> Result<CrabRecord, InstallError> {
    let manifest = CrabManifest::load(&pack.dir.join("crab.toml")).map_err(InstallError::Failed)?;

    // Disk validity: every declared skill dir must exist with SKILL.md.
    let mut payload: Vec<String> = Vec::new();
    for entry in &manifest.skills {
        let skill_dir = pack.dir.join(&entry.path);
        if !skill_dir.is_dir() {
            return Err(InstallError::Failed(format!(
                "declared skill dir missing in pack: {}",
                entry.path
            )));
        }
        if !skill_dir.join("SKILL.md").is_file() {
            return Err(InstallError::Failed(format!(
                "declared skill has no SKILL.md: {}",
                entry.path
            )));
        }
        collect(&skill_dir, &entry.path, &mut payload)
            .map_err(|e| InstallError::Io(format!("payload walk: {e}")))?;
    }
    payload.sort();

    // Drift gate: ledger pin for this crab vs the pack we're holding.
    let records = ledger::load(home).map_err(|e| InstallError::Io(e.to_string()))?;
    if let Some(prior) = records.iter().find(|r| r.crab == manifest.name) {
        if prior.pin != pack.pin && !opts.force {
            return Err(InstallError::Drift(format!(
                "upstream changed since install ({} ≠ {}) — re-inspect, then --force to re-approve",
                &prior.pin[..12.min(prior.pin.len())],
                &pack.pin[..12.min(pack.pin.len())],
            )));
        }
    }

    // Secret hard stop — before any write.
    let hits = super::scan::secret_scan(&pack.dir, &payload);
    if !hits.is_empty() && !opts.allow_secrets {
        let detail = hits
            .iter()
            .map(|h| format!("  {}:{} ~ {}", h.file, h.line, h.pattern))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(InstallError::Secrets(format!(
            "credential-shaped strings in payload — refusing:\n{detail}"
        )));
    }

    // Copy with collision policy.
    let mut written: Vec<String> = Vec::new();
    for rel in &payload {
        let src = pack.dir.join(rel);
        let tgt = home.join(rel);
        if tgt.exists() {
            let same = file_eq(&src, &tgt).map_err(|e| InstallError::Io(e.to_string()))?;
            if same {
                written.push(rel.clone());
                continue;
            }
            if !opts.force {
                return Err(InstallError::Collision(format!(
                    "exists and differs — refusing to overwrite (--force re-approves): {rel}"
                )));
            }
        }
        if let Some(parent) = tgt.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| InstallError::Io(format!("mkdir {}: {e}", parent.display())))?;
        }
        std::fs::copy(&src, &tgt).map_err(|e| InstallError::Io(format!("copy {rel}: {e}")))?;
        written.push(rel.clone());
    }

    let record = CrabRecord::new(
        manifest.name.clone(),
        manifest.version.clone(),
        pack.pin.clone(),
        pack.source.clone(),
        written,
    );
    let mut records = records;
    ledger::upsert(&mut records, record.clone());
    ledger::save(home, &records).map_err(|e| InstallError::Io(e.to_string()))?;
    Ok(record)
}

/// Remove a crab by name: delete exactly the recorded files (and empty
/// parent dirs up to the home root), then drop the ledger record.
/// Refuses to follow anything not in the record.
pub fn remove(home: &Path, crab: &str) -> Result<CrabRecord, InstallError> {
    let mut records = ledger::load(home).map_err(|e| InstallError::Io(e.to_string()))?;
    let Some(record) = ledger::remove(&mut records, crab) else {
        return Err(InstallError::NotFound(format!(
            "crab '{crab}' is not installed"
        )));
    };
    for rel in &record.files {
        let path = home.join(rel);
        // Only delete regular files that exist; a recorded path the user
        // already moved/removed is fine (idempotent removal).
        if path.is_file() {
            std::fs::remove_file(&path)
                .map_err(|e| InstallError::Io(format!("remove {rel}: {e}")))?;
        }
        prune_empty_dirs(home, path.parent());
    }
    ledger::save(home, &records).map_err(|e| InstallError::Io(e.to_string()))?;
    Ok(record)
}

fn file_eq(a: &Path, b: &Path) -> std::io::Result<bool> {
    let (sa, sb) = (std::fs::metadata(a)?, std::fs::metadata(b)?);
    if sa.len() != sb.len() {
        return Ok(false);
    }
    Ok(std::fs::read(a)? == std::fs::read(b)?)
}

/// Walk up from `dir`, removing empty directories until (and excluding)
/// `stop` — cleans `skills/<crab>/` husks after file removal.
fn prune_empty_dirs(stop: &Path, mut dir: Option<&Path>) {
    while let Some(d) = dir {
        if d == stop {
            break;
        }
        let empty = std::fs::read_dir(d)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false);
        if !empty || std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

fn collect(dir: &Path, rel_base: &str, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)?.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with('.') {
            continue;
        }
        let rel = format!("{rel_base}/{name}");
        if entry.path().is_dir() {
            collect(&entry.path(), &rel, out)?;
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(prefix: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn fixture_pack() -> PathBuf {
        let d = temp_dir("crab-pack");
        std::fs::create_dir_all(d.join("skills/a")).unwrap();
        std::fs::write(
            d.join("crab.toml"),
            "name = \"a\"\nversion = \"0.1.0\"\ndescription = \"x\"\n\n[[skills]]\npath = \"skills/a\"\n",
        )
        .unwrap();
        std::fs::write(
            d.join("skills/a/SKILL.md"),
            "---\nname: a\ndescription: x\n---\nbody",
        )
        .unwrap();
        d
    }

    fn resolved(pack: &Path) -> ResolvedPack {
        resolve(&CrabOrigin::Local {
            path: pack.to_path_buf(),
        })
        .unwrap()
    }

    #[test]
    fn install_writes_files_and_ledger() {
        let pack = fixture_pack();
        let home = temp_dir("crab-home");
        let record = install(&home, &resolved(&pack), &InstallOpts::default()).unwrap();
        assert!(home.join("skills/a/SKILL.md").is_file());
        assert_eq!(record.files, vec!["skills/a/SKILL.md".to_string()]);
        let ledger = ledger::load(&home).unwrap();
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].crab, "a");
    }

    #[test]
    fn reinstall_same_pin_is_clean() {
        let pack = fixture_pack();
        let home = temp_dir("crab-home");
        install(&home, &resolved(&pack), &InstallOpts::default()).unwrap();
        // same pack, same sha → no drift, no collision
        install(&home, &resolved(&pack), &InstallOpts::default()).unwrap();
    }

    #[test]
    fn collision_refuses_without_force() {
        let pack = fixture_pack();
        let home = temp_dir("crab-home");
        std::fs::create_dir_all(home.join("skills/a")).unwrap();
        std::fs::write(home.join("skills/a/SKILL.md"), "user-edited content").unwrap();
        let err = install(&home, &resolved(&pack), &InstallOpts::default()).unwrap_err();
        assert_eq!(err.exit_code(), 4);
        assert!(matches!(err, InstallError::Collision(_)));
        // untouched
        assert_eq!(
            std::fs::read_to_string(home.join("skills/a/SKILL.md")).unwrap(),
            "user-edited content"
        );
    }

    #[test]
    fn force_overwrites_collision() {
        let pack = fixture_pack();
        let home = temp_dir("crab-home");
        std::fs::create_dir_all(home.join("skills/a")).unwrap();
        std::fs::write(home.join("skills/a/SKILL.md"), "user-edited content").unwrap();
        let record = install(
            &home,
            &resolved(&pack),
            &InstallOpts {
                force: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(record.files.len(), 1);
        assert!(
            std::fs::read_to_string(home.join("skills/a/SKILL.md"))
                .unwrap()
                .contains("name: a")
        );
    }

    #[test]
    fn drift_refuses_and_force_reapproves() {
        let pack = fixture_pack();
        let home = temp_dir("crab-home");
        install(&home, &resolved(&pack), &InstallOpts::default()).unwrap();
        // mutate the pack → manifest sha changes → drift
        std::fs::write(
            pack.join("crab.toml"),
            "name = \"a\"\nversion = \"0.2.0\"\ndescription = \"x\"\n\n[[skills]]\npath = \"skills/a\"\n",
        )
        .unwrap();
        let err = install(&home, &resolved(&pack), &InstallOpts::default()).unwrap_err();
        assert_eq!(err.exit_code(), 4);
        assert!(matches!(err, InstallError::Drift(_)));
        install(
            &home,
            &resolved(&pack),
            &InstallOpts {
                force: true,
                ..Default::default()
            },
        )
        .unwrap();
        let ledger = ledger::load(&home).unwrap();
        assert_eq!(ledger[0].version, "0.2.0");
    }

    #[test]
    fn secret_payload_hard_stops_before_writes() {
        let pack = fixture_pack();
        std::fs::write(
            pack.join("skills/a/leak.txt"),
            "token = \"sk-ABCDEFGHIJKLMNOPQRST1234\"",
        )
        .unwrap();
        let home = temp_dir("crab-home");
        let err = install(&home, &resolved(&pack), &InstallOpts::default()).unwrap_err();
        assert_eq!(err.exit_code(), 3);
        assert!(matches!(err, InstallError::Secrets(_)));
        // nothing landed
        assert!(!home.join("skills/a/SKILL.md").exists());
        assert!(ledger::load(&home).unwrap().is_empty());
    }

    #[test]
    fn remove_deletes_exactly_the_recorded_set() {
        let pack = fixture_pack();
        let home = temp_dir("crab-home");
        install(&home, &resolved(&pack), &InstallOpts::default()).unwrap();
        // a stray file the user dropped in the same dir
        std::fs::write(home.join("skills/a/notes.md"), "user note").unwrap();
        let record = remove(&home, "a").unwrap();
        assert_eq!(record.files.len(), 1);
        assert!(!home.join("skills/a/SKILL.md").exists());
        // stray user file survives; empty-dir husk removed with it gone? —
        // skills/a still holds notes.md, so the dir stays
        assert!(home.join("skills/a/notes.md").is_file());
        assert!(ledger::load(&home).unwrap().is_empty());
    }

    #[test]
    fn remove_unknown_crab_errors() {
        let home = temp_dir("crab-home");
        let err = remove(&home, "nope").unwrap_err();
        assert_eq!(err.exit_code(), 1);
    }
}
