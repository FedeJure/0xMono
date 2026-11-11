//! El globo con lo que dice (o piensa) el mono, dibujado encima del 3D.
//!
//! [`Habitat::render`](crate::Habitat::render) lo pinta en cada franja,
//! después del 3D de esa franja y antes de entregarla: así el firmware lo
//! recibe sin hacer nada y la pantalla nunca ve el 3D sin el globo encima.
//!
//! Como la pantalla guarda la imagen y cada fotograma se redibuja solo una
//! zona, el globo tiene que estar en todas las franjas que lo tocan:
//!
//! - cuando aparece, cambia de texto o de lugar, o se va, [`Bubble::update`]
//!   devuelve su rectángulo (el de antes y el de ahora) para que el hábitat
//!   lo marque con `Stage::invalidate` (al irse, el 3D de abajo vuelve);
//! - cualquier franja que se redibuje y lo toque lo vuelve a pintar entero
//!   en su parte ([`Bubble::draw`]), así que no quedan medios globos ni
//!   píxeles viejos, sea cual sea el alto o el corrimiento de la franja.
//!
//! La forma se arma por renglones de pantalla: para cada fila, los tramos
//! que cubre (la caja redondeada, el piquito o las burbujitas). El borde son
//! los píxeles del tramo que no tienen vecino arriba, abajo o al costado, y
//! la sombra es la misma forma corrida, oscureciendo lo que hay debajo.
//! Pintar una franja cuesta lo que ocupa el globo en ella, nada más.
//!
//! El texto va con la letra de los textos ([`fonts::BODY`], Helvetica
//! negrita con mayúsculas de 14 px, que se lee en la pantalla de 1,3": tiene
//! á, é, ñ, ü, ¡ y ¿), proporcional: los renglones se cortan por ancho en
//! píxeles al armar el globo, y se centran uno por uno. Los renglones que
//! caen fuera de la franja ni se miran.

use companion::{Line, TalkKind};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::pixelcolor::raw::{RawData, RawU16};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use engine3d::{Band, Bounds, PixelOrder, Rgb};
use fonts::{Align, BODY};

/// Renglones como mucho (las frases de `companion::lines` entran: lo
/// prueba `every_line_fits_in_the_bubble`).
pub const MAX_ROWS: usize = 4;
/// De un renglón al siguiente.
const LINE_H: i32 = 19;
/// Aire entre el texto y el borde: a los costados, arriba de las mayúsculas
/// (los acentos de Á o Ñ asoman 4 px) y debajo de las colitas (g, p).
const PAD_X: i32 = 7;
const PAD_TOP: i32 = 7;
const PAD_BOTTOM: i32 = 3;
/// Distancia al borde de la pantalla.
const MARGIN: i32 = 4;
/// Corrimiento de la sombra (abajo a la derecha).
const SHADOW: i32 = 2;
/// Esquinas redondeadas: cuánto se mete cada fila desde arriba (o abajo).
const INSET: [i32; 6] = [6, 4, 3, 2, 1, 1];
/// El piquito: filas y medio ancho en la base.
const TAIL: i32 = 9;
const TAIL_W: i32 = 6;
/// Cuánto se puede torcer el piquito hacia la cara.
const TAIL_SKEW: i32 = 8;
/// Las burbujitas del pensamiento: radio y fila del centro (desde el borde
/// de la caja), de la más cercana a la más lejana.
const DOTS: [(i32, i32); 2] = [(4, 5), (2, 13)];
/// Filas que ocupan el piquito o las burbujitas fuera de la caja.
const REACH: i32 = 16;
const _: () =
    assert!(REACH >= TAIL && REACH > DOTS[0].0 + DOTS[0].1 && REACH > DOTS[1].0 + DOTS[1].1);

/// Dónde está la cara del mono en pantalla (centro y radio en píxeles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spot {
    pub x: i32,
    pub y: i32,
    pub r: i32,
}

impl Spot {
    fn rect(&self) -> Bounds {
        Bounds::new(
            self.x - self.r,
            self.y - self.r,
            self.x + self.r,
            self.y + self.r,
        )
    }
}

/// Colores del globo (relleno, borde y letra).
struct Look {
    fill: Rgb,
    edge: Rgb,
    ink: Rgb,
}

/// Lo que dice: blanco crema con borde ciruela.
const SAY: Look = Look {
    fill: Rgb(255, 252, 242),
    edge: Rgb(96, 78, 118),
    ink: Rgb(44, 36, 64),
};
/// Lo que piensa: lila pastel.
const THINK: Look = Look {
    fill: Rgb(236, 228, 255),
    edge: Rgb(146, 128, 206),
    ink: Rgb(70, 54, 124),
};

/// Lo más ancho que puede ser un renglón (en una pantalla de 240).
pub const TEXT_W: i32 = 240 - 2 * MARGIN - SHADOW - 2 * PAD_X;

/// La frase cortada en renglones: desde, hasta (bytes) y ancho de cada uno.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Rows {
    n: u8,
    r: [(u8, u8, u8); MAX_ROWS],
}

impl Rows {
    fn of(s: &str, max: i32) -> Rows {
        let mut rows = Rows {
            n: 0,
            r: [(0, 0, 0); MAX_ROWS],
        };
        for row in BODY.wrap(s, max).take(MAX_ROWS) {
            let a = row.as_ptr() as usize - s.as_ptr() as usize;
            let w = BODY.width(row).min(255);
            rows.r[rows.n as usize] = (a as u8, (a + row.len()) as u8, w as u8);
            rows.n += 1;
        }
        rows
    }

    /// Los renglones de `s` (la misma frase) con su ancho.
    fn iter<'a>(&'a self, s: &'a str) -> impl Iterator<Item = (&'a str, i32)> + 'a {
        self.r[..self.n as usize]
            .iter()
            .map(move |&(a, b, w)| (s.get(a as usize..b as usize).unwrap_or(""), w as i32))
    }

    fn width(&self) -> i32 {
        self.r[..self.n as usize]
            .iter()
            .map(|r| r.2 as i32)
            .max()
            .unwrap_or(0)
    }
}

/// El globo armado: la frase y dónde va.
#[derive(Clone, Copy)]
struct Layout {
    line: Line,
    rows: Rows,
    /// La caja (sin piquito ni sombra).
    x0: i32,
    y0: i32,
    w: i32,
    h: i32,
    /// El piquito (o las burbujitas) sale por abajo de la caja; si no, por
    /// arriba (la caja está abajo de todo).
    down: bool,
    /// Dónde nace el piquito y hacia dónde apunta (x de pantalla). Sin
    /// cara a la vista (no está en casa), no hay piquito.
    tail: Option<(i32, i32)>,
}

impl PartialEq for Layout {
    fn eq(&self, o: &Self) -> bool {
        self.line.kind == o.line.kind
            && self.line.as_str() == o.line.as_str()
            && (self.x0, self.y0, self.w, self.h, self.down, self.tail)
                == (o.x0, o.y0, o.w, o.h, o.down, o.tail)
    }
}

impl Layout {
    /// Arma el globo para `line` (ya cortada en `rows`): arriba de todo,
    /// salvo que tape la cara (entonces abajo de todo), corrido hacia el
    /// lado de la cara.
    fn new(line: &Line, rows: Rows, face: Option<Spot>, screen: Bounds) -> Layout {
        let max_w = screen.width() - 2 * MARGIN - SHADOW;
        let w = (rows.width() + 2 * PAD_X).min(max_w);
        let n = rows.n.max(1) as i32;
        let h = PAD_TOP + (n - 1) * LINE_H + BODY.cap + BODY.descent + PAD_BOTTOM;
        let x0 = match face {
            Some(f) => (f.x - w / 2).clamp(
                screen.x0 + MARGIN,
                (screen.x1 - MARGIN - SHADOW - w).max(screen.x0 + MARGIN),
            ),
            None => screen.x0 + (screen.width() - w) / 2,
        };
        let top = screen.y0 + MARGIN;
        let bottom = screen.y1 - MARGIN - SHADOW - h;
        let tail = face.map(|f| {
            // Lejos de las esquinas redondeadas.
            let edge = 14.min(w / 2);
            let base = f.x.clamp(x0 + edge, x0 + w - edge);
            (base, base + (f.x - base).clamp(-TAIL_SKEW, TAIL_SKEW))
        });
        let at = |y0: i32, down: bool| Layout {
            line: *line,
            rows,
            x0,
            y0,
            w,
            h,
            down,
            tail,
        };
        let up = at(top, true);
        match face {
            Some(f) if up.covers(f) => {
                let low = at(bottom, false);
                // Si tapa la cara igual (muy de cerca), el que menos tape.
                if !low.covers(f) || low.overlap(f) < up.overlap(f) {
                    low
                } else {
                    up
                }
            }
            _ => up,
        }
    }

    /// Todo lo que pinta: la caja, el piquito y la sombra.
    fn rect(&self) -> Bounds {
        let (mut y0, mut y1) = (self.y0, self.y0 + self.h);
        if self.down {
            y1 += REACH;
        } else {
            y0 -= REACH;
        }
        Bounds::new(self.x0, y0, self.x0 + self.w + SHADOW, y1 + SHADOW)
    }

    fn overlap(&self, f: Spot) -> i32 {
        self.rect().intersect(&f.rect()).area()
    }

    fn covers(&self, f: Spot) -> bool {
        self.rect().overlaps(&f.rect())
    }

    fn look(&self) -> &'static Look {
        match self.line.kind {
            TalkKind::Say => &SAY,
            TalkKind::Think => &THINK,
        }
    }

    /// Los tramos de la forma en la fila `y` de pantalla.
    fn spans(&self, y: i32) -> Spans {
        let mut s = Spans::default();
        let (y1, x1) = (self.y0 + self.h, self.x0 + self.w);
        if (self.y0..y1).contains(&y) {
            let k = (y - self.y0).min(y1 - 1 - y) as usize;
            let inset = INSET.get(k).copied().unwrap_or(0);
            s.add(self.x0 + inset, x1 - inset);
        }
        // Filas fuera de la caja, contando desde su borde (0 = la pegada).
        let d = if self.down { y - y1 } else { self.y0 - 1 - y };
        let Some((base, tip)) = self.tail else {
            return s;
        };
        match self.line.kind {
            TalkKind::Say if (0..TAIL).contains(&d) => {
                let hw = TAIL_W * (TAIL - 1 - d) / (TAIL - 1);
                let c = base + (tip - base) * d / (TAIL - 1);
                s.add(c - hw, c + hw + 1);
            }
            TalkKind::Think => {
                for (i, &(r, at)) in DOTS.iter().enumerate() {
                    let dy = d - at;
                    if dy.abs() <= r {
                        let c = base + (tip - base) * (i as i32 + 1) / DOTS.len() as i32;
                        let hw = isqrt(r * r + r - dy * dy);
                        s.add(c - hw, c + hw + 1);
                    }
                }
            }
            _ => {}
        }
        s
    }
}

/// Tramos `[desde, hasta)` de una fila, ordenados y sin encimarse.
#[derive(Default)]
struct Spans {
    n: usize,
    s: [(i32, i32); 4],
}

impl Spans {
    fn add(&mut self, a: i32, b: i32) {
        if a >= b || self.n == self.s.len() {
            return;
        }
        let mut k = self.n;
        while k > 0 && self.s[k - 1].0 > a {
            self.s[k] = self.s[k - 1];
            k -= 1;
        }
        self.s[k] = (a, b);
        self.n += 1;
        // Junta los que se tocan.
        let mut out = 0;
        for i in 1..self.n {
            if self.s[i].0 <= self.s[out].1 {
                self.s[out].1 = self.s[out].1.max(self.s[i].1);
            } else {
                out += 1;
                self.s[out] = self.s[i];
            }
        }
        self.n = out + 1;
    }

    fn iter(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.s[..self.n].iter().copied()
    }

    /// Llama a `f` con los pedazos de `[a, b)` que estos tramos no cubren.
    fn gaps(&self, a: i32, b: i32, mut f: impl FnMut(i32, i32)) {
        let mut x = a;
        for (s, e) in self.iter() {
            if e <= x {
                continue;
            }
            if s >= b {
                break;
            }
            if s > x {
                f(x, s);
            }
            x = e;
            if x >= b {
                return;
            }
        }
        if x < b {
            f(x, b);
        }
    }
}

fn isqrt(n: i32) -> i32 {
    let mut r = 0;
    while (r + 1) * (r + 1) <= n {
        r += 1;
    }
    r
}

/// Oscurece un píxel RGB565 (la sombra: más oscuro y apenas violeta).
fn shade(c: u16) -> u16 {
    let (r, g, b) = (c >> 11, (c >> 5) & 0x3F, c & 0x1F);
    ((r * 9 / 16) << 11) | ((g * 9 / 16) << 5) | (b * 11 / 16)
}

/// El globo en pantalla.
#[derive(Default)]
pub struct Bubble {
    shown: Option<Layout>,
}

impl Bubble {
    pub const fn new() -> Self {
        Bubble { shown: None }
    }

    /// Lo que ocupa en pantalla (vacío si no hay globo).
    pub fn area(&self) -> Bounds {
        self.shown.as_ref().map_or(Bounds::EMPTY, Layout::rect)
    }

    /// Lo que dice ahora (si hay globo).
    pub fn text(&self) -> Option<&str> {
        self.shown.as_ref().map(|l| l.line.as_str())
    }

    /// Pone al día el globo con lo que dice (`speech`) y dónde está la cara.
    /// Devuelve la zona que hay que redibujar: el globo de antes y el de
    /// ahora si cambió algo (vacía si no).
    ///
    /// Mientras dura una frase el globo se queda quieto (moverlo obligaría a
    /// redibujarlo cada fotograma); solo se muda si la cara se le mete
    /// abajo y en el otro lugar no la tapa, o si la cámara se mueve
    /// (`panning`: igual se redibuja toda la pantalla, no cuesta nada).
    pub fn update(
        &mut self,
        speech: Option<&Line>,
        face: Option<Spot>,
        screen: Bounds,
        panning: bool,
    ) -> Bounds {
        let next = speech.map(|line| {
            // Cortarla en renglones mide letra por letra: solo si cambió.
            let same = |l: &Layout| l.line.kind == line.kind && l.line.as_str() == line.as_str();
            let rows = match &self.shown {
                Some(old) if same(old) => old.rows,
                _ => Rows::of(
                    line.as_str(),
                    screen.width() - 2 * MARGIN - SHADOW - 2 * PAD_X,
                ),
            };
            let fresh = Layout::new(line, rows, face, screen);
            match self.shown {
                Some(old)
                    if !panning
                        && old.line.kind == line.kind
                        && old.line.as_str() == line.as_str()
                        && !face.is_some_and(|f| old.covers(f) && !fresh.covers(f)) =>
                {
                    old
                }
                _ => fresh,
            }
        });
        if next == self.shown {
            return Bounds::EMPTY;
        }
        let dirty = self
            .area()
            .union(&next.as_ref().map_or(Bounds::EMPTY, Layout::rect));
        self.shown = next;
        dirty
    }

    /// Pinta la parte del globo que cae en la franja (encima de lo que ya
    /// tiene). `order` es el orden de bytes de los píxeles de la franja.
    pub fn draw(&self, band: &mut Band, order: PixelOrder) {
        let Some(l) = &self.shown else {
            return;
        };
        let bx = Bounds::new(
            band.x0 as i32,
            band.y0 as i32,
            (band.x0 + band.width) as i32,
            (band.y0 + band.height) as i32,
        );
        let clip = l.rect().intersect(&bx);
        if clip.is_empty() {
            return;
        }
        let look = l.look();
        let (fill, edge) = (
            order.encode(look.fill.to_565()),
            order.encode(look.edge.to_565()),
        );
        let w = band.width;
        for y in clip.y0..clip.y1 {
            let row = &mut band.color[(y - bx.y0) as usize * w..][..w];
            // Índice en la fila de un tramo de pantalla, recortado.
            let cut = |a: i32, b: i32| {
                let (a, b) = (a.max(clip.x0), b.min(clip.x1));
                (a < b).then(|| ((a - bx.x0) as usize, (b - bx.x0) as usize))
            };
            for (a, b) in l.spans(y - SHADOW).iter() {
                if let Some((a, b)) = cut(a + SHADOW, b + SHADOW) {
                    for px in &mut row[a..b] {
                        *px = order.encode(shade(order.encode(*px)));
                    }
                }
            }
            let here = l.spans(y);
            if here.n == 0 {
                continue;
            }
            let (up, down) = (l.spans(y - 1), l.spans(y + 1));
            let mut paint = |a: i32, b: i32, c: u16| {
                if let Some((a, b)) = cut(a, b) {
                    row[a..b].fill(c);
                }
            };
            for (a, b) in here.iter() {
                paint(a, b, fill);
                paint(a, a + 1, edge);
                paint(b - 1, b, edge);
                up.gaps(a, b, |s, e| paint(s, e, edge));
                down.gaps(a, b, |s, e| paint(s, e, edge));
            }
        }

        // El texto: solo los renglones que tocan la franja.
        let ink = Rgb565::from(RawU16::new(look.ink.to_565()));
        let mut canvas = Canvas { band, clip, order };
        let text = l.line.as_str();
        for (k, (row, w)) in l.rows.iter(text).enumerate() {
            let top = l.y0 + PAD_TOP + k as i32 * LINE_H;
            let x = l.x0 + (l.w - w) / 2;
            // `draw` saltea el renglón si no toca el lienzo (`clip`).
            BODY.draw(&mut canvas, row, x, top, Align::Left, ink).ok();
        }
    }
}

/// La franja como lienzo de `embedded-graphics`, en coordenadas de
/// pantalla y recortada a `clip`.
struct Canvas<'a, 'b> {
    band: &'a mut Band<'b>,
    clip: Bounds,
    order: PixelOrder,
}

impl Dimensions for Canvas<'_, '_> {
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(
            Point::new(self.clip.x0, self.clip.y0),
            Size::new(self.clip.width() as u32, self.clip.height() as u32),
        )
    }
}

impl DrawTarget for Canvas<'_, '_> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error> {
        let c = self.clip;
        for Pixel(p, color) in pixels {
            if (c.x0..c.x1).contains(&p.x) && (c.y0..c.y1).contains(&p.y) {
                let i =
                    (p.y as usize - self.band.y0) * self.band.width + p.x as usize - self.band.x0;
                self.band.color[i] = self.order.encode(RawU16::from(color).into_inner());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use companion::{Companion, LocalTime, Personality};

    const W: usize = 240;
    const SCREEN: Bounds = Bounds::new(0, 0, 240, 240);

    fn said() -> Line {
        let now = LocalTime::new(20_002, 14, 0);
        let mut c = Companion::new(Personality::from_seed(9), None, now);
        c.poke(now);
        *c.speech().expect("contesta")
    }

    /// Dibuja el globo sobre un fondo de prueba en franjas de `rows` filas
    /// que cubren `area`.
    fn paint(b: &Bubble, order: PixelOrder, area: Bounds, rows: usize) -> Vec<u16> {
        let mut screen: Vec<u16> = (0..W * W).map(|i| order.encode((i * 37) as u16)).collect();
        let w = area.width() as usize;
        let mut color = vec![0u16; w * rows];
        let mut depth = vec![0u16; w * rows];
        let mut y0 = area.y0 as usize;
        while y0 < area.y1 as usize {
            let h = rows.min(area.y1 as usize - y0);
            for r in 0..h {
                let src = (y0 + r) * W + area.x0 as usize;
                color[r * w..][..w].copy_from_slice(&screen[src..src + w]);
            }
            let mut band = Band {
                color: &mut color[..w * h],
                depth: &mut depth[..w * h],
                x0: area.x0 as usize,
                width: w,
                y0,
                height: h,
            };
            b.draw(&mut band, order);
            for r in 0..h {
                let dst = (y0 + r) * W + area.x0 as usize;
                screen[dst..dst + w].copy_from_slice(&band.color[r * w..][..w]);
            }
            y0 += h;
        }
        screen
    }

    #[test]
    fn any_band_split_gives_the_same_bubble() {
        let line = said();
        let mut b = Bubble::new();
        let face = Spot {
            x: 150,
            y: 160,
            r: 30,
        };
        assert!(!b.update(Some(&line), Some(face), SCREEN, false).is_empty());
        for order in [PixelOrder::Native, PixelOrder::Swapped] {
            let whole = paint(&b, order, SCREEN, 240);
            for rows in [1, 3, 7, 13, 32] {
                assert!(paint(&b, order, SCREEN, rows) == whole, "franjas de {rows}");
            }
            // Una zona angosta que corta el globo por la mitad.
            let part = Bounds::new(37, 9, 151, 40);
            let mut expect = paint(&b, order, Bounds::new(0, 0, 0, 0), 1);
            for y in part.y0..part.y1 {
                for x in part.x0..part.x1 {
                    let i = (y * 240 + x) as usize;
                    expect[i] = whole[i];
                }
            }
            assert!(paint(&b, order, part, 5) == expect, "zona parcial");
            // El relleno, con el orden de bytes de la pantalla.
            let fill = order.encode(SAY.fill.to_565());
            let a = b.area();
            let mid = ((a.y0 + 2) as usize * W) + (a.x0 + a.x1) as usize / 2;
            assert_eq!(whole[mid], fill);
        }
    }

    #[test]
    fn goes_down_when_the_face_is_up() {
        let line = said();
        let mut b = Bubble::new();
        b.update(
            Some(&line),
            Some(Spot {
                x: 120,
                y: 150,
                r: 30,
            }),
            SCREEN,
            false,
        );
        assert!(b.area().y0 < 20, "{:?}", b.area());
        // La cara sube y se le mete abajo: se muda abajo.
        let up = Spot {
            x: 120,
            y: 40,
            r: 25,
        };
        assert!(!b.update(Some(&line), Some(up), SCREEN, false).is_empty());
        assert!(
            b.area().y1 > 220 && !b.area().overlaps(&up.rect()),
            "{:?}",
            b.area()
        );
        // Mientras no lo tape, se queda donde está.
        let near = Spot {
            x: 100,
            y: 120,
            r: 25,
        };
        assert!(b.update(Some(&line), Some(near), SCREEN, false).is_empty());
        // Se va: hay que redibujar donde estaba.
        let was = b.area();
        assert_eq!(b.update(None, None, SCREEN, false), was);
        assert!(b.area().is_empty());
        assert!(b.update(None, None, SCREEN, false).is_empty());
    }

    #[test]
    fn spans_merge_and_gaps() {
        let mut s = Spans::default();
        s.add(10, 20);
        s.add(30, 40);
        s.add(18, 31);
        assert_eq!(s.iter().collect::<Vec<_>>(), [(10, 40)]);
        let mut g = Vec::new();
        s.gaps(0, 50, |a, b| g.push((a, b)));
        assert_eq!(g, [(0, 10), (40, 50)]);
    }

    #[test]
    fn spanish_glyphs_are_in_the_font() {
        // Las letras raras tienen dibujo propio.
        for ch in "áéíóúñÑ¡¿üÁÉ".chars() {
            assert!(BODY.has(ch), "{ch}");
        }
    }

    #[test]
    fn every_line_fits_in_the_bubble() {
        // Todas las reglas de `companion::lines`, armadas muchas veces (los
        // huecos más largos salen alguna vez), entran en MAX_ROWS renglones
        // del ancho de la pantalla, sin cortar palabras.
        use companion::lines::RULES;
        use companion::routine::{day_kind, mood};
        use companion::talk::{Context, compose_rule};
        use companion::{Activity, rng::Rng};
        let p = Personality::from_seed(1);
        let now = LocalTime::new(20_002, 10, 15);
        let c = Context {
            activity: Activity::Code,
            now,
            kind: day_kind(now.day),
            mood: mood(&p, now.day),
            personality: &p,
            market: 2,
            did: Some(Activity::Clean),
            next: Some(Activity::Cook),
        };
        let mut rng = Rng::new(3);
        let mut most = (0, std::string::String::new());
        for i in 0..RULES.len() {
            for _ in 0..40 {
                let l = compose_rule(i, TalkKind::Say, &c, &mut rng);
                let rows: Vec<&str> = BODY.wrap(l.as_str(), TEXT_W).collect();
                assert!(rows.len() <= MAX_ROWS, "no entra: {rows:?}");
                for r in &rows {
                    assert!(BODY.width(r) <= TEXT_W, "{r:?}");
                }
                // Ninguna palabra cortada al medio.
                assert_eq!(
                    rows.join(" "),
                    l.as_str().split_whitespace().collect::<Vec<_>>().join(" ")
                );
                if rows.len() > most.0 {
                    most = (rows.len(), std::string::String::from(l.as_str()));
                }
            }
        }
        std::println!("la más larga: {most:?}");
        // Y el globo más alto (con el piquito) deja lugar para la cara.
        let h = PAD_TOP + (MAX_ROWS as i32 - 1) * LINE_H + BODY.cap + BODY.descent + PAD_BOTTOM;
        assert!(h + REACH + SHADOW <= 110, "{h}");
    }
}
