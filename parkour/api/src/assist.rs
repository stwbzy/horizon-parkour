//! Margen al saltar: lo que hace que el parkour perdone un poco el tiempo de las teclas. Lógica
//! pura.
//!
//! - **Coyote time**: tras salir de un borde andando o corriendo (sin saltar), aún se puede saltar
//!   unos ticks en el aire, como si se pisara el borde.
//! - **Salto guardado**: un salto pulsado un poco antes de tiempo (antes de llegar al obstáculo o a
//!   la pared) sigue valiendo unos ticks para el vault, la subida por la pared o el tic-tac.

/// Ticks en el aire tras salir de un borde en los que aún vale saltar (0.15 s).
pub const COYOTE_TICKS: u32 = 3;
/// Ticks que se guarda un salto pulsado (0.25 s).
pub const BUFFER_TICKS: u32 = 5;
/// Velocidad vertical de un salto de vanilla, y el empujón hacia delante del salto esprintando.
pub const JUMP_SPEED: f64 = 0.42;
pub const SPRINT_JUMP_BOOST: f64 = 0.2;

const GRAVITY: f64 = 0.08;
const DRAG_Y: f64 = 0.98;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Assist {
    /// Ticks seguidos en el aire.
    air: u32,
    /// Dejó el suelo saltando (o ya usó el coyote): no hay coyote.
    jumped: bool,
    /// Ticks desde el último salto pulsado, mientras se guarda; y si se pulsó ya en el aire.
    jump_ago: Option<u32>,
    in_air: bool,
}

impl Assist {
    /// Un tick del cliente: si pisa suelo, la velocidad vertical del último tick y si se pulsó
    /// saltar desde el tick anterior.
    pub fn tick(&mut self, ground: bool, vy: f64, pressed: bool) {
        if ground {
            self.air = 0;
            self.jumped = false;
        } else {
            self.air = self.air.saturating_add(1);
            // El primer tick en el aire ya sube si fue un salto; cayendo de un borde, baja.
            if self.air == 1 && vy > 0.05 {
                self.jumped = true;
            }
        }
        if pressed {
            self.in_air = !ground;
        }
        self.jump_ago = if pressed {
            Some(0)
        } else {
            self.jump_ago.map(|t| t + 1).filter(|t| *t < BUFFER_TICKS)
        };
    }

    /// Acaba de pulsar saltar justo después de salir de un borde sin saltar.
    pub fn coyote(&self, pressed: bool) -> bool {
        pressed && !self.jumped && (1..=COYOTE_TICKS).contains(&self.air)
    }

    /// Hay un salto guardado.
    pub fn buffered(&self) -> bool {
        self.jump_ago.is_some()
    }

    /// Hay un salto guardado pulsado ya en el aire (no el que despegó del suelo).
    pub fn buffered_in_air(&self) -> bool {
        self.jump_ago.is_some() && self.in_air
    }

    /// Ticks en el aire.
    pub fn air(&self) -> u32 {
        self.air
    }

    /// Dejó el suelo saltando.
    pub fn jumped(&self) -> bool {
        self.jumped
    }

    /// El salto guardado (o el del coyote) ya se usó.
    pub fn consume(&mut self) {
        self.jump_ago = None;
        self.jumped = true;
    }
}

/// El empujón del salto del coyote, con `last` el desplazamiento del último tick: el siguiente
/// sube como un salto de vanilla y, esprintando, lleva su empujón hacia `facing`.
pub fn coyote_push(last: [f64; 3], facing: [f64; 2], sprinting: bool) -> [f64; 3] {
    let coast_y = (last[1] - GRAVITY) * DRAG_Y;
    let boost = if sprinting { SPRINT_JUMP_BOOST } else { 0.0 };
    [facing[0] * boost, JUMP_SPEED - coast_y, facing[1] * boost]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_off_an_edge_leaves_a_moment_to_jump() {
        let mut a = Assist::default();
        a.tick(true, 0.0, false);
        a.tick(false, -0.08, false);
        assert!(a.coyote(true));
        a.tick(false, -0.16, false);
        a.tick(false, -0.23, false);
        assert!(a.coyote(true));
        a.tick(false, -0.3, false);
        assert!(!a.coyote(true), "demasiado tarde");
        assert!(!a.coyote(false));
    }

    #[test]
    fn a_real_jump_or_a_used_coyote_gives_no_second_jump() {
        let mut a = Assist::default();
        a.tick(true, 0.0, false);
        a.tick(false, 0.42, true);
        assert!(!a.coyote(true));
        let mut a = Assist::default();
        a.tick(true, 0.0, false);
        a.tick(false, -0.08, false);
        a.consume();
        assert!(!a.coyote(true));
    }

    #[test]
    fn an_early_jump_is_kept_for_a_few_ticks() {
        let mut a = Assist::default();
        a.tick(true, 0.0, true);
        assert!(a.buffered());
        for _ in 1..BUFFER_TICKS {
            a.tick(false, 0.3, false);
        }
        assert!(a.buffered());
        a.tick(false, 0.2, false);
        assert!(!a.buffered());
        a.tick(false, 0.2, true);
        a.consume();
        assert!(!a.buffered());
    }

    #[test]
    fn only_a_jump_pressed_in_the_air_counts_as_an_air_jump() {
        let mut a = Assist::default();
        a.tick(true, 0.0, true);
        a.tick(false, 0.42, false);
        assert!(a.buffered() && !a.buffered_in_air());
        a.tick(false, 0.3, true);
        assert!(a.buffered_in_air());
    }

    #[test]
    fn the_coyote_jump_rises_like_a_real_one() {
        let last = [0.0, -0.08, 0.28];
        let push = coyote_push(last, [0.0, 1.0], true);
        let next_y = (last[1] - GRAVITY) * DRAG_Y + push[1];
        assert!((next_y - JUMP_SPEED).abs() < 1e-9);
        assert!((push[2] - SPRINT_JUMP_BOOST).abs() < 1e-9);
    }
}
