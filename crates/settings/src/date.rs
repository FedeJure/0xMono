//! Fechas del calendario: de un día contado desde 1970 al año, mes y día y
//! al revés, y leer o escribir la fecha y la hora como las muestra el
//! celular (`2026-11-11`, `14:32`).
//!
//! [`companion::LocalTime`] cuenta días desde el 1970-01-01 (que fue jueves),
//! así que el día de la semana sale solo y coincide con el calendario real.

use core::fmt;

use companion::LocalTime;

/// Años que se aceptan al poner la hora a mano.
pub const YEARS: core::ops::RangeInclusive<i32> = 2024..=2099;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    /// 1..=12
    pub month: u8,
    /// 1..=31
    pub day: u8,
}

/// Días desde el 1970-01-01 (algoritmo de Howard Hinnant, calendario
/// gregoriano proléptico).
pub fn days_from_civil(d: Date) -> i64 {
    let y = d.year as i64 - (d.month <= 2) as i64;
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = d.month as i64;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d.day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(days: i64) -> Date {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + (month <= 2) as i64) as i32;
    Date { year, month, day }
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i32, m: u8) -> u8 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// `2026-11-11` (lo que manda un `<input type=date>`). También acepta
/// `11/11/2026`, por si el celular no tiene selector de fecha.
pub fn parse_date(s: &str) -> Option<Date> {
    let s = s.trim();
    let mut parts = [0i32; 3];
    let (sep, order) = if s.contains('-') {
        ('-', [0, 1, 2])
    } else {
        ('/', [2, 1, 0])
    };
    let mut n = 0;
    for p in s.split(sep) {
        if n == 3 || p.is_empty() || p.len() > 4 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        parts[n] = p.parse().ok()?;
        n += 1;
    }
    if n != 3 {
        return None;
    }
    let (year, month, day) = (parts[order[0]], parts[order[1]], parts[order[2]]);
    if !YEARS.contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    let month = month as u8;
    if day < 1 || day > days_in_month(year, month) as i32 {
        return None;
    }
    Some(Date {
        year,
        month,
        day: day as u8,
    })
}

/// `14:32` o `14:32:05` → segundos del día.
pub fn parse_time(s: &str) -> Option<u32> {
    let mut parts = s.trim().split(':');
    let mut field = |max: u32| -> Option<u32> {
        let p = parts.next()?;
        if p.is_empty() || p.len() > 2 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let v: u32 = p.parse().ok()?;
        (v <= max).then_some(v)
    };
    let h = field(23)?;
    let m = field(59)?;
    let s = field(59).unwrap_or(0);
    parts.next().is_none().then_some(h * 3600 + m * 60 + s)
}

/// La hora local de una fecha y una hora escritas a mano.
pub fn local_time(date: &str, time: &str) -> Option<LocalTime> {
    let days = days_from_civil(parse_date(date)?);
    Some(LocalTime {
        day: u32::try_from(days).ok()?,
        secs: parse_time(time)?,
    })
}

pub fn date_of(t: LocalTime) -> Date {
    civil_from_days(t.day as i64)
}

/// `mié 11/11/2026`
pub struct DayLabel(pub LocalTime);

impl fmt::Display for DayLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = date_of(self.0);
        write!(
            f,
            "{} {:02}/{:02}/{}",
            self.0.weekday().short_name(),
            d.day,
            d.month,
            d.year
        )
    }
}

/// `14:32`
pub struct Clock(pub LocalTime);

impl fmt::Display for Clock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.0.hour(), self.0.minute() % 60)
    }
}

/// Para precargar los campos de la página: `2026-11-11` y `14:32`.
pub struct IsoDate(pub LocalTime);

impl fmt::Display for IsoDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = date_of(self.0);
        write!(f, "{}-{:02}-{:02}", d.year, d.month, d.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use companion::clock::Weekday;

    #[test]
    fn known_days() {
        let epoch = Date {
            year: 1970,
            month: 1,
            day: 1,
        };
        assert_eq!(days_from_civil(epoch), 0);
        // 2026-11-11 fue miércoles.
        let d = Date {
            year: 2026,
            month: 11,
            day: 11,
        };
        let days = days_from_civil(d);
        assert_eq!(days, 20_768);
        assert_eq!(civil_from_days(days), d);
        assert_eq!(companion::clock::weekday(days as u32), Weekday::Wed);
        // 2028-02-29 existe; 2027-02-29 no.
        assert!(parse_date("2028-02-29").is_some());
        assert!(parse_date("2027-02-29").is_none());
    }

    #[test]
    fn round_trip_every_day_for_a_century() {
        let mut last = days_from_civil(Date {
            year: 1999,
            month: 12,
            day: 31,
        });
        for days in 10_957..10_957 + 36_600 {
            let d = civil_from_days(days);
            assert_eq!(days_from_civil(d), days);
            assert_eq!(days, last + 1);
            last = days;
        }
    }

    #[test]
    fn weekdays_match_the_real_calendar() {
        // Sábado 2026-11-14 y domingo 2026-11-15: la agenda del mono cambia.
        let sat = local_time("2026-11-14", "10:00").unwrap();
        let sun = local_time("15/11/2026", "10:00").unwrap();
        assert_eq!(sat.weekday(), Weekday::Sat);
        assert_eq!(sun.weekday(), Weekday::Sun);
        // Y con la hora de internet: 2026-11-16 02:30 UTC es domingo 23:30 en Argentina.
        let unix = days_from_civil(Date {
            year: 2026,
            month: 11,
            day: 16,
        }) as u64
            * 86_400
            + 2 * 3600
            + 30 * 60;
        let ar = LocalTime::from_unix(unix, -180);
        assert_eq!(ar.weekday(), Weekday::Sun);
        assert_eq!(
            std::format!("{} {}", DayLabel(ar), Clock(ar)),
            "dom 15/11/2026 23:30"
        );
    }

    #[test]
    fn parses_what_phones_send() {
        assert_eq!(parse_time("14:32"), Some(14 * 3600 + 32 * 60));
        assert_eq!(parse_time("07:05:09"), Some(7 * 3600 + 5 * 60 + 9));
        for bad in ["", "24:00", "12:60", "12", "1:2:3:4", "aa:bb", "-1:00"] {
            assert_eq!(parse_time(bad), None, "{bad}");
        }
        for bad in [
            "",
            "2026-13-01",
            "2026-00-10",
            "2026-11-31",
            "1999-01-01",
            "2026-11",
            "x",
        ] {
            assert_eq!(parse_date(bad), None, "{bad}");
        }
        let t = local_time("2026-11-11", "14:32").unwrap();
        assert_eq!(std::format!("{}", IsoDate(t)), "2026-11-11");
        assert_eq!(std::format!("{}", DayLabel(t)), "mié 11/11/2026");
    }
}
