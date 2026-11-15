//! Aterrizaje: cuánto daño de caída quita cada forma de caer. Lógica pura.
//!
//! El cliente decide cómo cae (`Landing`) y se lo dice al servidor; quien calcula el daño lo
//! aplica en `living_fall` multiplicando el multiplicador de vanilla (así se suma a Feather
//! Falling y a otros mods en vez de pisarlos).

/// Ticks antes del impacto en los que pulsar agacharse cuenta (0.6 s).
pub const SNEAK_WINDOW_TICKS: u32 = 12;
/// Caída mínima (bloques) para que el aterrizaje tenga animación. Vanilla hace daño desde 3.
pub const MIN_FALL: f64 = 2.5;
/// Hasta esta caída, un roll bien hecho no hace daño.
pub const SAFE_ROLL_FALL: f64 = 8.0;

/// Cómo aterriza el jugador.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Landing {
    /// Caída a plomo: daño de vanilla.
    #[default]
    Plain,
    /// Corriendo: parte del golpe se va en la carrera.
    Run,
    /// Agachándose al caer, sin carrera.
    Crouch,
    /// Corriendo y agachándose a tiempo: rueda.
    Roll,
}

impl Landing {
    /// `running`: lleva velocidad de carrera. `since_sneak`: ticks desde que pulsó agacharse en
    /// el aire, si lo hizo.
    pub fn classify(running: bool, since_sneak: Option<u32>) -> Landing {
        let crouched = since_sneak.is_some_and(|t| t <= SNEAK_WINDOW_TICKS);
        match (running, crouched) {
            (true, true) => Landing::Roll,
            (false, true) => Landing::Crouch,
            (true, false) => Landing::Run,
            (false, false) => Landing::Plain,
        }
    }

    /// Por cuánto se multiplica el daño de una caída de `distance` bloques.
    pub fn damage_factor(self, distance: f64) -> f32 {
        match self {
            Landing::Plain => 1.0,
            Landing::Run => 0.75,
            Landing::Crouch => 0.5,
            Landing::Roll if distance <= SAFE_ROLL_FALL => 0.0,
            Landing::Roll => 0.15,
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Landing::Plain => 0,
            Landing::Run => 1,
            Landing::Crouch => 2,
            Landing::Roll => 3,
        }
    }

    /// Un byte desconocido (otra versión del mod, un cliente raro) es una caída normal.
    pub fn from_byte(byte: u8) -> Landing {
        match byte {
            1 => Landing::Run,
            2 => Landing::Crouch,
            3 => Landing::Roll,
            _ => Landing::Plain,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Landing::Plain => "a plomo",
            Landing::Run => "corriendo",
            Landing::Crouch => "agachado",
            Landing::Roll => "roll",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sneak_must_be_pressed_inside_the_window() {
        assert_eq!(Landing::classify(true, Some(3)), Landing::Roll);
        assert_eq!(Landing::classify(true, Some(SNEAK_WINDOW_TICKS + 1)), Landing::Run);
        assert_eq!(Landing::classify(false, Some(0)), Landing::Crouch);
        assert_eq!(Landing::classify(false, None), Landing::Plain);
    }

    #[test]
    fn a_roll_is_free_up_to_the_safe_fall_and_cheap_after() {
        assert_eq!(Landing::Roll.damage_factor(8.0), 0.0);
        assert_eq!(Landing::Roll.damage_factor(20.0), 0.15);
        assert!(Landing::Run.damage_factor(20.0) > Landing::Crouch.damage_factor(20.0));
        assert_eq!(Landing::Plain.damage_factor(20.0), 1.0);
    }

    #[test]
    fn bytes_round_trip_and_unknown_is_plain() {
        for l in [Landing::Plain, Landing::Run, Landing::Crouch, Landing::Roll] {
            assert_eq!(Landing::from_byte(l.to_byte()), l);
        }
        assert_eq!(Landing::from_byte(200), Landing::Plain);
    }
}
