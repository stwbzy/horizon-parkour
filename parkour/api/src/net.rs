//! Mensajes entre la mitad cliente y la mitad servidor del mod (`horizon:mod_net`). Lo que manda
//! un cliente es entrada del jugador: el servidor no se fía de más de lo que dice cada canal.

/// Versión del protocolo entre las dos mitades. Cambia cuando cambian los mensajes o lo que el
/// servidor acepta; cliente y servidor deben tener la misma.
pub const PROTOCOL: u8 = 1;

/// Cliente → servidor, `[PROTOCOL]`: "tengo el mod, dime mi id de entidad". Se repite hasta
/// recibir `ID`.
pub const HELLO: &str = "parkour:hello";

/// Servidor → cliente: la bienvenida (`Welcome`): el id de red del jugador (i32, little endian),
/// la versión del protocolo del servidor y sus permisos. `living_fall` sólo da ids, y el cliente
/// no tiene otra forma de saber cuál es el suyo. Sin bienvenida, el cliente no hace parkour.
pub const ID: &str = "parkour:id";

/// Permiso de la bienvenida: el servidor deja usar los movimientos.
pub const ALLOW_MOVES: u8 = 1;
/// Permiso de la bienvenida: el servidor quita daño de caída al aterrizar bien.
pub const ALLOW_FALL: u8 = 2;

/// Lo que el servidor contesta a `HELLO`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Welcome {
    pub id: u32,
    pub protocol: u8,
    pub flags: u8,
}

impl Welcome {
    pub fn encode(self) -> [u8; 6] {
        let id = (self.id as i32).to_le_bytes();
        [id[0], id[1], id[2], id[3], self.protocol, self.flags]
    }

    pub fn decode(data: &[u8]) -> Option<Welcome> {
        let [a, b, c, d, protocol, flags] = data.try_into().ok()?;
        let id = decode_id(&[a, b, c, d])?;
        Some(Welcome { id, protocol, flags })
    }

    pub fn allows(self, flag: u8) -> bool {
        self.flags & flag != 0
    }
}

/// La versión que dice un `HELLO` (0 si no dice ninguna: un cliente sin versión).
pub fn hello_protocol(data: &[u8]) -> u8 {
    data.first().copied().unwrap_or(0)
}

/// Cliente → servidor: cómo va a aterrizar (`Landing::to_byte`). Se manda al cambiar y se
/// refresca mientras cae; el servidor lo olvida si no es reciente.
pub const LAND: &str = "parkour:land";

pub fn encode_id(id: i32) -> [u8; 4] {
    id.to_le_bytes()
}

pub fn decode_id(data: &[u8]) -> Option<u32> {
    let id = i32::from_le_bytes(data.try_into().ok()?);
    u32::try_from(id).ok().filter(|id| *id != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_bad_ones_are_refused() {
        assert_eq!(decode_id(&encode_id(1234)), Some(1234));
        assert_eq!(decode_id(&encode_id(0)), None);
        assert_eq!(decode_id(&encode_id(-5)), None);
        assert_eq!(decode_id(&[1, 2]), None);
    }

    #[test]
    fn a_welcome_round_trips() {
        let w = Welcome { id: 77, protocol: PROTOCOL, flags: ALLOW_MOVES };
        assert_eq!(Welcome::decode(&w.encode()), Some(w));
        assert!(w.allows(ALLOW_MOVES) && !w.allows(ALLOW_FALL));
        assert_eq!(Welcome::decode(&encode_id(77)), None);
        assert_eq!(hello_protocol(&[]), 0);
        assert_eq!(hello_protocol(&[PROTOCOL]), PROTOCOL);
    }
}
