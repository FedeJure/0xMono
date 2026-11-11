//! La pantalla ST7789 de 240×240 por SPI, sin CS: se habla directo con el chip
//! para mandar cada franja tal como la dejó el motor, sin convertir píxel por
//! píxel. Los búferes tienen que armarse con `PixelOrder::Swapped`.
//!
//! Conexiones: SCL=GPIO6, SDA=GPIO7, DC=GPIO5, RES=GPIO20, BLK=GPIO21 (CS a GND).

use esp_hal::Blocking;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{GPIO5, GPIO6, GPIO7, GPIO20, GPIO21, SPI2};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;

pub struct Lcd {
    spi: Spi<'static, Blocking>,
    dc: Output<'static>,
    _backlight: Output<'static>,
}

impl Lcd {
    pub fn new(
        spi: SPI2<'static>,
        sck: GPIO6<'static>,
        mosi: GPIO7<'static>,
        dc: GPIO5<'static>,
        rst: GPIO20<'static>,
        blk: GPIO21<'static>,
    ) -> Self {
        let out = OutputConfig::default();
        // Sin CS, el ST7789 solo se sincroniza bien en modo 3.
        let spi = Spi::new(
            spi,
            SpiConfig::default()
                // Con cables dupont no conviene más: 40 MHz es mucho.
                .with_frequency(Rate::from_mhz(20))
                .with_mode(Mode::_3),
        )
        .expect("config SPI inválida")
        .with_sck(sck)
        .with_mosi(mosi);
        let mut reset = Output::new(rst, Level::High, out);
        let delay = Delay::new();
        reset.set_low();
        delay.delay_millis(10);
        reset.set_high();
        delay.delay_millis(120);

        let mut lcd = Self {
            spi,
            dc: Output::new(dc, Level::Low, out),
            _backlight: Output::new(blk, Level::High, out),
        };
        lcd.init(&delay);
        lcd
    }

    fn command(&mut self, command: u8, data: &[u8]) {
        self.dc.set_low();
        self.spi.write(&[command]).expect("SPI");
        self.dc.set_high();
        if !data.is_empty() {
            self.spi.write(data).expect("SPI");
        }
    }

    fn init(&mut self, delay: &Delay) {
        self.command(0x01, &[]); // SWRESET
        delay.delay_millis(150);
        self.command(0x11, &[]); // SLPOUT
        delay.delay_millis(120);
        self.command(0x3A, &[0x55]); // COLMOD: RGB565
        self.command(0x36, &[0x00]); // MADCTL: sin rotar, orden RGB
        // Tensiones y gamma habituales de los paneles IPS de 1,3".
        self.command(0xB2, &[0x0C, 0x0C, 0x00, 0x33, 0x33]); // PORCTRL
        self.command(0xB7, &[0x35]); // GCTRL
        self.command(0xBB, &[0x19]); // VCOMS
        self.command(0xC0, &[0x2C]); // LCMCTRL
        self.command(0xC2, &[0x01]); // VDVVRHEN
        self.command(0xC3, &[0x12]); // VRHS
        self.command(0xC4, &[0x20]); // VDVS
        self.command(0xC6, &[0x0F]); // FRCTRL2: 60 Hz
        self.command(0xD0, &[0xA4, 0xA1]); // PWCTRL1
        self.command(
            0xE0, // PVGAMCTRL
            &[0xD0, 0x04, 0x0D, 0x11, 0x13, 0x2B, 0x3F, 0x54, 0x4C, 0x18, 0x0D, 0x0B, 0x1F, 0x23],
        );
        self.command(
            0xE1, // NVGAMCTRL
            &[0xD0, 0x04, 0x0C, 0x11, 0x13, 0x2C, 0x3F, 0x44, 0x51, 0x2F, 0x1F, 0x1F, 0x20, 0x23],
        );
        // Los paneles IPS de estos módulos tienen los colores invertidos.
        self.command(0x21, &[]); // INVON
        self.command(0x13, &[]); // NORON
        self.command(0x29, &[]); // DISPON
        delay.delay_millis(20);
    }

    /// Elige el rectángulo: lo que siga en `pixels` lo llena de arriba abajo.
    pub fn window(&mut self, x0: usize, y0: usize, width: usize, height: usize) {
        let [xa, xb] = (x0 as u16).to_be_bytes();
        let [xc, xd] = ((x0 + width - 1) as u16).to_be_bytes();
        let [ya, yb] = (y0 as u16).to_be_bytes();
        let [yc, yd] = ((y0 + height - 1) as u16).to_be_bytes();
        self.command(0x2A, &[xa, xb, xc, xd]); // CASET
        self.command(0x2B, &[ya, yb, yc, yd]); // RASET
        self.command(0x2C, &[]); // RAMWR
    }

    pub fn pixels(&mut self, pixels: &[u16]) {
        // SAFETY: `u8` no pide alineación y el largo cubre justo los mismos
        // bytes de `pixels`. Con `PixelOrder::Swapped` ya están big-endian.
        let bytes = unsafe {
            core::slice::from_raw_parts(pixels.as_ptr().cast::<u8>(), size_of_val(pixels))
        };
        self.spi.write(bytes).expect("SPI");
    }
}
