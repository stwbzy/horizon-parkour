#!/usr/bin/env bash
# Compila Parkour en la instancia de desarrollo `parkour-dev`, como lo haría un jugador:
#   1. empaqueta con tools/package.py (igual que la release) e instala parkour.hnmod,
#   2. deja activos sólo Parkour (los otros mods nativos copiados quedan en false),
#   3. compila con el launcher y comprueba que la instancia juega la build nueva.
# Así el botón Jugar del launcher usa la build con Parkour (antes elegía la de los otros mods).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LAUNCHER="$HOME/juegos/horizon-launcher"
INSTANCE="parkour-dev"
GAME_DIR="$HOME/.local/share/horizon/instances/$INSTANCE"
BASE_VERSION="nightly-2026-11-08-fdc87ae"

# Mismo empaquetado que la release: crea dist/parkour.hnmod.
python3 "$ROOT/tools/package.py"
mkdir -p "$GAME_DIR/horizon-native-mods"
cp "$ROOT/dist/parkour.hnmod" "$GAME_DIR/horizon-native-mods/parkour.hnmod"

# horizon-mods.txt: parkour on (contenido y nativo comparten la clave), el resto off.
mods="$GAME_DIR/horizon-mods.txt"
touch "$mods"
sed -i -E 's/^(mod\.[a-z0-9_]+):true$/\1:false/' "$mods"
if grep -q '^mod\.parkour:' "$mods"; then
    sed -i 's/^mod\.parkour:.*/mod.parkour:true/' "$mods"
else
    echo 'mod.parkour:true' >> "$mods"
fi

"$LAUNCHER" native-build --instance "$INSTANCE" --version "$BASE_VERSION" --trust-native-code

built="$(python3 -c "import json,os;print(json.load(open(os.path.expanduser('~/.local/share/horizon-launcher/instances.json')))['instances']['$INSTANCE']['version'])")"
mods_json="$HOME/.local/share/horizon-launcher/versions/$built/native-mods.json"
if grep -q '"id":"parkour"' "$mods_json"; then
    echo "OK: $INSTANCE juega $built, con Parkour."
else
    echo "ERROR: $INSTANCE juega $built, que no incluye Parkour." >&2
    exit 1
fi
