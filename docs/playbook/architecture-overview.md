# OpenCrabs Desktop GUI — Architecture Playbook

## System Overview

OpenCrabs Desktop GUI provides a native multi-panel "Agent OS" interface powered by a 100% Rust architecture:

```
[ Dioxus 0.6 WASM UI ] (desktop/src-ui)
         │
  (wasm-bindgen IPC)
         │
         ▼
[ Tauri 2 Desktop Crate ] (desktop/src-tauri)
         │
   (In-Process)
         │
         ▼
[ OpenCrabs Engine Crate ] (src/brain, src/services, src/db)
```

## Key Layers

1. **Frontend Layer (`desktop/src-ui`)**: Written in Dioxus 0.6 (Wasm target). Uses fine-grained signals (`Signal<T>`) for reactive UI state.
2. **IPC Bridge Layer (`bridge.rs`)**: Type-safe FFI wrapper calling Tauri backend commands via `wasm_bindgen`.
3. **Backend Command Layer (`desktop/src-tauri/src/commands/`)**: Modular Rust handlers executing inside Tokio tasks (`chat.rs`, `session.rs`, `config_cmd.rs`, `brain.rs`, `tools.rs`, `skills.rs`, `cron.rs`).
4. **Core Engine Layer (`src/`)**: Shared library crate providing `AgentService`, `ServiceContext`, SQLite repositories, Cron scheduler, and Channel gateways.
