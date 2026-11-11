//! Demo del motor 3D a 240×240, dibujando por franjas igual que el firmware.
//!
//! ```text
//! cargo run -p simulator --release --example engine_demo              # ventana
//! cargo run -p simulator --release --example engine_demo -- --png f.png
//! cargo run -p simulator --release --example engine_demo -- --bench
//! ```
//!
//! Teclas: ←/→ girar, Espacio pausa, F sombreado plano/suave, Esc salir.
//! Los tiempos de PC NO son los del ESP32; sirven para comparar cambios.
//! Para números reales: `cd firmware && cargo run --release --bin bench`.

use std::time::Instant;

use engine3d::demo::{self, BAND_PIXELS, DemoRenderer, HEIGHT, PetScene, WIDTH};
use engine3d::fixed::deg;
use engine3d::{Angle, PixelOrder};
use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};

fn rgb565_to_rgb(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 0x1F) as u8;
    let g = ((c >> 5) & 0x3F) as u8;
    let b = (c & 0x1F) as u8;
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

struct Frame {
    rgb: Vec<[u8; 3]>,
}

impl Frame {
    fn new() -> Self {
        Self {
            rgb: vec![[0; 3]; WIDTH * HEIGHT],
        }
    }
}

fn draw(scene: &PetScene, r: &mut DemoRenderer, t: u32, yaw: Angle, frame: &mut Frame) {
    let mut color = [0u16; BAND_PIXELS];
    let mut depth = [0u16; BAND_PIXELS];
    demo::render_frame(scene, r, t, yaw, &mut color, &mut depth, |band| {
        let start = band.y0 * WIDTH;
        for (dst, &c) in frame.rgb[start..start + band.height * WIDTH]
            .iter_mut()
            .zip(band.color.iter())
        {
            *dst = rgb565_to_rgb(c);
        }
    });
}

fn setup() -> (PetScene, Box<DemoRenderer>) {
    let scene = PetScene::new(PixelOrder::Native);
    let mut r = Box::new(DemoRenderer::new(demo::camera(), demo::light()));
    scene.setup(&mut r);
    (scene, r)
}

fn save_png(path: &str, frame: &Frame) {
    let file = std::fs::File::create(path).expect("no se pudo crear el PNG");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), WIDTH as u32, HEIGHT as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = frame.rgb.iter().flatten().copied().collect();
    enc.write_header().unwrap().write_image_data(&data).unwrap();
}

fn bench() {
    let (scene, mut r) = setup();
    let mut frame = Frame::new();
    const N: u32 = 300;
    let start = Instant::now();
    for i in 0..N {
        draw(&scene, &mut r, i * 33, 0, &mut frame);
    }
    let per = start.elapsed().as_secs_f64() * 1000.0 / N as f64;
    let s = r.stats;
    println!("PC: {per:.3} ms/fotograma (solo orientativo)");
    println!(
        "vértices {}, triángulos {} (descartados {}, visibles {}), tramos {}, píxeles evaluados {}",
        s.verts, s.tris_in, s.tris_culled, s.tris_visible, s.raster.spans, s.raster.pixels
    );
    println!("zona ocupada: {:?}", r.bounds());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--bench") {
        return bench();
    }
    if let Some(i) = args.iter().position(|a| a == "--png") {
        let path = args.get(i + 1).map(String::as_str).unwrap_or("frame.png");
        let t: u32 = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(500);
        let (scene, mut r) = setup();
        let mut frame = Frame::new();
        draw(&scene, &mut r, t, 0, &mut frame);
        save_png(path, &frame);
        println!("guardado {path} (t = {t} ms), {:?}", r.stats);
        return;
    }

    let (scene, mut r) = setup();
    let mut frame = Frame::new();
    let mut window = Window::new(
        "Motor 3D (←/→ girar, Espacio pausa, F plano/suave)",
        WIDTH,
        HEIGHT,
        WindowOptions {
            scale: Scale::X2,
            ..WindowOptions::default()
        },
    )
    .expect("no se pudo abrir la ventana");
    window.set_target_fps(30);

    let mut buf = vec![0u32; WIDTH * HEIGHT];
    let mut t = 0u32;
    let mut yaw: Angle = 0;
    let mut paused = false;
    let mut last = Instant::now();
    let (mut acc_ms, mut frames) = (0.0f64, 0u32);

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let dt = last.elapsed().as_millis() as u32;
        last = Instant::now();
        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            paused = !paused;
        }
        if window.is_key_pressed(Key::F, KeyRepeat::No) {
            for m in r.materials.iter_mut() {
                m.flat = !m.flat;
            }
        }
        if window.is_key_down(Key::Left) {
            yaw = yaw.wrapping_add(deg(3));
        }
        if window.is_key_down(Key::Right) {
            yaw = yaw.wrapping_sub(deg(3));
        }
        if !paused {
            t = t.wrapping_add(dt);
        }

        let start = Instant::now();
        draw(&scene, &mut r, t, yaw, &mut frame);
        acc_ms += start.elapsed().as_secs_f64() * 1000.0;
        frames += 1;
        if frames == 60 {
            let s = r.stats;
            window.set_title(&format!(
                "Motor 3D · PC {:.2} ms · {} tris visibles · {} px",
                acc_ms / frames as f64,
                s.tris_visible,
                s.raster.pixels
            ));
            (acc_ms, frames) = (0.0, 0);
        }

        for (dst, [r, g, b]) in buf.iter_mut().zip(frame.rgb.iter()) {
            *dst = (*r as u32) << 16 | (*g as u32) << 8 | *b as u32;
        }
        window.update_with_buffer(&buf, WIDTH, HEIGHT).unwrap();
    }
}
