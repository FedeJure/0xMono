//! El mono en su casa, el menú y los minijuegos: todo lo que usa el 3D.
//!
//! - **En la casa:** cualquier toque le habla y suena un pitido; mantener B
//!   adelanta el reloj una hora.
//! - **A+C mantenidos** (`companion::buttons::CHORD_MS`) abren el menú desde
//!   cualquier lado. En el menú, A baja, B sube y C elige.
//! - **Minijuegos:** cada fotograma se dibuja el juego entero por franjas.
//!
//! El reloj y el guardado siguen andando en el menú y en los juegos.

use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;

use companion::save::SaveData;
use companion::{Button, Companion, Input, LocalTime, Pad, Personality};
use engine3d::PixelOrder;
use esp_hal::delay::Delay;
use esp_hal::gpio::Input as Pin;
use esp_hal::peripherals::{GPIO10, UART1};
use esp_hal::rng::Rng;
use esp_hal::time::Instant;
use esp_hal::uart::{Config as UartConfig, Uart};
use games::{GameId, Keys, Running, Status};
use habitat::Habitat;
use log::{info, warn};
use settings::Settings;
use settings::handoff::{HouseStart, Next, SyncStart};
use sound::Sound;
use ui::menu::{self, Message, Nav};
use ui::screens::{self, Screen};
use ui::{Action, Menu};

use crate::memory::{self, ARENA_LEN, Arena};
use crate::nvs::Nvs;
use crate::save_store::SaveStore;
use crate::st7789::Lcd;
use crate::{VERSION, handoff};

const W: usize = 240;
const BAND_PIXELS: usize = W * 32;

/// Veces que corre el reloj más rápido que el real: 60 hace que un día dure
/// 24 minutos, para probar.
const SPEED: u32 = 1;
/// Un fotograma lento no hace saltar la animación.
const MAX_DT_MS: u32 = 100;
/// Pausa de cada vuelta mientras se muestra el menú.
const MENU_TICK_MS: u32 = 15;
/// Cada cuánto se guarda la hora. Hablarle guarda enseguida.
const SAVE_EVERY_SECS: u32 = 5 * 60;
/// La placa no conoce la hora: la primera vez arranca a las 8.
const FIRST_DAY: u32 = 1;
const FIRST_HOUR: u8 = 8;

/// Lo de la casa que vive en el [`Arena`] (la RAM que el WiFi usa como
/// memoria dinámica en los otros modos). Las franjas van primero: antes del
/// primer dibujo, ese lugar es la pila para armar el hábitat
/// ([`build_habitat`]).
#[repr(C)]
struct HouseRam {
    color: [u16; BAND_PIXELS],
    depth: [u16; BAND_PIXELS],
    habitat: MaybeUninit<Habitat>,
    game: Option<(GameId, Running)>,
}

const _: () = assert!(
    size_of::<HouseRam>() <= ARENA_LEN,
    "la casa no entra en el arena"
);
const _: () = assert!(align_of::<HouseRam>() <= align_of::<Arena>());

/// Arma el hábitat en el arena y deja listo el resto de lo de la casa.
fn house_ram(r: &mut memory::R) -> &'static mut HouseRam {
    struct Args {
        slot: *mut MaybeUninit<Habitat>,
        r: *mut memory::R,
    }
    unsafe extern "C" fn build(args: *mut u8) {
        // SAFETY: `args` apunta a un `Args` válido mientras dura la llamada.
        let a = unsafe { &mut *(args as *mut Args) };
        build_habitat(unsafe { &mut *a.slot }, unsafe { &mut *a.r });
    }

    let p = memory::arena() as *mut HouseRam;
    // SAFETY: el arena es nuestro, alcanza y está alineado (ver los `assert`
    // de arriba). El hábitat se arma usando de pila el lugar de las franjas
    // (todavía sin usar) y después se inicializa cada campo antes de crear
    // la referencia.
    unsafe {
        let scratch = p as *mut u8;
        let mut args = Args {
            slot: addr_of_mut!((*p).habitat),
            r,
        };
        let used = memory::call_on_stack(
            scratch,
            scratch.add(core::mem::offset_of!(HouseRam, habitat)),
            build,
            &mut args as *mut Args as *mut u8,
        );
        info!("armar el hábitat usó {used} bytes de pila");
        addr_of_mut!((*p).color).write([0; BAND_PIXELS]);
        addr_of_mut!((*p).depth).write([0; BAND_PIXELS]);
        addr_of_mut!((*p).game).write(None);
        &mut *p
    }
}

enum View {
    House,
    Menu,
    Game,
}

pub struct Hw {
    pub lcd: Lcd,
    pub buttons: [Pin<'static>; 3],
    pub uart: UART1<'static>,
    pub sound_tx: GPIO10<'static>,
}

/// Fuera de línea: así sus marcos de pila no se suman a los de `main`.
#[inline(never)]
pub fn run(hw: Hw, mut nvs: Nvs, settings: Settings, start: HouseStart) -> ! {
    let Hw {
        mut lcd,
        buttons,
        uart,
        sound_tx,
    } = hw;
    // Solo transmitimos: así el TX del módulo, de 5 V, nunca toca el ESP32.
    let mut player = Uart::new(uart, UartConfig::default().with_baudrate(9600))
        .expect("config UART inválida")
        .with_tx(sound_tx);
    // Darle tiempo a arrancar antes de hablarle.
    Delay::new().delay_millis(500);
    let _ = player.write(&sound::set_volume(sound::SAFE_VOLUME));
    let mut beep = || {
        let _ = player.write(&sound::play(Sound::Beep));
    };

    let (mut store, saved) = SaveStore::open(&mut nvs);
    let (mut seed, mut last_seen, saved_clock) = match saved {
        Some(s) => (s.seed, s.last_seen, s.clock),
        None => (Rng::new().random(), None, None),
    };
    if start.new_monkey || saved.is_none() {
        if start.new_monkey {
            seed = Rng::new().random();
            last_seen = None;
        }
        info!("personaje nuevo (semilla {seed:#x})");
    } else {
        info!("personaje guardado (semilla {seed:#x})");
    }

    let r = memory::renderer();
    let ram = house_ram(r);
    // SAFETY: `house_ram` lo dejó armado.
    let hab = unsafe { ram.habitat.assume_init_mut() };
    let color = &mut ram.color;
    let depth = &mut ram.depth;
    let game = &mut ram.game;

    let mut now = start
        .clock
        .or(saved_clock)
        .unwrap_or(LocalTime::new(FIRST_DAY, FIRST_HOUR, 0));
    let mut c = Companion::new(Personality::from_seed(seed), last_seen, now);
    hab.teleport(&c);
    info!(
        "casa del mono: arrancando a las {:02}:{:02} ({})",
        now.hour(),
        now.minute() % 60,
        now.weekday().short_name()
    );

    let mut menu = Menu::new();
    menu.about = menu::about_text(VERSION, &settings);
    let mut view = View::House;
    let mut menu_dirty = false;
    if let Some(n) = start.notice {
        menu.show(Message::from_notice(n, now));
        view = View::Menu;
        menu_dirty = true;
    }

    let mut pad = Pad::new();
    let mut clock = now.total_secs();
    let mut rest_ms = 0u32;
    // `None` para guardar enseguida: un personaje nuevo o una hora nueva.
    let fresh = saved.is_none() || start.new_monkey || start.clock.is_some();
    let mut saved_clock = (!fresh).then_some(clock);
    let mut saved_seen = c.last_seen;
    let mut last = Instant::now();
    let mut activity = c.activity();
    let mut frames = 0u32;
    let mut busy_ms = 0u32;
    let mut away_activity = None;
    loop {
        // En el menú quieto no se dibuja nada y una vuelta tarda microsegundos:
        // sin esta pausa daría 0 ms siempre y los botones nunca llegarían a
        // pasar el antirrebote (el menú quedaba trabado).
        if matches!(view, View::Menu) {
            Delay::new().delay_millis(MENU_TICK_MS);
        }
        let frame = Instant::now();
        let real_dt = (frame - last).as_millis() as u32;
        let dt = real_dt.min(MAX_DT_MS);
        // Solo se descuentan los milisegundos enteros: el resto queda para la
        // vuelta siguiente en vez de perderse.
        last += esp_hal::time::Duration::from_millis(real_dt as u64);

        rest_ms += real_dt * SPEED;
        clock += rest_ms / 1000;
        rest_ms %= 1000;
        now = LocalTime::from_total_secs(clock);

        let pressed = buttons.each_ref().map(Pin::is_low);
        let input = pad.update(pressed, dt);
        if input == Some(Input::Chord) {
            info!("menú");
            if matches!(view, View::House) {
                away_activity = Some(c.activity());
            }
            *game = None;
            menu.open_main();
            view = View::Menu;
            menu_dirty = true;
            beep();
        }

        let _ = c.update(now, dt);
        match view {
            View::House => {
                match input {
                    Some(Input::Tap(_)) => {
                        c.poke(now);
                        beep();
                    }
                    Some(Input::Hold(Button::B)) => {
                        clock += 3600;
                        now = LocalTime::from_total_secs(clock);
                        saved_clock = None;
                        info!("reloj: {:02}:{:02}", now.hour(), now.minute() % 60);
                    }
                    _ => {}
                }
                hab.update(&c, now, dt);
                hab.render(r, &c, now, color, depth, |band| {
                    lcd.window(band.x0, band.y0, band.width, band.height);
                    lcd.pixels(&band.color[..]);
                });
            }
            View::Menu => {
                let nav = match input {
                    Some(Input::Tap(b) | Input::Hold(b)) => Some(match b {
                        Button::A => Nav::Down,
                        Button::B => Nav::Up,
                        Button::C => Nav::Accept,
                    }),
                    _ => None,
                };
                if let Some(nav) = nav {
                    menu_dirty = true;
                    if nav == Nav::Accept {
                        beep();
                    }
                    match menu.press(nav) {
                        None => {}
                        Some(Action::Home) => {
                            view = View::House;
                            // Si mientras tanto cambió de actividad, aparece
                            // directamente donde le toca.
                            if away_activity.take() != Some(c.activity()) {
                                hab.teleport(&c);
                            }
                            hab.stage.invalidate_all();
                        }
                        Some(Action::Play(id)) => {
                            info!("jugando a {}", id.title());
                            *game = Some((id, Running::new(id, Rng::new().random())));
                            view = View::Game;
                        }
                        Some(Action::Portal) => {
                            save(&mut store, &mut nvs, seed, &c, now);
                            show(&mut lcd, color, &Screen::Preparing);
                            handoff::reboot(Next::Portal);
                        }
                        Some(Action::SyncTime) if settings.has_wifi() => {
                            save(&mut store, &mut nvs, seed, &c, now);
                            show(
                                &mut lcd,
                                color,
                                &Screen::Syncing {
                                    ssid: &settings.ssid,
                                },
                            );
                            handoff::reboot(Next::Sync(SyncStart {
                                manual: true,
                                new_monkey: false,
                                fallback: Some(now),
                            }));
                        }
                        Some(Action::SyncTime) => {
                            menu.show(Message::from_notice(settings::handoff::Notice::NoWifi, now));
                        }
                    }
                }
                if menu_dirty && matches!(view, View::Menu) {
                    menu_dirty = false;
                    ui::band::render(
                        color,
                        true,
                        |b| {
                            let _ = menu::draw(&menu, b);
                        },
                        |y0, rows, px| {
                            lcd.window(0, y0, W, rows);
                            lcd.pixels(px);
                        },
                    );
                }
            }
            View::Game => {
                if let Some((id, g)) = game.as_mut() {
                    let keys = Keys {
                        a: pressed[0],
                        b: pressed[1],
                        c: pressed[2],
                    };
                    if g.update(keys, dt) == Status::Exit {
                        menu.open_games(Some(*id));
                        *game = None;
                        view = View::Menu;
                        menu_dirty = true;
                    } else {
                        ui::band::render(
                            color,
                            true,
                            |b| {
                                let _ = g.draw(b);
                            },
                            |y0, rows, px| {
                                lcd.window(0, y0, W, rows);
                                lcd.pixels(px);
                            },
                        );
                    }
                } else {
                    view = View::Menu;
                    menu_dirty = true;
                }
            }
        }

        let due = saved_clock.is_none_or(|t| clock >= t + SAVE_EVERY_SECS);
        if due || c.last_seen != saved_seen {
            save(&mut store, &mut nvs, seed, &c, now);
            // Si falló, se reintenta en el próximo turno y no en cada fotograma.
            saved_clock = Some(clock);
            saved_seen = c.last_seen;
        }

        if c.activity() != activity {
            activity = c.activity();
            info!(
                "{:02}:{:02} {}",
                now.hour(),
                now.minute() % 60,
                activity.label()
            );
        }
        busy_ms += frame.elapsed().as_millis() as u32;
        frames += 1;
        if frames == 300 {
            info!(
                "{} ms/fotograma, pila libre {} bytes",
                busy_ms / frames,
                memory::stack_free()
            );
            frames = 0;
            busy_ms = 0;
        }
    }
}

/// Armar el hábitat usa ~23 KB de pila de paso (escenas que se copian), más
/// de lo que queda libre con el WiFi en el mismo programa: por eso se llama
/// sobre otra pila (`house_ram`).
#[inline(never)]
fn build_habitat<'a>(slot: &'a mut MaybeUninit<Habitat>, r: &mut memory::R) -> &'a mut Habitat {
    slot.write(Habitat::new(r, PixelOrder::Swapped))
}

fn save(store: &mut SaveStore, nvs: &mut Nvs, seed: u32, c: &Companion, now: LocalTime) {
    let data = SaveData {
        seed,
        last_seen: c.last_seen,
        clock: Some(now),
    };
    if !store.save(nvs, &data) {
        warn!("no se pudo guardar");
    }
}

/// Dibuja una pantalla quieta por franjas.
pub fn show(lcd: &mut Lcd, band: &mut [u16], s: &Screen) {
    ui::band::render(
        band,
        true,
        |b| {
            let _ = screens::draw(b, s);
        },
        |y0, rows, px| {
            lcd.window(0, y0, W, rows);
            lcd.pixels(px);
        },
    );
}
