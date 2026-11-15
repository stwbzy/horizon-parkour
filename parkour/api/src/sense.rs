//! Sensores: qué hay alrededor del jugador (suelo, paredes, repisas, techo), leído de las cajas
//! de colisión de los bloques. Lógica pura sobre el trait `World`, para probarla sin el juego.
//!
//! Coordenadas: `feet` es la posición del jugador (centro de la base de su caja). Las direcciones
//! horizontales son vectores unitarios `[dx, dz]`.

/// Media anchura de la caja del jugador.
pub const HALF_WIDTH: f64 = 0.3;
pub const STAND_HEIGHT: f64 = 1.8;
pub const CROUCH_HEIGHT: f64 = 1.5;
/// Lo que el jugador sube andando sin saltar: por debajo no es pared ni repisa.
pub const STEP_HEIGHT: f64 = 0.6;

/// Hasta dónde se busca una pared a los lados y de frente.
pub const WALL_REACH: f64 = 0.7;
/// Altura máxima de una repisa sobre los pies para que el sensor la vea.
pub const LEDGE_MAX: f64 = 3.3;
/// Hasta dónde se mide el vacío bajo los pies.
pub const DROP_MAX: f64 = 16.0;

const EPS: f64 = 1e-6;

/// Caja en coordenadas del mundo: `[min_x, min_y, min_z, max_x, max_y, max_z]`.
pub type Aabb = [f64; 6];

/// El mundo tal como lo ven los sensores.
pub trait World {
    /// Cajas de colisión del bloque `pos`, en coordenadas locales del bloque
    /// (`Host::block_collision`). Vacío para aire, fluidos y bloques sin cargar.
    fn boxes(&self, pos: [i32; 3]) -> Vec<Aabb>;
}

/// Hacia dónde mira el jugador en horizontal. Yaw de Minecraft: 0 mira al sur (+z), 90 al oeste (-x).
pub fn forward(yaw_deg: f32) -> [f64; 2] {
    let yaw = f64::from(yaw_deg).to_radians();
    [-yaw.sin(), yaw.cos()]
}

/// La derecha del jugador.
pub fn right(yaw_deg: f32) -> [f64; 2] {
    let yaw = f64::from(yaw_deg).to_radians();
    [-yaw.cos(), -yaw.sin()]
}

/// Una pared tocada por un barrido horizontal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wall {
    /// Cuánto puede avanzar la caja del jugador antes de tocarla (0 = pegado).
    pub distance: f64,
    /// Normal de la cara tocada, hacia el jugador (alineada a un eje).
    pub normal: [f64; 2],
}

/// Una repisa a la que se podría subir.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ledge {
    /// Altura del borde sobre los pies.
    pub height: f64,
    /// Y del borde en el mundo.
    pub top: f64,
    /// Distancia horizontal hasta la pared de la repisa.
    pub distance: f64,
    pub normal: [f64; 2],
    /// Dónde quedarían los pies al subir.
    pub landing: [f64; 3],
    /// Cabe agachado pero no de pie.
    pub crouch_only: bool,
}

/// Todo lo que los sensores leen en un tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Senses {
    pub ground: bool,
    pub wall_left: Option<Wall>,
    pub wall_right: Option<Wall>,
    pub wall_front: Option<Wall>,
    pub ledge: Option<Ledge>,
    /// Espacio libre sobre la cabeza (de pie), hasta 2 bloques.
    pub headroom: f64,
    /// Vacío bajo los pies, hasta `DROP_MAX`.
    pub drop: f64,
}

pub fn sense(world: &impl World, feet: [f64; 3], yaw_deg: f32) -> Senses {
    let fwd = forward(yaw_deg);
    let rgt = right(yaw_deg);
    let lft = [-rgt[0], -rgt[1]];
    Senses {
        ground: on_ground(world, feet),
        wall_left: tall_wall(world, feet, lft),
        wall_right: tall_wall(world, feet, rgt),
        wall_front: tall_wall(world, feet, fwd),
        ledge: ledge(world, feet, fwd),
        headroom: headroom(world, feet),
        drop: drop_below(world, feet),
    }
}

/// La caja del jugador con los pies en `feet` y altura `height`.
pub fn player_box(feet: [f64; 3], height: f64) -> Aabb {
    [
        feet[0] - HALF_WIDTH,
        feet[1],
        feet[2] - HALF_WIDTH,
        feet[0] + HALF_WIDTH,
        feet[1] + height,
        feet[2] + HALF_WIDTH,
    ]
}

/// Las cajas del mundo (en coordenadas del mundo) que pueden tocar `area`. Mira un bloque más
/// abajo porque vallas y muros miden 1.5 de alto.
fn boxes_near(world: &impl World, area: Aabb) -> Vec<Aabb> {
    let lo = |v: f64| (v - EPS).floor() as i32;
    let hi = |v: f64| (v + EPS).floor() as i32;
    let mut out = Vec::new();
    for x in lo(area[0])..=hi(area[3]) {
        for y in (lo(area[1]) - 1)..=hi(area[4]) {
            for z in lo(area[2])..=hi(area[5]) {
                let (fx, fy, fz) = (f64::from(x), f64::from(y), f64::from(z));
                for b in world.boxes([x, y, z]) {
                    out.push([b[0] + fx, b[1] + fy, b[2] + fz, b[3] + fx, b[4] + fy, b[5] + fz]);
                }
            }
        }
    }
    out
}

/// Se solapan con volumen (tocarse por una cara no cuenta).
fn overlaps(a: &Aabb, b: &Aabb) -> bool {
    (0..3).all(|i| a[i] < b[i + 3] - EPS && a[i + 3] > b[i] + EPS)
}

/// Nada sólido dentro de `area`.
pub fn is_free(world: &impl World, area: Aabb) -> bool {
    !boxes_near(world, area).iter().any(|b| overlaps(&area, b))
}

/// Hay algo sólido justo debajo de la caja del jugador.
pub fn on_ground(world: &impl World, feet: [f64; 3]) -> bool {
    let mut probe = player_box(feet, 0.0);
    probe[1] = feet[1] - 0.05;
    probe[4] = feet[1];
    !is_free(world, probe)
}

/// Barre la caja del jugador en la dirección `dir` hasta `reach`, mirando sólo la franja vertical
/// `[y0, y1]` del mundo. Devuelve lo primero que toca.
pub fn sweep(
    world: &impl World,
    feet: [f64; 3],
    dir: [f64; 2],
    reach: f64,
    y0: f64,
    y1: f64,
) -> Option<Wall> {
    let (cx, cz) = (feet[0], feet[2]);
    let (ex, ez) = (cx + dir[0] * reach, cz + dir[1] * reach);
    let area = [
        cx.min(ex) - HALF_WIDTH,
        y0,
        cz.min(ez) - HALF_WIDTH,
        cx.max(ex) + HALF_WIDTH,
        y1,
        cz.max(ez) + HALF_WIDTH,
    ];
    let mut best: Option<Wall> = None;
    for b in boxes_near(world, area) {
        if b[1] >= y1 - EPS || b[4] <= y0 + EPS {
            continue;
        }
        // Rayo desde el centro contra la caja ensanchada con la media anchura del jugador.
        let slabs = [
            (cx, dir[0], b[0] - HALF_WIDTH, b[3] + HALF_WIDTH, [1.0, 0.0]),
            (cz, dir[1], b[2] - HALF_WIDTH, b[5] + HALF_WIDTH, [0.0, 1.0]),
        ];
        let mut enter = f64::NEG_INFINITY;
        let mut exit = f64::INFINITY;
        let mut normal = [0.0, 0.0];
        let mut miss = false;
        for (origin, d, min, max, axis) in slabs {
            if d.abs() < EPS {
                // Paralelo a estas caras: sólo toca si ya está entre ellas.
                if origin <= min + EPS || origin >= max - EPS {
                    miss = true;
                }
                continue;
            }
            let (t0, t1) = ((min - origin) / d, (max - origin) / d);
            let (near, far) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
            if near > enter {
                enter = near;
                let sign = if d > 0.0 { -1.0 } else { 1.0 };
                normal = [axis[0] * sign, axis[1] * sign];
            }
            exit = exit.min(far);
        }
        // `enter` algo negativo: ya pegado a la cara (o un pelo dentro por redondeo).
        if miss || enter > exit - EPS || enter < -0.05 || enter > reach {
            continue;
        }
        let distance = enter.max(0.0);
        if best.is_none_or(|w| distance < w.distance) {
            best = Some(Wall { distance, normal });
        }
    }
    best
}

/// Una pared de al menos dos bloques de alto: la tocan tanto las piernas como el pecho. La franja
/// baja empieza sobre la altura de un escalón para que losas y escaleras no cuenten.
pub fn tall_wall(world: &impl World, feet: [f64; 3], dir: [f64; 2]) -> Option<Wall> {
    let low = sweep(world, feet, dir, WALL_REACH, feet[1] + STEP_HEIGHT, feet[1] + 1.0)?;
    let high = sweep(world, feet, dir, WALL_REACH, feet[1] + 1.1, feet[1] + STAND_HEIGHT)?;
    // Misma cara, no un saliente arriba y otro bloque más lejos abajo.
    ((low.distance - high.distance).abs() < 0.2 && low.normal == high.normal)
        .then_some(if low.distance <= high.distance { low } else { high })
}

/// La repisa más baja delante del jugador a la que se puede subir: un obstáculo más alto que un
/// escalón, con sitio libre encima para el jugador y sin techo en el camino.
pub fn ledge(world: &impl World, feet: [f64; 3], dir: [f64; 2]) -> Option<Ledge> {
    let wall = sweep(world, feet, dir, WALL_REACH, feet[1] + STEP_HEIGHT, feet[1] + LEDGE_MAX)?;
    // Un punto justo dentro del obstáculo, y donde quedaría el jugador subido a él.
    let inside = wall.distance + HALF_WIDTH + 0.05;
    let column = [
        feet[0] + dir[0] * inside - 0.05,
        feet[1] + STEP_HEIGHT,
        feet[2] + dir[1] * inside - 0.05,
        feet[0] + dir[0] * inside + 0.05,
        feet[1] + LEDGE_MAX,
        feet[2] + dir[1] * inside + 0.05,
    ];
    let mut tops: Vec<f64> = boxes_near(world, column)
        .iter()
        .filter(|b| overlaps(&column, b))
        .map(|b| b[4])
        .filter(|top| *top > feet[1] + STEP_HEIGHT && *top <= feet[1] + LEDGE_MAX)
        .collect();
    tops.sort_by(f64::total_cmp);
    tops.dedup_by(|a, b| (*a - *b).abs() < EPS);

    let step_in = wall.distance + 2.0 * HALF_WIDTH + 0.05;
    for top in tops {
        let landing = [feet[0] + dir[0] * step_in, top, feet[2] + dir[1] * step_in];
        let crouch = is_free(world, player_box(landing, CROUCH_HEIGHT));
        if !crouch {
            continue;
        }
        // El hueco por el que sube el cuerpo: sobre la cabeza hasta la altura final.
        let mut shaft = player_box(feet, STAND_HEIGHT);
        shaft[1] = feet[1] + STAND_HEIGHT;
        shaft[4] = top + CROUCH_HEIGHT;
        if shaft[4] > shaft[1] && !is_free(world, shaft) {
            return None;
        }
        return Some(Ledge {
            height: top - feet[1],
            top,
            distance: wall.distance,
            normal: wall.normal,
            landing,
            crouch_only: !is_free(world, player_box(landing, STAND_HEIGHT)),
        });
    }
    None
}

/// Espacio libre sobre la cabeza de un jugador de pie, hasta 2 bloques.
pub fn headroom(world: &impl World, feet: [f64; 3]) -> f64 {
    const MAX: f64 = 2.0;
    let head = feet[1] + STAND_HEIGHT;
    let mut probe = player_box(feet, 0.0);
    probe[1] = head;
    probe[4] = head + MAX;
    boxes_near(world, probe)
        .iter()
        .filter(|b| overlaps(&probe, b))
        .map(|b| b[1] - head)
        .fold(MAX, f64::min)
        .max(0.0)
}

/// Vacío bajo los pies hasta lo primero sólido, como mucho `DROP_MAX`.
pub fn drop_below(world: &impl World, feet: [f64; 3]) -> f64 {
    let mut probe = player_box(feet, 0.0);
    probe[1] = feet[1] - DROP_MAX;
    probe[4] = feet[1];
    boxes_near(world, probe)
        .iter()
        .filter(|b| overlaps(&probe, b))
        .map(|b| feet[1] - b[4])
        .fold(DROP_MAX, f64::min)
        .max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Un mundo de prueba: bloques con sus cajas locales.
    #[derive(Default)]
    struct Blocks(HashMap<[i32; 3], Vec<Aabb>>);

    const FULL: Aabb = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    const SLAB: Aabb = [0.0, 0.0, 0.0, 1.0, 0.5, 1.0];

    impl Blocks {
        fn full(mut self, x: i32, y: i32, z: i32) -> Self {
            self.0.insert([x, y, z], vec![FULL]);
            self
        }
        fn with(mut self, x: i32, y: i32, z: i32, b: Aabb) -> Self {
            self.0.insert([x, y, z], vec![b]);
            self
        }
        /// Suelo de bloques enteros en y=63 (los pies van en y=64).
        fn floor(mut self) -> Self {
            for x in -4..=4 {
                for z in -4..=4 {
                    self.0.insert([x, 63, z], vec![FULL]);
                }
            }
            self
        }
        /// Columna de `height` bloques sobre el suelo.
        fn pillar(mut self, x: i32, z: i32, height: i32) -> Self {
            for y in 64..64 + height {
                self.0.insert([x, y, z], vec![FULL]);
            }
            self
        }
    }

    impl World for Blocks {
        fn boxes(&self, pos: [i32; 3]) -> Vec<Aabb> {
            self.0.get(&pos).cloned().unwrap_or_default()
        }
    }

    const SOUTH: f32 = 0.0; // +z
    const WEST: f32 = 90.0; // -x
    const FEET: [f64; 3] = [0.5, 64.0, 0.5];

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn yaw_follows_minecraft() {
        let f = forward(SOUTH);
        assert!(close(f[0], 0.0) && close(f[1], 1.0));
        let f = forward(WEST);
        assert!(close(f[0], -1.0) && close(f[1], 0.0));
        // Mirando al sur, la derecha es el oeste.
        let r = right(SOUTH);
        assert!(close(r[0], -1.0) && close(r[1], 0.0));
    }

    #[test]
    fn ground_under_feet_and_not_in_the_air() {
        let world = Blocks::default().floor();
        assert!(on_ground(&world, FEET));
        assert!(!on_ground(&world, [0.5, 64.3, 0.5]));
        assert!(close(drop_below(&world, [0.5, 67.0, 0.5]), 3.0));
    }

    #[test]
    fn standing_on_a_slab_edge_counts_as_ground() {
        let world = Blocks::default().with(0, 64, 0, SLAB);
        // El centro está fuera del bloque, pero la caja aún pisa la losa.
        assert!(on_ground(&world, [1.2, 64.5, 0.5]));
        assert!(!on_ground(&world, [1.4, 64.5, 0.5]));
    }

    #[test]
    fn a_two_block_wall_to_the_side_is_seen_with_its_distance() {
        // Pared al oeste (x = -1), jugador mirando al sur: queda a su derecha.
        let world = Blocks::default().floor().pillar(-1, 0, 3);
        let s = sense(&world, FEET, SOUTH);
        let wall = s.wall_right.expect("pared a la derecha");
        assert!(close(wall.distance, 0.2));
        assert_eq!(wall.normal, [1.0, 0.0]);
        assert_eq!(s.wall_left, None);
        assert_eq!(s.wall_front, None);
    }

    #[test]
    fn touching_the_wall_is_distance_zero() {
        let world = Blocks::default().floor().pillar(-1, 0, 3);
        let wall = tall_wall(&world, [0.3, 64.0, 0.5], right(SOUTH)).expect("pegado");
        assert!(close(wall.distance, 0.0));
    }

    #[test]
    fn a_one_block_obstacle_is_not_a_wall_but_is_a_ledge() {
        let world = Blocks::default().floor().pillar(0, 1, 1);
        let s = sense(&world, FEET, SOUTH);
        assert_eq!(s.wall_front, None);
        let ledge = s.ledge.expect("repisa de 1 bloque");
        assert!(close(ledge.height, 1.0));
        assert!(close(ledge.distance, 0.2));
        assert!(!ledge.crouch_only);
    }

    #[test]
    fn a_slab_is_neither_wall_nor_ledge() {
        let world = Blocks::default().floor().with(0, 64, 1, SLAB);
        let s = sense(&world, FEET, SOUTH);
        assert_eq!(s.wall_front, None);
        assert_eq!(s.ledge, None);
    }

    #[test]
    fn a_three_block_wall_has_its_ledge_on_top() {
        let world = Blocks::default().floor().pillar(0, 1, 3);
        let s = sense(&world, FEET, SOUTH);
        assert!(s.wall_front.is_some());
        let ledge = s.ledge.expect("repisa a 3 bloques");
        assert!(close(ledge.height, 3.0));
        assert!(close(ledge.landing[2], 1.35));
    }

    #[test]
    fn a_wall_too_high_has_no_ledge() {
        let world = Blocks::default().floor().pillar(0, 1, 5);
        assert_eq!(sense(&world, FEET, SOUTH).ledge, None);
    }

    #[test]
    fn a_low_ceiling_over_the_ledge_allows_only_crouching_or_nothing() {
        // Hueco de 1.5 sobre la repisa: losa superior en y=66.5.
        let roof = [0.0, 0.5, 0.0, 1.0, 1.0, 1.0];
        let world = Blocks::default().floor().pillar(0, 1, 1).with(0, 66, 1, roof);
        let ledge = sense(&world, FEET, SOUTH).ledge.expect("cabe agachado");
        assert!(ledge.crouch_only);
        // Hueco de 1 bloque bajo un muro alto: no cabe, y lo de arriba queda fuera de alcance.
        let mut world = Blocks::default().floor().pillar(0, 1, 1);
        for y in 66..70 {
            world = world.full(0, y, 1);
        }
        assert_eq!(sense(&world, FEET, SOUTH).ledge, None);
    }

    #[test]
    fn a_ceiling_over_the_player_blocks_the_climb() {
        let world = Blocks::default().floor().pillar(0, 1, 2).full(0, 66, 0);
        let s = sense(&world, FEET, SOUTH);
        assert_eq!(s.ledge, None);
        assert!(close(s.headroom, 0.2));
    }

    #[test]
    fn a_fence_is_one_and_a_half_blocks_high() {
        let fence = [0.375, 0.0, 0.375, 0.625, 1.5, 0.625];
        let world = Blocks::default().floor().with(0, 64, 1, fence);
        let ledge = sense(&world, FEET, SOUTH).ledge.expect("valla");
        assert!(close(ledge.height, 1.5));
    }

    #[test]
    fn a_diagonal_sweep_hits_the_nearest_face() {
        let world = Blocks::default().floor().pillar(0, 1, 3);
        let d = std::f64::consts::FRAC_1_SQRT_2;
        let wall = sweep(&world, FEET, [d, d], 1.0, 64.6, 65.8).expect("en diagonal");
        assert_eq!(wall.normal, [0.0, -1.0]);
        assert!(close(wall.distance, 0.2 / d));
    }
}
