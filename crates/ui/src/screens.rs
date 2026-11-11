//! Pantallas de los modos con WiFi: la configuración desde el celular y la
//! hora por internet. Son quietas: se dibujan una vez por cambio.

use core::fmt::Write;

use captive::{AP_HOST, AP_URL, wifi_qr_text};
use companion::LocalTime;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, PrimitiveStyle, Rectangle, Triangle};
use heapless::String;
use settings::date::{Clock, DayLabel};

use crate::SIZE;
use crate::paint::{self, BG, BODY, ERR, HEADER, HUGE, INK, MINT, MUTE, PINK, SMALL, TITLE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen<'a> {
    /// Buscando redes y armando la propia.
    Preparing,
    /// Paso 1: escanear el QR para unirse a la red del mono.
    Join {
        ssid: &'a str,
        pass: &'a str,
    },
    /// Paso 2: el celular ya está en la red; abrir la página.
    Open,
    /// Se eligió algo en la página: guarda y se reinicia.
    Saved,
    /// Volviendo a la casa sin cambiar nada.
    Leaving,
    /// Conectándose para preguntar la hora.
    Syncing {
        ssid: &'a str,
    },
    Synced {
        now: LocalTime,
    },
    SyncFailed {
        reason: &'a str,
    },
}

pub fn draw<D: DrawTarget<Color = Rgb565>>(d: &mut D, s: &Screen) -> Result<(), D::Error> {
    d.clear(BG)?;
    match *s {
        Screen::Preparing => {
            big_monkey(d, 66)?;
            paint::centered(d, "Preparando", 126, &TITLE, INK)?;
            paint::centered(d, "mi red WiFi...", 152, &TITLE, INK)?;
            paint::paragraph(
                d,
                "Estoy buscando las redes de tu casa.",
                12,
                216,
                188,
                &BODY,
                MUTE,
            )?;
        }
        Screen::Join { ssid, pass } => {
            // El QR grande (4 píxeles por módulo) y la clave bien grande,
            // que a veces hay que escribirla a mano.
            strip(d, "¡Conectate a mi red!")?;
            let y = paint::qr(d, &wifi_qr_text(ssid, pass), 36, 132)?;
            paint::labeled(d, "Red:", &SMALL, MUTE, ssid, &BODY, INK, y + 8)?;
            paint::labeled(d, "Clave:", &BODY, MUTE, pass, &TITLE, INK, y + 28)?;
            paint::centered(d, "C: volver con el mono", 225, &SMALL, MUTE)?;
        }
        Screen::Open => {
            strip(d, "¡Ya estás conectado!")?;
            let y = paint::qr(d, AP_URL, 38, 100)?;
            paint::centered(d, AP_HOST, y + 10, &TITLE, PINK)?;
            paint::paragraph(
                d,
                "¿No se abrió la página? Escaneá el QR o escribí esa dirección.",
                8,
                224,
                y + 40,
                &SMALL,
                MUTE,
            )?;
            paint::centered(d, "C: volver con el mono", 224, &SMALL, MUTE)?;
        }
        Screen::Saved => {
            big_monkey(d, 70)?;
            paint::centered(d, "¡Guardado!", 132, &TITLE, INK)?;
            hearts(d, 170)?;
            paint::centered(d, "Vuelvo a mi casita...", 192, &BODY, MUTE)?;
        }
        Screen::Leaving => {
            big_monkey(d, 70)?;
            paint::centered(d, "¡Chau, celu!", 132, &TITLE, INK)?;
            paint::centered(d, "Vuelvo a mi casita...", 172, &BODY, MUTE)?;
        }
        Screen::Syncing { ssid } => {
            big_monkey(d, 62)?;
            paint::centered(d, "Poniéndome", 120, &TITLE, INK)?;
            paint::centered(d, "en hora...", 146, &TITLE, INK)?;
            let mut line: String<64> = String::new();
            let _ = write!(line, "Me conecto a\n«{ssid}»");
            paint::paragraph(d, &line, 8, 224, 180, &BODY, MUTE)?;
        }
        Screen::Synced { now } => {
            big_monkey(d, 62)?;
            paint::centered(d, "¡En hora!", 120, &TITLE, INK)?;
            let mut line: String<24> = String::new();
            let _ = write!(line, "{}", Clock(now));
            paint::centered(d, &line, 152, &HUGE, paint::rgb(0x2F, 0xA0, 0x78))?;
            line.clear();
            let _ = write!(line, "{}", DayLabel(now));
            paint::centered(d, &line, 190, &BODY, MUTE)?;
        }
        Screen::SyncFailed { reason } => {
            // El mono más chico: el motivo puede ser largo.
            Circle::with_center(Point::new(120, 40), 70)
                .into_styled(PrimitiveStyle::with_fill(paint::PINK_LIGHT))
                .draw(d)?;
            paint::monkey(d, Point::new(120, 42), 2)?;
            paint::centered(d, "No pude", 84, &TITLE, ERR)?;
            paint::centered(d, "ponerme en hora", 110, &TITLE, ERR)?;
            paint::paragraph(d, reason, 8, 224, 144, &BODY, INK)?;
        }
    }
    Ok(())
}

/// Encabezado finito, sin el mono, para dejarle lugar al QR.
fn strip<D: DrawTarget<Color = Rgb565>>(d: &mut D, title: &str) -> Result<(), D::Error> {
    d.fill_solid(&Rectangle::new(Point::zero(), Size::new(SIZE, 30)), HEADER)?;
    for i in 0..9 {
        Circle::with_center(Point::new(i * 30 + 15, 36), 16)
            .into_styled(PrimitiveStyle::with_fill(BG))
            .draw(d)?;
    }
    paint::centered(d, title, 7, &BODY, INK)
}

fn big_monkey<D: DrawTarget<Color = Rgb565>>(d: &mut D, y: i32) -> Result<(), D::Error> {
    // Una sombrita y el mono grande.
    Circle::with_center(Point::new(120, y - 2), 104)
        .into_styled(PrimitiveStyle::with_fill(paint::PINK_LIGHT))
        .draw(d)?;
    paint::monkey(d, Point::new(120, y), 3)
}

fn hearts<D: DrawTarget<Color = Rgb565>>(d: &mut D, y: i32) -> Result<(), D::Error> {
    for (x, c) in [(96, PINK), (120, MINT), (144, paint::LAVENDER)] {
        let fill = PrimitiveStyle::with_fill(c);
        Circle::with_center(Point::new(x - 3, y), 8)
            .into_styled(fill)
            .draw(d)?;
        Circle::with_center(Point::new(x + 3, y), 8)
            .into_styled(fill)
            .draw(d)?;
        Triangle::new(
            Point::new(x - 7, y + 1),
            Point::new(x + 7, y + 1),
            Point::new(x, y + 9),
        )
        .into_styled(fill)
        .draw(d)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_reasons_fit_under_the_title() {
        // El motivo más largo, debajo de "ponerme en hora".
        for f in captive::Failure::ALL {
            let lines = paint::count_lines(f.message(), &BODY, 224) as i32;
            assert!(
                144 + (lines - 1) * BODY.line + BODY.cap <= SIZE as i32 - 4,
                "{f:?}"
            );
        }
        for s in [
            settings::handoff::SyncError::NoServer.message(),
            "Una red con nombre largo de 32 c",
        ] {
            let lines = paint::count_lines(s, &BODY, 224) as i32;
            assert!(lines <= 4, "{s}");
        }
    }
}
