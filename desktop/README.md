# OpenCrabs Desktop UI

This frontend is now a Dioxus WebAssembly app embedded in Tauri 2.

## Active structure

- `src/main.rs` — Dioxus entry point
- `src/app.rs` — routed shell and panel components
- `src/bridge.rs` — Tauri invoke bridge for the wasm frontend
- `src/models.rs` — frontend DTOs matching Tauri command payloads
- `src/css/app.css` — active desktop stylesheet
- `src-tauri/` — native Tauri backend and command handlers

## Frontend contract note

The desktop DTO/command contract is documented in:

- `desktop-command-contract.md`

That file is the current truth source for which desktop commands are ready, partial, or unsupported.

## Toolchain and bootstrap

This project now treats toolchain versions as part of the product surface.

### Required tools

| Tool | Version expectation | Why |
|---|---|---|
| Rust | `1.91.0` | Matches the native Tauri crate `rust-version` and avoids drift between frontend/backend builds |
| `trunk` | `0.21.x` | Builds and serves the Dioxus WASM frontend |
| `tauri-cli` | `2.x` | Provides the `cargo tauri` subcommand used to run and bundle the desktop shell |
| Node | not required | This desktop path is Rust + Trunk, not a JS build stack |

### Recommended install

```text
rustup toolchain install 1.91.0
rustup override set 1.91.0
cargo install trunk --version ^0.21 --locked
cargo install tauri-cli --version ^2 --locked
```

### Local development

From `desktop/`:

```text
trunk serve --port 8080 --no-autoreload
```

From `desktop/src-tauri/` in another terminal:

```text
cargo tauri dev
```

Or let Tauri drive Trunk automatically:

```text
cd src-tauri && cargo tauri dev
```

### Verification commands

Frontend crate:

```text
cargo check --message-format short
cargo test --message-format short
trunk build --release
```

Native shell:

```text
cd src-tauri && cargo check --message-format short
cd src-tauri && cargo test --message-format short
cd src-tauri && cargo tauri build
```

## Content-security policy and development server

The Tauri CSP admits the current Trunk bootstrap module by SHA-256 hash; it does **not** enable `unsafe-inline`. The generated stylesheet is declared in `index.html` with `data-trunk`, so it is copied into `dist/app.css` for both development and release builds.

The CSP hash is tied to the generated bootstrap. `trunk build` or `trunk serve` after a frontend change must produce a bootstrap whose hash matches `src-tauri/tauri.conf.json`; otherwise the native WebView deliberately fails closed. Development uses `trunk serve --no-autoreload` because Trunk's live-reload client is another generated inline script.


## Release strategy

### Current stance

The desktop app is **bundle-ready but not auto-update-ready**.

That means:

- local desktop bundles are a supported target
- in-app update *checking* is allowed
- in-app update *installation* is intentionally deferred
- release docs and config assume **signed manual distribution first**

### Why updater install is deferred

The backend currently exposes:

- `check_for_updates` → partial
- `install_update` → unsupported

That is deliberate. Shipping a fake updater is worse than shipping none.

Until desktop-native install/restart semantics are implemented and tested per platform, the release path is:

1. build signed bundles
2. publish release notes + artifacts
3. let the desktop app report that an update exists
4. direct the operator to upgrade via a new packaged release or OpenCrabs-native `/evolve` where appropriate

### Initial production release policy

| Area | Policy |
|---|---|
| Distribution | signed manual artifacts |
| Channels | `dev`, `beta`, `stable` documented release lanes |
| Auto-update install | deferred until native restart/install flow is real |
| Update check UX | allowed, but must clearly state install is external/manual |
| Rollback | reinstall previous signed artifact |

## Packaging notes

- Tauri bundle generation is enabled in `src-tauri/tauri.conf.json`
- app identifiers and icons exist, but release signing/notarization is still an explicit production task
- `dist/` is generated output and should not be hand-edited
- release runbook: `docs/release-runbook.md`
- release checklist: `docs/release-checklist.md`
- release strategy note: `docs/release-strategy.md`

## Release verification

Run the complete release gate from `desktop/`:

```text
./scripts/release-verify.sh
```

The script enforces the correct crate directories and runs formatting, checks, tests, the Trunk release build, and Tauri packaging. It stops immediately with an install command when `trunk` or `tauri-cli` is absent.

## Verified release evidence (2026-07-25)

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

## Production gates still external

- run `./scripts/release-verify.sh` on every target platform;
- sign and notarize/staple macOS artifacts before stable distribution;
- sign Windows installers and publish checksums for every release;
- keep desktop update installation unsupported until a tested native install/restart flow exists;
- add a deliberate crash/debug-bundle export workflow if bounded diagnostics are insufficient for support.

## Migration cleanup status

Legacy pre-Dioxus frontend leftovers have been removed.

The desktop project now has one active frontend path: the Dioxus app under `src/`.
