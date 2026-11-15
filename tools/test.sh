#!/usr/bin/env bash
# Tests de los módulos de lógica pura (sin Host), con el rustc del launcher. No compila el juego.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUSTC="$(find "$HOME/.local/share/horizon-launcher/rust" -name rustc -type f | head -1)"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
"$RUSTC" --edition 2024 --test "$ROOT/tools/pure_tests.rs" -o "$OUT/pure_tests"
"$OUT/pure_tests" --quiet "$@"
