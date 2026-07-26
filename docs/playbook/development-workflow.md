# OpenCrabs Desktop GUI — Development Workflow Playbook

## Prerequisites

Ensure Rust and Trunk are installed:
```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
```

> **NEVER USE NPM**. NPM commands are strictly prohibited. Package compilation is handled exclusively via Cargo and Trunk.

## Local Development Workflow

1. **Build WASM Frontend**:
   ```bash
   cargo build -p opencrabs-desktop-ui --target wasm32-unknown-unknown
   ```

2. **Bundle Static Assets with Trunk**:
   ```bash
   cd desktop && trunk build
   ```

3. **Launch Tauri Application in Dev Mode**:
   ```bash
   cd desktop && cargo tauri dev
   ```

## Verification & Quality Gates

Run Babashka LOC validation script:
```clojure
(require '[babashka.fs :as fs] '[clojure.string :as str])
(doseq [f (fs/glob "desktop/src-ui/src" "**/*.rs")]
  (let [n (count (str/split-lines (slurp (str f))))]
    (assert (< n 250) (str "LOC Limit Exceeded: " f " (" n " lines)"))))
(println "✅ All 24 Dioxus UI files are <250 LOC!")
```
