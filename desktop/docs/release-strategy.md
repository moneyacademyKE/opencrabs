# Desktop release strategy

This directory documents the current production release stance for the OpenCrabs desktop app.

## Current release posture

The desktop app supports **signed manual bundle distribution first**.

What exists now:

- Dioxus WASM frontend embedded in Tauri 2
- `cargo tauri build` bundle path enabled
- update discovery command (`check_for_updates`)
- explicit non-support for in-app install (`install_update` returns an honest error)

What does **not** exist yet:

- platform-native in-app installer/update application flow
- restart-and-swap lifecycle per platform
- signed updater feed validation path
- rollback automation

## Release lanes

| Lane | Purpose | Rule |
|---|---|---|
| `dev` | local engineering builds | may be unsigned/local-only |
| `beta` | pre-release manual QA | signed whenever platform requires it |
| `stable` | user-facing production release | signed artifacts required |

## Distribution path

1. Run repo verification commands.
2. Build desktop bundles with `cargo tauri build`.
3. Sign / notarize artifacts as required by platform.
4. Publish release notes and artifacts.
5. Desktop clients may detect a newer version, but installation happens outside the running app until updater install is implemented for real.

## Guardrails

- Do **not** pretend the desktop app can self-update yet.
- Keep `install_update` unsupported until the full install/restart path is implemented and tested.
- Any UI button for update installation must remain hidden or explicitly disabled until that contract changes.
- If using `/evolve`, treat it as an OpenCrabs runtime upgrade path, not as a generic desktop GUI updater.

## Future upgrade path options

Choose one before claiming desktop auto-update support:

1. **Tauri updater**
   - signed feed per platform
   - download + verify + install + restart semantics
   - per-platform QA required
2. **OpenCrabs-managed updater wrapper**
   - desktop delegates to a controlled OpenCrabs-native runtime path
   - still requires explicit restart/install behavior
3. **Remain manual**
   - simplest and most honest if operational burden is acceptable

## Release checklist seed

- [ ] `./scripts/release-verify.sh` passes from `desktop/`
- [ ] signing/notarization complete for target platform
- [ ] release notes written
- [ ] checksums generated and published
- [ ] updater/install UX matches actual capability
