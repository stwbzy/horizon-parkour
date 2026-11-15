# Parkour — mod de parkour cinemático para Horizon

Mod nativo para [Horizon](https://github.com/horizon-rust/horizon) (Minecraft 26.3 en Rust): movimiento de
parkour al estilo Mirror's Edge, con cámara y manos animadas en primera persona.

**Estado: en desarrollo.** Ya funcionan:

- **Aterrizaje**: al caer, correr quita daño, pulsar agacharse en el aire quita más, y las dos cosas a la vez
  hacen rodar (sin daño hasta 8 bloques).
- **Mantle**: salta hacia una repisa manteniendo avanzar y te agarras al borde y subes a pulso (hasta 3 bloques
  desde el suelo). Suelta avanzar para quedarte colgado; agáchate para soltarte.
- **Vault**: corriendo hacia un obstáculo bajo (de un escalón a 1.5 bloques, vallas incluidas), salta y lo pasas
  apoyando las manos sin perder la carrera. Si es hondo, acabas de pie encima.
- **Slide**: corriendo, pulsa agacharse y te deslizas unos 5 bloques (por huecos de 1.5 también). Salta a media
  deslizada para salir con la inercia.

El diseño completo y las fases están en [PLAN.md](PLAN.md).

## Movimientos previstos (v1)

- Wall-run lateral (cámara inclinada) y vertical, con wall jump.
- Aterrizaje amortiguado: correr, agacharse y rodar reducen el daño de caída.
- Mantle: subir con las manos a repisas de más de dos bloques, y quedarse colgado.

Sin teclas propias: todo es contextual con salto, sprint y agacharse.

## Desarrollo

| Comando | Qué hace |
|---|---|
| `tools/dev_build.sh` | Empaqueta e instala el mod en la instancia `parkour-dev` y compila el juego con el launcher |
| `tools/test.sh` | Tests de la lógica pura (estados, sensores, aterrizaje, mantle, vault, slide, curvas de cámara) |
| `python3 tools/package.py` | Crea `dist/parkour.hnmod` |

El HUD de depuración se activa en Options > Mods > Parkour.

## Licencia

MIT.
