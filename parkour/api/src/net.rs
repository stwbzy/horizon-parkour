//! Mensajes entre la mitad cliente y la mitad servidor del mod (`horizon:mod_net`). Lo que manda
//! un cliente es entrada del jugador: el servidor no se fía de más de lo que dice cada canal.

/// Cliente → servidor, sin datos: "dime mi id de entidad". Se repite hasta recibir `ID`.
pub const HELLO: &str = "parkour:hello";

/// Servidor → cliente: el id de red del jugador (i32, little endian). `living_fall` sólo da ids,
/// y el cliente no tiene otra forma de saber cuál es el suyo.
pub const ID: &str = "parkour:id";

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
}
