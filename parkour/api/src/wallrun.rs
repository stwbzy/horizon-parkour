//! Wall-run: correr por una pared lateral (con la cámara inclinada), subir corriendo por una pared
//! de frente, y el wall jump desde cualquiera de los dos. Lógica pura.
//!
//! Como el mantle, es un controlador de velocidad: cada tick empuja la diferencia entre el
//! desplazamiento deseado y el que daría la física sola en el aire.

use crate::sense::Wall;
use crate::state::Input;

/// Distancia máxima a una pared lateral para empezar a correr por ella.
pub const SIDE_REACH: f64 = 0.35;
/// Velocidad a lo largo de la pared necesaria para empezar (esprintar es ~0.28).
pub const SIDE_SPEED: f64 = 0.18;
/// Vacío mínimo bajo los pies: justo encima del suelo no tiene sentido.
pub const SIDE_DROP: f64 = 1.0;
/// Cayendo más rápido que esto ya no se engancha a la pared.
pub const SIDE_FALL_SPEED: f64 = -0.6;
/// Cuánto hay que mirar hacia una pared de frente para subir por ella (coseno, unos 37°).
pub const UP_FACING: f64 = 0.8;
/// Paredes seguidas sin tocar suelo (con wall jumps entre medias).
pub const MAX_WALLS: u8 = 4;
/// Ticks tras soltarse en los que aún se puede hacer el wall jump.
pub const GRACE_TICKS: u32 = 5;

/// Lo que dura como mucho la carrera lateral, y su arco: sube al principio y baja al final.
const SIDE_TICKS: u32 = 28;
const SIDE_RISE: f64 = 0.1;
const SIDE_SINK: f64 = -0.12;
const SIDE_MIN_SPEED: f64 = 0.26;
const SIDE_MAX_SPEED: f64 = 0.34;
/// La subida vertical: empieza como un salto y frena cada tick (unos 2.5 bloques).
const UP_SPEED: f64 = 0.42;
const UP_DECAY: f64 = 0.032;
/// Cuánto se acerca a la pared por tick para no despegarse.
const HUG: f64 = 0.08;
/// Ticks seguidos sin ver la pared antes de soltarse (una ventana, un hueco).
const LOST_TICKS: u32 = 2;
/// Wall jump: hacia fuera de la pared y hacia arriba.
const KICK_OUT: f64 = 0.32;
const KICK_UP: f64 = 0.42;

const GRAVITY: f64 = 0.08;
const DRAG_Y: f64 = 0.98;
const DRAG_AIR: f64 = 0.91;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Pared a un lado: corre a lo largo.
    Side,
    /// Pared de frente: sube.
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// Wall jump: `Step::push` lleva el impulso.
    Jump,
    /// Soltó avanzar.
    Released,
    /// Se acabó la pared (o algo la corta).
    Lost,
    /// Se acabó el impulso.
    Tired,
    /// Miró hacia fuera de la pared.
    LookedAway,
    /// Se agachó: se suelta.
    Dropped,
    /// Tocó suelo.
    Ground,
}

impl End {
    /// Tras estas aún se puede saltar de la pared unos ticks.
    pub fn allows_grace(self) -> bool {
        matches!(self, End::Released | End::Lost | End::Tired | End::LookedAway)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub push: [f64; 3],
    pub end: Option<End>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallRun {
    pub kind: Kind,
    /// Normal de la pared, hacia el jugador.
    pub normal: [f64; 2],
    /// A lo largo de la pared, en el sentido de la carrera (cero subiendo).
    tangent: [f64; 2],
    speed: f64,
    pub ticks: u32,
    lost: u32,
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// La velocidad horizontal a lo largo de la pared (quitando la que va hacia ella o se aleja).
fn along(normal: [f64; 2], vel: [f64; 3]) -> [f64; 2] {
    let h = [vel[0], vel[2]];
    let n = dot(h, normal);
    [h[0] - n * normal[0], h[1] - n * normal[1]]
}

/// Se puede empezar a correr por esta pared lateral: en el aire, pegado a ella, moviéndose a lo
/// largo y mirando hacia donde va, pidiendo avanzar y con vacío debajo.
pub fn can_run_side(wall: &Wall, facing: [f64; 2], vel: [f64; 3], input: Input, drop: f64) -> bool {
    let t = along(wall.normal, vel);
    let speed = t[0].hypot(t[1]);
    let into = dot(facing, wall.normal);
    wall.distance <= SIDE_REACH
        && speed >= SIDE_SPEED
        && dot(facing, [t[0] / speed, t[1] / speed]) >= 0.3
        && (-0.85..=0.5).contains(&into)
        && vel[1] >= SIDE_FALL_SPEED
        && drop >= SIDE_DROP
        && input.forward > 0.1
        && !input.sneak
}

/// Se puede subir corriendo por esta pared de frente: mirándola, saltando y habiendo corrido.
pub fn can_run_up(wall: &Wall, facing: [f64; 2], ran: bool, input: Input) -> bool {
    -dot(facing, wall.normal) >= UP_FACING && ran && input.jump && input.forward > 0.1 && !input.sneak
}

impl WallRun {
    pub fn side(wall: &Wall, vel: [f64; 3]) -> WallRun {
        let t = along(wall.normal, vel);
        let speed = t[0].hypot(t[1]).max(1e-9);
        WallRun {
            kind: Kind::Side,
            normal: wall.normal,
            tangent: [t[0] / speed, t[1] / speed],
            speed: speed.clamp(SIDE_MIN_SPEED, SIDE_MAX_SPEED),
            ticks: 0,
            lost: 0,
        }
    }

    pub fn up(wall: &Wall) -> WallRun {
        WallRun { kind: Kind::Up, normal: wall.normal, tangent: [0.0; 2], speed: 0.0, ticks: 0, lost: 0 }
    }

    /// Hacia la pared: la dirección en la que buscarla cada tick.
    pub fn toward(&self) -> [f64; 2] {
        [-self.normal[0], -self.normal[1]]
    }

    /// El empujón del wall jump, con `last` el desplazamiento del último tick: hacia fuera de la
    /// pared y arriba; corriendo de lado, además sigue hacia delante.
    pub fn kick(&self, last: [f64; 3]) -> [f64; 3] {
        let forward = self.speed.max(SIDE_MIN_SPEED) * 0.9;
        let h = match self.kind {
            Kind::Side => [
                self.tangent[0] * forward + self.normal[0] * KICK_OUT,
                self.tangent[1] * forward + self.normal[1] * KICK_OUT,
            ],
            Kind::Up => [self.normal[0] * KICK_OUT, self.normal[1] * KICK_OUT],
        };
        push([h[0], KICK_UP, h[1]], last)
    }

    /// Un tick. `wall` es la pared vista ahora en la dirección de `toward` (o nada), `last` el
    /// desplazamiento del tick anterior, `facing` hacia dónde mira, `input` lo que pide el
    /// jugador y `jump` si acaba de pulsar saltar.
    pub fn tick(
        &mut self,
        wall: Option<Wall>,
        last: [f64; 3],
        ground: bool,
        facing: [f64; 2],
        input: Input,
        jump: bool,
    ) -> Step {
        self.ticks += 1;
        let stop = |end| Step { push: [0.0; 3], end: Some(end) };
        if jump {
            return Step { push: self.kick(last), end: Some(End::Jump) };
        }
        if input.sneak {
            return stop(End::Dropped);
        }
        if ground && self.ticks > 2 {
            return stop(End::Ground);
        }
        self.lost = if wall.is_some() { 0 } else { self.lost + 1 };
        if self.lost >= LOST_TICKS {
            return stop(End::Lost);
        }
        let hug = wall.map_or(0.0, |w| w.distance.min(HUG));
        let (h, v) = match self.kind {
            Kind::Side => {
                if input.forward <= 0.1 {
                    return stop(End::Released);
                }
                if dot(facing, self.normal) > 0.7 {
                    return stop(End::LookedAway);
                }
                if self.ticks > SIDE_TICKS {
                    return stop(End::Tired);
                }
                // Algo delante en la pared lo frena: se acabó.
                if self.ticks > 3 && dot([last[0], last[2]], self.tangent) < 0.08 {
                    return stop(End::Lost);
                }
                let u = f64::from(self.ticks) / f64::from(SIDE_TICKS);
                let v = SIDE_RISE + (SIDE_SINK - SIDE_RISE) * u;
                let h = [
                    self.tangent[0] * self.speed - self.normal[0] * hug,
                    self.tangent[1] * self.speed - self.normal[1] * hug,
                ];
                (h, v)
            }
            Kind::Up => {
                let v = UP_SPEED - UP_DECAY * f64::from(self.ticks);
                if v <= 0.0 {
                    return stop(End::Tired);
                }
                // Un techo: deja de subir.
                if self.ticks > 2 && last[1] < 0.02 {
                    return stop(End::Lost);
                }
                ([-self.normal[0] * hug, -self.normal[1] * hug], v)
            }
        };
        Step { push: push([h[0], v, h[1]], last), end: None }
    }
}

/// Lo que hay que sumar a la velocidad para que el próximo desplazamiento sea `desired`, en el aire.
fn push(desired: [f64; 3], last: [f64; 3]) -> [f64; 3] {
    let coast = [last[0] * DRAG_AIR, (last[1] - GRAVITY) * DRAG_Y, last[2] * DRAG_AIR];
    [desired[0] - coast[0], desired[1] - coast[1], desired[2] - coast[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: Input = Input { forward: 1.0, strafe: 0.0, jump: false, sneak: false };
    const IDLE: Input = Input { forward: 0.0, strafe: 0.0, jump: false, sneak: false };
    const SOUTH: [f64; 2] = [0.0, 1.0];

    /// Pared al oeste (normal +x), pegada.
    fn west_wall() -> Wall {
        Wall { distance: 0.0, normal: [1.0, 0.0] }
    }

    /// Pared al sur, de frente (normal -z).
    fn south_wall() -> Wall {
        Wall { distance: 0.0, normal: [0.0, -1.0] }
    }

    /// Física del aire de vanilla; la pared lateral en x = 0.3 (el centro no pasa de ahí).
    fn simulate(run: &mut WallRun, ticks: u32, input: Input, wall: Wall) -> (Vec<[f64; 3]>, Option<End>) {
        let mut pos = [0.3, 70.0, 0.0];
        let mut last = [0.0, 0.1, 0.28];
        let mut vel = [0.0, (0.1 - GRAVITY) * DRAG_Y, 0.28 * DRAG_AIR];
        let mut path = Vec::new();
        for _ in 0..ticks {
            let step = run.tick(Some(wall), last, false, SOUTH, input, false);
            if step.end.is_some() {
                return (path, step.end);
            }
            for i in 0..3 {
                vel[i] += step.push[i];
            }
            let mut next = [pos[0] + vel[0], pos[1] + vel[1], pos[2] + vel[2]];
            if run.kind == Kind::Side && next[0] < 0.3 {
                next[0] = 0.3;
                vel[0] = 0.0;
            }
            last = [next[0] - pos[0], next[1] - pos[1], next[2] - pos[2]];
            pos = next;
            path.push(pos);
            vel = [vel[0] * DRAG_AIR, (vel[1] - GRAVITY) * DRAG_Y, vel[2] * DRAG_AIR];
        }
        (path, None)
    }

    #[test]
    fn a_side_run_needs_speed_along_the_wall_and_air_below() {
        let w = west_wall();
        let vel = [0.0, 0.1, 0.28];
        assert!(can_run_side(&w, SOUTH, vel, RUN, 3.0));
        assert!(!can_run_side(&w, SOUTH, vel, RUN, 0.5));
        assert!(!can_run_side(&w, SOUTH, [0.0, 0.1, 0.05], RUN, 3.0));
        assert!(!can_run_side(&w, SOUTH, vel, IDLE, 3.0));
        assert!(!can_run_side(&Wall { distance: 0.6, ..w }, SOUTH, vel, RUN, 3.0));
        // Mirando hacia atrás o de espaldas a la pared, no.
        assert!(!can_run_side(&w, [0.0, -1.0], vel, RUN, 3.0));
        assert!(!can_run_side(&w, [1.0, 0.0], vel, RUN, 3.0));
    }

    #[test]
    fn a_side_run_arcs_along_the_wall_and_tires() {
        let mut run = WallRun::side(&west_wall(), [0.0, 0.1, 0.28]);
        let (path, end) = simulate(&mut run, 60, RUN, west_wall());
        assert_eq!(end, Some(End::Tired));
        let top = path.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
        let last = path.last().expect("corre");
        assert!(top > 70.3 && top < 71.5, "sube {top}");
        assert!((last[1] - 70.0).abs() < 0.8, "acaba a {}", last[1]);
        assert!(last[2] > 7.0, "recorre {}", last[2]);
        assert!(path.iter().all(|p| (p[0] - 0.3).abs() < 1e-9), "se despega");
    }

    #[test]
    fn letting_go_looking_away_or_losing_the_wall_ends_it() {
        let mut run = WallRun::side(&west_wall(), [0.0, 0.1, 0.28]);
        assert_eq!(run.tick(Some(west_wall()), [0.0, 0.1, 0.28], false, SOUTH, IDLE, false).end, Some(End::Released));
        let mut run = WallRun::side(&west_wall(), [0.0, 0.1, 0.28]);
        assert_eq!(
            run.tick(Some(west_wall()), [0.0, 0.1, 0.28], false, [1.0, 0.0], RUN, false).end,
            Some(End::LookedAway)
        );
        let mut run = WallRun::side(&west_wall(), [0.0, 0.1, 0.28]);
        assert_eq!(run.tick(None, [0.0, 0.1, 0.28], false, SOUTH, RUN, false).end, None);
        assert_eq!(run.tick(None, [0.0, 0.1, 0.28], false, SOUTH, RUN, false).end, Some(End::Lost));
    }

    #[test]
    fn a_wall_jump_pushes_out_up_and_on() {
        let mut run = WallRun::side(&west_wall(), [0.0, 0.1, 0.28]);
        let last = [0.0, 0.0, 0.3];
        let step = run.tick(Some(west_wall()), last, false, SOUTH, RUN, true);
        assert_eq!(step.end, Some(End::Jump));
        // El siguiente desplazamiento: fuera (+x), arriba y hacia delante.
        let coast = [0.0, (0.0 - GRAVITY) * DRAG_Y, 0.3 * DRAG_AIR];
        let next = [coast[0] + step.push[0], coast[1] + step.push[1], coast[2] + step.push[2]];
        assert!((next[0] - KICK_OUT).abs() < 1e-9 && (next[1] - KICK_UP).abs() < 1e-9 && next[2] > 0.25);
    }

    #[test]
    fn running_up_a_wall_climbs_about_two_and_a_half_blocks() {
        assert!(can_run_up(&south_wall(), SOUTH, true, Input { jump: true, ..RUN }));
        assert!(!can_run_up(&south_wall(), SOUTH, false, Input { jump: true, ..RUN }));
        assert!(!can_run_up(&south_wall(), [1.0, 0.0], true, Input { jump: true, ..RUN }));
        let mut run = WallRun::up(&south_wall());
        let (path, end) = simulate(&mut run, 40, RUN, south_wall());
        assert_eq!(end, Some(End::Tired));
        let top = path.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
        assert!((72.2..73.0).contains(&top), "sube hasta {top}");
    }

    #[test]
    fn kicking_off_a_front_wall_goes_back() {
        let run = WallRun::up(&south_wall());
        let push = run.kick([0.0, 0.2, 0.0]);
        assert!(push[2] <= -KICK_OUT + 1e-9 && push[1] > 0.3, "{push:?}");
    }

    #[test]
    fn only_some_ends_leave_time_for_a_late_jump() {
        assert!(End::Tired.allows_grace() && End::Released.allows_grace());
        assert!(!End::Ground.allows_grace() && !End::Dropped.allows_grace() && !End::Jump.allows_grace());
    }
}
