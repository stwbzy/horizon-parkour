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
    /// Cuánto se adelanta la cámara en la dirección de la vista (horizontal), en bloques.
    pub lunge: f32,
    /// Manos en primera persona: bajan (bloques), cabecean (grados) y se adelantan (bloques).
    pub hand_drop: f32,
    pub hand_pitch: f32,
    pub hand_forward: f32,
    /// Las manos no se dibujan (la voltereta): se recogen fuera de la vista.
    pub hide_hands: bool,
}

impl Pose {
    pub const NONE: Pose = Pose {
        spin: 0.0,
        pitch: 0.0,
        roll: 0.0,
        drop: 0.0,
        fov: 1.0,
        lunge: 0.0,
        hand_drop: 0.0,
        hand_pitch: 0.0,
        hand_forward: 0.0,
        hide_hands: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// Voltereta al aterrizar rodando.
    Roll,
    /// Aterrizaje agachado: la cámara se hunde y vuelve. `strength` 0 a 1 según la caída.
    SoftLanding { strength: f32 },
    /// Golpe seco contra el suelo.
    HardLanding { strength: f32 },
    /// Las manos agarran una repisa: tirón corto.
    Grab,
    /// Pasar por encima del borde al acabar de subir.
    MantleOver,
    /// Apoyar las manos y saltar un obstáculo; `quick` es el vault rápido de obstáculos bajos.
    Vault { quick: bool },
    /// Impulsarse desde una pared.
    WallJump,
    /// Despegar subiendo por una pared de frente.
    ClimbStart,
    /// Tic-tac: pisar una pared en diagonal e impulsarse a un lado. `side` es hacia dónde sale
    /// respecto a la vista: 1 a la derecha, -1 a la izquierda.
    TicTac { side: f32 },
    /// Lanzarse en plancha en un salto con carrera: el roll vendrá al caer.
    Dive,
}

/// Lo que dura la voltereta, en segundos y en ticks.
pub const ROLL_SECS: f32 = 0.7;
pub const ROLL_TICKS: u32 = 14;

/// La voltereta gira alrededor del centro del cuerpo hecho bola, no del ojo: altura de ese
/// centro sobre los pies y radios (vertical y horizontal) de la vuelta que da el ojo.
const ROLL_PIVOT: f32 = 0.8;
const ROLL_RADIUS_V: f32 = 0.45;
const ROLL_RADIUS_H: f32 = 0.3;
/// Altura del ojo de pie (vanilla).
const EYE: f32 = 1.62;

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

/// 0 → 1 entre `a` y `b`, suave.
fn ramp(u: f32, a: f32, b: f32) -> f32 {
    smoothstep(((u - a) / (b - a)).clamp(0.0, 1.0))
}

/// Entra entre 0 y `inn`, se mantiene y sale entre `out` y 1.
fn hold(u: f32, inn: f32, out: f32) -> f32 {
    ramp(u, 0.0, inn) * (1.0 - ramp(u, out, 1.0))
}

/// La vuelta de la voltereta en grados (0 a 360) a `u` (0 a 1) del efecto: arranca con el
/// impacto, va más rápida al principio que al final y acaba un poco antes que el efecto, que
/// guarda un momento para incorporarse.
pub fn roll_angle(u: f32) -> f32 {
    let s = ((u - 0.04) / 0.78).clamp(0.0, 1.0);
    let early = 1.0 - (1.0 - s).powf(1.6);
    360.0 * smoothstep(early)
}

impl Effect {
    pub fn duration(self) -> f32 {
        match self {
            Effect::Roll => ROLL_SECS,
            Effect::SoftLanding { .. } => 0.55,
            Effect::HardLanding { .. } => 0.45,
            Effect::Grab => 0.3,
            Effect::MantleOver => 0.45,
            Effect::Vault { quick: true } => 0.32,
            Effect::Vault { quick: false } => 0.5,
            Effect::WallJump => 0.4,
            Effect::ClimbStart => 0.35,
            Effect::TicTac { .. } => 0.4,
            Effect::Dive => 0.5,
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
                // Con la cinemática al 100 % o más la cámara da la vuelta entera alrededor del
                // cuerpo: el ojo baja hacia delante, pasa cerca del suelo y vuelve a subir. Por
                // debajo, sólo se agacha y cabecea (una media vuelta no acabaría al frente).
                let crouch = hold(u, 0.12, 0.8);
                let (spin, pitch, eye_y, lunge) = if k >= 1.0 {
                    let angle = roll_angle(u);
                    let (sin, cos) = angle.to_radians().sin_cos();
                    let eye = ROLL_PIVOT + ROLL_RADIUS_V * cos;
                    (angle, 0.0, eye, ROLL_RADIUS_H * sin)
                } else {
                    (0.0, 10.0 * arc * k, ROLL_PIVOT + ROLL_RADIUS_V, 0.0)
                };
                // Las manos se recogen enseguida y no se ven durante la vuelta.
                let tuck = hold(u, 0.1, 0.82);
                Pose {
                    spin,
                    pitch,
                    roll: 3.0 * (2.0 * PI * u).sin() * k,
                    drop: crouch * (EYE - eye_y) * k.min(1.0),
                    fov: 1.0 + 0.08 * arc * k,
                    lunge: crouch * lunge * k.min(1.0),
                    hand_drop: 1.2 * tuck,
                    hand_pitch: 70.0 * tuck,
                    hand_forward: 0.0,
                    hide_hands: tuck > 0.7,
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
                    ..Pose::NONE
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
                    ..Pose::NONE
                }
            }
            Effect::Grab => {
                // El cuerpo se frena de golpe: la cámara cae un poco y mira hacia arriba.
                let s = kick(u, 0.25);
                Pose {
                    spin: 0.0,
                    pitch: -4.0 * s * k,
                    roll: 0.0,
                    drop: 0.15 * s * k.min(1.0),
                    fov: 1.0 - 0.02 * s * k,
                    hand_drop: 0.0,
                    hand_pitch: 0.0,
                    ..Pose::NONE
                }
            }
            Effect::MantleOver => Pose {
                spin: 0.0,
                pitch: 5.0 * arc * k,
                roll: 3.0 * arc * k,
                drop: 0.25 * arc * k.min(1.0),
                fov: 1.0 + 0.03 * arc * k,
                hand_drop: 0.5 * arc,
                hand_pitch: 25.0 * arc,
                ..Pose::NONE
            },
            Effect::WallJump => {
                // Sale despedido: pico de FOV y la cabeza atrás un momento.
                let s = kick(u, 0.2);
                Pose {
                    spin: 0.0,
                    pitch: -4.0 * s * k,
                    roll: 0.0,
                    drop: 0.0,
                    fov: 1.0 + 0.07 * s * k,
                    hand_drop: 0.2 * s,
                    hand_pitch: 15.0 * s,
                    ..Pose::NONE
                }
            }
            Effect::Vault { quick: false } => {
                // Mira al obstáculo al apoyarse, se ladea al pasar y sale con un pico de FOV.
                let lean = kick(u, 0.35);
                Pose {
                    pitch: 6.0 * lean * k,
                    roll: -5.0 * arc * k,
                    drop: 0.15 * lean * k.min(1.0),
                    fov: 1.0 + 0.05 * (PI * u * u).sin().max(0.0) * k,
                    hand_drop: 0.35 * lean,
                    hand_pitch: 30.0 * lean,
                    ..Pose::NONE
                }
            }
            Effect::Vault { quick: true } => {
                // La mano se apoya delante y abajo, y al pasar el cuerpo sale por abajo de la
                // vista. La cámara se ladea y sale lanzada.
                let plant = (PI * (u / 0.35).min(1.0)).sin();
                let pass = (PI * ((u - 0.35) / 0.65).clamp(0.0, 1.0)).sin();
                let lean = kick(u, 0.3);
                Pose {
                    pitch: 5.0 * lean * k,
                    roll: -4.0 * arc * k,
                    drop: 0.1 * lean * k.min(1.0),
                    fov: 1.0 + 0.06 * (PI * u * u).sin().max(0.0) * k,
                    hand_drop: 0.1 * plant + 0.7 * pass,
                    hand_pitch: 35.0 * plant + 20.0 * pass,
                    hand_forward: 0.25 * plant,
                    ..Pose::NONE
                }
            }
            Effect::ClimbStart => {
                // Despega hacia arriba: la mirada sube un momento y el FOV se abre.
                let s = kick(u, 0.2);
                Pose { pitch: -6.0 * s * k, fov: 1.0 + 0.05 * s * k, ..Pose::NONE }
            }
            Effect::TicTac { side } => {
                // Se ladea hacia donde sale, cabeza atrás y pico de FOV.
                let s = kick(u, 0.2);
                Pose {
                    pitch: -3.0 * s * k,
                    roll: 6.0 * side.clamp(-1.0, 1.0) * s * k,
                    fov: 1.0 + 0.06 * s * k,
                    hand_drop: 0.2 * s,
                    hand_pitch: 15.0 * s,
                    ..Pose::NONE
                }
            }
            Effect::Dive => {
                // En plancha: mira hacia abajo y el FOV se abre; las manos van por delante.
                let s = kick(u, 0.35);
                Pose {
                    pitch: 6.0 * s * k,
                    fov: 1.0 + 0.06 * s * k,
                    hand_drop: 0.3 * s,
                    hand_pitch: 30.0 * s,
                    ..Pose::NONE
                }
            }
        })
    }
}

/// Inclinación máxima de la cámara corriendo por una pared lateral, en grados. El motor no deja
/// pasar de 10 por `camera_shake`.
pub const WALL_TILT: f32 = 8.0;

/// La pose del wall-run. `side` va de -1 (pared a la izquierda) a 1 (a la derecha): la cámara se
/// inclina hacia fuera de la pared. `up` (0 a 1) es la subida por una pared de frente: mira hacia
/// arriba y cada pisada da un golpe corto. `step` (0 a 1) es la fase de la zancada. Las manos de
/// la subida van aparte (`climb_hand`).
pub fn wallrun_pose(side: f32, up: f32, step: f32, intensity: f32) -> Pose {
    let k = intensity.clamp(0.0, 1.5);
    let (side, up) = (side.clamp(-1.0, 1.0), up.clamp(0.0, 1.0));
    if k == 0.0 || (side == 0.0 && up == 0.0) {
        return Pose::NONE;
    }
    let bob = (2.0 * PI * step).sin();
    // El golpe de la pisada: sólo al apoyar (media zancada), no al recoger el pie.
    let stomp = bob.max(0.0);
    Pose {
        pitch: (-4.0 * up + 0.8 * bob * side.abs() + 1.5 * stomp * up) * k,
        roll: -WALL_TILT * side * k.min(1.25),
        drop: 0.04 * stomp * up * k.min(1.0),
        fov: 1.0 + (0.05 * side.abs() + 0.04 * up) * k,
        hand_drop: 0.04 * bob * side.abs(),
        ..Pose::NONE
    }
}

/// La mano `main` (o la otra) trepando por una pared de frente, con `phase` (0 a 1) del ciclo de
/// las dos manos y `amount` (0 a 1) de subida: sube y se adelanta hasta la pared, y tira hacia
/// abajo al pasar el cuerpo. Las dos manos van a contratiempo. Devuelve lo que sube (bloques), lo
/// que se adelanta (bloques) y su cabeceo (grados, positivo hacia arriba).
pub fn climb_hand(phase: f32, main: bool, amount: f32) -> (f32, f32, f32) {
    let a = amount.clamp(0.0, 1.0);
    let c = phase + if main { 0.0 } else { 0.5 };
    let w = (2.0 * PI * c).sin();
    (a * (0.05 + 0.22 * w), a * (0.06 + 0.12 * w.max(0.0)), a * (15.0 + 25.0 * w))
}

/// La pose del slide con `amount` (0 a 1) de deslizamiento: la cámara baja al suelo, se ladea un
/// poco y el FOV se abre. A diferencia de los efectos, dura lo que dura el slide.
pub fn slide_pose(amount: f32, intensity: f32) -> Pose {
    let a = amount.clamp(0.0, 1.0);
    let k = intensity.clamp(0.0, 1.5);
    if a == 0.0 || k == 0.0 {
        return Pose::NONE;
    }
    Pose {
        spin: 0.0,
        pitch: -2.0 * a * k,
        roll: 4.0 * a * k,
        drop: 0.55 * a * k.min(1.0),
        fov: 1.0 + 0.06 * a * k,
        hand_drop: 0.2 * a,
        hand_pitch: -8.0 * a,
        ..Pose::NONE
    }
}

impl Pose {
    /// Dos poses a la vez: se suman, y el FOV se multiplica.
    pub fn and(self, other: Pose) -> Pose {
        Pose {
            spin: self.spin + other.spin,
            pitch: self.pitch + other.pitch,
            roll: self.roll + other.roll,
            drop: self.drop + other.drop,
            fov: self.fov * other.fov,
            lunge: self.lunge + other.lunge,
            hand_drop: self.hand_drop + other.hand_drop,
            hand_pitch: self.hand_pitch + other.hand_pitch,
            hand_forward: self.hand_forward + other.hand_forward,
            hide_hands: self.hide_hands || other.hide_hands,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Effect; 11] = [
        Effect::Roll,
        Effect::SoftLanding { strength: 1.0 },
        Effect::HardLanding { strength: 1.0 },
        Effect::Grab,
        Effect::MantleOver,
        Effect::Vault { quick: false },
        Effect::Vault { quick: true },
        Effect::WallJump,
        Effect::ClimbStart,
        Effect::TicTac { side: -1.0 },
        Effect::Dive,
    ];

    #[test]
    fn effects_start_and_end_at_rest() {
        for effect in ALL {
            let start = effect.pose(0.0, 1.0).expect("empieza");
            let end = effect.pose(effect.duration() - 1e-4, 1.0).expect("aún dura");
            for pose in [start, end] {
                assert!(pose.pitch.abs() < 0.1 && pose.roll.abs() < 0.1, "{effect:?} {pose:?}");
                assert!(pose.drop.abs() < 0.01 && (pose.fov - 1.0).abs() < 0.001, "{effect:?} {pose:?}");
                assert!(pose.lunge.abs() < 0.01 && pose.hand_drop.abs() < 0.02 && !pose.hide_hands);
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
        assert!(full.spin > 90.0 && full.spin < 360.0);
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
    fn the_slide_pose_follows_its_amount() {
        assert_eq!(slide_pose(0.0, 1.0), Pose::NONE);
        assert_eq!(slide_pose(1.0, 0.0), Pose::NONE);
        let full = slide_pose(1.0, 1.5);
        assert!(full.drop > 0.5 && full.fov > 1.0 && full.roll.abs() <= 10.0);
        // Con un efecto encima, los giros siguen dentro del límite.
        let both = full.and(Effect::Vault { quick: false }.pose(0.2, 1.5).expect("dura"));
        assert!(both.roll.abs() <= 10.0 && both.pitch.abs() <= 10.0, "{both:?}");
    }

    #[test]
    fn the_wall_run_tilts_away_from_the_wall_within_the_limit() {
        assert_eq!(wallrun_pose(0.0, 0.0, 0.3, 1.0), Pose::NONE);
        let right = wallrun_pose(1.0, 0.0, 0.0, 1.5);
        let left = wallrun_pose(-1.0, 0.0, 0.0, 1.5);
        assert!(right.roll < 0.0 && left.roll > 0.0);
        for step in 0..10 {
            let p = wallrun_pose(1.0, 1.0, step as f32 / 10.0, 1.5);
            assert!(p.roll.abs() <= 10.0 && p.pitch.abs() <= 10.0, "{p:?}");
        }
    }

    #[test]
    fn the_roll_turns_around_the_body_and_hides_the_hands() {
        let at = |u: f32| Effect::Roll.pose(ROLL_SECS * u, 1.0).expect("dura");
        // Boca abajo a media vuelta: el ojo cerca del suelo pero por encima.
        let mid = (1..100).map(|i| at(i as f32 / 100.0)).fold(0.0f32, |m, p| m.max(p.drop));
        assert!(mid > 1.0 && EYE - mid >= 0.3, "el ojo baja hasta {}", EYE - mid);
        // A un cuarto de vuelta el ojo va por delante del cuerpo.
        let quarter = (1..100).map(|i| at(i as f32 / 100.0)).find(|p| p.spin >= 90.0).expect("gira");
        assert!(quarter.lunge > 0.2, "{quarter:?}");
        // Las manos no se ven en medio de la vuelta, y sí al principio y al final.
        assert!(at(0.5).hide_hands && at(0.3).hide_hands);
        assert!(!at(0.02).hide_hands && !at(0.97).hide_hands);
        // La vuelta gira más deprisa al principio que al final.
        let speed = |u: f32| roll_angle(u + 0.01) - roll_angle(u);
        assert!(speed(0.3) > speed(0.7));
        assert!((roll_angle(0.83) - 360.0).abs() < 0.1);
    }

    #[test]
    fn the_quick_vault_plants_the_hand_then_drops_it() {
        let at = |u: f32| Effect::Vault { quick: true }.pose(0.32 * u, 1.0).expect("dura");
        assert!(at(0.17).hand_forward > 0.2);
        assert!(at(0.67).hand_drop > 0.6);
    }

    #[test]
    fn climbing_hands_take_turns() {
        let (main_up, ..) = climb_hand(0.25, true, 1.0);
        let (off_up, ..) = climb_hand(0.25, false, 1.0);
        assert!(main_up > 0.2 && off_up < 0.0);
        assert_eq!(climb_hand(0.3, true, 0.0), (0.0, 0.0, 0.0));
    }

    #[test]
    fn harder_falls_hit_harder() {
        assert!(landing_strength(3.0) < landing_strength(8.0));
        assert_eq!(landing_strength(40.0), 1.0);
    }
}
