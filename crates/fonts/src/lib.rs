//! Las letras del 0xMono.
//!
//! La pantalla mide 1,3": 240 píxeles en 23 mm, cada píxel es una décima de
//! milímetro. Una letra de 7×13 queda de 1 mm y no se lee con el aparato en
//! la mano, así que todo va con Helvetica negrita de U8g2 (`u8g2-fonts`), que
//! es proporcional (entra más texto por renglón que con una letra de ancho
//! fijo del mismo alto) y trae todo el Latin-1: á, é, ñ, ü, ¡, ¿, «, »...
//! Los títulos van con Free Universal Bold, más gruesa en tamaños grandes.
//! (Las letras tienen su licencia, la de cada una en U8g2: las Helvetica de
//! X11 y Free Universal con la SIL OFL.)
//!
//! | Letra     | Mayúscula | Para qué |
//! |-----------|-----------|----------|
//! | [`HUGE`]  | 25 px     | la hora grande |
//! | [`TITLE`] | 20 px     | títulos, la clave del WiFi |
//! | [`BODY`]  | 14 px     | textos, opciones del menú, el globo |
//! | [`SMALL`] | 12 px     | ayudas ("A baja · B sube · C elige") |
//!
//! Se dibuja por franjas: [`Font::draw`] saltea el renglón entero si no toca
//! la `bounding_box()` del `DrawTarget` (la franja), así que lo que no se ve
//! no cuesta nada.
#![cfg_attr(not(test), no_std)]

use embedded_graphics::prelude::*;
use u8g2_fonts::types::{FontColor, VerticalPosition};
use u8g2_fonts::{FontRenderer, fonts as u8g2};

/// Dónde va la `x` que se pasa a [`Font::draw`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Una letra con sus medidas. Las `y` son siempre el borde de arriba de las
/// mayúsculas: los acentos de las mayúsculas (Á, Ñ) asoman un poco más
/// arriba y las colitas (g, j, p) bajan [`Font::descent`].
pub struct Font {
    r: FontRenderer,
    /// Alto de las mayúsculas (la `H`).
    pub cap: i32,
    /// Lo que bajan la g, la j, la p.
    pub descent: i32,
    /// De un renglón al siguiente (lo que se usa en párrafos).
    pub line: i32,
    /// Lo más alto y lo más bajo que dibuja alguna letra, desde la base.
    above: i32,
    below: i32,
}

impl Font {
    const fn new(r: FontRenderer, gap: i32) -> Font {
        let cap = r.get_ascent() as i32;
        let descent = -(r.get_descent() as i32);
        let bbox = r.get_font_bounding_box(VerticalPosition::Baseline);
        Font {
            cap,
            descent,
            line: cap + descent + gap,
            above: -bbox.top_left.y,
            below: bbox.top_left.y + bbox.size.height as i32,
            r: r.with_ignore_unknown_chars(true),
        }
    }

    /// Ancho de `s` en píxeles (lo que avanza).
    pub fn width(&self, s: &str) -> i32 {
        self.r
            .get_rendered_dimensions(s, Point::zero(), VerticalPosition::Baseline)
            .map_or(0, |d| d.advance.x)
    }

    /// Ancho de una letra.
    pub fn advance(&self, ch: char) -> i32 {
        self.r
            .get_rendered_dimensions(ch, Point::zero(), VerticalPosition::Baseline)
            .map_or(0, |d| d.advance.x)
    }

    /// ¿La letra tiene dibujo propio? (Las que no, no se dibujan.)
    pub fn has(&self, ch: char) -> bool {
        self.r
            .get_rendered_dimensions(ch, Point::zero(), VerticalPosition::Baseline)
            .is_ok_and(|d| d.bounding_box.is_some())
    }

    /// Las filas que puede pintar un renglón con las mayúsculas arriba en
    /// `y`: de `y + top` a `y + bottom` (sin incluir).
    pub fn rows(&self, y: i32) -> (i32, i32) {
        (y + self.cap - self.above, y + self.cap + self.below)
    }

    /// Dibuja un renglón con las mayúsculas arriba en `y`; la `x` es el
    /// borde izquierdo, el centro o el borde derecho según `align`. Si el
    /// renglón no toca la `bounding_box()` de `d`, ni lo mira.
    pub fn draw<D: DrawTarget>(
        &self,
        d: &mut D,
        s: &str,
        x: i32,
        y: i32,
        align: Align,
        color: D::Color,
    ) -> Result<(), D::Error> {
        let vis = d.bounding_box();
        let (top, bottom) = self.rows(y);
        if bottom <= vis.top_left.y || top >= vis.top_left.y + vis.size.height as i32 {
            return Ok(());
        }
        let x = match align {
            Align::Left => x,
            Align::Center => x - self.width(s) / 2,
            Align::Right => x - self.width(s),
        };
        match self.r.render(
            s,
            Point::new(x, y + self.cap),
            VerticalPosition::Baseline,
            FontColor::Transparent(color),
            d,
        ) {
            Err(u8g2_fonts::Error::DisplayError(e)) => Err(e),
            _ => Ok(()),
        }
    }

    /// Los renglones de `s` que entran en `max` píxeles de ancho, cortando
    /// entre palabras (las que no entran solas se cortan donde sea). Los
    /// `'\n'` cortan siempre; dos seguidos dejan un renglón vacío.
    pub fn wrap<'a>(&'a self, s: &'a str, max: i32) -> Wrap<'a> {
        Wrap {
            font: self,
            rest: s,
            max,
            done: false,
        }
    }
}

/// Iterador de [`Font::wrap`].
pub struct Wrap<'a> {
    font: &'a Font,
    rest: &'a str,
    max: i32,
    done: bool,
}

impl<'a> Iterator for Wrap<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        let s = self.rest.trim_start_matches(' ');
        let (seg, after) = match s.find('\n') {
            Some(i) => (&s[..i], Some(i + 1)),
            None => (s, None),
        };
        if seg.is_empty() && after.is_none() {
            self.done = true;
            return None;
        }
        let mut w = 0;
        let mut space = None;
        let mut cut = seg.len();
        for (i, ch) in seg.char_indices() {
            if ch == ' ' {
                space = Some(i);
            }
            let cw = self.font.advance(ch);
            if w + cw > self.max && i > 0 && ch != ' ' {
                cut = space.unwrap_or(i);
                break;
            }
            w += cw;
        }
        if cut == seg.len() {
            match after {
                Some(k) => self.rest = &s[k..],
                None => self.done = true,
            }
        } else {
            self.rest = &s[cut..];
        }
        Some(seg[..cut].trim_end())
    }
}

/// La hora grande (mayúsculas de 25 px).
pub static HUGE: Font = Font::new(FontRenderer::new::<u8g2::u8g2_font_fub25_tf>(), 3);
/// Títulos (20 px).
pub static TITLE: Font = Font::new(FontRenderer::new::<u8g2::u8g2_font_fub20_tf>(), 2);
/// Textos, opciones y el globo (14 px).
pub static BODY: Font = Font::new(FontRenderer::new::<u8g2::u8g2_font_helvB14_tf>(), 2);
/// Ayudas chicas, pero que se lean (12 px).
pub static SMALL: Font = Font::new(FontRenderer::new::<u8g2::u8g2_font_helvB12_tf>(), 2);

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::mock_display::MockDisplay;
    use embedded_graphics::pixelcolor::BinaryColor;
    use embedded_graphics::primitives::Rectangle;

    const ALL: [&Font; 4] = [&HUGE, &TITLE, &BODY, &SMALL];

    #[test]
    fn spanish_letters_have_their_own_glyph() {
        for f in ALL {
            for ch in "áéíóúÁÉÍÓÚñÑüÜ¡¿«»·ºª".chars() {
                assert!(f.has(ch), "falta {ch:?}");
            }
            assert!(!f.has('☃'));
        }
    }

    #[test]
    fn big_enough_to_read() {
        // Mayúsculas de al menos 1,4 mm (14 px) en el texto, 1,2 en las ayudas.
        assert!(BODY.cap >= 14 && TITLE.cap > BODY.cap && HUGE.cap > TITLE.cap);
        assert!(SMALL.cap >= 10);
        for f in ALL {
            assert!(f.line >= f.cap + f.descent);
        }
    }

    #[test]
    fn proportional_widths() {
        assert!(BODY.width("mmmm") > BODY.width("iiii"));
        assert_eq!(
            BODY.width("hola che"),
            "hola che".chars().map(|c| BODY.advance(c)).sum::<i32>()
        );
    }

    fn lines(f: &Font, s: &str, max: i32) -> Vec<String> {
        f.wrap(s, max).map(String::from).collect()
    }

    #[test]
    fn wraps_between_words_within_the_width() {
        let s = "Hoy deployé el contrato nuevo en mainnet y no explotó nada, increíble";
        let max = 150;
        let rows = lines(&BODY, s, max);
        assert!(rows.len() >= 3, "{rows:?}");
        for r in &rows {
            assert!(BODY.width(r) <= max, "{r:?}");
            assert!(!r.starts_with(' ') && !r.ends_with(' '));
        }
        assert_eq!(rows.join(" "), s);
        // Lo más largo posible: la palabra siguiente no entraba.
        for w in rows.windows(2) {
            let next = w[1].split(' ').next().unwrap();
            assert!(BODY.width(&format!("{} {next}", w[0])) > max, "{w:?}");
        }
    }

    #[test]
    fn long_words_are_cut_and_newlines_kept() {
        let rows = lines(&BODY, "supercalifragilísticoespialidoso", 60);
        assert!(rows.len() > 2 && rows.iter().all(|r| BODY.width(r) <= 60));
        assert_eq!(rows.concat(), "supercalifragilísticoespialidoso");
        assert_eq!(
            lines(&SMALL, "Versión 1\n\nWiFi: casa\n", 200),
            ["Versión 1", "", "WiFi: casa"]
        );
        assert!(lines(&BODY, "", 100).is_empty());
        assert!(lines(&BODY, "   ", 100).is_empty());
    }

    #[test]
    fn draws_inside_its_rows_and_skips_other_bands() {
        let mut d: MockDisplay<BinaryColor> = MockDisplay::new();
        d.set_allow_overdraw(true);
        BODY.draw(&mut d, "¡Ñú!", 2, 10, Align::Left, BinaryColor::On)
            .unwrap();
        let (top, bottom) = BODY.rows(10);
        let bb = d.affected_area();
        assert!(bb.top_left.y >= top && bb.top_left.y + bb.size.height as i32 <= bottom);
        assert!(
            bb.top_left.y < 10,
            "la Ñ y la ú asoman arriba de las mayúsculas"
        );

        // Una franja que no toca el renglón: no dibuja nada.
        struct Band(u32);
        impl Dimensions for Band {
            fn bounding_box(&self) -> Rectangle {
                Rectangle::new(Point::new(0, 40), Size::new(64, 10))
            }
        }
        impl DrawTarget for Band {
            type Color = BinaryColor;
            type Error = core::convert::Infallible;
            fn draw_iter<I: IntoIterator<Item = Pixel<BinaryColor>>>(
                &mut self,
                p: I,
            ) -> Result<(), Self::Error> {
                self.0 += p.into_iter().count() as u32;
                Ok(())
            }
        }
        let mut b = Band(0);
        BODY.draw(&mut b, "Hola", 0, 10, Align::Left, BinaryColor::On)
            .unwrap();
        assert_eq!(b.0, 0);
        BODY.draw(&mut b, "Hola", 0, 32, Align::Left, BinaryColor::On)
            .unwrap();
        assert!(b.0 > 0);
    }
}
