//! Mitad cliente: sigue al jugador local tick a tick, lee los sensores, decide los movimientos y
//! anima cámara y manos. También dibuja el HUD de depuración.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use horizon_client::mod_api::{
    CameraPoseEvent, CameraShakeEvent, ComputeFovModifierEvent, Corner, Host, HudEvent,
    MovementInputEvent, NetFromServerEvent, RenderHandPoseEvent,
};

use crate::cinematic::{self, Effect, Pose};
use crate::config;
use crate::land::{self, Landing};
use crate::mantle::{self, End, Mantle, Phase};
use crate::net;
use crate::sense::{self, Aabb, Senses, Wall};
use crate::slide::{self, Slide};
use crate::state::{Input, Move, RUN_SPEED, Tracker};
use crate::vault::{self, Vault};
use crate::wallrun::{self, Kind, WallRun};

const LINE_H: f32 = 10.0;
const PAD: f32 = 3.0;

/// Cada cuántos ticks se repite la pregunta por el id propio, y el aviso de aterrizaje en caída.
const HELLO_EVERY: u32 = 40;
const LAND_EVERY: u32 = 10;
/// Caída desde la que se avisa al servidor de cómo se va a aterrizar.
const WARN_FALL: f64 = 1.0;
/// Velocidad vertical (bloques/tick) por debajo de la cual tocar suelo es un aterrizaje de
/// verdad: bajar una escalera de mano o por agua también "cae" varios bloques, pero despacio.
const IMPACT_SPEED: f64 = -0.45;
/// Lo que vale el aterrizaje decidido aquí cuando `living_fall` pregunta.
const LANDING_FRESH: Duration = Duration::from_millis(400);
/// Empujón hacia delante al empezar a rodar, en bloques por tick.
const ROLL_PUSH: f64 = 0.1;
/// Lo que suben y se adelantan las manos agarradas a una repisa, en bloques de la vista.
const HANDS_UP: f32 = 0.35;
const HANDS_FORWARD: f32 = 0.15;
/// Cuánto se acercan las manos a su postura en cada tick.
const HANDS_EASE: f32 = 0.4;
/// Lo mismo para la cámara al deslizarse.
const SLIDE_EASE: f32 = 0.35;
/// Avance que se deja pedir mientras el mod mueve al jugador: casi nada, pero cuenta como
/// "adelante" y así el juego no le quita el sprint.
const KEEP_SPRINT: f32 = 0.01;
/// Ticks desde que corría con los que aún vale para subir por una pared (al chocar se frena).
const RAN_TICKS: u32 = 6;
/// Ticks por zancada corriendo por una pared (sonido de pisada y balanceo).
const STRIDE_TICKS: u32 = 6;

#[derive(Default)]
pub struct Client {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    tracker: Tracker,
    senses: Senses,
    /// El id de red del jugador, cuando el servidor lo ha dicho.
    own_id: Option<u32>,
    hello_wait: u32,
    /// Cómo va a aterrizar (o acaba de hacerlo) y cuándo se decidió.
    landing: Option<(Landing, Instant)>,
    /// El último aviso mandado al servidor y los ticks desde entonces.
    sent: (Landing, u32),
    /// El efecto de cámara en curso.
    effect: Option<(Effect, Instant)>,
    /// El ajuste Cinemática, de 0 a 1.5 (se lee una vez por tick).
    intensity: f32,
    /// La subida a una repisa en curso.
    mantle: Option<Mantle>,
    /// El mantle pide ir agachado este tick.
    mantle_sneak: bool,
    /// Tras soltarse de una repisa no se vuelve a agarrar hasta tocar suelo.
    grab_locked: bool,
    /// Manos levantadas hacia el borde, de 0 a 1: el tick anterior y éste, para interpolar.
    hands: [f32; 2],
    /// Hacia dónde mira el jugador (de su yaw), con lo que se midió el obstáculo.
    facing: [f64; 2],
    /// El vault en curso, y si pide ir agachado.
    vault: Option<Vault>,
    vault_sneak: bool,
    /// El slide en curso, si pide ir agachado (techo bajo) y los ticks hasta poder repetirlo.
    slide: Option<Slide>,
    slide_sneak: bool,
    slide_cool: u32,
    /// Cámara al ras del suelo por el slide, de 0 a 1: el tick anterior y éste.
    sliding: [f32; 2],
    /// Movimiento empezado en `movement_input`, para darle sonido y efecto en el tick.
    started: Option<Move>,
    /// El wall-run en curso, y el último tras soltarse mientras aún vale saltar de él.
    wallrun: Option<WallRun>,
    wall_grace: Option<(WallRun, u32)>,
    /// Paredes usadas desde el último suelo, y la última (no se repite la misma seguida).
    walls_used: u8,
    last_wall: Option<[f64; 2]>,
    /// Se pulsó saltar desde el último tick.
    jump_queued: bool,
    /// Ticks desde la última vez que corría.
    ran_ago: u32,
    /// Cámara del wall-run: lado (-1 a 1) y subida (0 a 1), el tick anterior y éste; y la zancada.
    wall_side: [f32; 2],
    wall_up: [f32; 2],
    stride: [f32; 2],
}

/// El mundo de la ventana para los sensores. Guarda las cajas ya pedidas: en un tick los
/// sensores preguntan varias veces por los mismos bloques.
struct HostWorld<'a> {
    host: &'a dyn Host,
    cache: RefCell<HashMap<[i32; 3], Vec<Aabb>>>,
}

impl sense::World for HostWorld<'_> {
    fn boxes(&self, pos: [i32; 3]) -> Vec<Aabb> {
        self.cache.borrow_mut().entry(pos).or_insert_with(|| self.host.block_collision(pos)).clone()
    }
}

impl Client {
    /// Un tick de la ventana (no del servidor integrado).
    pub fn tick(&self, host: &mut dyn Host) {
        let Ok(mut inner) = self.inner.lock() else { return };
        let Some(player) = host.player() else {
            *inner = Inner::default();
            return;
        };
        let feet = [player.x, player.y, player.z];
        let world = HostWorld { host: &*host, cache: RefCell::default() };
        inner.senses = sense::sense(&world, feet, player.yaw);
        inner.facing = sense::forward(player.yaw);
        let low_ceiling = inner.slide.is_some_and(|s| sense::low_ceiling(&world, feet, s.dir));
        let run_wall = inner.wallrun.and_then(|w| match w.kind {
            Kind::Side => sense::tall_wall(&world, feet, w.toward()),
            // Subiendo, vale cualquier pared delante del cuerpo, aunque arriba se acabe.
            Kind::Up => sense::sweep(
                &world,
                feet,
                w.toward(),
                sense::WALL_REACH,
                feet[1] + sense::STEP_HEIGHT,
                feet[1] + sense::STAND_HEIGHT,
            ),
        });
        drop(world);
        let ground = inner.senses.ground;
        let fall_speed = inner.tracker.vel[1];
        inner.tracker.tick(feet, ground);
        if ground {
            inner.walls_used = 0;
            inner.last_wall = None;
            inner.wall_grace = None;
        }
        inner.ran_ago = if inner.running() { 0 } else { inner.ran_ago.saturating_add(1) };

        let stored = host.setting(crate::NAMESPACE, config::CINEMATIC.key);
        inner.intensity =
            config::CINEMATIC.value(stored.as_deref()).parse::<f32>().unwrap_or(100.0) / 100.0;

        inner.ask_own_id(host);
        inner.announce(host, feet);
        let mantling = inner.mantle(host, feet, player.yaw);
        let vaulting = inner.vault(host, feet);
        inner.slide(host, low_ceiling);
        let wallrunning = inner.wallrun(host, feet, run_wall);
        inner.jump_queued = false;
        if !mantling && !vaulting && !wallrunning {
            inner.warn_landing(host);
            if let Some(fall) = inner.tracker.landed
                && fall >= land::MIN_FALL
                && fall_speed <= IMPACT_SPEED
            {
                inner.land(host, fall, feet, player.yaw);
            }
        }
        let raised = inner.mantle.is_some_and(|m| m.phase != Phase::Over);
        let now = inner.hands[1];
        inner.hands = [now, now + (f32::from(raised) - now) * HANDS_EASE];
        let low = inner.slide.is_some();
        let now = inner.sliding[1];
        inner.sliding = [now, now + (f32::from(low) - now) * SLIDE_EASE];
        let (side, up) = match inner.wallrun {
            Some(w) if w.kind == Kind::Side => {
                let r = sense::right(player.yaw);
                (-(w.normal[0] * r[0] + w.normal[1] * r[1]) as f32, 0.0)
            }
            Some(_) => (0.0, 1.0),
            None => (0.0, 0.0),
        };
        let now = inner.wall_side[1];
        inner.wall_side = [now, now + (side - now) * SLIDE_EASE];
        let now = inner.wall_up[1];
        inner.wall_up = [now, now + (up - now) * SLIDE_EASE];
        let now = inner.stride[1];
        let next = if inner.wallrun.is_some() { now + 1.0 / STRIDE_TICKS as f32 } else { now };
        // Sin saltos al dar la vuelta: se interpola entre `now` y `next` y luego se reduce.
        inner.stride = if next >= 1.0 { [now - 1.0, next - 1.0] } else { [now, next] };
    }

    /// Aquí empiezan el vault (saltar corriendo hacia un obstáculo bajo) y el slide (agacharse
    /// corriendo), para quitarle al juego ese mismo salto o agachado.
    ///
    /// Mientras rueda, el jugador sigue hacia delante: no frena, no salta ni se queda agachado.
    /// Agarrado a una repisa, saltando un obstáculo o deslizando lo mueve el mod, no las teclas;
    /// en vault y slide se deja un mínimo de avance para no perder el sprint.
    pub fn movement_input(&self, event: &mut MovementInputEvent) {
        let Ok(mut inner) = self.inner.lock() else { return };
        let input =
            Input { forward: event.forward, strafe: event.strafe, jump: event.jump, sneak: event.sneak };
        let sneak_pressed = input.sneak && !inner.tracker.input.sneak;
        if input.jump && !inner.tracker.input.jump {
            inner.jump_queued = true;
        }
        inner.tracker.input = input;
        inner.try_start(input, sneak_pressed);
        if inner.slide.is_some() && input.jump {
            // Slide-jump: sale con la inercia y el salto del sprint.
            inner.end_slide();
            event.sneak = false;
            return;
        }
        if inner.wallrun.is_some() {
            event.forward = KEEP_SPRINT;
            event.strafe = 0.0;
            event.jump = false;
            event.sneak = false;
        } else if inner.vault.is_some() {
            event.forward = KEEP_SPRINT;
            event.strafe = 0.0;
            event.jump = false;
            event.sneak = inner.vault_sneak;
        } else if inner.slide.is_some() {
            event.forward = KEEP_SPRINT;
            event.strafe = 0.0;
            event.sneak = inner.slide_sneak;
        } else if inner.tracker.current == Move::Roll {
            event.forward = 1.0;
            event.jump = false;
            event.sneak = false;
        } else if inner.mantle.is_some() {
            event.forward = 0.0;
            event.strafe = 0.0;
            event.jump = false;
            event.sneak = inner.mantle_sneak;
        }
    }

    pub fn net_from_server(&self, event: &NetFromServerEvent<'_>) {
        if event.channel == net::ID
            && let Some(id) = net::decode_id(event.data)
            && let Ok(mut inner) = self.inner.lock()
        {
            inner.own_id = Some(id);
        }
    }

    pub fn own_id(&self) -> Option<u32> {
        self.inner.lock().ok()?.own_id
    }

    /// Cómo aterriza el jugador local, si se acaba de decidir.
    pub fn landing(&self) -> Option<Landing> {
        let inner = self.inner.lock().ok()?;
        inner.landing.filter(|(_, at)| at.elapsed() < LANDING_FRESH).map(|(l, _)| l)
    }

    /// La pose del efecto en curso más la del slide, si hay alguna. `partial` es la fracción de
    /// tick para suavizar el slide.
    fn pose(&self, partial: f32) -> Option<Pose> {
        let mut inner = self.inner.lock().ok()?;
        let effect = inner.effect.and_then(|(effect, at)| {
            effect.pose(at.elapsed().as_secs_f32(), inner.intensity)
        });
        if effect.is_none() {
            inner.effect = None;
        }
        let [before, now] = inner.sliding;
        let amount = before + (now - before) * partial.clamp(0.0, 1.0);
        let slide = (amount > 0.001).then(|| cinematic::slide_pose(amount, inner.intensity));
        let lerp = |[a, b]: [f32; 2]| a + (b - a) * partial.clamp(0.0, 1.0);
        let (side, up) = (lerp(inner.wall_side), lerp(inner.wall_up));
        let wall = (side.abs() > 0.001 || up > 0.001)
            .then(|| cinematic::wallrun_pose(side, up, lerp(inner.stride), inner.intensity));
        [effect, slide, wall].into_iter().flatten().reduce(Pose::and)
    }

    /// La voltereta y la bajada de cámara: giros grandes, que `camera_shake` no admite.
    pub fn camera_pose(&self, event: &mut CameraPoseEvent<'_>) {
        if let Some(pose) = self.pose(event.partial) {
            event.pitch += pose.spin;
            event.position[1] -= f64::from(pose.drop);
        }
    }

    /// Se suma a la sacudida de otros mods, nunca la reemplaza.
    pub fn camera_shake(&self, event: &mut CameraShakeEvent<'_>) {
        if let Some(pose) = self.pose(event.partial) {
            event.shake.pitch += pose.pitch;
            event.shake.roll += pose.roll;
        }
    }

    pub fn compute_fov_modifier(&self, event: &mut ComputeFovModifierEvent<'_>) {
        if let Some(pose) = self.pose(1.0) {
            event.modifier *= pose.fov;
        }
    }

    pub fn render_hand_pose(&self, event: &mut RenderHandPoseEvent<'_>) {
        if let Some(pose) = self.pose(event.partial) {
            event.y -= pose.hand_drop;
            event.pitch -= pose.hand_pitch;
        }
        let Ok(inner) = self.inner.lock() else { return };
        if inner.intensity > 0.0 {
            let [before, now] = inner.hands;
            let raised = before + (now - before) * event.partial.clamp(0.0, 1.0);
            event.y += HANDS_UP * raised;
            event.z -= HANDS_FORWARD * raised;
        }
    }

    pub fn hud(&self, host: &dyn Host, event: &mut HudEvent<'_>) {
        let shown = host.setting(crate::NAMESPACE, config::DEBUG_HUD.key);
        if config::DEBUG_HUD.value(shown.as_deref()) != "sí" {
            return;
        }
        let Ok(inner) = self.inner.lock() else { return };
        let (tracker, senses) = (&inner.tracker, &inner.senses);
        if tracker.pos.is_none() {
            return;
        }
        let input = tracker.input;
        let wall = |w: Option<Wall>| w.map_or("--".to_owned(), |w| format!("{:.2}", w.distance));
        let ledge = senses.ledge.map_or("--".to_owned(), |l| {
            format!(
                "+{:.2} a {:.2}{}",
                l.height,
                l.distance,
                if l.crouch_only { " (agachado)" } else { "" }
            )
        });
        let obstacle = senses.obstacle.map_or("--".to_owned(), |o| {
            format!(
                "+{:.2} a {:.2}, {} {:.2}{}",
                o.height,
                o.distance,
                if o.onto { "encima" } else { "pasa" },
                o.clear,
                if o.crouch { " (agachado)" } else { "" }
            )
        });
        let landing = inner.landing.map_or("--".to_owned(), |(l, at)| {
            format!("{} (hace {:.1} s)", l.name(), at.elapsed().as_secs_f32())
        });
        let lines = [
            format!("Parkour: {} ({} t)", tracker.current.name(), tracker.ticks_in_move),
            format!(
                "vel h {:.3}  v {:+.3}  caída {:.1}",
                tracker.speed_h(),
                tracker.vel[1],
                tracker.fall_distance()
            ),
            format!(
                "suelo {}  av {:+.1} lat {:+.1}{}{}",
                if tracker.on_ground() { "sí" } else { "no" },
                input.forward,
                input.strafe,
                if input.jump { "  SALTO" } else { "" },
                if input.sneak { "  AGACHADO" } else { "" },
            ),
            format!(
                "pared izq {}  der {}  frente {}",
                wall(senses.wall_left),
                wall(senses.wall_right),
                wall(senses.wall_front)
            ),
            format!("repisa {ledge}"),
            format!("obstáculo {obstacle}"),
            format!("techo {:.2}  vacío {:.1}", senses.headroom, senses.drop),
            format!(
                "aterrizaje {landing}  id {}",
                inner.own_id.map_or("?".to_owned(), |id| id.to_string())
            ),
        ];
        drop(inner);

        let width = lines.iter().map(|l| host.text_width(l)).fold(0.0, f32::max) + 2.0 * PAD;
        let height = LINE_H * lines.len() as f32 + 2.0 * PAD;
        let (x, y) = event.corners.place(Corner::TopLeft, width, height);
        host.fill(event.batch, [x, y, width, height], 0x7800_0000);
        for (i, line) in lines.iter().enumerate() {
            let at = [x + PAD, y + PAD + LINE_H * i as f32];
            host.text(event.batch, line, at, 0x00FF_FFFF, true);
        }
    }
}

impl Inner {
    /// Pregunta al servidor por el id propio hasta que conteste. Falla sin más en servidores sin
    /// el mod: allí no hay nada que hacer con el id.
    fn ask_own_id(&mut self, host: &mut dyn Host) {
        if self.own_id.is_some() {
            return;
        }
        if self.hello_wait == 0 {
            let _ = host.net_send_to_server(net::HELLO, &[]);
            self.hello_wait = HELLO_EVERY;
        } else {
            self.hello_wait -= 1;
        }
    }

    /// Empieza un vault o un slide si el jugador lo pide ahora (desde `movement_input`, con los
    /// sensores del último tick).
    fn try_start(&mut self, input: Input, sneak_pressed: bool) {
        let busy = self.mantle.is_some()
            || self.vault.is_some()
            || self.slide.is_some()
            || self.wallrun.is_some()
            || self.tracker.current == Move::Roll;
        let Some(feet) = self.tracker.pos.filter(|_| !busy) else { return };
        let t = &self.tracker;
        // Desde el suelo, o recién despegado de un salto (aún subiendo).
        let launch = t.on_ground() || (t.current == Move::Air && t.ticks_in_move <= 3 && t.vel[1] > 0.0);
        if launch
            && let Some(ob) = self.senses.obstacle
            && vault::can_vault(&ob, self.facing, t.speed_h(), input)
        {
            self.vault = Some(Vault::start(&ob, feet, self.facing, t.speed_h()));
            self.vault_sneak = ob.crouch;
            self.tracker.hold(Move::Vault);
            self.started = Some(Move::Vault);
            return;
        }
        if launch
            && self.walls_used < wallrun::MAX_WALLS
            && let Some(wall) = self.senses.wall_front
            && self.last_wall != Some(wall.normal)
            && wallrun::can_run_up(&wall, self.facing, self.ran_ago <= RAN_TICKS, input)
        {
            self.wallrun = Some(WallRun::up(&wall));
            self.walls_used += 1;
            self.last_wall = Some(wall.normal);
            self.wall_grace = None;
            self.tracker.hold(Move::WallRunUp);
            self.started = Some(Move::WallRunUp);
            return;
        }
        if self.slide_cool == 0 && slide::can_slide(t.on_ground(), t.speed_h(), input, sneak_pressed) {
            self.slide = Some(Slide::start(t.vel));
            self.slide_sneak = false;
            self.tracker.hold(Move::Slide);
            self.started = Some(Move::Slide);
        }
    }

    /// Sonido y efecto de cámara de lo que empezó en `movement_input`.
    fn announce(&mut self, host: &mut dyn Host, feet: [f64; 3]) {
        match self.started.take() {
            Some(Move::Vault) => {
                self.effect = Some((Effect::Vault, Instant::now()));
                host.play_local_sound("minecraft:item.armor.equip_leather", feet, 0.8, 1.2, 0);
            }
            Some(Move::WallRunUp) => {
                host.play_local_sound("minecraft:block.stone.step", feet, 1.0, 0.9, 0);
            }
            Some(Move::Slide) => {
                host.play_local_sound("minecraft:block.gravel.step", feet, 1.0, 0.7, 0);
                host.play_local_sound("minecraft:item.armor.equip_leather", feet, 0.6, 0.8, 0);
            }
            _ => {}
        }
    }

    /// Un tick del vault en curso. Devuelve si lo hay.
    fn vault(&mut self, host: &mut dyn Host, feet: [f64; 3]) -> bool {
        let Some(mut v) = self.vault else { return false };
        let step = v.tick(feet, self.tracker.vel, self.tracker.on_ground());
        if step.over_started {
            host.play_local_sound("minecraft:block.stone.step", feet, 0.9, 1.1, 0);
        }
        if step.end.is_some() {
            self.vault = None;
            self.vault_sneak = false;
            self.tracker.stop();
        } else {
            host.push_player(step.push.map(|v| v as f32));
            self.vault = Some(v);
            self.vault_sneak = step.sneak;
        }
        true
    }

    /// Un tick del slide en curso. `low_ceiling`: hay techo bajo encima o delante.
    fn slide(&mut self, host: &mut dyn Host, low_ceiling: bool) {
        self.slide_cool = self.slide_cool.saturating_sub(1);
        let Some(mut s) = self.slide else { return };
        let step = s.tick(self.tracker.vel, self.tracker.on_ground(), self.tracker.input);
        if step.end.is_some() {
            self.end_slide();
            return;
        }
        host.push_player(step.push.map(|v| v as f32));
        self.slide = Some(s);
        self.slide_sneak = low_ceiling;
        self.tracker.hold(Move::Slide);
    }

    fn end_slide(&mut self) {
        self.slide = None;
        self.slide_sneak = false;
        self.slide_cool = slide::COOLDOWN;
        self.tracker.stop();
    }

    /// El wall-run: empezar a correr por una pared lateral (sólo con estar en el aire junto a ella
    /// corriendo), un tick del que esté en curso, o el wall jump. `wall` es la pared del wall-run
    /// en curso vista este tick. Devuelve si el wall-run manda este tick.
    fn wallrun(&mut self, host: &mut dyn Host, feet: [f64; 3], wall: Option<sense::Wall>) -> bool {
        let jump = self.jump_queued;
        let input = self.tracker.input;
        let last = self.tracker.vel;
        let Some(mut run) = self.wallrun else {
            // Recién soltado: el salto aún vale.
            if let Some((run, left)) = self.wall_grace {
                if jump {
                    self.wall_jump(host, &run, feet);
                    return true;
                }
                self.wall_grace = (left > 1).then_some((run, left - 1));
            }
            let side = [self.senses.wall_left, self.senses.wall_right]
                .into_iter()
                .flatten()
                .filter(|w| self.last_wall != Some(w.normal))
                .min_by(|a, b| a.distance.total_cmp(&b.distance));
            let can = self.tracker.current == Move::Air
                && self.tracker.free_fall()
                && self.walls_used < wallrun::MAX_WALLS
                && side.is_some_and(|w| {
                    wallrun::can_run_side(&w, self.facing, last, input, self.senses.drop)
                });
            let Some(wall) = side.filter(|_| can) else { return false };
            self.wallrun = Some(WallRun::side(&wall, last));
            self.walls_used += 1;
            self.last_wall = Some(wall.normal);
            self.wall_grace = None;
            self.tracker.hold(Move::WallRunSide);
            host.play_local_sound("minecraft:block.stone.step", feet, 1.0, 1.0, 0);
            return true;
        };
        let step = run.tick(wall, last, self.tracker.on_ground(), self.facing, input, jump);
        match step.end {
            None => {
                // Subiendo, en cuanto el borde queda al alcance de las manos se agarra.
                if run.kind == Kind::Up
                    && !self.grab_locked
                    && let Some(ledge) = self.senses.ledge
                    && mantle::can_grab(&ledge, self.facing, last[1], input)
                {
                    self.wallrun = None;
                    self.tracker.stop();
                    self.grab(host, &ledge, feet);
                    return true;
                }
                host.push_player(step.push.map(|v| v as f32));
                self.wallrun = Some(run);
                self.tracker.hold(if run.kind == Kind::Side { Move::WallRunSide } else { Move::WallRunUp });
                if run.ticks % STRIDE_TICKS == 0 {
                    host.play_local_sound("minecraft:block.stone.step", feet, 0.5, 1.1, 0);
                }
            }
            Some(wallrun::End::Jump) => {
                self.wallrun = None;
                self.tracker.stop();
                self.wall_jump(host, &run, feet);
            }
            Some(end) => {
                self.wallrun = None;
                self.tracker.stop();
                self.wall_grace = end.allows_grace().then_some((run, wallrun::GRACE_TICKS));
            }
        }
        true
    }

    /// Impulsarse desde la pared de `run`.
    fn wall_jump(&mut self, host: &mut dyn Host, run: &WallRun, feet: [f64; 3]) {
        host.push_player(run.kick(self.tracker.vel).map(|v| v as f32));
        self.wall_grace = None;
        self.effect = Some((Effect::WallJump, Instant::now()));
        host.play_local_sound("minecraft:block.stone.step", feet, 1.0, 0.8, 0);
        host.play_local_sound("minecraft:item.armor.equip_leather", feet, 0.7, 1.3, 0);
    }

    /// Se agarra a `ledge` y empieza el mantle.
    fn grab(&mut self, host: &mut dyn Host, ledge: &sense::Ledge, feet: [f64; 3]) {
        self.mantle = Some(Mantle::grab(ledge));
        self.tracker.hold(Move::Mantle);
        self.effect = Some((Effect::Grab, Instant::now()));
        host.play_local_sound("minecraft:block.stone.step", feet, 1.0, 0.7, 0);
        self.hold_fall(host, true);
    }

    /// Agarrarse a una repisa y subir. Devuelve si el mantle manda este tick (entonces no hay
    /// aterrizaje que decidir).
    fn mantle(&mut self, host: &mut dyn Host, feet: [f64; 3], yaw: f32) -> bool {
        if self.tracker.on_ground() {
            self.grab_locked = false;
        }
        let Some(mut mantle) = self.mantle else {
            let grab = self.senses.ledge.filter(|ledge| {
                self.tracker.current == Move::Air
                    && self.tracker.free_fall()
                    && !self.grab_locked
                    && mantle::can_grab(
                        ledge,
                        sense::forward(yaw),
                        self.tracker.vel[1],
                        self.tracker.input,
                    )
            });
            let Some(ledge) = grab else { return false };
            self.grab(host, &ledge, feet);
            return true;
        };
        let step = mantle.tick(feet, self.tracker.vel, self.tracker.on_ground(), self.tracker.input);
        host.push_player(step.push.map(|v| v as f32));
        self.mantle_sneak = step.sneak;
        if step.over_started {
            self.effect = Some((Effect::MantleOver, Instant::now()));
            host.play_local_sound("minecraft:item.armor.equip_leather", feet, 0.8, 1.0, 0);
        }
        match step.end {
            None => {
                self.mantle = Some(mantle);
                self.tracker.hold(if step.hanging { Move::Hang } else { Move::Mantle });
                self.hold_fall(host, false);
            }
            Some(end) => {
                self.mantle = None;
                self.mantle_sneak = false;
                self.grab_locked = end != End::Done;
                self.tracker.stop();
                self.hold_fall(host, true);
            }
        }
        true
    }

    /// Mientras cuelga de una repisa, la caída anterior no cuenta: se lo recuerda a quien calcula
    /// el daño (aquí y en el servidor), ahora si `now` y si no cada poco.
    fn hold_fall(&mut self, host: &mut dyn Host, now: bool) {
        self.landing = Some((Landing::Grab, Instant::now()));
        let (last, age) = self.sent;
        if now || last != Landing::Grab || age >= LAND_EVERY {
            let _ = host.net_send_to_server(net::LAND, &[Landing::Grab.to_byte()]);
            self.sent = (Landing::Grab, 0);
        } else {
            self.sent.1 = age.saturating_add(1);
        }
    }

    fn running(&self) -> bool {
        self.tracker.speed_h() >= RUN_SPEED
    }

    /// En caída, decide cómo se aterrizaría ahora y se lo dice al servidor cuando cambia (y cada
    /// poco, para que no caduque).
    fn warn_landing(&mut self, host: &mut dyn Host) {
        if self.tracker.on_ground() {
            if self.tracker.landed.is_none() {
                self.sent = (Landing::Plain, 0);
            }
            return;
        }
        if self.tracker.fall_distance() < WARN_FALL {
            return;
        }
        let landing = Landing::classify(self.running(), self.tracker.since_sneak);
        self.landing = Some((landing, Instant::now()));
        let (last, age) = self.sent;
        if landing != last || (landing != Landing::Plain && age >= LAND_EVERY) {
            let _ = host.net_send_to_server(net::LAND, &[landing.to_byte()]);
            self.sent = (landing, 0);
        } else {
            self.sent.1 = age.saturating_add(1);
        }
    }

    /// Acaba de tocar suelo tras caer `fall` bloques.
    fn land(&mut self, host: &mut dyn Host, fall: f64, feet: [f64; 3], yaw: f32) {
        let landing = Landing::classify(self.running(), self.tracker.since_sneak);
        self.landing = Some((landing, Instant::now()));
        if landing != self.sent.0 {
            let _ = host.net_send_to_server(net::LAND, &[landing.to_byte()]);
        }
        let strength = cinematic::landing_strength(fall);
        let effect = match landing {
            Landing::Roll => {
                self.tracker.start(Move::Roll, cinematic::ROLL_TICKS);
                let dir = sense::forward(yaw);
                host.push_player([(dir[0] * ROLL_PUSH) as f32, 0.0, (dir[1] * ROLL_PUSH) as f32]);
                host.play_local_sound("minecraft:item.armor.equip_leather", feet, 1.0, 0.8, 0);
                host.play_local_sound("minecraft:block.wool.fall", feet, 1.0, 0.9, 5);
                Effect::Roll
            }
            Landing::Crouch => Effect::SoftLanding { strength },
            Landing::Run | Landing::Plain | Landing::Grab => Effect::HardLanding { strength },
        };
        self.effect = Some((effect, Instant::now()));
    }
}
