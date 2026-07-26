# OpenCrabs Desktop UI

OpenCrabs Desktop is a **Dioxus WebAssembly frontend embedded in a Tauri 2 native shell**. It is currently an unsigned, Apple-Silicon macOS preview: suitable for internal testing, not a stable signed release.

## Run the native desktop app

The desktop GUI must run inside Tauri. A browser-only Trunk preview can render the UI but cannot execute OpenCrabs desktop commands.

```text
cd /Users/moe/Desktop/crabz/desktop/src-tauri
cargo tauri dev
```

Tauri starts the Trunk frontend automatically. To inspect only the frontend layout, run:

```text
cd /Users/moe/Desktop/crabz/desktop
trunk serve --port 8080 --no-autoreload
```

Browser previews deliberately report that desktop actions require `cargo tauri dev`.

## Active structure

- `src/main.rs` — Dioxus entry point
- `src/app.rs` — routed shell and panel components
- `src/bridge.rs` — native Tauri invoke/event bridge for the WASM frontend
- `src/models.rs` — frontend DTOs matching Tauri command payloads
- `src/css/app.css` — active desktop stylesheet
- `src-tauri/` — native Tauri backend and command handlers

The desktop DTO/command contract is documented in `desktop-command-contract.md`.

## Toolchain

| Tool | Version expectation | Why |
|---|---|---|
| Rust | `1.91.0` | Matches the native Tauri crate `rust-version` |
| `trunk` | `0.21.x` | Builds and serves the Dioxus WASM frontend |
| `tauri-cli` | `2.x` | Provides the `cargo tauri` command |
| Node | not required | This path is Rust + Trunk |

Install the CLI tools with:

```text
rustup toolchain install 1.91.0
rustup override set 1.91.0
cargo install trunk --version ^0.21 --locked
cargo install tauri-cli --version ^2 --locked
```

## Frontend ↔ native bridge

The WASM bridge resolves `window.__TAURI__.core.invoke` at runtime and preserves the `core` receiver when invoking commands. It always forwards the serialized argument object, including named arguments such as `sessionId` for `get_session_messages`.

This matters because detached JavaScript method calls can lose their receiver and silently drop or mishandle IPC arguments. The bridge fails with an explicit native-runtime error when launched in a browser preview rather than pretending desktop controls are functional.

## CSP and Trunk assets

The Tauri CSP does **not** enable `unsafe-inline`. It pins the current Trunk WASM bootstrap module by SHA-256 and permits only self-hosted script/style assets. The stylesheet is declared as a Trunk-managed `data-trunk` asset, so a build must produce `dist/app.css`.

Development uses `trunk serve --no-autoreload`: Trunk's live-reload client is another inline script and is intentionally excluded rather than opening the CSP.

After changing the frontend or upgrading Trunk, validate the generated bootstrap hash against `src-tauri/tauri.conf.json`. A stale hash intentionally fails closed and can leave the native WebView blank.

## Verification

Frontend crate, from `desktop/`:

```text
cargo fmt --check
cargo check --message-format short
cargo test --message-format short
trunk build --release
```

Native shell, from `desktop/src-tauri/`:

```text
cargo fmt --check
cargo check --message-format short
cargo test --message-format short
cargo tauri build
```

Run the complete gate from `desktop/`:

```text
./scripts/release-verify.sh
```

A Cargo command launched from a parent directory without the relevant `Cargo.toml` is an invocation error, not desktop verification evidence.

## Release posture

Desktop distribution is **manual bundles first**:

- update discovery may be available;
- in-app update installation is intentionally unsupported;
- stable releases require signed artifacts, checksums, and target-platform verification;
- the current macOS preview is ARM64-only and unsigned, so Gatekeeper may require explicit approval.

See `docs/release-strategy.md`, `docs/release-runbook.md`, `docs/release-checklist.md`, and `docs/diagnostics.md` for release and support details.

## Known limitations

- Channel status reports configuration/credential readiness, not a live channel connection.
- Provider cancellation is asynchronous and provider-specific.
- Voice and native in-app update installation are explicitly unsupported.
- Session rename/delete, cron creation/deletion/history, dynamic-tool removal, and privileged Brain/config edits need clearer confirmation and validation UX before stable distribution.
- macOS code signing, notarization, stapling, fresh-install testing, and upgrade testing remain external release gates.
