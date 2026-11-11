//! Escena de prueba: una mascota "blob" low-poly que salta, se aplasta al
//! aterrizar y cambia de expresión. La usan el simulador y el benchmark del
//! firmware, así los números de PC y de la placa miden exactamente lo mismo.
//!
//! La cara es una textura: el frente del cuerpo lleva UV por proyección
//! frontal y dos materiales texturizados (ojos y boca) que apuntan a celdas de
//! una hoja de expresiones ([`FACE_SHEET`]). Cambiar de expresión es cambiar
//! el desplazamiento de esas celdas.

use crate::fixed::{Angle, Mat3, ONE_Q14, Transform, deg, isqrt32, sin};
use crate::material::{IDENTITY_COLORS, Material, PixelOrder, Rgb, TexMap, Texture};
use crate::meshgen::{Ellipsoid, MeshBuf};
use crate::raster::{Band, fill16};
use crate::render::{Camera, Light, Renderer};

pub const WIDTH: usize = 240;
pub const HEIGHT: usize = 240;
pub const BAND_HEIGHT: usize = 48;
pub const BAND_PIXELS: usize = WIDTH * BAND_HEIGHT;

pub type DemoRenderer = Renderer<512, 1024>;
pub type DemoMesh = MeshBuf<512, 1024>;

const HORIZON: usize = 150;
/// Altura del suelo (y de las plantas de los pies) en Q8.
const GROUND_Y: i32 = -240;
const HOP_MS: u32 = 1200;
const HOP_HEIGHT: i32 = 90;

// Materiales. Los texels de la cara son índices en esta misma tabla.
const BODY: u8 = 0;
const INK: u8 = 1;
const CHEEK: u8 = 2;
const FEET: u8 = 3;
const FACE_EYES: u8 = 4;
const FACE_MOUTH: u8 = 5;
const WHITE: u8 = 6;
const MOUTH: u8 = 7;

const BONE_FEET: u8 = 0;
const BONE_BODY: u8 = 1;

// --- Hoja de expresiones ----------------------------------------------------
//
// 128×128 texels de 4 bits (8 KB en flash): columna izquierda = 4 pares de
// ojos, columna derecha = 4 bocas, en celdas de 64×32.
//
// La cara se proyecta de frente: 1 texel = 5 unidades Q8 del modelo. La zona
// de ojos va de y = 130 (v = 0) a y = 0 (v = 26); la de la boca, de y = 0 a
// y = −130 (v = 26..52). Los bordes de cada celda son color piel, así un
// texel que se pase de la zona no se nota.

const SHEET_LOG2: u8 = 7;
const SHEET: i32 = 1 << SHEET_LOG2;
const CELL_W: i32 = 64;
const CELL_H: i32 = 32;
/// Unidades Q8 por texel.
const TEXEL: i32 = 5;
/// Esquina superior izquierda de la cara en el modelo (x, y), Q8.
const FACE_X0: i32 = -160;
const FACE_Y0: i32 = 130;
/// Fila de la hoja (proyectada) donde empieza la zona de la boca.
const MOUTH_V0: i16 = 26;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eyes {
    Open = 0,
    Blink = 1,
    Happy = 2,
    Sleepy = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mouth {
    Smile = 0,
    Open = 1,
    Grin = 2,
    Neutral = 3,
}

/// ¿(x, y) dentro de la elipse de semiejes (a, b)? Todo en medios texels.
const fn ell(x: i32, y: i32, a: i32, b: i32) -> bool {
    x * x * b * b + y * y * a * a <= a * a * b * b
}

/// Texel de los ojos. `lx`, `ly` dentro de la celda.
const fn eye_texel(lx: i32, ly: i32, kind: i32) -> u8 {
    // Ojos centrados en los texels (15, 17) y (48, 17): x = ±85 del modelo.
    let x = 2 * lx - if lx < CELL_W / 2 { 30 } else { 96 };
    let y = 2 * ly - 34;
    match kind {
        0 => {
            if ell(x + 4, y + 6, 3, 3) {
                WHITE
            } else if ell(x, y, 11, 15) {
                INK
            } else {
                BODY
            }
        }
        1 => {
            if y >= 0 && y <= 2 && x >= -12 && x <= 12 {
                INK
            } else {
                BODY
            }
        }
        2 => {
            // "^ ^": medio anillo de arriba.
            let y = y - 4;
            if y <= 0 && ell(x, y, 13, 12) && !ell(x, y, 8, 7) {
                INK
            } else {
                BODY
            }
        }
        _ => {
            // Medio cerrado: solo la mitad de abajo del ojo.
            if y >= 0 && ell(x, y, 11, 15) {
                INK
            } else {
                BODY
            }
        }
    }
}

/// Texel de la boca, centrada en x = 0 del modelo y la fila 12 de la celda.
const fn mouth_texel(lx: i32, ly: i32, kind: i32) -> u8 {
    let x = 2 * lx + 1 - CELL_W;
    let y = 2 * ly - 24;
    match kind {
        0 => {
            // Sonrisa: medio anillo de abajo.
            let y = y + 6;
            if y >= 0 && ell(x, y, 19, 13) && !ell(x, y, 14, 8) {
                INK
            } else {
                BODY
            }
        }
        1 => {
            if ell(x, y, 10, 12) {
                MOUTH
            } else if ell(x, y, 14, 16) {
                INK
            } else {
                BODY
            }
        }
        2 => {
            // Sonrisa abierta con dientes.
            let y = y + 8;
            if y < 0 || !ell(x, y, 21, 17) {
                BODY
            } else if y == 0 || !ell(x, y, 16, 12) {
                INK
            } else if y <= 4 {
                WHITE
            } else {
                MOUTH
            }
        }
        _ => {
            if y >= 0 && y <= 2 && x >= -11 && x <= 11 {
                INK
            } else {
                BODY
            }
        }
    }
}

const fn build_sheet() -> [u8; (SHEET * SHEET / 2) as usize] {
    let mut out = [0u8; (SHEET * SHEET / 2) as usize];
    let mut y = 0;
    while y < SHEET {
        let mut x = 0;
        while x < SHEET {
            let (lx, ly, row) = (x % CELL_W, y % CELL_H, y / CELL_H);
            let m = if x < CELL_W {
                eye_texel(lx, ly, row)
            } else {
                mouth_texel(lx, ly, row)
            };
            let i = (y * SHEET + x) as usize;
            out[i / 2] |= m << ((i & 1) * 4);
            x += 1;
        }
        y += 1;
    }
    out
}

/// La hoja de expresiones, generada al compilar. En el ESP32 va en RAM
/// (8 KB): desde flash, los fallos de caché encarecen cada texel.
#[cfg_attr(feature = "iram", unsafe(link_section = ".data"))]
pub static FACE_SHEET: [u8; (SHEET * SHEET / 2) as usize] = build_sheet();
pub static FACE_TEXTURE: Texture = Texture::new(&FACE_SHEET, SHEET_LOG2, SHEET_LOG2);

/// Pone una expresión: elige la celda de ojos y la de boca.
pub fn set_face(r: &mut DemoRenderer, eyes: Eyes, mouth: Mouth) {
    let cell = |col: i32, row: i32| [(col * CELL_W) as i16, (row * CELL_H) as i16];
    r.materials[FACE_EYES as usize] = Material::textured(TexMap {
        texture: FACE_TEXTURE,
        offset: cell(0, eyes as i32),
        colors: IDENTITY_COLORS,
    });
    let [u, v] = cell(1, mouth as i32);
    r.materials[FACE_MOUTH as usize] = Material::textured(TexMap {
        texture: FACE_TEXTURE,
        offset: [u, v - MOUTH_V0],
        colors: IDENTITY_COLORS,
    });
}

/// Expresión de la demo en el instante `t_ms`: cambia cada 2,4 s y parpadea.
pub fn face_at(t_ms: u32) -> (Eyes, Mouth) {
    const FACES: [(Eyes, Mouth); 4] = [
        (Eyes::Open, Mouth::Smile),
        (Eyes::Happy, Mouth::Grin),
        (Eyes::Open, Mouth::Open),
        (Eyes::Sleepy, Mouth::Neutral),
    ];
    let (eyes, mouth) = FACES[(t_ms / 2400 % 4) as usize];
    let blink = eyes == Eyes::Open && t_ms % 3200 < 140;
    (if blink { Eyes::Blink } else { eyes }, mouth)
}

/// ¿La zona de la hoja bajo el triángulo `uv` (texels Q4) tiene algo que no
/// sea piel en alguna de las celdas de la columna `col`? `v_shift` es el
/// desplazamiento vertical del material (ver [`set_face`]).
fn shows_features(uv: [[i16; 2]; 3], col: i32, v_shift: i32) -> bool {
    // Caja del triángulo con un texel de margen (el muestreo es por centro).
    let lo = |k: usize| uv.iter().map(|t| t[k] as i32 >> 4).min().unwrap() - 1;
    let hi = |k: usize| uv.iter().map(|t| t[k] as i32 >> 4).max().unwrap() + 1;
    (0..SHEET / CELL_H).any(|row| {
        (lo(1)..=hi(1)).any(|v| {
            (lo(0)..=hi(0)).any(|u| {
                let (x, y) = (u + col * CELL_W, v + row * CELL_H + v_shift);
                FACE_TEXTURE.get(x as u32, y as u32) != BODY
            })
        })
    })
}

/// Convierte el frente del cuerpo (vértices `0..verts`, triángulos
/// `0..tris`) en la cara: UV por proyección frontal y los triángulos de la
/// zona de ojos y de boca pasan a los materiales con textura. Los que en
/// ninguna expresión muestran más que piel se quedan lisos: se ven igual y
/// el rasterizado liso es más barato.
fn texture_face(mesh: &mut DemoMesh, verts: usize, tris: usize) {
    for i in 0..verts {
        let [x, y, _] = mesh.positions[i];
        mesh.uvs[i] = [
            ((x as i32 - FACE_X0) * 16 / TEXEL) as i16,
            ((FACE_Y0 - y as i32) * 16 / TEXEL) as i16,
        ];
    }
    for t in 0..tris {
        let p = mesh.tris[t].map(|i| mesh.positions[i as usize]);
        // De frente (z < 0 mira a la cámara) y sin llegar a los costados.
        if !p.iter().all(|v| v[2] < 0 && v[0].abs() <= 175) {
            continue;
        }
        let uv = mesh.tris[t].map(|i| mesh.uvs[i as usize]);
        if p.iter().all(|v| (-8..=135).contains(&v[1])) {
            if shows_features(uv, 0, 0) {
                mesh.tri_materials[t] = FACE_EYES;
            }
        } else if p.iter().all(|v| (-135..=8).contains(&v[1]))
            && shows_features(uv, 1, -(MOUTH_V0 as i32))
        {
            mesh.tri_materials[t] = FACE_MOUTH;
        }
    }
}

pub struct PetScene {
    pub mesh: DemoMesh,
    order: PixelOrder,
    sky: [u16; HEIGHT],
    shadow: [u16; HEIGHT],
}

pub fn camera() -> Camera {
    Camera::new(WIDTH as i32, HEIGHT as i32, 300, 128)
}

pub fn light() -> Light {
    // Hacia arriba-izquierda-delante, normalizada: (-0.45, 0.72, -0.53).
    Light {
        dir: [-7373, 11796, -8684],
        ambient: 18_000,
    }
}

fn ellipsoid(
    center: [i32; 3],
    radii: [i32; 3],
    detail: (usize, usize),
    material: u8,
    bone: u8,
) -> Ellipsoid {
    Ellipsoid {
        center,
        radii,
        rings: detail.0,
        segments: detail.1,
        material,
        bone,
    }
}

impl PetScene {
    pub fn new(order: PixelOrder) -> Self {
        let mut mesh = DemoMesh::new();
        assert!(mesh.add_ellipsoid(ellipsoid(
            [0, 0, 0],
            [240, 215, 225],
            (10, 16),
            BODY,
            BONE_BODY
        )));
        let (verts, tris) = (mesh.vertex_count(), mesh.tri_count());
        texture_face(&mut mesh, verts, tris);

        let parts = [
            ellipsoid([-140, 190, 0], [55, 90, 45], (6, 10), BODY, BONE_BODY),
            ellipsoid([140, 190, 0], [55, 90, 45], (6, 10), BODY, BONE_BODY),
            ellipsoid([-150, -20, -165], [38, 22, 12], (3, 8), CHEEK, BONE_BODY),
            ellipsoid([150, -20, -165], [38, 22, 12], (3, 8), CHEEK, BONE_BODY),
            ellipsoid([-110, -200, -40], [70, 40, 85], (5, 10), FEET, BONE_FEET),
            ellipsoid([110, -200, -40], [70, 40, 85], (5, 10), FEET, BONE_FEET),
        ];
        for p in parts {
            assert!(
                mesh.add_ellipsoid(p),
                "la malla de demo no cabe en el búfer"
            );
        }

        let mut sky = [0u16; HEIGHT];
        let mut shadow = [0u16; HEIGHT];
        let (top, horizon) = (Rgb(120, 190, 235), Rgb(228, 240, 226));
        let (grass_far, grass_near) = (Rgb(176, 210, 150), Rgb(120, 170, 110));
        for (y, (s, sh)) in sky.iter_mut().zip(shadow.iter_mut()).enumerate() {
            let c = if y < HORIZON {
                top.lerp(horizon, y as u32, HORIZON as u32)
            } else {
                grass_far.lerp(grass_near, (y - HORIZON) as u32, (HEIGHT - HORIZON) as u32)
            };
            *s = order.encode(c.to_565());
            *sh = order.encode(c.lerp(Rgb(20, 40, 30), 2, 5).to_565());
        }
        Self {
            mesh,
            order,
            sky,
            shadow,
        }
    }

    pub fn setup(&self, r: &mut DemoRenderer) {
        let o = self.order;
        r.materials[BODY as usize] =
            Material::toon(Rgb(34, 70, 92), Rgb(118, 208, 168), Rgb(226, 255, 238), o);
        r.materials[INK as usize] =
            Material::toon(Rgb(6, 6, 16), Rgb(26, 26, 44), Rgb(60, 60, 90), o);
        r.materials[WHITE as usize] = Material::toon(
            Rgb(150, 160, 180),
            Rgb(250, 250, 255),
            Rgb(255, 255, 255),
            o,
        );
        r.materials[MOUTH as usize] =
            Material::toon(Rgb(70, 10, 30), Rgb(170, 40, 70), Rgb(230, 110, 130), o);
        r.materials[CHEEK as usize] =
            Material::toon(Rgb(150, 60, 90), Rgb(255, 140, 165), Rgb(255, 210, 220), o);
        r.materials[FEET as usize] =
            Material::toon(Rgb(24, 52, 60), Rgb(72, 150, 122), Rgb(160, 220, 190), o);
        set_face(r, Eyes::Open, Mouth::Smile);
    }

    /// Vista: cámara un poco elevada mirando levemente hacia abajo.
    fn view() -> Transform {
        Transform::new(Mat3::rot_x(deg(-10)), [0, -60, 1150])
    }

    /// Altura del salto (Q8) en el instante `t_ms`.
    fn hop(t_ms: u32) -> i32 {
        let p = (t_ms % HOP_MS) as i64;
        (4 * p * (HOP_MS as i64 - p) * HOP_HEIGHT as i64 / (HOP_MS as i64 * HOP_MS as i64)) as i32
    }

    /// Envía la mascota al renderer en el instante `t_ms`, girada `yaw` extra.
    pub fn submit(&self, r: &mut DemoRenderer, t_ms: u32, yaw: Angle) -> bool {
        let sway = (sin((t_ms.wrapping_mul(11)) as u16) * 22) >> 14; // ±22°
        let model = Transform::new(Mat3::rot_y(yaw.wrapping_add(deg(sway))), [0; 3]);
        let world = Self::view().then(&model);

        let h = Self::hop(t_ms);
        let phase = t_ms % HOP_MS;
        let landing = !(100..=HOP_MS - 100).contains(&phase);
        let (sxz, sy) = if landing {
            (ONE_Q14 * 108 / 100, ONE_Q14 * 86 / 100)
        } else {
            (ONE_Q14 * 98 / 100, ONE_Q14 * 104 / 100)
        };
        // Aplastar respecto a la base del cuerpo, no a su centro.
        let pivot = -215;
        let body = Transform::translate([0, h + pivot, 0])
            .then(&Transform::new(Mat3::scale(sxz, sy, sxz), [0; 3]))
            .then(&Transform::translate([0, -pivot, 0]));

        let (eyes, mouth) = face_at(t_ms);
        set_face(r, eyes, mouth);

        let feet = Transform::translate([0, h, 0]);
        let bones = [world.then(&feet), world.then(&body)];
        r.submit(&self.mesh.mesh(), &bones)
    }

    /// Datos del fondo que cambian por fotograma (la sombra), calculados una
    /// sola vez en lugar de en cada franja.
    pub fn backdrop(&self, t_ms: u32) -> Backdrop {
        let cam = camera();
        let ground = Self::view().apply([0, GROUND_Y, -30]);
        let k = (cam.focal << 16) / ground[2];
        let cx = cam.cx + ((ground[0] as i64 * k as i64) >> 16) as i32;
        let cy = cam.cy - ((ground[1] as i64 * k as i64) >> 16) as i32;
        // La sombra se encoge cuanto más alto está el salto.
        let shrink = 100 - Self::hop(t_ms) * 30 / HOP_HEIGHT;
        let rx = (78 * shrink / 100).max(1) as u32;
        let ry = ((14 * shrink / 100).max(1) as u32).min(SHADOW_MAX_RY as u32);
        let mut half = [0u16; SHADOW_MAX_RY];
        for (dy, h) in half.iter_mut().enumerate().take(ry as usize) {
            // Semiancho de la elipse a `dy` filas del centro, con 4 bits extra.
            let root = isqrt32((ry * ry - (dy * dy) as u32) << 8);
            *h = (rx * root / (16 * ry)) as u16;
        }
        Backdrop {
            cx,
            cy,
            ry: ry as i32,
            half,
        }
    }

    /// Pinta cielo, césped y la sombra de la mascota en la franja.
    pub fn fill_background(&self, band: &mut Band, bd: &Backdrop) {
        for row in 0..band.height {
            let y = band.y0 + row;
            let line = &mut band.color[row * band.width..(row + 1) * band.width];
            fill16(line, self.sky[y]);
            let dy = (y as i32 - bd.cy).unsigned_abs() as usize;
            if dy < bd.ry as usize {
                let half = bd.half[dy] as i32;
                let x0 = (bd.cx - half).clamp(0, band.width as i32) as usize;
                let x1 = (bd.cx + half).clamp(0, band.width as i32) as usize;
                fill16(&mut line[x0..x1], self.shadow[y]);
            }
        }
    }
}

const SHADOW_MAX_RY: usize = 16;

/// Sombra elíptica precalculada para un fotograma.
pub struct Backdrop {
    cx: i32,
    cy: i32,
    ry: i32,
    half: [u16; SHADOW_MAX_RY],
}

/// Dibuja un fotograma completo franja a franja y entrega cada franja a `sink`
/// (copiar a la ventana, enviar por DMA...).
pub fn render_frame(
    scene: &PetScene,
    r: &mut DemoRenderer,
    t_ms: u32,
    yaw: Angle,
    color: &mut [u16; BAND_PIXELS],
    depth: &mut [u16; BAND_PIXELS],
    mut sink: impl FnMut(&Band),
) {
    r.begin_frame();
    scene.submit(r, t_ms, yaw);
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
        scene.fill_background(&mut band, &backdrop);
        if r.touches_rows(band.y0, band.y1()) {
            r.clear_depth(&mut band);
            r.render_band(&mut band);
        }
        sink(&band);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_triangles_with_features_are_textured() {
        let scene = PetScene::new(PixelOrder::Native);
        let mesh = scene.mesh.mesh();
        let count = |m: u8| mesh.tri_materials.iter().filter(|&&x| x == m).count();
        let (eyes, mouth) = (count(FACE_EYES), count(FACE_MOUTH));
        // La zona de la cara tiene 16 triángulos para los ojos y 16 para la
        // boca; los ojos cruzan bordes de triángulos y los ocupan todos.
        assert_eq!(eyes, 16);
        assert!(mouth > 0 && mouth < 16, "boca: {mouth}");
    }
}
