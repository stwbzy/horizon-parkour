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
    pub input: Input,
    pub current: Move,
    /// Ticks seguidos en el estado actual.
    pub ticks_in_move: u32,
    /// Sobre un bloque, según los sensores (`sense::on_ground`).
    ground: bool,
    /// Altura máxima desde que dejó el suelo, para la distancia de caída.
    apex: f64,
}

impl Tracker {
    pub fn on_ground(&self) -> bool {
        self.ground
    }

    pub fn speed_h(&self) -> f64 {
        self.vel[0].hypot(self.vel[2])
    }

    /// Bloques caídos desde el punto más alto del salto o caída en curso.
    pub fn fall_distance(&self) -> f64 {
        self.pos.map_or(0.0, |p| (self.apex - p[1]).max(0.0))
    }

    /// Un tick del cliente con la posición nueva del jugador y si pisa suelo. Un salto de más de 8 bloques es un
    /// teletransporte (o cambio de mundo): se reinicia en vez de contarlo como velocidad.
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
        self.pos = Some(pos);
        self.vel = vel;
        self.ground = ground;
        if self.on_ground() {
            self.apex = pos[1];
        } else {
            self.apex = self.apex.max(pos[1]);
        }
        let next = self.classify();
        if next == self.current {
            self.ticks_in_move = self.ticks_in_move.saturating_add(1);
        } else {
            self.current = next;
            self.ticks_in_move = 0;
        }
    }

    fn classify(&self) -> Move {
        if !self.on_ground() {
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
    fn a_teleport_is_not_speed() {
        let mut t = Tracker::default();
        t.tick([0.0, 64.0, 0.0], true);
        t.tick([500.0, 64.0, 0.0], true);
        assert_eq!(t.vel, [0.0; 3]);
    }
}
