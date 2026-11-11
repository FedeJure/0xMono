#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

//! Benchmark del motor 3D en el ESP32-C3. No necesita pantalla ni cables:
//! solo la SuperMini por USB-C.
//!
//! ```text
//! cd firmware
//! cargo run --release --bin bench
//! ```
//!
//! Dibuja la escena de demo completa (240×240, franjas de 24 filas) en RAM y
//! mide cada etapa. El envío por SPI al ST7789 no está incluido: se medirá
//! aparte cuando la pantalla esté conectada.

use engine3d::demo::{self, BAND_HEIGHT, BAND_PIXELS, DemoRenderer, HEIGHT, PetScene, WIDTH};
use engine3d::{Band, PixelOrder};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::Instant;
use esp_println::println;
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

static RENDERER: StaticCell<DemoRenderer> = StaticCell::new();
static SCENE: StaticCell<PetScene> = StaticCell::new();
static COLOR: StaticCell<[u16; BAND_PIXELS]> = StaticCell::new();
static DEPTH: StaticCell<[u16; BAND_PIXELS]> = StaticCell::new();

const FRAMES: u32 = 60;

#[derive(Default)]
struct Timing {
    submit: u64,
    background: u64,
    depth: u64,
    raster: u64,
}

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let _peripherals = esp_hal::init(config);

    engine3d::raster::cycles::enable();
    let scene = SCENE.init(PetScene::new(PixelOrder::Swapped));
    let r = RENDERER.init(DemoRenderer::new(demo::camera(), demo::light()));
    scene.setup(r);
    let color = COLOR.init([0; BAND_PIXELS]);
    let depth = DEPTH.init([0; BAND_PIXELS]);

    println!("\n=== Benchmark motor 3D · ESP32-C3 · {WIDTH}x{HEIGHT} ===");

    loop {
        let mut tm = Timing::default();
        let mut checksum = 0u32;
        let total = Instant::now();

        for f in 0..FRAMES {
            let t_ms = f * 33;
            r.begin_frame();
            let t0 = Instant::now();
            scene.submit(r, t_ms, 0);
            tm.submit += t0.elapsed().as_micros();
            let backdrop = scene.backdrop(t_ms);

            for y0 in (0..HEIGHT).step_by(BAND_HEIGHT) {
                let mut band = Band {
                    color: &mut color[..],
                    depth: &mut depth[..],
                    x0: 0,
                    width: WIDTH,
                    y0,
                    height: BAND_HEIGHT,
                };
                let t1 = Instant::now();
                scene.fill_background(&mut band, &backdrop);
                tm.background += t1.elapsed().as_micros();

                if r.touches_rows(band.y0, band.y1()) {
                    let t2 = Instant::now();
                    r.clear_depth(&mut band);
                    tm.depth += t2.elapsed().as_micros();

                    let t3 = Instant::now();
                    r.render_band(&mut band);
                    tm.raster += t3.elapsed().as_micros();
                }

                // Evita que el compilador descarte el trabajo.
                checksum = checksum.wrapping_add(band.color[BAND_PIXELS / 2] as u32);
            }
        }

        let total_us = total.elapsed().as_micros();
        let per = |us: u64| us as f32 / FRAMES as f32 / 1000.0;
        let s = r.stats;
        println!(
            "fotograma completo: {:.2} ms  ({:.1} fps sin contar SPI)",
            per(total_us),
            1_000_000.0 / (total_us as f32 / FRAMES as f32)
        );
        println!("  transformar + luz: {:.2} ms", per(tm.submit));
        println!("  fondo:             {:.2} ms", per(tm.background));
        println!("  limpiar z-buffer:  {:.2} ms", per(tm.depth));
        println!("  rasterizado:       {:.2} ms", per(tm.raster));
        println!(
            "  escena: {} vértices, {} triángulos visibles de {}, {} preparaciones, {} tramos, {} píxeles  [chk {checksum}]",
            s.verts, s.tris_visible, s.tris_in, s.raster.setups, s.raster.spans, s.raster.pixels
        );
        let rs = s.raster;
        if rs.cyc_pixels > 0 {
            let per = |c: u32, n: u32| c as f32 / n.max(1) as f32;
            println!(
                "  ciclos/fotograma: preparación {} ({:.0}/triángulo) · filas {} ({:.0}/tramo) · píxeles {} ({:.1}/píxel)",
                rs.cyc_setup,
                per(rs.cyc_setup, rs.setups),
                rs.cyc_rows,
                per(rs.cyc_rows, rs.spans),
                rs.cyc_pixels,
                per(rs.cyc_pixels, rs.pixels),
            );
            println!(
                "  con textura: {} píxeles, {:.1} ciclos/píxel · sin textura: {:.1} ciclos/píxel",
                rs.tex_pixels,
                per(rs.cyc_tex_pixels, rs.tex_pixels),
                per(
                    rs.cyc_pixels.wrapping_sub(rs.cyc_tex_pixels),
                    rs.pixels - rs.tex_pixels
                ),
            );
        }
        println!("  zona ocupada: {:?}", r.bounds());
    }
}
