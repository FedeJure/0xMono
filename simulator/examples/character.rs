//! Visor del personaje exportado de Blender, con el motor 3D a 240×240.
//!
//! ```text
//! cargo run -p simulator --release --example character
//! cargo run -p simulator --release --example character -- --png mono.png [clip] [ms] [giro°] [cerca]
//! cargo run -p simulator --release --example character -- --sheet hoja.png clip1 clip2 ...
//! ```
//!
//! Teclas: A/Shift+A animación siguiente/anterior, L capas (piernas de
//! caminar + tronco de saludar), ←/→ girar, E ojos, M boca, Z cuerpo entero /
//! primer plano, Espacio pausa, Esc salir.

use std::time::Instant;

use assets::anim::{AnimPose, Layer};
use assets::mono::MONO;
use assets::mono_anims::{CLIPS, clip, find};
use assets::scene::{self, CharacterScene, Pose};
use companion::{Emotion, Eyes, FaceAnimator};
use engine3d::fixed::{Angle, deg};
use engine3d::raster::fill16;
use engine3d::{Band, Camera, Mat3, PixelOrder, Renderer, Rgb, Transform};
use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};

const W: usize = 240;
const H: usize = 240;
const BAND: usize = 48;
type R = Renderer<1024, 1536>;

fn rgb565_to_rgb(c: u16) -> u32 {
    let (r, g, b) = ((c >> 11) & 0x1F, (c >> 5) & 0x3F, c & 0x1F);
    let (r, g, b) = (
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    );
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// Cámara: cuerpo entero o primer plano de la cara.
fn model(yaw: Angle, close: bool) -> Transform {
    let (center_y, dist) = if close { (720, 700) } else { (450, 2000) };
    Transform::new(Mat3::rot_x(deg(-4)), [0, -center_y, dist])
        .then(&Transform::new(Mat3::rot_y(yaw), [0; 3]))
}

/// Qué mostrar.
struct View {
    clip: usize,
    t_ms: u32,
    layered: bool,
    yaw: Angle,
    face: (u8, u8),
    close: bool,
}

/// Pose de la animación elegida, o las capas de demostración.
fn pose_of(v: &View, out: &mut Pose) {
    let mut p = AnimPose::REST;
    if v.layered {
        let spine = MONO.bone(":Spine").expect("falta Spine");
        p.layer(&Layer::full(&CLIPS[clip::WALKING], v.t_ms));
        p.layer(&Layer {
            mask: MONO.mask_from(spine),
            ..Layer::full(&CLIPS[clip::WAVING], v.t_ms % 2000)
        });
    } else {
        p.layer(&Layer::full(&CLIPS[v.clip], v.t_ms));
    }
    p.to_pose(out);
}

fn render(r: &mut R, scene: &CharacterScene, v: &View, pose: &mut Pose, out: &mut [u32]) {
    pose_of(v, pose);
    r.begin_frame();
    scene.set_face(r, v.face.0, v.face.1);
    scene.submit(r, &model(v.yaw, v.close), pose);
    let bg: Vec<u16> = (0..H)
        .map(|y| {
            Rgb(70, 74, 92)
                .lerp(Rgb(30, 32, 44), y as u32, H as u32)
                .to_565()
        })
        .collect();
    let mut color = vec![0u16; W * BAND];
    let mut depth = vec![0u16; W * BAND];
    for y0 in (0..H).step_by(BAND) {
        let mut band = Band {
            color: &mut color,
            depth: &mut depth,
            x0: 0,
            width: W,
            y0,
            height: BAND,
        };
        for row in 0..BAND {
            fill16(&mut band.color[row * W..(row + 1) * W], bg[y0 + row]);
        }
        if r.touches_rows(y0, y0 + BAND) {
            r.clear_depth(&mut band);
            r.render_band(&mut band);
        }
        for (dst, &c) in out[y0 * W..(y0 + BAND) * W]
            .iter_mut()
            .zip(band.color.iter())
        {
            *dst = rgb565_to_rgb(c);
        }
    }
}

fn setup() -> (Box<R>, CharacterScene, Box<Pose>) {
    let mut r = Box::new(R::new(
        Camera::new(W as i32, H as i32, 300, 128),
        scene::light(),
    ));
    let scene = CharacterScene::new(&MONO);
    scene.setup(&mut r, PixelOrder::Native);
    (r, scene, Box::new(Pose::REST))
}

fn save_png(path: &str, w: usize, h: usize, px: &[u32]) {
    let file = std::fs::File::create(path).expect("no se pudo crear el PNG");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = px
        .iter()
        .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8])
        .collect();
    enc.write_header().unwrap().write_image_data(&data).unwrap();
}

fn clip_index(name: &str) -> usize {
    let c = find(name).unwrap_or_else(|| panic!("no existe el clip {name}"));
    CLIPS.iter().position(|x| std::ptr::eq(x, c)).unwrap()
}

/// Hoja de contacto: una fila por clip, 6 cuadros repartidos en su duración.
fn sheet(path: &str, names: &[String]) {
    let (mut r, scene, mut pose) = setup();
    let names: Vec<String> = if names.is_empty() {
        CLIPS.iter().map(|c| c.name.to_string()).collect()
    } else {
        names.to_vec()
    };
    const COLS: usize = 6;
    let (cw, ch) = (W / 2, H / 2);
    let mut out = vec![0u32; COLS * cw * names.len() * ch];
    let mut frame = vec![0u32; W * H];
    for (row, name) in names.iter().enumerate() {
        let ci = clip_index(name);
        let d = CLIPS[ci].duration_ms();
        for col in 0..COLS {
            let v = View {
                clip: ci,
                t_ms: d * col as u32 / (COLS as u32 - 1).max(1),
                layered: false,
                yaw: 0,
                face: (0, 0),
                close: false,
            };
            render(&mut r, &scene, &v, &mut pose, &mut frame);
            // Reducir a la mitad promediando 2×2.
            for y in 0..ch {
                for x in 0..cw {
                    let mut acc = [0u32; 3];
                    for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                        let p = frame[(2 * y + dy) * W + 2 * x + dx];
                        acc[0] += (p >> 16) & 255;
                        acc[1] += (p >> 8) & 255;
                        acc[2] += p & 255;
                    }
                    let o = (row * ch + y) * COLS * cw + col * cw + x;
                    out[o] = ((acc[0] / 4) << 16) | ((acc[1] / 4) << 8) | (acc[2] / 4);
                }
            }
        }
    }
    save_png(path, COLS * cw, names.len() * ch, &out);
    println!("guardado {path}: {} clips", names.len());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--sheet") {
        let path = args.get(i + 1).map_or("hoja.png", String::as_str);
        return sheet(path, &args[(i + 2).min(args.len())..]);
    }
    let (mut r, scene, mut pose) = setup();
    let mut frame = vec![0u32; W * H];

    if let Some(i) = args.iter().position(|a| a == "--png") {
        let arg = |k: usize| args.get(i + k).and_then(|s| s.parse::<i32>().ok());
        let path = args.get(i + 1).map_or("mono.png", String::as_str);
        let v = View {
            clip: args.get(i + 2).map_or(clip::IDLE, |n| clip_index(n)),
            t_ms: arg(3).unwrap_or(0) as u32,
            layered: false,
            yaw: deg(arg(4).unwrap_or(0)),
            face: (0, 0),
            close: arg(5).unwrap_or(0) != 0,
        };
        render(&mut r, &scene, &v, &mut pose, &mut frame);
        save_png(path, W, H, &frame);
        let s = r.stats;
        println!(
            "guardado {path}: {} triángulos visibles, {} píxeles ({} con textura)",
            s.tris_visible, s.raster.pixels, s.raster.tex_pixels
        );
        return;
    }

    let mut window = Window::new(
        "Personaje (A animación, L capas, ←/→ girar, E ojos, M boca, Z acercar)",
        W,
        H,
        WindowOptions {
            scale: Scale::X2,
            ..WindowOptions::default()
        },
    )
    .expect("no se pudo abrir la ventana");
    window.set_target_fps(30);
    let mut v = View {
        clip: clip::IDLE,
        t_ms: 0,
        layered: false,
        yaw: 0,
        face: (0, 0),
        close: false,
    };
    let mut paused = false;
    // Parpadeo: mientras los ojos sean los normales, parpadea como en el juego.
    let mut blink = FaceAnimator::new(Emotion::Content, 1);
    let mut last = Instant::now();
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let dt = last.elapsed().as_millis() as u32;
        last = Instant::now();
        if !paused {
            v.t_ms = v.t_ms.wrapping_add(dt);
        }
        let shift = window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
        if window.is_key_pressed(Key::A, KeyRepeat::No) {
            let n = CLIPS.len();
            v.clip = if shift {
                (v.clip + n - 1) % n
            } else {
                (v.clip + 1) % n
            };
            v.t_ms = 0;
            v.layered = false;
        }
        if window.is_key_pressed(Key::L, KeyRepeat::No) {
            v.layered = !v.layered;
        }
        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            paused = !paused;
        }
        if window.is_key_down(Key::Left) {
            v.yaw = v.yaw.wrapping_add(deg(3));
        }
        if window.is_key_down(Key::Right) {
            v.yaw = v.yaw.wrapping_sub(deg(3));
        }
        if window.is_key_pressed(Key::E, KeyRepeat::No) {
            v.face.0 = (v.face.0 + 1) % 6;
        }
        if window.is_key_pressed(Key::M, KeyRepeat::No) {
            v.face.1 = (v.face.1 + 1) % 6;
        }
        if window.is_key_pressed(Key::Z, KeyRepeat::No) {
            v.close = !v.close;
        }
        let mut shown = View { ..v };
        if v.face.0 == 0 && blink.update(dt).eyes != Eyes::Normal {
            shown.face.0 = blink.face().eyes as u8;
        }
        render(&mut r, &scene, &shown, &mut pose, &mut frame);
        let name = if v.layered {
            "capas: walking + waving"
        } else {
            CLIPS[v.clip].name
        };
        window.set_title(&format!("Personaje · {name} (A animación, L capas)"));
        window.update_with_buffer(&frame, W, H).unwrap();
    }
}
