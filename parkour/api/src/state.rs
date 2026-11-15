//! Máquina de estados del parkour del jugador local. Lógica pura (sin `Host`), para poder
//! probarla sin el juego. Por ahora sólo clasifica suelo/aire/carrera; los movimientos (fases 2
//! a 5) añadirán sus transiciones aquí.

/// Qué está haciendo el jugador. Los movimientos aún sin implementar ya tienen nombre para que el
/// HUD de depuración y la red no cambien de formato entre fases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Move {
    #[default]
    Idle,
    Run,
    Air,
    Roll,
    Mantle,
    Hang,
    Vault,
    Slide,
    WallRunSide,
    WallRunUp,
}

impl Move {
    pub fn name(self) -> &'static str {
        match self {
            Move::Idle => "quieto",
            Move::Run => "carrera",
            Move::Air => "aire",
            Move::Roll => "roll",
            Move::Mantle => "mantle",
            Move::Hang => "colgado",
            Move::Vault => "vault",
            Move::Slide => "slide",
            Move::WallRunSide => "wall-run lateral",
            Move::WallRunUp => "wall-run vertical",
        }
    }
}

/// Velocidad horizontal (bloques/tick) desde la que cuenta como carrera: andar es ~0.216,
/// esprintar ~0.28.
pub const RUN_SPEED: f64 = 0.25;
/// Ticks en los que se recuerda la velocidad horizontal más alta (`recent_speed`).
const SPEED_MEMORY: usize = 6;
/// Lo que el jugador pide este tick (`movement_input`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub forward: f32,
    pub strafe: f32,
    pub jump: bool,
    pub sneak: bool,
}

/// Seguimiento del jugador local, tick a tick.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    pub pos: Option<[f64; 3]>,
    /// Bloques por tick, de la diferencia de posiciones.
    pub vel: [f64; 3],
    /// La velocidad vertical del tick anterior.
    prev_vy: f64,
    pub input: Input,
    pub current: Move,
    /// Ticks seguidos en el estado actual.
    pub ticks_in_move: u32,
    /// Sobre un bloque, según los sensores (`sense::on_ground`).
    ground: bool,
    /// Altura máxima desde que dejó el suelo, para la distancia de caída.
    apex: f64,
    /// Bloques caídos, sólo en el tick en que toca suelo.
    pub landed: Option<f64>,
    /// Ticks desde que pulsó agacharse en el aire (no cuenta llevarlo pulsado desde el suelo).
    pub since_sneak: Option<u32>,
    sneak_was_down: bool,
    /// Velocidad horizontal de los últimos ticks (la más vieja se pisa).
    speeds: [f64; SPEED_MEMORY],
    speed_at: usize,
    /// Un movimiento en curso y los ticks que le quedan: manda sobre suelo/aire/carrera.
    busy: Option<(Move, u32)>,
}

impl Tracker {
    pub fn on_ground(&self) -> bool {
        self.ground
    }

    pub fn speed_h(&self) -> f64 {
        self.vel[0].hypot(self.vel[2])
    }

    /// La velocidad horizontal más alta de los últimos ticks: un golpe contra una pared, soltar
    /// la tecla un instante o caer en diagonal no borran que venía corriendo.
    pub fn recent_speed(&self) -> f64 {
        self.speeds.iter().copied().fold(self.speed_h(), f64::max)
    }

    /// Pulsó agacharse en el aire justo ahora (desde `movement_input`, antes de que el juego mueva
    /// al jugador): cuenta ya, sin esperar al tick, por si este mismo tick toca suelo.
    pub fn sneak_now(&mut self) {
        if !self.ground {
            self.since_sneak = Some(0);
        }
    }

    /// En el aire y bajo la gravedad normal: la velocidad vertical baja lo que toca cada tick.
    /// Volando en creativo, planeando con élitros o dentro del agua no se cumple, y ahí los
    /// movimientos de parkour no deben saltar solos.
    pub fn free_fall(&self) -> bool {
        !self.ground && (-0.12..=-0.03).contains(&(self.vel[1] - self.prev_vy))
    }

    /// Bloques caídos desde el punto más alto del salto o caída en curso.
    pub fn fall_distance(&self) -> f64 {
        self.pos.map_or(0.0, |p| (self.apex - p[1]).max(0.0))
    }

    /// Un tick del cliente con la posición nueva del jugador y si pisa suelo. Un salto de más de
    /// 8 bloques es un teletransporte (o cambio de mundo): se reinicia en vez de contarlo como
    /// velocidad.
    pub fn tick(&mut self, pos: [f64; 3], ground: bool) {
        let vel = match self.pos {
            Some(prev) => [pos[0] - prev[0], pos[1] - prev[1], pos[2] - prev[2]],
            None => [0.0; 3],
        };
        if vel.iter().any(|v| v.abs() > 8.0) {
            *self = Tracker {
                pos: Some(pos),
                apex: pos[1],
                input: self.input,
                ground,
                ..Tracker::default()
            };
            return;
        }
        let was_airborne = self.pos.is_some() && !self.ground;
        let pressed = self.input.sneak && !self.sneak_was_down;
        self.prev_vy = self.vel[1];
        self.pos = Some(pos);
        self.vel = vel;
        self.speeds[self.speed_at] = vel[0].hypot(vel[2]);
        self.speed_at = (self.speed_at + 1) % SPEED_MEMORY;
        self.ground = ground;
        self.landed = (ground && was_airborne).then(|| (self.apex - pos[1]).max(0.0));
        if ground {
            self.apex = pos[1];
            // En el tick del aterrizaje aún hace falta saber si se agachó a tiempo, y la tecla de
            // este tick se pulsó aún en el aire (el juego lee las teclas antes de moverlo).
            if self.landed.is_none() {
                self.since_sneak = None;
            } else if pressed {
                self.since_sneak = Some(0);
            }
        } else {
            self.apex = self.apex.max(pos[1]);
            self.since_sneak = if pressed {
                Some(0)
            } else {
                self.since_sneak.map(|t| t.saturating_add(1))
            };
        }
        self.sneak_was_down = self.input.sneak;
        self.busy = match self.busy {
            Some((mv, u32::MAX)) => Some((mv, u32::MAX)),
            Some((mv, left)) if left > 1 => Some((mv, left - 1)),
            _ => None,
        };
        let next = self.classify();
        if next == self.current {
            self.ticks_in_move = self.ticks_in_move.saturating_add(1);
        } else {
            self.current = next;
            self.ticks_in_move = 0;
        }
    }

    /// Empieza un movimiento que dura `ticks` ticks.
    pub fn start(&mut self, mv: Move, ticks: u32) {
        self.busy = Some((mv, ticks));
        self.current = mv;
        self.ticks_in_move = 0;
    }

    /// Empieza (o cambia a) un movimiento sin duración fija: dura hasta `stop`.
    pub fn hold(&mut self, mv: Move) {
        if self.current != mv {
            self.ticks_in_move = 0;
        }
        self.busy = Some((mv, u32::MAX));
        self.current = mv;
    }

    /// Acaba el movimiento en curso.
    pub fn stop(&mut self) {
        self.busy = None;
    }

    fn classify(&self) -> Move {
        if let Some((mv, _)) = self.busy {
            mv
        } else if !self.on_ground() {
            Move::Air
        } else if self.speed_h() >= RUN_SPEED {
            Move::Run
        } else {
            Move::Idle
        }
    }

    pub fn reset(&mut self) {
        *self = Tracker::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_still_is_idle() {
        let mut t = Tracker::default();
        for _ in 0..5 {
            t.tick([0.0, 64.0, 0.0], true);
        }
        assert_eq!(t.current, Move::Idle);
        assert!(t.on_ground());
    }

    #[test]
    fn sprinting_on_flat_ground_is_run() {
        let mut t = Tracker::default();
        for i in 0..6 {
            t.tick([f64::from(i) * 0.28, 64.0, 0.0], true);
        }
        assert_eq!(t.current, Move::Run);
    }

    #[test]
    fn falling_is_air_and_measures_the_drop() {
        let mut t = Tracker::default();
        for _ in 0..3 {
            t.tick([0.0, 70.0, 0.0], true);
        }
        for i in 1..=4 {
            t.tick([0.0, 70.0 - f64::from(i), 0.0], false);
        }
        assert_eq!(t.current, Move::Air);
        assert!((t.fall_distance() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn landing_reports_the_fall_once() {
        let mut t = Tracker::default();
        t.tick([0.0, 70.0, 0.0], true);
        t.tick([0.0, 68.0, 0.0], false);
        assert_eq!(t.landed, None);
        t.tick([0.0, 65.0, 0.0], true);
        assert_eq!(t.landed, Some(5.0));
        t.tick([0.0, 65.0, 0.0], true);
        assert_eq!(t.landed, None);
    }

    #[test]
    fn only_a_sneak_pressed_in_the_air_counts() {
        let mut t = Tracker::default();
        // Agachado desde el suelo: no cuenta al caer.
        t.input.sneak = true;
        t.tick([0.0, 70.0, 0.0], true);
        t.tick([0.0, 69.0, 0.0], false);
        assert_eq!(t.since_sneak, None);
        // Soltar y volver a pulsar en el aire: sí.
        t.input.sneak = false;
        t.tick([0.0, 68.0, 0.0], false);
        t.input.sneak = true;
        t.tick([0.0, 67.0, 0.0], false);
        assert_eq!(t.since_sneak, Some(0));
        t.tick([0.0, 66.0, 0.0], false);
        assert_eq!(t.since_sneak, Some(1));
        // Sigue disponible en el tick del aterrizaje y se borra después.
        t.tick([0.0, 65.0, 0.0], true);
        assert_eq!(t.since_sneak, Some(1));
        t.tick([0.0, 65.0, 0.0], true);
        assert_eq!(t.since_sneak, None);
    }

    #[test]
    fn a_sneak_pressed_in_the_landing_tick_still_counts() {
        let mut t = Tracker::default();
        t.tick([0.0, 70.0, 0.0], true);
        t.tick([0.0, 68.0, 0.0], false);
        // Pulsa en el último instante: el juego lo lee antes de moverlo y toca suelo ese tick.
        t.input.sneak = true;
        t.sneak_now();
        assert_eq!(t.since_sneak, Some(0));
        t.tick([0.0, 65.0, 0.0], true);
        assert_eq!(t.landed, Some(5.0));
        assert_eq!(t.since_sneak, Some(0));
        // Agacharse en el suelo, después, no se cuenta como del aire.
        t.sneak_now();
        t.tick([0.0, 65.0, 0.0], true);
        assert_eq!(t.since_sneak, None);
    }

    #[test]
    fn the_recent_speed_survives_a_slow_last_tick() {
        let mut t = Tracker::default();
        for i in 0..4 {
            t.tick([f64::from(i) * 0.3, 70.0, 0.0], false);
        }
        t.tick([0.95, 69.0, 0.0], true);
        assert!(t.speed_h() < 0.1);
        assert!((t.recent_speed() - 0.3).abs() < 1e-9);
        for _ in 0..SPEED_MEMORY {
            t.tick([0.95, 69.0, 0.0], true);
        }
        assert!(t.recent_speed() < 1e-9);
    }

    #[test]
    fn a_started_move_holds_for_its_ticks() {
        let mut t = Tracker::default();
        t.tick([0.0, 64.0, 0.0], true);
        t.start(Move::Roll, 3);
        t.tick([0.0, 64.0, 0.0], true);
        t.tick([0.0, 64.0, 0.0], true);
        assert_eq!(t.current, Move::Roll);
        t.tick([0.0, 64.0, 0.0], true);
        assert_eq!(t.current, Move::Idle);
    }

    #[test]
    fn a_held_move_lasts_until_stopped() {
        let mut t = Tracker::default();
        t.hold(Move::Hang);
        for _ in 0..50 {
            t.tick([0.0, 64.0, 0.0], false);
        }
        assert_eq!(t.current, Move::Hang);
        t.stop();
        t.tick([0.0, 64.0, 0.0], false);
        assert_eq!(t.current, Move::Air);
    }

    #[test]
    fn free_fall_tells_gravity_from_flying() {
        // Un salto: 0.42 hacia arriba y la gravedad de vanilla.
        let mut t = Tracker::default();
        t.tick([0.0, 64.0, 0.0], true);
        let (mut y, mut v) = (64.0, 0.42);
        for i in 0..6 {
            y += v;
            v = (v - 0.08) * 0.98;
            t.tick([0.0, y, 0.0], false);
            assert_eq!(t.free_fall(), i > 0, "tick {i}");
        }
        // Volando a altura fija, o subiendo a velocidad constante: no.
        let mut t = Tracker::default();
        for i in 0..6 {
            t.tick([0.0, 80.0 + 0.3 * f64::from(i), 0.0], false);
        }
        assert!(!t.free_fall());
    }

    #[test]
    fn a_teleport_is_not_speed() {
        let mut t = Tracker::default();
        t.tick([0.0, 64.0, 0.0], true);
        t.tick([500.0, 64.0, 0.0], true);
        assert_eq!(t.vel, [0.0; 3]);
    }
}
