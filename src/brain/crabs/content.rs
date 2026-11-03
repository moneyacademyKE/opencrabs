//! Per-crab content identity — the fix for the HEAD-pin quirk.
//!
//! A crab's pin used to be the market repo's HEAD commit, so ANY market
//! commit (someone else's new crab, a README bump) drifted EVERY installed
//! crab and forced a hand re-pin. Content identity replaces that: a crab
//! is "current" when its recorded file set hashes the same upstream as on
//! disk, regardless of what the repo's HEAD is doing.
//!
//! The hash is positional (sorted relpath + byte-length prefix + bytes) so
//! file sets and boundaries can't alias. `None` from [`content_sha`] means
//! "a recorded file is unreadable at this base" — callers decide whether
//! that is drift (upstream) or an unverifiable install (local).

use sha2::{Digest, Sha256};
use std::path::Path;

/// Split a `<repo-url>#<subdir>` source into (repo, subdir). The fragment
/// is the pack's dir inside the repo (market monorepo layout); no
/// fragment = the pack lives at the repo root.
pub fn split_repo_fragment(url: &str) -> (String, Option<String>) {
    match url.split_once('#') {
        Some((r, s)) if !s.is_empty() => {
            (r.to_string(), Some(s.trim_start_matches('/').to_string()))
        }
        _ => (url.to_string(), None),
    }
}

/// sha256 over the recorded file set, read under `base`. Files are hashed
/// in sorted relpath order, each as `relpath \0 len(u64 LE) bytes`.
/// Returns `None` when any recorded file is missing/unreadable.
pub fn content_sha(files: &[String], base: &Path) -> Option<String> {
    let mut sorted: Vec<&String> = files.iter().collect();
    sorted.sort();
    let mut h = Sha256::new();
    for rel in sorted {
        let bytes = std::fs::read(base.join(rel.as_str())).ok()?;
        h.update(rel.as_bytes());
        h.update([0u8]);
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(&bytes);
    }
    Some(hex(&h.finalize()))
}

fn hex(digest: &[u8]) -> String {
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn seed(base: &Path) -> Vec<String> {
        std::fs::create_dir_all(base.join("skills/a")).unwrap();
        std::fs::write(base.join("skills/a/SKILL.md"), "body").unwrap();
        std::fs::write(base.join("skills/a/extra.md"), "more").unwrap();
        vec!["skills/a/SKILL.md".into(), "skills/a/extra.md".into()]
    }

    #[test]
    fn same_content_same_sha_regardless_of_order() {
        let a = scratch("crab-ct");
        let b = scratch("crab-ct");
        let files = seed(&a);
        let mut rev = seed(&b);
        rev.reverse();
        assert_eq!(content_sha(&files, &a), content_sha(&rev, &b));
    }

    #[test]
    fn byte_change_or_file_set_change_changes_sha() {
        let a = scratch("crab-ct");
        let files = seed(&a);
        let base_sha = content_sha(&files, &a).unwrap();
        std::fs::write(a.join("skills/a/SKILL.md"), "body!").unwrap();
        assert_ne!(content_sha(&files, &a).unwrap(), base_sha);

        let b = scratch("crab-ct");
        seed(&b);
        let subset = vec!["skills/a/SKILL.md".to_string()];
        assert_ne!(
            content_sha(&subset, &b).unwrap(),
            content_sha(&files, &b).unwrap()
        );
    }

    #[test]
    fn missing_file_is_none_not_a_panic() {
        let a = scratch("crab-ct");
        assert_eq!(content_sha(&["nope/x.md".into()], &a), None);
    }

    #[test]
    fn fragment_split_matches_install_semantics() {
        assert_eq!(
            split_repo_fragment("https://x/y.git#crabs/a"),
            ("https://x/y.git".into(), Some("crabs/a".into()))
        );
        assert_eq!(
            split_repo_fragment("https://x/y.git#/crabs/a"),
            ("https://x/y.git".into(), Some("crabs/a".into()))
        );
        assert_eq!(
            split_repo_fragment("https://x/y.git"),
            ("https://x/y.git".into(), None)
        );
    }
}
