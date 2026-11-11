//! Dibujar por franjas (como la placa) tiene que dar la misma imagen que
//! dibujar la pantalla entera, en todas las pantallas del juego.

use std::convert::Infallible;

use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use games::{GameId, H, Keys, Running, Status, W};

const BAND: u32 = 30;

struct Full(Vec<Rgb565>);

impl OriginDimensions for Full {
    fn size(&self) -> Size {
        Size::new(W, H)
    }
}

impl DrawTarget for Full {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        for Pixel(p, c) in pixels {
            if (0..W as i32).contains(&p.x) && (0..H as i32).contains(&p.y) {
                self.0[(p.y as u32 * W + p.x as u32) as usize] = c;
            }
        }
        Ok(())
    }
}

/// Una franja con su propio búfer, que empieza en la fila `y0`. Cuenta lo
/// que se le pide dibujar, para medir el costo.
struct Band {
    y0: i32,
    buf: Vec<Rgb565>,
    fills: u32,
    area: u32,
    pixels: u32,
}

impl Dimensions for Band {
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(Point::new(0, self.y0), Size::new(W, BAND))
    }
}

impl DrawTarget for Band {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        for Pixel(p, c) in pixels {
            self.pixels += 1;
            let y = p.y - self.y0;
            if (0..W as i32).contains(&p.x) && (0..BAND as i32).contains(&y) {
                self.buf[(y as u32 * W + p.x as u32) as usize] = c;
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Infallible> {
        self.fills += 1;
        let a = area.intersection(&self.bounding_box());
        self.area += a.size.width * a.size.height;
        if let Some(br) = a.bottom_right() {
            for y in a.top_left.y..=br.y {
                let row = ((y - self.y0) as u32 * W) as usize;
                self.buf[row + a.top_left.x as usize..=row + br.x as usize].fill(color);
            }
        }
        Ok(())
    }
}

/// Devuelve (pixeles distintos, rectángulos, área llenada, pixeles sueltos).
fn compare(game: &Running) -> (usize, u32, u32, u32) {
    let mut full = Full(vec![Rgb565::BLACK; (W * H) as usize]);
    game.draw(&mut full).unwrap();
    let mut banded = vec![Rgb565::BLACK; (W * H) as usize];
    let mut band = Band {
        y0: 0,
        buf: vec![Rgb565::CSS_HOT_PINK; (W * BAND) as usize],
        fills: 0,
        area: 0,
        pixels: 0,
    };
    for y0 in (0..H).step_by(BAND as usize) {
        band.y0 = y0 as i32;
        // Basura de la franja anterior: el juego tiene que pintar todo.
        band.buf.fill(Rgb565::CSS_HOT_PINK);
        game.draw(&mut band).unwrap();
        let start = (y0 * W) as usize;
        banded[start..start + band.buf.len()].copy_from_slice(&band.buf);
    }
    let diff = full.0.iter().zip(&banded).filter(|(a, b)| a != b).count();
    (diff, band.fills, band.area, band.pixels)
}

fn run(g: &mut Running, keys: Keys, ms: u32) {
    let mut left = ms;
    while left > 0 {
        let dt = left.min(33);
        assert_eq!(g.update(keys, dt), Status::Playing);
        left -= dt;
    }
}

#[test]
fn banded_draw_matches_full_draw() {
    let none = Keys::default();
    let a = Keys { a: true, ..none };
    let b = Keys { b: true, ..none };
    let c = Keys { c: true, ..none };
    let mut g = Running::new(GameId::Cocos, 42);
    let check = |g: &Running, what: &str| {
        let (diff, fills, area, pixels) = compare(g);
        assert_eq!(diff, 0, "{what}: {diff} pixeles distintos");
        // Para tener idea del costo en la placa (todas las franjas juntas).
        println!("{what}: {fills} rectángulos, {area} px llenados, {pixels} px sueltos");
        // Que no se vuelva caro sin darse cuenta.
        assert!(fills < 2500, "{what}: {fills} rectángulos");
        assert!(area < 3 * W * H, "{what}: {area} px");
    };
    for t in [0, 700, 1500] {
        run(&mut g, none, t);
        check(&g, "portada");
    }
    run(&mut g, c, 50);
    run(&mut g, none, 500);
    check(&g, "nivel 1");
    run(&mut g, a, 2650);
    run(&mut g, b, 500);
    check(&g, "trepando");
    run(&mut g, b, 600);
    for _ in 0..10 {
        run(&mut g, none, 1700);
        check(&g, "jugando");
    }
    // Contra la pared de la izquierda hasta perder (en algún momento).
    for _ in 0..12 {
        run(&mut g, a, 4000);
        check(&g, "perdiendo");
        run(&mut g, none, 16_000);
    }
    check(&g, "fin");
}
