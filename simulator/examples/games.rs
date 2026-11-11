//! Los minijuegos en la PC, con el mismo código que corre en la placa
//! (crate `games`).
//!
//! cargo run -p simulator --release --example games              # jugar
//! cargo run -p simulator --release --example games -- --png dir # capturas
//!
//! Teclas: A / S / D = botones A / B / C. También ← y → para caminar y
//! espacio o ↑ para saltar y trepar. Esc sale.
//!
//! Cada cuadro se dibuja como en la placa, en franjas de 240×30 con un
//! `DrawTarget` que recorta y desplaza. Al arrancar se compara una vez contra
//! el dibujo de la pantalla entera: tienen que dar la misma imagen.

use std::convert::Infallible;
use std::time::Instant;

use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use games::{GameId, H, Keys, Running, Status, W};
use minifb::{Key, Scale, Window, WindowOptions};

const BAND: u32 = 30;
const SEED: u32 = 0x000C_0C05;

/// La pantalla entera.
struct Framebuffer(Vec<Rgb565>);

impl Framebuffer {
    fn new() -> Self {
        Framebuffer(vec![Rgb565::BLACK; (W * H) as usize])
    }
}

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(W, H)
    }
}

impl DrawTarget for Framebuffer {
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

/// Una franja de 240×30 como la de la placa: su propio búfer, que empieza en
/// la fila `y0` de la pantalla. Lo de afuera se descarta.
struct Band {
    y0: i32,
    buf: Vec<Rgb565>,
    fills: u32,
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
        if let Some(br) = a.bottom_right() {
            for y in a.top_left.y..=br.y {
                let row = ((y - self.y0) as u32 * W) as usize;
                self.buf[row + a.top_left.x as usize..=row + br.x as usize].fill(color);
            }
        }
        Ok(())
    }
}

/// Dibuja por franjas, como el firmware. Devuelve cuántos rectángulos pidió.
fn draw_banded(game: &Running, fb: &mut Framebuffer) -> u32 {
    let mut fills = 0;
    let mut band = Band {
        y0: 0,
        buf: vec![Rgb565::BLACK; (W * BAND) as usize],
        fills: 0,
    };
    for y0 in (0..H).step_by(BAND as usize) {
        band.y0 = y0 as i32;
        band.fills = 0;
        game.draw(&mut band).unwrap();
        fills += band.fills;
        let start = (y0 * W) as usize;
        fb.0[start..start + band.buf.len()].copy_from_slice(&band.buf);
    }
    fills
}

/// Compara el dibujo por franjas con el de la pantalla entera.
fn check_bands(game: &Running) {
    let mut full = Framebuffer::new();
    game.draw(&mut full).unwrap();
    let mut banded = Framebuffer::new();
    let fills = draw_banded(game, &mut banded);
    let diff = full.0.iter().zip(&banded.0).filter(|(a, b)| a != b).count();
    if diff == 0 {
        println!("franjas: misma imagen que la pantalla entera ({fills} rectángulos)");
    } else {
        eprintln!("franjas: {diff} pixeles distintos de la pantalla entera");
        std::process::exit(1);
    }
}

fn save_png(path: &std::path::Path, fb: &Framebuffer, scale: u32) {
    let (w, h) = (W * scale, H * scale);
    let mut data = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = fb.0[((y / scale) * W + x / scale) as usize];
            data.extend([
                c.r() << 3 | c.r() >> 2,
                c.g() << 2 | c.g() >> 4,
                c.b() << 3 | c.b() >> 2,
            ]);
        }
    }
    let file = std::fs::File::create(path).expect("no se pudo crear el PNG");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&data).unwrap();
}

const NONE: Keys = Keys {
    a: false,
    b: false,
    c: false,
};
const A: Keys = Keys { a: true, ..NONE };
const B: Keys = Keys { b: true, ..NONE };
const C: Keys = Keys { c: true, ..NONE };

/// Un paso del guion: botones durante tantos milisegundos, y si al final se
/// guarda una captura con ese nombre.
type Cue = (Keys, u32, Option<&'static str>);

/// Partida fija (misma semilla, mismos botones): portada, el mono trepa la
/// primera escalera, cruza la segunda tabla entre cocos, sube a la tercera y
/// después se queda al pie de la torre hasta perder las tres vidas.
const SCRIPT: &[Cue] = &[
    (NONE, 1400, Some("1_portada")),
    (C, 100, None),
    (NONE, 100, None),
    (A, 2650, None),
    (B, 1000, None),
    (C, 2330, None),
    (B, 450, Some("2_trepando")),
    (B, 600, None),
    (A, 1500, Some("3_jugando")),
];

/// Después, a perder: contra la pared de la izquierda le llueven cocos.
const LOSE: &[Cue] = &[(A, 4000, None), (NONE, 20_000, None)];

fn export(dir: &str) {
    std::fs::create_dir_all(dir).expect("no se pudo crear la carpeta");
    let mut game = Running::new(GameId::Cocos, SEED);
    let mut fb = Framebuffer::new();
    let shot = |name: &str, game: &Running, fb: &mut Framebuffer| {
        draw_banded(game, fb);
        let path = std::path::Path::new(dir).join(format!("{name}.png"));
        save_png(&path, fb, 2);
        println!("{}", path.display());
    };
    check_bands(&game);
    for &(keys, ms, name) in SCRIPT {
        let mut left = ms;
        while left > 0 {
            let dt = left.min(33);
            game.update(keys, dt);
            left -= dt;
        }
        if let Some(name) = name {
            shot(name, &game, &mut fb);
            check_bands(&game);
        }
    }
    for _ in 0..8 {
        for &(keys, ms, _) in LOSE {
            let mut left = ms;
            while left > 0 {
                let dt = left.min(33);
                game.update(keys, dt);
                left -= dt;
            }
        }
    }
    shot("4_fin", &game, &mut fb);
    check_bands(&game);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--png") {
        export(args.get(i + 1).map_or("juegos", String::as_str));
        return;
    }

    let mut game = Running::new(
        GameId::Cocos,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(SEED, |d| d.subsec_nanos()),
    );
    check_bands(&game);
    let mut window = Window::new(
        "Cocos Rodantes (A/S/D = botones, ←/→ caminar, espacio/↑ saltar, Esc salir)",
        W as usize,
        H as usize,
        WindowOptions {
            scale: Scale::X2,
            ..WindowOptions::default()
        },
    )
    .expect("no se pudo abrir la ventana");
    window.set_target_fps(60);
    let mut fb = Framebuffer::new();
    let mut px = vec![0u32; (W * H) as usize];
    let mut last = Instant::now();
    let mut slowest = 0u128;
    let mut frames = 0u32;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let dt = last.elapsed().as_millis() as u32;
        last = Instant::now();
        let down = |ks: &[Key]| ks.iter().any(|&k| window.is_key_down(k));
        let keys = Keys {
            a: down(&[Key::A, Key::Left]),
            b: down(&[Key::S, Key::Space, Key::Up]),
            c: down(&[Key::D, Key::Right]),
        };
        let t0 = Instant::now();
        if game.update(keys, dt) == Status::Exit {
            println!("salir (en la placa vuelve al menú)");
            break;
        }
        draw_banded(&game, &mut fb);
        slowest = slowest.max(t0.elapsed().as_micros());
        frames += 1;
        if frames.is_multiple_of(300) {
            println!("cuadro más lento de los últimos 300 (PC): {slowest} µs");
            slowest = 0;
        }
        for (d, c) in px.iter_mut().zip(&fb.0) {
            *d = (c.r() as u32) << 19 | (c.g() as u32) << 10 | (c.b() as u32) << 3;
        }
        window
            .update_with_buffer(&px, W as usize, H as usize)
            .unwrap();
    }
}
