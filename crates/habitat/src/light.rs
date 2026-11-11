//! La luz según la hora: el color del cielo, un tinte para toda la escena
//! (rosado al amanecer, dorado al atardecer, azul de noche), la luz
//! ambiente y de dónde viene el sol (de la izquierda a la mañana, de la
//! derecha a la tarde).
//!
//! De noche, adentro y despierto, prende las lámparas: luz cálida y las
//! pantallas de las lámparas encendidas. Dormido quedan apagadas y solo
//! entra la luz azul de la luna.
//!
//! Cambia de a pasos de unos minutos: cada paso retiñe los materiales y
//! redibuja todo (ver `Habitat::render`).

use engine3d::Rgb;
use libm::sqrtf;

/// Cómo se ve la escena en un momento.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub sky_top: Rgb,
    pub sky_low: Rgb,
    /// Multiplica el color de cada material (255 = sin cambio).
    pub tint: Rgb,
    /// Luz ambiente, Q16.
    pub ambient: i32,
    /// Dirección hacia la luz (Q14, espacio de vista).
    pub dir: [i32; 3],
    /// Las lámparas encendidas.
    pub lamps: bool,
}

/// (hora, cielo arriba, cielo en el horizonte, tinte, ambiente).
type Key = (f32, Rgb, Rgb, Rgb, f32);

const NIGHT: (Rgb, Rgb, Rgb, f32) = (Rgb(10, 14, 40), Rgb(34, 38, 78), Rgb(96, 112, 170), 9_500.0);

const KEYS: [Key; 10] = [
    (0.0, NIGHT.0, NIGHT.1, NIGHT.2, NIGHT.3),
    (5.4, NIGHT.0, NIGHT.1, NIGHT.2, NIGHT.3),
    // Amanecer: horizonte anaranjado, todo rosado.
    (
        6.6,
        Rgb(72, 92, 164),
        Rgb(250, 168, 120),
        Rgb(255, 208, 186),
        14_500.0,
    ),
    (
        8.2,
        Rgb(110, 170, 230),
        Rgb(214, 232, 244),
        Rgb(255, 246, 232),
        20_000.0,
    ),
    (
        12.5,
        Rgb(120, 178, 232),
        Rgb(210, 232, 244),
        Rgb(255, 255, 255),
        22_000.0,
    ),
    (
        17.0,
        Rgb(112, 166, 226),
        Rgb(232, 226, 204),
        Rgb(255, 242, 220),
        21_000.0,
    ),
    // Hora dorada y puesta del sol.
    (
        18.8,
        Rgb(92, 112, 184),
        Rgb(255, 162, 92),
        Rgb(255, 214, 168),
        17_500.0,
    ),
    // Crepúsculo: violeta.
    (
        19.9,
        Rgb(42, 42, 104),
        Rgb(192, 104, 124),
        Rgb(198, 164, 206),
        12_500.0,
    ),
    (21.0, NIGHT.0, NIGHT.1, NIGHT.2, NIGHT.3),
    (24.0, NIGHT.0, NIGHT.1, NIGHT.2, NIGHT.3),
];

/// Luz de las lámparas (cálida).
const LAMP: Rgb = Rgb(255, 212, 158);

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t + 0.5) as u8;
    Rgb(f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

/// 0 = noche cerrada, 1 = pleno día.
pub fn daylight(minute: f32) -> f32 {
    let h = minute / 60.0;
    ((h - 5.8) / 2.2)
        .clamp(0.0, 1.0)
        .min(((21.0 - h) / 2.0).clamp(0.0, 1.0))
}

fn unit(v: [f32; 3]) -> [i32; 3] {
    let l = sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).max(1e-6);
    v.map(|c| (c / l * 16384.0) as i32)
}

/// La luz a la hora `minute` (0..1440). `outdoor`: la cámara está en el
/// jardín. `awake`: si está despierto (de noche, adentro, prende las
/// lámparas).
pub fn look(minute: f32, outdoor: bool, awake: bool) -> Look {
    let h = libm::fmodf(minute / 60.0, 24.0);
    let h = if h < 0.0 { h + 24.0 } else { h };
    let k = KEYS
        .windows(2)
        .position(|w| h < w[1].0)
        .unwrap_or(KEYS.len() - 2);
    let (a, b) = (KEYS[k], KEYS[k + 1]);
    let t = (h - a.0) / (b.0 - a.0);
    let sky_top = mix(a.1, b.1, t);
    let sky_low = mix(a.2, b.2, t);
    let mut tint = mix(a.3, b.3, t);
    let mut ambient = a.4 + (b.4 - a.4) * t;
    // El sol cruza de izquierda a derecha y está más alto al mediodía.
    let day = daylight(minute);
    let across = ((h - 6.0) / 15.0).clamp(0.0, 1.0) * 2.0 - 1.0;
    let mut dir = unit([0.75 * across - 0.1, 0.45 + 0.4 * day, -0.55]);
    let dark = 1.0 - day;
    let lamps = !outdoor && awake && dark > 0.25;
    if lamps {
        // Adentro, con las lámparas: luz cálida desde arriba.
        tint = mix(tint, LAMP, dark);
        ambient = ambient + (17_000.0 - ambient) * dark;
        dir = unit([-0.3, 0.85, -0.45]);
    }
    Look {
        sky_top,
        sky_low,
        tint,
        ambient: ambient as i32,
        dir,
        lamps,
    }
}

/// El color `c` bajo el tinte `tint`.
pub fn tinted(c: Rgb, tint: Rgb) -> Rgb {
    let f = |x: u8, t: u8| (x as u32 * t as u32 / 255) as u8;
    Rgb(f(c.0, tint.0), f(c.1, tint.1), f(c.2, tint.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noon_is_neutral_and_night_is_blue_and_dark() {
        let noon = look(12.5 * 60.0, true, true);
        assert_eq!(noon.tint, Rgb(255, 255, 255));
        let night = look(2.0 * 60.0, true, true);
        assert!(night.tint.2 > night.tint.0 && night.ambient < noon.ambient);
        assert!(!night.lamps, "afuera no hay lámparas");
    }

    #[test]
    fn sunset_is_warm_and_lamps_light_the_house_at_night() {
        let sunset = look(18.8 * 60.0, true, true);
        assert!(sunset.tint.0 > sunset.tint.2 + 60, "{sunset:?}");
        let home = look(23.0 * 60.0, false, true);
        assert!(home.lamps && home.tint.0 > home.tint.2, "{home:?}");
        let asleep = look(23.0 * 60.0, false, false);
        assert!(!asleep.lamps && asleep.tint.2 > asleep.tint.0);
    }

    #[test]
    fn the_sun_moves_from_left_to_right() {
        let morning = look(8.0 * 60.0, true, true).dir[0];
        let evening = look(18.0 * 60.0, true, true).dir[0];
        assert!(morning < 0 && evening > 0);
    }

    #[test]
    fn the_day_wraps_without_jumps() {
        let mut last = look(0.0, true, true);
        for m in 1..1440 {
            let l = look(m as f32, true, true);
            let d = |a: Rgb, b: Rgb| {
                (a.0 as i32 - b.0 as i32)
                    .abs()
                    .max((a.2 as i32 - b.2 as i32).abs())
            };
            assert!(d(l.sky_low, last.sky_low) <= 12, "salto a los {m} min");
            last = l;
        }
    }
}
