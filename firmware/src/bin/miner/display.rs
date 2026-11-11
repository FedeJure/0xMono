//! La pantalla ST7789 de 240×240 por SPI, para las pantallas del modo minero.

use core::convert::Infallible;

use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
use esp_hal::Blocking;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{GPIO5, GPIO6, GPIO7, GPIO20, GPIO21, SPI2};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;
use log::error;
use miner::mirror;
use miner::screen::{self, Screen};
use mipidsi::interface::SpiInterface;
use mipidsi::models::ST7789;
use mipidsi::options::ColorInversion;
use mipidsi::{Builder, Display};
use static_cell::ConstStaticCell;

/// El módulo no tiene CS (o va a GND): el chip siempre escucha.
pub struct NoCs;

impl embedded_hal::digital::ErrorType for NoCs {
    type Error = Infallible;
}

impl embedded_hal::digital::OutputPin for NoCs {
    fn set_low(&mut self) -> Result<(), Infallible> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Infallible> {
        Ok(())
    }
}

type Iface =
    SpiInterface<'static, ExclusiveDevice<Spi<'static, Blocking>, NoCs, NoDelay>, Output<'static>>;

static BUFFER: ConstStaticCell<[u8; 512]> = ConstStaticCell::new([0; 512]);

pub struct Panel {
    lcd: Display<Iface, ST7789, Output<'static>>,
    _backlight: Output<'static>,
}

impl Panel {
    pub fn new(
        spi: SPI2<'static>,
        sck: GPIO6<'static>,
        mosi: GPIO7<'static>,
        dc: GPIO5<'static>,
        rst: GPIO20<'static>,
        blk: GPIO21<'static>,
    ) -> Self {
        let out = OutputConfig::default();
        // Sin CS, el ST7789 solo se sincroniza bien en modo 3 (reloj en alto
        // en reposo).
        let bus = Spi::new(
            spi,
            SpiConfig::default()
                .with_frequency(Rate::from_mhz(40))
                .with_mode(Mode::_3),
        )
        .expect("config SPI inválida")
        .with_sck(sck)
        .with_mosi(mosi);
        let device = ExclusiveDevice::new_no_delay(bus, NoCs).unwrap();
        let iface = SpiInterface::new(device, Output::new(dc, Level::Low, out), BUFFER.take());
        let lcd = Builder::new(ST7789, iface)
            .display_size(240, 240)
            // Los paneles IPS de estos módulos tienen los colores invertidos.
            .invert_colors(ColorInversion::Inverted)
            .reset_pin(Output::new(rst, Level::High, out))
            .init(&mut Delay::new())
            .expect("no se pudo iniciar la ST7789");
        Self {
            lcd,
            _backlight: Output::new(blk, Level::High, out),
        }
    }

    /// Solo los números de la pantalla de minado (ya dibujada con `show`).
    pub fn update_mining(&mut self, m: &screen::Mining) {
        if let Err(e) = screen::draw_mining(&mut self.lcd, m) {
            error!("pantalla: {e:?}");
        }
        mirror_line(&Screen::Mining(*m));
    }

    pub fn show(&mut self, s: &Screen) {
        if let Err(e) = screen::draw(&mut self.lcd, s) {
            error!("pantalla: {e:?}");
        }
        mirror_line(s);
    }
}

/// La pantalla, descripta por el USB para el visor de la PC
/// (`cargo run -p simulator --example miner_view`).
fn mirror_line(s: &Screen) {
    let mut line: heapless::String<320> = heapless::String::new();
    if mirror::describe(s, &mut line).is_ok() {
        esp_println::println!("{line}");
    }
}
