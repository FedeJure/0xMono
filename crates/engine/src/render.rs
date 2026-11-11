//! Pipeline: transformar y proyectar los vértices de una malla (en un búfer
//! de trabajo), descartar caras traseras y triángulos fuera del recorte, y
//! guardar solo los triángulos visibles con sus vértices, ya sombreados.
//! Luego cada franja se rasteriza por separado con [`Renderer::render_band`].
//!
//! Memoria: los búferes del fotograma guardan solo lo visible (en el hábitat,
//! una fracción de lo enviado), así que se dimensionan por lo que se ve y no
//! por el tamaño de la escena.

use crate::fixed::{Mat3, Transform};
pub use crate::material::MAX_MATERIALS;
use crate::material::{LUT_LEN, Material, Palette, RAMP_LEN, TEX_COLORS, palette_of};

/// Materiales con textura por renderer (cada uno tiene su paleta en RAM).
pub const MAX_TEXTURED: usize = 8;
/// Vértices máximos de una malla (tamaño del búfer de trabajo).
pub const MAX_MESH_VERTS: usize = 1024;
use crate::mesh::Mesh;
use crate::raster::{Band, RasterStats, ScreenVert, draw_triangle};

/// Cámara en el origen mirando hacia +Z, con X a la derecha e Y arriba.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// Distancia focal en píxeles.
    pub focal: i32,
    pub cx: i32,
    pub cy: i32,
    /// Plano cercano en Q8. Los triángulos que lo cruzan se descartan.
    pub near: i32,
    pub width: i32,
    pub height: i32,
}

impl Camera {
    pub const fn new(width: i32, height: i32, focal: i32, near_q8: i32) -> Camera {
        // La profundidad inversa (focal·2^16 / z) debe caber en 18 bits.
        assert!(
            (focal << 16) / near_q8 < (1 << 18),
            "plano cercano demasiado cerca"
        );
        Camera {
            focal,
            cx: width / 2,
            cy: height / 2,
            near: near_q8,
            width,
            height,
        }
    }
}

/// Luz direccional + ambiente.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Light {
    /// Dirección HACIA la luz, normalizada, Q14, en espacio de vista.
    pub dir: [i32; 3],
    /// Luz ambiente, Q16 (65536 = todo iluminado).
    pub ambient: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    /// Vértices transformados y, de ellos, los que usa algún triángulo guardado.
    pub verts: u32,
    pub verts_kept: u32,
    pub tris_in: u32,
    pub tris_culled: u32,
    pub tris_visible: u32,
    /// Mallas que no entraron enteras en los búferes (deberían ser 0).
    pub dropped: u32,
    /// Ciclos (feature `profile`): transformar vértices y procesar triángulos.
    pub cyc_verts: u32,
    pub cyc_tris: u32,
    pub raster: RasterStats,
}

#[derive(Clone, Copy, Debug, Default)]
struct TriRef {
    v: [u16; 3],
    mat: u8,
    /// Filas y columnas de pantalla que toca (extremo exclusivo).
    row0: i16,
    row1: i16,
    col0: i16,
    col1: i16,
    /// Tono plano del triángulo (índice en la rampa, Q13), o `NO_FLAT`.
    flat: u16,
}

const NO_FLAT: u16 = u16::MAX;

/// Rectángulo de pantalla (en píxeles, extremo exclusivo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Bounds {
    pub const EMPTY: Bounds = Bounds {
        x0: i32::MAX,
        y0: i32::MAX,
        x1: i32::MIN,
        y1: i32::MIN,
    };

    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Bounds {
        Bounds { x0, y0, x1, y1 }
    }

    pub fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    /// El rectángulo más chico que contiene a los dos.
    pub fn union(&self, o: &Bounds) -> Bounds {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        Bounds {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }

    pub fn intersect(&self, o: &Bounds) -> Bounds {
        Bounds {
            x0: self.x0.max(o.x0),
            y0: self.y0.max(o.y0),
            x1: self.x1.min(o.x1),
            y1: self.y1.min(o.y1),
        }
    }

    pub fn overlaps(&self, o: &Bounds) -> bool {
        !self.intersect(o).is_empty()
    }

    pub fn width(&self) -> i32 {
        (self.x1 - self.x0).max(0)
    }

    pub fn height(&self) -> i32 {
        (self.y1 - self.y0).max(0)
    }

    pub fn area(&self) -> i32 {
        self.width() * self.height()
    }

    /// Agranda el rectángulo `m` píxeles por lado.
    pub fn grow(&self, m: i32) -> Bounds {
        if self.is_empty() {
            return *self;
        }
        Bounds {
            x0: self.x0 - m,
            y0: self.y0 - m,
            x1: self.x1 + m,
            y1: self.y1 + m,
        }
    }
}

/// Vértice de la malla en curso ya proyectado (búfer de trabajo).
#[derive(Clone, Copy, Debug)]
struct Projected {
    x: i32,
    y: i32,
    d: i32,
    /// Dónde quedó en la lista del fotograma, o `UNUSED` si todavía no lo usa
    /// ningún triángulo guardado.
    idx: u16,
}

const UNUSED: u16 = u16::MAX;

/// `V` vértices y `T` triángulos visibles máximos por fotograma, sin memoria
/// dinámica.
pub struct Renderer<const V: usize, const T: usize> {
    pub camera: Camera,
    pub light: Light,
    pub materials: [Material; MAX_MATERIALS],
    /// Paletas de los materiales con textura (se arman en `begin_frame`) y
    /// qué paleta usa cada material.
    palettes: [Palette; MAX_TEXTURED],
    palette_of_material: [u8; MAX_MATERIALS],
    verts: [ScreenVert; V],
    nverts: usize,
    tris: [TriRef; T],
    ntris: usize,
    scratch: [Projected; MAX_MESH_VERTS],
    /// Zona ocupada por lo enviado desde el último `take_bounds`, y por lo
    /// anterior del fotograma.
    bounds: Bounds,
    earlier: Bounds,
    /// Solo se guardan los triángulos que tocan este rectángulo.
    scissor: Bounds,
    /// Triángulos conservados del fotograma anterior (`begin_frame_keeping`):
    /// se dibujan después de los nuevos.
    kept: usize,
    pub stats: FrameStats,
}

/// Lo enviado hasta cierto momento del fotograma (ver [`Renderer::mark`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    nverts: usize,
    ntris: usize,
    bounds: Bounds,
}

/// Marca de vértice detrás del plano cercano.
const BEHIND: i32 = i32::MIN;

/// Huesos máximos por malla para la tabla de luz.
pub const MAX_BONES: usize = 128;

#[inline(always)]
fn bone_index(mesh: &Mesh, vertex: usize, nbones: usize) -> usize {
    (mesh.bones.get(vertex).copied().unwrap_or(0) as usize).min(nbones - 1)
}

/// La dirección de la luz vista desde el hueso (`Mᵀ · luz`, Q14): así
/// `normal · luz_local` da lo mismo que `(M · normal) · luz` con 3
/// multiplicaciones por cara en vez de 12.
#[inline(always)]
fn local_light(m: &Mat3, light: &Light) -> [i32; 3] {
    m.transpose().apply(light.dir)
}

/// Lambert + ambiente en aritmética de 32 bits, con la luz en el espacio de
/// la normal. Devuelve el índice fraccionario en la rampa (Q16).
#[inline(always)]
fn shade_normal(nrm: [i8; 3], l: &[i32; 3], light: &Light) -> i32 {
    // Normal (i8, 127 = 1) · luz (Q14) → Q21; ≤ 2^22 en valor absoluto.
    let dot = nrm[0] as i32 * l[0] + nrm[1] as i32 * l[1] + nrm[2] as i32 * l[2];
    let diffuse = ((dot.max(0) >> 5) as u32).min(65535); // Q16
    let unlit = (65536 - light.ambient) as u32;
    let intensity = light.ambient as u32 + ((diffuse * unlit) >> 16);
    (intensity * (RAMP_LEN as u32 - 1)) as i32
}

/// Coordenada de pantalla máxima (en píxeles) antes de recortar: evita desbordes.
const GUARD: i64 = 2048;

impl<const V: usize, const T: usize> Renderer<V, T> {
    pub const fn new(camera: Camera, light: Light) -> Self {
        assert!(
            V < UNUSED as usize,
            "demasiados vértices para índices de 16 bits"
        );
        Self {
            camera,
            light,
            materials: [Material::BLACK; MAX_MATERIALS],
            palettes: [[0; TEX_COLORS * LUT_LEN]; MAX_TEXTURED],
            palette_of_material: [0; MAX_MATERIALS],
            verts: [ScreenVert {
                x: 0,
                y: 0,
                d: 0,
                s: 0,
                u: 0,
                v: 0,
            }; V],
            nverts: 0,
            tris: [TriRef {
                v: [0; 3],
                mat: 0,
                row0: 0,
                row1: 0,
                col0: 0,
                col1: 0,
                flat: NO_FLAT,
            }; T],
            ntris: 0,
            scratch: [Projected {
                x: 0,
                y: 0,
                d: 0,
                idx: UNUSED,
            }; MAX_MESH_VERTS],
            bounds: Bounds::EMPTY,
            earlier: Bounds::EMPTY,
            scissor: Bounds::new(0, 0, camera.width, camera.height),
            kept: 0,
            stats: FrameStats {
                verts: 0,
                verts_kept: 0,
                tris_in: 0,
                tris_culled: 0,
                tris_visible: 0,
                dropped: 0,
                cyc_verts: 0,
                cyc_tris: 0,
                raster: RasterStats {
                    setups: 0,
                    spans: 0,
                    pixels: 0,
                    cyc_setup: 0,
                    cyc_rows: 0,
                    cyc_pixels: 0,
                    tex_pixels: 0,
                    cyc_tex_pixels: 0,
                },
            },
        }
    }

    /// Empieza un fotograma. Los colores de los materiales que usan las
    /// texturas se leen acá: cambiarlos después no afecta a este fotograma
    /// (cambiar el desplazamiento de una textura sí).
    pub fn begin_frame(&mut self) {
        self.begin_frame_keeping(Mark {
            nverts: 0,
            ntris: 0,
            bounds: Bounds::EMPTY,
        });
    }

    /// Lo enviado hasta ahora en este fotograma, para conservarlo en los
    /// siguientes con [`begin_frame_keeping`](Self::begin_frame_keeping).
    pub fn mark(&self) -> Mark {
        Mark {
            nverts: self.nverts,
            ntris: self.ntris,
            bounds: self.earlier.union(&self.bounds),
        }
    }

    /// Marca lo enviado hasta ahora como conservado (igual que si viniera
    /// de `begin_frame_keeping`): se dibuja después de lo que siga, y la
    /// zona de `take_bounds` empieza de cero. Devuelve la marca.
    pub fn keep_so_far(&mut self) -> Mark {
        let m = self.mark();
        self.kept = m.ntris;
        self.earlier = m.bounds;
        self.bounds = Bounds::EMPTY;
        m
    }

    /// Empieza un fotograma conservando lo enviado hasta `mark` en uno
    /// anterior, sin volver a transformarlo: con la cámara quieta, el
    /// escenario proyectado es el mismo. Lo nuevo se dibuja antes que lo
    /// conservado (suele estar adelante: menos píxeles pintados de más).
    /// Si cambió la luz, la cámara o lo conservado, hay que reenviarlo.
    pub fn begin_frame_keeping(&mut self, mark: Mark) {
        self.nverts = mark.nverts;
        self.ntris = mark.ntris;
        self.kept = mark.ntris;
        self.bounds = Bounds::EMPTY;
        self.earlier = mark.bounds;
        self.scissor = self.screen();
        self.stats = FrameStats::default();
        let mut slot = 0;
        for (i, m) in self.materials.iter().enumerate() {
            if let Some(map) = &m.texture {
                debug_assert!(slot < MAX_TEXTURED, "demasiados materiales con textura");
                let s = slot.min(MAX_TEXTURED - 1);
                self.palettes[s] = palette_of(&self.materials, &map.colors);
                self.palette_of_material[i] = s as u8;
                slot += 1;
            }
        }
    }

    /// La pantalla entera.
    pub fn screen(&self) -> Bounds {
        Bounds::new(0, 0, self.camera.width, self.camera.height)
    }

    /// Zona de pantalla que ocupa lo enviado en este fotograma. Fuera de ella
    /// basta con el fondo: no hace falta redibujar ni reenviar al display.
    pub fn bounds(&self) -> Bounds {
        self.earlier.union(&self.bounds).intersect(&self.screen())
    }

    /// Zona ocupada por lo enviado desde la llamada anterior (o desde
    /// `begin_frame`): sirve para saber dónde quedó algo en particular, como
    /// el personaje. [`bounds`](Self::bounds) sigue cubriendo todo el fotograma.
    pub fn take_bounds(&mut self) -> Bounds {
        let b = self.bounds;
        self.earlier = self.earlier.union(&b);
        self.bounds = Bounds::EMPTY;
        b.intersect(&self.screen())
    }

    /// Desde ahora solo se guardan los triángulos que tocan `rect`: al
    /// redibujar una parte de la pantalla, el resto no hace falta.
    pub fn set_scissor(&mut self, rect: Bounds) {
        self.scissor = rect.intersect(&self.screen());
    }

    pub fn scissor(&self) -> Bounds {
        self.scissor
    }

    /// Vértices y triángulos guardados en este fotograma (para dimensionar
    /// `V` y `T`).
    pub fn usage(&self) -> (usize, usize) {
        (self.nverts, self.ntris)
    }

    /// Transforma y encola una malla. `bones[i]` es la transformación completa
    /// (vista · modelo · hueso) de los vértices con hueso `i`.
    ///
    /// Devuelve `false` si no cupo entera en los búferes.
    pub fn submit(&mut self, mesh: &Mesh, bones: &[Transform]) -> bool {
        self.submit_with_base(mesh, bones, 0)
    }

    /// Como [`submit`](Self::submit), sumando `material_base` a los materiales
    /// de la malla: así varias mallas (personaje, escenario) comparten la
    /// tabla de materiales sin pisarse.
    #[cfg_attr(feature = "iram", unsafe(link_section = ".rwtext"))]
    #[inline(never)]
    pub fn submit_with_base(
        &mut self,
        mesh: &Mesh,
        bones: &[Transform],
        material_base: u8,
    ) -> bool {
        let n = mesh.positions.len();
        if n > MAX_MESH_VERTS || bones.is_empty() {
            self.stats.dropped += 1;
            return false;
        }
        self.stats.verts += n as u32;

        debug_assert!(mesh.is_valid(), "malla fuera de formato");
        let cam = self.camera;
        let clip = self.scissor;
        if clip.is_empty() {
            return true;
        }

        let t0 = crate::raster::cycles::now();
        // 1) Posiciones → pantalla, al búfer de trabajo. La luz y las UV se
        //    calculan después, solo para los vértices de triángulos guardados.
        for (i, out) in self.scratch[..n].iter_mut().enumerate() {
            let bone = mesh.bones.get(i).copied().unwrap_or(0) as usize;
            let xf = &bones[bone.min(bones.len() - 1)];
            let p = mesh.positions[i];
            let p = xf.apply_small([p[0] as i32, p[1] as i32, p[2] as i32]);
            out.idx = UNUSED;
            if p[2] < cam.near {
                out.d = BEHIND;
                continue;
            }

            // k = focal / z_real · 256  →  x_pantalla(16.16) = X_q8 · k.
            // División de 32 bits: instrucción de hardware.
            let k = (cam.focal << 16) / p[2];
            let px = (p[0] as i64 * k as i64).clamp(-GUARD << 16, GUARD << 16) as i32;
            let py = (p[1] as i64 * k as i64).clamp(-GUARD << 16, GUARD << 16) as i32;
            out.x = (cam.cx << 16) + px;
            out.y = (cam.cy << 16) - py;
            out.d = k << 6;
        }

        let t1 = crate::raster::cycles::now();
        self.stats.cyc_verts = self.stats.cyc_verts.wrapping_add(t1.wrapping_sub(t0));
        let nb = bones.len().min(MAX_BONES);
        let mut lights = [[0i32; 3]; MAX_BONES];
        for (l, b) in lights.iter_mut().zip(bones) {
            *l = local_light(&b.m, &self.light);
        }

        // 2) Triángulos: descartar y guardar los visibles con sus vértices.
        for (t, tri) in mesh.tris.iter().enumerate() {
            self.stats.tris_in += 1;
            let [i0, i1, i2] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let (a, b, c) = (&self.scratch[i0], &self.scratch[i1], &self.scratch[i2]);
            if a.d == BEHIND || b.d == BEHIND || c.d == BEHIND {
                self.stats.tris_culled += 1;
                continue;
            }
            // Cara trasera: en pantalla (Y hacia abajo) las caras frontales dan det > 0.
            let det = ((b.x - a.x) >> 12) as i64 * ((c.y - a.y) >> 12) as i64
                - ((c.x - a.x) >> 12) as i64 * ((b.y - a.y) >> 12) as i64;
            if det <= 0 {
                self.stats.tris_culled += 1;
                continue;
            }
            let ymin = a.y.min(b.y).min(c.y);
            let ymax = a.y.max(b.y).max(c.y);
            let xmin = a.x.min(b.x).min(c.x);
            let xmax = a.x.max(b.x).max(c.x);
            let row0 = ((ymin + 0x7FFF) >> 16).max(clip.y0);
            let row1 = ((ymax + 0x7FFF) >> 16).min(clip.y1);
            let col0 = ((xmin + 0x7FFF) >> 16).max(clip.x0);
            let col1 = ((xmax + 0x7FFF) >> 16).min(clip.x1);
            if row0 >= row1 || col0 >= col1 {
                self.stats.tris_culled += 1;
                continue;
            }
            if self.ntris == T || self.nverts + 3 > V {
                self.stats.dropped += 1;
                return false;
            }
            // Iluminación por cara si la malla trae normales de triángulo.
            let flat = match mesh.tri_normals.get(t) {
                Some(&n) => {
                    let s = shade_normal(n, &lights[bone_index(mesh, i0, nb)], &self.light);
                    (s >> 3) as u16
                }
                None => NO_FLAT,
            };
            let mut v = [0u16; 3];
            for (k, i) in [i0, i1, i2].into_iter().enumerate() {
                let p = &mut self.scratch[i];
                if p.idx == UNUSED {
                    let s = if flat == NO_FLAT {
                        let l = &lights[bone_index(mesh, i, nb)];
                        (shade_normal(mesh.normals[i], l, &self.light) >> 3) as u16
                    } else {
                        0
                    };
                    let [u, v] = mesh.uvs.get(i).copied().unwrap_or([0, 0]);
                    self.verts[self.nverts] = ScreenVert {
                        x: p.x,
                        y: p.y,
                        d: p.d,
                        s,
                        u,
                        v,
                    };
                    p.idx = self.nverts as u16;
                    self.nverts += 1;
                    self.stats.verts_kept += 1;
                }
                v[k] = p.idx;
            }
            self.tris[self.ntris] = TriRef {
                v,
                mat: material_base.wrapping_add(mesh.tri_materials.get(t).copied().unwrap_or(0)),
                row0: row0 as i16,
                row1: row1 as i16,
                col0: col0 as i16,
                col1: col1 as i16,
                flat,
            };
            self.ntris += 1;
            self.stats.tris_visible += 1;
            let bb = &mut self.bounds;
            bb.x0 = bb.x0.min(col0);
            bb.x1 = bb.x1.max(col1);
            bb.y0 = bb.y0.min(row0);
            bb.y1 = bb.y1.max(row1);
        }
        let t2 = crate::raster::cycles::now();
        self.stats.cyc_tris = self.stats.cyc_tris.wrapping_add(t2.wrapping_sub(t1));
        true
    }

    /// ¿Hay algún triángulo en las filas `y0..y1`? Si no, la franja es solo fondo.
    pub fn touches_rows(&self, y0: usize, y1: usize) -> bool {
        let b = self.bounds();
        !b.is_empty() && (b.y0 as usize) < y1 && (b.y1 as usize) > y0
    }

    /// Prepara el z-buffer de la franja limpiando solo la zona con geometría.
    pub fn clear_depth(&self, band: &mut Band) {
        let b = self.bounds();
        if !b.is_empty() {
            band.clear_depth_region(b.x0 as usize, b.x1 as usize, b.y0 as usize, b.y1 as usize);
        }
    }

    /// Rasteriza en la franja todos los triángulos que la tocan. El color de
    /// fondo y el z-buffer deben estar ya preparados.
    pub fn render_band(&mut self, band: &mut Band) {
        let (y0, y1) = (band.y0 as i16, band.y1() as i16);
        let (x0, x1) = (band.x0 as i16, band.x1() as i16);
        let (kept, new) = self.tris[..self.ntris].split_at(self.kept);
        for t in new.iter().chain(kept) {
            if t.row1 <= y0 || t.row0 >= y1 || t.col1 <= x0 || t.col0 >= x1 {
                continue;
            }
            let v = [
                &self.verts[t.v[0] as usize],
                &self.verts[t.v[1] as usize],
                &self.verts[t.v[2] as usize],
            ];
            let m = t.mat as usize % MAX_MATERIALS;
            let palette = &self.palettes[self.palette_of_material[m] as usize];
            let flat = if t.flat == NO_FLAT {
                None
            } else {
                Some((t.flat as i32) << 3)
            };
            draw_triangle(
                band,
                v,
                &self.materials[m],
                palette,
                flat,
                &mut self.stats.raster,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{PixelOrder, Rgb};

    type R = Renderer<64, 64>;

    // Un cuadrado de 1 m (dos triángulos que comparten vértices).
    const P: [[i16; 3]; 4] = [
        [-128, -128, 0],
        [128, -128, 0],
        [128, 128, 0],
        [-128, 128, 0],
    ];
    const TRIS: [[u16; 3]; 2] = [[0, 2, 1], [0, 3, 2]];
    const NORMALS: [[i8; 3]; 2] = [[0, 0, -127]; 2];
    const QUAD: Mesh = Mesh {
        positions: &P,
        normals: &[],
        bones: &[],
        uvs: &[],
        tris: &TRIS,
        tri_normals: &NORMALS,
        tri_materials: &[],
    };

    fn renderer() -> R {
        let mut r = R::new(
            Camera::new(64, 64, 64, 64),
            Light {
                dir: [0, 0, -16384],
                ambient: 30000,
            },
        );
        let c = Rgb(200, 100, 50);
        r.materials[0] = Material::toon(c, c, c, PixelOrder::Native);
        r
    }

    /// Un cuadrado a la izquierda y otro a la derecha, a 4 m.
    fn two_quads(r: &mut R) -> (Bounds, Bounds) {
        r.submit(&QUAD, &[Transform::translate([-600, 0, 4096])]);
        let left = r.take_bounds();
        r.submit(&QUAD, &[Transform::translate([600, 0, 4096])]);
        (left, r.take_bounds())
    }

    #[test]
    fn keeps_only_vertices_of_visible_triangles() {
        let mut r = renderer();
        r.begin_frame();
        let (left, right) = two_quads(&mut r);
        assert!(!left.is_empty() && !right.is_empty());
        assert_eq!(r.usage(), (8, 4), "cada cuadrado guarda sus 4 vértices");
        // Con un recorte sobre el de la izquierda, el otro no se guarda.
        r.begin_frame();
        r.set_scissor(Bounds::new(0, 0, 32, 64));
        two_quads(&mut r);
        assert_eq!(r.usage(), (4, 2));
        // Y lo que queda se recorta al rectángulo.
        r.begin_frame();
        r.set_scissor(Bounds::new(left.x0 + 2, 0, left.x0 + 4, 64));
        two_quads(&mut r);
        assert_eq!(r.bounds().width(), 2);
    }

    #[test]
    fn take_bounds_splits_the_frame() {
        let mut r = renderer();
        r.begin_frame();
        let (left, right) = two_quads(&mut r);
        assert!(left.x1 <= right.x0, "{left:?} {right:?}");
        assert_eq!(r.bounds(), left.union(&right));
    }
}
