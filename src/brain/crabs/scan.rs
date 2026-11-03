//! Crab content scan — the inspect gate's eyes.
//!
//! Two pure-text analyses over a pack's payload, ported from the Tier 1
//! bb layer with the same patterns:
//!
//! - [`secret_scan`] — credential-shaped strings. HITS are a hard stop
//!   (install refuses); the point is a pack must never ship live secrets.
//! - [`blast_radius`] — which elevated tool families a skill body
//!   references. Report-only: it tells the reviewer what the crab could
//!   touch, it never blocks. `cron` was dropped from the schedule radar
//!   (bare-token noise — any scheduled crab mentions it); `cron_manage`
//!   covers the real signal.

use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

/// One credential-shaped hit: file, 1-based line, pattern label.
#[derive(Debug, Clone, PartialEq)]
pub struct SecretHit {
    pub file: String,
    pub line: usize,
    pub pattern: &'static str,
}

static SECRET_PATTERNS: &[(&str, &str)] = &[
    ("openai-key", r"sk-[A-Za-z0-9]{20,}"),
    ("slack-token", r"xox[abprs]-[A-Za-z0-9-]{10,}"),
    ("github-token", r"(?:ghp|gho|ghu|ghs)_[A-Za-z0-9]{36}"),
    ("google-key", r"AIza[0-9A-Za-z_\-]{35}"),
    ("private-key", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    (
        "generic-secret",
        r#"(?i)(?:api[_-]?key|secret|password|auth[_-]?token)\s*[:=]\s*["'][^"']{12,}"#,
    ),
];

static RADAR: &[(&str, &[&str])] = &[
    (
        "network",
        &[
            "http_request",
            "web_scrape",
            "exa_search",
            "web_search",
            "browser_navigate",
            "browser_click",
            "a2a_send",
        ],
    ),
    (
        "shell",
        &["bash", "spawn_agent", "execute_code", "subprocess"],
    ),
    (
        "channel-out",
        &[
            "telegram_send",
            "discord_send",
            "slack_send",
            "whatsapp_send",
            "trello_send",
        ],
    ),
    (
        "github-write",
        &["gh pr", "gh issue", "gh release", "git push"],
    ),
    ("schedule", &["cron_manage", "schedule"]),
    (
        "destructive",
        &["rm -rf", "git reset", "git checkout --", " trash "],
    ),
];

fn secret_regexes() -> &'static Vec<(&'static str, Regex)> {
    static RE: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    RE.get_or_init(|| {
        SECRET_PATTERNS
            .iter()
            .map(|(label, pat)| (*label, Regex::new(pat).expect("static secret pattern")))
            .collect()
    })
}

/// Scan payload files (paths relative to the pack root) for
/// credential-shaped strings. Binary/unreadable files are skipped; a
/// null byte in the first 8 KB marks the file binary and skips it too.
pub fn secret_scan(pack_dir: &Path, files: &[String]) -> Vec<SecretHit> {
    let mut hits = Vec::new();
    for rel in files {
        let full = pack_dir.join(rel);
        let Ok(raw) = std::fs::read(&full) else {
            continue;
        };
        if raw.contains(&0u8) || raw.len() > 8 * 1024 * 1024 {
            continue; // binary or oversized — not a text secret surface
        }
        let Ok(text) = String::from_utf8(raw) else {
            continue;
        };
        for (label, re) in secret_regexes() {
            for line in text.lines().enumerate() {
                if re.is_match(line.1) {
                    hits.push(SecretHit {
                        file: rel.clone(),
                        line: line.0 + 1,
                        pattern: label,
                    });
                }
            }
        }
    }
    hits
}

/// Which elevated tool families does this skill body reference?
/// Report-only — keys with hits, in declaration order.
pub fn blast_radius(skill_body: &str) -> Vec<(&'static str, Vec<&'static str>)> {
    RADAR
        .iter()
        .filter_map(|(family, tokens)| {
            let hits: Vec<&str> = tokens
                .iter()
                .copied()
                .filter(|t| skill_body.contains(t))
                .collect();
            if hits.is_empty() {
                None
            } else {
                Some((*family, hits))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_scan_flags_each_pattern_family() {
        let dir = std::env::temp_dir().join(format!("crab-scan-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("skills/a")).unwrap();
        let cases = [
            ("openai-key", "key = \"sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ1234\""),
            (
                "github-token",
                "token: ghp_0123456789abcdefghijklmnopqrstuvwxyz",
            ),
            ("private-key", "-----BEGIN RSA PRIVATE KEY-----"),
            ("generic-secret", "api_key: 'supersecretvalue123'"),
        ];
        for (i, (_, line)) in cases.iter().enumerate() {
            let f = format!("skills/a/file{i}.txt");
            std::fs::write(dir.join(&f), line).unwrap();
        }
        let files: Vec<String> = (0..cases.len())
            .map(|i| format!("skills/a/file{i}.txt"))
            .collect();
        let hits = secret_scan(&dir, &files);
        let labels: Vec<&str> = hits.iter().map(|h| h.pattern).collect();
        for (want, _) in &cases {
            assert!(labels.contains(want), "missed {want}: {labels:?}");
        }
    }

    #[test]
    fn secret_scan_clean_text_passes() {
        let dir = std::env::temp_dir().join(format!("crab-scan-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("skills/a")).unwrap();
        std::fs::write(
            dir.join("skills/a/SKILL.md"),
            "---\nname: a\ndescription: normal text\n---\nbody",
        )
        .unwrap();
        assert!(secret_scan(&dir, &["skills/a/SKILL.md".into()]).is_empty());
    }

    #[test]
    fn secret_scan_skips_binary() {
        let dir = std::env::temp_dir().join(format!("crab-scan-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("skills/a")).unwrap();
        std::fs::write(
            dir.join("skills/a/img.png"),
            [0x89, 0x50, 0x4E, 0x47, 0x00, 0x0D],
        )
        .unwrap();
        assert!(secret_scan(&dir, &["skills/a/img.png".into()]).is_empty());
    }

    #[test]
    fn blast_radius_maps_families() {
        let body =
            "use bash to run cron_manage, then telegram_send the result; git push at the end";
        let b = blast_radius(body);
        let families: Vec<&str> = b.iter().map(|(f, _)| *f).collect();
        assert!(families.contains(&"shell"));
        assert!(families.contains(&"channel-out"));
        assert!(families.contains(&"github-write"));
        assert!(families.contains(&"schedule"));
        assert!(!families.contains(&"network"));
    }
}
