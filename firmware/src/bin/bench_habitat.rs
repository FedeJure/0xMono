#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

//! Benchmark del hábitat en el ESP32-C3: las escenas de `assets::shots`
//! (la casa, el jardín y el mono haciendo su vida), dibujando todo cada
//! fotograma y redibujando solo lo que cambia. Solo la SuperMini por USB-C.
//!
//! ```text
//! cd firmware
//! cargo run --release --bin bench_habitat
//! ```

use assets::anim::{AnimPose, Animator};
use assets::house::HOUSE;
use assets::mono::MONO;
use assets::mono_anims::{CLIPS, clip};
use assets::scene::{self, CharacterScene};
use assets::scene::{MAX_BONES, Pose};
use assets::shots::{SHOTS, ShotPlayer};
use assets::stage::Stage;
use assets::world::{Eye, WorldScene};
use engine3d::fixed::Transform;
use engine3d::{Bounds, Camera, PixelOrder, Renderer, Rgb};
use companion::{Activity, Companion, LocalTime, Personality};
use esp_backtrace as _;
use habitat::Habitat;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::Instant;
use esp_println::println;
use static_cell::{ConstStaticCell, StaticCell};

esp_bootloader_esp_idf::esp_app_desc!();

const W: usize = 240;
const BAND_PIXELS: usize = W * 32;
type R = Renderer<4608, 2816>;

// Se arma en tiempo de compilación: construirlo en la pila al arrancar la
// desborda (son ~150 KB).
static RENDERER: ConstStaticCell<R> =
    ConstStaticCell::new(R::new(Camera::new(240, 240, 300, 96), scene::light()));
static STAGE: StaticCell<Stage> = StaticCell::new();
static HABITAT: StaticCell<Habitat> = StaticCell::new();
static COLOR: ConstStaticCell<[u16; BAND_PIXELS]> = ConstStaticCell::new([0; BAND_PIXELS]);
static DEPTH: ConstStaticCell<[u16; BAND_PIXELS]> = ConstStaticCell::new([0; BAND_PIXELS]);

const FRAMES: u32 = 30;
const DT_MS: u32 = 66;

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let _peripherals = esp_hal::init(config);

    engine3d::raster::cycles::enable();
    let r = RENDERER.take();
    let base = (MONO.colors.len() + MONO.parts.len()) as u8;
    let stage = STAGE.init(Stage::new(
        CharacterScene::new(&MONO),
        WorldScene::new(&HOUSE, base),
    ));
    stage.setup(r, PixelOrder::Swapped);
    let (top, horizon) = (Rgb(120, 178, 232), Rgb(210, 232, 244));
    for (y, s) in stage.sky.iter_mut().enumerate() {
        *s = PixelOrder::Swapped.encode(top.lerp(horizon, y as u32, W as u32).to_565());
    }
    let color = COLOR.take();
    let depth = DEPTH.take();

    println!(
        "\n=== Hábitat · ESP32-C3 · 240x240 · {} objetos, {} triángulos + mono {} ===",
        HOUSE.objects.len(),
        HOUSE
            .objects
            .iter()
            .map(|o| o.mesh.tri_count())
            .sum::<usize>(),
        MONO.mesh.tri_count()
    );
    println!(
        "escena        modo      ms/fot  preparar  dibujar   zona  tris   px   (verts/tris guardados)"
    );

    profile(r, stage);
    life(r, color, depth);
    loop {
        for shot in SHOTS {
            for full in [true, false] {
                if !full && shot.walk_to.is_some() {
                    continue; // con la cámara moviéndose siempre es completo
                }
                let mut player = ShotPlayer::new(shot, stage);
                let (mut t_prep, mut t_draw, mut area, mut px, mut tris) =
                    (0u64, 0u64, 0u64, 0u32, 0u32);
                let (mut mv, mut mt) = (0, 0);
                // El primer fotograma siempre es completo: no se cuenta.
                player.frame(stage, r, DT_MS);
                stage.draw(r, color, depth, |_| {});
                for _ in 0..FRAMES {
                    if full {
                        stage.invalidate_all();
                    }
                    let t0 = Instant::now();
                    let a = player.frame(stage, r, DT_MS);
                    let t1 = Instant::now();
                    stage.draw(r, color, depth, |_| {});
                    t_prep += (t1 - t0).as_micros();
                    t_draw += t1.elapsed().as_micros();
                    area += a.area() as u64;
                    px += r.stats.raster.pixels;
                    tris += r.stats.tris_visible;
                    let (v, t) = r.usage();
                    mv = mv.max(v);
                    mt = mt.max(t);
                }
                let per = |us: u64| us as f32 / FRAMES as f32 / 1000.0;
                println!(
                    "{:<12} {:<9} {:>6.2}  {:>7.2}  {:>7.2}  {:>4.0}%  {:>4}  {:>4}k  ({mv}/{mt})",
                    shot.name,
                    if full { "completo" } else { "parcial" },
                    per(t_prep + t_draw),
                    per(t_prep),
                    per(t_draw),
                    area as f32 / FRAMES as f32 / (W * W) as f32 * 100.0,
                    tris / FRAMES,
                    px / FRAMES / 1000,
                );
                raster_cycles(r);
            }
        }
        println!("---");
    }
}

/// Cuánto cuesta cada paso de preparar al personaje (promedio de 30).
fn profile(r: &mut R, stage: &mut Stage) {
    const N: u32 = 30;
    let mut anim = Animator::new(&CLIPS[clip::SITTING_TYPING_COMPUTER]);
    let mut ap = AnimPose::REST;
    let mut pose = Pose::REST;
    let mut bones = [Transform::IDENTITY; MAX_BONES];
    let model = Transform::translate([0, -600, 2400]);
    let time = |name: &str, f: &mut dyn FnMut()| {
        let t = Instant::now();
        for _ in 0..N {
            f();
        }
        println!(
            "  {name:<28} {:>7.3} ms",
            t.elapsed().as_micros() as f32 / N as f32 / 1000.0
        );
    };
    time("anim.pose", &mut || {
        anim.update(66);
        anim.pose(&mut ap)
    });
    time("AnimPose::to_pose", &mut || ap.to_pose(&mut pose));
    time("bone_transforms", &mut || {
        stage.chr.bone_transforms(&model, &pose, &mut bones)
    });
    time("begin_frame", &mut || r.begin_frame());
    time("submit mono", &mut || {
        r.begin_frame();
        r.submit(&MONO.mesh, &bones[..MONO.bones.len()]);
    });
    let view = Eye::of(HOUSE.camera("cam_kitchen").unwrap()).view();
    time("Eye::of + view", &mut || {
        core::hint::black_box(Eye::of(HOUSE.camera("cam_kitchen").unwrap()).view());
    });
    time("mundo, recorte 10x10", &mut || {
        r.begin_frame();
        r.set_scissor(Bounds::new(100, 100, 110, 110));
        stage.world.submit(r, &view);
    });
    time("mundo, recorte vacío", &mut || {
        r.begin_frame();
        r.set_scissor(Bounds::new(0, 0, 0, 0));
        stage.world.submit(r, &view);
    });
    time("mundo, pantalla entera", &mut || {
        r.begin_frame();
        stage.world.submit(r, &view);
    });
    let s = r.stats;
    println!(
        "  ({} enviados, {} guardados, {} vértices)",
        s.tris_in, s.tris_visible, s.verts
    );
    println!(
        "  ciclos: {:.0}/vértice · {:.0}/triángulo",
        s.cyc_verts as f32 / s.verts.max(1) as f32,
        s.cyc_tris as f32 / s.tris_in.max(1) as f32
    );
}

/// Ciclos por etapa del rasterizado del último fotograma (feature `profile`).
fn raster_cycles(r: &R) {
    let rs = r.stats.raster;
    if rs.cyc_pixels > 0 {
        let per = |c: u32, n: u32| c as f32 / n.max(1) as f32;
        println!(
            "    ciclos: {:.0}/triángulo · {:.0}/tramo · {:.1}/píxel ({:.1} con textura) · {} preparaciones, {} tramos",
            per(rs.cyc_setup, rs.setups),
            per(rs.cyc_rows, rs.spans),
            per(rs.cyc_pixels, rs.pixels),
            per(rs.cyc_tex_pixels, rs.tex_pixels),
            rs.setups,
            rs.spans,
        );
    }
}

/// El hábitat de verdad (crate `habitat`, el mismo del simulador): la agenda,
/// los caminos, las sillas y lo que tiene en la mano. Para cada actividad,
/// llega, se acomoda y la hace durante un rato.
fn life(r: &mut R, color: &mut [u16], depth: &mut [u16]) {
    println!("
--- el hábitat completo (habitat::Habitat) ---");
    let hab = HABITAT.init(Habitat::new(r, PixelOrder::Swapped));
    let now = LocalTime::new(20_000, 14, 0);
    let c = Companion::new(Personality::from_seed(0x5EED), None, now);
    for act in [
        Activity::Code,
        Activity::Eat,
        Activity::Cook,
        Activity::Guitar,
        Activity::Phone,
        Activity::Wander,
    ] {
        hab.force = Some(act);
        let (mut t_upd, mut t_draw, mut frames, mut worst) = (0u64, 0u64, 0u32, 0u64);
        let mut area = 0u64;
        // 20 s de juego a 15 fotogramas por segundo: incluye ir hasta el lugar.
        for _ in 0..300 {
            let t0 = Instant::now();
            hab.update(&c, now, 66);
            let t1 = Instant::now();
            let a = hab.render(r, &c, now, color, depth, |_| {});
            let t2 = Instant::now();
            t_upd += (t1 - t0).as_micros();
            t_draw += (t2 - t1).as_micros();
            worst = worst.max((t2 - t0).as_micros());
            area += a.area() as u64;
            frames += 1;
        }
        let per = |us: u64| us as f32 / frames as f32 / 1000.0;
        println!(
            "{:<22} vida {:>5.2} ms  dibujo {:>6.2} ms  peor {:>6.2} ms  zona {:>3.0}%",
            act.label(),
            per(t_upd),
            per(t_draw),
            worst as f32 / 1000.0,
            area as f32 / frames as f32 / (W * W) as f32 * 100.0
        );
    }
}
