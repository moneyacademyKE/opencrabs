# Production release runbook

This runbook makes the OpenCrabs desktop build repeatable instead of vibes-based.

## Supported shape

- Frontend: Dioxus WebAssembly via Trunk
- Native shell: Tauri 2
- Distribution: manual bundles first
- Current preview: unsigned Apple-Silicon macOS build
- Update installation: intentionally unsupported

## Toolchain contract

| Tool | Required version |
|---|---|
| Rust | `1.91.0` |
| `trunk` | `0.21.x` |
| `tauri-cli` | `2.x` |

Bootstrap from `desktop/`:

```text
rustup toolchain install 1.91.0
rustup override set 1.91.0
cargo install trunk --version ^0.21 --locked
cargo install tauri-cli --version ^2 --locked
```

The installable package is `tauri-cli`; the command it provides is `cargo tauri`.

## Native development

Run the GUI inside Tauri, not only in a browser preview:

```text
cd /Users/moe/Desktop/crabz/desktop/src-tauri
cargo tauri dev
```

Tauri starts Trunk through `beforeDevCommand`. A browser-only preview is useful for layout inspection but cannot execute desktop IPC commands.

## Release verification

From `desktop/`, run the single gate:

```text
./scripts/release-verify.sh
```

It executes, from the correct crate directories:

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

A Cargo error from a parent directory without the relevant manifest is an invocation error, not a build verdict. Preserve successful command output with the release notes, target platform, artifact version, and signing evidence.

## CSP and bridge contract

The Tauri CSP permits the current Trunk bootstrap only through a SHA-256 hash; it does not permit `unsafe-inline`. The generated stylesheet is a Trunk-managed `data-trunk` asset and must exist at `dist/app.css` after every build.

Before packaging:

```text
trunk build --release
test -f dist/app.css
```

Development uses `trunk serve --no-autoreload`, because Trunk's live-reload client is an additional inline script. If a Trunk upgrade or frontend build changes the generated bootstrap, update the one pinned hash in `src-tauri/tauri.conf.json`; do not weaken the CSP.

The WASM bridge resolves `window.__TAURI__.core.invoke` at runtime, preserves `core` as the JavaScript receiver, and always passes the serialized arguments object. Named Tauri arguments, including `sessionId`, therefore reach native commands reliably.

## Package and publish

1. Run `./scripts/release-verify.sh` from `desktop/`.
2. Compute checksums, for example:
   ```text
   shasum -a 256 src-tauri/target/release/bundle/dmg/*.dmg
   ```
3. Sign and notarize platform artifacts.
4. Staple macOS notarization tickets:
   ```text
   xcrun stapler staple <app-or-dmg>
   ```
5. Test a fresh install and upgrade from the previously supported version.
6. Publish release notes, artifacts, and checksums together.

## Release lanes

| Lane | Purpose | Rule |
|---|---|---|
| `dev` | local engineering builds | may be unsigned/local-only |
| `beta` | pre-release QA | signed whenever the platform requires it |
| `stable` | user-facing production release | signed artifacts, checksums, and platform verification required |

## Current updater policy

- `check_for_updates`: partial; it may report available releases.
- `install_update`: unsupported; no in-app native installer/restart path exists.
- Do not expose an enabled Install button until the full download, verification, install, and restart lifecycle is implemented and tested.
- `/evolve` is an OpenCrabs runtime upgrade path, not generic desktop-GUI updater UX.

## Platform signing tasks

### macOS

- produce `.app` and `.dmg`
- sign with Developer ID
- notarize
- staple the notarization ticket

### Linux

- produce intended package formats (`.AppImage`, `.deb`, etc.)
- sign where the distribution channel requires it

### Windows

- produce the intended installer/bundle
- Authenticode-sign before distribution
