//! Sonido y partículas de los movimientos: de qué material es el bloque tocado (para que una
//! pisada en madera suene a madera), el FOV de la velocidad y un azar barato. Lógica pura.

/// El grupo de sonido de vanilla de un bloque (`minecraft:block.<grupo>.step`), deducido de su
/// id. No es exacto bloque a bloque, pero acierta lo común; lo que no reconoce suena a piedra.
pub fn material(block: &str) -> &'static str {
    let name = block.rsplit(':').next().unwrap_or(block);
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    if has(&["wool", "carpet"]) {
        "wool"
    } else if has(&["snow", "powder_snow"]) {
        "snow"
    } else if has(&["gravel"]) {
        "gravel"
    } else if has(&["sand"]) && !has(&["sandstone"]) {
        "sand"
    } else if has(&["glass"]) {
        "glass"
    } else if has(&["grass", "dirt", "farmland", "podzol", "mycelium", "moss", "leaves", "hay", "clay"]) {
        "grass"
    } else if has(&[
        "planks", "_log", "_wood", "_stem", "hyphae", "bamboo", "barrel", "crafting_table", "bookshelf",
        "chest", "ladder", "scaffolding", "_door", "trapdoor", "_fence", "_sign",
    ]) && !has(&["iron", "copper", "nether_brick"])
    {
        "wood"
    } else if has(&["iron", "gold_block", "copper", "anvil", "chain", "_bars", "netherite_block"]) {
        "metal"
    } else {
        "stone"
    }
}

/// El sonido `kind` (`step`, `hit`, `fall`) del material del bloque `block`.
pub fn sound(block: &str, kind: &str) -> String {
    format!("minecraft:block.{}.{kind}", material(block))
}

/// El silbido del aire de un movimiento rápido (wall jump, tic-tac, vault, plancha).
pub const WHOOSH: &str = "minecraft:entity.player.attack.sweep";
/// El roce de la ropa.
pub const CLOTH: &str = "minecraft:item.armor.equip_leather";

/// Sin sonido ni polvo: aire y fluidos.
pub fn solid(block: &str) -> bool {
    let name = block.rsplit(':').next().unwrap_or(block);
    !(name.is_empty() || name.ends_with("air") || name == "water" || name == "lava")
}

/// Velocidad horizontal (bloques/tick) desde la que el FOV empieza a abrirse, y la de máximo.
const FAST: f64 = 0.3;
const FASTEST: f64 = 0.55;
/// Lo que se abre como mucho, a intensidad 1.
const SPEED_FOV: f32 = 0.08;

/// El FOV de la velocidad: más rápido que esprintar (slides, wall jumps, caídas largas) se abre
/// un poco, para que la velocidad se sienta.
pub fn speed_fov(speed_h: f64, fall_speed: f64, intensity: f32) -> f32 {
    let k = intensity.clamp(0.0, 1.5);
    let fast = |v: f64, from: f64, to: f64| ((v - from) / (to - from)).clamp(0.0, 1.0) as f32;
    let h = fast(speed_h, FAST, FASTEST);
    let fall = fast(-fall_speed, 1.0, 3.0);
    1.0 + SPEED_FOV * h.max(fall) * k
}

/// Azar barato (xorshift) para repartir partículas; no hace falta más.
#[derive(Clone, Copy, Debug)]
pub struct Rng(u32);

impl Default for Rng {
    fn default() -> Self {
        Rng(0x9E37_79B9)
    }
}

impl Rng {
    /// Entre -1 y 1.
    pub fn signed(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        f64::from(x) / f64::from(u32::MAX) * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_blocks_sound_like_their_material() {
        assert_eq!(material("minecraft:oak_planks"), "wood");
        assert_eq!(material("minecraft:spruce_log"), "wood");
        assert_eq!(material("minecraft:oak_leaves"), "grass");
        assert_eq!(material("minecraft:grass_block"), "grass");
        assert_eq!(material("minecraft:sand"), "sand");
        assert_eq!(material("minecraft:sandstone"), "stone");
        assert_eq!(material("minecraft:white_wool"), "wool");
        assert_eq!(material("minecraft:iron_block"), "metal");
        assert_eq!(material("minecraft:iron_trapdoor"), "metal");
        assert_eq!(material("minecraft:glass_pane"), "glass");
        assert_eq!(material("minecraft:cobblestone"), "stone");
        assert_eq!(material("mimod:raro"), "stone");
        assert_eq!(sound("minecraft:gravel", "step"), "minecraft:block.gravel.step");
    }

    #[test]
    fn air_and_fluids_are_not_solid() {
        assert!(!solid("minecraft:air") && !solid("minecraft:cave_air") && !solid("minecraft:water"));
        assert!(solid("minecraft:stone"));
    }

    #[test]
    fn speed_opens_the_fov_only_past_sprinting() {
        assert_eq!(speed_fov(0.28, 0.0, 1.0), 1.0);
        assert!(speed_fov(0.45, 0.0, 1.0) > 1.0);
        assert!((speed_fov(2.0, 0.0, 1.0) - 1.08).abs() < 1e-6);
        assert!(speed_fov(0.0, -2.0, 1.0) > 1.0);
        assert_eq!(speed_fov(1.0, 0.0, 0.0), 1.0);
    }

    #[test]
    fn the_rng_stays_in_range_and_varies() {
        let mut r = Rng::default();
        let xs: Vec<f64> = (0..100).map(|_| r.signed()).collect();
        assert!(xs.iter().all(|x| (-1.0..=1.0).contains(x)));
        assert!(xs.iter().any(|x| *x > 0.5) && xs.iter().any(|x| *x < -0.5));
    }
}
