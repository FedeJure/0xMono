//! Husos horarios para elegir en la página. Son diferencias fijas con UTC: si
//! en tu país hay horario de verano, se cambia a mano dos veces por año (en
//! Argentina no hay).

use core::fmt;

pub struct Zone {
    /// Minutos respecto de UTC.
    pub offset_min: i16,
    pub places: &'static str,
}

const fn z(offset_min: i16, places: &'static str) -> Zone {
    Zone { offset_min, places }
}

/// De oeste a este. Argentina va por defecto.
pub const ZONES: &[Zone] = &[
    z(-600, "Hawái"),
    z(-480, "Los Ángeles, Vancouver"),
    z(-420, "Denver, Phoenix"),
    z(-360, "México, Centroamérica"),
    z(
        -300,
        "Colombia, Perú, Ecuador, Panamá, Nueva York (invierno)",
    ),
    z(
        -240,
        "Venezuela, Bolivia, Chile (invierno), Nueva York (verano)",
    ),
    z(
        -180,
        "Argentina, Uruguay, Paraguay, Brasil (Brasilia), Chile (verano)",
    ),
    z(-120, "Fernando de Noronha"),
    z(-60, "Azores, Cabo Verde"),
    z(0, "Portugal, Reino Unido, Canarias (invierno)"),
    z(60, "España, Italia, Francia, Alemania (invierno)"),
    z(
        120,
        "España, Italia, Francia, Alemania (verano), Grecia (invierno)",
    ),
    z(180, "Moscú, Estambul, Grecia (verano)"),
    z(240, "Dubái"),
    z(330, "India"),
    z(420, "Tailandia, Vietnam"),
    z(480, "China, Singapur, Perth"),
    z(540, "Japón, Corea"),
    z(600, "Sídney (invierno)"),
    z(660, "Sídney (verano)"),
    z(780, "Nueva Zelanda (verano)"),
];

/// `UTC-3`, `UTC+5:30`, `UTC+0`.
pub struct Offset(pub i16);

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { '-' } else { '+' };
        let m = self.0.unsigned_abs();
        write!(f, "UTC{sign}{}", m / 60)?;
        if !m.is_multiple_of(60) {
            write!(f, ":{:02}", m % 60)?;
        }
        Ok(())
    }
}

/// Lee el valor del selector (minutos). Solo se aceptan diferencias reales.
pub fn parse_offset(s: &str) -> Option<i16> {
    let v: i16 = s.trim().parse().ok()?;
    ((-12 * 60..=14 * 60).contains(&v) && v % 15 == 0).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_read_and_print() {
        assert_eq!(std::format!("{}", Offset(-180)), "UTC-3");
        assert_eq!(std::format!("{}", Offset(330)), "UTC+5:30");
        assert_eq!(std::format!("{}", Offset(0)), "UTC+0");
        assert_eq!(parse_offset("-180"), Some(-180));
        assert_eq!(parse_offset("330"), Some(330));
        assert_eq!(parse_offset("7"), None);
        assert_eq!(parse_offset("-9999"), None);
        assert_eq!(parse_offset("x"), None);
    }

    #[test]
    fn zones_are_sorted_unique_and_include_argentina() {
        assert!(ZONES.windows(2).all(|w| w[0].offset_min < w[1].offset_min));
        assert!(
            ZONES
                .iter()
                .any(|z| z.offset_min == crate::DEFAULT_UTC_OFFSET_MIN
                    && z.places.starts_with("Argentina"))
        );
        assert!(
            ZONES
                .iter()
                .all(|z| parse_offset(&std::format!("{}", z.offset_min)).is_some())
        );
    }
}
