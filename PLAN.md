# Parkour — mod de parkour cinemático para Horizon (Minecraft 26.3 en Rust)

Fecha: 2026-11-14 · Estado: fases 0 y 1 hechas (esqueleto, HUD de depuración, sensores); siguiente: fase 2 (aterrizaje y roll)
Mod hermano de `~/Proyectos/horizon-armas` (Arsenal): misma instancia, mismas rutas, mismo flujo de build.

## 1. Rutas y base técnica (reutilizado de Arsenal)

| Ruta | Uso |
|---|---|
| `~/Proyectos/horizon-parkour/` | Este proyecto (mod en `parkour/`, contenido en `content/`, scripts en `tools/`) |
| `~/Proyectos/horizon-armas/tools/dev_build.sh` | Plantilla del build de desarrollo (instancia `parkour-dev`, empaqueta `.hnmod`, apaga otros mods nativos) |
| `~/.local/share/horizon/` | Instancia del juego |
| `~/.local/share/horizon-launcher/native/stage/` | Fuente de Horizon `fdc87ae` + `docs/native-mods.md` + `native-hooks.json` |
| `~/Proyectos/horizon-vr/` | Fork del motor (sólo si algún día hace falta algo que los hooks no dan) |

- Mod **trait** (`[api]` crate que implementa `mod_api::Hooks`), sólo hooks del catálogo → `"hooks": 411`, sin fijar commit.
- Cliente y servidor necesitan el mod. Contenido (sonidos, partículas, modelos) en `.hmod` con el mismo id.
- Compatibilidad: `living_camera` comparte `camera_shake` / FOV / `render_hand_pose` → todo **aditivo** (nunca "set"), igual que en Arsenal. Con Arsenal conviven: pose de mano sólo se toca cuando no hay arma en mano o durante una acción de parkour (la acción tiene prioridad unos ms).

## 2. Qué da el motor (y qué no) — decide la arquitectura

| Necesidad | Hook / helper | Veredicto |
|---|---|---|
| Leer input y alterarlo | `movement_input` (cliente, antes de aplicar) | OK |
| Empujar al jugador | `native_push_player([x,y,z])` (cliente, bloques/tick), `entity_set_velocity` | OK (cliente) |
| Detectar paredes/repisas | `native_block_collision(pos)` → cajas locales; `raycast` | OK, geometría exacta (escaleras, losas…) |
| Suelo / posición / ojos | `entity_on_ground`, `entity_position`, `entity_eye_position` | OK |
| Cámara: pitch/yaw/**roll** | `camera_shake` (aditivo), `camera_pose` (posición, yaw, pitch), `compute_fov_modifier` | OK — roll por `camera_shake` → wall-run inclinado |
| Manos en 1ª persona | `render_hand_pose` (x,y,z,pitch,yaw,roll,scale), `render_hand` (cancelar) | OK, pero **sólo transforma** la mano vanilla; no dibuja otra |
| Caída | `living_fall(distance, multiplier, cancel)` — cliente y servidor; `block_fall_on` (servidor) | OK |
| Daño | `living_incoming_damage_v3` | OK (respaldo) |
| Teclas propias | `mod_key` / `set_mod_key` (+ `key`) | OK |
| Sonido / partículas | helpers de Host | OK |
| Cuerpo en 3ª persona | `player_submit` + `player_render_listened`: dibuja geometría propia anclada al frame del jugador, **no a los huesos** | **Limitado** (ver §6) |
| Pose del jugador | `entity_pose_change` (servidor) | sólo lectura |

### Riesgo principal: autoridad del movimiento
El movimiento es del **cliente**; el servidor valida con vanilla (“moved too quickly / wrongly”) y el anticheat
de `horizon-server` (`ground` actúa; `timer`/`prediction` sólo loguean). Consecuencias de diseño:
1. Nunca superar velocidades que vanilla ya tolera con holgura (sprint-salto ≈ 0.36 b/t; wall-run ≤ ~0.3 b/t horizontal).
2. Subidas (mantle) como **impulsos verticales cortos**, no teletransporte → parecen un salto normal al servidor.
3. El claim `on_ground` no se falsea: el rodar no se basa en mentir sobre el suelo, sino en cambiar el daño (§4.3).
4. Daño de caída decidido en el **servidor** (`living_fall` corre en ambos lados; el servidor consulta el estado “rodando” que el cliente le avisa por mensaje de mod y que él valida con su propia ventana de tiempo).
5. Modo de seguridad en config: `strict_server_validation` (si el servidor es dedicado y registra setbacks, desactiva movimientos de impulso fuerte).

## 3. Arquitectura del mod

```
parkour/api/src/
  lib.rs            impl Hooks: registra hooks, dispatch
  state.rs          PlayerParkour: máquina de estados por jugador (cliente) + espejo mínimo en servidor
  sense.rs          sensores: paredes, repisas, ancho de hueco, altura libre (usa native_block_collision)
  moves/
    wallrun.rs      lateral + vertical
    mantle.rs       subida de manos (≤ 2.x bloques) + hang/shimmy
    vault.rs        saltar obstáculo bajo (hasta 1 bloque) sin perder velocidad
    roll.rs         parachute roll / aterrizaje amortiguado
    slide.rs        deslizamiento (propuesta)
    …
  cinematic.rs      director: curvas de cámara (roll/FOV/bob), pose de manos, timewarp
  net.rs            mensajes cliente→servidor (parkour:state), validación servidor
  config.rs         ajustes (Options > Mods): sensibilidad, intensidad cinemática, activar/desactivar movimientos
```

- **Máquina de estados** (Idle → Ground-run → {Vault, Mantle, Slide, WallRun, Roll, Hang…}) con transiciones explícitas y tiempos mínimos entre movimientos (evita encadenados rotos y spam al servidor).
- **Tick de cliente** en `movement_input` + `frame` para animación (interpolación con `partial`).
- Servidor: espejo mínimo (`state` + `last_move_tick`) vía canal `parkour:state` (rate-limit; máx. 32 msgs/tick/jugador).

## 4. Movimientos pedidos — diseño detallado

### 4.1 Wall run lateral (pared a un lado, inclinado)
- **Entrada**: en el aire, sprint/velocidad horizontal ≥ umbral, pared sólida a ≤ 0.6 b en el eje lateral (cajas de `block_collision`, ≥ 2 bloques de altura de contacto), mirando ≲ 60° respecto a la pared; mantener adelante.
- **Física**: se anula gravedad parcialmente (cada tick `push_player` compensa ~85 % de la gravedad), velocidad hacia delante = conservada (cap 0.30 b/t), leve atracción hacia la pared (mantiene contacto). Duración máx. ~1.4 s (barra de “agarre” que decae), sube un poco al inicio (arco).
- **Salida**: salto → *wall jump* (impulso hacia fuera 45° + arriba); soltar adelante/llegar al final de la pared → caída normal; mirar lejos de la pared → se suelta.
- **Cinemática**: roll de cámara ±8–12° hacia el lado opuesto a la pared (`camera_shake.roll`, con ease in/out), FOV +4–6 %, balanceo de pasos a ritmo de zancada, mano del lado de la pared “roza” (render_hand_pose: se adelanta y se apoya), partículas de polvo del bloque tocado, sonido de roce.
- Pared “inclinada” se interpreta como **pared lateral con la cámara inclinada** (estilo Mirror's Edge). Si además quieres que funcione en paredes no axiales (esquinas diagonales), se generaliza con la normal calculada de las cajas.

### 4.2 Wall run vertical (subir por la pared) + wall kick
- **Entrada**: correr hacia una pared de frente (≤ 0.7 b) con velocidad y saltar → “carrera vertical”: impulso hacia arriba que decae (≈ 2.5–3 bloques máx.).
- **Salidas**: (a) llega a una repisa → enlaza con **mantle**; (b) salto → *wall kick* hacia atrás/lateral (giro de cámara 180° opcional asistido); (c) agotada la energía → cae.
- **Cinemática**: cámara con pitch hacia arriba suave, manos alternan “agarrando” (render_hand_pose oscilante), zoom FOV al despegar, partículas por pisada.
- Comparte la barra de “agarre” con el wall-run lateral (no se encadenan infinitamente; se recarga al tocar suelo).

### 4.3 Aterrizaje amortiguado: correr + agacharse + rodar (parachute roll)
- Daño de caída = `vanilla × multiplicador` mediante `living_fall` (ambos lados):
  - **Correr** (velocidad horizontal ≥ umbral) en el aterrizaje: ×0.75 (“absorbes menos daño al correr y caer”).
  - **Agacharse** (sneak pulsado en ventana [−0.35 s, +0.1 s] alrededor del impacto): ×0.5.
  - **Roll**: correr + agachar en ventana → ×0.15 y, si la caída ≤ ~8 bloques, **0 daño** (vale la pena porque conserva el impulso). Se pierde el roll si se pulsa tarde (golpe seco, ×1.0 + aturdimiento leve: breve lentitud).
  - Todo apilable con Feather Falling de vanilla (no se pisa: se multiplica `event.multiplier`).
- Roll = **animación cinemática** de ~0.55 s: cámara hace una vuelta completa de pitch (camera_shake.pitch con curva), FOV pulse, manos cruzadas (pose), sonido de ropa + golpe sordo, partículas de impacto, movimiento hacia delante conservado (impulso en el sentido que mirabas).
- Servidor: valida que el cliente estuvo “rodando” por ventana de tiempo coherente; si no coincide → daño normal (no se confía en un flag del cliente sin ventana).

### 4.4 Vault (saltar obstáculos bajos amortiguando)
- Obstáculo de altura 0.5–1.1 bloques (vallas, losas a nivel de cadera, barriles) con espacio libre detrás o encima; sprint hacia él + salto/Ctrl.
- Tres variantes: **speed vault** (una mano, mantiene velocidad), **lazy vault** (apoyo y salto lateral), **kong** (si hay hueco ≥ 1 b: impulso de tronco).
- Al terminar el vault en una caída ≤ 3 b se aplica el mismo multiplicador de agacharse (sirve para “amortiguar”).
- Cinemática: mano hacia el borde del obstáculo (pose), cámara baja ~0.2 b durante el apoyo (`camera_pose`, pos Y), FOV pico en la salida.

### 4.5 Mantle — “subida simple de manos”, más de 2 bloques
- **Entrada**: caer/saltar hacia una repisa donde `top_edge ≤ eye + 0.4` y ≤ ~2.5 b sobre los pies (más de dos bloques con el salto previo, o hasta 3 con wall-run vertical antes); espacio libre sobre la repisa ≥ altura del jugador (1.8, o 1.5 si te agachas).
- **Fases**: **reach** (manos suben, 0.15 s) → **grip** (cuelgas, 0.1 s, pausa cinemática opcional con respiración/ruido) → **pull-up** (impulso vertical + hacia delante, 0.35 s) → **stand** (sale con un pequeño tropiezo de cámara).
- **Control**: salto/avanzar para subir; agacharse para soltarse. Si no hay espacio libre → sólo hang (colgado) y posibilidad de shimmy lateral.
- Servidor: parece un salto + escalón de ~1 b/t²; sin teletransportes. Hecho con impulsos cortos y reanudados.

## 5. Otros movimientos de parkour propuestos

| # | Movimiento | Resumen | Dificultad |
|---|---|---|---|
| 1 | **Slide** (deslizamiento) | Sprint + agacharse en suelo → desliza 6–8 bloques, hitbox baja a 1 b (pasa por huecos de 1.5 b); salto al final = “slide-jump”. | Media |
| 2 | **Wall jump / wall kick** | Salto contra pared en el aire; reinicia ligeramente la barra de agarre; rebote entre dos paredes cercanas (“wall-zigzag”). | Media |
| 3 | **Ledge hang & shimmy** | Colgarte de un borde; desplazarte lateralmente; subir (mantle) o soltarte; cambio de repisa por salto lateral. | Media |
| 4 | **Pole / bar swing** | Barras/cadenas (cualquier bloque con `end_rod`/`chain` configurable): te agarras, giras alrededor y sales con impulso. | Media-alta |
| 5 | **Precision jump** | Mantener agachado al despegar para “clavar” un salto corto exacto (sin deriva, aterrizaje agarrándose al borde). | Baja |
| 6 | **Cat leap / ledge grab de caída** | Si caes junto a una pared con repisa a ≤ 0.6 b de distancia, clic para agarrarte (cooldown). | Media |
| 7 | **Tic-tac** | Pared de frente, salto + empuje hacia un lado para superar un hueco de 1–2 b. | Baja |
| 8 | **Zip/Ziplines** (entidades de cuerda) | Cuerda entre dos bloques; te cuelgas y deslizas; salto para soltarte. Requiere entidad propia → fase tardía. | Alta |
| 9 | **Climb-up de esquinas** | Escalar esquinas exteriores 90° agarrado (cornering) sin caída. | Media |
| 10 | **Diving roll** | Salto adelante en carrera → picado + roll al caer (conserva más velocidad que el roll normal). | Media |
| 11 | **Ground pound / Stomp** (opcional) | Agacharse en el aire → bajada rápida con onda de choque al aterrizar (no hace daño a bloques). | Baja |
| 12 | **Momentum meter** | Barra (HUD discreto) que sube al encadenar movimientos; da más vel. y sube la “cinemática” (zoom, viento, estela de partículas). | Media |

Recomendación de v1: 4.1 + 4.2 + 4.3 + 4.4 + 4.5 + slide + wall jump + ledge hang. El resto, v1.x.

## 6. “Demasiado cinemático” — plan de animaciones

**Capa 1 (barata, viable ya) — primera persona**
- Cámara: roll/pitch/yaw por `camera_shake` (aditivo), posición Y y offset lateral por `camera_pose` (no se toca la posición real del jugador), FOV por `compute_fov_modifier`.
- Manos: `render_hand_pose` con curvas por movimiento (reach, grip, pull-up, roce de pared…). Mano oculta/cancelable con `render_hand`.
- Curvas: motor propio de keyframes con easing (cubic, back, overshoot), sampling con `partial` para suavidad a 144 Hz.
- Timewarp suave (“hitstop” de 2–3 ticks) en el pico de impacto, sólo visual (no toca ticks del servidor).
- Sonido + partículas por fase: roces, esfuerzo, golpe, polvo, viento a alta velocidad.

**Capa 2 (3ª persona, requiere decisión) — cuerpo**
`player_submit` sólo permite dibujar geometría fija al frame del jugador, **sin tocar huesos** del modelo vanilla. Opciones:
1. **Cuerpo propio**: ocultar el modelo vanilla (`drawn = true`) y dibujar nuestro maniquí (cubos articulados en Rust, con poses por keyframes) durante las acciones de parkour. Funciona, pero pierde skin y capas (armadura, capa, objetos) durante la acción. Se puede aplicar la skin si el helper de textura de jugador lo permite (**por verificar**).
2. **Sólo 1ª persona** en v1, 3ª persona sin animación específica (sólo mover el cuerpo vía pose/rotación vanilla).
3. **Fork de motor** (como Horizon-VR) para exponer huesos del jugador → más trabajo, más mantenimiento; sólo si 1 y 2 no bastan.
- Recomendación: capa 1 completa en v1; capa 2 con la opción 1 en la fase 6 tras comprobar la skin.

**Opcionales cinemáticos**: cámara “handheld” sutil al correr, lens-distortion/viñeta en wall-run (si hay hook de post-proceso; si no, se descarta), polvo/viento en pantalla.

## 7. Fases de desarrollo

| Fase | Entregable | Verificación |
|---|---|---|
| 0 | Esqueleto del mod (`parkour/`, `dev_build.sh`, `tools/package.py`, repo local) + hook de depuración en pantalla (estado actual) | Compila, aparece en Options > Mods |
| 1 | Sensores (`sense.rs`): wall left/right/front, ledge, ancho de hueco, altura libre; HUD de depuración | Mapa de pruebas con pared y repisas; capturas |
| 2 | **Aterrizaje/rodar** (`living_fall` + roll) + animación cámara | Caída de 5/10/20 bloques con y sin sneak; mensajes en servidor |
| 3 | **Mantle** + hang | Subida a repisa de 2/2.5/3 b; “sin espacio” sólo cuelga |
| 4 | **Vault** + slide | Vallas, losas, huecos de 1.5 b |
| 5 | **Wall-run lateral y vertical** + wall jump | Pasillo con paredes; cadenas de movimientos |
| 6 | Capa cinemática completa (curvas, sonido, partículas) + opcional cuerpo 3ª persona | Comparación visual; rendimiento |
| 7 | Multijugador (LAN + servidor dedicado), anticheat, config, empaquetado y release | Dos clientes, `logs/anticheat.log` limpio |

## 8. Decisiones (confirmadas el 2026-11-14)

1. Nombre e id `parkour`; mod en `~/Proyectos/horizon-parkour/parkour`.
2. v1 (MVP): los 5 movimientos pedidos + slide, wall jump y ledge hang.
3. 3ª persona sin animación propia en v1.
4. Controles sólo contextuales (salto, sprint, agacharse). Ninguna tecla propia.
5. Servidores de amigos (LAN / hosting propio), no servidores públicos de terceros.
6. Repositorio público `stwbzy/horizon-parkour`.

### Notas de la fase 0
- `living_fall` y `living_jump` son funciones libres: no reciben `Host`, sólo el id de red de la entidad.
- `Host::player()` da posición, yaw, pitch y dimensión, pero no el `EntityRef` del jugador local; la fase 0
  deduce velocidad y suelo de la posición. La fase 1 debe conseguir el `EntityRef` (para `entity_on_ground`,
  `entity_is_sprinting`, `entity_bounding_box`) o quedarse con la deducción.
- El `.hmod` de contenido sólo se empaqueta cuando `content/` tenga assets (fase 6).
- `ref/` (ignorado por git) guarda la API generada (`mod_api_generated.rs`, Host) para consultar firmas.

### Notas de la fase 1
- Los sensores (`sense.rs`) salen sólo de `block_collision`: suelo, paredes izq/der/frente (≥ 2 bloques, por
  encima de la altura de escalón 0.6), repisa (borde ≤ 3.3 sobre los pies, con sitio de pie o agachado y hueco
  libre para subir), techo y vacío bajo los pies. No hace falta el `EntityRef` del jugador: el suelo también es
  geométrico, así que la deducción por velocidad vertical de la fase 0 desaparece.
- Las cajas se piden una vez por bloque y tick (cache en `client.rs`).
- Las normales de pared están alineadas a ejes (la cara del bloque tocado).

## 9. Riesgos y mitigaciones
- **Anticheat/vanilla del servidor** corrige movimientos agresivos → velocidades conservadoras, impulsos cortos y configurables.
- **Hooks sin garantía de orden entre mods** (living_camera) → todo aditivo; pruebas con ambos activos.
- **Sin huesos en 3ª persona** → ver §6.
- **Rendimiento**: el sensor de colisión hace pocas consultas por tick (cache por posición de bloque); medir en fase 1.
- **Sensación (game feel)**: es lo que más tiempo toma; todas las constantes en `config` con valores por defecto y recarga en caliente para iterar sin recompilar.
