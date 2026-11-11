//! Dibujar por franjas: la placa no tiene memoria para la pantalla entera
//! (240×240×2 = 112 KB), así que se dibuja todo una vez por franja de 240×32
//! y cada franja se manda a la pantalla apenas está lista.

use core::convert::Infallible;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

use crate::SIZE;

const W: usize = SIZE as usize;

/// Un `DrawTarget` de 240×240 que guarda solo las filas `y0..y0 + rows` y
/// descarta el resto. Su `bounding_box()` es la franja: quien dibuja puede
/// saltear lo que no la toca (los juegos lo hacen).
pub struct Band<'a> {
    buf: &'a mut [u16],
    y0: i32,
    rows: i32,
    /// Bytes al revés (big-endian en memoria), como los quiere la ST7789.
    swap: bool,
}

impl<'a> Band<'a> {
    pub fn new(buf: &'a mut [u16], y0: i32, rows: i32, swap: bool) -> Self {
        let rows = rows.min((buf.len() / W) as i32);
        Self {
            buf,
            y0,
            rows,
            swap,
        }
    }

    fn encode(&self, c: Rgb565) -> u16 {
        let raw = c.into_storage();
        if self.swap { raw.swap_bytes() } else { raw }
    }

    fn area(&self) -> Rectangle {
        Rectangle::new(Point::new(0, self.y0), Size::new(SIZE, self.rows as u32))
    }
}

impl Dimensions for Band<'_> {
    fn bounding_box(&self) -> Rectangle {
        self.area()
    }
}

impl DrawTarget for Band<'_> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        for Pixel(p, c) in pixels {
            let y = p.y - self.y0;
            if (0..W as i32).contains(&p.x) && (0..self.rows).contains(&y) {
                let v = self.encode(c);
                self.buf[y as usize * W + p.x as usize] = v;
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Infallible> {
        let r = area.intersection(&self.area());
        let Some(br) = r.bottom_right() else {
            return Ok(());
        };
        let v = self.encode(color);
        for y in r.top_left.y..=br.y {
            let row = (y - self.y0) as usize * W;
            self.buf[row + r.top_left.x as usize..=row + br.x as usize].fill(v);
        }
        Ok(())
    }

    fn clear(&mut self, color: Rgb565) -> Result<(), Infallible> {
        let v = self.encode(color);
        self.buf[..self.rows as usize * W].fill(v);
        Ok(())
    }
}

/// Dibuja la pantalla entera franja por franja: `draw` dibuja todo (lo que
/// cae fuera de la franja se descarta) y `push` recibe cada franja lista
/// con su primera fila, su alto y sus píxeles.
pub fn render(
    buf: &mut [u16],
    swap: bool,
    mut draw: impl FnMut(&mut Band),
    mut push: impl FnMut(usize, usize, &[u16]),
) {
    let rows = (buf.len() / W).min(W);
    assert!(rows > 0, "la franja tiene que tener al menos una fila");
    let mut y0 = 0;
    while y0 < W {
        let n = rows.min(W - y0);
        {
            let mut band = Band::new(buf, y0 as i32, n as i32, swap);
            draw(&mut band);
        }
        push(y0, n, &buf[..n * W]);
        y0 += n;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::primitives::{Circle, PrimitiveStyle};

    fn scene(d: &mut Band) {
        d.clear(Rgb565::BLUE).unwrap();
        Circle::new(Point::new(20, 10), 90)
            .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
            .draw(d)
            .unwrap();
        d.fill_solid(
            &Rectangle::new(Point::new(-10, 25), Size::new(300, 50)),
            Rgb565::GREEN,
        )
        .unwrap();
    }

    #[test]
    fn bands_add_up_to_the_whole_screen() {
        let mut full = std::vec![0u16; W * W];
        render(&mut full, false, scene, |_, _, _| {});
        let mut out = std::vec![0u16; W * W];
        let mut band = std::vec![0u16; W * 32];
        let mut calls = 0;
        render(&mut band, false, scene, |y0, rows, px| {
            out[y0 * W..(y0 + rows) * W].copy_from_slice(px);
            calls += 1;
        });
        assert_eq!(calls, 8);
        assert_eq!(out, full);
        assert_eq!(full[30 * W], Rgb565::GREEN.into_storage());
    }

    #[test]
    fn bounding_box_is_the_band() {
        let mut buf = std::vec![0u16; W * 32];
        let b = Band::new(&mut buf, 64, 32, false);
        assert_eq!(
            b.bounding_box(),
            Rectangle::new(Point::new(0, 64), Size::new(240, 32))
        );
    }

    #[test]
    fn swapped_bytes_for_the_panel() {
        let mut buf = std::vec![0u16; W];
        let mut b = Band::new(&mut buf, 0, 1, true);
        b.clear(Rgb565::RED).unwrap();
        assert_eq!(buf[0], Rgb565::RED.into_storage().swap_bytes());
    }
}
