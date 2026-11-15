//! Los brazos que se ven agarrados a un borde, con la skin del jugador. Lógica pura: dados la mano
//! (en el borde) y el hombro, las caras de una caja de brazo de Minecraft (4×12×4 píxeles, o 3 de
//! ancho en las skins finas) con las coordenadas de su textura en la skin.
//!
//! La mano de vanilla sólo se puede mover, no anclar al mundo; estos brazos se dibujan en el mundo
//! (`world_submit`) para que las manos se queden quietas en el borde mientras el cuerpo cuelga, se
//! mueve de lado o sube.

/// Largo y grueso de un brazo, en bloques (12 y 4 píxeles de 16).
pub const LENGTH: f32 = 0.75;
pub const THICK: f32 = 0.25;
/// Ancho de un brazo fino (3 píxeles).
pub const SLIM: f32 = 0.1875;
/// Cuánto pasa la punta de la mano del punto de agarre (los dedos por encima del borde).
const FINGERS: f32 = 0.06;

/// Una cara: sus cuatro esquinas (arriba-izquierda, abajo-izquierda, abajo-derecha, arriba-derecha
/// de la textura), su recorte de la skin en unidades de `world_quad` (16 por lado de la skin) y lo
/// que se oscurece (1 sin sombra).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub corners: [[f32; 3]; 4],
    pub uv: [f32; 4],
    pub shade: f32,
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: [f32; 3]) -> Option<[f32; 3]> {
    let len = dot(a, a).sqrt();
    (len > 1e-5).then(|| scale(a, 1.0 / len))
}

/// Un recorte de la skin (64×64) en píxeles, como lo pide `world_quad`.
fn skin(x0: f32, y0: f32, x1: f32, y1: f32) -> [f32; 4] {
    [x0 / 4.0, y0 / 4.0, x1 / 4.0, y1 / 4.0]
}

/// Las caras de un brazo agarrado en `hand` que sube hasta `shoulder`. `outer` apunta hacia fuera
/// del cuerpo por ese lado (para orientar el ancho). `left`: el brazo izquierdo (su sitio en la
/// skin es otro); `slim`: skin de brazos finos. Todo en las mismas coordenadas (bloques).
pub fn arm(hand: [f32; 3], shoulder: [f32; 3], outer: [f32; 3], left: bool, slim: bool) -> Vec<Face> {
    let Some(a) = unit(sub(shoulder, hand)) else { return Vec::new() };
    // El ancho, perpendicular al brazo; si el brazo va justo hacia fuera, cualquier horizontal.
    let across = sub(outer, scale(a, dot(outer, a)));
    let Some(u) = unit(across).or_else(|| unit(cross(a, [0.0, 1.0, 0.0]))) else { return Vec::new() };
    let w = cross(a, u);
    let width = if slim { SLIM } else { THICK };
    let (hu, hw) = (width / 2.0, THICK / 2.0);
    let base = sub(hand, scale(a, FINGERS));
    let top = add(base, scale(a, LENGTH));
    // Esquina a lo ancho (su), a lo grueso (sw) y a lo largo (en la mano o en el hombro).
    let at = |end: [f32; 3], su: f32, sw: f32| add(add(end, scale(u, su * hu)), scale(w, sw * hw));

    // Recortes de la skin: brazo derecho en (40, 16), izquierdo en (32, 48); 4 (o 3) de ancho,
    // 4 de grueso y 12 de largo. Arriba es el hombro, abajo la mano.
    let (x, y) = if left { (32.0, 48.0) } else { (40.0, 16.0) };
    let (wd, d, h) = (if slim { 3.0 } else { 4.0 }, 4.0, 12.0);
    let outer_uv = skin(x, y + d, x + d, y + d + h);
    let front_uv = skin(x + d, y + d, x + d + wd, y + d + h);
    let inner_uv = skin(x + d + wd, y + d, x + 2.0 * d + wd, y + d + h);
    let back_uv = skin(x + 2.0 * d + wd, y + d, x + 2.0 * (d + wd), y + d + h);
    let shoulder_uv = skin(x + d, y, x + d + wd, y + d);
    let hand_uv = skin(x + d + wd, y, x + d + 2.0 * wd, y + d);

    // Las caras de los lados van del hombro (arriba de la textura) a la mano (abajo).
    let side = |a0: (f32, f32), a1: (f32, f32), uv: [f32; 4], shade: f32| Face {
        corners: [at(top, a0.0, a0.1), at(base, a0.0, a0.1), at(base, a1.0, a1.1), at(top, a1.0, a1.1)],
        uv,
        shade,
    };
    let mut faces = vec![
        side((1.0, 1.0), (1.0, -1.0), outer_uv, 0.8),
        side((1.0, -1.0), (-1.0, -1.0), front_uv, 0.9),
        side((-1.0, -1.0), (-1.0, 1.0), inner_uv, 0.8),
        side((-1.0, 1.0), (1.0, 1.0), back_uv, 0.7),
        Face {
            corners: [at(base, -1.0, -1.0), at(base, -1.0, 1.0), at(base, 1.0, 1.0), at(base, 1.0, -1.0)],
            uv: hand_uv,
            shade: 1.0,
        },
        Face {
            corners: [at(top, -1.0, 1.0), at(top, -1.0, -1.0), at(top, 1.0, -1.0), at(top, 1.0, 1.0)],
            uv: shoulder_uv,
            shade: 0.6,
        },
    ];
    // Cada cara también al revés: así se ve sea cual sea el sentido que el motor descarta.
    let back: Vec<Face> = faces
        .iter()
        .map(|f| Face {
            corners: [f.corners[3], f.corners[2], f.corners[1], f.corners[0]],
            uv: [f.uv[2], f.uv[1], f.uv[0], f.uv[3]],
            shade: f.shade,
        })
        .collect();
    faces.extend(back);
    faces
}

/// La luz empaquetada como la espera `BlockEntityDraw::light`, de la luz de bloque y de cielo
/// (0 a 15) de `Host::light`.
pub fn packed_light((block, sky): (u8, u8)) -> u32 {
    u32::from(block.min(15)) * 16 | (u32::from(sky.min(15)) * 16) << 8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
        let d = sub(a, b);
        dot(d, d).sqrt()
    }

    #[test]
    fn an_arm_is_a_closed_box_from_the_hand_toward_the_shoulder() {
        let hand = [0.0, 1.0, 1.0];
        let shoulder = [0.0, 0.6, 0.6];
        let faces = arm(hand, shoulder, [1.0, 0.0, 0.0], false, false);
        assert_eq!(faces.len(), 12);
        let points: Vec<[f32; 3]> = faces.iter().flat_map(|f| f.corners).collect();
        // Ocho esquinas distintas, todas a menos de largo + grueso de la mano.
        let mut distinct: Vec<[f32; 3]> = Vec::new();
        for p in &points {
            if !distinct.iter().any(|q| dist(*p, *q) < 1e-4) {
                distinct.push(*p);
            }
        }
        assert_eq!(distinct.len(), 8);
        assert!(points.iter().all(|p| dist(*p, hand) < LENGTH + THICK));
        // Va hacia el hombro: la mitad de las esquinas queda más cerca de él que la mano.
        let nearer = points.iter().filter(|p| dist(**p, shoulder) < dist(hand, shoulder)).count();
        assert_eq!(nearer, points.len() / 2);
    }

    #[test]
    fn the_texture_comes_from_the_right_place_of_the_skin() {
        let right = arm([0.0; 3], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0], false, false);
        let left = arm([0.0; 3], [0.0, -1.0, 0.0], [-1.0, 0.0, 0.0], true, true);
        // Mano del brazo derecho: (48, 16) a (52, 20) de la skin.
        assert_eq!(right[4].uv, [12.0, 4.0, 13.0, 5.0]);
        // Brazo izquierdo: empieza en (32, 48), y el fino tiene 3 de ancho.
        assert_eq!(left[1].uv, [9.0, 13.0, 9.75, 16.0]);
        assert!(arm([0.0; 3], [0.0; 3], [1.0, 0.0, 0.0], false, false).is_empty());
    }

    #[test]
    fn light_is_packed_block_low_and_sky_high() {
        assert_eq!(packed_light((15, 15)), 240 | 240 << 8);
        assert_eq!(packed_light((0, 7)), 112 << 8);
    }
}
