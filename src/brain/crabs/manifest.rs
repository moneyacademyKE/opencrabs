//! Crab manifest — parse and validate `crab.toml`.
//!
//! The manifest is the allowlist: a crab installs exactly the skill dirs it
//! declares, nothing else. No prefix conventions, no implicit payload — the
//! data is the contract.
//!
//! ```toml
//! name = "competitor-watch"
//! version = "0.1.0"
//! description = "Daily page-diff over watched URLs; reports only changes"
//! category = "monitor"              # optional, for market browsing
//! homepage = "https://github.com/…"  # optional
//!
//! [[skills]]
//! path = "skills/competitor-watch"   # a dir containing SKILL.md
//! ```
//!
//! Unknown keys are tolerated (forward-compat, same policy as skill
//! frontmatter); the known ones are validated strictly.

use std::path::Path;

/// One skill dir shipped by a crab. `path` is relative to the crab root.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SkillEntry {
    pub path: String,
}

/// A parsed, shape-validated `crab.toml`.
///
/// Disk validation (does `skills/<name>/SKILL.md` actually exist in the
/// pack?) deliberately lives in `install.rs` — this type is pure data, so
/// it parses and round-trips without fixtures.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CrabManifest {
    /// Slug: lowercase alphanumerics and hyphens (`competitor-watch`).
    pub name: String,
    /// Version string; not semver-parsed, only non-empty.
    pub version: String,
    /// One-line summary shown by `crab list` and the market index.
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Skill dirs to install. At least one.
    pub skills: Vec<SkillEntry>,
}

impl CrabManifest {
    /// Parse and shape-validate a `crab.toml` blob.
    pub fn parse_str(raw: &str) -> Result<Self, String> {
        let m: Self = toml::from_str(raw).map_err(|e| format!("crab.toml: {e}"))?;
        m.validate()?;
        Ok(m)
    }

    /// Read + parse + validate `crab.toml` at `path`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Self::parse_str(&raw)
    }

    /// Shape validation: slug name, non-empty version/description, at
    /// least one skill dir, every skill dir relative, `..`-free, and
    /// rooted under `skills/`.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("crab.toml: 'name' is required".into());
        }
        if !self
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            || self.name.starts_with('-')
            || self.name.ends_with('-')
        {
            return Err(format!(
                "crab.toml: 'name' must be a lowercase slug (a-z, 0-9, '-'), got \"{}\"",
                self.name
            ));
        }
        if self.version.trim().is_empty() {
            return Err("crab.toml: 'version' is required".into());
        }
        if self.description.trim().is_empty() {
            return Err("crab.toml: 'description' is required".into());
        }
        if self.skills.is_empty() {
            return Err("crab.toml: at least one [[skills]] entry is required".into());
        }
        for s in &self.skills {
            validate_skill_path(&s.path)?;
        }
        Ok(())
    }

    /// The skill names this crab provides: the leaf dir of each entry
    /// (`skills/competitor-watch` → `competitor-watch`). These are the
    /// `/slash` names that go live on install.
    pub fn skill_names(&self) -> Vec<&str> {
        self.skills
            .iter()
            .filter_map(|s| s.path.trim_end_matches('/').rsplit('/').next())
            .collect()
    }
}

/// A skill path must be relative, `..`-free, at least `skills/<name>`,
/// with a non-empty slug leaf.
fn validate_skill_path(path: &str) -> Result<(), String> {
    let p = path.trim_end_matches('/');
    if p.is_empty() {
        return Err("crab.toml: empty [[skills]] path".into());
    }
    if Path::new(p).is_absolute() {
        return Err(format!(
            "crab.toml: skill path must be relative, got \"{path}\""
        ));
    }
    let parts: Vec<&str> = p.split('/').collect();
    if parts
        .iter()
        .any(|c| c.is_empty() || *c == ".." || *c == ".")
    {
        return Err(format!(
            "crab.toml: malformed skill path \"{path}\" (empty or '.'/'..' components)"
        ));
    }
    if parts.first() != Some(&"skills") || parts.len() < 2 {
        return Err(format!(
            "crab.toml: skill path must be under skills/ (\"skills/<name>\"), got \"{path}\""
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
name = "competitor-watch"
version = "0.1.0"
description = "Daily page-diff over watched URLs"
category = "monitor"

[[skills]]
path = "skills/competitor-watch"
"#;

    #[test]
    fn parses_valid_manifest() {
        let m = CrabManifest::parse_str(VALID).unwrap();
        assert_eq!(m.name, "competitor-watch");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.category.as_deref(), Some("monitor"));
        assert_eq!(m.skill_names(), vec!["competitor-watch"]);
    }

    #[test]
    fn round_trips_through_toml() {
        let m = CrabManifest::parse_str(VALID).unwrap();
        let raw = toml::to_string(&m).unwrap();
        let again = CrabManifest::parse_str(&raw).unwrap();
        assert_eq!(m, again);
    }

    #[test]
    fn unknown_keys_are_tolerated() {
        let raw = format!("{VALID}\nfuture_field = \"whatever\"\n");
        assert!(CrabManifest::parse_str(&raw).is_ok());
    }

    #[test]
    fn rejects_invalid_manifests() {
        for (raw, why) in [
            (
                "version = \"1\"\ndescription = \"x\"\n[[skills]]\npath = \"skills/a\"\n",
                "missing name",
            ),
            (
                "name = \"Bad Name\"\nversion = \"1\"\ndescription = \"x\"\n[[skills]]\npath = \"skills/a\"\n",
                "uppercase name",
            ),
            (
                "name = \"a\"\ndescription = \"x\"\n[[skills]]\npath = \"skills/a\"\n",
                "missing version",
            ),
            (
                "name = \"a\"\nversion = \"1\"\n[[skills]]\npath = \"skills/a\"\n",
                "missing description",
            ),
            (
                "name = \"a\"\nversion = \"1\"\ndescription = \"x\"\n",
                "no skills",
            ),
            (
                "name = \"a\"\nversion = \"1\"\ndescription = \"x\"\n[[skills]]\npath = \"scripts/a\"\n",
                "skill not under skills/",
            ),
            (
                "name = \"a\"\nversion = \"1\"\ndescription = \"x\"\n[[skills]]\npath = \"skills/../etc\"\n",
                "traversal",
            ),
            (
                "name = \"a\"\nversion = \"1\"\ndescription = \"x\"\n[[skills]]\npath = \"/abs/skills/a\"\n",
                "absolute path",
            ),
            ("this is not toml at all [[", "unparseable"),
        ] {
            let err = CrabManifest::parse_str(raw)
                .err()
                .unwrap_or_else(|| panic!("should reject: {why}"));
            assert!(
                err.contains("crab.toml"),
                "error should name the file: {err}"
            );
        }
    }

    #[test]
    fn skill_paths_trim_trailing_slash() {
        let raw = VALID.replace("skills/competitor-watch", "skills/competitor-watch/");
        let m = CrabManifest::parse_str(&raw).unwrap();
        assert_eq!(m.skill_names(), vec!["competitor-watch"]);
    }
}
