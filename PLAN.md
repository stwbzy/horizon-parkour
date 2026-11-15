# Parkour — mod de parkour cinemático para Horizon (Minecraft 26.3 en Rust)

Fecha: 2026-11-14 · Estado: fases 0 a 8 hechas (0.1.0 probada en el juego; 0.2.0 sin probar). Siguiente: probar la fase 8 en el juego y publicar la 0.2.0
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
| 8 | Pulido (wall-run vertical, vault bajo, roll) + margen al saltar, inercia de cadena, colgarse y shimmy, tic-tac, sonidos y plancha | Mismas pruebas, comparando con 0.1.0 |

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

### Notas de la fase 2
- Reglas en `land.rs`: corriendo ×0.75, agachado ×0.5, roll ×0.15 (y 0 hasta 8 bloques). "Agachado" exige
  **pulsar** agacharse en el aire, hasta 12 ticks (0.6 s) antes del impacto; llevarlo pulsado desde el suelo no cuenta.
- `camera_shake` está limitado por el motor a 10° y 0.5 bloques por eje (y lo escala el ajuste Distortion
  Effects). Los giros pequeños van por ahí; la voltereta (360° de pitch) y la bajada de cámara van por
  `camera_pose`. Con Cinemática < 100 % el roll no da la vuelta, sólo cabecea. **El wall-run inclinado queda
  limitado a 10° de roll.**
- Red: `parkour:hello` → `parkour:id` (el cliente aprende su id de entidad, porque `living_fall` sólo da ids) y
  `parkour:land` (cómo va a aterrizar; el servidor lo olvida a los 1.5 s). En un jugador el cliente y el
  servidor comparten proceso y se usa directamente lo decidido por el cliente.
- Un aterrizaje sólo cuenta si se llega al suelo rápido (vy ≤ −0.45): bajar una escalera de mano no anima nada.
- Durante el roll (12 ticks) el input se fuerza hacia delante, sin salto ni agachado.

### Notas de la fase 3
- El juego sólo deja **empujar** al jugador (`push_player`), no colocarlo. El mantle (`mantle.rs`) es un
  controlador de velocidad: cada tick calcula el desplazamiento deseado y empuja la diferencia con lo que haría
  la física sola (gravedad 0.08, rozamientos 0.98 / 0.91). Probado contra una simulación en los tests.
- Agarre: en el aire y en caída libre, repisa a ≤ 2.05 sobre los pies y ≤ 0.3 de distancia, mirándola (≤ 53°),
  pidiendo avanzar, sin agacharse y sin caer a más de 0.9 b/t. Desde el suelo, con el salto, alcanza 3 bloques.
- Fases: reach (2 ticks) → pull (0.2 b/t) → over (0.16 b/t). Soltar avanzar durante el pull = colgado (hang);
  agacharse = soltarse (y no se reagarra hasta tocar suelo). Si no sube en 5 ticks (techo) se rinde.
- "Caída libre" (`Tracker::free_fall`) = la velocidad vertical baja lo que toca por gravedad. Excluye volar en
  creativo, élitros y agua, para que los movimientos no salten solos ahí.
- Vanilla sigue contando los bloques caídos antes del agarre y los cobraría al pisar la repisa: mientras dura el
  mantle se avisa `Landing::Grab` (daño ×0), igual que los aterrizajes.
- Pendiente: colgarse de repisas sin sitio encima (el sensor no las da) y shimmy lateral.

### Notas de la fase 4
- Sensor `sense::obstacle`: cara a ≤ 1 bloque delante, borde entre la altura de escalón y 1.55 (vallas), sitio
  libre por encima al menos agachado (1.5) en todo el camino. Fino (fondo ≤ 1.5) = se pasa al otro lado; hondo =
  se acaba encima. `crouch` si de pie no cabe (ventanas).
- Vault (`vault.rs`): saltar corriendo (≥ 0.25 b/t, mirando ≤ 45°) desde el suelo o en los 3 primeros ticks de
  un salto. Se empieza en `movement_input` para quitar el salto de vanilla. Controlador de velocidad como el
  mantle: sube (≤ 0.42 b/t) llegando a la cara justo al pasar el borde y luego avanza a la velocidad de la
  carrera (0.28–0.4) hasta quedar pasado; suelta con la inercia.
- Slide (`slide.rs`): en suelo, corriendo (≥ 0.26) y **recién pulsado** agacharse. Sale a ×1.45 la velocidad
  (máx. 0.5) y frena ×0.955 por tick: unos 5 bloques en ~1 s. Sigue la dirección de la carrera, no la vista.
  Acaba al soltar agacharse (tras 8 ticks), al chocar, al quedarse 3 ticks sin suelo o con la velocidad baja.
  Saltar = slide-jump (salto de sprint con la inercia). 10 ticks de espera entre slides.
- El agachado de vanilla se anula durante el slide (si no, el borde de los bloques frenaría al jugador, como al
  andar agachado); sólo se fuerza con techo bajo encima o delante (`sense::low_ceiling`), para caber por huecos
  de 1.5.
- Mientras el mod mueve al jugador en vault y slide, el avance se deja en 0.01: cuenta como "adelante" y el juego
  no le quita el sprint (vanilla lo quita sin avance).
- El rozamiento del suelo se supone normal (0.6): en hielo el slide va más lejos.

### Notas de la fase 5
- `wallrun.rs`, controlador de velocidad en el aire como los demás.
- **Lateral**: empieza solo, sin tecla: en el aire (caída libre), pared alta a ≤ 0.35 a un lado, ≥ 0.18 b/t a lo
  largo de ella, mirando hacia donde va (no hacia fuera), avanzar pulsado y ≥ 1 bloque de vacío debajo. Corre a
  la velocidad de entrada (0.26–0.34) hasta 28 ticks (~8 bloques) en arco: sube 0.1 b/t al principio y baja
  0.12 al final. Se acaba al soltar avanzar, mirar hacia fuera (> 45°), perder la pared 2 ticks, chocar,
  agacharse o tocar suelo.
- **Vertical**: saltar mirando de frente (≤ 37°) a una pared alta a ≤ 0.7, habiendo corrido en los últimos 6
  ticks (al chocar se frena). Sube 0.42 b/t frenando 0.032 por tick: ~2.5 bloques. Si el borde queda al alcance
  del mantle, se agarra directamente.
- **Wall jump**: pulsar saltar durante el wall-run o hasta 5 ticks después de soltarse. Lateral: 0.32 hacia fuera
  + 0.42 arriba + sigue hacia delante; vertical: 0.32 hacia atrás + 0.42 arriba (sin giro de cámara).
- Hasta 4 paredes sin tocar suelo, y no la misma dos veces seguidas (misma normal): zigzag entre dos paredes sí.
- Cámara: inclinación de 8° hacia fuera de la pared (`camera_shake`, límite 10°), FOV +5 %, balanceo de zancada
  y pisada cada 6 ticks. **El signo de la inclinación está sin comprobar en el juego.**

### Notas de la fase 6
- `fx.rs`: grupo de sonido de vanilla por id de bloque (`block.<grupo>.step/hit/fall`: madera, piedra, hierba,
  arena, grava, lana, nieve, cristal, metal), FOV de la velocidad (+8 % máx. entre 0.3 y 0.55 b/t, o cayendo a más
  de 1 b/t) y un xorshift para repartir partículas.
- Polvo con la textura del bloque (`add_particle` + `ParticleType::Block`; el sprite sale de `block_face_sprite`
  del estado por defecto, cara norte; los teñidos por bioma usan un verde fijo). En aterrizajes (anillo según la
  fuerza), roll, slide (cada 2 ticks, hacia atrás), pisadas en la pared, wall jump, agarre del mantle y apoyo del
  vault. Ajuste Partículas sí/no.
- Manos: corriendo por una pared, la del lado de la pared se adelanta hacia ella. Se supone jugador diestro
  (`Hand::Main` a la derecha).
- Descartado en v1: hitstop/timewarp (los mods no controlan el tiempo de render), viento en bucle (no hay sonido
  en bucle para mods de cliente), 3ª persona animada (decisión 3).

### Notas de la fase 7
- **Consentimiento del servidor**: el cliente sólo hace parkour tras la bienvenida del servidor (`parkour:id`,
  `net::Welcome`: id de entidad, protocolo y permisos `ALLOW_MOVES` / `ALLOW_FALL`). El `HELLO` lleva la versión
  de protocolo (`net::PROTOCOL` = 1); con otra, el servidor avisa una vez por el chat y no contesta. Sin
  bienvenida en 10 s, aviso arriba a la derecha y el parkour queda apagado (servidores sin el mod). Sin
  `ALLOW_FALL`, los aterrizajes se clasifican siempre a plomo (no se rueda para nada).
- **Ajustes del servidor**: `config/parkour-server.txt` (`moves`, `fall_damage`, `validate`), creado con los
  valores de fábrica, releído cada 200 ticks (`config::ServerConfig`).
- **Comprobaciones propias** (`guard.rs`): el `tick` del servidor guarda 60 ticks de posiciones de cada jugador
  (`server_players`). Roll y carrera exigen ≥ 0.15 b/t de media en los últimos 6 ticks (si no, roll → agachado,
  carrera → a plomo); "agarrado" exige estar ≥ 0.5 sobre lo más bajo de esos 3 s (si no, a plomo). Máximo 20
  mensajes por segundo y jugador. El jugador local (un jugador, anfitrión de LAN) usa lo decidido por su cliente.
- **Anticheat de Horizon**: `prediction` (sólo registra por defecto) ve los empujes de cliente como movimientos
  inexplicables; `ground` y `combat` no se ven afectados. README: dejar `prediction` en sólo registrar. Que el
  servidor mande él los empujes (lo que la doc recomienda) haría los movimientos lentos con latencia; no en v1.
- **Servidor dedicado**: `native-build parkour --server-output DIR` compila el motor con el mod (comprobado).
- **Release**: `parkour.hnmod` adjunto a la release de GitHub (lo que instala "Add from GitHub"); sin `.hmod`.

### Plan de la fase 8 (pulido tras la primera prueba)
Prueba del usuario (2026-11-14): todo funciona y es divertido. A mejorar: el wall-run vertical se siente lento y
la mano sólo se levanta; el vault de 1 bloque es lento; en el roll la vuelta de cámara se ve rara y la mano sigue
en pantalla (se encoge pero no se va).

**8.1 Roll** (`cinematic.rs`, `client.rs`)
- Hoy la cámara gira 360° sobre el propio ojo con `smoothstep` (lenta al empezar, ~900°/s en medio) y la bajada
  va por separado: parece una cabeza girando en un pincho. Nuevo: giro alrededor del centro del cuerpo hecho
  bola (~0.5 sobre el suelo), así el ojo avanza, baja y vuelve a subir como en una voltereta de verdad; curva que
  arranca rápido con el impacto y frena al final (ease-out), con los últimos ~0.1 s para incorporarse. La cámara
  nunca baja de 0.3 sobre el suelo.
- Mano: se recoge hacia abajo en el primer 15 % y en el tramo central se oculta del todo (`render_hand` →
  cancelar); vuelve desde abajo al final. También con la cinemática por debajo del 100 %.

**8.2 Vault rápido** (`vault.rs`, `cinematic.rs`, `client.rs`) — obstáculos de ≤ 1.2 bloques
- Hoy: sube y pasa en plano a la velocidad de la carrera (0.28–0.4): ~10 ticks flotando a altura fija.
- Nuevo: subida más rápida (0.5 b/t), pasada en arco (pequeño salto que sube y cae) en vez de plana, +20 % de
  velocidad al apoyar las manos (0.34–0.45) y se suelta en cuanto el cuerpo pasa el borde de atrás, conservando la
  inercia y el sprint. Objetivo: valla de 1 bloque en ~6 ticks (0.3 s) en lugar de ~10.
- Los obstáculos altos (1.2–1.55) siguen como ahora, algo más ágiles.
- Cámara y mano: efecto más corto (0.35 s); la mano principal se apoya (baja y adelante) al empezar la pasada y
  sale por abajo al pasar; pico de FOV al soltar.

**8.3 Wall-run vertical** (`wallrun.rs`, `cinematic.rs`, `client.rs`)
- Hoy: 0.42 b/t frenando 0.032 por tick, ~2.5 bloques en 0.65 s; la mano sube y se queda.
- Nuevo: arranque explosivo, 0.54 b/t frenando 0.05 por tick: ~3.2 bloques en 0.55 s. Pisadas cada 3 ticks
  (antes 6) con un golpe de cámara pequeño en cada una; mirada hacia arriba al despegar (−6°) y pulso de FOV.
- Manos que trepan: en cada zancada la mano sube y se adelanta hasta la pared y luego tira hacia abajo al pasar el
  cuerpo; si la otra mano se dibuja, alternan.

**Verificación**: tests de curvas (mano oculta en el centro del roll, cámara sobre el suelo, giro entero), del
vault bajo (duración y velocidad de salida) y de la subida vertical (altura y ticks); prueba en el juego.

### Notas de la fase 8
- Pedido tras la prueba (2026-11-14), además de 8.1–8.3: margen al saltar, inercia de cadena, colgarse y moverse
  por el borde (que se vea claro que cuelgas: brazos de Minecraft y el objeto en la mano escondido), tic-tac,
  sonidos de los movimientos y plancha.
- **Roll** (`cinematic.rs`): 0.7 s; la cámara gira alrededor de un centro a 0.8 sobre los pies (radios 0.45
  vertical y 0.3 horizontal, `Pose::lunge` por `camera_pose`), el ojo nunca baja de 0.35 sobre el suelo; la vuelta
  (`roll_angle`) acaba al 82 % y deja tiempo para incorporarse. Las manos se recogen y se cancelan con
  `render_hand` en el centro (`Pose::hide_hands`).
- **Vault rápido** (`vault.rs`, hasta 1.2 bloques y sin pasar agachado ni acabar encima): sube a 0.55 b/t, pasa
  a 1.2× la carrera (0.34–0.46) en un arco (0.12 b/t, −0.04 por tick, sin bajar del borde) y suelta al pasar.
  Bloque de 1 desde 0.7: 7 ticks. Efecto `Vault { quick }` de 0.32 s con la mano apoyándose (`hand_forward`).
- **Wall-run vertical**: 0.6 b/t frenando 0.055 por tick (~3 bloques en 10 ticks). Pisadas cada 3 ticks con
  golpe de cámara; efecto `ClimbStart`; manos trepando a contratiempo (`climb_hand`, ciclo de 6 ticks).
- **Margen al saltar** (`assist.rs`): coyote de 3 ticks tras salir de un borde sin saltar (empuje de salto de
  vanilla + 0.2 del sprint); salto guardado 5 ticks para vault, subida y tic-tac; el vault y la subida valen hasta
  6 ticks en el aire mientras no se caiga a más de 0.1 b/t.
- **Inercia de cadena** (`flow.rs`): +6 % por movimiento hasta 3 (+18 %) en vault, slide, wall-run lateral,
  tic-tac y roll, y un empuje en el suelo que lleva la carrera a 0.28 × bonus. Se pierde a los 30 ticks en el
  suelo sin hacer nada o por debajo de 0.2 b/t.
- **Colgarse y shimmy** (`mantle.rs`, `sense::edge`, `arms.rs`): colgado, los pies a 1.75 bajo el borde; de lado
  a 0.085 b/t mientras `sense::edge` vea el mismo borde 0.4 más allá y quepa el cuerpo (`Hold`, medido por el
  cliente). Se cuelga también de bordes sin sitio para subir. Atrás + saltar: 0.3 fuera y 0.42 arriba. Con los pies
  en el suelo deja de colgar.
- **Brazos**: la mano de vanilla no se puede anclar al mundo ni dibujar vacía con un objeto en la mano, así que se
  cancela (`render_hand`) y se dibujan dos brazos con la skin del jugador en `world_submit` (`world_quad` con el
  sprite de la skin del atlas: `client_profiles`, por el UUID que ahora manda la bienvenida, protocolo 2; si no,
  Steve). Manos fijas en el borde, cambiando de sitio mano sobre mano al moverse; hombros relativos a la cámara;
  luz de `Host::light`. No se dibujan en tercera persona (cámara a más de 1 bloque del ojo). Cada cara va en los
  dos sentidos por si el motor descarta caras traseras.
- **Tic-tac** (`wallrun::can_tic_tac`): en el aire, pared delante a ≤ 0.45 mirándola en diagonal (coseno
  0.3–0.82), yendo hacia ella y a lo largo; salto (o guardado) → a lo largo ≥ 0.28 × bonus, 0.3 fuera, 0.45
  arriba. Cuenta como pared usada. Efecto `TicTac` (se ladea hacia donde sale).
- **Plancha**: en un salto con carrera, agacharse en los primeros 8 ticks sin caer a más de 0.3 b/t → empuje 0.08
  y efecto `Dive`; al tocar suelo rueda siempre (roll con empuje 0.18). Necesita `ALLOW_FALL`.
- **Sonidos**: `fx::WHOOSH` (`entity.player.attack.sweep`, agudo y bajo) en wall jump, tic-tac, vault, subida y
  plancha; `fx::CLOTH` al agarrarse, plancha y saltos; golpecitos del material al mover cada mano colgado.
- **Protocolo 2**: la bienvenida lleva el UUID. Cliente y servidor 0.2.0 deben ir juntos.

## 9. Riesgos y mitigaciones
- **Anticheat/vanilla del servidor** corrige movimientos agresivos → velocidades conservadoras, impulsos cortos y configurables.
- **Hooks sin garantía de orden entre mods** (living_camera) → todo aditivo; pruebas con ambos activos.
- **Sin huesos en 3ª persona** → ver §6.
- **Rendimiento**: el sensor de colisión hace pocas consultas por tick (cache por posición de bloque); medir en fase 1.
- **Sensación (game feel)**: es lo que más tiempo toma; todas las constantes en `config` con valores por defecto y recarga en caliente para iterar sin recompilar.

### Notas del arreglo del roll (0.2.1)
- La pulsación de agacharse se cuenta en `movement_input` (antes de que el juego mueva al jugador), así que
  vale también la del tick en que toca suelo; antes se perdía y además arrancaba un slide.
- Slide bloqueado 5 ticks tras un aterrizaje de verdad.
- Correr para rodar = velocidad horizontal más alta de los últimos 6 ticks ≥ 0.2 (antes la del último tick ≥ 0.25).
- Ventana de 12 ticks, +1 por bloque de caída por encima de 4, hasta 20.
- Plancha sólo en los 4 primeros ticks del salto y subiendo; la plancha siempre acaba en roll.
- Límite conocido: en servidor dedicado, el aviso de una pulsación en el último tick llega después del paquete
  de movimiento que calcula el daño. En un solo jugador/LAN propio no pasa. Arreglarlo exigiría aplazar el daño
  en el servidor y recalcularlo (no hay API para devolver vida); pendiente si molesta en partidas con amigos.
