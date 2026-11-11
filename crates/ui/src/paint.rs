//! Colores y piezas de dibujo compartidas: texto centrado o en párrafos, el
//! mono, los iconitos del menú y el QR.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{
    Circle, Ellipse, Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
    Sector, Triangle,
};
pub use fonts::{Align, BODY, Font, HUGE, SMALL, TITLE};
use qrcodegen_no_heap::{QrCode, QrCodeEcc, Version};

use crate::SIZE;

pub const fn rgb(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new(r >> 3, g >> 2, b >> 3)
}

// Paleta pastel, la misma de la página del celular.
pub const BG: Rgb565 = rgb(0xFF, 0xF4, 0xE8);
pub const HEADER: Rgb565 = rgb(0xFF, 0xDF, 0xE9);
pub const INK: Rgb565 = rgb(0x4B, 0x34, 0x26);
pub const MUTE: Rgb565 = rgb(0x9B, 0x85, 0x79);
pub const LINE: Rgb565 = rgb(0xF0, 0xDE, 0xD0);
pub const CARD: Rgb565 = rgb(0xFF, 0xFF, 0xFF);
pub const PINK: Rgb565 = rgb(0xFF, 0x8F, 0xAB);
pub const PINK_LIGHT: Rgb565 = rgb(0xFF, 0xE3, 0xEC);
pub const MINT: Rgb565 = rgb(0x7F, 0xD6, 0xB2);
pub const MINT_LIGHT: Rgb565 = rgb(0xDC, 0xF6, 0xEA);
pub const LAVENDER: Rgb565 = rgb(0xB7, 0xA6, 0xFF);
pub const LAVENDER_LIGHT: Rgb565 = rgb(0xEC, 0xE7, 0xFF);
pub const BANANA: Rgb565 = rgb(0xFF, 0xD3, 0x6E);
pub const BANANA_LIGHT: Rgb565 = rgb(0xFF, 0xF1, 0xC9);
pub const SKY: Rgb565 = rgb(0xA8, 0xD8, 0xFF);
pub const SKY_LIGHT: Rgb565 = rgb(0xE1, 0xF1, 0xFF);
pub const ERR: Rgb565 = rgb(0xE0, 0x56, 0x6C);

const FUR: Rgb565 = rgb(0x85, 0x56, 0x30);
const FACE: Rgb565 = rgb(0xF0, 0xC9, 0xA2);
const CAP: Rgb565 = rgb(0x7F, 0xD6, 0xB2);
const CAP_DARK: Rgb565 = rgb(0x5F, 0xBF, 0x98);
const EYE: Rgb565 = rgb(0x3B, 0x2A, 0x20);

fn fill(c: Rgb565) -> PrimitiveStyle<Rgb565> {
    PrimitiveStyle::with_fill(c)
}

/// Texto con su alineación, con las mayúsculas arriba en `y` (la `x` es el
/// centro, el borde izquierdo o el derecho según `align`).
pub fn text<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    s: &str,
    x: i32,
    y: i32,
    font: &Font,
    color: Rgb565,
    align: Align,
) -> Result<(), D::Error> {
    font.draw(d, s, x, y, align, color)
}

pub fn centered<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    s: &str,
    y: i32,
    font: &Font,
    color: Rgb565,
) -> Result<(), D::Error> {
    text(d, s, SIZE as i32 / 2, y, font, color, Align::Center)
}

/// Cuántos renglones ocupa `s` en `width` píxeles.
pub fn count_lines(s: &str, font: &Font, width: u32) -> usize {
    font.wrap(s, width as i32).count()
}

/// Párrafo centrado entre `x0` y `x0 + width`, cortado por palabras (los
/// `'\n'` se respetan). Devuelve la `y` debajo del último renglón.
pub fn paragraph<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    s: &str,
    x0: i32,
    width: u32,
    y: i32,
    font: &Font,
    color: Rgb565,
) -> Result<i32, D::Error> {
    let mut y = y;
    for l in font.wrap(s, width as i32) {
        text(d, l, x0 + width as i32 / 2, y, font, color, Align::Center)?;
        y += font.line;
    }
    Ok(y)
}

/// Una etiqueta y su valor en un renglón centrado, cada uno con su letra
/// (las bases alineadas): "Clave: kd7wq3mz", con la clave grande.
#[allow(clippy::too_many_arguments)]
pub fn labeled<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    label: &str,
    label_font: &Font,
    label_color: Rgb565,
    value: &str,
    value_font: &Font,
    value_color: Rgb565,
    y: i32,
) -> Result<(), D::Error> {
    let (lw, vw) = (label_font.width(label), value_font.width(value));
    let gap = label_font.advance(' ');
    let x = (SIZE as i32 - lw - gap - vw) / 2;
    let base = y + value_font.cap;
    text(
        d,
        label,
        x,
        base - label_font.cap,
        label_font,
        label_color,
        Align::Left,
    )?;
    text(
        d,
        value,
        x + lw + gap,
        y,
        value_font,
        value_color,
        Align::Left,
    )
}

/// Una tarjeta blanca con borde suave, como las de la página.
pub fn card<D: DrawTarget<Color = Rgb565>>(d: &mut D, area: Rectangle) -> Result<(), D::Error> {
    let shadow = Rectangle::new(area.top_left + Point::new(0, 3), area.size);
    RoundedRectangle::with_equal_corners(shadow, Size::new(14, 14))
        .into_styled(fill(LINE))
        .draw(d)?;
    RoundedRectangle::with_equal_corners(area, Size::new(14, 14))
        .into_styled(fill(CARD))
        .draw(d)
}

/// Alto del encabezado (sin las ondas de abajo).
pub const HEADER_H: i32 = 44;
/// Donde empieza el título en el encabezado y cuánto puede ocupar.
const HEADER_X: i32 = 50;
const HEADER_W: i32 = SIZE as i32 - HEADER_X - 4;

/// El encabezado rosado con el mono chiquito y el título: grande si entra;
/// si no, más chico, y si tampoco, en dos renglones.
pub fn header<D: DrawTarget<Color = Rgb565>>(d: &mut D, title: &str) -> Result<(), D::Error> {
    d.fill_solid(
        &Rectangle::new(Point::zero(), Size::new(SIZE, HEADER_H as u32)),
        HEADER,
    )?;
    // Borde de abajo ondulado: medias lunas del color del fondo.
    for i in 0..9 {
        Circle::with_center(Point::new(i * 30 + 15, 50), 16)
            .into_styled(fill(BG))
            .draw(d)?;
    }
    monkey(d, Point::new(26, 22), 1)?;
    let x = HEADER_X;
    if TITLE.width(title) <= HEADER_W {
        return text(d, title, x, 11, &TITLE, INK, Align::Left);
    }
    if BODY.width(title) <= HEADER_W {
        return text(d, title, x, 14, &BODY, INK, Align::Left);
    }
    let mut y = 4;
    for l in BODY.wrap(title, HEADER_W).take(2) {
        text(d, l, x, y, &BODY, INK, Align::Left)?;
        y += 18;
    }
    Ok(())
}

/// ¿El título entra en el encabezado (en dos renglones como mucho)?
pub fn header_fits(title: &str) -> bool {
    BODY.wrap(title, HEADER_W).count() <= 2
}

/// La cara del mono, de 36×30 por `scale`, centrada en `c`.
pub fn monkey<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    c: Point,
    scale: i32,
) -> Result<(), D::Error> {
    let s = scale;
    let at = |x: i32, y: i32| c + Point::new(x * s, y * s);
    let circle = |d: &mut D, x: i32, y: i32, diameter: i32, color: Rgb565| {
        Circle::with_center(at(x, y), (diameter * s) as u32)
            .into_styled(fill(color))
            .draw(d)
    };
    // Orejas.
    circle(d, -14, 2, 12, FUR)?;
    circle(d, 14, 2, 12, FUR)?;
    circle(d, -14, 2, 6, FACE)?;
    circle(d, 14, 2, 6, FACE)?;
    // Cabeza y cara.
    circle(d, 0, 2, 28, FUR)?;
    Ellipse::with_center(at(0, 5), Size::new((22 * s) as u32, (18 * s) as u32))
        .into_styled(fill(FACE))
        .draw(d)?;
    Ellipse::with_center(at(0, -1), Size::new((16 * s) as u32, (10 * s) as u32))
        .into_styled(fill(FACE))
        .draw(d)?;
    // Gorra: media luna arriba, con visera.
    Sector::with_center(at(0, -2), (30 * s) as u32, 180.0.deg(), 180.0.deg())
        .into_styled(fill(CAP))
        .draw(d)?;
    RoundedRectangle::with_equal_corners(
        Rectangle::new(at(-2, -4), Size::new((17 * s) as u32, (4 * s) as u32)),
        Size::new((2 * s) as u32, (2 * s) as u32),
    )
    .into_styled(fill(CAP_DARK))
    .draw(d)?;
    circle(d, 0, -16, 4, PINK)?;
    // Ojos con brillo y cachetes.
    for x in [-5, 5] {
        Ellipse::with_center(at(x, 3), Size::new((4 * s) as u32, (5 * s) as u32))
            .into_styled(fill(EYE))
            .draw(d)?;
        if s > 1 {
            circle(d, x + 1, 2, 1, CARD)?;
        }
        Ellipse::with_center(
            at(x * 2 - x.signum(), 9),
            Size::new((4 * s) as u32, (2 * s) as u32),
        )
        .into_styled(fill(PINK))
        .draw(d)?;
    }
    // Sonrisa.
    let w = s.max(1) as u32 + 1;
    Line::new(at(-3, 10), at(0, 12))
        .into_styled(PrimitiveStyle::with_stroke(EYE, w))
        .draw(d)?;
    Line::new(at(0, 12), at(3, 10))
        .into_styled(PrimitiveStyle::with_stroke(EYE, w))
        .draw(d)?;
    Ok(())
}

/// Iconitos del menú, de unos 14 px, sobre un círculo de color de 28.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Home,
    Game,
    Gear,
    Phone,
    Clock,
    Info,
    Back,
    Star,
}

impl Icon {
    fn colors(self) -> (Rgb565, Rgb565) {
        match self {
            Icon::Home => (BANANA_LIGHT, rgb(0xC9, 0x8A, 0x2E)),
            Icon::Game | Icon::Star => (LAVENDER_LIGHT, rgb(0x7A, 0x63, 0xE0)),
            Icon::Gear | Icon::Info => (SKY_LIGHT, rgb(0x3E, 0x8E, 0xD0)),
            Icon::Phone => (PINK_LIGHT, rgb(0xE0, 0x5C, 0x84)),
            Icon::Clock => (MINT_LIGHT, rgb(0x2F, 0xA0, 0x78)),
            Icon::Back => (rgb(0xF4, 0xEC, 0xE6), MUTE),
        }
    }
}

pub fn icon<D: DrawTarget<Color = Rgb565>>(d: &mut D, i: Icon, c: Point) -> Result<(), D::Error> {
    let (bg, fg) = i.colors();
    Circle::with_center(c, 28).into_styled(fill(bg)).draw(d)?;
    let p = |x: i32, y: i32| c + Point::new(x, y);
    let stroke = PrimitiveStyle::with_stroke(fg, 2);
    match i {
        Icon::Home => {
            Triangle::new(p(-7, -1), p(0, -7), p(7, -1))
                .into_styled(fill(fg))
                .draw(d)?;
            d.fill_solid(&Rectangle::new(p(-5, -1), Size::new(11, 7)), fg)?;
            d.fill_solid(&Rectangle::new(p(-1, 2), Size::new(3, 4)), bg)?;
        }
        Icon::Game => {
            RoundedRectangle::with_equal_corners(
                Rectangle::new(p(-8, -4), Size::new(17, 9)),
                Size::new(4, 4),
            )
            .into_styled(fill(fg))
            .draw(d)?;
            d.fill_solid(&Rectangle::new(p(-6, 0), Size::new(5, 1)), bg)?;
            d.fill_solid(&Rectangle::new(p(-4, -2), Size::new(1, 5)), bg)?;
            Circle::with_center(p(4, -1), 3)
                .into_styled(fill(bg))
                .draw(d)?;
            Circle::with_center(p(6, 2), 3)
                .into_styled(fill(bg))
                .draw(d)?;
        }
        Icon::Gear => {
            for (x, y) in [
                (0, -6),
                (0, 6),
                (-6, 0),
                (6, 0),
                (-4, -4),
                (4, 4),
                (-4, 4),
                (4, -4),
            ] {
                Circle::with_center(p(x, y), 4)
                    .into_styled(fill(fg))
                    .draw(d)?;
            }
            Circle::with_center(c, 11).into_styled(fill(fg)).draw(d)?;
            Circle::with_center(c, 4).into_styled(fill(bg)).draw(d)?;
        }
        Icon::Phone => {
            RoundedRectangle::with_equal_corners(
                Rectangle::new(p(-4, -7), Size::new(9, 15)),
                Size::new(3, 3),
            )
            .into_styled(fill(fg))
            .draw(d)?;
            d.fill_solid(&Rectangle::new(p(-2, -5), Size::new(5, 9)), bg)?;
            d.fill_solid(&Rectangle::new(p(0, 5), Size::new(1, 1)), bg)?;
        }
        Icon::Clock => {
            Circle::with_center(c, 15)
                .into_styled(PrimitiveStyle::with_stroke(fg, 2))
                .draw(d)?;
            Line::new(c, p(0, -4)).into_styled(stroke).draw(d)?;
            Line::new(c, p(3, 1)).into_styled(stroke).draw(d)?;
        }
        Icon::Info => {
            d.fill_solid(&Rectangle::new(p(-1, -2), Size::new(3, 8)), fg)?;
            Circle::with_center(p(0, -5), 3)
                .into_styled(fill(fg))
                .draw(d)?;
        }
        Icon::Back => {
            Line::new(p(-5, 0), p(5, 0)).into_styled(stroke).draw(d)?;
            Line::new(p(-5, 0), p(-1, -4)).into_styled(stroke).draw(d)?;
            Line::new(p(-5, 0), p(-1, 4)).into_styled(stroke).draw(d)?;
        }
        Icon::Star => {
            Triangle::new(p(0, -7), p(-6, 4), p(6, 4))
                .into_styled(fill(fg))
                .draw(d)?;
            Triangle::new(p(0, 7), p(-6, -3), p(6, -3))
                .into_styled(fill(fg))
                .draw(d)?;
        }
    }
    Ok(())
}

/// Una fila resaltada: la elegida del menú.
pub fn highlight<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    area: Rectangle,
    fill_color: Rgb565,
    border: Rgb565,
) -> Result<(), D::Error> {
    RoundedRectangle::with_equal_corners(area, Size::new(12, 12))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(fill_color)
                .stroke_color(border)
                .stroke_width(2)
                .build(),
        )
        .draw(d)
}

/// QR centrado en horizontal, con su margen blanco, en el cuadrado de lado
/// `max` que empieza en `y`. Devuelve la `y` de abajo.
pub fn qr<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    text: &str,
    y: i32,
    max: u32,
) -> Result<i32, D::Error> {
    const MAX_VERSION: Version = Version::new(8);
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
        centered(d, "(texto largo para QR)", y + 40, &SMALL, ERR)?;
        return Ok(y + max as i32);
    };
    let n = code.size();
    // 2 módulos de margen blanco, que las cámaras necesitan.
    let quiet = 2;
    let scale = (max as i32 / (n + 2 * quiet)).max(1);
    let side = (n + 2 * quiet) * scale;
    let x0 = (SIZE as i32 - side) / 2;
    RoundedRectangle::with_equal_corners(
        Rectangle::new(
            Point::new(x0 - 4, y - 4),
            Size::new(side as u32 + 8, side as u32 + 8),
        ),
        Size::new(12, 12),
    )
    .into_styled(fill(CARD))
    .draw(d)?;
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
                INK,
            )?;
        }
    }
    Ok(y + side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_titles_go_in_two_lines() {
        assert!(TITLE.width("Configuración") <= HEADER_W);
        assert!(header_fits("No pude ponerme en hora"));
        assert!(header_fits("Configurar desde el celular"));
        assert_eq!(count_lines("hola che como andas", &BODY, 1000), 1);
        assert_eq!(count_lines("hola\nche", &BODY, 1000), 2);
    }
}
