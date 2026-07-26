# ADR-0004: Enforcement of Zero-npm Build Pipeline via Trunk & Bun

## Context & Problem Statement
User project constraints explicitly prohibit the use of `npm` and Node.js package managers (`npm install`, `npm run`, `package-lock.json`).

## Decision Drivers
- Strict compliance with user rule `Never use npm`.
- Eliminating heavy `node_modules` directory overhead (>300MB).
- Deterministic, fast static site generation for Tauri's webview.

## Decision Outcome
Enforce a **Zero-npm Build Pipeline** using:
1. **Trunk**: Pure Rust WASM web application bundler (`cargo install trunk`).
2. **Bun**: Optional high-speed runner when utility scripts are needed.

`Trunk.toml` compiles `desktop/src-ui/` directly into static `.wasm`, `.html`, and `.css` assets inside `desktop/dist/`, which Tauri consumes natively.

## Consequences
- **Positive**: No `npm`, `node_modules`, or JS lockfile pollution in the repo.
- **Positive**: Blazing fast compilation pipeline natively driven by Cargo.
- **Negative**: Third-party JS libraries must be compiled via Wasm or vendored as static assets.
