# Ficha: Parkour (parkour, native-mod)

- Archivo: dist/parkour.hnmod · id `parkour` · versión 0.2.1 · Minecraft 26.3
- Nombre: Parkour
- Licencia: MIT · Autor: stwby · Código: https://github.com/stwbzy/horizon-parkour
- Categorías: Game mechanics, Movement
- Tags: parkour, movement, wall-run, multiplayer
- Resumen: Parkour al estilo Mirror's Edge sin teclas nuevas: roll, vault, mantle, slide, wall-run, colgarse de repisas y plancha.

## Descripción

# Parkour

Movimiento fluido para Horizon, sin cinemáticas pesadas y **sin teclas propias**: todo depende del contexto con avanzar, salto, sprint y agacharse.

- **Roll:** agáchate al caer (hasta 8 bloques sin daño); da igual si pulsas un poco antes o justo al tocar suelo.
- **Vault y mantle:** salta obstáculos bajos sin perder la carrera y sube repisas a pulso, o quédate colgado y muévete de lado.
- **Slide:** agáchate corriendo; slide-jump.
- **Wall-run** lateral y vertical, wall jump encadenado, tic-tac y plancha.
- Coyote time, buffer de salto y momento al encadenar movimientos.
- Cámara y manos animadas, sonido según el material, polvo. Ajustes: cinemática 0–150 %, partículas, HUD de depuración.

## Multijugador
El servidor decide el daño de caída: **todos los jugadores y el servidor necesitan la misma versión**. Sin el mod en el servidor, el parkour queda apagado. Ajustes en `config/parkour-server.txt`.

## Aviso
Mod nativo: se compila y ejecuta sin sandbox.
