#!/usr/bin/env bash
# Builds the editor core to WebAssembly and generates JS bindings into ui/src/wasm.
set -euo pipefail
cd "$(dirname "$0")/.."
# rustup's toolchain carries the wasm32 target; Homebrew's rustc does not.
export PATH="$HOME/.cargo/bin:$PATH"

profile="${1:-release}"
if [[ "$profile" == "release" ]]; then
  cargo build -p ppt-wasm --target wasm32-unknown-unknown --release
else
  cargo build -p ppt-wasm --target wasm32-unknown-unknown
fi
wasm-bindgen --target web --out-dir ui/src/wasm --out-name ppt_wasm \
  "target/wasm32-unknown-unknown/$profile/ppt_wasm.wasm"
