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
use crate::net;
use crate::sense::{self, Aabb, Senses, Wall};
use crate::state::{Input, Move, RUN_SPEED, Tracker};

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
        let ground = inner.senses.ground;
        let fall_speed = inner.tracker.vel[1];
        inner.tracker.tick(feet, ground);

        let stored = host.setting(crate::NAMESPACE, config::CINEMATIC.key);
        inner.intensity =
            config::CINEMATIC.value(stored.as_deref()).parse::<f32>().unwrap_or(100.0) / 100.0;

        inner.ask_own_id(host);
        inner.warn_landing(host);
        if let Some(fall) = inner.tracker.landed
            && fall >= land::MIN_FALL
            && fall_speed <= IMPACT_SPEED
        {
            inner.land(host, fall, feet, player.yaw);
        }
    }

    /// Mientras rueda, el jugador sigue hacia delante: no frena, no salta ni se queda agachado.
    pub fn movement_input(&self, event: &mut MovementInputEvent) {
        let Ok(mut inner) = self.inner.lock() else { return };
        inner.tracker.input = Input {
            forward: event.forward,
            strafe: event.strafe,
            jump: event.jump,
            sneak: event.sneak,
        };
        if inner.tracker.current == Move::Roll {
            event.forward = 1.0;
            event.jump = false;
            event.sneak = false;
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

    /// La pose del efecto en curso, si hay alguno.
    fn pose(&self) -> Option<Pose> {
        let mut inner = self.inner.lock().ok()?;
        let (effect, at) = inner.effect?;
        let pose = effect.pose(at.elapsed().as_secs_f32(), inner.intensity);
        if pose.is_none() {
            inner.effect = None;
        }
        pose
    }

    /// La voltereta y la bajada de cámara: giros grandes, que `camera_shake` no admite.
    pub fn camera_pose(&self, event: &mut CameraPoseEvent<'_>) {
        if let Some(pose) = self.pose() {
            event.pitch += pose.spin;
            event.position[1] -= f64::from(pose.drop);
        }
    }

    /// Se suma a la sacudida de otros mods, nunca la reemplaza.
    pub fn camera_shake(&self, event: &mut CameraShakeEvent<'_>) {
        if let Some(pose) = self.pose() {
            event.shake.pitch += pose.pitch;
            event.shake.roll += pose.roll;
        }
    }

    pub fn compute_fov_modifier(&self, event: &mut ComputeFovModifierEvent<'_>) {
        if let Some(pose) = self.pose() {
            event.modifier *= pose.fov;
        }
    }

    pub fn render_hand_pose(&self, event: &mut RenderHandPoseEvent<'_>) {
        if let Some(pose) = self.pose() {
            event.y -= pose.hand_drop;
            event.pitch -= pose.hand_pitch;
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
            Landing::Run | Landing::Plain => Effect::HardLanding { strength },
        };
        self.effect = Some((effect, Instant::now()));
    }
}
