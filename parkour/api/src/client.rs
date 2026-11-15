//! Mitad cliente: sigue al jugador local tick a tick y dibuja el HUD de depuración.

use std::sync::Mutex;

use horizon_client::mod_api::{Corner, Host, HudEvent, MovementInputEvent};

use crate::config;
use crate::state::{Input, Tracker};

const LINE_H: f32 = 10.0;
const PAD: f32 = 3.0;

#[derive(Default)]
pub struct Client {
    tracker: Mutex<Tracker>,
}

impl Client {
    /// Un tick de la ventana (no del servidor integrado).
    pub fn tick(&self, host: &dyn Host) {
        let Ok(mut tracker) = self.tracker.lock() else { return };
        match host.player() {
            Some(p) => tracker.tick([p.x, p.y, p.z]),
            None => tracker.reset(),
        }
    }

    pub fn movement_input(&self, event: &MovementInputEvent) {
        if let Ok(mut tracker) = self.tracker.lock() {
            tracker.input = Input {
                forward: event.forward,
                strafe: event.strafe,
                jump: event.jump,
                sneak: event.sneak,
            };
        }
    }

    pub fn hud(&self, host: &dyn Host, event: &mut HudEvent<'_>) {
        let shown = host.setting(crate::NAMESPACE, config::DEBUG_HUD.key);
        if config::DEBUG_HUD.value(shown.as_deref()) != "sí" {
            return;
        }
        let Ok(tracker) = self.tracker.lock() else { return };
        if tracker.pos.is_none() {
            return;
        }
        let input = tracker.input;
        let lines = [
            format!("Parkour: {} ({} t)", tracker.current.name(), tracker.ticks_in_move),
            format!(
                "vel h {:.3}  v {:+.3}  caída {:.1}",
                tracker.speed_h(),
                tracker.vel[1],
                tracker.fall_distance()
            ),
            format!(
                "suelo {}  av {:+.1} lat {:+.1}{}{}",
                if tracker.on_ground() { "sí" } else { "no" },
                input.forward,
                input.strafe,
                if input.jump { "  SALTO" } else { "" },
                if input.sneak { "  AGACHADO" } else { "" },
            ),
        ];
        drop(tracker);

        let width = lines.iter().map(|l| host.text_width(l)).fold(0.0, f32::max) + 2.0 * PAD;
        let height = LINE_H * lines.len() as f32 + 2.0 * PAD;
        let (x, y) = event.corners.place(Corner::TopLeft, width, height);
        host.fill(event.batch, [x, y, width, height], 0x7800_0000);
        for (i, line) in lines.iter().enumerate() {
            let at = [x + PAD, y + PAD + LINE_H * i as f32];
            host.text(event.batch, line, at, 0x00FF_FFFF, true);
        }
    }
}
