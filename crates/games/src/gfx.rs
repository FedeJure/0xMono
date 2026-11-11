//! Ayudas de dibujo compartidas por los juegos.
//!
//! Todo se dibuja con rectángulos llenos (`fill_solid`) y se saltea lo que no
//! toca la zona visible (`vis`, la `bounding_box()` del `DrawTarget`). En la
//! placa el dibujo se repite una vez por franja: lo que cae afuera no cuesta
//! casi nada.

use core::fmt::Write;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use fonts::{Align, Font};

/// Color desde `0xRRGGBB`.
pub const fn rgb(hex: u32) -> Rgb565 {
    Rgb565::new(
        ((hex >> 16) & 0xFF) as u8 >> 3,
        ((hex >> 8) & 0xFF) as u8 >> 2,
        (hex & 0xFF) as u8 >> 3,
    )
}

/// ¿Se cruzan los dos rectángulos?
pub fn touches(vis: &Rectangle, x: i32, y: i32, w: u32, h: u32) -> bool {
    let (vx, vy) = (vis.top_left.x, vis.top_left.y);
    x < vx + vis.size.width as i32
        && x + w as i32 > vx
        && y < vy + vis.size.height as i32
        && y + h as i32 > vy
}

/// Un rectángulo lleno, salteado si no se ve.
pub fn rect<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    vis: &Rectangle,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    c: Rgb565,
) -> Result<(), D::Error> {
    if w == 0 || h == 0 || !touches(vis, x, y, w, h) {
        return Ok(());
    }
    d.fill_solid(&Rectangle::new(Point::new(x, y), Size::new(w, h)), c)
}

/// Un panel con las esquinas recortadas y borde.
#[allow(clippy::too_many_arguments)]
pub fn panel<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    vis: &Rectangle,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    fill: Rgb565,
    border: Rgb565,
) -> Result<(), D::Error> {
    if !touches(vis, x, y, w, h) {
        return Ok(());
    }
    let (wi, hi) = (w as i32, h as i32);
    // Borde de 2 px con esquinas redondeadas a lo pixel art.
    rect(d, vis, x + 2, y, w - 4, 2, border)?;
    rect(d, vis, x + 2, y + hi - 2, w - 4, 2, border)?;
    rect(d, vis, x, y + 2, 2, h - 4, border)?;
    rect(d, vis, x + wi - 2, y + 2, 2, h - 4, border)?;
    rect(d, vis, x + 1, y + 1, 2, 2, border)?;
    rect(d, vis, x + wi - 3, y + 1, 2, 2, border)?;
    rect(d, vis, x + 1, y + hi - 3, 2, 2, border)?;
    rect(d, vis, x + wi - 3, y + hi - 3, 2, 2, border)?;
    rect(d, vis, x + 2, y + 2, w - 4, h - 4, fill)
}

/// Un sprite de pixel art: una fila por `&str`, un carácter por pixel
/// (`.` es transparente) y `pal` traduce cada carácter a un color. Se dibuja
/// por tramos del mismo color, `scale` veces más grande y espejado si `flip`.
#[allow(clippy::too_many_arguments)]
pub fn sprite<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    vis: &Rectangle,
    rows: &[&str],
    x: i32,
    y: i32,
    flip: bool,
    scale: u32,
    pal: fn(u8) -> Rgb565,
) -> Result<(), D::Error> {
    let w = rows.first().map_or(0, |r| r.len()) as i32;
    let s = scale as i32;
    if !touches(vis, x, y, w as u32 * scale, rows.len() as u32 * scale) {
        return Ok(());
    }
    let (top, bottom) = (vis.top_left.y, vis.top_left.y + vis.size.height as i32);
    for (j, row) in rows.iter().enumerate() {
        let ry = y + j as i32 * s;
        if ry + s <= top || ry >= bottom {
            continue;
        }
        let b = row.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let ch = b[i];
            let start = i;
            while i < b.len() && b[i] == ch {
                i += 1;
            }
            if ch == b'.' {
                continue;
            }
            let (a, z) = if flip {
                (w - i as i32, w - start as i32)
            } else {
                (start as i32, i as i32)
            };
            d.fill_solid(
                &Rectangle::new(
                    Point::new(x + a * s, ry),
                    Size::new(((z - a) * s) as u32, scale),
                ),
                pal(ch),
            )?;
        }
    }
    Ok(())
}

/// Texto con las mayúsculas arriba en `y` y el borde izquierdo en `x`
/// (`Font::draw` lo saltea si no toca la franja).
pub fn text<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    s: &str,
    x: i32,
    y: i32,
    font: &Font,
    c: Rgb565,
) -> Result<(), D::Error> {
    font.draw(d, s, x, y, Align::Left, c)
}

/// Texto centrado en `cx`, con sombra si `shadow` tiene color.
#[allow(clippy::too_many_arguments)]
pub fn text_c<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    vis: &Rectangle,
    s: &str,
    cx: i32,
    y: i32,
    font: &Font,
    c: Rgb565,
    shadow: Option<Rgb565>,
) -> Result<(), D::Error> {
    // Medir el ancho cuesta: solo si se ve.
    let (top, bottom) = font.rows(y);
    if !touches(vis, 0, top, 240, (bottom - top + 1) as u32) {
        return Ok(());
    }
    let x = cx - font.width(s) / 2;
    if let Some(sh) = shadow {
        font.draw(d, s, x + 1, y + 1, Align::Left, sh)?;
    }
    font.draw(d, s, x, y, Align::Left, c)
}

/// Texto armado en la pila, sin memoria dinámica.
pub struct Line {
    buf: [u8; 40],
    len: usize,
}

impl Line {
    pub fn new() -> Self {
        Line {
            buf: [0; 40],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        // Solo se escribe con `write_str`, que copia `&str` enteros.
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        if self.len + b.len() > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[self.len..self.len + b.len()].copy_from_slice(b);
        self.len += b.len();
        Ok(())
    }
}
