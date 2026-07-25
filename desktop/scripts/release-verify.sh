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
test -f dist/app.css || {
  printf '%s\n' 'Trunk did not emit dist/app.css; refusing to package a blank/unstyled desktop UI.' >&2
  exit 1
}

cd src-tauri
cargo fmt --check
cargo check --message-format short
cargo test --message-format short
cargo tauri build
