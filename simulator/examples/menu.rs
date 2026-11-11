//! El menú del 0xMono, las pantallas de la configuración desde el celular y
//! la página que se abre en el teléfono, con el mismo código que la placa
//! (crates `ui` y `settings`).
//!
//! ```text
//! cargo run -p simulator --example menu                  # ventana: A/S/D = botones A/B/C
//! cargo run -p simulator --example menu -- --png carpeta # cada pantalla como PNG
//! cargo run -p simulator --example menu -- --serve       # la página en http://127.0.0.1:8080
//! ```
//!
//! En la ventana, mantener A y D juntos 2 segundos abre el menú (como A+C en
//! la placa). La "casa" es un fondo de mentira: el 3D está en el ejemplo
//! `habitat`. Esc sale.

use std::convert::Infallible;
use std::io::{Read, Write as _};
use std::net::TcpListener;
use std::path::Path;
use std::time::Instant;

use captive::{BufWriter, Network, add_network, http, text};
use companion::{Button, Input, LocalTime, Pad};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use games::{GameId, Keys, Running, Status};
use minifb::{Key, Scale, Window, WindowOptions};
use settings::Settings;
use settings::handoff::{Notice, SyncError};
use settings::page::{self, Context, Form, Outcome};
use ui::menu::{self, Message, Nav};
use ui::screens::{self, Screen};
use ui::{Action, Menu, paint};

const SIZE: u32 = 240;
const VERSION: &str = env!("CARGO_PKG_VERSION");

struct Framebuffer(Vec<Rgb565>);

impl Framebuffer {
    fn new() -> Self {
        Self(vec![Rgb565::BLACK; (SIZE * SIZE) as usize])
    }

    fn to_u32(&self) -> Vec<u32> {
        self.0
            .iter()
            .map(|c| {
                let (r, g, b) = rgb8(*c);
                (r as u32) << 16 | (g as u32) << 8 | b as u32
            })
            .collect()
    }
}

/// De 5/6/5 bits a 8, como se ve en la pantalla.
fn rgb8(c: Rgb565) -> (u8, u8, u8) {
    (
        c.r() << 3 | c.r() >> 2,
        c.g() << 2 | c.g() >> 4,
        c.b() << 3 | c.b() >> 2,
    )
}

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(SIZE, SIZE)
    }
}

impl DrawTarget for Framebuffer {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        for Pixel(p, c) in pixels {
            if (0..SIZE as i32).contains(&p.x) && (0..SIZE as i32).contains(&p.y) {
                self.0[(p.y as u32 * SIZE + p.x as u32) as usize] = c;
            }
        }
        Ok(())
    }
}

fn save_png(path: &Path, fb: &Framebuffer, scale: u32) {
    let side = SIZE * scale;
    let mut data = Vec::with_capacity((side * side * 3) as usize);
    for y in 0..side {
        for x in 0..side {
            let (r, g, b) = rgb8(fb.0[((y / scale) * SIZE + x / scale) as usize]);
            data.extend([r, g, b]);
        }
    }
    let file = std::fs::File::create(path).expect("no se pudo crear el PNG");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), side, side);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&data).unwrap();
}

fn now() -> LocalTime {
    use chrono::{Local, Offset};
    let t = Local::now();
    LocalTime::from_unix(
        t.timestamp() as u64,
        (t.offset().fix().local_minus_utc() / 60) as i16,
    )
}

fn sample_settings() -> Settings {
    Settings {
        ssid: text("WiFi de casa").unwrap(),
        pass: text("clave-secreta").unwrap(),
        ..Settings::default()
    }
}

// --- PNG ------------------------------------------------------------------------

fn export(dir: &str) {
    std::fs::create_dir_all(dir).expect("no se pudo crear la carpeta");
    let dir = Path::new(dir);
    let shot = |name: &str, draw: &dyn Fn(&mut Framebuffer)| {
        let mut fb = Framebuffer::new();
        draw(&mut fb);
        let path = dir.join(format!("{name}.png"));
        save_png(&path, &fb, 2);
        println!("{}", path.display());
    };
    let menu_shot = |name: &str, m: &Menu| shot(name, &|fb| menu::draw(m, fb).unwrap());

    let mut m = Menu::new();
    m.about = menu::about_text(VERSION, &sample_settings());
    menu_shot("menu_1_principal", &m);
    m.press(Nav::Down);
    menu_shot("menu_2_principal_minijuegos", &m);
    m.press(Nav::Accept);
    menu_shot("menu_3_minijuegos", &m);
    m.open_main();
    m.press(Nav::Up);
    menu_shot("menu_4_principal_configuracion", &m);
    m.press(Nav::Accept);
    menu_shot("menu_5_configuracion", &m);
    m.press(Nav::Down);
    menu_shot("menu_6_configuracion_hora", &m);
    m.press(Nav::Down);
    m.press(Nav::Accept);
    menu_shot("menu_7_acerca", &m);

    let t = LocalTime::new(20_768, 14, 32);
    let notices = [
        ("aviso_sin_wifi", Notice::NoWifi),
        ("aviso_en_hora", Notice::TimeSet),
        (
            "aviso_hora_fallo_clave",
            Notice::TimeFailed(SyncError::Wifi(captive::Failure::WrongPassword)),
        ),
        (
            "aviso_hora_sin_internet",
            Notice::TimeFailed(SyncError::NoServer),
        ),
        ("aviso_guardado", Notice::Saved),
        ("aviso_mono_nuevo", Notice::NewMonkey),
        ("aviso_de_fabrica", Notice::Factory),
    ];
    for (name, n) in notices {
        let mut m = Menu::new();
        m.show(Message::from_notice(n, t));
        menu_shot(name, &m);
    }

    let screens = [
        ("config_1_preparando", Screen::Preparing),
        (
            "config_2_unirse",
            Screen::Join {
                ssid: "0xMono-4F2A",
                pass: "kd7wq3mz",
            },
        ),
        ("config_3_abrir", Screen::Open),
        ("config_4_guardado", Screen::Saved),
        ("config_5_volviendo", Screen::Leaving),
        (
            "hora_1_conectando",
            Screen::Syncing {
                ssid: "WiFi de casa",
            },
        ),
        ("hora_2_listo", Screen::Synced { now: t }),
        (
            "hora_3_fallo",
            Screen::SyncFailed {
                reason: captive::Failure::NotFound.message(),
            },
        ),
    ];
    for (name, s) in screens {
        shot(name, &|fb| screens::draw(fb, &s).unwrap());
    }

    // La página del celular, para abrir en el navegador.
    let nets = fake_networks();
    let current = Settings::default();
    let cx = Context {
        networks: &nets,
        current: &current,
        notice: None,
        now: Some(t),
        version: VERSION,
    };
    let html = |name: &str, f: &dyn Fn(&mut BufWriter) -> std::fmt::Result| {
        let mut buf = vec![0u8; page::MAX_PAGE];
        let mut w = BufWriter::new(&mut buf);
        f(&mut w).expect("la página no entra");
        let path = dir.join(format!("{name}.html"));
        std::fs::write(&path, w.as_bytes()).unwrap();
        println!("{} ({} bytes)", path.display(), w.len());
    };
    html("pagina_1_nueva", &|w| {
        page::page(w, &cx, &Form::from_settings(&current), None)
    });
    let saved = sample_settings();
    let cx2 = Context {
        current: &saved,
        ..cx
    };
    html("pagina_2_configurada", &|w| {
        page::page(w, &cx2, &Form::from_settings(&saved), None)
    });
    html("pagina_3_error", &|w| {
        page::page(
            w,
            &cx,
            &Form::from_settings(&current),
            Some("La clave del WiFi tiene que tener entre 8 y 63 caracteres."),
        )
    });
    html("pagina_4_listo", &|w| {
        page::done_page(
            w,
            &Outcome::Saved {
                settings: saved.clone(),
                clock: None,
            },
            VERSION,
        )
    });
    html("pagina_5_mono_nuevo", &|w| {
        page::done_page(w, &Outcome::NewMonkey { clock: None }, VERSION)
    });
    html("pagina_6_de_fabrica", &|w| {
        page::done_page(w, &Outcome::Factory { clock: None }, VERSION)
    });
}

fn fake_networks() -> heapless::Vec<Network, 16> {
    let mut list = heapless::Vec::new();
    for (ssid, rssi, open) in [
        ("WiFi de casa", -48, false),
        ("Fibertel WiFi482 2.4GHz", -71, false),
        ("Personal-WiFi-7A1", -80, false),
        ("Café Libre", -66, true),
    ] {
        add_network(&mut list, ssid, rssi, open);
    }
    list
}

// --- Página ---------------------------------------------------------------------

fn serve() {
    let listener = TcpListener::bind("127.0.0.1:8080").expect("no se pudo abrir el puerto 8080");
    println!("Página de configuración en http://127.0.0.1:8080 (Ctrl+C para salir)");
    let nets = fake_networks();
    let mut current = Settings::default();
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut buf = vec![0u8; 4096];
        let mut len = 0;
        let ok = loop {
            match stream.read(&mut buf[len..]) {
                Ok(0) | Err(_) => break false,
                Ok(n) => len += n,
            }
            match http::parse(&buf[..len]) {
                Ok(req) if len >= req.body_start + req.content_length => break true,
                Err(http::ParseError::Bad) => break false,
                _ if len == buf.len() => break false,
                _ => {}
            }
        };
        if !ok {
            continue;
        }
        let mut req = http::parse(&buf[..len]).unwrap();
        // En la PC el navegador pide 127.0.0.1: que cuente como el mono.
        req.host = captive::AP_HOST;
        let body_in = &buf[req.body_start..req.body_start + req.content_length];
        let cx = Context {
            networks: &nets,
            current: &current,
            notice: None,
            now: Some(now()),
            version: VERSION,
        };
        let mut head_buf = [0u8; 256];
        let mut body_buf = vec![0u8; page::MAX_PAGE];
        let mut head = BufWriter::new(&mut head_buf);
        let mut body = BufWriter::new(&mut body_buf);
        let out = page::respond(&req, body_in, &cx, &mut head, &mut body).expect("no entra");
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(body.as_bytes());
        if let Some(o) = out {
            println!("{o:?}\n  -> próximo arranque: {:?}", o.next());
            if let Outcome::Saved { settings, .. } = o {
                current = settings;
            }
        }
    }
}

// --- Ventana --------------------------------------------------------------------

enum Mode {
    House,
    Menu,
    Game(GameId, Running),
}

fn draw_house(fb: &mut Framebuffer, t: LocalTime) {
    fb.clear(paint::SKY_LIGHT).unwrap();
    paint::monkey(fb, Point::new(120, 110), 3).unwrap();
    let line = format!("{:02}:{:02}  (la casa)", t.hour(), t.minute() % 60);
    paint::centered(fb, &line, 170, &paint::BODY, paint::INK).unwrap();
    paint::centered(
        fb,
        "Mantené A+D para el menú",
        200,
        &paint::SMALL,
        paint::MUTE,
    )
    .unwrap();
}

fn window() {
    let mut window = Window::new(
        "0xMono — menú (A/S/D = botones, A+D = menú)",
        SIZE as usize,
        SIZE as usize,
        WindowOptions {
            scale: Scale::X2,
            ..WindowOptions::default()
        },
    )
    .expect("no se pudo abrir la ventana");
    window.set_target_fps(30);
    let mut fb = Framebuffer::new();
    let mut pad = Pad::new();
    let mut menu = Menu::new();
    menu.about = menu::about_text(VERSION, &sample_settings());
    let mut mode = Mode::House;
    let mut last = Instant::now();
    let mut seed = 1u32;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let dt = last.elapsed().as_millis() as u32;
        last = Instant::now();
        let raw = [Key::A, Key::S, Key::D].map(|k| window.is_key_down(k));
        let input = pad.update(raw, dt);
        if input == Some(Input::Chord) {
            menu.open_main();
            mode = Mode::Menu;
        }
        match &mut mode {
            Mode::House => draw_house(&mut fb, now()),
            Mode::Menu => {
                let nav = match input {
                    Some(Input::Tap(b) | Input::Hold(b)) => Some(match b {
                        Button::A => Nav::Down,
                        Button::B => Nav::Up,
                        Button::C => Nav::Accept,
                    }),
                    _ => None,
                };
                match nav.and_then(|n| menu.press(n)) {
                    Some(Action::Home) => mode = Mode::House,
                    Some(Action::Play(g)) => {
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        mode = Mode::Game(g, Running::new(g, seed));
                    }
                    Some(Action::Portal) => {
                        println!("(en la placa: se reinicia en modo configuración)");
                        menu.show(Message::new(
                            "Modo configuración",
                            "En la placa me reinicio, armo mi red WiFi y muestro el QR. Probá la página con --serve.",
                            &[],
                        ));
                    }
                    Some(Action::SyncTime) => {
                        println!("(en la placa: se reinicia para ponerse en hora)");
                        menu.show(Message::from_notice(Notice::TimeSet, now()));
                    }
                    None => {}
                }
                if matches!(mode, Mode::Menu) {
                    menu::draw(&menu, &mut fb).unwrap();
                }
            }
            Mode::Game(g, game) => {
                let keys = Keys {
                    a: raw[0],
                    b: raw[1],
                    c: raw[2],
                };
                if game.update(keys, dt) == Status::Exit {
                    menu.open_games(Some(*g));
                    mode = Mode::Menu;
                    menu::draw(&menu, &mut fb).unwrap();
                } else {
                    game.draw(&mut fb).unwrap();
                }
            }
        }
        window
            .update_with_buffer(&fb.to_u32(), SIZE as usize, SIZE as usize)
            .unwrap();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--png") {
        export(args.get(i + 1).map_or("menu_png", String::as_str));
    } else if args.iter().any(|a| a == "--serve") {
        serve();
    } else {
        window();
    }
}
