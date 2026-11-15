//! Lo que el servidor comprueba de lo que dicen los clientes. Lógica pura.
//!
//! El cliente dice cómo aterriza (`Landing`) y eso quita daño de caída, así que un cliente
//! trucado podría decir siempre "roll" o "agarrado". El servidor ve dónde está cada jugador tick
//! a tick y sólo acepta lo que encaja con eso: rodar o correr exige haber llegado con carrera, y
//! "agarrado" exige haber subido después de caer. Lo que no encaja se rebaja, no se castiga: en
//! servidores de amigos basta con que no salga gratis. También limita los mensajes por segundo.

use std::collections::VecDeque;

use crate::land::Landing;

/// Ticks de posiciones que se guardan por jugador (3 s).
pub const HISTORY: usize = 60;
/// Mensajes por segundo que se le aceptan a un jugador; el resto se ignora.
pub const MAX_MESSAGES: u32 = 20;
/// Ticks antes del aterrizaje en los que se mide la carrera.
const RUN_TICKS: usize = 6;
/// Velocidad horizontal media (bloques/tick) que cuenta como llegar corriendo: algo menos que
/// andar, para no rechazar a quien frena justo antes de caer.
const RUN_SPEED: f64 = 0.15;
/// Lo que hay que haber subido sobre el punto más bajo reciente para que "agarrado" valga.
const GRAB_CLIMB: f64 = 0.5;
/// Un salto de posición mayor es un teletransporte: se empieza de cero.
const TELEPORT: f64 = 8.0;

/// Un jugador visto desde el servidor.
#[derive(Clone, Debug, Default)]
pub struct Watch {
    positions: VecDeque<[f64; 3]>,
    /// Mensajes en el segundo actual y el tick en que empezó.
    messages: u32,
    second: u64,
}

impl Watch {
    /// Su posición en este tick del servidor.
    pub fn observe(&mut self, pos: [f64; 3]) {
        if let Some(last) = self.positions.back()
            && (0..3).any(|i| (pos[i] - last[i]).abs() > TELEPORT)
        {
            self.positions.clear();
        }
        self.positions.push_back(pos);
        while self.positions.len() > HISTORY {
            self.positions.pop_front();
        }
    }

    /// Si se le acepta un mensaje más en el tick `tick` del servidor.
    pub fn allow_message(&mut self, tick: u64) -> bool {
        if tick >= self.second + 20 {
            self.second = tick;
            self.messages = 0;
        }
        self.messages += 1;
        self.messages <= MAX_MESSAGES
    }

    /// Velocidad horizontal media de los últimos ticks.
    pub fn speed_h(&self) -> f64 {
        let n = self.positions.len();
        if n < 2 {
            return 0.0;
        }
        let from = n.saturating_sub(RUN_TICKS + 1);
        let path: f64 = (from + 1..n)
            .map(|i| {
                let (a, b) = (self.positions[i - 1], self.positions[i]);
                (b[0] - a[0]).hypot(b[2] - a[2])
            })
            .sum();
        path / (n - 1 - from) as f64
    }

    /// Ha subido desde lo más bajo de estos segundos (se agarró a algo y trepó).
    pub fn climbed(&self) -> bool {
        let Some(now) = self.positions.back() else { return false };
        let low = self.positions.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        now[1] >= low + GRAB_CLIMB
    }

    /// Lo que se le acepta de lo que dice: lo mismo si encaja, si no algo menos.
    pub fn check(&self, claim: Landing) -> Landing {
        let ran = self.speed_h() >= RUN_SPEED;
        match claim {
            Landing::Plain | Landing::Crouch => claim,
            Landing::Run if ran => Landing::Run,
            Landing::Run => Landing::Plain,
            Landing::Roll if ran => Landing::Roll,
            Landing::Roll => Landing::Crouch,
            Landing::Grab if self.climbed() => Landing::Grab,
            Landing::Grab => Landing::Plain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch(path: impl Iterator<Item = [f64; 3]>) -> Watch {
        let mut w = Watch::default();
        path.for_each(|p| w.observe(p));
        w
    }

    /// Cae 10 bloques en vertical, sin moverse en horizontal.
    fn straight_drop() -> Watch {
        watch((0..20).map(|i| [0.0, 80.0 - 0.5 * f64::from(i), 0.0]))
    }

    /// Corre a 0.28 mientras cae.
    fn running_drop() -> Watch {
        watch((0..20).map(|i| [0.28 * f64::from(i), 80.0 - 0.5 * f64::from(i), 0.0]))
    }

    #[test]
    fn a_roll_needs_a_run() {
        assert_eq!(running_drop().check(Landing::Roll), Landing::Roll);
        assert_eq!(straight_drop().check(Landing::Roll), Landing::Crouch);
        assert_eq!(running_drop().check(Landing::Run), Landing::Run);
        assert_eq!(straight_drop().check(Landing::Run), Landing::Plain);
        assert_eq!(straight_drop().check(Landing::Crouch), Landing::Crouch);
    }

    #[test]
    fn a_grab_needs_a_climb_after_the_fall() {
        assert_eq!(straight_drop().check(Landing::Grab), Landing::Plain);
        // Cae 3 bloques, se agarra y sube 1.8 hasta la repisa.
        let fall = (0..6).map(|i| [0.0, 70.0 - 0.5 * f64::from(i), 0.0]);
        let climb = (0..9).map(|i| [0.0, 67.5 + 0.2 * f64::from(i), 0.0]);
        let w = watch(fall.chain(climb));
        assert_eq!(w.check(Landing::Grab), Landing::Grab);
    }

    #[test]
    fn a_teleport_forgets_the_old_path() {
        let mut w = running_drop();
        w.observe([500.0, 64.0, 0.0]);
        assert_eq!(w.speed_h(), 0.0);
        assert_eq!(w.check(Landing::Roll), Landing::Crouch);
    }

    #[test]
    fn too_many_messages_are_dropped_until_the_next_second() {
        let mut w = Watch::default();
        let allowed = (0..30).filter(|_| w.allow_message(100)).count();
        assert_eq!(allowed, MAX_MESSAGES as usize);
        assert!(w.allow_message(120));
    }
}
