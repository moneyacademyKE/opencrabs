# OpenCrabs Desktop GUI — Component Standards Playbook

## Code Guidelines & Governance

1. **Strict File Size Cap (<250 LOC)**:
   - Every `.rs` file in `desktop/src-ui/src/` must remain strictly under 250 lines of code.
   - If a component grows large, split sub-views into separate child modules (e.g. `chat.rs`, `chat_input.rs`, `message_bubble.rs`).

2. **High Cohesion, Low Coupling**:
   - UI components consume data via Dioxus Signals (`use_context::<AppState>()`).
   - Signal mutations happen in async `spawn` tasks invoking `bridge.rs`.

3. **Intention-Revealing Naming**:
   - Struct names are prefixed with `T` for IPC mirror types (`TSession`, `TMessage`, `TToolInfo`).
   - Signal names reflect exact state purpose (`active_session_id`, `is_sending`).

4. **Zero JS Invocations in Components**:
   - Raw JavaScript calls are isolated strictly within `bridge.rs` via `wasm_bindgen`.
