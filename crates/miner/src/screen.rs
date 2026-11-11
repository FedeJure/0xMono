//! Pantallas del modo minero (240×240, sin el 3D: el renderer no entra en
//! memoria junto al WiFi).

use core::fmt::Write;

use embedded_graphics::{
    mono_font::{
        MonoFont, MonoTextStyleBuilder,
        iso_8859_1::{FONT_6X10, FONT_9X15, FONT_9X15_BOLD, FONT_10X20},
    },
    pixelcolor::Rgb565,
    prelude::*,
    primitives::Rectangle,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use heapless::String;
use qrcodegen_no_heap::{QrCode, QrCodeEcc, Version};

pub use captive::wifi_qr_text;

use crate::portal::{AP_HOST, AP_URL};

pub const SIZE: u32 = 240;

const fn rgb(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new(r >> 3, g >> 2, b >> 3)
}

const BG: Rgb565 = rgb(0x14, 0x11, 0x0F);
const INK: Rgb565 = rgb(0xF3, 0xEC, 0xE4);
const MUTE: Rgb565 = rgb(0xA8, 0x9D, 0x92);
const ACCENT: Rgb565 = rgb(0xF7, 0x93, 0x1A);
const OK: Rgb565 = rgb(0x6B, 0xD1, 0x7A);
const ERR: Rgb565 = rgb(0xFF, 0x6B, 0x5B);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Screen<'a> {
    /// Mirando qué redes WiFi hay alrededor.
    Scanning,
    /// Paso 1: escanear el QR para unirse a la red del mono.
    Join {
        ssid: &'a str,
        pass: &'a str,
    },
    /// Paso 2: el celular ya está en la red; abrir la página.
    Open,
    Saved {
        ssid: &'a str,
    },
    Connecting {
        ssid: &'a str,
    },
    Online {
        ssid: &'a str,
        ip: [u8; 4],
    },
    Failed {
        ssid: &'a str,
        reason: &'a str,
    },
    /// Un aviso cualquiera (por ejemplo, "no llego al pool").
    Notice {
        title: &'a str,
        text: &'a str,
    },
    /// Minando. Después del primer dibujo, [`draw_mining`] actualiza solo
    /// los números.
    Mining(Mining<'a>),
}

/// Lo que muestra la pantalla de minado.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mining<'a> {
    pub ssid: &'a str,
    pub pool: &'a str,
    pub hashrate: f32,
    pub accepted: u32,
    pub rejected: u32,
    /// La mejor dificultad encontrada.
    pub best: f64,
    pub uptime_secs: u32,
    /// Conectado al pool (si no, está reconectando).
    pub online: bool,
}

pub fn draw<D: DrawTarget<Color = Rgb565>>(d: &mut D, screen: &Screen) -> Result<(), D::Error> {
    d.clear(BG)?;
    match *screen {
        Screen::Scanning => {
            title(d, "Modo minero", ACCENT)?;
            centered(d, "Buscando redes WiFi...", 110, &FONT_9X15, INK)?;
        }
        Screen::Join { ssid, pass } => {
            title(d, "Configurar minado", ACCENT)?;
            qr(d, &wifi_qr_text(ssid, pass), 26, 166)?;
            let mut line: String<48> = String::new();
            let _ = write!(line, "Red: {ssid}");
            centered(d, &line, 196, &FONT_9X15, INK)?;
            line.clear();
            let _ = write!(line, "Clave: {pass}");
            centered(d, &line, 216, &FONT_9X15, INK)?;
        }
        Screen::Open => {
            title(d, "Abrí la página", ACCENT)?;
            qr(d, AP_URL, 26, 150)?;
            centered(d, AP_HOST, 184, &FONT_9X15_BOLD, INK)?;
            paragraph(
                d,
                "Si no se abrió sola, escaneá este QR o escribí esa dirección.",
                206,
                &FONT_6X10,
                MUTE,
            )?;
        }
        Screen::Saved { ssid } => {
            title(d, "¡Guardado!", OK)?;
            centered(d, "El mono se va a", 90, &FONT_9X15, INK)?;
            centered(d, "conectar a", 108, &FONT_9X15, INK)?;
            paragraph(d, ssid, 132, &FONT_9X15_BOLD, ACCENT)?;
        }
        Screen::Connecting { ssid } => {
            title(d, "Conectando...", ACCENT)?;
            paragraph(d, ssid, 100, &FONT_9X15_BOLD, INK)?;
        }
        Screen::Online { ssid, ip } => {
            title(d, "Conectado", OK)?;
            paragraph(d, ssid, 60, &FONT_9X15_BOLD, INK)?;
            let mut line: String<24> = String::new();
            let _ = write!(line, "IP {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
            centered(d, &line, 100, &FONT_9X15, MUTE)?;
            centered(d, "Conectando al pool...", 140, &FONT_9X15, ACCENT)?;
            centered(d, "Mantené C para configurar", 220, &FONT_6X10, MUTE)?;
        }
        Screen::Failed { ssid, reason } => {
            title(d, "No pude conectar", ERR)?;
            let y = paragraph(d, ssid, 50, &FONT_9X15_BOLD, INK)?;
            paragraph(d, reason, y + 12, &FONT_9X15, INK)?;
            centered(d, "Abriendo la configuración...", 220, &FONT_6X10, MUTE)?;
        }
        Screen::Notice { title: t, text } => {
            title(d, t, ACCENT)?;
            paragraph(d, text, 70, &FONT_9X15, INK)?;
            centered(d, "Mantené C para configurar", 220, &FONT_6X10, MUTE)?;
        }
        Screen::Mining(ref m) => {
            title(d, "Minando", ACCENT)?;
            centered(d, "hashes por segundo", 66, &FONT_6X10, MUTE)?;
            let mut line: String<48> = String::new();
            let _ = write!(line, "{} - {}", fit(m.pool, 18), fit(m.ssid, 12));
            centered(d, &line, 200, &FONT_6X10, MUTE)?;
            centered(d, "Mantené C para configurar", 220, &FONT_6X10, MUTE)?;
            draw_mining(d, m)?;
        }
    }
    Ok(())
}

/// Solo los números de la pantalla de minado: cada renglón tiene ancho fijo
/// y fondo, así se pisa lo anterior sin borrar toda la pantalla.
pub fn draw_mining<D: DrawTarget<Color = Rgb565>>(d: &mut D, m: &Mining) -> Result<(), D::Error> {
    let mut line: String<32> = String::new();
    let _ = write!(line, "{}", Rate(m.hashrate));
    pad_center(&mut line, 14);
    let color = if m.online { INK } else { MUTE };
    centered(d, &line, 40, &FONT_10X20, color)?;

    row(d, 96, "Shares", |l| {
        write!(l, "{} ok, {} mal", m.accepted, m.rejected)
    })?;
    row(d, 118, "Mejor tiro", |l| write!(l, "{}", Diff(m.best)))?;
    row(d, 140, "Hace", |l| {
        let (h, min) = (m.uptime_secs / 3600, m.uptime_secs / 60 % 60);
        if h > 0 {
            write!(l, "{h} h {min} min")
        } else {
            write!(l, "{min} min")
        }
    })?;
    row(d, 162, "Pool", |l| {
        l.write_str(if m.online {
            "conectado"
        } else {
            "reconectando"
        })
    })?;
    Ok(())
}

/// Un renglón "etiqueta ....... valor", con el valor alineado a la derecha y
/// de ancho fijo.
fn row<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    y: i32,
    label: &str,
    value: impl FnOnce(&mut String<32>) -> core::fmt::Result,
) -> Result<(), D::Error> {
    const WIDTH: usize = 14;
    let style = |color| {
        MonoTextStyleBuilder::new()
            .font(&FONT_9X15)
            .text_color(color)
            .background_color(BG)
            .build()
    };
    Text::with_baseline(label, Point::new(12, y), style(MUTE), Baseline::Top).draw(d)?;
    let mut v: String<32> = String::new();
    let _ = value(&mut v);
    let mut padded: String<32> = String::new();
    for _ in v.chars().count()..WIDTH {
        let _ = padded.push(' ');
    }
    let _ = padded.push_str(&v);
    let x = SIZE as i32 - 12 - (WIDTH as i32) * 9;
    Text::with_baseline(&padded, Point::new(x, y), style(INK), Baseline::Top).draw(d)?;
    Ok(())
}

/// Espacios a los dos lados, para que un texto corto tape al largo de antes.
fn pad_center<const N: usize>(s: &mut String<N>, width: usize) {
    let n = s.chars().count();
    if n >= width {
        return;
    }
    let mut out: String<N> = String::new();
    for _ in 0..(width - n) / 2 {
        let _ = out.push(' ');
    }
    let _ = out.push_str(s);
    while out.chars().count() < width {
        let _ = out.push(' ');
    }
    *s = out;
}

/// Corta un texto a `max` caracteres.
fn fit(s: &str, max: usize) -> &str {
    s.char_indices().nth(max).map_or(s, |(i, _)| &s[..i])
}

/// Hashes por segundo en la unidad que corresponda.
pub struct Rate(pub f32);

impl core::fmt::Display for Rate {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self.0 {
            r if r >= 1e6 => write!(f, "{:.2} MH/s", r / 1e6),
            r if r >= 1e3 => write!(f, "{:.1} kH/s", r / 1e3),
            r => write!(f, "{r:.0} H/s"),
        }
    }
}

/// Dificultad con 3 o 4 cifras útiles.
pub struct Diff(pub f64);

impl core::fmt::Display for Diff {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self.0 {
            d if d <= 0.0 => f.write_str("-"),
            d if d >= 1e6 => write!(f, "{:.2}M", d / 1e6),
            d if d >= 1e3 => write!(f, "{:.2}k", d / 1e3),
            d if d >= 10.0 => write!(f, "{d:.1}"),
            d if d >= 0.001 => write!(f, "{d:.4}"),
            d => write!(f, "{d:.2e}"),
        }
    }
}

fn title<D: DrawTarget<Color = Rgb565>>(d: &mut D, s: &str, color: Rgb565) -> Result<(), D::Error> {
    centered(d, s, 6, &FONT_9X15_BOLD, color)
}

fn centered<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    s: &str,
    y: i32,
    font: &MonoFont,
    color: Rgb565,
) -> Result<(), D::Error> {
    let style = MonoTextStyleBuilder::new()
        .font(font)
        .text_color(color)
        .background_color(BG)
        .build();
    let layout = TextStyleBuilder::new()
        .alignment(Alignment::Center)
        .baseline(Baseline::Top)
        .build();
    Text::with_text_style(s, Point::new(SIZE as i32 / 2, y), style, layout).draw(d)?;
    Ok(())
}

/// Texto centrado y partido en renglones por palabras (las palabras que no
/// entran solas se cortan). Devuelve la `y` debajo del último renglón.
fn paragraph<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    s: &str,
    mut y: i32,
    font: &MonoFont,
    color: Rgb565,
) -> Result<i32, D::Error> {
    let max = ((SIZE - 16) / font.character_size.width) as usize;
    let line_h = font.character_size.height as i32 + 3;
    let mut line: String<64> = String::new();
    let flush = |line: &mut String<64>, y: &mut i32, d: &mut D| -> Result<(), D::Error> {
        if !line.is_empty() {
            centered(d, line, *y, font, color)?;
            *y += line_h;
            line.clear();
        }
        Ok(())
    };
    for word in s.split(' ') {
        let mut word = word;
        loop {
            let used = line.chars().count();
            let room = if used == 0 {
                max
            } else {
                max.saturating_sub(used + 1)
            };
            let n = word.chars().count();
            if n <= room {
                if used > 0 {
                    let _ = line.push(' ');
                }
                let _ = line.push_str(word);
                break;
            }
            if used > 0 {
                flush(&mut line, &mut y, d)?;
                continue;
            }
            // Palabra más larga que un renglón: se corta.
            let cut = word.char_indices().nth(max).map_or(word.len(), |(i, _)| i);
            let _ = line.push_str(&word[..cut]);
            flush(&mut line, &mut y, d)?;
            word = &word[cut..];
            if word.is_empty() {
                break;
            }
        }
    }
    flush(&mut line, &mut y, d)?;
    Ok(y)
}

/// QR centrado en horizontal, con su margen blanco, en el cuadrado de lado
/// `max` que empieza en `y`.
fn qr<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    text: &str,
    y: i32,
    max: u32,
) -> Result<(), D::Error> {
    const MAX_VERSION: Version = Version::new(10);
    let mut temp = [0u8; MAX_VERSION.buffer_len()];
    let mut out = [0u8; MAX_VERSION.buffer_len()];
    let Ok(code) = QrCode::encode_text(
        text,
        &mut temp,
        &mut out,
        QrCodeEcc::Low,
        Version::MIN,
        MAX_VERSION,
        None,
        true,
    ) else {
        return centered(d, "(texto largo para QR)", y + 40, &FONT_6X10, ERR);
    };

    let n = code.size();
    // 2 módulos de margen blanco a cada lado, que las cámaras necesitan.
    let quiet = 2;
    let scale = (max as i32 / (n + 2 * quiet)).max(1);
    let side = (n + 2 * quiet) * scale;
    let x0 = (SIZE as i32 - side) / 2;
    Rectangle::new(Point::new(x0, y), Size::new(side as u32, side as u32))
        .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
            Rgb565::WHITE,
        ))
        .draw(d)?;
    // Tramos negros seguidos de cada fila en un solo rectángulo: muchos
    // menos comandos a la pantalla.
    for my in 0..n {
        let mut mx = 0;
        while mx < n {
            if !code.get_module(mx, my) {
                mx += 1;
                continue;
            }
            let start = mx;
            while mx < n && code.get_module(mx, my) {
                mx += 1;
            }
            d.fill_solid(
                &Rectangle::new(
                    Point::new(x0 + (quiet + start) * scale, y + (quiet + my) * scale),
                    Size::new(((mx - start) * scale) as u32, scale as u32),
                ),
                Rgb565::BLACK,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_well() {
        assert_eq!(std::format!("{}", Rate(123_456.0)), "123.5 kH/s");
        assert_eq!(std::format!("{}", Rate(950.0)), "950 H/s");
        assert_eq!(std::format!("{}", Diff(0.0)), "-");
        assert_eq!(std::format!("{}", Diff(0.012345)), "0.0123");
        assert_eq!(std::format!("{}", Diff(0.00012345)), "1.23e-4");
        assert_eq!(std::format!("{}", Diff(2536.86)), "2.54k");
        assert_eq!(fit("public-pool.io", 6), "public");
    }
}
