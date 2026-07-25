# Verification notes

This document captures the current verification pyramid for the desktop frontend and native Tauri shell.

## Scope

The desktop app consists of:

- a Dioxus WebAssembly frontend crate at `desktop/`
- a native Tauri 2 shell/backend crate at `desktop/src-tauri/`

Verification is only meaningful when commands are run from the correct crate directories.

## Automated coverage currently present

### Frontend crate (`desktop/`)

- DTO contract smoke tests in `tests/models_contract.rs`
- integration smoke checks in `tests/integration_smoke.rs`
- crate/unit suites totaling **11 passing tests**

### Native shell (`desktop/src-tauri/`)

- file/path containment tests
- brain-file guard tests
- config allowlist tests
- chat/cancellation tests
- onboarding/channel/update/voice truthfulness tests
- crate/unit suites totaling **28 passing tests**

## Correct verification commands

### Frontend

Run from `desktop/`:

```text
cargo fmt --check
cargo check --message-format short
cargo test --message-format short
trunk build --release -v
```

### Native shell

Run from `desktop/src-tauri/`:

```text
cargo fmt --check
cargo check --message-format short
cargo test --message-format short
cargo tauri build
```

### Full release gate

Run from `desktop/`:

```text
./scripts/release-verify.sh
```

## Verified evidence (2026-07-25)

The following commands were run successfully in the correct directories:

- `cargo test --message-format short` in `desktop/` → **11 passed**
- `cargo test --message-format short` in `desktop/src-tauri/` → **28 passed**
- `cargo build --message-format short` in `desktop/` → passed
- `cargo build --message-format short` in `desktop/src-tauri/` → passed
- `trunk build --release -v` in `desktop/` → passed
- `cargo tauri build` in `desktop/src-tauri/` → passed

Produced artifacts:

- `src-tauri/target/release/bundle/macos/OpenCrabs.app`
- `src-tauri/target/release/bundle/dmg/OpenCrabs_0.1.0_aarch64.dmg`

## Important guardrail

A failed Cargo command run from `/Users/moe` or another parent directory without the relevant `Cargo.toml` is an invocation error, not release evidence for this desktop app.

## What this verification does *not* claim

- platform signing/notarization is complete
- accessibility review is complete
- every loading/empty/offline state has been UX-reviewed
- desktop auto-install updates are supported

## Remaining external gates

- sign and notarize/staple macOS artifacts before stable release
- sign Windows installers and publish checksums for each release
- run the release gate on each target platform before promotion
- preserve release logs/evidence alongside release notes
