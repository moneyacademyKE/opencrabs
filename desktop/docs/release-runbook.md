# Production release runbook

This runbook turns the current desktop app into a repeatable distributable artifact instead of a vibes-based build.

## Supported shape

- Frontend: Dioxus WebAssembly via Trunk
- Native shell: Tauri 2
- Distribution: signed manual bundles first
- Update install: intentionally deferred until desktop-native apply/restart flow is implemented and tested

## Toolchain contract

| Tool | Required version |
|---|---|
| Rust | `1.91.0` |
| `trunk` | `0.21.x` |
| `tauri-cli` | `2.x` |

These are pinned by:

- `rust-toolchain.toml`
- `README.md`
- `src-tauri/Cargo.toml` metadata

## Bootstrap

From `desktop/`:

```text
rustup toolchain install 1.91.0
rustup override set 1.91.0
cargo install trunk --version ^0.21 --locked
cargo install tauri-cli --version ^2 --locked
```

## Verification before release

From `desktop/`, run the single release gate:

```text
./scripts/release-verify.sh
```

It runs, in the correct directories:

```text
cargo fmt --check
cargo check --message-format short
cargo test --message-format short
trunk build --release
cd src-tauri && cargo fmt --check
cd src-tauri && cargo check --message-format short
cd src-tauri && cargo test --message-format short
cd src-tauri && cargo tauri build
```

The script exits before building when `trunk` or `tauri-cli` is unavailable and prints the exact missing tool.

## CSP and frontend asset verification

The Tauri CSP allows the single SHA-256-pinned Trunk WebAssembly bootstrap module, not `unsafe-inline`. The Dioxus stylesheet is a Trunk-managed `data-trunk` CSS asset and must appear as `dist/app.css` after every frontend build.

Before packaging, verify both conditions:

```text
trunk build --release
test -f dist/app.css
```

Development is deliberately run with `trunk serve --no-autoreload`; the live-reload client is an additional generated inline script and is excluded rather than broadly allowing inline JavaScript.

If upgrading Trunk changes the bootstrap script, compute its SHA-256 base64 digest from the generated `dist/index.html` and update the single script hash in `src-tauri/tauri.conf.json`. Do not loosen the CSP to make an upgrade pass.


Run each command from the directory named in this runbook. A Cargo failure from a parent directory without the relevant manifest is an invocation error, not a build verdict. Preserve command output alongside the release notes, target platform, artifact version, and signing evidence.

## Native runtime evidence (2026-07-25)

A fresh `cargo tauri dev --no-watch` launch loaded the Tauri window and fetched the Trunk HTML, stylesheet, JavaScript, WebAssembly module, and Dioxus runtime snippets successfully. During this inspection, a stale bootstrap CSP hash was corrected. Keep the CSP hash in `src-tauri/tauri.conf.json` synchronized with the emitted Trunk bootstrap; stale hashes produce an intentionally blank/fail-closed WebView.


These commands were run successfully:

- `cargo test --message-format short` in `desktop/` → **11 passed**
- `cargo test --message-format short` in `desktop/src-tauri/` → **28 passed**
- `cargo build --message-format short` in `desktop/` → passed
- `cargo build --message-format short` in `desktop/src-tauri/` → passed
- `trunk build --release -v` in `desktop/` → passed
- `cargo tauri build` in `desktop/src-tauri/` → passed

Artifacts produced:

- `src-tauri/target/release/bundle/macos/OpenCrabs.app`
- `src-tauri/target/release/bundle/dmg/OpenCrabs_0.1.0_aarch64.dmg`

## Release lanes

| Lane | Purpose | Rule |
|---|---|---|
| `dev` | local engineering builds | may be unsigned |
| `beta` | pre-release QA | signed whenever platform requires it |
| `stable` | production release | signed artifacts required |

## Current updater policy

- `check_for_updates`: allowed
- `install_update`: intentionally unsupported
- GUI must not claim in-app install works
- release notes must direct users to signed artifacts or OpenCrabs-native `/evolve` where appropriate

## Platform signing tasks

### macOS
- produce `.app` / `.dmg`
- sign with Developer ID
- notarize
- staple notarization ticket

### Linux
- produce target package(s) intentionally (`.AppImage`, `.deb`, etc.)
- sign where your distribution channel expects it

### Windows
- produce installer/bundle target intentionally
- Authenticode sign before distribution

## Artifact expectations

Each release should have:

- versioned desktop bundle(s)
- release notes
- checksum manifest
- lane designation (`dev` / `beta` / `stable`)
- explicit statement that update install is manual unless the updater contract changes

## Guardrails

Do not:

- expose an enabled “Install update” button while install is unsupported
- treat `/evolve` as generic desktop updater UX
- ship unsigned stable artifacts
- change Rust/Trunk/Tauri versions without updating the documented contract

## Promotion checklist

- [ ] verification commands green
- [ ] desktop command contract still matches runtime behavior
- [ ] signing/notarization complete for target platform
- [ ] release notes written
- [ ] artifact checksums generated
- [ ] updater/install UX still honest
