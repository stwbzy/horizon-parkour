//! Parkour: movimiento de parkour cinemático para Horizon. El launcher enlaza este crate en el
//! juego y llama a `hooks()`. Cada método de `Hooks` es un hook del catálogo (`native-hooks.json`).
//!
//! Reglas de compatibilidad: los efectos de cámara, FOV y manos se suman a los de otros mods,
//! nunca los reemplazan; no hay teclas propias (todo es contextual con salto, sprint y agacharse).
//!
//! Multijugador: el cliente sólo hace parkour si el servidor tiene el mod con la misma versión
//! y lo permite (`net::Welcome`); el servidor comprueba lo que dicen los clientes (`guard`).
//!
//! Hecho hasta la fase 8 (ver PLAN.md): seguimiento del jugador, sensores, HUD de depuración,
//! ajustes, el aterrizaje (correr, agacharse o rodar al caer quita daño de caída) y la plancha,
//! el mantle (agarrarse a una repisa en el aire y subir a pulso, o colgarse de un borde y moverse
//! por él, con los brazos de la skin agarrados), el vault (normal y rápido), el slide, el wall-run
//! (lateral y vertical), el wall jump y el tic-tac; el margen al saltar (coyote y salto guardado),
//! la inercia de encadenar movimientos, sonido según el material, polvo y FOV de la velocidad.

pub mod arms;
pub mod assist;
pub mod cinematic;
pub mod config;
pub mod flow;
pub mod fx;
pub mod guard;
pub mod land;
pub mod mantle;
pub mod net;
pub mod sense;
pub mod slide;
pub mod state;
pub mod vault;
pub mod wallrun;

mod client;
mod server;

use horizon_client::mod_api::{
    CameraPoseEvent, CameraShakeEvent, ComputeFovModifierEvent, Flow, Hooks, Host, HudEvent,
    LivingFallEvent, ModsEvent, MovementInputEvent, NativeMod, NetFromClientEvent,
    NetFromServerEvent, PlayerLoggedInEvent, RenderHandEvent, RenderHandPoseEvent,
    SettingsClickEvent, SettingsEvent, Text, TickEvent, WorldSubmitEvent,
};

/// Id del mod: namespace, carpeta de ajustes y canales de red.
pub const NAMESPACE: &str = "parkour";

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Parkour::default())
}

#[derive(Default)]
struct Parkour {
    client: client::Client,
    server: server::Server,
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
            "[Parkour {}] Este servidor tiene Parkour (hace falta el mod también en tu juego). \
             Corre y salta cerca de paredes, repisas y obstáculos; agáchate corriendo para \
             deslizarte. Ajustes en Opciones > Mods > Parkour.",
            env!("CARGO_PKG_VERSION"),
        );
        host.tell(event.player, Text::literal(line));
    }

    // --- Donde se calcula el daño (servidor, o el propio cliente al predecir) ---

    /// Quita daño de caída según cómo aterriza el jugador. El hook sólo da el id de la entidad:
    /// si es el jugador de esta ventana, vale lo que decidió su cliente; si es otro, lo que
    /// avisó por red. Se multiplica el multiplicador para sumarse a vanilla y a otros mods.
    fn living_fall(&self, event: &mut LivingFallEvent<'_>) -> Flow {
        if event.kind != "minecraft:player" {
            return Flow::Continue;
        }
        let landing = match self.client.own_id() {
            Some(own) if own == event.entity => self.client.landing(),
            Some(_) => self.server.landing(event.entity),
            // Aún sin id propio (recién entrado): lo avisado por red, o lo local.
            None => self.server.landing(event.entity).or_else(|| self.client.landing()),
        };
        if let Some(landing) = landing {
            event.multiplier *= landing.damage_factor(event.distance);
        }
        Flow::Continue
    }

    fn net_from_client(&self, host: &mut dyn Host, event: &mut NetFromClientEvent<'_>) {
        self.server.net_from_client(host, event);
    }

    // --- Cliente ---

    fn net_from_server(&self, _host: &mut dyn Host, event: &mut NetFromServerEvent<'_>) {
        self.client.net_from_server(event);
    }

    fn camera_pose(&self, _host: &dyn Host, event: &mut CameraPoseEvent<'_>) {
        self.client.camera_pose(event);
    }

    fn camera_shake(&self, _host: &dyn Host, event: &mut CameraShakeEvent<'_>) {
        self.client.camera_shake(event);
    }

    fn compute_fov_modifier(&self, _host: &dyn Host, event: &mut ComputeFovModifierEvent<'_>) {
        self.client.compute_fov_modifier(event);
    }

    fn render_hand_pose(&self, _host: &dyn Host, event: &mut RenderHandPoseEvent<'_>) {
        self.client.render_hand_pose(event);
    }

    fn render_hand(&self, _host: &dyn Host, event: &mut RenderHandEvent<'_>) -> Flow {
        self.client.render_hand(event)
    }

    fn world_submit(&self, host: &dyn Host, event: &mut WorldSubmitEvent<'_>) {
        self.client.world_submit(host, event);
    }

    fn tick(&self, host: &mut dyn Host, _event: &mut TickEvent) {
        if host.window() {
            self.client.tick(host);
        } else {
            self.server.tick(host);
        }
    }

    fn movement_input(&self, _host: &dyn Host, event: &mut MovementInputEvent) {
        self.client.movement_input(event);
    }

    fn hud(&self, host: &mut dyn Host, event: &mut HudEvent<'_>) {
        self.client.hud(host, event);
    }
}
