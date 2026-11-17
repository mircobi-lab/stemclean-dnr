#!/usr/bin/env bash
# Crea l'app per Mac: src-tauri/target/.../bundle/macos/StemClean DNR.app e il .dmg
#   ./scripts/build-mac.sh            → app per il Mac che stai usando
#   ./scripts/build-mac.sh universal  → app unica per Mac Intel e Apple Silicon
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || { echo "Manca Rust: installalo con  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"; exit 1; }
command -v node >/dev/null || { echo "Manca Node.js: installalo da https://nodejs.org (versione LTS)"; exit 1; }
./scripts/build-web.sh
cd src-tauri
if [ "${1:-}" = "universal" ]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  npx --yes @tauri-apps/cli@2 build --target universal-apple-darwin
  echo "Fatto: src-tauri/target/universal-apple-darwin/release/bundle/"
else
  npx --yes @tauri-apps/cli@2 build
  echo "Fatto: src-tauri/target/release/bundle/"
fi
