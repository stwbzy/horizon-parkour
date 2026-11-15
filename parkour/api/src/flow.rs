//! Inercia de cadena: encadenar movimientos (vault, slide, wall-run, wall jump, tic-tac, roll,
//! mantle) sin pararse da un poco más de velocidad, hasta un tope. Parar, andar despacio o pasar
//! un rato corriendo sin hacer nada la pierde. Lógica pura.

/// Movimientos encadenados que cuentan, como mucho.
pub const MAX_LEVEL: u8 = 3;
/// Velocidad extra por movimiento encadenado (+6 %, +18 % como mucho).
const STEP: f64 = 0.06;
/// Ticks en el suelo sin encadenar nada tras los que se pierde (1.5 s).
const KEEP_TICKS: u32 = 30;
/// Por debajo de esta velocidad horizontal en el suelo se pierde de golpe (andar es ~0.22).
const SLOW: f64 = 0.2;
/// Velocidad de la carrera de vanilla en suelo, y lo que conserva el suelo normal por tick.
const SPRINT: f64 = 0.28;
const GROUND_KEEP: f64 = 0.546;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flow {
    level: u8,
    idle: u32,
}

impl Flow {
    /// Un movimiento más en la cadena.
    pub fn chain(&mut self) {
        self.level = (self.level + 1).min(MAX_LEVEL);
        self.idle = 0;
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    /// Un tick: `ground` si pisa suelo sin ningún movimiento en curso, y su velocidad horizontal.
    pub fn tick(&mut self, ground: bool, speed_h: f64) {
        if !ground {
            self.idle = 0;
            return;
        }
        self.idle += 1;
        if self.idle > KEEP_TICKS || speed_h < SLOW {
            *self = Flow::default();
        }
    }

    /// Lo que multiplica las velocidades de los movimientos (1 sin cadena).
    pub fn bonus(&self) -> f64 {
        1.0 + STEP * f64::from(self.level)
    }

    /// El empujón por tick que, corriendo por el suelo en la dirección `dir` (unitaria), lleva la
    /// carrera a la velocidad de la cadena en vez de a la de vanilla. Cero sin cadena.
    pub fn ground_push(&self, dir: [f64; 2]) -> [f64; 3] {
        // En equilibrio, cada empujón `p` por tick suma p·k/(1−k) a la velocidad (k: lo que
        // conserva el suelo), así que para sumar `extra` hace falta extra·(1−k)/k.
        let extra = SPRINT * (self.bonus() - 1.0);
        let p = extra * (1.0 - GROUND_KEEP) / GROUND_KEEP;
        [dir[0] * p, 0.0, dir[1] * p]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaining_adds_up_to_a_cap() {
        let mut f = Flow::default();
        assert_eq!(f.bonus(), 1.0);
        for _ in 0..5 {
            f.chain();
        }
        assert_eq!(f.level(), MAX_LEVEL);
        assert!((f.bonus() - 1.18).abs() < 1e-9);
    }

    #[test]
    fn stopping_or_waiting_loses_it_but_the_air_does_not() {
        let mut f = Flow::default();
        f.chain();
        f.tick(true, 0.1);
        assert_eq!(f.level(), 0, "se paró");
        f.chain();
        for _ in 0..100 {
            f.tick(false, 0.3);
        }
        assert_eq!(f.level(), 1, "en el aire no se pierde");
        for _ in 0..=KEEP_TICKS {
            f.tick(true, 0.3);
        }
        assert_eq!(f.level(), 0, "corriendo sin hacer nada");
    }

    #[test]
    fn the_ground_push_holds_the_chain_speed() {
        let mut f = Flow::default();
        assert_eq!(f.ground_push([0.0, 1.0]), [0.0; 3]);
        f.chain();
        f.chain();
        // Simula el suelo: vanilla deja la carrera en 0.28 con su propio empuje `a`.
        let a = SPRINT * (1.0 - GROUND_KEEP) / GROUND_KEEP;
        let p = f.ground_push([0.0, 1.0])[2];
        let mut v = SPRINT;
        for _ in 0..60 {
            v = (v * GROUND_KEEP) + (a + p) * GROUND_KEEP;
        }
        assert!((v - SPRINT * f.bonus()).abs() < 1e-3, "llega a {v}");
    }
}
