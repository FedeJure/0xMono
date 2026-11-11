//! El hábitat: la casa y el jardín en 3D, con el mono haciendo su vida según
//! el reloj real (crate `habitat`). Todo lo dibuja el motor como en el
//! ESP32; abajo hay un panel de depuración con la hora y la agenda.
//!
//! ```text
//! cargo run -p simulator --release --example habitat
//! cargo run -p simulator --release --example habitat -- --png foto.png 14:30 [segundos] [--near grados] [--act Leyendo]
//! cargo run -p simulator --release --example habitat -- --sim 06:00 120 hoja.png [velocidad] [cada] [desde]
//! ```
//!
//! Con `--png`, `--say` o `--think` lo hacen hablar o pensar justo antes de
//! la foto; con `--seq`, `--say 2,9` y `--think 5` en esos segundos (el
//! globo se dibuja en las franjas redibujadas, como en la placa).
//!
//! Teclas: A/S/D hablarle, T velocidad x1/x60/x600/x3600, H adelantar una
//! hora, N personaje nuevo, C cámara general, R mostrar la zona redibujada,
//! G un gag ya (dormido: un pedo; cocinando: humo, explosión o fuego), Esc
//! salir (guarda en `companion.bin`).
//!
//! Se dibuja como en el ESP32: la imagen queda en la "pantalla" (`screen`,
//! como la memoria del ST7789) y cada fotograma solo se redibuja la zona que
//! cambió.

use std::convert::Infallible;
use std::time::Instant;

use assets::scene;
use assets::world::Eye;
use chrono::{Local, Offset};
use companion::clock::DAY_MIN;
use companion::{Activity, Companion, Input, LocalTime, Pad, Personality, TalkKind, routine, save};
use embedded_graphics::{
    mono_font::{MonoTextStyle, iso_8859_1::FONT_6X10},
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};
use engine3d::{Bounds, Camera, PixelOrder, Quat, Renderer};
use habitat::Habitat;
use habitat::gags::Gag;
use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};

const W: u32 = 240;
const SCREEN: u32 = 240;
const PANEL: u32 = 64;
const H: u32 = SCREEN + PANEL;
/// Filas de las franjas (con el ancho de la pantalla; más si la zona es angosta).
const BAND: usize = 32;
const SAVE_PATH: &str = "companion.bin";
const SPEEDS: [u32; 4] = [1, 60, 600, 3600];
/// Lo mismo que en el ESP32 (`firmware/src/bin/bench_habitat.rs`).
type R = Renderer<4608, 2816>;

struct Framebuffer(Vec<u32>);

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(W, H)
    }
}

impl DrawTarget for Framebuffer {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb888>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        for Pixel(p, c) in pixels {
            if (0..W as i32).contains(&p.x) && (0..H as i32).contains(&p.y) {
                self.0[(p.y as u32 * W + p.x as u32) as usize] =
                    (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32;
            }
        }
        Ok(())
    }
}

/// El hábitat con su renderer y la "memoria de la pantalla".
struct Sim {
    r: Box<R>,
    hab: Box<Habitat>,
    screen: Vec<u16>,
    color: Vec<u16>,
    depth: Vec<u16>,
    area: Bounds,
    show_area: bool,
    ms: f32,
}

impl Sim {
    fn new() -> Self {
        let mut r = Box::new(R::new(
            Camera::new(SCREEN as i32, SCREEN as i32, 300, 96),
            scene::light(),
        ));
        let hab = Box::new(Habitat::new(&mut r, PixelOrder::Native));
        let w = SCREEN as usize;
        Self {
            r,
            hab,
            screen: vec![0; w * w],
            color: vec![0; w * BAND],
            depth: vec![0; w * BAND],
            area: Bounds::EMPTY,
            show_area: false,
            ms: 0.0,
        }
    }

    /// Dibuja el fotograma en `fb`: lo que cambió va a la "pantalla" y de
    /// ahí a la ventana. El globo de texto lo dibuja el hábitat en las
    /// franjas, como en la placa (con R se ve su lugar en celeste).
    fn frame(&mut self, fb: &mut Framebuffer, c: &Companion, now: LocalTime) {
        let w = SCREEN as usize;
        let screen = &mut self.screen;
        let t0 = Instant::now();
        self.area = self.hab.render(
            &mut self.r,
            c,
            now,
            &mut self.color,
            &mut self.depth,
            |band| {
                for row in 0..band.height {
                    let dst = (band.y0 + row) * w + band.x0;
                    screen[dst..dst + band.width]
                        .copy_from_slice(&band.color[row * band.width..][..band.width]);
                }
            },
        );
        self.ms = t0.elapsed().as_secs_f32() * 1000.0;
        for (dst, &px) in fb.0.iter_mut().zip(self.screen.iter()) {
            *dst = rgb565_to_888(px);
        }
        if self.show_area {
            let zones = [
                (self.hab.bubble.area(), Rgb888::new(60, 200, 255)),
                (self.area, Rgb888::new(255, 40, 200)),
            ];
            for (a, color) in zones.into_iter().filter(|(a, _)| !a.is_empty()) {
                Rectangle::new(
                    Point::new(a.x0, a.y0),
                    Size::new(a.width() as u32, a.height() as u32),
                )
                .into_styled(PrimitiveStyle::with_stroke(color, 1))
                .draw(fb)
                .ok();
            }
        }
    }

    fn stats(&self) -> String {
        let s = self.r.stats;
        let (v, t) = self.r.usage();
        format!(
            "zona {:>3}% {}k px {:.1}ms  {v}v {t}t",
            self.area.area() * 100 / (SCREEN * SCREEN) as i32,
            s.raster.pixels / 1000,
            self.ms
        )
    }
}

/// Cámara cercana al personaje, desde `angle` grados alrededor de él (0 =
/// de frente), para revisar manos y objetos.
fn near_camera(hab: &Habitat, angle: f32) -> Eye {
    let pl = hab.actor.placement();
    let a = (pl.yaw + angle).to_radians();
    // Delante del personaje es -z local.
    let dir = [-a.sin(), -a.cos()];
    let target = [pl.pos[0], pl.lift + 0.36 * 1024.0, pl.pos[1]];
    let pos = [
        target[0] + dir[0] * 0.95 * 1024.0,
        target[1] + 0.2 * 1024.0,
        target[2] + dir[1] * 0.95 * 1024.0,
    ];
    look_at(pos, target)
}

/// Cámara muy cerca de una mano (`--hand Right|Left`), desde `angle` grados
/// alrededor del personaje: para revisar cómo agarra los objetos.
fn hand_camera(hab: &Habitat, side: &str, angle: f32) -> Option<Eye> {
    let pl = hab.actor.placement();
    let a = (pl.yaw + angle).to_radians();
    let dir = [-a.sin(), -a.cos()];
    let h = hab
        .bone_position(&format!(":{side}Hand"))?
        .map(|c| c as f32);
    let f = hab
        .bone_position(&format!(":{side}HandIndex1"))?
        .map(|c| c as f32);
    let target = [
        (h[0] + f[0]) / 2.0,
        (h[1] + f[1]) / 2.0,
        (h[2] + f[2]) / 2.0,
    ];
    let pos = [
        target[0] + dir[0] * 0.38 * 1024.0,
        target[1] + 0.12 * 1024.0,
        target[2] + dir[1] * 0.38 * 1024.0,
    ];
    Some(look_at(pos, target))
}

/// Cámara de frente a la cara del personaje, de cerca.
fn face_camera(hab: &Habitat) -> Eye {
    let pl = hab.actor.placement();
    let a = pl.yaw.to_radians();
    let dir = [-a.sin(), -a.cos()];
    let head = hab.face_position().map(|c| c as f32);
    let pos = [
        head[0] + dir[0] * 0.55 * 1024.0,
        head[1] + 0.05 * 1024.0,
        head[2] + dir[1] * 0.55 * 1024.0,
    ];
    look_at(pos, head)
}

fn look_at(pos: [f32; 3], target: [f32; 3]) -> Eye {
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let fwd = norm([target[0] - pos[0], target[1] - pos[1], target[2] - pos[2]]);
    let right = norm(cross([0.0, 1.0, 0.0], fwd));
    let up = cross(fwd, right);
    // Columnas: derecha, arriba, adelante.
    let m = [
        [right[0], up[0], fwd[0]],
        [right[1], up[1], fwd[1]],
        [right[2], up[2], fwd[2]],
    ];
    let tr = m[0][0] + m[1][1] + m[2][2];
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [
            s / 4.0,
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
        ]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [
            (m[2][1] - m[1][2]) / s,
            s / 4.0,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [
            (m[0][2] - m[2][0]) / s,
            (m[0][1] + m[1][0]) / s,
            s / 4.0,
            (m[1][2] + m[2][1]) / s,
        ]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [
            (m[1][0] - m[0][1]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            s / 4.0,
        ]
    };
    Eye {
        pos: pos.map(|x| x as i32),
        rot: Quat(q.map(|x| (x * 16384.0) as i32)),
    }
}

/// `--look x,y,z,tx,ty,tz`: cámara fija en metros de Blender (como en
/// `tools/blender/build_house.py`), desde (x, y, z) mirando a (tx, ty, tz).
fn look_arg(args: &[String]) -> Option<Eye> {
    let k = args.iter().position(|a| a == "--look")?;
    let v: Vec<f32> = args
        .get(k + 1)?
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    // Blender -> motor: el origen de la escena está corrido 2,9 m en x.
    let eng = |p: &[f32]| [(p[0] - 2.9) * 1024.0, p[2] * 1024.0, p[1] * 1024.0];
    (v.len() == 6).then(|| look_at(eng(&v[..3]), eng(&v[3..])))
}

fn gag_named(name: &str) -> Option<Gag> {
    Some(match name {
        "Pedo" => Gag::Fart,
        "Humo" => Gag::Smoke,
        "Explosion" => Gag::Pop,
        "Fuego" => Gag::Fire,
        _ => return None,
    })
}

/// La tecla G: el gag que corresponde a lo que está haciendo (en la cocina,
/// va rotando entre humo, explosión y fuego).
fn gag_for(act: Activity, n: u32) -> Option<Gag> {
    match act {
        Activity::Sleep | Activity::Nap => Some(Gag::Fart),
        Activity::Cook => Some([Gag::Smoke, Gag::Pop, Gag::Fire][n as usize % 3]),
        _ => None,
    }
}

fn rgb565_to_888(c: u16) -> u32 {
    let (r, g, b) = ((c >> 11) & 0x1F, (c >> 5) & 0x3F, c & 0x1F);
    let (r, g, b) = (
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    );
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

fn activity_color(a: Activity) -> Rgb888 {
    let (r, g, b) = a.color();
    Rgb888::new(r, g, b)
}

fn draw_panel(fb: &mut Framebuffer, c: &Companion, now: LocalTime, speed: u32, stats: &str) {
    Rectangle::new(Point::new(0, SCREEN as i32), Size::new(W, PANEL))
        .into_styled(PrimitiveStyle::with_fill(Rgb888::new(16, 18, 24)))
        .draw(fb)
        .ok();
    let text = MonoTextStyle::new(&FONT_6X10, Rgb888::new(220, 220, 230));
    let dim = MonoTextStyle::new(&FONT_6X10, Rgb888::new(140, 140, 160));
    let y = SCREEN as i32;
    let line1 = format!(
        "{} {:02}:{:02}  x{speed}  {}",
        now.weekday().short_name(),
        now.hour(),
        now.minute() % 60,
        c.activity().label()
    );
    Text::new(&line1, Point::new(4, y + 11), text).draw(fb).ok();
    Text::new(stats, Point::new(4, y + 23), dim).draw(fb).ok();
    let bar_y = y + 32;
    routine::timeline(&c.personality, now.day, |s, e, a| {
        let x0 = s * W as i32 / DAY_MIN as i32;
        let x1 = (e * W as i32 / DAY_MIN as i32).max(x0 + 1);
        Rectangle::new(Point::new(x0, bar_y), Size::new((x1 - x0) as u32, 12))
            .into_styled(PrimitiveStyle::with_fill(activity_color(a)))
            .draw(fb)
            .ok();
    });
    let x = now.minute() as i32 * W as i32 / DAY_MIN as i32;
    Line::new(Point::new(x, bar_y - 3), Point::new(x, bar_y + 14))
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::WHITE, 1))
        .draw(fb)
        .ok();
    for h in [6, 12, 18] {
        Text::with_alignment(
            &h.to_string(),
            Point::new(h * W as i32 / 24, y + 57),
            dim,
            Alignment::Center,
        )
        .draw(fb)
        .ok();
    }
}

fn real_now() -> LocalTime {
    let now = Local::now();
    let offset_min = (now.offset().fix().local_minus_utc() / 60) as i16;
    LocalTime::from_unix(now.timestamp() as u64, offset_min)
}

fn load_or_new(now: LocalTime) -> Companion {
    let data = std::fs::read(SAVE_PATH).ok().and_then(|b| save::decode(&b));
    let data = data.unwrap_or_else(|| {
        let seed = Local::now().timestamp_subsec_nanos() ^ 0x5EED;
        println!("personaje nuevo (semilla {seed:#x})");
        save::SaveData {
            seed,
            last_seen: None,
            clock: None,
        }
    });
    Companion::new(Personality::from_seed(data.seed), data.last_seen, now)
}

fn save_png(path: &str, fb: &Framebuffer) {
    let file = std::fs::File::create(path).expect("no se pudo crear el PNG");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, H);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> =
        fb.0.iter()
            .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8])
            .collect();
    enc.write_header().unwrap().write_image_data(&data).unwrap();
}

fn parse_time(s: Option<&String>, day: u32) -> Option<LocalTime> {
    let (h, m) = s?.split_once(':')?;
    Some(LocalTime::new(day, h.parse().ok()?, m.parse().ok()?))
}

/// `--sim HH:MM segundos [hoja.png] [velocidad] [cada] [desde]`: corre el hábitat
/// sin ventana a 30 fotogramas por segundo desde esa hora y cuenta cuánto
/// se redibuja (los fotogramas completos son los caros en el ESP32). Con
/// `hoja.png` guarda un fotograma cada `cada` (30 = uno por segundo) a
/// partir del segundo `desde`.
fn simulate(args: &[String], fb: &mut Framebuffer) {
    let start =
        parse_time(args.first(), real_now().day).unwrap_or(LocalTime::new(real_now().day, 12, 0));
    let secs: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(60);
    let speed: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let every: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(30);
    let from: u32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut c = load_or_new(start);
    let mut sim = Sim::new();
    sim.show_area = true;
    sim.hab.teleport(&c);
    let mut clock = start.total_secs() as f64;
    let (mut full, mut area, mut frames) = (0u32, 0u64, 0u32);
    let mut sheet: Vec<Vec<u32>> = Vec::new();
    let mut last_act = c.activity();
    for f in 0..secs * 30 {
        clock += speed / 30.0;
        let now = LocalTime::from_total_secs(clock as u32);
        c.update(now, 33);
        if c.activity() != last_act {
            println!(
                "{:02}:{:02} {}",
                now.hour(),
                now.minute() % 60,
                c.activity().label()
            );
            last_act = c.activity();
        }
        sim.hab.update(&c, now, 33);
        sim.frame(fb, &c, now);
        frames += 1;
        let a = sim.area.area() as u64;
        area += a;
        if a == (SCREEN * SCREEN) as u64 {
            full += 1;
        }
        if f >= from * 30 && f % every == 0 && sheet.len() < 48 {
            sheet.push(fb.0[..(SCREEN * SCREEN) as usize].to_vec());
        }
    }
    println!(
        "{frames} fotogramas: {full} completos ({:.1}%), zona media {:.1}% de la pantalla",
        full as f32 / frames as f32 * 100.0,
        area as f32 / frames as f32 / (SCREEN * SCREEN) as f32 * 100.0
    );
    if let Some(path) = args.get(2) {
        save_sheet(path, &sheet);
        println!("hoja: {path}");
    }
}

fn save_sheet(path: &str, sheet: &[Vec<u32>]) {
    let cols = 8usize.min(sheet.len().max(1));
    let rows = sheet.len().div_ceil(cols);
    let w = SCREEN as usize;
    let mut out = vec![0u32; cols * w * rows * w];
    for (k, img) in sheet.iter().enumerate() {
        let (cx, cy) = (k % cols * w, k / cols * w);
        for y in 0..w {
            let dst = (cy + y) * cols * w + cx;
            out[dst..dst + w].copy_from_slice(&img[y * w..(y + 1) * w]);
        }
    }
    let file = std::fs::File::create(path).expect("no se pudo crear el PNG");
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(file),
        (cols * w) as u32,
        (rows * w) as u32,
    );
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = out
        .iter()
        .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8])
        .collect();
    enc.write_header().unwrap().write_image_data(&data).unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut fb = Framebuffer(vec![0; (W * H) as usize]);

    if let Some(i) = args.iter().position(|a| a == "--png") {
        let path = args.get(i + 1).map_or("habitat.png", String::as_str);
        let today = real_now();
        let now = parse_time(args.get(i + 2), today.day).unwrap_or(today);
        let near: Option<f32> = args
            .iter()
            .position(|a| a == "--near")
            .map(|k| args.get(k + 1).and_then(|s| s.parse().ok()).unwrap_or(90.0));
        let mut c = load_or_new(now);
        let mut sim = Sim::new();
        // `--act Programando`: esa actividad en vez de la de la agenda.
        if let Some(k) = args.iter().position(|a| a == "--act") {
            let name = args.get(k + 1).map_or("", String::as_str);
            sim.hab.force = Activity::ALL.iter().copied().find(|a| a.label() == name);
            if sim.hab.force.is_none() {
                let all: Vec<_> = Activity::ALL.iter().map(|a| a.label()).collect();
                panic!("actividad desconocida {name:?}; hay: {all:?}");
            }
        }
        sim.hab.teleport(&c);
        let extra: f32 = args.get(i + 3).and_then(|s| s.parse().ok()).unwrap_or(1.5);
        for _ in 0..((extra * 30.0) as u32) {
            sim.hab.update(&c, now, 33);
        }
        if let Some(angle) = near {
            sim.hab.camera.fixed = Some(near_camera(&sim.hab, angle));
            sim.hab.update(&c, now, 1);
        }
        if let Some(eye) = look_arg(&args) {
            sim.hab.camera.fixed = Some(eye);
            sim.hab.update(&c, now, 1);
        }
        // `--hand Right [grados]`: de cerca a la mano (hace falta un
        // fotograma dibujado para saber dónde está).
        if let Some(k) = args.iter().position(|a| a == "--hand") {
            let side = args.get(k + 1).map_or("Right", String::as_str);
            let angle = args.get(k + 2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            sim.frame(&mut fb, &c, now);
            sim.hab.camera.fixed = hand_camera(&sim.hab, side, angle);
            sim.hab.update(&c, now, 1);
            // La pose cambió un poco con la cámara nueva: otra vez.
            sim.frame(&mut fb, &c, now);
            sim.hab.camera.fixed = hand_camera(&sim.hab, side, angle);
            sim.hab.update(&c, now, 1);
        }
        // `--say` / `--think`: el globo, dibujado sobre lo que ya estaba
        // (solo se redibuja su zona).
        if args.iter().any(|a| a == "--say") {
            c.poke(now);
        }
        if args.iter().any(|a| a == "--think") {
            c.think(now);
        }
        if let Some(l) = c.speech() {
            sim.frame(&mut fb, &c, now);
            println!("{:?}: {}", l.kind, l.as_str());
        }
        sim.frame(&mut fb, &c, now);
        let st = sim.stats();
        draw_panel(&mut fb, &c, now, 1, &st);
        save_png(path, &fb);
        println!("guardado {path}: {} · {st}", c.activity().label());
        return;
    }

    // `--faces Actividad tira.png`: primeros planos de la cara cada 1/4 s
    // durante 12 s, para ver cómo cambia la expresión.
    if let Some(i) = args.iter().position(|a| a == "--faces") {
        let name = args.get(i + 1).map_or("", String::as_str);
        let path = args.get(i + 2).map_or("caras.png", String::as_str);
        let now = LocalTime::new(real_now().day, 14, 0);
        let c = load_or_new(now);
        let mut sim = Sim::new();
        sim.hab.force = Activity::ALL.iter().copied().find(|a| a.label() == name);
        sim.hab.teleport(&c);
        for _ in 0..90 {
            sim.hab.update(&c, now, 33);
        }
        let mut sheet = Vec::new();
        for f in 0..(12 * 30) {
            sim.hab.camera.fixed = Some(face_camera(&sim.hab));
            sim.hab.update(&c, now, 33);
            sim.frame(&mut fb, &c, now);
            if f % 8 == 0 {
                sheet.push(fb.0[..(SCREEN * SCREEN) as usize].to_vec());
            }
        }
        save_sheet(path, &sheet);
        println!("{path}: {} caras de {name}", sheet.len());
        return;
    }

    // `--seq hoja.png Act1:segundos,Act2:segundos... [cada] [HH:MM] [--near grados] [--from s]`:
    // encadena actividades forzadas (la primera ya en su lugar) y guarda
    // un cuadro cada `cada` fotogramas (30 = uno por segundo).
    if let Some(i) = args.iter().position(|a| a == "--seq") {
        let path = args.get(i + 1).map_or("seq.png", String::as_str);
        let steps: Vec<(Activity, f32)> = args
            .get(i + 2)
            .map_or("", String::as_str)
            .split(',')
            .map(|s| {
                let (name, secs) = s.split_once(':').unwrap_or((s, "10"));
                let act = Activity::ALL
                    .iter()
                    .copied()
                    .find(|a| a.label() == name)
                    .unwrap_or_else(|| panic!("actividad desconocida {name:?}"));
                (act, secs.parse().unwrap_or(10.0))
            })
            .collect();
        let every: u32 = args.get(i + 3).and_then(|s| s.parse().ok()).unwrap_or(15);
        let now = parse_time(args.get(i + 4), real_now().day).unwrap_or(LocalTime::new(
            real_now().day,
            14,
            0,
        ));
        let near: Option<f32> = args
            .iter()
            .position(|a| a == "--near")
            .and_then(|k| args.get(k + 1).and_then(|s| s.parse().ok()));
        // `--from segundos`: los cuadros empiezan ahí.
        let from: u32 = args
            .iter()
            .position(|a| a == "--from")
            .and_then(|k| args.get(k + 1).and_then(|s| s.parse::<f32>().ok()))
            .map_or(0, |s| (s * 30.0) as u32);
        let mut c = load_or_new(now);
        let mut sim = Sim::new();
        sim.hab.force = Some(steps[0].0);
        sim.hab.teleport(&c);
        sim.hab.camera.fixed = look_arg(&args);
        // `--gag segundos:Pedo|Humo|Explosion|Fuego,...`: dispara gags.
        let gags: Vec<(u32, Gag)> = args
            .iter()
            .position(|a| a == "--gag")
            .and_then(|k| args.get(k + 1))
            .map(|s| {
                s.split(',')
                    .filter_map(|g| {
                        let (t, name) = g.split_once(':')?;
                        Some(((t.parse::<f32>().ok()? * 30.0) as u32, gag_named(name)?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // `--say 2,9` / `--think 5`: le habla o piensa en esos segundos.
        let at = |flag: &str| -> Vec<u32> {
            args.iter()
                .position(|a| a == flag)
                .and_then(|k| args.get(k + 1))
                .map(|s| {
                    s.split(',')
                        .filter_map(|t| Some((t.parse::<f32>().ok()? * 30.0) as u32))
                        .collect()
                })
                .unwrap_or_default()
        };
        let (says, thinks) = (at("--say"), at("--think"));
        let mut sheet = Vec::new();
        let mut f = 0u32;
        for (act, secs) in steps {
            sim.hab.force = Some(act);
            for _ in 0..(secs * 30.0) as u32 {
                for &(_, g) in gags.iter().filter(|(t, _)| *t == f) {
                    sim.hab.trigger(g);
                }
                if let Some(angle) = near {
                    sim.hab.camera.fixed = Some(near_camera(&sim.hab, angle));
                }
                if says.contains(&f) {
                    c.poke(now);
                }
                if thinks.contains(&f) {
                    c.think(now);
                }
                c.update(now, 33);
                sim.hab.update(&c, now, 33);
                if std::env::var_os("SEQ_DEBUG").is_some() && f >= from {
                    let pl = sim.hab.actor.placement();
                    eprintln!(
                        "{f} {:?} {:?} t={:.2} pos=({:.0},{:.0}) lift={:.0} yaw={:.0}",
                        sim.hab.actor.step,
                        sim.hab.clip_name(),
                        sim.hab.actor.step_t,
                        pl.pos[0],
                        pl.pos[1],
                        pl.lift,
                        pl.yaw
                    );
                }
                if f >= from && (f - from).is_multiple_of(every) && sheet.len() < 64 {
                    sim.frame(&mut fb, &c, now);
                    sheet.push(fb.0[..(SCREEN * SCREEN) as usize].to_vec());
                }
                f += 1;
            }
        }
        save_sheet(path, &sheet);
        println!("{path}: {} cuadros", sheet.len());
        return;
    }

    if let Some(i) = args.iter().position(|a| a == "--sim") {
        simulate(&args[i + 1..], &mut fb);
        return;
    }

    let mut window = Window::new(
        "Hábitat (A/S/D hablarle, T velocidad, H +1 h, C cámara general, R zona, G gag, N nuevo, Esc salir)",
        W as usize,
        H as usize,
        WindowOptions {
            scale: Scale::X2,
            ..WindowOptions::default()
        },
    )
    .expect("no se pudo abrir la ventana");
    window.set_target_fps(30);

    let mut clock = real_now().total_secs() as f64;
    let mut c = load_or_new(LocalTime::from_total_secs(clock as u32));
    let mut sim = Sim::new();
    sim.hab.teleport(&c);
    let mut speed_i = 0;
    let mut last = Instant::now();
    let mut stats = String::new();
    let mut frame = 0u32;
    let mut overview = false;
    let mut said: Option<String> = None;
    let mut gag_n = 0u32;
    let mut pad = Pad::new();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let dt = last.elapsed();
        last = Instant::now();
        clock += dt.as_secs_f64() * SPEEDS[speed_i] as f64;
        if window.is_key_pressed(Key::T, KeyRepeat::No) {
            speed_i = (speed_i + 1) % SPEEDS.len();
        }
        if window.is_key_pressed(Key::H, KeyRepeat::Yes) {
            clock += 3600.0;
        }
        if window.is_key_pressed(Key::C, KeyRepeat::No) {
            overview = !overview;
            sim.hab.set_overview(overview);
        }
        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            sim.show_area = !sim.show_area;
        }
        if window.is_key_pressed(Key::G, KeyRepeat::No) {
            if let Some(g) = gag_for(c.activity(), gag_n) {
                sim.hab.trigger(g);
            }
            gag_n += 1;
        }
        let now = LocalTime::from_total_secs(clock as u32);
        if window.is_key_pressed(Key::N, KeyRepeat::No) {
            let seed = Local::now().timestamp_subsec_nanos();
            c = Companion::new(Personality::from_seed(seed), None, now);
        }
        let dt_ms = dt.as_millis().min(100) as u32;
        let keys = [Key::A, Key::S, Key::D].map(|k| window.is_key_down(k));
        match pad.update(keys, dt_ms) {
            Some(Input::Tap(_)) => {
                c.poke(now);
            }
            // Todavía sin función: solo avisa que se detectó.
            Some(Input::Hold(b)) => {
                println!("{:02}:{:02} mantener {b:?}", now.hour(), now.minute() % 60);
            }
            Some(Input::Chord) => {
                println!(
                    "{:02}:{:02} A+C: modo minero (en la placa reinicia en ese modo)",
                    now.hour(),
                    now.minute() % 60
                );
            }
            None => {}
        }
        if let Some(ev) = c.update(now, dt_ms) {
            println!("{:02}:{:02} {ev:?}", now.hour(), now.minute() % 60);
        }
        if let Some(l) = c.speech()
            && Some(l.as_str()) != said.as_deref()
        {
            let who = if l.kind == TalkKind::Think {
                "piensa"
            } else {
                "dice"
            };
            println!(
                "{:02}:{:02} {who}: {}",
                now.hour(),
                now.minute() % 60,
                l.as_str()
            );
            said = Some(l.as_str().to_string());
        }
        sim.hab.update(&c, now, dt_ms);
        sim.frame(&mut fb, &c, now);
        frame += 1;
        if frame.is_multiple_of(15) {
            stats = sim.stats();
        }
        draw_panel(&mut fb, &c, now, SPEEDS[speed_i], &stats);
        window
            .update_with_buffer(&fb.0, W as usize, H as usize)
            .unwrap();
    }

    let data = save::SaveData {
        seed: c.personality.seed,
        last_seen: c.last_seen,
        clock: Some(LocalTime::from_total_secs(clock as u32)),
    };
    match std::fs::write(SAVE_PATH, save::encode(&data)) {
        Ok(()) => println!("guardado en {SAVE_PATH}"),
        Err(e) => eprintln!("no se pudo guardar: {e}"),
    }
}
