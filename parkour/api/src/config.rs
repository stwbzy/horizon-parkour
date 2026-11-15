//! Ajustes del mod (Options > Mods > Parkour). Cada ajuste es un botón que cicla sus valores y se
//! guarda con `Host::set_setting` (un archivo por clave).

/// Un ajuste: clave guardada, etiqueta y los valores por los que cicla (el primero es el de fábrica).
pub struct Setting {
    pub key: &'static str,
    pub label: &'static str,
    pub values: &'static [&'static str],
}

pub const DEBUG_HUD: Setting =
    Setting { key: "debug_hud", label: "HUD de depuración", values: &["no", "sí"] };
/// Intensidad de cámara/FOV/manos, en %: 0 deja el parkour sin efectos de cámara.
pub const CINEMATIC: Setting =
    Setting { key: "cinematic", label: "Cinemática", values: &["100", "150", "0", "50"] };

/// En el orden en que salen en la página.
pub const ALL: [&Setting; 2] = [&DEBUG_HUD, &CINEMATIC];

impl Setting {
    /// El valor guardado si es uno de los válidos; si no, el de fábrica.
    pub fn value(&self, stored: Option<&str>) -> &'static str {
        stored
            .and_then(|s| self.values.iter().copied().find(|v| *v == s))
            .unwrap_or(self.values[0])
    }

    pub fn next(&self, stored: Option<&str>) -> &'static str {
        let now = self.value(stored);
        let i = self.values.iter().position(|v| *v == now).unwrap_or(0);
        self.values[(i + 1) % self.values.len()]
    }

    pub fn button(&self, stored: Option<&str>) -> String {
        let value = self.value(stored);
        if self.key == CINEMATIC.key {
            format!("{}: {value} %", self.label)
        } else {
            format!("{}: {value}", self.label)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_values_fall_back_to_default() {
        assert_eq!(DEBUG_HUD.value(None), "no");
        assert_eq!(DEBUG_HUD.value(Some("quizá")), "no");
        assert_eq!(DEBUG_HUD.value(Some("sí")), "sí");
    }

    #[test]
    fn clicking_cycles_and_wraps() {
        assert_eq!(CINEMATIC.next(None), "150");
        assert_eq!(CINEMATIC.next(Some("50")), "100");
        assert_eq!(CINEMATIC.button(Some("50")), "Cinemática: 50 %");
    }
}
