#!/usr/bin/env bash
# Construye el circuito de demostración en el mundo abierto (necesita un juego con --control-socket).
# Uso: tools/build_demo.sh /ruta/al/horizon-ctl [--socket SOCKET]
# Todo a Y=150, avanzando por +X desde X=-4. Pide comandos (trucos) activados.
set -euo pipefail
CTL=("$@"); Y=150; B=minecraft:stone_bricks
c() { "${CTL[@]}" "$@" >/dev/null; }
seg() { c command "tp @s $1 $((Y+12)) 0"; c wait 30; }
for x0 in -4 28 60 92; do seg $((x0+16)); c fill $x0 $((Y-8)) -8 $((x0+31)) $((Y+16)) 8 minecraft:air; done
seg 8;  c fill -4 $Y -6 27 $Y 6 minecraft:smooth_stone; c fill -4 $Y -6 27 $Y -6 $B; c fill -4 $Y 6 27 $Y 6 $B
c fill 12 $((Y+1)) -5 12 $((Y+1)) 5 minecraft:oak_planks
c fill 17 $((Y+1)) -5 17 $((Y+2)) 5 minecraft:hay_block
c fill 22 $((Y+1)) -5 22 $((Y+1)) 5 minecraft:oak_fence
seg 38; c fill 28 $((Y-7)) -6 47 $((Y-7)) 6 minecraft:sandstone      # no arena: cae
c fill 28 $((Y-6)) -3 47 $((Y+9)) -3 $B; c fill 28 $((Y-6)) 3 47 $((Y+9)) 3 $B
seg 56; c fill 48 $Y -6 63 $Y 6 minecraft:smooth_stone; c fill 56 $((Y+1)) -6 56 $((Y+3)) 6 $B
c fill 57 $((Y+3)) -6 63 $((Y+3)) 6 minecraft:smooth_stone
seg 76; for r in "64 67" "71 75" "79 83"; do set -- $r; c fill $1 $((Y+3)) -6 $2 $((Y+3)) 6 minecraft:smooth_stone; done
c fill 64 $((Y-4)) -6 91 $((Y-4)) 6 minecraft:sandstone
seg 96; c fill 86 $((Y+3)) -6 93 $((Y+3)) 6 minecraft:smooth_stone
c fill 94 $((Y+3)) -2 105 $((Y+16)) -2 $B; c fill 94 $((Y+3)) 2 105 $((Y+16)) 2 $B
c fill 100 $((Y+15)) -2 106 $((Y+15)) 2 minecraft:smooth_stone
c command "tp @s 112 $((Y+20)) 0"; c wait 30
c fill 107 $((Y+8)) -6 125 $((Y+8)) 6 minecraft:smooth_stone; c fill 107 $((Y+9)) -6 125 $((Y+20)) 6 minecraft:air
c command "tp @s -2 $((Y+1)) 0"
