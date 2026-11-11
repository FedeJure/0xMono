//! El menú del 0xMono. Se abre manteniendo A+C desde cualquier lado (la
//! casa, un juego) y se maneja con los tres botones: **A baja, B sube, C
//! elige**.
//!
//! Es una lista de páginas hecha con datos: para agregar una opción alcanza
//! con sumar un [`Item`] a la página que corresponda (o una [`PageId`] nueva
//! con su título y sus opciones). Lo que el menú no puede resolver solo (ir
//! a la casa, jugar, reiniciar en otro modo) lo devuelve como [`Action`].

use core::fmt::Write;

use companion::LocalTime;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, Triangle};
use games::GameId;
use heapless::{String, Vec};
use settings::Settings;
use settings::date::{Clock, DayLabel};
use settings::handoff::{Notice, SyncError};
use settings::tz::Offset;

use crate::SIZE;
use crate::paint::{
    self, Align, BG, BODY, HEADER_H, INK, Icon, LAVENDER, LAVENDER_LIGHT, MUTE, SMALL,
};

/// Lo que se pide con los botones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    /// A
    Down,
    /// B
    Up,
    /// C
    Accept,
}

/// Lo que el menú le pide al firmware.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Cerrar el menú y volver con el mono.
    Home,
    Play(GameId),
    /// Reiniciar en el modo configuración (la red del mono y la página).
    Portal,
    /// Reiniciar para ponerse en hora por internet.
    SyncTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageId {
    Main,
    Games,
    Settings,
    About,
    Message,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Go {
    Do(Action),
    Open(PageId),
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    pub label: &'static str,
    pub icon: Icon,
    pub go: Go,
}

const fn item(label: &'static str, icon: Icon, go: Go) -> Item {
    Item { label, icon, go }
}

const BACK: Item = item("Volver", Icon::Back, Go::Back);

const MAIN: &[Item] = &[
    item("Volver con el mono", Icon::Home, Go::Do(Action::Home)),
    item("Minijuegos", Icon::Game, Go::Open(PageId::Games)),
    item("Configuración", Icon::Gear, Go::Open(PageId::Settings)),
];

const SETTINGS: &[Item] = &[
    SETUP,
    item("Hora por internet", Icon::Clock, Go::Do(Action::SyncTime)),
    item("Acerca del 0xMono", Icon::Info, Go::Open(PageId::About)),
    BACK,
];

pub const MAX_ITEMS: usize = 12;

/// Un aviso con una o dos opciones (por ejemplo, "¡ya estoy en hora!").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub title: String<24>,
    pub body: String<200>,
    pub items: Vec<Item, 2>,
}

const HOME: Item = item("Volver con el mono", Icon::Home, Go::Do(Action::Home));
/// Configurar desde el celular (en la página "Configuración" y en los
/// avisos que lo piden: corto para que entre con letra grande).
const SETUP: Item = item("Desde el celular", Icon::Phone, Go::Do(Action::Portal));

impl Message {
    pub fn new(title: &str, body: &str, items: &[Item]) -> Message {
        let mut m = Message {
            title: String::new(),
            body: String::new(),
            items: Vec::new(),
        };
        let _ = m.title.push_str(title);
        let _ = m.body.push_str(body);
        for i in items.iter().take(2) {
            let _ = m.items.push(*i);
        }
        m
    }

    /// Lo que se cuenta al volver a la casa después de un reinicio.
    pub fn from_notice(n: Notice, now: LocalTime) -> Message {
        let mut body: String<200> = String::new();
        match n {
            Notice::Saved => Message::new(
                "¡Guardado!",
                "Ya tengo todo configurado. ¡Gracias!",
                &[HOME],
            ),
            Notice::NewMonkey => Message::new(
                "¡Hola, soy nuevo!",
                "Recién llego a la casa. Tocame cuando quieras para charlar.",
                &[HOME],
            ),
            Notice::Factory => Message::new(
                "Como nuevo",
                "Borré todo. Cuando quieras, configurame desde el celular.",
                &[HOME, SETUP],
            ),
            Notice::TimeSet => {
                let _ = write!(
                    body,
                    "Son las {} del {}. Ya me acomodo.",
                    Clock(now),
                    DayLabel(now)
                );
                Message::new("¡En hora!", &body, &[HOME])
            }
            Notice::TimeFailed(e) => {
                // Que sigue con la hora que tenía ya se ve: está en la casa.
                let _ = body.push_str(e.message());
                let items: &[Item] = match e {
                    SyncError::Wifi(_) => &[HOME, SETUP],
                    SyncError::NoServer => &[HOME],
                };
                Message::new("No pude ponerme en hora", &body, items)
            }
            Notice::NoWifi => Message::new(
                "Me falta el WiFi",
                "Para ponerme en hora por internet, primero conectame a tu WiFi desde el celular.",
                &[SETUP, BACK],
            ),
        }
    }
}

/// Lo que muestra "Acerca del 0xMono".
pub fn about_text(version: &str, s: &Settings) -> String<200> {
    let mut t = String::new();
    let _ = writeln!(t, "Versión {version}");
    if s.has_wifi() {
        let _ = writeln!(t, "WiFi: {}", s.ssid);
    } else {
        let _ = t.push_str("WiFi: sin configurar\n");
    }
    if s.wants_sync() {
        let _ = writeln!(t, "Hora: automática\nZona: {}", Offset(s.utc_offset_min));
    } else {
        let _ = t.push_str("Hora: a mano\n");
    }
    let _ = t.push_str("Hecho con cariño.");
    t
}

pub struct Menu {
    /// Páginas abiertas, con la opción elegida en cada una.
    stack: Vec<(PageId, u8), 4>,
    message: Option<Message>,
    pub about: String<200>,
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu {
    pub fn new() -> Self {
        let mut stack = Vec::new();
        let _ = stack.push((PageId::Main, 0));
        Self {
            stack,
            message: None,
            about: String::new(),
        }
    }

    /// Vuelve a la primera página, con "Volver con el mono" elegido.
    pub fn open_main(&mut self) {
        self.stack.clear();
        self.message = None;
        let _ = self.stack.push((PageId::Main, 0));
    }

    /// Al terminar un juego: la lista de juegos con ese elegido.
    pub fn open_games(&mut self, last: Option<GameId>) {
        self.open_main();
        let cursor = last
            .and_then(|g| GameId::ALL.iter().position(|&x| x == g))
            .unwrap_or(0);
        self.stack[0].1 = 1;
        let _ = self.stack.push((PageId::Games, cursor as u8));
    }

    /// Muestra un aviso encima de la página actual.
    pub fn show(&mut self, m: Message) {
        self.message = Some(m);
        if self.page() != PageId::Message {
            if self.stack.is_full() {
                self.stack.remove(1);
            }
            let _ = self.stack.push((PageId::Message, 0));
        } else if let Some(top) = self.stack.last_mut() {
            top.1 = 0;
        }
    }

    pub fn page(&self) -> PageId {
        self.stack.last().map_or(PageId::Main, |p| p.0)
    }

    pub fn cursor(&self) -> usize {
        self.stack.last().map_or(0, |p| p.1 as usize)
    }

    pub fn title(&self) -> &str {
        match self.page() {
            PageId::Main => "Menú",
            PageId::Games => "Minijuegos",
            PageId::Settings => "Configuración",
            PageId::About => "Acerca del 0xMono",
            PageId::Message => self.message.as_ref().map_or("", |m| m.title.as_str()),
        }
    }

    /// Texto arriba de las opciones (los avisos y "Acerca de").
    pub fn body(&self) -> Option<&str> {
        match self.page() {
            PageId::About => Some(&self.about),
            PageId::Message => self.message.as_ref().map(|m| m.body.as_str()),
            _ => None,
        }
    }

    pub fn items(&self) -> Vec<Item, MAX_ITEMS> {
        let mut v = Vec::new();
        let mut add = |items: &[Item]| {
            for i in items {
                let _ = v.push(*i);
            }
        };
        match self.page() {
            PageId::Main => add(MAIN),
            PageId::Settings => add(SETTINGS),
            PageId::About => add(&[BACK]),
            PageId::Message => add(self.message.as_ref().map_or(&[BACK][..], |m| &m.items)),
            PageId::Games => {
                for &g in GameId::ALL.iter().take(MAX_ITEMS - 1) {
                    add(&[item(g.title(), Icon::Star, Go::Do(Action::Play(g)))]);
                }
                add(&[BACK]);
            }
        }
        v
    }

    /// A baja, B sube (las dos dan la vuelta) y C elige.
    pub fn press(&mut self, nav: Nav) -> Option<Action> {
        let items = self.items();
        let n = items.len().max(1);
        let cursor = self.cursor();
        match nav {
            Nav::Down => self.set_cursor((cursor + 1) % n),
            Nav::Up => self.set_cursor((cursor + n - 1) % n),
            Nav::Accept => match items.get(cursor)?.go {
                Go::Do(a) => {
                    if matches!(a, Action::Home) {
                        self.open_main();
                    }
                    return Some(a);
                }
                Go::Open(p) => {
                    let _ = self.stack.push((p, 0));
                }
                Go::Back => self.back(),
            },
        }
        None
    }

    fn set_cursor(&mut self, c: usize) {
        if let Some(top) = self.stack.last_mut() {
            top.1 = c as u8;
        }
    }

    fn back(&mut self) {
        if self.page() == PageId::Message {
            self.message = None;
        }
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }
}

// --- Dibujo --------------------------------------------------------------------

/// Arriba de la lista (debajo del encabezado), alto de cada fila (con su
/// separación) y abajo de la lista (arriba de la ayuda).
const TOP: i32 = HEADER_H + 6;
const ROW: i32 = 38;
const ROW_H: u32 = 34;
const BOTTOM: i32 = 216;
/// La ayuda de abajo.
const HINT_Y: i32 = 222;
/// Donde empieza el texto de las opciones.
const LABEL_X: i32 = 46;
/// La tarjeta con el texto (avisos y "Acerca de") y el ancho del texto.
const CARD_X: i32 = 6;
const TEXT_W: u32 = SIZE - 2 * CARD_X as u32 - 12;

/// Cuántas filas entran debajo de `top`.
fn visible(top: i32) -> usize {
    ((BOTTOM - top) / ROW).max(1) as usize
}

/// Dibuja el menú entero (240×240).
pub fn draw<D: DrawTarget<Color = Rgb565>>(m: &Menu, d: &mut D) -> Result<(), D::Error> {
    d.clear(BG)?;
    paint::header(d, m.title())?;
    let items = m.items();
    let mut top = TOP;
    if let Some(body) = m.body() {
        // Tarjeta con el texto, y las opciones abajo de todo.
        let rows = items.len() as i32;
        let list_top = BOTTOM - rows * ROW;
        let card = Rectangle::new(
            Point::new(CARD_X, TOP - 2),
            Size::new(SIZE - 2 * CARD_X as u32, (list_top - TOP - 4) as u32),
        );
        paint::card(d, card)?;
        let lines = paint::count_lines(body, &BODY, TEXT_W) as i32;
        let text_h = (lines - 1) * BODY.line + BODY.cap;
        let y = card.top_left.y + ((card.size.height as i32 - text_h) / 2).max(4);
        paint::paragraph(d, body, CARD_X + 6, TEXT_W, y, &BODY, INK)?;
        top = list_top;
    }

    let rows = visible(top);
    let cursor = m.cursor();
    let first = cursor.saturating_sub(rows - 1);
    for (k, it) in items.iter().enumerate().skip(first).take(rows) {
        let y = top + (k - first) as i32 * ROW;
        if k == cursor {
            paint::highlight(
                d,
                Rectangle::new(Point::new(6, y), Size::new(SIZE - 12, ROW_H)),
                LAVENDER_LIGHT,
                LAVENDER,
            )?;
        }
        paint::icon(d, it.icon, Point::new(26, y + ROW_H as i32 / 2))?;
        let ty = y + (ROW_H as i32 - BODY.cap) / 2;
        paint::text(d, it.label, LABEL_X, ty, &BODY, INK, Align::Left)?;
    }
    // Hay más arriba o más abajo: una flechita en el borde de la lista.
    if first > 0 {
        arrow(d, top - 2, true)?;
    }
    if first + rows < items.len() {
        arrow(d, top + rows as i32 * ROW - 2, false)?;
    }
    paint::centered(d, "A baja · B sube · C elige", HINT_Y, &SMALL, MUTE)
}

/// Una flechita centrada con la punta en `y`, para arriba o para abajo.
fn arrow<D: DrawTarget<Color = Rgb565>>(d: &mut D, y: i32, up: bool) -> Result<(), D::Error> {
    let (c, s) = (SIZE as i32 / 2, if up { 1 } else { -1 });
    Triangle::new(
        Point::new(c, y),
        Point::new(c - 6, y + 5 * s),
        Point::new(c + 6, y + 5 * s),
    )
    .into_styled(PrimitiveStyle::with_fill(MUTE))
    .draw(d)
}

/// El tamaño de la pantalla, para quien dibuja el menú fuera de una franja.
pub const SCREEN: Size = Size::new(SIZE, SIZE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_goes_down_b_goes_up_and_both_wrap() {
        let mut m = Menu::new();
        assert_eq!(m.cursor(), 0);
        m.press(Nav::Down);
        m.press(Nav::Down);
        assert_eq!(m.cursor(), 2);
        m.press(Nav::Down);
        assert_eq!(m.cursor(), 0, "da la vuelta");
        m.press(Nav::Up);
        assert_eq!(m.cursor(), MAIN.len() - 1);
    }

    #[test]
    fn home_is_the_first_choice() {
        let mut m = Menu::new();
        assert_eq!(m.press(Nav::Accept), Some(Action::Home));
    }

    #[test]
    fn submenus_open_and_go_back() {
        let mut m = Menu::new();
        m.press(Nav::Down);
        m.press(Nav::Down);
        assert_eq!(m.press(Nav::Accept), None);
        assert_eq!(m.page(), PageId::Settings);
        assert_eq!(m.items()[0].label, "Desde el celular");
        assert_eq!(m.press(Nav::Accept), Some(Action::Portal));
        m.press(Nav::Down);
        assert_eq!(m.press(Nav::Accept), Some(Action::SyncTime));
        // Volver (la última) regresa al menú con Configuración elegida.
        m.press(Nav::Up);
        m.press(Nav::Up);
        assert_eq!(m.items()[m.cursor()].go, Go::Back);
        m.press(Nav::Accept);
        assert_eq!((m.page(), m.cursor()), (PageId::Main, 2));
    }

    #[test]
    fn games_list_has_every_game_and_back() {
        let mut m = Menu::new();
        m.press(Nav::Down);
        m.press(Nav::Accept);
        assert_eq!(m.page(), PageId::Games);
        let items = m.items();
        assert_eq!(items.len(), GameId::ALL.len() + 1);
        assert_eq!(m.press(Nav::Accept), Some(Action::Play(GameId::ALL[0])));
        // Al salir del juego se vuelve a la lista, con el mismo elegido.
        m.open_games(Some(GameId::ALL[0]));
        assert_eq!((m.page(), m.cursor()), (PageId::Games, 0));
        m.press(Nav::Up);
        m.press(Nav::Accept);
        assert_eq!((m.page(), m.cursor()), (PageId::Main, 1));
    }

    #[test]
    fn about_and_messages() {
        let mut m = Menu::new();
        m.about = about_text("0.1.0", &Settings::default());
        m.press(Nav::Up);
        m.press(Nav::Accept);
        m.press(Nav::Down);
        m.press(Nav::Down);
        m.press(Nav::Accept);
        assert_eq!(m.page(), PageId::About);
        assert!(m.body().unwrap().contains("sin configurar"));
        m.press(Nav::Accept);
        assert_eq!(m.page(), PageId::Settings);

        // Sin WiFi: el aviso ofrece configurarlo o volver.
        let now = LocalTime::new(20_768, 14, 32);
        m.show(Message::from_notice(Notice::NoWifi, now));
        assert_eq!(m.page(), PageId::Message);
        assert_eq!(m.press(Nav::Accept), Some(Action::Portal));
        m.press(Nav::Down);
        m.press(Nav::Accept);
        assert_eq!(m.page(), PageId::Settings);
        assert!(m.message.is_none());

        // Al volver de ponerse en hora: el aviso lleva a la casa.
        m.open_main();
        m.show(Message::from_notice(Notice::TimeSet, now));
        assert!(m.body().unwrap().contains("14:32 del mié 11/11/2026"));
        assert_eq!(m.press(Nav::Accept), Some(Action::Home));
        assert_eq!(m.page(), PageId::Main);
    }

    /// Lo que puede ocupar el texto de una opción.
    const LABEL_W: i32 = SIZE as i32 - LABEL_X - 8;

    /// ¿El texto del aviso entra en su tarjeta?
    fn fits(body: &str, items: usize) -> bool {
        let card_h = BOTTOM - items as i32 * ROW - TOP - 4;
        let lines = paint::count_lines(body, &BODY, TEXT_W) as i32;
        (lines - 1) * BODY.line + BODY.cap + BODY.descent + 4 <= card_h
    }

    #[test]
    fn every_page_fits_on_screen() {
        let now = LocalTime::new(20_768, 14, 32);
        let mut notices = std::vec![
            Notice::Saved,
            Notice::NewMonkey,
            Notice::Factory,
            Notice::TimeSet,
            Notice::NoWifi,
            Notice::TimeFailed(SyncError::NoServer),
        ];
        for f in captive::Failure::ALL {
            notices.push(Notice::TimeFailed(SyncError::Wifi(f)));
        }
        for n in notices {
            let msg = Message::from_notice(n, now);
            assert!(paint::header_fits(&msg.title), "{}", msg.title);
            assert!(fits(&msg.body, msg.items.len()), "{n:?}: {}", msg.body);
            for i in &msg.items {
                assert!(BODY.width(i.label) <= LABEL_W, "{}", i.label);
            }
        }
        let mut games = std::vec::Vec::new();
        for g in GameId::ALL {
            games.push(item(g.title(), Icon::Star, Go::Back));
        }
        for it in MAIN.iter().chain(SETTINGS).chain(&games) {
            assert!(BODY.width(it.label) <= LABEL_W, "{}", it.label);
        }
        for p in [PageId::Main, PageId::Games, PageId::Settings, PageId::About] {
            let mut m = Menu::new();
            let _ = m.stack.push((p, 0));
            assert!(paint::header_fits(m.title()), "{}", m.title());
        }
        // Lo más largo de "Acerca de": una red con nombre de 32 letras.
        let about = about_text(
            "0.10.0",
            &Settings {
                ssid: captive::text("Una red con nombre largo de 32 c").unwrap(),
                ..Settings::default()
            },
        );
        assert!(fits(&about, 1), "{about}");
        assert!(SMALL.width("A baja · B sube · C elige") <= SIZE as i32 - 8);
    }

    #[test]
    fn the_cursor_is_always_visible() {
        // Con más opciones de las que entran, la lista se corre.
        let rows = visible(TOP);
        assert!(rows >= 4, "entran {rows}");
        let mut m = Menu::new();
        m.open_games(None);
        let n = m.items().len();
        for _ in 0..n * 2 {
            let first = m.cursor().saturating_sub(rows - 1);
            assert!((first..first + rows).contains(&m.cursor()));
            m.press(Nav::Down);
        }
    }
}
