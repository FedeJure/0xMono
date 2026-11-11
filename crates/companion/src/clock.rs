//! Hora local. El firmware la obtendrá por NTP (WiFi) y la mantendrá con el
//! RTC del ESP32; el simulador usa la hora del PC.

pub const DAY_SECS: u32 = 86_400;
pub const DAY_MIN: u16 = 1_440;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Weekday {
    Mon = 0,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    const ALL: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];

    pub fn is_weekend(self) -> bool {
        matches!(self, Weekday::Sat | Weekday::Sun)
    }

    pub fn short_name(self) -> &'static str {
        ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"][self as usize]
    }
}

/// Un instante en hora local: días desde 1970-01-01 y segundos dentro del día.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalTime {
    pub day: u32,
    pub secs: u32,
}

impl LocalTime {
    /// `utc_offset_min`: diferencia con UTC en minutos (Argentina = -180).
    pub fn from_unix(unix_secs: u64, utc_offset_min: i16) -> Self {
        let local = (unix_secs as i64 + utc_offset_min as i64 * 60).max(0) as u64;
        Self::from_total_secs((local.min(u32::MAX as u64)) as u32)
    }

    pub fn new(day: u32, hour: u8, minute: u8) -> Self {
        Self {
            day,
            secs: hour as u32 * 3600 + minute as u32 * 60,
        }
    }

    pub fn from_total_secs(total: u32) -> Self {
        Self {
            day: total / DAY_SECS,
            secs: total % DAY_SECS,
        }
    }

    /// Segundos desde 1970 en hora local (alcanza hasta el año 2106).
    pub fn total_secs(&self) -> u32 {
        self.day * DAY_SECS + self.secs
    }

    pub fn add_secs(&self, s: u32) -> Self {
        Self::from_total_secs(self.total_secs() + s)
    }

    /// Minuto del día (0..1440).
    pub fn minute(&self) -> u16 {
        (self.secs / 60) as u16
    }

    pub fn hour(&self) -> u8 {
        (self.secs / 3600) as u8
    }

    pub fn weekday(&self) -> Weekday {
        weekday(self.day)
    }
}

/// El 1970-01-01 fue jueves.
pub fn weekday(day: u32) -> Weekday {
    Weekday::ALL[((day + 3) % 7) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_to_local() {
        // 2026-10-01 15:30:00 UTC = 12:30 en Argentina (UTC-3), jueves.
        let t = LocalTime::from_unix(1_790_868_600, -180);
        assert_eq!((t.hour(), t.minute() % 60), (12, 30));
        assert_eq!(t.weekday(), Weekday::Thu);
    }

    #[test]
    fn offset_can_change_the_day() {
        // 01:00 UTC del viernes = 22:00 del jueves en Argentina.
        let utc = LocalTime::new(20_000, 1, 0);
        let ar = LocalTime::from_unix(utc.total_secs() as u64, -180);
        assert_eq!(ar.day, 19_999);
        assert_eq!(ar.hour(), 22);
    }
}
