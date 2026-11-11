//! La configuración del 0xMono: lo que se elige desde el celular (el WiFi y
//! cómo se pone en hora), cómo se guarda en la flash, la página que lo
//! edita, la hora por internet (SNTP) y lo que un arranque le deja dicho al
//! siguiente ([`handoff`]).
//!
//! Sin hardware ni memoria dinámica: corre igual en el simulador (con tests)
//! y en el ESP32-C3.
#![cfg_attr(not(test), no_std)]

pub mod date;
pub mod handoff;
pub mod page;
pub mod sntp;
pub mod tz;

pub use captive::{Pass, Ssid};

/// Argentina: UTC−3, sin horario de verano.
pub const DEFAULT_UTC_OFFSET_MIN: i16 = -180;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Vacío: sin WiFi.
    pub ssid: Ssid,
    /// Vacía para redes abiertas.
    pub pass: Pass,
    /// Ponerse en hora por internet al prender (si hay WiFi).
    pub auto_time: bool,
    /// Diferencia con UTC en minutos (Argentina = -180).
    pub utc_offset_min: i16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ssid: Ssid::new(),
            pass: Pass::new(),
            auto_time: true,
            utc_offset_min: DEFAULT_UTC_OFFSET_MIN,
        }
    }
}

const MAGIC: &[u8; 4] = b"0xMS";
const VERSION: u8 = 1;

/// Lugar que ocupa el registro en la flash (lo que sobra queda en `0xFF`).
pub const RECORD_LEN: usize = 128;

impl Settings {
    pub fn has_wifi(&self) -> bool {
        !self.ssid.is_empty()
    }

    /// Si al prender tiene que ir a buscar la hora a internet.
    pub fn wants_sync(&self) -> bool {
        self.has_wifi() && self.auto_time
    }

    /// El registro: `0xMS`, versión, opciones, diferencia horaria, red y
    /// clave como (largo, bytes) y un CRC32 al final. La flash borrada se lee
    /// como `0xFF`, así que un aparato nuevo (o un guardado cortado a la
    /// mitad) no pasa la verificación y arranca con lo de fábrica.
    pub fn encode(&self) -> [u8; RECORD_LEN] {
        let mut out = [0xFF; RECORD_LEN];
        let mut n = 0;
        let mut put = |bytes: &[u8]| {
            out[n..n + bytes.len()].copy_from_slice(bytes);
            n += bytes.len();
        };
        put(MAGIC);
        put(&[VERSION, self.auto_time as u8]);
        put(&self.utc_offset_min.to_le_bytes());
        for s in [self.ssid.as_str(), self.pass.as_str()] {
            put(&[s.len() as u8]);
            put(s.as_bytes());
        }
        let crc = captive::crc32(&out[..n]);
        out[n..n + 4].copy_from_slice(&crc.to_le_bytes());
        out
    }

    /// Lee un registro. Cuando cambie el formato se sube `VERSION` y se
    /// agrega un brazo: los registros viejos se siguen leyendo.
    pub fn decode(data: &[u8]) -> Option<Settings> {
        let mut r = Reader { data, pos: 0 };
        if r.take(4)? != MAGIC {
            return None;
        }
        match r.take(1)?[0] {
            1 => {
                let flags = r.take(1)?[0];
                let off = r.take(2)?;
                let utc_offset_min = i16::from_le_bytes([off[0], off[1]]);
                let ssid = r.text()?;
                let pass = r.text()?;
                let end = r.pos;
                let crc = r.take(4)?;
                let crc = u32::from_le_bytes([crc[0], crc[1], crc[2], crc[3]]);
                (crc == captive::crc32(&data[..end])
                    && (-14 * 60..=14 * 60).contains(&utc_offset_min))
                .then_some(Settings {
                    ssid,
                    pass,
                    auto_time: flags & 1 != 0,
                    utc_offset_min,
                })
            }
            _ => None,
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.data.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(s)
    }

    fn text<const N: usize>(&mut self) -> Option<heapless::String<N>> {
        let len = self.take(1)?[0] as usize;
        captive::text(core::str::from_utf8(self.take(len)?).ok()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use captive::text;

    fn sample() -> Settings {
        Settings {
            ssid: text("WiFi de casa").unwrap(),
            pass: text("ñandú-2026!").unwrap(),
            auto_time: false,
            utc_offset_min: -240,
        }
    }

    #[test]
    fn round_trip_and_defaults() {
        let s = sample();
        assert_eq!(Settings::decode(&s.encode()), Some(s));
        let d = Settings::default();
        assert_eq!(Settings::decode(&d.encode()), Some(d.clone()));
        assert!(!d.has_wifi() && !d.wants_sync());
        assert!(sample().has_wifi() && !sample().wants_sync());
    }

    #[test]
    fn longest_fields_fit() {
        let mut s = sample();
        s.ssid = text(&"s".repeat(32)).unwrap();
        s.pass = text(&"p".repeat(64)).unwrap();
        assert_eq!(Settings::decode(&s.encode()), Some(s));
    }

    #[test]
    fn erased_flash_and_corruption_are_rejected() {
        assert_eq!(Settings::decode(&[0xFF; RECORD_LEN]), None);
        assert_eq!(Settings::decode(&[0x00; RECORD_LEN]), None);
        let buf = sample().encode();
        let used = 4 + 2 + 2 + 1 + 12 + 1 + "ñandú-2026!".len() + 4;
        for i in 0..used {
            let mut bad = buf;
            bad[i] ^= 0x10;
            assert_eq!(Settings::decode(&bad), None, "byte {i}");
        }
        assert_eq!(Settings::decode(&buf[..used - 1]), None);
    }
}
