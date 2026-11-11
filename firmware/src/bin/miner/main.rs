#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

//! Modo minero del mono (por ahora: configuración y conexión al WiFi).
//!
//! Arranca en uno de dos modos:
//!
//! - **Configuración** ([`setup`]): si no hay nada guardado, si la conexión
//!   falló o si se pidió con el botón C. El mono arma su propia red WiFi y
//!   muestra un QR para unirse; el celular abre solo la página donde se elige
//!   la red, la clave y la dirección de Bitcoin.
//! - **En línea** ([`online`]): se conecta al WiFi guardado.
//!
//! Para pasar de un modo al otro se reinicia: cada modo arranca con toda la
//! memoria libre, y es lo mismo que hará el firmware completo para entrar y
//! salir de este modo (el WiFi y el 3D no entran juntos en la RAM).
//!
//! Conexiones: pantalla ST7789 en SCL=GPIO6, SDA=GPIO7, DC=GPIO5, RES=GPIO20,
//! BLK=GPIO21 (CS a GND); botones A/B/C en GPIO3/1/0 (a GND).

#[path = "../../captive_net.rs"]
mod captive_net;
mod display;
mod handoff;
mod hasher;
mod mine;
mod online;
mod setup;
mod store;

use companion::{ButtonState, Gesture};
use embassy_executor::Spawner;
use embassy_net::Runner;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::peripherals::{GPIO0, GPIO1, GPIO3};
use esp_hal::ram;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::wifi::Interface;
use log::info;

esp_bootloader_esp_idf::esp_app_desc!();

/// Cada cuánto se leen los botones mientras se espera otra cosa.
const TICK_MS: u64 = 30;

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    // El WiFi necesita memoria dinámica (~60 KB en modo punto de acceso).
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 36 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let panel = display::Panel::new(
        peripherals.SPI2,
        peripherals.GPIO6,
        peripherals.GPIO7,
        peripherals.GPIO5,
        peripherals.GPIO20,
        peripherals.GPIO21,
    );
    let buttons = Buttons::new(peripherals.GPIO3, peripherals.GPIO1, peripherals.GPIO0);
    let mut store = store::Store::new(peripherals.FLASH);
    let saved = store.load();
    let request = handoff::take();
    let rng = Rng::new();

    match (saved, request) {
        (Some(config), handoff::Request::None) => {
            info!("configuración guardada: red «{}»", config.ssid);
            online::run(
                spawner,
                peripherals.WIFI,
                peripherals.SHA,
                config,
                panel,
                buttons,
                rng,
            )
            .await
        }
        (saved, request) => {
            info!("modo configuración ({request:?})");
            setup::run(
                spawner,
                peripherals.WIFI,
                saved,
                request,
                panel,
                buttons,
                store,
                rng,
            )
            .await
        }
    }
}

#[embassy_executor::task(pool_size = 2)]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

/// Los tres botones (a GND, con pull-up interno). Nunca configurarlos como
/// salida: con el botón apretado sería un cortocircuito.
pub struct Buttons {
    pins: [Input<'static>; 3],
    state: [ButtonState; 3],
}

impl Buttons {
    fn new(a: GPIO3<'static>, b: GPIO1<'static>, c: GPIO0<'static>) -> Self {
        let pull_up = InputConfig::default().with_pull(Pull::Up);
        Self {
            pins: [
                Input::new(a, pull_up),
                Input::new(b, pull_up),
                Input::new(c, pull_up),
            ],
            state: [ButtonState::new(); 3],
        }
    }

    /// Lee los botones; devuelve el gesto de C (A y B no se usan acá).
    pub fn poll_c(&mut self, dt_ms: u32) -> Option<Gesture> {
        let mut c = None;
        for (i, (pin, state)) in self.pins.iter().zip(&mut self.state).enumerate() {
            let g = state.update(pin.is_low(), dt_ms);
            if i == 2 {
                c = g;
            }
        }
        c
    }
}
