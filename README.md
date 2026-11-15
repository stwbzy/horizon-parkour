# Parkour — mod de parkour cinemático para Horizon

Mod nativo para [Horizon](https://github.com/horizon-rust/horizon) (Minecraft 26.3 en Rust): movimiento de
parkour al estilo Mirror's Edge, con cámara y manos animadas en primera persona.

**Estado: versión preliminar (0.1.0).** Todos los movimientos de la v1 están hechos, pero varios aún no se han
probado a fondo dentro del juego: espera ajustes de velocidades y sensaciones.

## Movimientos

Sin teclas propias: todo es contextual con avanzar, salto, sprint y agacharse.

- **Aterrizaje**: al caer, correr quita daño, pulsar agacharse en el aire quita más, y las dos cosas a la vez
  hacen rodar (sin daño hasta 8 bloques).
- **Mantle**: salta hacia una repisa manteniendo avanzar y te agarras al borde y subes a pulso (hasta 3 bloques
  desde el suelo). Suelta avanzar para quedarte colgado; agáchate para soltarte.
- **Vault**: corriendo hacia un obstáculo bajo (de un escalón a 1.5 bloques, vallas incluidas), salta y lo pasas
  apoyando las manos sin perder la carrera. Si es hondo, acabas de pie encima.
- **Slide**: corriendo, pulsa agacharse y te deslizas unos 5 bloques (por huecos de 1.5 también). Salta a media
  deslizada para salir con la inercia.
- **Wall-run lateral**: salta corriendo junto a una pared (mirando hacia donde vas, manteniendo avanzar) y corres
  por ella con la cámara inclinada, unos 8 bloques.
- **Wall-run vertical**: salta corriendo de frente a una pared y subes unos 2.5 bloques; si el borde queda al
  alcance, te agarras y subes (mantle).
- **Wall jump**: salta durante un wall-run (o justo al soltarte) y te impulsas hacia fuera; puedes encadenar
  hasta 4 paredes distintas sin tocar el suelo.

Cada movimiento tiene su animación de cámara y manos en primera persona, suena según el material del bloque que
tocas (madera, piedra, arena…) y levanta polvo de ese bloque. Ir más rápido que esprintando abre un poco el FOV.
En Options > Mods > Parkour: intensidad de la cinemática (0–150 %), partículas sí/no y HUD de depuración.

El diseño completo y las fases están en [PLAN.md](PLAN.md).

Sin teclas propias: todo es contextual con salto, sprint y agacharse.

## Instalación

Es un mod **nativo** (código Rust que el launcher compila dentro del juego); no tiene parte de contenido
(`.hmod`).

1. En el launcher de Horizon: **Mods → Add from GitHub** y pega `https://github.com/stwbzy/horizon-parkour`.
   Acepta el aviso y pulsa **Build**. (Por terminal:
   `horizon-launcher mods add-github https://github.com/stwbzy/horizon-parkour --trust-native-code` y después
   `horizon-launcher native-build --trust-native-code`.) La primera compilación tarda: el launcher descarga Rust
   y la fuente de Horizon.
2. Abre el juego, ve a **Opciones → Mods**, activa **Parkour** y reinicia.
3. Al entrar a un mundo verás en el chat `[Parkour 0.1.0] Este servidor tiene Parkour…`.

Para actualizar: `horizon-launcher mods update-github` y vuelve a compilar.

## Multijugador

El mod tiene dos mitades en el mismo código: la del cliente (movimientos, cámara, sonido) y la del servidor
(quita el daño de caída y comprueba a los clientes). **El servidor tiene que tener el mod**, y la misma versión:

- **Un jugador**: funciona sin más (el mundo lleva su propio servidor).
- **LAN**: el que abre el mundo y los que se unen necesitan el mod.
- **Servidor dedicado (`horizon-server`)**: exporta un motor con el mod y apunta el servidor a él:

  ```sh
  horizon-launcher native-build /ruta/a/horizon-parkour/parkour --server-output /ruta/export \
      --version <versión de Horizon> --trust-native-code
  horizon-server --dir SERVIDOR config set engine.path=/ruta/export/horizon-engine
  horizon-server --dir SERVIDOR restart
  ```

  (en el mismo sistema operativo y arquitectura que el servidor; ver `docs/hosting.md` de Horizon).
- **Servidores sin el mod** (vanilla, otros): el parkour se queda **apagado** y se avisa arriba a la derecha.
  El movimiento lo decide tu cliente, y moverse así en un servidor que no lo ha permitido sería hacer trampa.
- Con versiones distintas de cliente y servidor, el servidor avisa por el chat y el parkour se queda apagado.

### Ajustes del servidor

En `config/parkour-server.txt` (junto a `server.properties` en un dedicado, o en la carpeta de la instancia en
un jugador y LAN), creado la primera vez y releído cada 10 s:

| Clave | Por defecto | Qué hace |
|---|---|---|
| `moves` | `true` | Deja usar los movimientos. Con `false`, los clientes juegan como en vanilla |
| `fall_damage` | `true` | Aterrizar bien (correr, agacharse, rodar, agarrarse) quita daño de caída |
| `validate` | `true` | Comprueba lo que dicen los clientes: rodar o correr al caer exige haber llegado corriendo, y agarrarse exige haber subido después. Lo que no encaja se rebaja (un roll falso cuenta como agacharse), sin castigar |

### Anticheat de Horizon

Los movimientos empujan al jugador desde su propio cliente, así que el chequeo `prediction` del anticheat de
`horizon-server` los ve como movimientos que no sabe explicar. Por defecto ese chequeo **sólo registra**
(`logs/anticheat.log`) y no hace nada más; si lo has configurado para devolver jugadores (`setback`), los
movimientos de parkour rebotarán. En un servidor con Parkour, deja `prediction` en sólo registrar:

```toml
[anticheat.prediction]
setback = 0
alert = 0
kick = 0
```

Los chequeos que actúan por defecto (`ground`, `combat`) no se ven afectados: el mod no falsea el suelo ni los
ataques.

## Desarrollo

| Comando | Qué hace |
|---|---|
| `tools/dev_build.sh` | Empaqueta e instala el mod en la instancia `parkour-dev` y compila el juego con el launcher |
| `tools/test.sh` | Tests de la lógica pura (estados, sensores, aterrizaje, mantle, vault, slide, wall-run, curvas de cámara, red, comprobaciones del servidor) |
| `python3 tools/package.py` | Crea `dist/parkour.hnmod` |

## Licencia

MIT.
