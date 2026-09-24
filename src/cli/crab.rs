//! `opencrabs crab` — CLI surface for the Crab Market.
//!
//! Inspect renders the pre-approval report (and archives it under
//! `state/crab-inspections/`); install gates on a prompt (or `--yes`)
//! and maps [`InstallError`] to the Tier 1 exit-code contract:
//! 3 = secrets, 4 = collision/drift. List and remove are ledger reads.

use super::args::CrabCommands;
use crate::brain::crabs::{self, inspect::Inspection, CrabOrigin, InstallOpts};
use crate::config::opencrabs_home;
use anyhow::Result;

/// A source string is a git URL when it looks like one; otherwise a
/// local directory.
fn parse_origin(source: &str, pin: Option<&str>) -> CrabOrigin {
    let looks_like_url = source.starts_with("https://")
        || source.starts_with("git@")
        || source.starts_with("http://")
        || source.ends_with(".git");
    if looks_like_url {
        CrabOrigin::Git { url: source.to_string(), pin: pin.map(str::to_string) }
    } else {
        CrabOrigin::Local { path: std::path::PathBuf::from(source) }
    }
}

pub(crate) async fn cmd_crab(_config: &crate::config::Config, operation: CrabCommands) -> Result<()> {
    let home = opencrabs_home();
    match operation {
        CrabCommands::Inspect { source, pin } => {
            let origin = parse_origin(&source, pin.as_deref());
            let pack = crabs::install::resolve(&origin).unwrap_or_exit();
            let report = crabs::inspect::inspect_pack(&pack.dir, &pack.source.kind, pack.source.url.as_deref(), &pack.pin)
                .unwrap_or_exit();
            let rendered = render_inspection(&report);
            println!("{rendered}");
            let report_dir = home.join("state").join("crab-inspections");
            let _ = std::fs::create_dir_all(&report_dir);
            let path = report_dir.join(format!("{}.md", report.manifest.name));
            let _ = std::fs::write(&path, &rendered);
            println!("[report] {}", path.display());
            Ok(())
        }
        CrabCommands::Install { source, pin, yes, force } => {
            let origin = parse_origin(&source, pin.as_deref());
            let pack = crabs::install::resolve(&origin).unwrap_or_exit();
            let report = crabs::inspect::inspect_pack(&pack.dir, &pack.source.kind, pack.source.url.as_deref(), &pack.pin)
                .unwrap_or_exit();
            println!("{}", render_inspection(&report));
            if !report.clean() {
                // hard stop even before the prompt: the installer would
                // refuse anyway; print the exact same reason.
                crabs::install::install(&home, &pack, &InstallOpts { force, ..Default::default() })
                    .unwrap_or_exit();
            }
            if !yes && !confirm(&report) {
                println!("[crab] install declined — nothing written. Re-run with --yes to confirm.");
                return Ok(());
            }
            let record = crabs::install::install(&home, &pack, &InstallOpts { force, ..Default::default() })
                .unwrap_or_exit();
            println!(
                "[crab] installed {} v{} @ {} — {} file(s), skills: {}",
                record.crab,
                record.version,
                &record.pin[..12.min(record.pin.len())],
                record.files.len(),
                report.manifest.skill_names().join(", ")
            );
            println!("[ledger] {}", crabs::ledger::ledger_path(&home).display());
            Ok(())
        }
        CrabCommands::List => {
            let records = crabs::ledger::load(&home).unwrap_or_exit();
            if records.is_empty() {
                println!("no crabs installed — try `opencrabs crab inspect <source>`");
                return Ok(());
            }
            println!("{:<24} {:<10} {:<13} {:<12} SOURCE", "CRAB", "VERSION", "PIN", "INSTALLED");
            for r in &records {
                let url = r.source.url.clone().unwrap_or_default();
                println!(
                    "{:<24} {:<10} {:<13} {:<12} {} {}",
                    r.crab,
                    r.version,
                    &r.pin[..12.min(r.pin.len())],
                    r.installed_at.split('T').next().unwrap_or("?"),
                    r.source.kind,
                    url
                );
            }
            Ok(())
        }
        CrabCommands::Search { query, index } => {
            let url = index.as_deref().unwrap_or(crabs::DEFAULT_MARKET_INDEX);
            let entries = crabs::market::fetch_index(url).await.unwrap_or_exit();
            let hits = crabs::market::search(&entries, &query);
            if hits.is_empty() {
                println!("no market entries match \"{query}\" — {url}");
                return Ok(());
            }
            println!("{:<24} {:<10} {:<10} {}", "CRAB", "VERSION", "CATEGORY", "DESCRIPTION");
            for e in &hits {
                println!(
                    "{:<24} {:<10} {:<10} {}",
                    e.name,
                    e.version,
                    e.category.clone().unwrap_or_else(|| "-".into()),
                    e.description
                );
            }
            println!("\ninstall with: opencrabs crab inspect {} → crab install", hits[0].repo);
            Ok(())
        }
        CrabCommands::Updates { index } => {
            let url = index.as_deref().unwrap_or(crabs::DEFAULT_MARKET_INDEX);
            let report = crabs::market::check_updates(&home, url).await;
            if report.is_empty() {
                println!("no crabs installed — nothing to check.");
                return Ok(());
            }
            let mut drift = 0;
            for r in &report {
                match &r.status {
                    crabs::UpdateStatus::UpToDate => {
                        println!("{:<24} v{:<8} up to date", r.crab, r.installed_version)
                    }
                    crabs::UpdateStatus::Drift { current } => {
                        drift += 1;
                        println!(
                            "{:<24} v{:<8} DRIFT → {} (re-inspect, then --force to re-approve)",
                            r.crab,
                            r.installed_version,
                            &current[..12.min(current.len())]
                        );
                    }
                    crabs::UpdateStatus::Unknown(why) => {
                        println!("{:<24} v{:<8} ? {why}", r.crab, r.installed_version)
                    }
                }
            }
            println!(
                "\nreport only — nothing was applied.{}",
                if drift > 0 { format!(" {drift} crab(s) drifted.") } else { " all current.".into() }
            );
            Ok(())
        }
        CrabCommands::Remove { name } => {
            let record = crabs::install::remove(&home, &name).unwrap_or_exit();
            println!("[crab] removed {} v{} — {} file(s) deleted", record.crab, record.version, record.files.len());
            for f in &record.files {
                println!("  - {f}");
            }
            Ok(())
        }
    }
}

/// Prompt y/N on the inspection. Non-TTY stdin (agent, cron, pipe) reads
/// EOF → declines — approval must be an explicit `--yes`.
fn confirm(report: &Inspection) -> bool {
    use std::io::{BufRead, Write};
    print!(
        "\ninstall {} v{} ({})? [y/N] ",
        report.manifest.name,
        report.manifest.version,
        report.source_kind
    );
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let n = std::io::stdin().lock().read_line(&mut line).unwrap_or(0);
    n > 0 && line.trim().eq_ignore_ascii_case("y")
}

/// Render the inspection as compact markdown (report parity with the
/// Tier 1 bb layer: manifest → files → skills → secrets → verdict).
fn render_inspection(r: &Inspection) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Crab inspection — {}\n\n", r.manifest.name));
    out.push_str(&format!(
        "source: {} {}\nversion: {} · category: {}\npin: {}\n\n",
        r.source_kind,
        r.source_url.clone().unwrap_or_default(),
        r.manifest.version,
        r.manifest.category.clone().unwrap_or_else(|| "-".into()),
        r.pin
    ));
    out.push_str(&format!("## Files ({})\n", r.files.len()));
    for f in &r.files {
        out.push_str(&format!("- `{f}`\n"));
    }
    out.push_str("\n## Skills\n");
    for s in &r.skills {
        out.push_str(&format!(
            "- **{}** — {} ({} file(s))\n",
            s.path,
            s.description.clone().unwrap_or_else(|| "⚠ frontmatter unreadable".into()),
            s.file_count
        ));
        if !s.blast.is_empty() {
            let b: Vec<String> = s.blast.iter().map(|(f, t)| format!("{}: {}", f, t.join(","))).collect();
            out.push_str(&format!("  - blast: {}\n", b.join(" · ")));
        }
    }
    out.push_str("\n## Secret scan\n");
    if r.secret_hits.is_empty() {
        out.push_str("CLEAN\n");
    } else {
        out.push_str("HITS (install will refuse):\n");
        for h in &r.secret_hits {
            out.push_str(&format!("- {}:{} ~ {}\n", h.file, h.line, h.pattern));
        }
    }
    out.push_str("\n## Verdict\n");
    out.push_str(if r.clean() {
        "SAFE TO REVIEW — nothing lands until you approve."
    } else {
        "REFUSED — credential-shaped strings in payload."
    });
    out
}

/// Print and exit with the Tier 1 exit-code contract.
trait InstallExit<T> {
    fn unwrap_or_exit(self) -> T;
}

impl<T> InstallExit<T> for Result<T, crabs::install::InstallError> {
    fn unwrap_or_exit(self) -> T {
        match self {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(e.exit_code());
            }
        }
    }
}

impl<T> InstallExit<T> for Result<T, anyhow::Error> {
    fn unwrap_or_exit(self) -> T {
        match self {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[crab:io] {e:#}");
                std::process::exit(1);
            }
        }
    }
}

impl<T> InstallExit<T> for Result<T, String> {
    fn unwrap_or_exit(self) -> T {
        match self {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[crab:failed] {e}");
                std::process::exit(2);
            }
        }
    }
}
