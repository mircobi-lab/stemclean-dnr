#!/usr/bin/env bash
# Compila il programma da Terminale e lo copia in ~/bin/stemclean
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p stemclean-cli
mkdir -p "$HOME/bin" && cp target/release/stemclean "$HOME/bin/stemclean"
echo "Installato in ~/bin/stemclean  (prova: stemclean -h)"
