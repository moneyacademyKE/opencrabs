//! Crab Market — installable skill packs ("crabs"), native to the binary.
//!
//! A **crab** is a portable skill pack: a `crab.toml` manifest plus one or
//! more skill directories, distributed from a git repo (the Crab Market) or
//! a local path. Installing a crab copies its skill dirs into
//! `~/.opencrabs/skills/`, pins the source commit in the ledger, and the
//! skill becomes live through the ordinary skills loader — zero new
//! runtime code.
//!
//! Layout:
//!
//! ```text
//! <crab root>/
//! ├── crab.toml                  ← manifest: name, version, skills
//! └── skills/
//!     └── <skill-name>/
//!         └── SKILL.md
//! ```
//!
//! ## Policy note (the one deliberate exception)
//!
//! `skills.rs` documents "writes never come from the binary". The crab
//! installer is the single sanctioned write path into `skills/`:
//! approval-gated (`crab install` inspects first, `--yes` confirms),
//! ledger-tracked (`state/crabs.json`), and reversible (`crab remove`
//! deletes exactly the recorded files).
//!
//! ## Modules
//!
//! - [`manifest`] — parse/validate `crab.toml`
//! - [`ledger`] — the pinning record of installed crabs
//! - `install` — inspect → gate → copy → pin (and remove)
//! - `market` — remote index fetch, search, drift report

pub mod inspect;
pub mod install;
pub mod ledger;
pub mod manifest;
pub mod market;
pub mod scan;

pub use install::{CrabOrigin, InstallError, InstallOpts, ResolvedPack};
pub use ledger::{CrabRecord, CrabSource};
pub use manifest::CrabManifest;
pub use market::{DEFAULT_MARKET_INDEX, MarketEntry, UpdateReport, UpdateStatus};
