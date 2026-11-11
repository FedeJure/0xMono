//! Las escenas de medición del hábitat (`assets::shots`) en la PC: cuánto
//! ocupa cada una en los búferes del renderer (para dimensionarlos en el
//! ESP32), cuánto se redibuja con la cámara quieta, y una imagen de cada una.
//!
//! ```text
//! cargo run -p simulator --release --example habitat_bench [carpeta]
//! ```

use assets::house::HOUSE;
use assets::mono::MONO;
use assets::scene::{self, CharacterScene};
use assets::shots::{SHOTS, ShotPlayer};
use assets::stage::Stage;
use assets::world::WorldScene;
use engine3d::{Camera, PixelOrder, Renderer, Rgb};

const W: usize = 240;
const FRAMES: u32 = 90;
const DT_MS: u32 = 66;
type R = Renderer<4608, 2816>;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "shots".into());
    std::fs::create_dir_all(&dir).expect("no se pudo crear la carpeta");
    let mut r: Box<R> = Box::new(R::new(Camera::new(240, 240, 300, 96), scene::light()));
    let base = (MONO.colors.len() + MONO.parts.len()) as u8;
    let mut stage = Stage::new(CharacterScene::new(&MONO), WorldScene::new(&HOUSE, base));
    stage.setup(&mut r, PixelOrder::Native);
    let (top, horizon) = (Rgb(120, 178, 232), Rgb(210, 232, 244));
    for (y, s) in stage.sky.iter_mut().enumerate() {
        *s = top.lerp(horizon, y as u32, W as u32).to_565();
    }

    let mut screen = vec![0u16; W * W];
    let (mut color, mut depth) = (vec![0u16; W * 48], vec![0u16; W * 48]);
    println!(
        "{:<12} {:>6} {:>6} {:>6} {:>7} {:>7} {:>8}",
        "escena", "verts", "tris", "envia", "px", "zona%", "px zona"
    );
    let (mut max_v, mut max_t) = (0, 0);
    for shot in SHOTS {
        let mut player = ShotPlayer::new(shot, &mut stage);
        let (mut mv, mut mt, mut sent, mut px) = (0, 0, 0, 0);
        let (mut area_sum, mut px_part, mut dropped) = (0, 0, 0);
        for f in 0..FRAMES {
            let area = player.frame(&mut stage, &mut r, DT_MS);
            stage.draw(&mut r, &mut color, &mut depth, |b| {
                for row in 0..b.height {
                    let dst = (b.y0 + row) * W + b.x0;
                    screen[dst..dst + b.width]
                        .copy_from_slice(&b.color[row * b.width..][..b.width]);
                }
            });
            dropped += r.stats.dropped;
            let (v, t) = r.usage();
            mv = mv.max(v);
            mt = mt.max(t);
            if f == 0 {
                sent = r.stats.tris_in;
                px = r.stats.raster.pixels;
            } else {
                area_sum += area.area();
                px_part += r.stats.raster.pixels;
            }
        }
        max_v = max_v.max(mv);
        max_t = max_t.max(mt);
        let n = (FRAMES - 1) as i32;
        println!(
            "{:<12} {:>6} {:>6} {:>6} {:>6}k {:>6.1}% {:>7}k",
            shot.name,
            mv,
            mt,
            sent,
            px / 1000,
            area_sum as f32 / n as f32 / (W * W) as f32 * 100.0,
            px_part / n as u32 / 1000
        );
        if dropped > 0 {
            println!("  ¡{dropped} mallas no entraron en los búferes!");
        }
        save_png(
            &format!("{dir}/{}.png", shot.name.replace(' ', "_")),
            &screen,
        );
    }
    println!("máximo: {max_v} vértices, {max_t} triángulos guardados");
}

fn save_png(path: &str, img: &[u16]) {
    let file = std::fs::File::create(path).expect("no se pudo crear el PNG");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W as u32, W as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = img
        .iter()
        .flat_map(|&c| {
            let (r, g, b) = ((c >> 11) & 0x1F, (c >> 5) & 0x3F, c & 0x1F);
            [
                (r << 3 | r >> 2) as u8,
                (g << 2 | g >> 4) as u8,
                (b << 3 | b >> 2) as u8,
            ]
        })
        .collect();
    enc.write_header().unwrap().write_image_data(&data).unwrap();
}
