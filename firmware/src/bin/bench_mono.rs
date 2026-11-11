#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

//! Benchmark del personaje exportado de Blender (el mono) en el ESP32-C3:
//! cuerpo entero (como en el cuadro) y primer plano de la cara, con la
//! cabeza girando y las expresiones cambiando. Solo la SuperMini por USB-C.
//!
//! ```text
//! cd firmware
//! cargo run --release --bin bench_mono
//! ```

use assets::mono::MONO;
use assets::scene::{self, CharacterScene, Pose};
use engine3d::fixed::{Mat3, Transform, deg, sin};
use engine3d::raster::fill16;
use engine3d::{Band, Camera, PixelOrder, Renderer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::Instant;
use esp_println::println;
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

const W: usize = 240;
const H: usize = 240;
const BAND: usize = 48;
const BAND_PIXELS: usize = W * BAND;
type R = Renderer<1024, 1536>;

static RENDERER: StaticCell<R> = StaticCell::new();
static POSE: StaticCell<Pose> = StaticCell::new();
static COLOR: StaticCell<[u16; BAND_PIXELS]> = StaticCell::new();
static DEPTH: StaticCell<[u16; BAND_PIXELS]> = StaticCell::new();

const FRAMES: u32 = 60;

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let _peripherals = esp_hal::init(config);

    engine3d::raster::cycles::enable();
    let r = RENDERER.init(R::new(
        Camera::new(W as i32, H as i32, 300, 128),
        scene::light(),
    ));
    let scene = CharacterScene::new(&MONO);
    scene.setup(r, PixelOrder::Swapped);
    let pose = POSE.init(Pose::REST);
    let head = MONO.bone(":Head").unwrap_or(0);
    let color = COLOR.init([0; BAND_PIXELS]);
    let depth = DEPTH.init([0; BAND_PIXELS]);

    println!(
        "\n=== Benchmark personaje · ESP32-C3 · {W}x{H} · {} vértices, {} triángulos, {} huesos ===",
        MONO.mesh.vertex_count(),
        MONO.mesh.tri_count(),
        MONO.bones.len()
    );

    loop {
        for (name, center_y, dist) in [("cuerpo entero", 623, 2200), ("primer plano", 720, 700)] {
            let (mut t_submit, mut t_raster) = (0u64, 0u64);
            let total = Instant::now();
            for f in 0..FRAMES {
                r.begin_frame();
                scene.set_face(r, (f / 10 % 6) as u8, (f / 7 % 6) as u8);
                pose.rot[head] = Mat3::rot_y(deg((sin((f * 600) as u16) * 15) >> 14));
                let model = Transform::new(Mat3::rot_x(deg(-4)), [0, -center_y, dist]);
                let t0 = Instant::now();
                scene.submit(r, &model, pose);
                t_submit += t0.elapsed().as_micros();

                let t1 = Instant::now();
                for y0 in (0..H).step_by(BAND) {
                    let mut band = Band {
                        color: &mut color[..],
                        depth: &mut depth[..],
                        x0: 0,
                        width: W,
                        y0,
                        height: BAND,
                    };
                    fill16(band.color, 0x3186);
                    if r.touches_rows(y0, y0 + BAND) {
                        r.clear_depth(&mut band);
                        r.render_band(&mut band);
                    }
                }
                t_raster += t1.elapsed().as_micros();
            }
            let per = |us: u64| us as f32 / FRAMES as f32 / 1000.0;
            let s = r.stats;
            println!(
                "{name}: {:.2} ms/fotograma · transformar+luz {:.2} ms · fondo+rasterizado {:.2} ms",
                per(total.elapsed().as_micros()),
                per(t_submit),
                per(t_raster)
            );
            println!(
                "  {} triángulos visibles, {} píxeles ({} con textura)  [chk {}]",
                s.tris_visible,
                s.raster.pixels,
                s.raster.tex_pixels,
                color[BAND_PIXELS / 2]
            );
            let rs = s.raster;
            if rs.cyc_pixels > 0 {
                let per = |c: u32, n: u32| c as f32 / n.max(1) as f32;
                println!(
                    "  ciclos: {:.0}/triángulo · {:.0}/tramo · {:.1}/píxel ({:.1} con textura)",
                    per(rs.cyc_setup, rs.setups),
                    per(rs.cyc_rows, rs.spans),
                    per(rs.cyc_pixels, rs.pixels),
                    per(rs.cyc_tex_pixels, rs.tex_pixels),
                );
            }
        }
    }
}
