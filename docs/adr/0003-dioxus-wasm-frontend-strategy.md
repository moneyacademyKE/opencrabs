# ADR-0003: Adoption of 100% Rust Dioxus (Wasm) Frontend Strategy

## Context & Problem Statement
The desktop application frontend needs a reactive state system to manage 15+ sub-panels (Chat, Session List, Tools, Skills, Cron Jobs, Channels, Usage Analytics, Brain Editor).

Mixing JavaScript/TypeScript with Rust backend creates build system complection (Cargo + Vite/Node/Bun), state fragmentation, and duplicate type declarations across languages.

## Decision Drivers
- **Rich Hickey Simplicity**: De-complecting the build pipeline into a single compiler toolchain (`cargo`).
- 100% Rust codebase from backend engine down to frontend UI components.
- Fine-grained signal reactivity (`Signal<T>`) for real-time token streaming.

## Considered Options
1. **Hybrid TypeScript / Qwik**: Rust backend + Qwik TSX frontend.
2. **Dioxus 0.6 (Wasm)**: 100% Rust frontend using Dioxus signals and `rsx!` component macros.
3. **Leptos (Wasm)**: 100% Rust fine-grained reactive framework.

## Decision Outcome
Chosen option: **Dioxus 0.6 (Wasm)**. Dioxus `rsx!` syntax maps 1:1 with standard HTML structure, preserving existing CSS tokens while eliminating JavaScript entirely from the workspace.

## Consequences
- **Positive**: Single unified compiler toolchain (`cargo`).
- **Positive**: 100% shared Rust type definitions (`TMessage`, `TSession`, `TToolInfo`) between backend and UI.
- **Negative**: Requires installing the `wasm32-unknown-unknown` compilation target.
