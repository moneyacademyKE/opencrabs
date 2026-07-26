OpenCrabs Desktop preview refresh for macOS ARM64.

## What changed
- fixed the packaged desktop blank-screen startup failure by removing the fragile long-lived Tauri event-listener wasm closure bridge
- switched production desktop assets to `trunk build --release` with hashed JS/WASM references
- disabled `wasm-opt` in this packaging path to avoid shipping a broken optimized wasm bundle
- added release verification to assert the generated HTML references hashed frontend assets before bundling
- preserved structured desktop transcript disclosures for reasoning and tool activity

## Verification
- `cargo test --manifest-path desktop/Cargo.toml -p opencrabs-desktop-ui`
- `cargo clippy --manifest-path desktop/Cargo.toml -p opencrabs-desktop-ui --all-targets -- -D warnings`
- `sh desktop/scripts/release-verify.sh`

## Artifact
- `OpenCrabs_0.1.0_aarch64.dmg`
- `OpenCrabs_0.1.0_aarch64.dmg.sha256`
