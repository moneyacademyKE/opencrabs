# Release readiness checklist

Use this before calling the desktop app production-ready.

## Security

- [x] CSP keeps `style-src 'self'`; its only inline-script exception is a SHA-256 hash for the current Trunk WebAssembly bootstrap module. Development disables Trunk autoreload so it does not require an additional inline reload client. The bootstrap hash was revalidated after native runtime inspection on 2026-07-25.
- [x] no raw HTML rendering of untrusted model/provider output
- [x] file read/write/list commands enforce allowlisted roots and containment
- [x] config writes remain explicitly allowlisted
- [x] secrets are masked in UI/logs and only written through constrained codepaths
- [x] Tauri capabilities/plugins reviewed for least privilege; unused shell/dialog/fs plugins and broad capability grants were removed

## Backend truthfulness

- [x] every exposed desktop command is marked ready / partial / unsupported in `desktop-command-contract.md`
- [x] unsupported commands fail honestly, not with fake success
- [x] health checks report observable filesystem/config/tool state rather than hardcoded placeholders
- [x] update/install UX matches actual capability
- [x] voice UX matches actual capability

## Verification

- [x] `cargo fmt --check`, `cargo check --message-format short`, and `cargo test --message-format short` pass in `desktop/` (2026-07-25)
- [x] `trunk build --release -v` passes in `desktop/` (2026-07-25)
- [x] `cargo fmt --check`, `cargo check --message-format short`, and `cargo test --message-format short` pass in `desktop/src-tauri/` (2026-07-25)
- [x] `cargo tauri build` passes in `desktop/src-tauri/` and produces `.app` and `.dmg` artifacts (2026-07-25)
- [ ] Run `./scripts/release-verify.sh` from `desktop/` on each release target and attach its output to release notes

## Verification evidence

The commands below must be run from their named project directories. A Cargo invocation from a parent directory that does not contain the relevant `Cargo.toml` is not release evidence.

- Frontend crate: run the frontend commands from `desktop/`.
- Native shell: run the native commands from `desktop/src-tauri/`.
- Record the command output, target platform, and artifact version with the release notes.

## Distribution

- [ ] lane chosen: `dev`, `beta`, or `stable`
- [ ] signed artifacts produced for the target platform(s)
- [ ] notarization/stapling complete where required
- [ ] release notes written
- [ ] checksums generated and published
- [ ] rollback story documented

## Observability

- [x] frontend failures are visible and actionable
- [x] backend command failures are propagated with enough context for diagnostics
- [x] bounded, credential-redacted diagnostics snapshot exists
- [x] crash/debug bundle export is explicitly deferred; support uses the bounded snapshot and release runbook

## UX state

- [x] full pane split-tree, focus, and pane/session mapping persist across restart
- [x] last active route and selected session persist with safe fallback for invalid state
- [ ] loading / empty / offline states are deliberate across every panel
- [x] dangerous and partial actions are explicitly communicated
- [ ] accessibility baseline is reviewed
