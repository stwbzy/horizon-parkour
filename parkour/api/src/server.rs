//! Mitad servidor: recuerda cómo dijo cada jugador que iba a aterrizar, por id de entidad, para
//! cuando `living_fall` pregunte (ese hook no da `Host`, sólo el id).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use horizon_client::mod_api::{Host, NetFromClientEvent};

use crate::land::Landing;
use crate::net;

/// Un aviso de aterrizaje más viejo que esto ya no cuenta: el cliente lo refresca mientras cae.
const FRESH: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct Server {
    landings: Mutex<HashMap<u32, (Landing, Instant)>>,
}

impl Server {
    pub fn net_from_client(&self, host: &mut dyn Host, event: &NetFromClientEvent<'_>) {
        if event.channel != net::HELLO && event.channel != net::LAND {
            return;
        }
        let Some(id) = entity_id(host, event.player) else { return };
        if event.channel == net::HELLO {
            // Sin respuesta si el canal falla: el cliente vuelve a preguntar.
            let _ = host.net_send_to_player(event.player, net::ID, &net::encode_id(id as i32));
        } else if let (Some(&byte), Ok(mut landings)) = (event.data.first(), self.landings.lock()) {
            landings.retain(|_, (_, at)| at.elapsed() < FRESH);
            landings.insert(id, (Landing::from_byte(byte), Instant::now()));
        }
    }

    /// Cómo avisó la entidad `id` que aterriza, si el aviso es reciente.
    pub fn landing(&self, id: u32) -> Option<Landing> {
        let landings = self.landings.lock().ok()?;
        landings.get(&id).filter(|(_, at)| at.elapsed() < FRESH).map(|(l, _)| *l)
    }
}

/// El id de red del jugador `uuid` en esta sesión.
fn entity_id(host: &dyn Host, uuid: u128) -> Option<u32> {
    let player = host.server_players().into_iter().find(|p| p.uuid == uuid)?;
    u32::try_from(player.entity_id).ok().filter(|id| *id != 0)
}
