#!/usr/bin/env sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

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

cargo fmt --check
cargo check --message-format short
cargo test --message-format short
trunk build --release
grep -Eq 'opencrabs-desktop-ui-[0-9a-f]+\.js' dist/index.html
grep -Eq 'opencrabs-desktop-ui-[0-9a-f]+_bg\.wasm' dist/index.html
! grep -q 'TrunkApplicationStarted.*mounted' dist/index.html

cd src-tauri
cargo fmt --check
cargo check --message-format short
cargo test --message-format short
cargo tauri build
