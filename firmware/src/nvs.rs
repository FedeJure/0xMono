//! La partición `nvs` de la flash, por sectores de 4 KB. El firmware no usa
//! el NVS de ESP-IDF, así que la reparte a su manera (la tabla de particiones
//! por defecto de espflash la pone en `0x9000`, con 6 sectores):
//!
//! | sector | dirección | qué guarda |
//! |---|---|---|
//! | 0 | `0x9000` | configuración del minero (`bin/miner/store.rs`) |
//! | 1–2 | `0xA000`–`0xBFFF` | el personaje y la hora (`save_store.rs`) |
//! | 3 | `0xC000` | la configuración del aparato ([`settings::Settings`]) |
//! | 4–5 | `0xD000`–`0xEFFF` | libres |
//!
//! Cargar un firmware nuevo no toca esta zona.

use esp_bootloader_esp_idf::partitions::{
    self, DataPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionType,
};
use esp_hal::peripherals::FLASH;
use esp_storage::{FlashStorage, FlashStorageError};
use log::{error, info};
use settings::{RECORD_LEN, Settings};

pub const SECTOR: u32 = 4096;
pub const MINER_SECTOR: u32 = 0;
pub const JOURNAL_SECTOR: u32 = 1;
pub const SETTINGS_SECTOR: u32 = 3;
/// Lo que usa este firmware (hasta el sector de la configuración).
const NEEDED: u32 = (SETTINGS_SECTOR + 1) * SECTOR;

pub struct Nvs {
    pub flash: FlashStorage<'static>,
    /// Dónde empieza la partición, si está y alcanza.
    start: Option<u32>,
}

impl Nvs {
    /// Fuera de línea: la tabla de particiones ocupa 3 KB de pila de paso.
    #[inline(never)]
    pub fn new(flash: FLASH<'static>) -> Self {
        let mut flash = FlashStorage::new(flash);
        let mut table = [0u8; PARTITION_TABLE_MAX_LEN];
        let start = partitions::read_partition_table(&mut flash, &mut table)
            .ok()
            .and_then(|t| {
                t.find_partition(PartitionType::Data(DataPartitionSubType::Nvs))
                    .ok()
                    .flatten()
            })
            .filter(|p| p.len() >= NEEDED)
            .map(|p| p.offset());
        match start {
            Some(o) => info!("partición nvs en {o:#x}"),
            None => error!("no está la partición nvs (o es chica): no se va a poder guardar"),
        }
        Self { flash, start }
    }

    /// Dirección absoluta de un sector de la partición.
    pub fn sector(&self, i: u32) -> Option<u32> {
        self.start.map(|s| s + i * SECTOR)
    }

    pub fn erase_sector(&mut self, i: u32) -> Result<(), FlashStorageError> {
        let from = self.sector(i).ok_or(FlashStorageError::OutOfBounds)?;
        self.flash.erase(from, from + SECTOR)
    }

    pub fn load_settings(&mut self) -> Option<Settings> {
        let at = self.sector(SETTINGS_SECTOR)?;
        let mut buf = [0u8; RECORD_LEN];
        self.flash.read_nor(at, &mut buf).ok()?;
        Settings::decode(&buf)
    }

    /// Borra el sector y escribe el registro; después lo vuelve a leer.
    pub fn save_settings(&mut self, s: &Settings) -> bool {
        let Some(at) = self.sector(SETTINGS_SECTOR) else {
            return false;
        };
        let ok = self.erase_sector(SETTINGS_SECTOR).is_ok()
            && self.flash.write_nor(at, &s.encode()).is_ok();
        ok && self.load_settings().as_ref() == Some(s)
    }

    /// "Restablecer de fábrica": el minero, el personaje y la configuración.
    pub fn factory_reset(&mut self) -> bool {
        let mut ok = true;
        for i in [
            MINER_SECTOR,
            JOURNAL_SECTOR,
            JOURNAL_SECTOR + 1,
            SETTINGS_SECTOR,
        ] {
            ok &= self.erase_sector(i).is_ok();
        }
        ok
    }
}
