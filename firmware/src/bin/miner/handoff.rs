//! Lo que un arranque le deja dicho al siguiente: "entrá en configuración"
//! y, si fue por un error, cuál. Vive en la RAM del RTC, que sobrevive al
//! reinicio por software pero no a quedarse sin corriente.

use core::ptr::{addr_of_mut, read_volatile, write_volatile};

use esp_hal::ram;
use miner::Failure;

/// Al prender la placa esta memoria tiene basura (o ceros): sin la marca, no
/// hay pedido.
const MAGIC: u32 = 0x4D4F_4E4F;

#[ram(unstable(rtc_fast, persistent))]
static mut NEXT: [u32; 2] = [0; 2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    None,
    /// Entrar en configuración, con el motivo si fue por un error.
    Setup(Option<Failure>),
}

/// Lee el pedido y lo borra, para que el próximo reinicio no lo repita.
pub fn take() -> Request {
    // SAFETY: se usa solo al arrancar y justo antes de reiniciar, desde la
    // tarea principal.
    let [magic, code] = unsafe { read_volatile(addr_of_mut!(NEXT)) };
    unsafe { write_volatile(addr_of_mut!(NEXT), [0; 2]) };
    if magic == MAGIC {
        Request::Setup(Failure::from_code(code))
    } else {
        Request::None
    }
}

pub fn reboot_into_setup(failure: Option<Failure>) -> ! {
    // SAFETY: ver `take`.
    unsafe {
        write_volatile(
            addr_of_mut!(NEXT),
            [MAGIC, failure.map_or(0, Failure::code)],
        )
    };
    esp_hal::system::software_reset()
}

pub fn reboot() -> ! {
    esp_hal::system::software_reset()
}
