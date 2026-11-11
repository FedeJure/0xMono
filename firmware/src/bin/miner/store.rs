//! La configuración del minero en la flash: primer sector de la partición
//! `nvs` (el firmware no usa el NVS de ESP-IDF, así que el lugar está libre).

use esp_bootloader_esp_idf::partitions::{
    self, DataPartitionSubType, PARTITION_TABLE_MAX_LEN, PartitionType,
};
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;
use log::{error, info};
use miner::Config;
use miner::config::MAX_ENCODED;

pub struct Store {
    flash: FlashStorage<'static>,
    offset: Option<u32>,
}

impl Store {
    pub fn new(flash: FLASH<'static>) -> Self {
        let mut flash = FlashStorage::new(flash);
        let mut table = [0u8; PARTITION_TABLE_MAX_LEN];
        let offset = partitions::read_partition_table(&mut flash, &mut table)
            .ok()
            .and_then(|t| {
                t.find_partition(PartitionType::Data(DataPartitionSubType::Nvs))
                    .ok()
                    .flatten()
            })
            .map(|p| p.offset());
        match offset {
            Some(o) => info!("configuración en la flash: {o:#x}"),
            None => error!("no está la partición nvs: no se va a poder guardar"),
        }
        Self { flash, offset }
    }

    pub fn load(&mut self) -> Option<Config> {
        let mut buf = [0u8; MAX_ENCODED];
        self.flash.read(self.offset?, &mut buf).ok()?;
        Config::decode(&buf)
    }

    pub fn save(&mut self, config: &Config) -> bool {
        let Some(offset) = self.offset else {
            return false;
        };
        let mut buf = [0u8; MAX_ENCODED];
        let n = config.encode(&mut buf);
        // `write` borra el sector y lo reescribe.
        let ok = self.flash.write(offset, &buf[..n]).is_ok();
        ok && self.load().as_ref() == Some(config)
    }
}
