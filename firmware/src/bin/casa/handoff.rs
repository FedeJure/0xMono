//! El pedido para el próximo arranque, en la RAM del RTC: sobrevive al
//! reinicio por software pero no a quedarse sin corriente. Qué se pide y cómo
//! se escribe está en `settings::handoff`; acá solo se guarda y se lee.

use core::ptr::{addr_of_mut, read_volatile, write_volatile};

use esp_hal::ram;
use settings::handoff::{Next, Words};

#[ram(unstable(rtc_fast, persistent))]
static mut NEXT: Words = [0; 3];

/// Lee el pedido y lo borra, para que el próximo reinicio no lo repita.
pub fn take() -> Next {
    // SAFETY: se usa solo al arrancar y justo antes de reiniciar, desde el
    // programa principal.
    let words = unsafe { read_volatile(addr_of_mut!(NEXT)) };
    unsafe { write_volatile(addr_of_mut!(NEXT), [0; 3]) };
    Next::decode(words)
}

/// Deja el pedido y reinicia.
pub fn reboot(next: Next) -> ! {
    log::info!("reiniciando: {next:?}");
    // SAFETY: ver `take`.
    unsafe { write_volatile(addr_of_mut!(NEXT), next.encode()) };
    esp_hal::system::software_reset()
}
