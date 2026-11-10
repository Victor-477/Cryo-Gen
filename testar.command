#!/bin/bash
# Compila, testa e abre a Cryo Engine. Dê dois cliques neste arquivo no Finder
# (ou rode `bash testar.command` no Terminal). O resultado fica em teste-log.txt.
cd "$(dirname "$0")" || exit 1
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
LOG=teste-log.txt
{
  echo "=== Cryo Engine: teste em $(date) ==="
  sw_vers 2>/dev/null; uname -m
  cargo --version && rustc --version
} > "$LOG" 2>&1
if ! command -v cargo >/dev/null; then
  echo "ERRO: cargo não encontrado. Instale o Rust em https://rustup.rs" | tee -a "$LOG"
  exit 1
fi
echo "=== cargo test ===" | tee -a "$LOG"
cargo test --workspace 2>&1 | tee -a "$LOG"
echo "TESTES: código de saída ${PIPESTATUS[0]}" | tee -a "$LOG"
echo "=== cargo build --release ===" | tee -a "$LOG"
cargo build --release 2>&1 | tee -a "$LOG"
BUILD=${PIPESTATUS[0]}
echo "BUILD: código de saída $BUILD" | tee -a "$LOG"
if [ "$BUILD" = "0" ]; then
  echo "=== abrindo o editor (feche a janela para terminar) ===" | tee -a "$LOG"
  ./target/release/cryo 2>&1 | tee -a "$LOG"
  echo "EDITOR: código de saída ${PIPESTATUS[0]}" | tee -a "$LOG"
fi
echo "=== fim ===" | tee -a "$LOG"
