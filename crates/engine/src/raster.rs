//! Rasterizado de triángulos por scanlines sobre una franja (band) de la pantalla.
//!
//! Por qué franjas: un fotograma completo de 240×240 en RGB565 ocupa 115 KB y
//! su z-buffer otros 115 KB; el C3 tiene ~320 KB de RAM de datos. Con franjas
//! de 240×24 se usan ~23 KB, y mientras el DMA envía una franja al ST7789 la
//! CPU dibuja la siguiente.
//!
//! Convenciones: centros de píxel en (x+0.5, y+0.5), regla de relleno
//! superior-izquierda (ningún píxel se pinta dos veces entre triángulos
//! vecinos), profundidad ∝ 1/z (mayor = más cerca), interpolada linealmente en
//! pantalla, lo que es exacto para 1/z.

use crate::material::{LUT_LEN, Material, Palette, TexMap, Texture};

/// Umbrales de Bayer 4×4 en Q16, centrados en 0 para no aclarar la imagen:
/// `DITHER[fila & 3][x & 3]` es la fila de umbrales rotada para que el
/// elemento 0 corresponda a la columna `x`. Vive en RAM (`.data`): leerla
/// desde flash pasa por la caché en cada tramo.
#[cfg_attr(feature = "iram", unsafe(link_section = ".data"))]
static DITHER: [[[i32; 4]; 4]; 4] = {
    const B: [[i32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    let mut t = [[[0i32; 4]; 4]; 4];
    let mut y = 0;
    while y < 4 {
        let mut phase = 0;
        while phase < 4 {
            let mut k = 0;
            while k < 4 {
                t[y][phase][k] = B[y][(phase + k) & 3] * 4096 + 2048 - 32768;
                k += 1;
            }
            phase += 1;
        }
        y += 1;
    }
    t
};

/// Contador de ciclos del ESP32-C3 (solo con la feature `profile`).
pub mod cycles {
    /// Activa el contador de ciclos de máquina (PCER/PCMR del ESP32-C3).
    pub fn enable() {
        #[cfg(all(feature = "profile", target_arch = "riscv32"))]
        unsafe {
            core::arch::asm!("csrw 0x7e0, {0}", "csrw 0x7e1, {0}", in(reg) 1u32);
        }
    }

    #[inline(always)]
    pub fn now() -> u32 {
        #[cfg(all(feature = "profile", target_arch = "riscv32"))]
        {
            let c: u32;
            unsafe { core::arch::asm!("csrr {0}, 0x7e2", out(reg) c) };
            c
        }
        #[cfg(not(all(feature = "profile", target_arch = "riscv32")))]
        0
    }
}

/// Una franja horizontal de la pantalla (o de un rectángulo de ella) con su
/// z-buffer. Los búferes tienen `width` columnas por fila.
pub struct Band<'a> {
    pub color: &'a mut [u16],
    pub depth: &'a mut [u16],
    /// Primera columna de pantalla que cubre la franja.
    pub x0: usize,
    pub width: usize,
    /// Primera fila de pantalla que cubre la franja.
    pub y0: usize,
    pub height: usize,
}

impl Band<'_> {
    pub fn clear_depth(&mut self) {
        fill16(&mut self.depth[..self.width * self.height], 0);
    }

    /// Limpia el z-buffer solo en las columnas de pantalla `x0..x1` de las
    /// filas `y0..y1` que caen dentro de la franja (la zona con geometría).
    pub fn clear_depth_region(&mut self, x0: usize, x1: usize, y0: usize, y1: usize) {
        let x0 = x0.max(self.x0) - self.x0;
        let x1 = x1.min(self.x1()).saturating_sub(self.x0);
        let (rows_from, rows_to) = (y0.max(self.y0), y1.min(self.y1()));
        if x0 >= x1 {
            return;
        }
        for row in rows_from..rows_to {
            let off = (row - self.y0) * self.width;
            fill16(&mut self.depth[off + x0..off + x1], 0);
        }
    }

    pub fn x1(&self) -> usize {
        self.x0 + self.width
    }

    pub fn y1(&self) -> usize {
        self.y0 + self.height
    }
}

/// Rellena `buf` con `v` escribiendo de a 32 bits (dos píxeles por store).
#[cfg_attr(feature = "iram", unsafe(link_section = ".rwtext"))]
#[inline(never)]
pub fn fill16(buf: &mut [u16], v: u16) {
    // SAFETY: cualquier patrón de bits es un `u32` válido y `align_to_mut`
    // garantiza la alineación de la parte central.
    let (head, words, tail) = unsafe { buf.align_to_mut::<u32>() };
    head.fill(v);
    tail.fill(v);
    let w = v as u32 * 0x0001_0001;
    // De 8 en 8: el bucle simple gasta 4 instrucciones de control por store.
    let mut chunks = words.chunks_exact_mut(8);
    for c in &mut chunks {
        c.copy_from_slice(&[w; 8]);
    }
    for x in chunks.into_remainder() {
        *x = w;
    }
}

/// Vértice ya proyectado. 20 bytes: el renderer guarda miles por fotograma.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScreenVert {
    /// Pantalla en 16.16.
    pub x: i32,
    pub y: i32,
    /// Profundidad inversa (mayor = más cerca), < 2^24.
    pub d: i32,
    /// Índice fraccionario en la rampa del material, Q13 (0 ..= 7·8192).
    pub s: u16,
    /// Coordenadas de textura en texels, Q4 (como en la malla).
    pub u: i16,
    pub v: i16,
}

impl ScreenVert {
    /// Tono en Q16.
    #[inline(always)]
    fn shade(&self) -> i32 {
        (self.s as i32) << 3
    }

    /// Coordenadas de textura en 16.16.
    #[inline(always)]
    fn tex(&self) -> (i32, i32) {
        ((self.u as i32) << 12, (self.v as i32) << 12)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RasterStats {
    /// Llamadas a `draw_triangle` (un triángulo que cruza dos franjas cuenta dos).
    pub setups: u32,
    pub spans: u32,
    pub pixels: u32,
    /// Ciclos (feature `profile`): preparación, manejo de filas, bucle de píxeles.
    pub cyc_setup: u32,
    pub cyc_rows: u32,
    pub cyc_pixels: u32,
    /// Parte de `pixels` y `cyc_pixels` que corresponde a triángulos con textura.
    pub tex_pixels: u32,
    pub cyc_tex_pixels: u32,
}

/// Recíproco normalizado de `det` (> 0, < 2^32) con UNA división de 32 bits,
/// que el C3 hace por hardware. Devuelve `(q, sh)` tal que `x / det ≈ (x·q) >> sh`,
/// con ~16 bits de precisión relativa.
///
/// Evita `i64 / i64`, que en RV32 es una rutina de software (`__divdi3`) de
/// cientos de ciclos y que además vive en flash, fuera de IRAM.
#[inline(always)]
fn reciprocal(det: u32) -> (i64, u32) {
    let n = det.leading_zeros();
    let norm = det << n; // [2^31, 2^32)
    let q = u32::MAX / (norm >> 16); // (2^16, 2^17]
    (q as i64, 48 - n)
}

/// Gradientes por píxel `(da/dx, da/dy)` de un atributo. Coordenadas en 28.4.
#[inline(always)]
fn gradient(
    (dx1, dy1, dx2, dy2): (i64, i64, i64, i64),
    da1: i64,
    da2: i64,
    (q, sh, neg): (i64, u32, bool),
) -> (i32, i32) {
    // da/dx = (da1·dy2 − da2·dy1)·16 / det   (×16: de unidades 28.4 a píxeles)
    let nx = da1 * dy2 - da2 * dy1;
    let ny = dx1 * da2 - dx2 * da1;
    let (gx, gy) = ((nx * q) >> (sh - 4), (ny * q) >> (sh - 4));
    if neg {
        (-gx as i32, -gy as i32)
    } else {
        (gx as i32, gy as i32)
    }
}

/// Pendiente dx/dy de una arista en 16.16, con una división de 32 bits.
/// Precisión de 1/4096 px por fila: < 0.06 px de deriva en 240 filas.
#[inline(always)]
fn slope(a: &ScreenVert, b: &ScreenVert) -> i32 {
    let dy = (b.y - a.y) >> 12; // 28.4
    if dy == 0 {
        return 0;
    }
    let dx = (b.x - a.x) >> 4; // 20.12, |dx| < 2^24 dentro de la banda de guarda
    ((dx << 4) / dy) << 4
}

/// X de la arista que parte de `a` en la fila cuyo centro es `yc` (16.16).
#[inline(always)]
fn edge_x(a: &ScreenVert, slope: i32, yc: i32) -> i32 {
    a.x + ((slope as i64 * (yc - a.y) as i64) >> 16) as i32
}

/// Valor de un atributo en el centro del píxel x = 0 de la fila `yc`.
/// Puede salirse de rango (es una extrapolación): se trabaja con aritmética
/// modular (`wrapping_*`) y el valor final, ya dentro del triángulo, es exacto.
#[inline(always)]
fn row_origin(a0: i32, a: &ScreenVert, dadx: i32, dady: i32, yc: i32) -> i32 {
    let dx = (0x8000 - a.x) as i64;
    let dy = (yc - a.y) as i64;
    a0.wrapping_add(((dadx as i64 * dx + dady as i64 * dy) >> 16) as i32)
}

/// Un píxel: test de profundidad y, si pasa, el color `$c`.
///
/// `d` < 2^24 por construcción (ver `Camera::new`), así que `d >> 8` cabe en
/// 16 bits y se compara en 32 sin truncar.
macro_rules! depth_test {
    ($px:expr, $z:expr, $d:expr, $c:expr) => {
        let depth = ($d as u32) >> 8;
        if depth > *$z as u32 {
            *$z = depth as u16;
            *$px = $c;
        }
    };
}

/// Rellena un tramo horizontal de `n` píxeles con tono interpolado, de 4 en
/// 4 con los umbrales de dithering en registros. `(s + t) >> 16` ∈ [−1, 7];
/// con `& 15` la tabla con relleno da el tono sin recortes ni límites.
///
/// # Safety
/// `c` y `z` apuntan a `n` píxeles válidos.
#[inline(always)]
unsafe fn draw_span(
    mut c: *mut u16,
    mut z: *mut u16,
    n: usize,
    (mut d, dddx): (i32, i32),
    (mut s, dsdx): (i32, i32),
    th: &[i32; 4],
    lut: &[u16; LUT_LEN],
) {
    let [t0, t1, t2, t3] = *th;
    let tone = |s: i32, t: i32| lut[((s + t) >> 16) as usize & (LUT_LEN - 1)];
    unsafe {
        let end = c.add(n & !3);
        while c < end {
            depth_test!(c, z, d, tone(s, t0));
            d = d.wrapping_add(dddx);
            s = s.wrapping_add(dsdx);
            depth_test!(c.add(1), z.add(1), d, tone(s, t1));
            d = d.wrapping_add(dddx);
            s = s.wrapping_add(dsdx);
            depth_test!(c.add(2), z.add(2), d, tone(s, t2));
            d = d.wrapping_add(dddx);
            s = s.wrapping_add(dsdx);
            depth_test!(c.add(3), z.add(3), d, tone(s, t3));
            d = d.wrapping_add(dddx);
            s = s.wrapping_add(dsdx);
            c = c.add(4);
            z = z.add(4);
        }
        // Resto (0..=3 píxeles): los umbrales siguen el mismo orden.
        let rem = n & 3;
        if rem > 0 {
            depth_test!(c, z, d, tone(s, t0));
        }
        if rem > 1 {
            d = d.wrapping_add(dddx);
            s = s.wrapping_add(dsdx);
            depth_test!(c.add(1), z.add(1), d, tone(s, t1));
        }
        if rem > 2 {
            d = d.wrapping_add(dddx);
            s = s.wrapping_add(dsdx);
            depth_test!(c.add(2), z.add(2), d, tone(s, t2));
        }
    }
}

/// Tramo de tono plano: los 4 colores (tono + dithering) de la fila ya
/// vienen elegidos, así que cada píxel es solo la prueba de profundidad y
/// una escritura.
///
/// # Safety
/// `c` y `z` apuntan a `n` píxeles válidos.
#[inline(always)]
unsafe fn draw_span_flat(
    mut c: *mut u16,
    mut z: *mut u16,
    n: usize,
    (mut d, dddx): (i32, i32),
    k: [u16; 4],
) {
    unsafe {
        let end = c.add(n & !3);
        while c < end {
            depth_test!(c, z, d, k[0]);
            d = d.wrapping_add(dddx);
            depth_test!(c.add(1), z.add(1), d, k[1]);
            d = d.wrapping_add(dddx);
            depth_test!(c.add(2), z.add(2), d, k[2]);
            d = d.wrapping_add(dddx);
            depth_test!(c.add(3), z.add(3), d, k[3]);
            d = d.wrapping_add(dddx);
            c = c.add(4);
            z = z.add(4);
        }
        let rem = n & 3;
        if rem > 0 {
            depth_test!(c, z, d, k[0]);
        }
        if rem > 1 {
            d = d.wrapping_add(dddx);
            depth_test!(c.add(1), z.add(1), d, k[1]);
        }
        if rem > 2 {
            d = d.wrapping_add(dddx);
            depth_test!(c.add(2), z.add(2), d, k[2]);
        }
    }
}

/// Tramo texturizado: el texel elige el material y el sombreado elige el tono
/// dentro de su rampa, con una sola lectura en la paleta contigua. Mapeo afín
/// (sin corrección de perspectiva): en triángulos chicos la diferencia no se
/// ve y ahorra una división por píxel. Con `FLAT`, el tono de cada columna
/// & 3 viene calculado en `tones` y `s` no se usa.
///
/// # Safety
/// `c` y `z` apuntan a `n` píxeles válidos.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
unsafe fn draw_span_tex<const FLAT: bool>(
    mut c: *mut u16,
    mut z: *mut u16,
    n: usize,
    (mut d, dddx): (i32, i32),
    (mut s, dsdx): (i32, i32),
    (mut u, dudx): (i32, i32),
    (mut v, dvdx): (i32, i32),
    th: &[i32; 4],
    tones: [usize; 4],
    tex: &Texture,
    pal: &Palette,
) {
    let texels = tex.texels;
    // Byte del texel = fila · (ancho/2) + x/2, sacados de u y v con un
    // desplazamiento y una máscara cada uno; el bit 16 de u (x impar) elige
    // el nibble.
    let row_log2 = tex.width_log2 as u32 - 1;
    let col_mask = (1u32 << row_log2) - 1;
    let row_mask = ((1u32 << tex.height_log2) - 1) << row_log2;
    let row_shift = 16 - row_log2;
    // Sin desenrollar: con u y v hay demasiadas variables para los registros
    // del RISC-V y el desenrollado de a 4 las mandaba a la pila (medido en el C3).
    for i in 0..n {
        // SAFETY: `i < n`; las máscaras dejan el índice del texel dentro de
        // la textura (largo garantizado por `Texture::new`) y el de la
        // paleta < 16·16.
        unsafe {
            let depth = (d as u32) >> 8;
            if depth > *z as u32 {
                *z = depth as u16;
                let byte_i =
                    (((v as u32) >> row_shift) & row_mask) | (((u as u32) >> 17) & col_mask);
                let byte = *texels.get_unchecked(byte_i as usize) as u32;
                let m = (byte >> ((u as u32 >> 14) & 4)) & 0xF;
                let tone = if FLAT {
                    tones[i & 3]
                } else {
                    ((s + th[i & 3]) >> 16) as usize & (LUT_LEN - 1)
                };
                *c = *pal.get_unchecked(((m as usize) << 4) | tone);
            }
            c = c.add(1);
            z = z.add(1);
        }
        d = d.wrapping_add(dddx);
        if !FLAT {
            s = s.wrapping_add(dsdx);
        }
        u = u.wrapping_add(dudx);
        v = v.wrapping_add(dvdx);
    }
}

/// Dibuja un triángulo en la franja. Los vértices pueden venir en cualquier orden.
/// `palette` son las rampas de los colores de la textura (ver
/// `material::palette_of`): solo la usan los materiales con textura.
/// `flat` fuerza un tono para todo el triángulo (iluminación por cara).
#[cfg_attr(feature = "iram", unsafe(link_section = ".rwtext"))]
#[inline(never)]
pub fn draw_triangle(
    band: &mut Band,
    v: [&ScreenVert; 3],
    mat: &Material,
    palette: &Palette,
    flat: Option<i32>,
    stats: &mut RasterStats,
) {
    // Tres copias del bucle: tono plano (casi todo el escenario), tono
    // interpolado y con textura.
    let flat_tone = flat.is_some() || mat.flat;
    match &mat.texture {
        None if flat_tone => raster::<false, true>(band, v, mat, None, palette, flat, stats),
        None => raster::<false, false>(band, v, mat, None, palette, flat, stats),
        Some(map) if flat_tone => {
            raster::<true, true>(band, v, mat, Some(map), palette, flat, stats)
        }
        Some(map) => raster::<true, false>(band, v, mat, Some(map), palette, flat, stats),
    }
}

#[inline(always)]
fn raster<const TEX: bool, const FLAT: bool>(
    band: &mut Band,
    v: [&ScreenVert; 3],
    mat: &Material,
    map: Option<&TexMap>,
    palette: &Palette,
    flat: Option<i32>,
    stats: &mut RasterStats,
) {
    let t_start = cycles::now();
    let [mut a, mut b, mut c] = v;
    if a.y > b.y {
        core::mem::swap(&mut a, &mut b);
    }
    if b.y > c.y {
        core::mem::swap(&mut b, &mut c);
    }
    if a.y > b.y {
        core::mem::swap(&mut a, &mut b);
    }

    // Filas cuyos centros están en [a.y, c.y), recortadas a la franja.
    let tri_first = (a.y + 0x7FFF) >> 16;
    let first = tri_first.max(band.y0 as i32);
    let end = ((c.y + 0x7FFF) >> 16).min(band.y1() as i32);
    if first >= end {
        return;
    }

    // Gradientes de plano con coordenadas en 28.4 para que los productos quepan.
    let e = (
        ((b.x - a.x) >> 12) as i64,
        ((b.y - a.y) >> 12) as i64,
        ((c.x - a.x) >> 12) as i64,
        ((c.y - a.y) >> 12) as i64,
    );
    let det = e.0 * e.3 - e.2 * e.1;
    if det == 0 || det.unsigned_abs() >= 1 << 32 {
        // Triángulo degenerado o gigantesco (fuera de la banda de guarda).
        return;
    }
    let (q, sh) = reciprocal(det.unsigned_abs() as u32);
    let inv = (q, sh, det < 0);

    let (dddx, dddy) = gradient(e, (b.d - a.d) as i64, (c.d - a.d) as i64, inv);
    let (dsdx, dsdy, s0) = if let Some(s) = flat {
        (0, 0, s)
    } else if mat.flat {
        (0, 0, (a.shade() + b.shade() + c.shade()) / 3)
    } else {
        let (sa, sb, sc) = (a.shade(), b.shade(), c.shade());
        let (gx, gy) = gradient(e, (sb - sa) as i64, (sc - sa) as i64, inv);
        (gx, gy, sa)
    };

    // Con los vértices ordenados, det > 0 ⇔ la arista larga (a→c) queda a la izquierda.
    let long_left = det > 0;
    let long_slope = slope(a, c);
    let upper_slope = slope(a, b);
    let lower_slope = slope(b, c);

    // Estado de la primera fila; después todo avanza sumando. Se calcula en
    // la primera fila del triángulo y se avanza `skip` filas multiplicando
    // (igual a sumar fila por fila): así el resultado no depende de dónde
    // empieza la franja y redibujar un pedazo de pantalla da exactamente los
    // mismos píxeles que el fotograma completo.
    let skip = first - tri_first;
    let yc_tri = (tri_first << 16) + 0x8000;
    let advance = |x: i32, step: i32, rows: i32| x.wrapping_add(step.wrapping_mul(rows));
    let mut yc = (first << 16) + 0x8000;
    let mut x_long = advance(edge_x(a, long_slope, yc_tri), long_slope, skip);
    let mut upper = yc < b.y;
    let (mut x_short, mut short_slope) = if upper {
        (
            advance(edge_x(a, upper_slope, yc_tri), upper_slope, skip),
            upper_slope,
        )
    } else {
        // Primera fila de la mitad de abajo.
        let b_row = ((b.y + 0x7FFF) >> 16).max(tri_first);
        let yc_b = (b_row << 16) + 0x8000;
        (
            advance(edge_x(b, lower_slope, yc_b), lower_slope, first - b_row),
            lower_slope,
        )
    };
    let mut d_row = advance(row_origin(a.d, a, dddx, dddy, yc_tri), dddy, skip);
    let mut s_row = advance(row_origin(s0, a, dsdx, dsdy, yc_tri), dsdy, skip);

    // Coordenadas de textura (solo en la copia texturizada).
    let (mut u_row, mut v_row, mut du, mut dv) = (0, 0, (0, 0), (0, 0));
    let mut tex = None;
    if TEX && let Some(map) = map {
        let ((au, av), (bu, bv), (cu, cv)) = (a.tex(), b.tex(), c.tex());
        du = gradient(e, (bu - au) as i64, (cu - au) as i64, inv);
        dv = gradient(e, (bv - av) as i64, (cv - av) as i64, inv);
        let (ou, ov) = ((map.offset[0] as i32) << 16, (map.offset[1] as i32) << 16);
        u_row = advance(
            row_origin(au.wrapping_add(ou), a, du.0, du.1, yc_tri),
            du.1,
            skip,
        );
        v_row = advance(
            row_origin(av.wrapping_add(ov), a, dv.0, dv.1, yc_tri),
            dv.1,
            skip,
        );
        tex = Some(&map.texture);
    }

    let (left, right) = (band.x0 as i32, band.x1() as i32);
    let lut: &[u16; LUT_LEN] = &mat.lut;
    let dither_rows = &DITHER;
    let mut off = (first as usize - band.y0) * band.width;
    assert!(
        band.color.len() >= band.width * band.height
            && band.depth.len() >= band.width * band.height,
        "franja más chica que sus búferes"
    );
    let (color, depth) = (band.color.as_mut_ptr(), band.depth.as_mut_ptr());
    // Contadores en registros; se vuelcan una vez al final.
    let (mut spans, mut pixels, mut cyc_px) = (0u32, 0u32, 0u32);
    let t_rows = cycles::now();

    for row in first..end {
        if upper && yc >= b.y {
            upper = false;
            x_short = edge_x(b, lower_slope, yc);
            short_slope = lower_slope;
        }
        let (xl, xr) = if long_left {
            (x_long, x_short)
        } else {
            (x_short, x_long)
        };
        let c0 = ((xl + 0x7FFF) >> 16).max(left);
        let c1 = ((xr + 0x7FFF) >> 16).min(right);

        if c0 < c1 {
            let n = (c1 - c0) as usize;
            let i = off + (c0 - left) as usize;
            // SAFETY: c0 ≥ left, c1 ≤ right y la fila está en la franja, así
            // que el tramo cae dentro de los búferes (comprobado arriba).
            let (cp, zp) = unsafe { (color.add(i), depth.add(i)) };
            let th = &dither_rows[row as usize & 3][c0 as usize & 3];
            spans += 1;
            pixels += n as u32;
            let t_px = cycles::now();
            let d0 = (d_row.wrapping_add(dddx.wrapping_mul(c0)), dddx);
            let s0 = (s_row.wrapping_add(dsdx.wrapping_mul(c0)), dsdx);
            // Tono de cada columna & 3 (tono plano + umbral de dithering).
            let tones = th.map(|t| ((s0.0 + t) >> 16) as usize & (LUT_LEN - 1));
            unsafe {
                if TEX && let Some(texture) = tex {
                    draw_span_tex::<FLAT>(
                        cp,
                        zp,
                        n,
                        d0,
                        s0,
                        (u_row.wrapping_add(du.0.wrapping_mul(c0)), du.0),
                        (v_row.wrapping_add(dv.0.wrapping_mul(c0)), dv.0),
                        th,
                        tones,
                        texture,
                        palette,
                    );
                } else if FLAT {
                    draw_span_flat(cp, zp, n, d0, tones.map(|t| lut[t]));
                } else {
                    draw_span(cp, zp, n, d0, s0, th, lut);
                }
            }
            cyc_px = cyc_px.wrapping_add(cycles::now().wrapping_sub(t_px));
        }

        yc += 1 << 16;
        x_long += long_slope;
        x_short += short_slope;
        d_row = d_row.wrapping_add(dddy);
        s_row = s_row.wrapping_add(dsdy);
        if TEX {
            u_row = u_row.wrapping_add(du.1);
            v_row = v_row.wrapping_add(dv.1);
        }
        off += band.width;
    }
    let t_end = cycles::now();
    stats.setups += 1;
    stats.spans += spans;
    stats.pixels += pixels;
    stats.cyc_setup = stats.cyc_setup.wrapping_add(t_rows.wrapping_sub(t_start));
    stats.cyc_rows = stats
        .cyc_rows
        .wrapping_add(t_end.wrapping_sub(t_rows).wrapping_sub(cyc_px));
    stats.cyc_pixels = stats.cyc_pixels.wrapping_add(cyc_px);
    if TEX {
        stats.tex_pixels += pixels;
        stats.cyc_tex_pixels = stats.cyc_tex_pixels.wrapping_add(cyc_px);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{IDENTITY_COLORS, MAX_MATERIALS, PixelOrder, Rgb, Texture, palette_of};

    const W: usize = 16;
    const H: usize = 16;

    fn vert(x: i32, y: i32) -> ScreenVert {
        ScreenVert {
            x: x << 16,
            y: y << 16,
            d: 1000 << 8,
            s: 7 << 13,
            u: 0,
            v: 0,
        }
    }

    fn palette() -> Palette {
        palette_of(&[solid(); MAX_MATERIALS], &IDENTITY_COLORS)
    }

    fn solid() -> Material {
        let c = Rgb(255, 255, 255);
        Material::toon(c, c, c, PixelOrder::Native)
    }

    fn render(tris: &[[ScreenVert; 3]]) -> ([u16; W * H], RasterStats) {
        let mut color = [0u16; W * H];
        let mut depth = [0u16; W * H];
        let mut stats = RasterStats::default();
        let mut band = Band {
            color: &mut color,
            depth: &mut depth,
            x0: 0,
            width: W,
            y0: 0,
            height: H,
        };
        for t in tris {
            draw_triangle(
                &mut band,
                [&t[0], &t[1], &t[2]],
                &solid(),
                &palette(),
                None,
                &mut stats,
            );
        }
        (color, stats)
    }

    #[test]
    fn two_triangles_tile_a_square_without_overlap_or_gaps() {
        // Cuadrado 4..12 × 4..12 partido en dos: 64 píxeles exactos.
        let (a, b, c, d) = (vert(4, 4), vert(12, 4), vert(12, 12), vert(4, 12));
        let (img, stats) = render(&[[a, b, c], [a, c, d]]);
        assert_eq!(img.iter().filter(|&&p| p != 0).count(), 64);
        assert_eq!(stats.pixels, 64, "ningún píxel se evalúa dos veces");
    }

    #[test]
    fn nearer_triangle_wins_regardless_of_order() {
        let near = |mut v: ScreenVert| {
            v.d = 2000 << 8;
            v.s = 0;
            v
        };
        let far = [vert(0, 0), vert(16, 0), vert(0, 16)];
        let close = [near(vert(0, 0)), near(vert(16, 0)), near(vert(0, 16))];
        let (img1, _) = render(&[far, close]);
        let (img2, _) = render(&[close, far]);
        assert_eq!(img1, img2);
        assert_eq!(img1[W + 1], solid().lut[0]);
    }

    #[test]
    fn band_clips_rows() {
        let mut color = [0u16; W * 4];
        let mut depth = [0u16; W * 4];
        let mut stats = RasterStats::default();
        let mut band = Band {
            color: &mut color,
            depth: &mut depth,
            x0: 0,
            width: W,
            y0: 8,
            height: 4,
        };
        let (a, b, c) = (vert(0, 0), vert(16, 0), vert(0, 16));
        draw_triangle(
            &mut band,
            [&a, &b, &c],
            &solid(),
            &palette(),
            None,
            &mut stats,
        );
        assert_eq!(stats.spans, 4);
        // Fila 8 (centro y = 8.5): la hipotenusa está en x = 7.5 → centros 0.5..6.5.
        assert_eq!(color[..W].iter().filter(|&&p| p != 0).count(), 7);
    }

    /// Dibuja `tris` en la zona `x0..x1 × y0..y1` partida en franjas de `rows`
    /// filas, sobre una imagen W×H.
    fn render_area(
        tris: &[[ScreenVert; 3]],
        (x0, x1, y0, y1): (usize, usize, usize, usize),
        rows: usize,
    ) -> [u16; W * H] {
        let mut img = [0u16; W * H];
        let w = x1 - x0;
        let mats: [Material; 3] = [
            Material::from_ramp([
                0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000, 0x7000, 0x8000,
            ]),
            Material::from_ramp([
                0x0100, 0x0200, 0x0300, 0x0400, 0x0500, 0x0600, 0x0700, 0x0800,
            ]),
            Material::from_ramp([
                0x0010, 0x0020, 0x0030, 0x0040, 0x0050, 0x0060, 0x0070, 0x0080,
            ]),
        ];
        let mut stats = RasterStats::default();
        for by in (y0..y1).step_by(rows) {
            let h = rows.min(y1 - by);
            let mut color = [0u16; W * H];
            let mut depth = [0u16; W * H];
            let mut band = Band {
                color: &mut color[..w * h],
                depth: &mut depth[..w * h],
                x0,
                width: w,
                y0: by,
                height: h,
            };
            for (i, t) in tris.iter().enumerate() {
                draw_triangle(
                    &mut band,
                    [&t[0], &t[1], &t[2]],
                    &mats[i % 3],
                    &palette(),
                    None,
                    &mut stats,
                );
            }
            for r in 0..h {
                img[(by + r) * W + x0..(by + r) * W + x1]
                    .copy_from_slice(&color[r * w..(r + 1) * w]);
            }
        }
        img
    }

    #[test]
    fn bands_and_partial_redraws_give_identical_pixels() {
        // Triángulos con posiciones, profundidad y tono "raros" (fracciones
        // de píxel), que se cruzan entre sí.
        let mut seed = 12345u32;
        let mut rnd = |m: i32| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            ((seed >> 8) % m as u32) as i32
        };
        let tris: [[ScreenVert; 3]; 12] = core::array::from_fn(|_| {
            core::array::from_fn(|_| ScreenVert {
                x: rnd(20 << 16) - (2 << 16),
                y: rnd(20 << 16) - (2 << 16),
                d: (500 + rnd(1500)) << 8,
                s: rnd(7 << 13) as u16,
                u: 0,
                v: 0,
            })
        });
        // Solo caras frontales (el renderer descarta las otras antes).
        let tris = tris.map(|[a, b, c]| {
            let det =
                (b.x - a.x) as i64 * (c.y - a.y) as i64 - (c.x - a.x) as i64 * (b.y - a.y) as i64;
            if det < 0 { [a, c, b] } else { [a, b, c] }
        });
        let full = render_area(&tris, (0, W, 0, H), H);
        assert!(full.iter().filter(|&&p| p != 0).count() > 100);
        for rows in [1, 3, 5, 7] {
            assert_eq!(
                render_area(&tris, (0, W, 0, H), rows),
                full,
                "franjas de {rows}"
            );
        }
        // Un rectángulo cualquiera: dentro, igual al completo; fuera, nada.
        let (x0, x1, y0, y1) = (3, 11, 5, 14);
        let part = render_area(&tris, (x0, x1, y0, y1), 4);
        for y in 0..H {
            for x in 0..W {
                let inside = (x0..x1).contains(&x) && (y0..y1).contains(&y);
                let want = if inside { full[y * W + x] } else { 0 };
                assert_eq!(part[y * W + x], want, "({x}, {y})");
            }
        }
    }

    #[test]
    fn texture_picks_material_per_texel() {
        // Textura 2×2: [1 2 / 3 4] estirada sobre el cuadrado 0..16.
        static TEX: [u8; 2] = [0x21, 0x43];
        let colors = [0x1111, 0x2222, 0x3333, 0x4444];
        let mut mats = [solid(); MAX_MATERIALS];
        for (i, c) in colors.iter().enumerate() {
            mats[i + 1] = Material::from_ramp([*c; 8]);
        }
        let pal = palette_of(&mats, &IDENTITY_COLORS);
        let map = TexMap {
            texture: Texture::new(&TEX, 1, 1),
            offset: [0, 0],
            colors: IDENTITY_COLORS,
        };
        let uv = |x: i32, y: i32, u: i16, v: i16| ScreenVert {
            u: u << 4,
            v: v << 4,
            ..vert(x, y)
        };
        let (a, b, c, d) = (
            uv(0, 0, 0, 0),
            uv(16, 0, 2, 0),
            uv(16, 16, 2, 2),
            uv(0, 16, 0, 2),
        );
        let draw = |mat: &Material| {
            let mut color = [0u16; W * H];
            let mut depth = [0u16; W * H];
            let mut stats = RasterStats::default();
            let mut band = Band {
                color: &mut color,
                depth: &mut depth,
                x0: 0,
                width: W,
                y0: 0,
                height: H,
            };
            for t in [[&a, &b, &c], [&a, &c, &d]] {
                draw_triangle(&mut band, t, mat, &pal, None, &mut stats);
            }
            color
        };

        let img = draw(&Material::textured(map));
        let at = |img: &[u16], x: usize, y: usize| img[y * W + x];
        assert_eq!(
            [
                at(&img, 3, 3),
                at(&img, 12, 3),
                at(&img, 3, 12),
                at(&img, 12, 12)
            ],
            colors,
            "cada cuadrante muestra su texel"
        );

        // Desplazar un texel en x (otra celda de la hoja): arriba queda 2 1.
        let img = draw(&Material::textured(TexMap {
            offset: [1, 0],
            ..map
        }));
        assert_eq!([at(&img, 3, 3), at(&img, 12, 3)], [0x2222, 0x1111]);
    }
}
