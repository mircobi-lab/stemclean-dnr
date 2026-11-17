#!/usr/bin/env bash
# Compila il motore Rust in WebAssembly e crea dist/index.html (un unico file da aprire nel browser).
set -euo pipefail
cd "$(dirname "$0")/.."
rustup target add wasm32-unknown-unknown >/dev/null
cargo build --release -p stemclean-wasm --target wasm32-unknown-unknown
node scripts/bundle-web.mjs target/wasm32-unknown-unknown/release/stemclean_wasm.wasm
