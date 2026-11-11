//! Lo que se guarda en flash. Es poco: la agenda se recalcula, así que basta
//! con la semilla del personaje, cuándo te vio por última vez y la hora en
//! que se quedó (sin batería el reloj se frena al desenchufar).
//!
//! El formato lleva versión para que una actualización por WiFi nunca borre
//! al personaje: cuando cambie, se sube `VERSION`, se agrega un brazo nuevo en
//! `decode` y los brazos viejos siguen leyendo (y convirtiendo) lo anterior.
//!
//! [`Journal`] lo escribe sin riesgo aunque se corte la corriente: cada
//! guardado es un registro nuevo y el anterior no se toca.

use crate::clock::LocalTime;

pub const SAVE_LEN: usize = 16;
const MAGIC: [u8; 2] = *b"CP";
const VERSION: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveData {
    pub seed: u32,
    pub last_seen: Option<LocalTime>,
    /// La hora en que se guardó: de ahí sigue el reloj al volver a enchufar.
    pub clock: Option<LocalTime>,
}

pub fn encode(s: &SaveData) -> [u8; SAVE_LEN] {
    let mut b = [0u8; SAVE_LEN];
    b[0..2].copy_from_slice(&MAGIC);
    b[2] = VERSION;
    b[3..7].copy_from_slice(&s.seed.to_le_bytes());
    // 0 = nunca (1970 no es una fecha posible para una visita).
    let seen = s.last_seen.map_or(0, |t| t.total_secs());
    b[7..11].copy_from_slice(&seen.to_le_bytes());
    let clock = s.clock.map_or(0, |t| t.total_secs());
    b[11..15].copy_from_slice(&clock.to_le_bytes());
    b[15] = checksum(&b[..15]);
    b
}

pub fn decode(b: &[u8]) -> Option<SaveData> {
    if b.len() < SAVE_LEN || b[0..2] != MAGIC || b[15] != checksum(&b[..15]) {
        return None;
    }
    let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let time_at = |i: usize| {
        let secs = u32_at(i);
        (secs != 0).then(|| LocalTime::from_total_secs(secs))
    };
    match b[2] {
        1 => Some(SaveData {
            seed: u32_at(3),
            last_seen: time_at(7),
            clock: None,
        }),
        2 => Some(SaveData {
            seed: u32_at(3),
            last_seen: time_at(7),
            clock: time_at(11),
        }),
        _ => None,
    }
}

fn checksum(data: &[u8]) -> u8 {
    data.iter().fold(0x5Au8, |acc, &x| acc.rotate_left(1) ^ x)
}

/// Lo poco que [`Journal`] necesita de la flash. Los desplazamientos cuentan
/// desde el principio de la zona de guardado, de `2 * SECTOR_LEN` bytes.
pub trait Flash {
    type Error;
    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error>;
    /// Escribe sobre bytes borrados, sin borrar antes.
    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error>;
    /// Deja el sector en `0xFF`.
    fn erase_sector(&mut self, offset: u32) -> Result<(), Self::Error>;
}

pub const SECTOR_LEN: u32 = 4096;
/// Número de guardado, datos y CRC. Múltiplo de 4: la flash escribe de a 4.
const RECORD_LEN: usize = 24;
const SLOTS: u32 = SECTOR_LEN / RECORD_LEN as u32;
const SECTORS: u32 = 2;
/// Si un registro no se puede volver a leer bien, cuántos lugares más prueba.
const TRIES: u32 = 3;

/// El guardado en dos sectores: cada registro va en el lugar libre que sigue
/// y, cuando un sector se llena, se borra el otro y se sigue ahí.
///
/// - **Corte de corriente:** un registro a medio escribir no pasa el CRC y se
///   lee el anterior. Nunca se borra el sector que tiene el último registro.
/// - **Desgaste:** un sector aguanta unos 100 000 borrados y acá se borra uno
///   cada 170 guardados, así que guardar cada 5 minutos dura siglos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Journal {
    sector: u32,
    slot: u32,
    /// Número del próximo guardado.
    seq: u32,
}

impl Journal {
    /// Recorre la zona y devuelve el último registro bueno, si hay.
    pub fn mount<F: Flash>(flash: &mut F) -> (Self, Option<SaveData>) {
        // (número, sector, lugar, datos)
        let mut best: Option<(u32, u32, u32, SaveData)> = None;
        for sector in 0..SECTORS {
            for slot in 0..SLOTS {
                let Some(raw) = read_record(flash, sector, slot) else {
                    continue;
                };
                if is_blank(&raw) {
                    break; // Se escribe en orden: lo que sigue también está libre.
                }
                if let Some((seq, data)) = decode_record(&raw)
                    && best.is_none_or(|(b, ..)| seq > b)
                {
                    best = Some((seq, sector, slot, data));
                }
            }
        }
        let (seq, sector, first) = best.map_or((0, 0, 0), |(s, sec, slot, _)| (s + 1, sec, slot + 1));
        // Después del último bueno puede haber restos de un corte: saltearlos.
        let slot = (first..SLOTS)
            .find(|&slot| read_record(flash, sector, slot).is_some_and(|r| is_blank(&r)))
            .unwrap_or(SLOTS);
        (Self { sector, slot, seq }, best.map(|(.., data)| data))
    }

    /// Guarda y comprueba leyendo. Devuelve `false` si no pudo.
    pub fn store<F: Flash>(&mut self, flash: &mut F, data: &SaveData) -> bool {
        let raw = encode_record(self.seq, data);
        for _ in 0..TRIES {
            if self.slot == SLOTS {
                // El sector lleno sigue guardando lo último hasta que entre
                // el primer registro en el otro.
                let other = (self.sector + 1) % SECTORS;
                if flash.erase_sector(other * SECTOR_LEN).is_err() {
                    return false;
                }
                self.sector = other;
                self.slot = 0;
            }
            let offset = offset_of(self.sector, self.slot);
            self.slot += 1;
            if flash.write(offset, &raw).is_ok()
                && read_record(flash, self.sector, self.slot - 1) == Some(raw)
            {
                self.seq += 1;
                return true;
            }
        }
        false
    }
}

fn offset_of(sector: u32, slot: u32) -> u32 {
    sector * SECTOR_LEN + slot * RECORD_LEN as u32
}

fn read_record<F: Flash>(flash: &mut F, sector: u32, slot: u32) -> Option<[u8; RECORD_LEN]> {
    let mut raw = [0u8; RECORD_LEN];
    flash.read(offset_of(sector, slot), &mut raw).ok()?;
    Some(raw)
}

fn is_blank(raw: &[u8; RECORD_LEN]) -> bool {
    raw.iter().all(|&b| b == 0xFF)
}

fn encode_record(seq: u32, data: &SaveData) -> [u8; RECORD_LEN] {
    let mut raw = [0u8; RECORD_LEN];
    raw[..4].copy_from_slice(&seq.to_le_bytes());
    raw[4..20].copy_from_slice(&encode(data));
    let crc = crc32(&raw[..20]);
    raw[20..].copy_from_slice(&crc.to_le_bytes());
    raw
}

fn decode_record(raw: &[u8; RECORD_LEN]) -> Option<(u32, SaveData)> {
    let crc = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);
    if crc != crc32(&raw[..20]) {
        return None;
    }
    let seq = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    Some((seq, decode(&raw[4..20])?))
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Una flash NOR de mentira: escribir solo baja bits, y se le puede
    /// cortar la corriente después de tocar una cantidad de bytes.
    struct FakeFlash {
        mem: [u8; (SECTORS * SECTOR_LEN) as usize],
        /// Bytes que faltan tocar hasta el corte.
        power: Option<usize>,
        erases: [u32; SECTORS as usize],
    }

    #[derive(Debug)]
    struct PowerCut;

    impl FakeFlash {
        fn new() -> Self {
            Self {
                mem: [0xFF; (SECTORS * SECTOR_LEN) as usize],
                power: None,
                erases: [0; SECTORS as usize],
            }
        }

        fn spend(&mut self) -> Result<(), PowerCut> {
            match &mut self.power {
                Some(0) => Err(PowerCut),
                Some(left) => {
                    *left -= 1;
                    Ok(())
                }
                None => Ok(()),
            }
        }
    }

    impl Flash for FakeFlash {
        type Error = PowerCut;

        fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), PowerCut> {
            let start = offset as usize;
            bytes.copy_from_slice(&self.mem[start..start + bytes.len()]);
            Ok(())
        }

        fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), PowerCut> {
            for (i, b) in bytes.iter().enumerate() {
                self.spend()?;
                self.mem[offset as usize + i] &= b;
            }
            Ok(())
        }

        fn erase_sector(&mut self, offset: u32) -> Result<(), PowerCut> {
            self.erases[(offset / SECTOR_LEN) as usize] += 1;
            for i in 0..SECTOR_LEN as usize {
                self.spend()?;
                self.mem[offset as usize + i] = 0xFF;
            }
            Ok(())
        }
    }

    fn data(n: u32) -> SaveData {
        SaveData {
            seed: 0xCAFE,
            last_seen: Some(LocalTime::from_total_secs(100_000 + n)),
            clock: Some(LocalTime::from_total_secs(200_000 + n)),
        }
    }

    #[test]
    fn blank_flash_has_nothing_and_first_save_comes_back() {
        let mut f = FakeFlash::new();
        let (mut j, found) = Journal::mount(&mut f);
        assert_eq!(found, None);
        assert!(j.store(&mut f, &data(1)));
        assert_eq!(Journal::mount(&mut f).1, Some(data(1)));
    }

    #[test]
    fn keeps_the_last_save_across_many_sectors_and_reboots() {
        let mut f = FakeFlash::new();
        for n in 0..3 * SLOTS * SECTORS {
            let (mut j, _) = Journal::mount(&mut f);
            assert!(j.store(&mut f, &data(n)));
            assert_eq!(Journal::mount(&mut f).1, Some(data(n)));
        }
    }

    /// El corte puede caer en cualquier byte de una escritura o un borrado,
    /// incluido el borrado del otro sector cuando se llena uno.
    #[test]
    fn a_power_cut_at_any_byte_never_loses_the_previous_save() {
        // Dejarlo a pocos registros de llenar el sector.
        let mut full = FakeFlash::new();
        let (mut j, _) = Journal::mount(&mut full);
        for n in 0..SLOTS - 2 {
            assert!(j.store(&mut full, &data(n)));
        }
        for saves in [1u32, 2, 3] {
            let spent = saves as usize * RECORD_LEN + SECTOR_LEN as usize;
            for cut in (0..spent).step_by(7).chain(0..(2 * RECORD_LEN)) {
                let mut f = FakeFlash {
                    mem: full.mem,
                    power: Some(cut),
                    erases: [0; SECTORS as usize],
                };
                let (mut j, before) = Journal::mount(&mut f);
                assert_eq!(before, Some(data(SLOTS - 3)));
                let mut last = before;
                for k in 0..saves {
                    let next = data(SLOTS + k);
                    if j.store(&mut f, &next) {
                        last = Some(next);
                    }
                }
                // Vuelve la corriente: lo último confirmado sigue ahí.
                f.power = None;
                let (mut j, found) = Journal::mount(&mut f);
                assert_eq!(found, last, "corte en el byte {cut}");
                // Y se puede seguir guardando.
                assert!(j.store(&mut f, &data(5000)));
                assert_eq!(Journal::mount(&mut f).1, Some(data(5000)));
            }
        }
    }

    #[test]
    fn a_ruined_record_falls_back_to_the_one_before() {
        let mut f = FakeFlash::new();
        let (mut j, _) = Journal::mount(&mut f);
        assert!(j.store(&mut f, &data(1)));
        assert!(j.store(&mut f, &data(2)));
        f.mem[RECORD_LEN + 6] ^= 0x10;
        let (mut j, found) = Journal::mount(&mut f);
        assert_eq!(found, Some(data(1)));
        assert!(j.store(&mut f, &data(3)));
        assert_eq!(Journal::mount(&mut f).1, Some(data(3)));
    }

    #[test]
    fn sectors_wear_evenly_and_slowly() {
        let mut f = FakeFlash::new();
        let (mut j, _) = Journal::mount(&mut f);
        let saves = 100 * SLOTS;
        for n in 0..saves {
            assert!(j.store(&mut f, &data(n)));
        }
        let [a, b] = f.erases;
        assert!(a.abs_diff(b) <= 1, "{a} y {b}");
        assert!(a + b <= saves / SLOTS);
    }

    #[test]
    fn reads_saves_from_version_1() {
        let mut b = encode(&SaveData {
            seed: 7,
            last_seen: Some(LocalTime::new(20_000, 9, 0)),
            clock: None,
        });
        b[2] = 1;
        b[15] = checksum(&b[..15]);
        let s = decode(&b).expect("versión 1");
        assert_eq!(s.seed, 7);
        assert_eq!(s.clock, None);
    }

    #[test]
    fn roundtrip() {
        for s in [
            SaveData {
                seed: 0xDEAD_BEEF,
                last_seen: Some(LocalTime::new(20_000, 21, 15)),
                clock: Some(LocalTime::new(20_001, 8, 30)),
            },
            SaveData {
                seed: 1,
                last_seen: None,
                clock: None,
            },
        ] {
            assert_eq!(decode(&encode(&s)), Some(s));
        }
    }

    #[test]
    fn rejects_corruption_and_blank_flash() {
        let mut b = encode(&SaveData {
            seed: 42,
            last_seen: None,
            clock: None,
        });
        b[4] ^= 1;
        assert_eq!(decode(&b), None);
        assert_eq!(decode(&[0xFF; SAVE_LEN]), None);
    }
}
