# ADR-0002: In-Process FFI IPC Bridge over Local REST Server

## Context & Problem Statement
Communicating between the desktop frontend interface and the core `AgentService` backend engine can be implemented either via a local HTTP/WebSocket REST server or via an in-process IPC FFI binding.

## Decision Drivers
- Zero network security vulnerabilities (no exposed TCP ports).
- Sub-millisecond IPC latency for token streaming and tool approvals.
- Simplification of process lifecycle management (no daemon orphan processes).

## Considered Options
1. **Local REST / WebSocket Server**: Spawning an internal HTTP server (e.g. `axum` or `actix`) listening on `127.0.0.1`.
2. **In-Process Tauri IPC Bridge**: Executing `tauri::command` functions in Tokio background tasks with direct FFI bindings.

## Decision Outcome
Chosen option: **In-Process Tauri IPC Bridge**, because it completely avoids opening TCP ports on the user's local network, eliminating firewall popup warnings and CSRF vulnerability vectors.

## Consequences
- **Positive**: Sub-millisecond token streaming performance.
- **Positive**: Complete process isolation; stopping the application cleanly terminates all async tasks.
- **Negative**: Commands must deserialize JSON across the JS/WASM FFI boundary (handled via `serde_wasm_bindgen`).
