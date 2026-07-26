#!/usr/bin/env sh
# Release verification ladder for the OpenCrabs desktop app.
#
# Every gate is fail-closed: the first failure aborts the run with a clear GATE
# label. Deterministic evidence is written under .verification/.
#
# Gates, in order:
#   frontend: fmt, clippy -D warnings, check, test, trunk release build,
#             paired/hashed JS+WASM assets present in dist/index.html
#   native:   fmt, clippy -D warnings, check, test, tauri bundle
#   smoke:    native macOS launch + IPC-readiness + liveness + log-clean
#
# Run from anywhere:  sh desktop/scripts/release-verify.sh
# Needs an interactive macOS session for the final smoke gate (GUI launch).
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
evidence="$root/.verification"
mkdir -p "$evidence"

gate() {
  printf '\n=== GATE: %s ===\n' "$1"
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf '%s\n' "Missing required release tool: $1" >&2
    exit 1
  fi
}

need trunk
need cargo
cargo tauri --version >/dev/null 2>&1 || {
  printf '%s\n' 'Missing required release tool: tauri-cli (install with cargo install tauri-cli --version "^2" --locked)' >&2
  exit 1
}

# ---------------- frontend gates ----------------
gate "frontend: cargo fmt --check"
cargo fmt --check

gate "frontend: cargo clippy -D warnings (all-targets)"
cargo clippy --all-targets -- -D warnings

gate "frontend: cargo check"
cargo check --message-format short

gate "frontend: cargo test"
cargo test --message-format short

gate "frontend: trunk build --release"
trunk build --release

gate "frontend: paired hashed release assets in dist/index.html"
grep -Eq 'opencrabs-desktop-ui-[0-9a-f]+\.js' dist/index.html \
  || { printf '%s\n' 'FAIL: hashed JS asset not referenced in dist/index.html' >&2; exit 1; }
grep -Eq 'opencrabs-desktop-ui-[0-9a-f]+_bg\.wasm' dist/index.html \
  || { printf '%s\n' 'FAIL: hashed WASM asset not referenced in dist/index.html' >&2; exit 1; }
js_hash=$(grep -oE 'opencrabs-desktop-ui-[0-9a-f]+\.js' dist/index.html | head -1 | sed -E 's/.*-([0-9a-f]+)\.js/\1/')
wasm_hash=$(grep -oE 'opencrabs-desktop-ui-[0-9a-f]+_bg\.wasm' dist/index.html | head -1 | sed -E 's/.*-([0-9a-f]+)_bg\.wasm/\1/')
[ -n "$js_hash" ] || { printf '%s\n' 'FAIL: could not extract JS hash' >&2; exit 1; }
[ "$js_hash" = "$wasm_hash" ] \
  || { printf '%s\n' "FAIL: JS/WASM hash pair mismatch ($js_hash != $wasm_hash)" >&2; exit 1; }
# The hashed asset files themselves must exist on disk (deterministic pairing).
test -f "dist/opencrabs-desktop-ui-$js_hash.js" \
  || { printf '%s\n' "FAIL: dist/opencrabs-desktop-ui-$js_hash.js missing" >&2; exit 1; }
test -f "dist/opencrabs-desktop-ui-${js_hash}_bg.wasm" \
  || { printf '%s\n' "FAIL: dist/opencrabs-desktop-ui-${js_hash}_bg.wasm missing" >&2; exit 1; }
# Startup probes must live in the JS bundle, not be inlined into the HTML shell.
! grep -q 'TrunkApplicationStarted.*mounted' dist/index.html \
  || { printf '%s\n' 'FAIL: startup marker leaked into dist/index.html' >&2; exit 1; }
printf '%s\n' "  hashed pair OK: $js_hash"

# ---------------- native gates ----------------
gate "native: cargo fmt --check"
( cd src-tauri && cargo fmt --check )

gate "native: cargo clippy -D warnings (all-targets)"
( cd src-tauri && cargo clippy --all-targets -- -D warnings )

gate "native: cargo check"
( cd src-tauri && cargo check --message-format short )

gate "native: cargo test"
( cd src-tauri && cargo test --message-format short )

gate "native: tauri bundle (release)"
( cd src-tauri && cargo tauri build )

# ---------------- smoke gate ----------------
gate "native: macOS launch smoke"
sh "$root/scripts/native-smoke.sh"

printf '\n=== ALL GATES PASSED ===\n'
sha=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
printf '%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ) release-verify: ALL GATES PASSED (sha $sha)" > "$evidence/release-verify.txt"
cat "$evidence/release-verify.txt"
