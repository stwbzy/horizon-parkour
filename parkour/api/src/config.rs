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

/// Polvo del bloque al aterrizar, deslizar, correr por paredes y apoyarse.
pub const PARTICLES: Setting =
    Setting { key: "particles", label: "Partículas", values: &["sí", "no"] };

/// En el orden en que salen en la página.
pub const ALL: [&Setting; 3] = [&CINEMATIC, &PARTICLES, &DEBUG_HUD];

/// El archivo de ajustes del servidor, en la carpeta `config/` del juego (junto a
/// `server.properties` en un servidor dedicado). Se crea con los valores de fábrica si no está.
pub const SERVER_FILE: &str = "parkour-server.txt";

pub const SERVER_DEFAULT: &str = "\
# Parkour: ajustes del servidor. true o false; se relee cada 10 s.

# Deja usar los movimientos de parkour (mantle, vault, slide, wall-run). Con false, los clientes
# con el mod juegan como en vanilla.
moves = true

# Aterrizar corriendo, agachado o rodando, o agarrarse a una repisa, quita daño de caída.
fall_damage = true

# Comprueba lo que dicen los clientes: rodar o correr al caer exige haber llegado corriendo, y
# agarrarse exige haber subido después. Sin esto, un cliente trucado podría no recibir daño.
validate = true
";

/// Los ajustes del servidor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    pub moves: bool,
    pub fall_damage: bool,
    pub validate: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig { moves: true, fall_damage: true, validate: true }
    }
}

impl ServerConfig {
    /// Lee `clave = true|false` por línea; `#` empieza un comentario. Lo que no entiende se queda
    /// con el valor de fábrica.
    pub fn parse(text: &str) -> ServerConfig {
        let mut config = ServerConfig::default();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = match value.trim() {
                "true" => true,
                "false" => false,
                _ => continue,
            };
            match key.trim() {
                "moves" => config.moves = value,
                "fall_damage" => config.fall_damage = value,
                "validate" => config.validate = value,
                _ => {}
            }
        }
        config
    }
}

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
    fn the_server_file_reads_its_own_default_and_changes() {
        assert_eq!(ServerConfig::parse(SERVER_DEFAULT), ServerConfig::default());
        let c = ServerConfig::parse("moves = false\nvalidate=false # sin comprobar\nraro = 3\n");
        assert_eq!(c, ServerConfig { moves: false, fall_damage: true, validate: false });
        assert_eq!(ServerConfig::parse("fall_damage = quizá"), ServerConfig::default());
    }

    #[test]
    fn clicking_cycles_and_wraps() {
        assert_eq!(CINEMATIC.next(None), "150");
        assert_eq!(CINEMATIC.next(Some("50")), "100");
        assert_eq!(CINEMATIC.button(Some("50")), "Cinemática: 50 %");
    }
}
