#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

//! El firmware del 0xMono: el mono viviendo en su casa en 3D, el menú, los
//! minijuegos, la configuración desde el celular y la hora por internet.
//!
//! El WiFi y la casa en 3D no entran juntos en la RAM, así que cada arranque
//! es de un solo modo ([`memory`]) y para cambiar se reinicia dejando el
//! pedido en la RAM del RTC ([`handoff`]):
//!
//! - **Casa** ([`house`]): el mono, el menú (A+C mantenidos) y los juegos.
//! - **Configuración** ([`net`]): la red del mono con la página para el
//!   celular.
//! - **Hora por internet** ([`net`]): al enchufarlo, si hay WiFi y la hora
//!   automática, primero pregunta la hora (SNTP) y después va a la casa.
//!
//! Conexiones: pantalla como en `st7789.rs`, botones A/B/C en GPIO3/1/0 (a
//! GND) y RX del DY-SV17F en GPIO10.
//!
//! ```text
//! cd firmware
//! cargo run --release
//! ```

extern crate alloc;

#[path = "../../captive_net.rs"]
mod captive_net;
mod handoff;
mod house;
mod memory;
mod net;
#[path = "../../nvs.rs"]
mod nvs;
#[path = "../../save_store.rs"]
mod save_store;
#[path = "../../st7789.rs"]
mod st7789;

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::main;
use log::info;
use settings::handoff::{self as plan, Boot};

esp_bootloader_esp_idf::esp_app_desc!();

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    memory::paint_stack();
    let lcd = st7789::Lcd::new(p.SPI2, p.GPIO6, p.GPIO7, p.GPIO5, p.GPIO20, p.GPIO21);

    // Botones a GND, con pull-up interno. Nunca configurarlos como salida:
    // con el botón apretado sería un cortocircuito.
    let pull_up = InputConfig::default().with_pull(Pull::Up);
    let buttons = [
        Input::new(p.GPIO3, pull_up),
        Input::new(p.GPIO1, pull_up),
        Input::new(p.GPIO0, pull_up),
    ];

    let mut nvs = nvs::Nvs::new(p.FLASH);
    let settings = nvs.load_settings();
    match &settings {
        Some(s) if s.has_wifi() => info!(
            "configuración: WiFi «{}», hora automática: {}",
            s.ssid, s.auto_time
        ),
        Some(_) => info!("configuración: sin WiFi"),
        None => info!("configuración: la de fábrica"),
    }
    let settings = settings.unwrap_or_default();
    let next = handoff::take();
    let boot = plan::boot(next, &settings);
    info!("0xMono {VERSION}: {next:?} -> {boot:?}");

    match boot {
        Boot::House(start) => house::run(
            house::Hw {
                lcd,
                buttons,
                uart: p.UART1,
                sound_tx: p.GPIO10,
            },
            nvs,
            settings,
            start,
        ),
        Boot::Portal => net::run(
            net::Job::Portal,
            net::Hw {
                lcd,
                buttons,
                wifi: p.WIFI,
                timg0: p.TIMG0,
                sw_int: p.FROM_CPU_INTR0,
            },
            nvs,
            settings,
        ),
        Boot::Sync(s) => net::run(
            net::Job::Sync(s),
            net::Hw {
                lcd,
                buttons,
                wifi: p.WIFI,
                timg0: p.TIMG0,
                sw_int: p.FROM_CPU_INTR0,
            },
            nvs,
            settings,
        ),
    }
}
