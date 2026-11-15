//! Mantle: agarrarse a una repisa y subir a pulso. Lógica pura.
//!
//! El juego sólo deja empujar al jugador (sumar a su velocidad), no colocarlo, así que la subida
//! es un controlador: cada tick calcula el desplazamiento que quiere y empuja la diferencia con
//! el que la física daría por sí sola (gravedad y rozamiento del aire de vanilla).
//!
//! Colgado (sin pedir avanzar, o en un borde sin sitio para subir) el cuerpo baja a la altura de
//! colgar y se mueve de lado con izquierda/derecha mientras el borde siga (`Hold`, que mide el
//! cliente). Saltar pidiendo atrás se impulsa hacia fuera de la pared.

use crate::sense::Ledge;
use crate::state::Input;

/// Altura máxima del borde sobre los pies para alcanzarlo con las manos.
pub const GRAB_HEIGHT: f64 = 2.05;
/// Distancia máxima a la pared para agarrarse.
pub const GRAB_DISTANCE: f64 = 0.3;
/// Cayendo más rápido que esto (bloques/tick) ya no hay quien se agarre (unos 4.5 bloques).
pub const GRAB_FALL_SPEED: f64 = -0.9;
/// Cuánto hay que mirar hacia la pared: coseno del ángulo máximo (unos 53°).
pub const GRAB_FACING: f64 = 0.6;

/// Ticks con las manos en el borde antes de tirar.
const REACH_TICKS: u32 = 2;
/// Velocidades de subida y de paso sobre el borde, en bloques por tick.
const PULL_SPEED: f64 = 0.2;
const OVER_SPEED: f64 = 0.16;
/// Cuánto se sube por encima del borde antes de avanzar.
const CLEARANCE: f64 = 0.06;
/// Ticks seguidos sin subir (un techo, algo en medio) antes de rendirse.
const STALL_TICKS: u32 = 5;
const OVER_MAX_TICKS: u32 = 12;
/// Avance sobre la repisa con el que ya se da por subido aunque no se note suelo.
const OVER_DISTANCE: f64 = 0.75;
/// Colgado, los pies quedan esto por debajo del borde (los ojos, un poco por debajo de él).
pub const HANG_DEPTH: f64 = 1.75;
/// Lo que se corrige la altura colgado por tick, y la velocidad de lado (bloques por tick).
const HANG_ADJUST: f64 = 0.06;
const SHIMMY_SPEED: f64 = 0.085;
/// Saltar hacia atrás desde el borde: hacia fuera y arriba.
const JUMP_OFF_OUT: f64 = 0.3;
const JUMP_OFF_UP: f64 = 0.42;

/// Gravedad y rozamientos por tick de un jugador en el aire (vanilla).
const GRAVITY: f64 = 0.08;
const DRAG_Y: f64 = 0.98;
const DRAG_H: f64 = 0.91;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Las manos llegan al borde.
    Reach,
    /// Sube a pulso (o cuelga, si no se pide avanzar).
    Pull,
    /// Pasa por encima del borde.
    Over,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// Arriba, de pie en la repisa.
    Done,
    /// Se soltó (agacharse).
    Dropped,
    /// Algo impide subir.
    Blocked,
    /// Saltó hacia atrás, fuera de la pared: `Step::push` lleva el impulso.
    JumpedOff,
}

/// Lo que hay alrededor del borde agarrado, medido por el cliente cada tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hold {
    /// El borde sigue a la izquierda / a la derecha (y cabe el cuerpo).
    pub left: bool,
    pub right: bool,
    /// Aquí se puede subir (hay sitio encima).
    pub climb: bool,
}

impl Hold {
    /// Todo libre: un borde largo con sitio encima.
    pub const OPEN: Hold = Hold { left: true, right: true, climb: true };
}

/// Lo que el mantle pide en un tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    /// Lo que hay que sumar a la velocidad del jugador.
    pub push: [f64; 3],
    /// Colgado sin subir.
    pub hanging: bool,
    /// Hay que ir agachado (la repisa sólo tiene 1.5 de alto).
    pub sneak: bool,
    /// Este tick empieza a pasar sobre el borde.
    pub over_started: bool,
    /// Hacia dónde se mueve de lado colgado: 1 a la derecha, -1 a la izquierda, 0 quieto.
    pub shimmy: f64,
    pub end: Option<End>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mantle {
    /// Hacia la repisa, unitario.
    pub dir: [f64; 2],
    /// Y del borde.
    pub top: f64,
    crouch: bool,
    pub phase: Phase,
    ticks: u32,
    stalled: u32,
    over_ticks: u32,
    over_from: [f64; 2],
    /// Subiendo a pulso (una vez empezado, sigue aunque el sensor deje de ver la repisa).
    climbing: bool,
}

/// Se puede agarrar esta repisa ahora: en el aire, cerca, al alcance de las manos, mirándola,
/// pidiendo avanzar y sin caer demasiado rápido.
pub fn can_grab(ledge: &Ledge, facing: [f64; 2], fall_speed: f64, input: Input) -> bool {
    can_reach(ledge.height, ledge.distance, ledge.normal, facing, fall_speed, input)
}

/// Lo mismo para un borde a `height` sobre los pies, a `distance` de una pared de normal `normal`.
pub fn can_reach(
    height: f64,
    distance: f64,
    normal: [f64; 2],
    facing: [f64; 2],
    fall_speed: f64,
    input: Input,
) -> bool {
    let toward = -(facing[0] * normal[0] + facing[1] * normal[1]);
    height <= GRAB_HEIGHT
        && distance <= GRAB_DISTANCE
        && toward >= GRAB_FACING
        && fall_speed >= GRAB_FALL_SPEED
        && input.forward > 0.1
        && !input.sneak
}

impl Mantle {
    pub fn grab(ledge: &Ledge) -> Mantle {
        Mantle::hang(ledge.top, ledge.normal, ledge.crouch_only)
    }

    /// Agarrado a un borde en `top` de una pared de normal `normal`, haya o no sitio encima.
    pub fn hang(top: f64, normal: [f64; 2], crouch: bool) -> Mantle {
        Mantle {
            dir: [-normal[0], -normal[1]],
            top,
            crouch,
            phase: Phase::Reach,
            ticks: 0,
            stalled: 0,
            over_ticks: 0,
            over_from: [0.0; 2],
            climbing: false,
        }
    }

    /// A la derecha a lo largo del borde (mirando a la pared), unitario.
    pub fn right(&self) -> [f64; 2] {
        [-self.dir[1], self.dir[0]]
    }

    /// Un tick: `feet` es la posición, `last` el desplazamiento del tick anterior y `ground` si
    /// pisa suelo. `input` es lo que pide el jugador (sin tocar) y `hold` lo que hay alrededor.
    pub fn tick(&mut self, feet: [f64; 3], last: [f64; 3], ground: bool, input: Input, hold: Hold) -> Step {
        self.ticks += 1;
        let mut step = Step {
            push: [0.0; 3],
            hanging: false,
            sneak: false,
            over_started: false,
            shimmy: 0.0,
            end: None,
        };
        if input.sneak && self.phase != Phase::Over {
            step.end = Some(End::Dropped);
            return step;
        }
        let remaining = self.top + CLEARANCE - feet[1];
        if self.phase == Phase::Reach && self.ticks > REACH_TICKS {
            self.phase = Phase::Pull;
        }
        if self.phase == Phase::Pull && remaining <= 0.0 {
            self.phase = Phase::Over;
            self.over_from = [feet[0], feet[2]];
            step.over_started = true;
        }
        let desired = match self.phase {
            Phase::Reach => [0.0; 3],
            Phase::Pull => {
                let climb = (input.forward > 0.1 || input.jump) && (hold.climb || self.climbing);
                self.climbing = climb;
                if input.jump && input.forward < -0.1 {
                    // Salta hacia atrás, lejos de la pared.
                    step.end = Some(End::JumpedOff);
                    let desired = [-self.dir[0] * JUMP_OFF_OUT, JUMP_OFF_UP, -self.dir[1] * JUMP_OFF_OUT];
                    let coast = [last[0] * DRAG_H, (last[1] - GRAVITY) * DRAG_Y, last[2] * DRAG_H];
                    step.push = [desired[0] - coast[0], desired[1] - coast[1], desired[2] - coast[2]];
                    return step;
                }
                if !climb && ground {
                    // Colgado, los pies han llegado al suelo: ya no cuelga.
                    step.end = Some(End::Dropped);
                    return step;
                }
                if !climb {
                    step.hanging = true;
                    self.stalled = 0;
                    // De lado (izquierda en `strafe` es positivo) mientras el borde siga.
                    let want = -f64::from(input.strafe);
                    step.shimmy = if want > 0.1 && hold.right {
                        1.0
                    } else if want < -0.1 && hold.left {
                        -1.0
                    } else {
                        0.0
                    };
                    let r = self.right();
                    let v = (self.top - HANG_DEPTH - feet[1]).clamp(-HANG_ADJUST, HANG_ADJUST);
                    let side = step.shimmy * SHIMMY_SPEED;
                    [r[0] * side, v, r[1] * side]
                } else {
                    self.stalled = if last[1] < 0.02 { self.stalled + 1 } else { 0 };
                    if self.stalled > STALL_TICKS {
                        step.end = Some(End::Blocked);
                        return step;
                    }
                    [0.0, remaining.min(PULL_SPEED), 0.0]
                }
            }
            Phase::Over => {
                self.over_ticks += 1;
                step.sneak = self.crouch;
                let progress = (feet[0] - self.over_from[0]) * self.dir[0]
                    + (feet[2] - self.over_from[1]) * self.dir[1];
                let standing = ground && feet[1] >= self.top - 0.01;
                if standing || progress >= OVER_DISTANCE || self.over_ticks > OVER_MAX_TICKS {
                    step.end = Some(End::Done);
                    return step;
                }
                [self.dir[0] * OVER_SPEED, remaining.clamp(-0.05, 0.1), self.dir[1] * OVER_SPEED]
            }
        };
        // Lo que la física haría sola este tick, a partir del último desplazamiento.
        let coast = [last[0] * DRAG_H, (last[1] - GRAVITY) * DRAG_Y, last[2] * DRAG_H];
        step.push = [desired[0] - coast[0], desired[1] - coast[1], desired[2] - coast[2]];
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORWARD: Input = Input { forward: 1.0, strafe: 0.0, jump: false, sneak: false };
    const IDLE: Input = Input { forward: 0.0, strafe: 0.0, jump: false, sneak: false };
    const SNEAK: Input = Input { forward: 0.0, strafe: 0.0, jump: false, sneak: true };

    /// Repisa al sur (+z): pared en z = 1, borde en y = 66. El jugador (media anchura 0.3) no
    /// pasa de z = 0.7 mientras sus pies estén por debajo del borde.
    fn ledge() -> Ledge {
        Ledge {
            height: 1.8,
            top: 66.0,
            distance: 0.0,
            normal: [0.0, -1.0],
            landing: [0.5, 66.0, 1.35],
            crouch_only: false,
        }
    }

    /// Física de vanilla en el aire, con la pared y el suelo de la repisa.
    struct Sim {
        pos: [f64; 3],
        vel: [f64; 3],
        last: [f64; 3],
    }

    impl Sim {
        /// `last` es el desplazamiento del tick anterior; la velocidad es la que la física
        /// deja después de él.
        fn new(pos: [f64; 3], last: [f64; 3]) -> Sim {
            let vel = [last[0] * DRAG_H, (last[1] - GRAVITY) * DRAG_Y, last[2] * DRAG_H];
            Sim { pos, vel, last }
        }

        fn on_ledge(&self) -> bool {
            self.pos[2] > 0.7 && (self.pos[1] - 66.0).abs() < 1e-9
        }

        fn step(&mut self, mantle: &mut Mantle, input: Input) -> Step {
            let step = mantle.tick(self.pos, self.last, self.on_ledge(), input, Hold::OPEN);
            for i in 0..3 {
                self.vel[i] += step.push[i];
            }
            let mut next = [
                self.pos[0] + self.vel[0],
                self.pos[1] + self.vel[1],
                self.pos[2] + self.vel[2],
            ];
            if next[1] < 66.0 {
                if self.pos[2] > 0.7 {
                    next[1] = 66.0; // suelo de la repisa
                    self.vel[1] = 0.0;
                } else if next[2] > 0.7 {
                    next[2] = 0.7; // la pared
                    self.vel[2] = 0.0;
                }
            }
            self.last = [next[0] - self.pos[0], next[1] - self.pos[1], next[2] - self.pos[2]];
            self.pos = next;
            self.vel =
                [self.vel[0] * DRAG_H, (self.vel[1] - GRAVITY) * DRAG_Y, self.vel[2] * DRAG_H];
            step
        }
    }

    #[test]
    fn grabbing_needs_reach_facing_and_intent() {
        let l = ledge();
        let south = [0.0, 1.0];
        assert!(can_grab(&l, south, -0.3, FORWARD));
        assert!(!can_grab(&l, south, -0.3, IDLE));
        assert!(!can_grab(&l, south, -0.3, Input { sneak: true, ..FORWARD }));
        assert!(!can_grab(&l, south, -1.2, FORWARD));
        assert!(!can_grab(&l, [1.0, 0.0], -0.3, FORWARD));
        assert!(!can_grab(&Ledge { height: 2.6, ..l }, south, -0.3, FORWARD));
        assert!(!can_grab(&Ledge { distance: 0.5, ..l }, south, -0.3, FORWARD));
    }

    #[test]
    fn holding_forward_climbs_onto_the_ledge() {
        let mut mantle = Mantle::grab(&ledge());
        // Agarra cayendo, 1.8 por debajo del borde.
        let mut sim = Sim::new([0.5, 64.2, 0.7], [0.0, -0.4, 0.05]);
        let mut end = None;
        let mut over = false;
        for tick in 0..40 {
            let step = sim.step(&mut mantle, FORWARD);
            over |= step.over_started;
            if step.end.is_some() {
                end = Some((step.end, tick));
                break;
            }
        }
        let (end, ticks) = end.expect("termina");
        assert_eq!(end, Some(End::Done));
        assert!(over);
        assert!(ticks <= 25, "tarda {ticks} ticks");
        assert!(sim.pos[2] > 0.7 && sim.pos[1] >= 66.0, "{:?}", sim.pos);
    }

    #[test]
    fn the_fall_stops_within_a_tick_of_grabbing() {
        let mut mantle = Mantle::grab(&ledge());
        let mut sim = Sim::new([0.5, 64.2, 0.7], [0.0, -0.6, 0.0]);
        sim.step(&mut mantle, FORWARD);
        assert!(sim.last[1].abs() < 1e-9, "sigue cayendo: {:?}", sim.last);
    }

    #[test]
    fn letting_go_of_forward_hangs_in_place() {
        let mut mantle = Mantle::grab(&ledge());
        let mut sim = Sim::new([0.5, 64.2, 0.7], [0.0, -0.2, 0.0]);
        for _ in 0..4 {
            sim.step(&mut mantle, FORWARD);
        }
        for _ in 0..20 {
            let step = sim.step(&mut mantle, IDLE);
            assert!(step.hanging);
            assert_eq!(step.end, None);
        }
        let hang = 66.0 - HANG_DEPTH;
        assert!((sim.pos[1] - hang).abs() < 0.05, "no cuelga a su altura: {}", sim.pos[1]);
        assert!((sim.pos[0] - 0.5).abs() < 1e-6, "se va de lado: {:?}", sim.pos);
        // Y vuelve a subir al pedir avanzar.
        let ended = (0..40).any(|_| sim.step(&mut mantle, FORWARD).end == Some(End::Done));
        assert!(ended);
    }

    const RIGHT: Input = Input { forward: 0.0, strafe: -1.0, jump: false, sneak: false };

    /// Cuelga quieto un rato.
    fn hanging() -> (Mantle, Sim) {
        let mut mantle = Mantle::grab(&ledge());
        let mut sim = Sim::new([0.5, 64.2, 0.7], [0.0, -0.2, 0.0]);
        for _ in 0..15 {
            sim.step(&mut mantle, IDLE);
        }
        (mantle, sim)
    }

    #[test]
    fn hanging_moves_sideways_while_the_edge_goes_on() {
        let (mut mantle, mut sim) = hanging();
        // Repisa al sur: mirando al sur, la derecha es el oeste (-x).
        assert_eq!(mantle.right(), [-1.0, 0.0]);
        let x = sim.pos[0];
        for _ in 0..10 {
            assert_eq!(sim.step(&mut mantle, RIGHT).shimmy, 1.0);
        }
        assert!(x - sim.pos[0] > 0.6, "se mueve {}", x - sim.pos[0]);
        assert!((sim.pos[1] - (66.0 - HANG_DEPTH)).abs() < 0.05, "baja o sube: {:?}", sim.pos);
        // Donde se acaba el borde, se para.
        let x = sim.pos[0];
        let stuck = Hold { right: false, ..Hold::OPEN };
        for _ in 0..5 {
            let step = mantle.tick(sim.pos, sim.last, false, RIGHT, stuck);
            assert_eq!(step.shimmy, 0.0);
            sim.vel = [sim.vel[0] + step.push[0], sim.vel[1] + step.push[1], sim.vel[2] + step.push[2]];
            sim.pos = [sim.pos[0] + sim.vel[0], sim.pos[1] + sim.vel[1], sim.pos[2] + sim.vel[2]];
            sim.last = sim.vel;
            sim.vel = [sim.vel[0] * DRAG_H, (sim.vel[1] - GRAVITY) * DRAG_Y, sim.vel[2] * DRAG_H];
        }
        assert!((sim.pos[0] - x).abs() < 0.1, "sigue de lado: {} -> {}", x, sim.pos[0]);
    }

    #[test]
    fn with_no_room_above_forward_just_hangs() {
        let (mut mantle, sim) = hanging();
        let closed = Hold { climb: false, ..Hold::OPEN };
        let step = mantle.tick(sim.pos, sim.last, false, FORWARD, closed);
        assert!(step.hanging && step.end.is_none());
    }

    #[test]
    fn jumping_back_pushes_away_from_the_wall() {
        let (mut mantle, sim) = hanging();
        let back = Input { forward: -1.0, jump: true, ..IDLE };
        let step = mantle.tick(sim.pos, sim.last, false, back, Hold::OPEN);
        assert_eq!(step.end, Some(End::JumpedOff));
        assert!(step.push[2] < -0.2 && step.push[1] > 0.4, "{:?}", step.push);
    }

    #[test]
    fn sneaking_lets_go() {
        let mut mantle = Mantle::grab(&ledge());
        let mut sim = Sim::new([0.5, 64.2, 0.7], [0.0, -0.2, 0.0]);
        sim.step(&mut mantle, FORWARD);
        assert_eq!(sim.step(&mut mantle, SNEAK).end, Some(End::Dropped));
    }

    #[test]
    fn a_ceiling_in_the_way_gives_up() {
        let mut mantle = Mantle::grab(&ledge());
        // No sube nunca: el último desplazamiento vertical es siempre 0.
        let end = (0..30).find_map(|_| mantle.tick([0.5, 64.2, 0.7], [0.0; 3], false, FORWARD, Hold::OPEN).end);
        assert_eq!(end, Some(End::Blocked));
    }

    #[test]
    fn a_low_ledge_is_climbed_crouching() {
        let mut mantle = Mantle::grab(&Ledge { crouch_only: true, ..ledge() });
        let mut sim = Sim::new([0.5, 65.5, 0.7], [0.0; 3]);
        let sneaks = (0..40).map(|_| sim.step(&mut mantle, FORWARD)).any(|s| s.sneak);
        assert!(sneaks);
    }
}
