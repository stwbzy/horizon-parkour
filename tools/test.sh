#!/usr/bin/env bash
# Tests de los módulos de lógica pura (sin Host), con el rustc del launcher. No compila el juego.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUSTC="$(find "$HOME/.local/share/horizon-launcher/rust" -name rustc -type f | head -1)"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
for module in state config sense land cinematic net; do
    "$RUSTC" --edition 2024 --test "$ROOT/parkour/api/src/$module.rs" -o "$OUT/$module"
    "$OUT/$module" --quiet
done
