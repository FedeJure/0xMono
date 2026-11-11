//! Materiales "toon": una rampa de 8 tonos precalculados en RGB565.
//!
//! La iluminación calcula un índice fraccionario en la rampa; el rasterizado
//! elige el tono con un dithering ordenado 4×4, que da degradados suaves sin
//! multiplicar colores por píxel.
//!
//! Un material puede llevar además una [`Texture`] de 4 bits por texel. Cada
//! texel no es un color sino una entrada de la paleta de la textura
//! ([`TexMap::colors`]), que apunta a **otro material** de la tabla del
//! renderer: la textura "pinta con los materiales del modelo", así que se
//! ilumina con las mismas rampas que el resto y no se notan costuras.

pub const RAMP_LEN: usize = 8;
/// Materiales por renderer.
pub const MAX_MATERIALS: usize = 64;
/// Colores por textura (texels de 4 bits).
pub const TEX_COLORS: usize = 16;
/// Tamaño de la tabla que usa el rasterizado: la rampa más relleno, para
/// indexar con `& 15` sin recortar ni comprobar límites.
pub const LUT_LEN: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const fn lerp(self, o: Rgb, t: u32, max: u32) -> Rgb {
        const fn mix(a: u8, b: u8, t: u32, max: u32) -> u8 {
            ((a as u32 * (max - t) + b as u32 * t) / max) as u8
        }
        Rgb(
            mix(self.0, o.0, t, max),
            mix(self.1, o.1, t, max),
            mix(self.2, o.2, t, max),
        )
    }

    pub const fn to_565(self) -> u16 {
        ((self.0 as u16 >> 3) << 11) | ((self.2 as u16 >> 3) & 0x1F) | ((self.1 as u16 >> 2) << 5)
    }
}

/// Orden de bytes de los píxeles en el búfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelOrder {
    /// `u16` nativo (little-endian): lo que espera el simulador.
    Native,
    /// Bytes intercambiados: el ST7789 recibe RGB565 big-endian por SPI, así
    /// el búfer se envía por DMA tal cual, sin convertir.
    Swapped,
}

impl PixelOrder {
    pub const fn encode(self, c: u16) -> u16 {
        match self {
            PixelOrder::Native => c,
            PixelOrder::Swapped => c.swap_bytes(),
        }
    }
}

/// Las rampas de los 16 colores de una textura, una detrás de otra: el
/// rasterizado texturizado lee `palette[texel · 16 + tono]` con un solo acceso.
pub type Palette = [u16; TEX_COLORS * LUT_LEN];

/// Arma la paleta de una textura: el texel `k` usa la rampa del material
/// `colors[k]`.
pub fn palette_of(materials: &[Material; MAX_MATERIALS], colors: &[u8; TEX_COLORS]) -> Palette {
    let mut p = [0; TEX_COLORS * LUT_LEN];
    for (dst, &m) in p.chunks_exact_mut(LUT_LEN).zip(colors) {
        dst.copy_from_slice(&materials[m as usize % MAX_MATERIALS].lut);
    }
    p
}

/// Paleta que deja cada texel apuntando al material de su mismo número.
pub const IDENTITY_COLORS: [u8; TEX_COLORS] = {
    let mut c = [0u8; TEX_COLORS];
    let mut i = 0;
    while i < TEX_COLORS {
        c[i] = i as u8;
        i += 1;
    }
    c
};

/// Imagen de 4 bits por texel con lados potencia de 2 (las coordenadas se
/// repiten con una máscara, sin comprobar límites). En el ESP32 conviene que
/// esté en RAM: leída desde flash, cada fallo de caché cuesta decenas de ciclos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Texture {
    /// Filas de `ancho / 2` bytes; en cada byte, el nibble bajo es la x par.
    pub texels: &'static [u8],
    pub width_log2: u8,
    pub height_log2: u8,
}

impl Texture {
    pub const fn new(texels: &'static [u8], width_log2: u8, height_log2: u8) -> Texture {
        assert!(width_log2 >= 1 && width_log2 <= 10 && height_log2 <= 10);
        assert!(texels.len() == 1 << (width_log2 + height_log2 - 1));
        Texture {
            texels,
            width_log2,
            height_log2,
        }
    }

    pub const fn width(&self) -> usize {
        1 << self.width_log2
    }

    pub const fn height(&self) -> usize {
        1 << self.height_log2
    }

    /// Índice de material del texel (x, y), con repetición.
    #[inline(always)]
    pub fn get(&self, x: u32, y: u32) -> u8 {
        let i = ((y & ((1 << self.height_log2) - 1)) << self.width_log2)
            | (x & ((1 << self.width_log2) - 1));
        (self.texels[(i >> 1) as usize] >> ((i & 1) << 2)) & 0xF
    }
}

/// Textura aplicada: la imagen y un desplazamiento que se suma a las UV de la
/// malla. Cambiando el desplazamiento se elige otra celda de una hoja de
/// sprites (otra expresión) sin tocar la malla.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TexMap {
    pub texture: Texture,
    /// En texels.
    pub offset: [i16; 2],
    /// Material de cada valor de texel.
    pub colors: [u8; TEX_COLORS],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Material {
    /// Rampa en las posiciones 0..=7. Con dithering, el índice puede salirse
    /// un tono por arriba (8..=14 repiten el más claro) o por abajo (−1 → 15,
    /// que repite el más oscuro).
    pub lut: [u16; LUT_LEN],
    /// Sombreado plano (un tono por triángulo) en vez de Gouraud.
    pub flat: bool,
    /// Si hay textura, la rampa propia no se usa: cada texel elige material.
    pub texture: Option<TexMap>,
}

impl Material {
    pub const BLACK: Material = Material::from_ramp([0; RAMP_LEN]);

    pub const fn from_ramp(ramp: [u16; RAMP_LEN]) -> Material {
        let mut lut = [ramp[RAMP_LEN - 1]; LUT_LEN];
        let mut i = 0;
        while i < RAMP_LEN {
            lut[i] = ramp[i];
            i += 1;
        }
        lut[LUT_LEN - 1] = ramp[0];
        Material {
            lut,
            flat: false,
            texture: None,
        }
    }

    pub fn ramp(&self) -> [u16; RAMP_LEN] {
        let mut r = [0; RAMP_LEN];
        r.copy_from_slice(&self.lut[..RAMP_LEN]);
        r
    }

    /// Rampa sombra → base → brillo. La base cae en el índice 5 de 7: la
    /// mayor parte de la cara iluminada muestra el color "real".
    pub const fn toon(shadow: Rgb, base: Rgb, highlight: Rgb, order: PixelOrder) -> Material {
        let mut ramp = [0u16; RAMP_LEN];
        let mut i = 0;
        while i < RAMP_LEN {
            let c = if i <= 5 {
                shadow.lerp(base, i as u32, 5)
            } else {
                base.lerp(highlight, (i - 5) as u32, 2)
            };
            ramp[i] = order.encode(c.to_565());
            i += 1;
        }
        Material::from_ramp(ramp)
    }

    pub const fn with_flat(mut self, flat: bool) -> Material {
        self.flat = flat;
        self
    }

    /// Material texturizado: los colores salen de la tabla de materiales.
    pub const fn textured(map: TexMap) -> Material {
        let mut m = Material::BLACK;
        m.texture = Some(map);
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_packing() {
        assert_eq!(Rgb(255, 0, 0).to_565(), 0xF800);
        assert_eq!(Rgb(0, 255, 0).to_565(), 0x07E0);
        assert_eq!(Rgb(0, 0, 255).to_565(), 0x001F);
        assert_eq!(PixelOrder::Swapped.encode(0xF800), 0x00F8);
    }

    #[test]
    fn ramp_goes_from_shadow_to_highlight() {
        let m = Material::toon(
            Rgb(0, 0, 0),
            Rgb(128, 128, 128),
            Rgb(255, 255, 255),
            PixelOrder::Native,
        );
        assert_eq!(m.ramp()[0], 0);
        assert_eq!(m.ramp()[5], Rgb(128, 128, 128).to_565());
        assert_eq!(m.ramp()[7], 0xFFFF);
        assert_eq!(m.lut[15], m.lut[0], "−1 debe dar el tono más oscuro");
        assert_eq!(m.lut[8], m.lut[7]);
    }

    #[test]
    fn texels_are_nibbles_and_wrap() {
        // 4×2: fila 0 = 1 2 3 4, fila 1 = 5 6 7 8.
        static T: [u8; 4] = [0x21, 0x43, 0x65, 0x87];
        let t = Texture::new(&T, 2, 1);
        assert_eq!((t.get(0, 0), t.get(1, 0), t.get(3, 0)), (1, 2, 4));
        assert_eq!((t.get(0, 1), t.get(3, 1)), (5, 8));
        assert_eq!(t.get(4, 2), 1, "las coordenadas se repiten");
        assert_eq!(t.get(u32::MAX, 0), 4, "negativos también (x = −1)");
    }
}
