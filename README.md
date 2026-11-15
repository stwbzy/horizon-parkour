# Parkour — mod de parkour cinemático para Horizon

Mod nativo para [Horizon](https://github.com/horizon-rust/horizon) (Minecraft 26.3 en Rust): movimiento de
parkour al estilo Mirror's Edge, con cámara y manos animadas en primera persona.

**Estado: en desarrollo (fase 0, esqueleto).** Todavía no hay movimientos: el mod sigue al jugador y lo
muestra en un HUD de depuración. El diseño completo y las fases están en [PLAN.md](PLAN.md).

## Movimientos previstos (v1)

- Wall-run lateral (cámara inclinada) y vertical, con wall jump.
- Aterrizaje amortiguado: correr, agacharse y rodar reducen el daño de caída.
- Vault sobre obstáculos bajos.
- Mantle: subir con las manos a repisas de más de dos bloques, y quedarse colgado.
- Slide.

Sin teclas propias: todo es contextual con salto, sprint y agacharse.

## Desarrollo

| Comando | Qué hace |
|---|---|
| `tools/dev_build.sh` | Empaqueta e instala el mod en la instancia `parkour-dev` y compila el juego con el launcher |
| `tools/test.sh` | Tests de la lógica pura (máquina de estados, ajustes) |
| `python3 tools/package.py` | Crea `dist/parkour.hnmod` |

El HUD de depuración se activa en Options > Mods > Parkour.

## Licencia

MIT.
