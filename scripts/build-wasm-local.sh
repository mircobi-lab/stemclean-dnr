#!/usr/bin/env bash
# Solo per l'ambiente di sviluppo senza il target wasm32 precompilato:
# compila anche la libreria standard da sorgente. Sul Mac usa scripts/build-web.sh.
set -euo pipefail
cd "$(dirname "$0")/.."
RUSTC_BOOTSTRAP=1 cargo build -q --release -p stemclean-wasm --target wasm32-unknown-unknown -Zbuild-std=std,panic_abort
