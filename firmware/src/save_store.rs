//! El guardado del personaje en la flash: el segundo y el tercer sector de la
//! partición `nvs` (ver `nvs.rs`). Cargar un firmware nuevo no toca esta
//! zona, así que el personaje sobrevive.

use companion::save::{self, Flash, Journal, SECTOR_LEN, SaveData};
use esp_storage::FlashStorageError;

use crate::nvs::{JOURNAL_SECTOR, Nvs};

struct Zone<'a> {
    nvs: &'a mut Nvs,
    start: u32,
}

impl Flash for Zone<'_> {
    type Error = FlashStorageError;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.nvs.flash.read_nor(self.start + offset, bytes)
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.nvs.flash.write_nor(self.start + offset, bytes)
    }

    fn erase_sector(&mut self, offset: u32) -> Result<(), Self::Error> {
        let from = self.start + offset;
        self.nvs.flash.erase(from, from + SECTOR_LEN)
    }
}

pub struct SaveStore {
    journal: Option<Journal>,
}

impl SaveStore {
    /// Abre la zona y devuelve lo último guardado, si hay.
    pub fn open(nvs: &mut Nvs) -> (Self, Option<SaveData>) {
        let Some(start) = nvs.sector(JOURNAL_SECTOR) else {
            return (Self { journal: None }, None);
        };
        let (journal, found) = save::Journal::mount(&mut Zone { nvs, start });
        (
            Self {
                journal: Some(journal),
            },
            found,
        )
    }

    pub fn save(&mut self, nvs: &mut Nvs, data: &SaveData) -> bool {
        let (Some(journal), Some(start)) = (self.journal.as_mut(), nvs.sector(JOURNAL_SECTOR))
        else {
            return false;
        };
        journal.store(&mut Zone { nvs, start }, data)
    }
}
