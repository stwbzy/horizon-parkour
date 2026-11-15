//! Curvas de cámara, FOV y manos de cada efecto. Lógica pura: dado el tiempo desde que empezó
//! un efecto, qué pose toca. `intensity` es el ajuste Cinemática (0 a 1.5); a 0 no hay nada.

use std::f32::consts::PI;

/// Lo que un efecto añade a la vista en un instante. Todo es aditivo salvo `fov`, que multiplica.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Giro grande de cabeceo en grados (la voltereta): va por `camera_pose`, sin límite.
    pub spin: f32,
    /// Cabeceo, guiñada y alabeo pequeños en grados: van por `camera_shake` (máx. 10).
    pub pitch: f32,
    pub roll: f32,
    /// Cuánto baja la cámara, en bloques.
    pub drop: f32,
    pub fov: f32,
    /// Manos en primera persona: bajan (bloques) y cabecean (grados).
    pub hand_drop: f32,
    pub hand_pitch: f32,
}

impl Pose {
    pub const NONE: Pose =
        Pose { spin: 0.0, pitch: 0.0, roll: 0.0, drop: 0.0, fov: 1.0, hand_drop: 0.0, hand_pitch: 0.0 };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// Voltereta al aterrizar rodando.
    Roll,
    /// Aterrizaje agachado: la cámara se hunde y vuelve. `strength` 0 a 1 según la caída.
    SoftLanding { strength: f32 },
    /// Golpe seco contra el suelo.
    HardLanding { strength: f32 },
}

/// Lo que dura la voltereta, en segundos y en ticks.
pub const ROLL_SECS: f32 = 0.6;
pub const ROLL_TICKS: u32 = 12;

/// Fuerza de un aterrizaje según los bloques caídos: suave a 3, máxima desde 13.
pub fn landing_strength(fall: f64) -> f32 {
    (((fall - 3.0) / 10.0) as f32).clamp(0.2, 1.0)
}

fn smoothstep(u: f32) -> f32 {
    u * u * (3.0 - 2.0 * u)
}

/// Sube rápido (hasta `peak`) y decae suave hasta 0 en u = 1.
fn kick(u: f32, peak: f32) -> f32 {
    (u / peak).min(1.0) * (1.0 - u).powi(2) / (1.0 - peak).powi(2)
}

impl Effect {
    pub fn duration(self) -> f32 {
        match self {
            Effect::Roll => ROLL_SECS,
            Effect::SoftLanding { .. } => 0.55,
            Effect::HardLanding { .. } => 0.45,
        }
    }

    /// La pose a `t` segundos del inicio; `None` cuando el efecto ya acabó.
    pub fn pose(self, t: f32, intensity: f32) -> Option<Pose> {
        let duration = self.duration();
        if !(0.0..duration).contains(&t) {
            return None;
        }
        let u = t / duration;
        let k = intensity.clamp(0.0, 1.5);
        if k == 0.0 {
            return Some(Pose::NONE);
        }
        let arc = (PI * u).sin();
        Some(match self {
            Effect::Roll => {
                // Con la cinemática al 100 % o más la cámara da la vuelta entera; por debajo,
                // sólo un cabeceo (una media vuelta no acabaría mirando al frente).
                let full = k >= 1.0;
                Pose {
                    spin: if full { 360.0 * smoothstep(u) } else { 0.0 },
                    pitch: if full { 0.0 } else { 10.0 * arc * k },
                    roll: 5.0 * (2.0 * PI * u).sin() * k,
                    drop: 0.9 * arc * k.min(1.0),
                    fov: 1.0 + 0.08 * arc * k,
                    hand_drop: 0.8 * arc,
                    hand_pitch: 60.0 * arc,
                }
            }
            Effect::SoftLanding { strength } => {
                let s = strength * kick(u, 0.3);
                Pose {
                    spin: 0.0,
                    pitch: 4.0 * s * k,
                    roll: 0.0,
                    drop: 0.6 * s * k.min(1.0),
                    fov: 1.0 - 0.02 * s * k,
                    hand_drop: 0.3 * s,
                    hand_pitch: 10.0 * s,
                }
            }
            Effect::HardLanding { strength } => {
                let s = strength * kick(u, 0.15);
                Pose {
                    spin: 0.0,
                    pitch: 6.5 * s * k,
                    roll: 3.0 * s * k,
                    drop: 0.4 * s * k.min(1.0),
                    fov: 1.0 - 0.04 * s * k,
                    hand_drop: 0.25 * s,
                    hand_pitch: 15.0 * s,
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Effect; 3] = [
        Effect::Roll,
        Effect::SoftLanding { strength: 1.0 },
        Effect::HardLanding { strength: 1.0 },
    ];

    #[test]
    fn effects_start_and_end_at_rest() {
        for effect in ALL {
            let start = effect.pose(0.0, 1.0).expect("empieza");
            let end = effect.pose(effect.duration() - 1e-4, 1.0).expect("aún dura");
            for pose in [start, end] {
                assert!(pose.pitch.abs() < 0.1 && pose.roll.abs() < 0.1, "{effect:?} {pose:?}");
                assert!(pose.drop.abs() < 0.01 && (pose.fov - 1.0).abs() < 0.001);
            }
            // La voltereta acaba mirando donde empezó: 0 o una vuelta entera.
            assert!(start.spin.abs() < 0.1 && (end.spin - 360.0).abs() < 0.1 || end.spin == 0.0);
            assert_eq!(effect.pose(effect.duration(), 1.0), None);
        }
    }

    #[test]
    fn zero_intensity_changes_nothing() {
        for effect in ALL {
            assert_eq!(effect.pose(effect.duration() / 2.0, 0.0), Some(Pose::NONE));
        }
    }

    #[test]
    fn the_full_spin_needs_full_intensity() {
        let half = Effect::Roll.pose(ROLL_SECS / 2.0, 0.5).expect("a medias");
        assert_eq!(half.spin, 0.0);
        assert!(half.pitch > 0.0);
        let full = Effect::Roll.pose(ROLL_SECS / 2.0, 1.0).expect("entera");
        assert!((full.spin - 180.0).abs() < 0.1);
    }

    #[test]
    fn small_turns_stay_inside_the_shake_limit() {
        for effect in ALL {
            for i in 0..100 {
                let t = effect.duration() * i as f32 / 100.0;
                let pose = effect.pose(t, 1.5).expect("dura");
                assert!(pose.pitch.abs() <= 10.0 && pose.roll.abs() <= 10.0, "{effect:?} {pose:?}");
            }
        }
    }

    #[test]
    fn harder_falls_hit_harder() {
        assert!(landing_strength(3.0) < landing_strength(8.0));
        assert_eq!(landing_strength(40.0), 1.0);
    }
}
