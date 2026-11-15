//! Mitad cliente: sigue al jugador local tick a tick, lee los sensores y dibuja el HUD de
//! depuración.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

use horizon_client::mod_api::{Corner, Host, HudEvent, MovementInputEvent};

use crate::config;
use crate::sense::{self, Aabb, Senses, Wall};
use crate::state::{Input, Tracker};

const LINE_H: f32 = 10.0;
const PAD: f32 = 3.0;

#[derive(Default)]
pub struct Client {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    tracker: Tracker,
    senses: Senses,
}

/// El mundo de la ventana para los sensores. Guarda las cajas ya pedidas: en un tick los
/// sensores preguntan varias veces por los mismos bloques.
struct HostWorld<'a> {
    host: &'a dyn Host,
    cache: RefCell<HashMap<[i32; 3], Vec<Aabb>>>,
}

impl sense::World for HostWorld<'_> {
    fn boxes(&self, pos: [i32; 3]) -> Vec<Aabb> {
        self.cache.borrow_mut().entry(pos).or_insert_with(|| self.host.block_collision(pos)).clone()
    }
}

impl Client {
    /// Un tick de la ventana (no del servidor integrado).
    pub fn tick(&self, host: &dyn Host) {
        let Ok(mut inner) = self.inner.lock() else { return };
        let Some(player) = host.player() else {
            *inner = Inner::default();
            return;
        };
        let feet = [player.x, player.y, player.z];
        let world = HostWorld { host, cache: RefCell::default() };
        inner.senses = sense::sense(&world, feet, player.yaw);
        let ground = inner.senses.ground;
        inner.tracker.tick(feet, ground);
    }

    pub fn movement_input(&self, event: &MovementInputEvent) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.tracker.input = Input {
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
        let Ok(inner) = self.inner.lock() else { return };
        let (tracker, senses) = (&inner.tracker, &inner.senses);
        if tracker.pos.is_none() {
            return;
        }
        let input = tracker.input;
        let wall = |w: Option<Wall>| w.map_or("--".to_owned(), |w| format!("{:.2}", w.distance));
        let ledge = senses.ledge.map_or("--".to_owned(), |l| {
            format!(
                "+{:.2} a {:.2}{}",
                l.height,
                l.distance,
                if l.crouch_only { " (agachado)" } else { "" }
            )
        });
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
            format!(
                "pared izq {}  der {}  frente {}",
                wall(senses.wall_left),
                wall(senses.wall_right),
                wall(senses.wall_front)
            ),
            format!("repisa {ledge}"),
            format!("techo {:.2}  vacío {:.1}", senses.headroom, senses.drop),
        ];
        drop(inner);

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
