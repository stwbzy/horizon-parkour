//! Vault: saltar un obstáculo bajo apoyando las manos, sin perder la carrera. Lógica pura.
//!
//! Igual que el mantle, es un controlador de velocidad: cada tick empuja la diferencia entre el
//! desplazamiento que quiere y el que daría la física sola. Sube el cuerpo hasta el borde a la vez
//! que llega a él, pasa por encima a la velocidad de la carrera y suelta: al otro lado cae con la
//! inercia; si el obstáculo es hondo, acaba de pie encima.

use crate::sense::Obstacle;
use crate::state::{Input, RUN_SPEED};

/// Cuánto hay que mirar hacia el obstáculo: coseno del ángulo máximo (unos 45°).
pub const VAULT_FACING: f64 = 0.7;

/// Cuánto pasan los pies por encima del borde.
const CLEARANCE: f64 = 0.08;
/// Lo más rápido que sube el cuerpo, en bloques por tick (un salto de vanilla empieza a 0.42).
const RISE_SPEED: f64 = 0.42;
/// Velocidad horizontal al pasar: la de la carrera, entre estos límites.
const MIN_SPEED: f64 = 0.28;
const MAX_SPEED: f64 = 0.4;
const MAX_TICKS: u32 = 24;
/// Ticks seguidos sin avanzar (algo en medio) antes de rendirse.
const STALL_TICKS: u32 = 4;

/// Gravedad y rozamientos por tick de vanilla: en el aire, y en suelo normal (0.6 × 0.91).
const GRAVITY: f64 = 0.08;
const DRAG_Y: f64 = 0.98;
const DRAG_AIR: f64 = 0.91;
const DRAG_GROUND: f64 = 0.546;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Sube hasta el borde mientras llega a él.
    Up,
    /// Pasa por encima.
    Over,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// Pasado (o encima, si era hondo).
    Done,
    /// Algo impide pasar.
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub push: [f64; 3],
    /// Hay que ir agachado (sólo cabe así por encima).
    pub sneak: bool,
    /// Este tick las manos se apoyan y empieza a pasar.
    pub over_started: bool,
    pub end: Option<End>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vault {
    dir: [f64; 2],
    start: [f64; 2],
    top: f64,
    /// Distancia hasta la cara del obstáculo y avance con el que queda pasado.
    wall: f64,
    clear: f64,
    pub onto: bool,
    crouch: bool,
    speed: f64,
    pub phase: Phase,
    ticks: u32,
    stalled: u32,
}

/// Se puede saltar este obstáculo ahora: corriendo hacia él y pidiendo saltar.
pub fn can_vault(ob: &Obstacle, facing: [f64; 2], speed_h: f64, input: Input) -> bool {
    let toward = -(facing[0] * ob.normal[0] + facing[1] * ob.normal[1]);
    toward >= VAULT_FACING
        && speed_h >= RUN_SPEED
        && input.jump
        && input.forward > 0.1
        && !input.sneak
}

impl Vault {
    /// Empieza con los pies en `feet`, mirando en `dir` (la dirección con la que se midió el
    /// obstáculo) y a `speed_h` bloques por tick.
    pub fn start(ob: &Obstacle, feet: [f64; 3], dir: [f64; 2], speed_h: f64) -> Vault {
        Vault {
            dir,
            start: [feet[0], feet[2]],
            top: ob.top,
            wall: ob.distance,
            clear: ob.clear,
            onto: ob.onto,
            crouch: ob.crouch,
            speed: speed_h.clamp(MIN_SPEED, MAX_SPEED),
            phase: Phase::Up,
            ticks: 0,
            stalled: 0,
        }
    }

    /// Un tick: `feet` es la posición, `last` el desplazamiento del tick anterior y `ground` si
    /// pisa suelo.
    pub fn tick(&mut self, feet: [f64; 3], last: [f64; 3], ground: bool) -> Step {
        self.ticks += 1;
        let mut step = Step { push: [0.0; 3], sneak: self.crouch, over_started: false, end: None };
        if self.ticks > MAX_TICKS {
            step.end = Some(End::Blocked);
            return step;
        }
        let progress = (feet[0] - self.start[0]) * self.dir[0] + (feet[2] - self.start[1]) * self.dir[1];
        let remaining = self.top + CLEARANCE - feet[1];
        if self.phase == Phase::Up && remaining <= 0.0 {
            self.phase = Phase::Over;
            self.stalled = 0;
            step.over_started = true;
        }
        let (h, v) = match self.phase {
            Phase::Up => {
                // Llega a la cara del obstáculo justo cuando los pies pasan el borde.
                let rise = remaining.min(RISE_SPEED);
                let ticks_left = (remaining / RISE_SPEED).ceil().max(1.0);
                let to_wall = (self.wall - progress).max(0.0);
                if self.ticks > 1 && last[1] < 0.05 {
                    self.stalled += 1;
                } else {
                    self.stalled = 0;
                }
                (self.speed.min(to_wall / ticks_left), rise)
            }
            Phase::Over => {
                if progress >= self.clear {
                    step.end = Some(End::Done);
                    return step;
                }
                let moved = last[0] * self.dir[0] + last[2] * self.dir[1];
                if !step.over_started && moved < 0.03 {
                    self.stalled += 1;
                } else {
                    self.stalled = 0;
                }
                (self.speed, remaining.clamp(-0.05, 0.1))
            }
        };
        if self.stalled > STALL_TICKS {
            step.end = Some(End::Blocked);
            return step;
        }
        let drag = if ground { DRAG_GROUND } else { DRAG_AIR };
        let coast = [last[0] * drag, (last[1] - GRAVITY) * DRAG_Y, last[2] * drag];
        let desired = [self.dir[0] * h, v, self.dir[1] * h];
        step.push = [desired[0] - coast[0], desired[1] - coast[1], desired[2] - coast[2]];
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN_JUMP: Input = Input { forward: 1.0, strafe: 0.0, jump: true, sneak: false };

    /// Valla al sur: cara en z = 1.375, fondo hasta z = 1.625, 1.5 de alto sobre el suelo y = 64.
    /// Con media anchura 0.3, el centro no puede estar en (1.075, 1.925) por debajo de y = 65.5.
    fn fence() -> Obstacle {
        Obstacle {
            height: 1.5,
            top: 65.5,
            distance: 0.575,
            normal: [0.0, -1.0],
            clear: 1.45,
            onto: false,
            crouch: false,
        }
    }

    /// Física de vanilla con suelo en y = 64 y la valla.
    struct Sim {
        pos: [f64; 3],
        vel: [f64; 3],
        last: [f64; 3],
    }

    impl Sim {
        fn running() -> Sim {
            // Corriendo al sur a 0.28 por tick en suelo.
            Sim { pos: [0.5, 64.0, 0.5], vel: [0.0, -0.0784, 0.28 * DRAG_GROUND], last: [0.0, 0.0, 0.28] }
        }

        fn ground(&self) -> bool {
            self.pos[1] <= 64.0 + 1e-9
        }

        fn step(&mut self, vault: &mut Vault) -> Step {
            let ground = self.ground();
            let step = vault.tick(self.pos, self.last, ground);
            for i in 0..3 {
                self.vel[i] += step.push[i];
            }
            let mut next = [self.pos[0] + self.vel[0], self.pos[1] + self.vel[1], self.pos[2] + self.vel[2]];
            if next[1] < 64.0 {
                next[1] = 64.0;
                self.vel[1] = 0.0;
            }
            let inside = |z: f64| z > 1.075 && z < 1.925;
            if next[1] < 65.5 && inside(next[2]) {
                next[2] = if self.pos[2] <= 1.075 { 1.075 } else { next[2] };
                self.vel[2] = 0.0;
            }
            self.last = [next[0] - self.pos[0], next[1] - self.pos[1], next[2] - self.pos[2]];
            self.pos = next;
            let drag = if self.ground() { DRAG_GROUND } else { DRAG_AIR };
            self.vel = [self.vel[0] * drag, (self.vel[1] - GRAVITY) * DRAG_Y, self.vel[2] * drag];
            step
        }
    }

    #[test]
    fn vaulting_needs_speed_facing_and_a_jump() {
        let ob = fence();
        let south = [0.0, 1.0];
        assert!(can_vault(&ob, south, 0.28, RUN_JUMP));
        assert!(!can_vault(&ob, south, 0.2, RUN_JUMP));
        assert!(!can_vault(&ob, south, 0.28, Input { jump: false, ..RUN_JUMP }));
        assert!(!can_vault(&ob, [1.0, 0.0], 0.28, RUN_JUMP));
        assert!(!can_vault(&ob, south, 0.28, Input { sneak: true, ..RUN_JUMP }));
    }

    #[test]
    fn a_running_vault_clears_the_fence_and_keeps_its_speed() {
        let mut sim = Sim::running();
        let mut vault = Vault::start(&fence(), sim.pos, [0.0, 1.0], 0.28);
        let mut over = false;
        let mut end = None;
        for tick in 0..30 {
            let step = sim.step(&mut vault);
            over |= step.over_started;
            if let Some(e) = step.end {
                end = Some((e, tick));
                break;
            }
        }
        let (end, ticks) = end.expect("termina");
        assert_eq!(end, End::Done);
        assert!(over);
        assert!(ticks <= 14, "tarda {ticks} ticks");
        assert!(sim.pos[2] >= 1.925, "no ha pasado: {:?}", sim.pos);
        assert!(sim.last[2] >= 0.25, "pierde la carrera: {:?}", sim.last);
    }

    #[test]
    fn something_in_the_way_gives_up() {
        let mut vault = Vault::start(&fence(), [0.5, 64.0, 0.5], [0.0, 1.0], 0.28);
        // No se mueve nunca.
        let end = (0..30).find_map(|_| vault.tick([0.5, 64.0, 0.5], [0.0; 3], true).end);
        assert_eq!(end, Some(End::Blocked));
    }

    #[test]
    fn a_window_is_crossed_crouching() {
        let mut vault = Vault::start(&Obstacle { crouch: true, ..fence() }, [0.5, 64.0, 0.5], [0.0, 1.0], 0.3);
        assert!(vault.tick([0.5, 64.0, 0.5], [0.0, 0.0, 0.28], true).sneak);
    }
}
