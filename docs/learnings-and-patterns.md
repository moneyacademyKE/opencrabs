# OpenCrabs Desktop GUI — Learnings, Antipatterns & Design Patterns

## Architectural Patterns

1. **The Typed WASM FFI Bridge Pattern**:
   - *Problem*: Deserializing dynamic JavaScript objects in WebAssembly can cause runtime type panics.
   - *Solution*: `bridge.rs` uses `serde_wasm_bindgen` with strongly-typed Rust IPC structs (`TSession`, `TMessage`), ensuring runtime safety at compile time.

2. **Dioxus Signal Isolation Pattern**:
   - *Problem*: Passing mutable references across async Tokio task boundaries leads to lifetime issues.
   - *Solution*: Use Dioxus `Signal<T>` with `spawn(async move { ... })`. Signals handle internal mutability cleanly across async tasks.

## Antipatterns Avoided

1. **The Node/NPM Tooling Complection**:
   - *Antipattern*: Mixing `npm install` and Node dependencies with Rust backends.
   - *Resolution*: Replaced with a pure `cargo` + `trunk` WASM pipeline.

2. **Immediate-Mode Redraw Waste**:
   - *Antipattern*: Redrawing complex markdown streams every frame 60 times a second.
   - *Resolution*: Fine-grained Dioxus signals update only the active message bubble when tokens arrive.
