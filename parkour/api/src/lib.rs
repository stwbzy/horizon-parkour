//! Parkour: movimiento de parkour cinemático para Horizon. El launcher enlaza este crate en el
//! juego y llama a `hooks()`. Cada método de `Hooks` es un hook del catálogo (`native-hooks.json`).
//!
//! Reglas de compatibilidad: los efectos de cámara, FOV y manos se suman a los de otros mods,
//! nunca los reemplazan; no hay teclas propias (todo es contextual con salto, sprint y agacharse).
//!
//! Fases 0 y 1: sigue al jugador local, lee los sensores (suelo, paredes, repisas), lo muestra en
//! un HUD de depuración y registra el mod y sus ajustes en Options > Mods. Ver PLAN.md.

pub mod config;
pub mod sense;
pub mod state;

mod client;

use horizon_client::mod_api::{
    Hooks, Host, HudEvent, ModsEvent, MovementInputEvent, NativeMod, PlayerLoggedInEvent,
    SettingsClickEvent, SettingsEvent, Text, TickEvent,
};

/// Id del mod: namespace, carpeta de ajustes y canales de red.
pub const NAMESPACE: &str = "parkour";

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Parkour::default())
}

#[derive(Default)]
struct Parkour {
    client: client::Client,
}

impl Hooks for Parkour {
    fn mods(&self, _host: &mut dyn Host, event: &mut ModsEvent<'_>) {
        event.mods.push(NativeMod {
            id: NAMESPACE.into(),
            title: format!("Parkour {}", env!("CARGO_PKG_VERSION")),
            description: "Parkour cinemático: wall-run, mantle, vault, roll y más.".into(),
            keys: Vec::new(),
        });
    }

    fn settings(&self, host: &mut dyn Host, event: &mut SettingsEvent<'_>) {
        if event.mod_id != NAMESPACE {
            return;
        }
        for setting in config::ALL {
            let stored = host.setting(NAMESPACE, setting.key);
            event.buttons.push(setting.button(stored.as_deref()));
        }
    }

    fn settings_click(&self, host: &mut dyn Host, event: &mut SettingsClickEvent<'_>) {
        if event.mod_id != NAMESPACE {
            return;
        }
        if let Some(setting) = config::ALL.get(event.button) {
            let stored = host.setting(NAMESPACE, setting.key);
            host.set_setting(NAMESPACE, setting.key, setting.next(stored.as_deref()));
        }
    }

    fn player_logged_in(&self, host: &mut dyn Host, event: &mut PlayerLoggedInEvent<'_>) {
        let line = format!(
            "[Parkour {}] cargado (sin movimientos aún). HUD de depuración en Options > Mods.",
            env!("CARGO_PKG_VERSION"),
        );
        host.tell(event.player, Text::literal(line));
    }

    // --- Cliente ---

    fn tick(&self, host: &mut dyn Host, _event: &mut TickEvent) {
        if host.window() {
            self.client.tick(host);
        }
    }

    fn movement_input(&self, _host: &dyn Host, event: &mut MovementInputEvent) {
        self.client.movement_input(event);
    }

    fn hud(&self, host: &mut dyn Host, event: &mut HudEvent<'_>) {
        self.client.hud(host, event);
    }
}
