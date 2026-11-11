//! Protocolo UART del reproductor MP3 DY-SV17F (9600 baudios, 8N1).
//!
//! Cada trama es `AA <cmd> <len> <datos…> <suma>`, donde la suma es el byte
//! bajo de la suma de todos los bytes anteriores. Este crate solo arma los
//! bytes: quién los envía (la UART1 del ESP32 por GPIO10) no importa acá.
//!
//! El módulo es opcional: sin él conectado, las tramas salen por el pin y
//! nadie las escucha. Los MP3 se copian a la memoria del módulo por su USB
//! (con la placa desenchufada), con nombres de 5 cifras en la raíz:
//! `00001.mp3`, `00002.mp3`...
#![no_std]

/// Volumen de arranque (0..=30). El amplificador del módulo está pensado para
/// 4 Ω / 5 W y el altavoz es de 8 Ω / 1 W: no subir de ~20 sin probar antes.
pub const SAFE_VOLUME: u8 = 15;
pub const MAX_VOLUME: u8 = 30;

/// Los sonidos que usa el firmware. Para sumar uno: una variante acá, su
/// número en [`Sound::track`] y el MP3 con ese número en el módulo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// Al tocar un botón en la casa.
    Beep,
}

impl Sound {
    /// Número de archivo en la memoria del DY-SV17F (1 → `00001.mp3`).
    pub fn track(self) -> u16 {
        match self {
            Sound::Beep => 1,
        }
    }
}

fn with_checksum<const N: usize>(mut frame: [u8; N]) -> [u8; N] {
    let sum = frame[..N - 1]
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_add(b));
    frame[N - 1] = sum;
    frame
}

/// Reproduce `/<track:05>.mp3` (p. ej. 1 → `00001.mp3`).
pub fn play_track(track: u16) -> [u8; 6] {
    let [hi, lo] = track.to_be_bytes();
    with_checksum([0xAA, 0x07, 0x02, hi, lo, 0])
}

/// Reproduce un [`Sound`].
pub fn play(sound: Sound) -> [u8; 6] {
    play_track(sound.track())
}

pub fn set_volume(volume: u8) -> [u8; 5] {
    with_checksum([0xAA, 0x13, 0x01, volume.min(MAX_VOLUME), 0])
}

pub fn stop() -> [u8; 4] {
    with_checksum([0xAA, 0x04, 0x00, 0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_match_datasheet_examples() {
        assert_eq!(stop(), [0xAA, 0x04, 0x00, 0xAE]);
        assert_eq!(play_track(1), [0xAA, 0x07, 0x02, 0x00, 0x01, 0xB4]);
        assert_eq!(set_volume(20), [0xAA, 0x13, 0x01, 0x14, 0xD2]);
    }

    #[test]
    fn volume_is_clamped() {
        assert_eq!(set_volume(200)[3], MAX_VOLUME);
    }

    #[test]
    fn beep_is_the_first_file() {
        assert_eq!(play(Sound::Beep), play_track(1));
    }
}
