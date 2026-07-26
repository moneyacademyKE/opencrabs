# Architectural Decision Records (ADR) Log

All architectural decisions for `opencrabs-desktop` GUI are documented here following Nygard ADR principles.

## Index of Decision Records

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [ADR-0001](0001-tauri2-architecture.md) | Adoption of Tauri 2 Native Container Architecture | Accepted | 2026-07-25 |
| [ADR-0002](0002-in-process-ipc-bridge.md) | In-Process FFI IPC Bridge over Local REST Server | Accepted | 2026-07-25 |
| [ADR-0003](0003-dioxus-wasm-frontend-strategy.md) | Adoption of 100% Rust Dioxus (Wasm) Frontend Strategy | Accepted | 2026-07-25 |
| [ADR-0004](0004-zero-npm-build-pipeline.md) | Enforcement of Zero-npm Build Pipeline via Trunk & Bun | Accepted | 2026-07-25 |
| [ADR-0005](0005-rejection-bevy-gpui-iced.md) | Evaluation & Rejection of Bevy UI, GPUI, and Iced | Accepted | 2026-07-25 |

---

## Governance & Principles

1. **Rich Hickey Simplicity**: De-complect state from time and de-complect build systems.
2. **Strict File LOC Cap**: No file in the desktop UI codebase shall exceed 250 LOC.
3. **Zero npm Dependencies**: Package management uses pure Rust (`cargo`) and static bundlers (`trunk`).
