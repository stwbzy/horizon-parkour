//! Mitad servidor: da la bienvenida a los clientes con el mod (su id de entidad y los permisos
//! del servidor), recuerda cómo dijo cada jugador que iba a aterrizar y lo comprueba contra lo
//! que el servidor ha visto de él (`guard`), para cuando `living_fall` pregunte (ese hook no da
//! `Host`, sólo el id).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use horizon_client::mod_api::{Host, NetFromClientEvent, Text};

use crate::config::{self, ServerConfig};
use crate::guard::Watch;
use crate::land::Landing;
use crate::net::{self, Welcome};

/// Un aviso de aterrizaje más viejo que esto ya no cuenta: el cliente lo refresca mientras cae.
const FRESH: Duration = Duration::from_millis(1500);
/// Cada cuántos ticks se relee el archivo de ajustes.
const RELOAD_TICKS: u64 = 200;
/// Modo de juego espectador: ni se vigila ni cae.
const SPECTATOR: i32 = 3;

#[derive(Default)]
pub struct Server {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    tick: u64,
    config: Option<ServerConfig>,
    /// Por id de entidad: lo visto del jugador y su último aviso de aterrizaje.
    watches: HashMap<u32, Watch>,
    landings: HashMap<u32, (Landing, Instant)>,
    /// Jugadores ya avisados de que su versión no es la del servidor.
    told: HashSet<u128>,
}

impl Server {
    /// Un tick del servidor (el del mundo de un jugador, el de LAN o el dedicado).
    pub fn tick(&self, host: &mut dyn Host) {
        let Ok(mut inner) = self.inner.lock() else { return };
        inner.tick += 1;
        if inner.config.is_none() || inner.tick % RELOAD_TICKS == 0 {
            inner.config = Some(load_config(host));
        }
        let players = host.server_players();
        let mut seen = HashSet::new();
        for p in players {
            let Some(id) = u32::try_from(p.entity_id).ok().filter(|id| *id != 0) else { continue };
            if p.game_mode == SPECTATOR {
                continue;
            }
            seen.insert(id);
            inner.watches.entry(id).or_default().observe(p.position);
        }
        inner.watches.retain(|id, _| seen.contains(id));
        inner.landings.retain(|_, (_, at)| at.elapsed() < FRESH);
    }

    pub fn net_from_client(&self, host: &mut dyn Host, event: &NetFromClientEvent<'_>) {
        if event.channel != net::HELLO && event.channel != net::LAND {
            return;
        }
        let Some(id) = entity_id(host, event.player) else { return };
        let Ok(mut inner) = self.inner.lock() else { return };
        let tick = inner.tick;
        if !inner.watches.entry(id).or_default().allow_message(tick) {
            return;
        }
        let config = inner.config.unwrap_or_default();
        if event.channel == net::HELLO {
            let theirs = net::hello_protocol(event.data);
            if theirs != net::PROTOCOL {
                // Sin bienvenida el cliente no hace parkour; se le dice por qué una vez.
                if inner.told.insert(event.player) {
                    let line = format!(
                        "[Parkour] Tu versión del mod no es la del servidor (protocolo {theirs}, el \
                         servidor usa {}): los movimientos de parkour quedan desactivados.",
                        net::PROTOCOL
                    );
                    host.tell(event.player, Text::literal(line));
                }
                return;
            }
            let mut flags = 0;
            if config.moves {
                flags |= net::ALLOW_MOVES;
            }
            if config.fall_damage {
                flags |= net::ALLOW_FALL;
            }
            let welcome = Welcome { id, protocol: net::PROTOCOL, flags };
            // Sin respuesta si el canal falla: el cliente vuelve a preguntar.
            let _ = host.net_send_to_player(event.player, net::ID, &welcome.encode());
        } else if let Some(&byte) = event.data.first()
            && config.fall_damage
        {
            inner.landings.insert(id, (Landing::from_byte(byte), Instant::now()));
        }
    }

    /// Cómo aterriza la entidad `id`, si lo avisó hace poco, rebajado a lo que encaja con lo que
    /// el servidor ha visto de ella.
    pub fn landing(&self, id: u32) -> Option<Landing> {
        let inner = self.inner.lock().ok()?;
        let config = inner.config.unwrap_or_default();
        if !config.fall_damage {
            return None;
        }
        let claim = inner.landings.get(&id).filter(|(_, at)| at.elapsed() < FRESH).map(|(l, _)| *l)?;
        if !config.validate {
            return Some(claim);
        }
        Some(inner.watches.get(&id).map_or(Landing::Plain, |w| w.check(claim)))
    }
}

/// Lee los ajustes del servidor, y escribe los de fábrica si el archivo no está.
fn load_config(host: &dyn Host) -> ServerConfig {
    let Ok(dir) = host.config_dir() else { return ServerConfig::default() };
    let path = dir.join(config::SERVER_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => ServerConfig::parse(&text),
        Err(_) => {
            let _ = std::fs::write(&path, config::SERVER_DEFAULT);
            ServerConfig::default()
        }
    }
}

/// El id de red del jugador `uuid` en esta sesión.
fn entity_id(host: &dyn Host, uuid: u128) -> Option<u32> {
    let player = host.server_players().into_iter().find(|p| p.uuid == uuid)?;
    u32::try_from(player.entity_id).ok().filter(|id| *id != 0)
}
