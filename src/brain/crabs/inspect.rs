//! Crab inspect — assemble the pre-approval report.
//!
//! Pure assembly: reads a resolved pack, combines the manifest, the
//! payload file list, the [`crate::brain::skills`] frontmatter parse,
//! the secret scan, and the blast radius into one structured
//! [`Inspection`]. Rendering belongs to the CLI layer; this is data.

use super::CrabManifest;
use super::scan::{SecretHit, blast_radius, secret_scan};
use std::path::Path;

/// One skill as seen by the inspect gate.
#[derive(Debug, Clone)]
pub struct InspectedSkill {
    /// Path as declared in the manifest.
    pub path: String,
    /// Parsed frontmatter (None = dir missing or SKILL.md unreadable —
    /// surfaced as an error by the caller, not silently dropped).
    pub name: Option<String>,
    pub description: Option<String>,
    /// Number of files under the skill dir.
    pub file_count: usize,
    /// Elevated tool families referenced in the body (report-only).
    pub blast: Vec<(&'static str, Vec<&'static str>)>,
}

/// The full pre-approval report. Nothing in here mutates anything.
#[derive(Debug, Clone)]
pub struct Inspection {
    pub manifest: CrabManifest,
    pub source_kind: String,
    pub source_url: Option<String>,
    pub pin: String,
    /// Payload files relative to the pack root, sorted.
    pub files: Vec<String>,
    pub skills: Vec<InspectedSkill>,
    pub secret_hits: Vec<SecretHit>,
}

impl Inspection {
    /// The inspect verdict: true when no credential-shaped string was
    /// found. Blast radius does NOT gate — it informs.
    pub fn clean(&self) -> bool {
        self.secret_hits.is_empty()
    }
}

/// Build the inspection for a resolved pack dir.
///
/// Skill dirs missing from disk or with unreadable SKILL.md are
/// reported as `None` name — the installer refuses them; the report
/// shows the reviewer why.
pub fn inspect_pack(
    pack_dir: &Path,
    source_kind: &str,
    source_url: Option<&str>,
    pin: &str,
) -> Result<Inspection, String> {
    let manifest = CrabManifest::load(&pack_dir.join("crab.toml"))?;

    let mut files = Vec::new();
    let mut skills = Vec::new();
    for entry in &manifest.skills {
        let skill_dir = pack_dir.join(&entry.path);
        let mut skill_files = Vec::new();
        // Tolerant walk: a missing dir surfaces as an empty file list +
        // unreadable frontmatter in the report; the installer is the one
        // that refuses. The report must never panic on a broken pack.
        let _ = collect_files(&skill_dir, &entry.path, &mut skill_files);
        files.extend(skill_files.iter().cloned());

        let skill_md = skill_dir.join("SKILL.md");
        let (name, description, blast) = match std::fs::read_to_string(&skill_md) {
            Ok(raw) => {
                let (n, d, b) = match crate::brain::skills::Skill::parse(
                    entry.path.rsplit('/').next().unwrap_or("?"),
                    &raw,
                    crate::brain::skills::SkillSource::User,
                ) {
                    Ok(s) => (Some(s.name), Some(s.description), blast_radius(&raw)),
                    Err(_) => (None, None, blast_radius(&raw)),
                };
                (n, d, b)
            }
            Err(_) => (None, None, Vec::new()),
        };
        skills.push(InspectedSkill {
            path: entry.path.clone(),
            name,
            description,
            file_count: skill_files.len(),
            blast,
        });
    }
    files.sort();
    files.dedup();

    let secret_hits = secret_scan(pack_dir, &files);

    Ok(Inspection {
        manifest,
        source_kind: source_kind.to_string(),
        source_url: source_url.map(str::to_string),
        pin: pin.to_string(),
        files,
        skills,
        secret_hits,
    })
}

/// Recursively list files under `dir`, as `<rel_base>/<...>` paths.
/// Hidden entries (`.DS_Store`, `.git`) are skipped.
fn collect_files(dir: &Path, rel_base: &str, out: &mut Vec<String>) -> std::io::Result<()> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let rel = format!("{rel_base}/{name}");
        if path.is_dir() {
            collect_files(&path, &rel, out)?;
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_pack() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("crab-inspect-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("skills/competitor-watch")).unwrap();
        std::fs::write(
            dir.join("crab.toml"),
            r#"name = "competitor-watch"
version = "0.1.0"
description = "Daily page-diff"

[[skills]]
path = "skills/competitor-watch"
"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("skills/competitor-watch/SKILL.md"),
            "---\nname: competitor-watch\ndescription: page diff daily\n---\nUse bash and http_request to diff pages.",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("skills/competitor-watch/scripts")).unwrap();
        std::fs::write(
            dir.join("skills/competitor-watch/scripts/diff.sh"),
            "#!/bin/sh\ndiff old new\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn inspects_manifest_files_and_frontmatter() {
        let dir = fixture_pack();
        let report = inspect_pack(&dir, "local", Some(dir.to_str().unwrap()), "abc123").unwrap();
        assert_eq!(report.manifest.name, "competitor-watch");
        assert_eq!(report.files.len(), 2); // SKILL.md + scripts/diff.sh
        assert_eq!(report.skills.len(), 1);
        assert_eq!(report.skills[0].name.as_deref(), Some("competitor-watch"));
        assert_eq!(report.skills[0].file_count, 2);
        // body mentions bash + http_request → shell + network
        let families: Vec<&str> = report.skills[0].blast.iter().map(|(f, _)| *f).collect();
        assert!(families.contains(&"shell"));
        assert!(families.contains(&"network"));
        assert!(report.clean());
    }

    #[test]
    fn secret_hits_make_it_unclean() {
        let dir = fixture_pack();
        std::fs::write(
            dir.join("skills/competitor-watch/scripts/diff.sh"),
            "TOKEN=ghp_0123456789abcdefghijklmnopqrstuvwxyz\n",
        )
        .unwrap();
        let report = inspect_pack(&dir, "local", None, "pin").unwrap();
        assert!(!report.clean());
        assert_eq!(report.secret_hits[0].pattern, "github-token");
    }

    #[test]
    fn missing_skill_dir_is_reported_not_panicked() {
        let dir = fixture_pack();
        std::fs::remove_dir_all(dir.join("skills/competitor-watch")).unwrap();
        let report = inspect_pack(&dir, "local", None, "pin").unwrap();
        assert_eq!(report.skills[0].name, None);
        assert!(report.files.is_empty());
    }

    #[test]
    fn hidden_files_are_skipped() {
        let dir = fixture_pack();
        std::fs::write(dir.join("skills/competitor-watch/.DS_Store"), "junk").unwrap();
        let report = inspect_pack(&dir, "local", None, "pin").unwrap();
        assert_eq!(report.files.len(), 2);
    }
}
