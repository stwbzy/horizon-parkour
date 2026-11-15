//! Slide: corriendo, pulsar agacharse desliza por el suelo unos 5 bloques. Lógica pura.
//!
//! Sigue la dirección en la que se corría (se puede mirar a otro lado mientras desliza) con una
//! velocidad que empieza por encima de la carrera y frena poco a poco. Saltar a media
//! deslizada sale con la inercia (slide-jump); soltar agacharse, chocar o quedarse sin suelo
//! la acaba.

use crate::state::Input;

/// Velocidad horizontal mínima para empezar a deslizar (esprintar es ~0.28).
pub const SLIDE_SPEED: f64 = 0.26;
/// Ticks tras un slide antes de poder empezar otro.
pub const COOLDOWN: u32 = 10;

/// La velocidad de salida respecto a la de la carrera, y su máximo.
const BOOST: f64 = 1.45;
const MAX_SPEED: f64 = 0.5;
/// Cuánto conserva la velocidad cada tick, y por debajo de cuánto se para.
const DECAY: f64 = 0.955;
const END_SPEED: f64 = 0.17;
const MAX_TICKS: u32 = 26;
/// Ticks mínimos antes de que soltar agacharse lo acabe (un toque también desliza).
const MIN_TICKS: u32 = 8;
/// Ticks seguidos sin suelo (un escalón hacia abajo) antes de darlo por caído.
const AIR_TICKS: u32 = 3;

/// Rozamiento horizontal en suelo normal por tick (0.6 × 0.91).
const DRAG_GROUND: f64 = 0.546;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// Se acabó la inercia.
    Stopped,
    /// Soltó agacharse.
    Released,
    /// Saltó: sale con la inercia.
    Jump,
    /// Se quedó sin suelo.
    Airborne,
    /// Chocó con algo.
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub push: [f64; 3],
    pub end: Option<End>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slide {
    pub dir: [f64; 2],
    speed: f64,
    ticks: u32,
    air: u32,
}

/// Empieza un slide: en suelo, corriendo hacia delante y recién pulsado agacharse.
pub fn can_slide(ground: bool, speed_h: f64, input: Input, sneak_pressed: bool) -> bool {
    ground && speed_h >= SLIDE_SPEED && sneak_pressed && input.forward > 0.1 && !input.jump
}

impl Slide {
    /// `vel` es el desplazamiento del último tick: marca la dirección y la velocidad de salida.
    pub fn start(vel: [f64; 3]) -> Slide {
        let speed_h = vel[0].hypot(vel[2]).max(1e-9);
        Slide {
            dir: [vel[0] / speed_h, vel[2] / speed_h],
            speed: (speed_h * BOOST).min(MAX_SPEED),
            ticks: 0,
            air: 0,
        }
    }

    pub fn ticks(&self) -> u32 {
        self.ticks
    }

    /// Un tick: `last` es el desplazamiento del tick anterior, `ground` si pisa suelo e `input`
    /// lo que pide el jugador (sin tocar).
    pub fn tick(&mut self, last: [f64; 3], ground: bool, input: Input) -> Step {
        self.ticks += 1;
        let mut step = Step { push: [0.0; 3], end: None };
        let end = if input.jump {
            Some(End::Jump)
        } else if !input.sneak && self.ticks >= MIN_TICKS {
            Some(End::Released)
        } else if self.ticks > 2 && (last[0] * self.dir[0] + last[2] * self.dir[1]) < 0.05 {
            Some(End::Blocked)
        } else {
            None
        };
        if end.is_some() {
            step.end = end;
            return step;
        }
        self.speed *= DECAY;
        if self.speed < END_SPEED || self.ticks > MAX_TICKS {
            step.end = Some(End::Stopped);
            return step;
        }
        if !ground {
            // Un escalón hacia abajo: se deja caer sin empujar, y si dura es que ya no hay suelo.
            self.air += 1;
            if self.air >= AIR_TICKS {
                step.end = Some(End::Airborne);
            }
            return step;
        }
        self.air = 0;
        let desired = [self.dir[0] * self.speed, self.dir[1] * self.speed];
        step.push = [desired[0] - last[0] * DRAG_GROUND, 0.0, desired[1] - last[2] * DRAG_GROUND];
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLIDING: Input = Input { forward: 1.0, strafe: 0.0, jump: false, sneak: true };

    /// En suelo llano, sin teclas (el cliente las anula mientras desliza).
    fn run(slide: &mut Slide, input: impl Fn(u32) -> Input) -> (f64, Option<End>, u32) {
        let mut vel = [0.0, 0.0, 0.28 * DRAG_GROUND];
        let mut last = [0.0, 0.0, 0.28];
        let mut z = 0.0;
        for tick in 0..60 {
            let step = slide.tick(last, true, input(tick));
            if let Some(end) = step.end {
                return (z, Some(end), tick);
            }
            vel[0] += step.push[0];
            vel[2] += step.push[2];
            last = [vel[0], 0.0, vel[2]];
            z += vel[2];
            vel = [vel[0] * DRAG_GROUND, 0.0, vel[2] * DRAG_GROUND];
        }
        (z, None, 60)
    }

    #[test]
    fn sliding_needs_a_fresh_sneak_while_running() {
        assert!(can_slide(true, 0.28, SLIDING, true));
        assert!(!can_slide(true, 0.28, SLIDING, false));
        assert!(!can_slide(false, 0.28, SLIDING, true));
        assert!(!can_slide(true, 0.2, SLIDING, true));
        assert!(!can_slide(true, 0.28, Input { forward: 0.0, ..SLIDING }, true));
    }

    #[test]
    fn a_held_slide_covers_about_five_blocks() {
        let mut slide = Slide::start([0.0, 0.0, 0.28]);
        assert_eq!(slide.dir, [0.0, 1.0]);
        let (z, end, ticks) = run(&mut slide, |_| SLIDING);
        assert_eq!(end, Some(End::Stopped));
        assert!((4.0..7.0).contains(&z), "desliza {z} bloques en {ticks} ticks");
    }

    #[test]
    fn a_tap_still_slides_a_bit_and_releasing_ends_it() {
        let mut slide = Slide::start([0.0, 0.0, 0.28]);
        let (z, end, ticks) = run(&mut slide, |t| Input { sneak: t == 0, ..SLIDING });
        assert_eq!(end, Some(End::Released));
        assert_eq!(ticks, MIN_TICKS - 1);
        assert!(z > 2.0, "{z}");
    }

    #[test]
    fn jumping_ends_the_slide() {
        let mut slide = Slide::start([0.0, 0.0, 0.28]);
        let (_, end, _) = run(&mut slide, |t| Input { jump: t == 4, ..SLIDING });
        assert_eq!(end, Some(End::Jump));
    }

    #[test]
    fn a_wall_stops_it() {
        let mut slide = Slide::start([0.0, 0.0, 0.28]);
        let end = (0..10).find_map(|_| slide.tick([0.0; 3], true, SLIDING).end);
        assert_eq!(end, Some(End::Blocked));
    }

    #[test]
    fn a_step_down_does_not_end_it_but_a_drop_does() {
        let mut slide = Slide::start([0.0, 0.0, 0.28]);
        let moving = [0.0, -0.1, 0.3];
        assert_eq!(slide.tick(moving, false, SLIDING).end, None);
        assert_eq!(slide.tick(moving, true, SLIDING).end, None);
        let end = (0..5).find_map(|_| slide.tick(moving, false, SLIDING).end);
        assert_eq!(end, Some(End::Airborne));
    }
}
