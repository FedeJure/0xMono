//! Los modos con WiFi. Arrancan después de un reinicio, con la memoria de la
//! casa convertida en memoria dinámica (`memory::give_to_heap`):
//!
//! - **Configuración** ([`Job::Portal`]): el mono busca las redes, arma la
//!   suya (`0xMono-XXXX`, clave al azar) y muestra un QR para unirse. El DHCP
//!   le dice al celular que el DNS es el mono y el DNS contesta a todo con
//!   `192.168.4.1`, así el chequeo de internet del teléfono abre la página
//!   solo. Al guardar, reinicia según lo elegido (`settings::page::Outcome`).
//!   C vuelve a la casa sin cambiar nada; a los 10 minutos, también.
//! - **Hora por internet** ([`Job::Sync`]): se conecta al WiFi guardado,
//!   pregunta la hora por SNTP y vuelve a la casa con ella. Si en 20 segundos
//!   no lo logra, vuelve con la hora que tenía.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write as _;
use core::net::Ipv4Addr;

use captive::http::{self, ParseError};
use captive::{AP_IP, BufWriter, Network, add_network};
use companion::{Input, LocalTime, Pad};
use embassy_executor::Spawner;
use embassy_futures::select::{Either3, select3};
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embedded_io_async::Write as _;
use esp_hal::gpio::Input as Pin;
use esp_hal::peripherals::{FROM_CPU_INTR0, TIMG0, WIFI};
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::wifi::ap::{AccessPointConfig, EventInfo};
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethod, AuthenticationMethodConfig, Config as WifiConfig, ControllerConfig,
    Interface, WifiController,
};
use esp_rtos::embassy::Executor;
use heapless::String;
use log::{error, info, warn};
use settings::Settings;
use settings::handoff::{HouseStart, Next, SyncError, SyncStart};
use settings::page::{self, Context, Outcome};
use settings::sntp;
use static_cell::StaticCell;
use ui::screens::Screen;

use crate::captive_net::{self, DhcpBuffers, DnsBuffers, seed};
use crate::house::show;
use crate::memory;
use crate::nvs::Nvs;
use crate::save_store::SaveStore;
use crate::st7789::Lcd;
use crate::{VERSION, handoff};

/// Si nadie configura nada en este tiempo, vuelve a la casa.
const PORTAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Lo máximo que espera la hora por internet (al prender).
const SYNC_TIMEOUT: Duration = Duration::from_secs(20);
/// Pedida a mano hay alguien mirando: se le da un poco más.
const MANUAL_SYNC_TIMEOUT: Duration = Duration::from_secs(30);
/// Cada cuánto se leen los botones mientras se espera otra cosa.
const TICK_MS: u64 = 30;
const BAND_PIXELS: usize = 240 * 32;

#[derive(Clone, Copy, Debug)]
pub enum Job {
    Portal,
    Sync(SyncStart),
}

/// Lo que cada modo necesita del hardware.
pub struct Hw {
    pub lcd: Lcd,
    pub buttons: [Pin<'static>; 3],
    pub wifi: WIFI<'static>,
    pub timg0: TIMG0<'static>,
    pub sw_int: FROM_CPU_INTR0<'static>,
}

struct Ctx {
    lcd: Lcd,
    band: Vec<u16>,
    buttons: [Pin<'static>; 3],
    nvs: Nvs,
    settings: Settings,
    /// La hora que tenía la casa al irse (la última guardada).
    clock: Option<LocalTime>,
    started: Instant,
    rng: Rng,
}

impl Ctx {
    fn show(&mut self, s: &Screen) {
        show(&mut self.lcd, &mut self.band, s);
    }

    /// `t` más lo que pasó desde que arrancó este modo.
    fn later(&self, t: Option<LocalTime>) -> Option<LocalTime> {
        let secs = self.started.elapsed().as_secs() as u32 + 1;
        t.map(|t| t.add_secs(secs))
    }
}

static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[inline(never)]
pub fn run(job: Job, hw: Hw, mut nvs: Nvs, settings: Settings) -> ! {
    let heap = memory::give_to_heap();
    info!("memoria dinámica: {heap} bytes");
    let Hw {
        lcd,
        buttons,
        wifi,
        timg0,
        sw_int,
    } = hw;
    let clock = SaveStore::open(&mut nvs).1.and_then(|s| s.clock);
    let ctx = Box::new(Ctx {
        lcd,
        band: vec![0; BAND_PIXELS],
        buttons,
        nvs,
        settings,
        clock,
        started: Instant::now(),
        rng: Rng::new(),
    });
    let timg0 = TimerGroup::new(timg0);
    esp_rtos::start(timg0.timer0, sw_int);
    EXECUTOR
        .init(Executor::new())
        .run(move |spawner| match job {
            Job::Portal => spawner.spawn(portal_task(spawner, ctx, wifi).unwrap()),
            Job::Sync(s) => spawner.spawn(sync_task(spawner, ctx, wifi, s).unwrap()),
        })
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

// --- Configuración -----------------------------------------------------------

/// Lo que ve cada pedido a la página. No cambia mientras dura el modo.
struct Portal {
    networks: heapless::Vec<Network, 16>,
    current: Settings,
    clock: Option<LocalTime>,
    started: Instant,
}

/// Lo que eligió el celular.
static CHOSEN: Signal<CriticalSectionRawMutex, Outcome> = Signal::new();
/// Cuántos teléfonos hay conectados a la red del mono.
static PHONES: Signal<CriticalSectionRawMutex, u32> = Signal::new();

#[embassy_executor::task]
async fn portal_task(spawner: Spawner, mut cx: Box<Ctx>, wifi: WIFI<'static>) {
    cx.show(&Screen::Preparing);
    let ap = Interface::access_point();
    let mac = ap.mac_address();
    let mut controller = WifiController::new(
        wifi,
        ControllerConfig::default()
            .with_initial_config(WifiConfig::Station(StationConfig::default())),
    )
    .expect("no arrancó el WiFi");

    // Las redes de alrededor, antes de prender la propia (después cambiar de
    // canal le cortaría la conexión al celular).
    let mut networks = heapless::Vec::new();
    match controller.scan_async(&captive_net::scan_config()).await {
        Ok(found) => {
            for n in found {
                let open = n.auth_method == Some(AuthenticationMethod::None);
                add_network(&mut networks, n.ssid.as_str(), n.signal_strength, open);
            }
        }
        Err(e) => warn!("no se pudo buscar redes: {e:?}"),
    }
    info!("{} redes encontradas", networks.len());

    let mut ssid: String<16> = String::new();
    let _ = write!(ssid, "0xMono-{:02X}{:02X}", mac[4], mac[5]);
    let pass = captive_net::random_pass(&cx.rng);
    let ap_config = AccessPointConfig::default()
        .with_ssid(ssid.as_str().try_into().unwrap())
        .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
            pass.as_str().try_into().unwrap(),
        ));
    controller
        .set_config(&WifiConfig::AccessPointStation(
            StationConfig::default(),
            ap_config,
        ))
        .expect("no arrancó la red del mono");
    info!("red del mono: «{ssid}», clave «{pass}»");

    let portal: &'static Portal = Box::leak(Box::new(Portal {
        networks,
        current: cx.settings.clone(),
        clock: cx.clock,
        started: cx.started,
    }));
    let ip = Ipv4Addr::from(AP_IP);
    let resources: &'static mut StackResources<8> = Box::leak(Box::new(StackResources::new()));
    let (stack, runner) = embassy_net::new(
        ap,
        embassy_net::Config::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(ip, 24),
            gateway: Some(ip),
            dns_servers: Default::default(),
        }),
        resources,
        seed(&cx.rng),
    );
    spawner.spawn(net_task(runner).unwrap());
    spawner.spawn(phones_task(controller).unwrap());
    spawner.spawn(dhcp_task(stack, Box::leak(Box::new(DhcpBuffers::new()))).unwrap());
    spawner.spawn(dns_task(stack, Box::leak(Box::new(DnsBuffers::new()))).unwrap());
    for _ in 0..2 {
        spawner.spawn(http_task(stack, portal).unwrap());
    }
    info!(
        "memoria dinámica libre: {} bytes, pila libre: {} bytes",
        esp_alloc::HEAP.free(),
        memory::stack_free()
    );

    cx.show(&Screen::Join {
        ssid: &ssid,
        pass: &pass,
    });
    let mut pad = Pad::new();
    let mut last = Instant::now();
    loop {
        match select3(
            CHOSEN.wait(),
            PHONES.wait(),
            Timer::after(Duration::from_millis(TICK_MS)),
        )
        .await
        {
            Either3::First(outcome) => {
                info!("elegido en la página: {outcome:?}");
                cx.show(&Screen::Saved);
                match &outcome {
                    Outcome::Saved { settings, .. } => {
                        if !cx.nvs.save_settings(settings) {
                            error!("no se pudo guardar la configuración");
                        }
                    }
                    Outcome::Factory { .. } => {
                        if !cx.nvs.factory_reset() {
                            error!("no se pudo borrar todo");
                        }
                    }
                    Outcome::NewMonkey { .. } => {}
                }
                // Que la respuesta llegue al celular antes de apagar la red.
                Timer::after(Duration::from_secs(2)).await;
                let next = match outcome.next() {
                    // Sin la hora del celular (sin JavaScript), la de la casa.
                    Next::House(h) if h.clock.is_none() => Next::House(HouseStart {
                        clock: cx.later(cx.clock),
                        ..h
                    }),
                    Next::House(h) => Next::House(HouseStart {
                        clock: h.clock.map(|t| t.add_secs(3)),
                        ..h
                    }),
                    Next::Sync(s) => Next::Sync(SyncStart {
                        fallback: s.fallback.map(|t| t.add_secs(3)).or(cx.later(cx.clock)),
                        ..s
                    }),
                    n => n,
                };
                handoff::reboot(next);
            }
            Either3::Second(phones) => {
                info!("teléfonos conectados: {phones}");
                if phones > 0 {
                    cx.show(&Screen::Open);
                } else {
                    cx.show(&Screen::Join {
                        ssid: &ssid,
                        pass: &pass,
                    });
                }
            }
            Either3::Third(()) => {
                let now = Instant::now();
                let dt = now.duration_since(last).as_millis() as u32;
                last = now;
                let pressed = cx.buttons.each_ref().map(Pin::is_low);
                let cancel = pad.update(pressed, dt) == Some(Input::Tap(companion::Button::C));
                if cancel || cx.started.elapsed() > PORTAL_TIMEOUT {
                    info!("configuración cancelada");
                    cx.show(&Screen::Leaving);
                    let clock = cx.later(cx.clock);
                    handoff::reboot(Next::House(HouseStart {
                        clock,
                        ..HouseStart::default()
                    }));
                }
            }
        }
    }
}

#[embassy_executor::task]
async fn phones_task(controller: WifiController<'static>) {
    let mut phones = 0u32;
    loop {
        match controller
            .wait_for_access_point_connected_event_async()
            .await
        {
            Ok(EventInfo::Connected(_)) => phones += 1,
            Ok(EventInfo::Disconnected(_)) => phones = phones.saturating_sub(1),
            Err(e) => {
                warn!("eventos del WiFi: {e:?}");
                Timer::after(Duration::from_secs(1)).await;
                continue;
            }
        }
        PHONES.signal(phones);
    }
}

#[embassy_executor::task]
async fn dhcp_task(stack: Stack<'static>, bufs: &'static mut DhcpBuffers) {
    captive_net::dhcp_server(stack, bufs).await
}

#[embassy_executor::task]
async fn dns_task(stack: Stack<'static>, bufs: &'static mut DnsBuffers) {
    captive_net::dns_server(stack, bufs).await
}

#[embassy_executor::task(pool_size = 2)]
async fn http_task(stack: Stack<'static>, portal: &'static Portal) {
    // En la memoria dinámica: la tarea en sí queda chica.
    let mut rx = vec![0u8; 1536];
    let mut tx = vec![0u8; 2048];
    let mut request = vec![0u8; 3072];
    let mut head = vec![0u8; 256];
    let mut body = vec![0u8; page::MAX_PAGE];
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(Duration::from_secs(5)));
        if socket.accept(80).await.is_err() {
            continue;
        }
        if let Some(outcome) = serve(&mut socket, portal, &mut request, &mut head, &mut body).await
        {
            CHOSEN.signal(outcome);
        }
        let _ = socket.flush().await;
        socket.close();
        Timer::after(Duration::from_millis(50)).await;
        socket.abort();
    }
}

/// Lee un pedido, contesta y devuelve lo elegido, si hubo.
async fn serve(
    socket: &mut TcpSocket<'_>,
    portal: &Portal,
    request: &mut [u8],
    head: &mut [u8],
    body: &mut [u8],
) -> Option<Outcome> {
    // Hasta tener la cabecera y, si es un POST, el cuerpo entero.
    let mut len = 0;
    loop {
        let n = socket.read(&mut request[len..]).await.ok()?;
        if n == 0 {
            return None;
        }
        len += n;
        match http::parse(&request[..len]) {
            Ok(req) if len >= req.body_start + req.content_length => break,
            Err(ParseError::Bad) => return None,
            _ if len == request.len() => return None,
            _ => {}
        }
    }
    let req = http::parse(&request[..len]).ok()?;
    let body_in = &request[req.body_start..req.body_start + req.content_length];
    let secs = portal.started.elapsed().as_secs() as u32;
    let cx = Context {
        networks: &portal.networks,
        current: &portal.current,
        notice: None,
        now: portal.clock.map(|t| t.add_secs(secs)),
        version: VERSION,
    };
    let mut head = BufWriter::new(head);
    let mut body = BufWriter::new(body);
    let outcome = page::respond(&req, body_in, &cx, &mut head, &mut body).ok()?;
    socket.write_all(head.as_bytes()).await.ok()?;
    socket.write_all(body.as_bytes()).await.ok()?;
    outcome
}

// --- Hora por internet --------------------------------------------------------

#[embassy_executor::task]
async fn sync_task(spawner: Spawner, mut cx: Box<Ctx>, wifi: WIFI<'static>, job: SyncStart) {
    let s = cx.settings.clone();
    cx.show(&Screen::Syncing { ssid: &s.ssid });
    let auth = if s.pass.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        AuthenticationMethodConfig::Wpa2Personal(s.pass.as_str().try_into().unwrap())
    };
    let station = StationConfig::default()
        .with_ssid(s.ssid.as_str().try_into().unwrap())
        .with_authentication(auth);
    let mut controller = WifiController::new(
        wifi,
        ControllerConfig::default().with_initial_config(WifiConfig::Station(station)),
    )
    .expect("no arrancó el WiFi");
    let resources: &'static mut StackResources<4> = Box::leak(Box::new(StackResources::new()));
    let (stack, runner) = embassy_net::new(
        Interface::station(),
        embassy_net::Config::dhcpv4(Default::default()),
        resources,
        seed(&cx.rng),
    );
    spawner.spawn(net_task(runner).unwrap());

    let limit = if job.manual {
        MANUAL_SYNC_TIMEOUT
    } else {
        SYNC_TIMEOUT
    };
    let result = match with_timeout(limit, internet_time(&mut controller, stack)).await {
        Ok(r) => r,
        Err(_) if stack.is_config_up() => Err(SyncError::NoServer),
        Err(_) if stack.is_link_up() => Err(SyncError::Wifi(captive::Failure::NoIp)),
        Err(_) => Err(SyncError::Wifi(captive::Failure::NotFound)),
    };
    info!("pila libre: {} bytes", memory::stack_free());
    let next = match result {
        Ok(unix) => {
            let now = LocalTime::from_unix(unix, s.utc_offset_min);
            info!(
                "en hora: {} {} (UTC{:+} min)",
                settings::date::DayLabel(now),
                settings::date::Clock(now),
                s.utc_offset_min
            );
            let got = Instant::now();
            if job.manual {
                cx.show(&Screen::Synced { now });
                Timer::after(Duration::from_secs(2)).await;
            }
            let secs = got.elapsed().as_secs() as u32 + 1;
            HouseStart {
                clock: Some(now.add_secs(secs)),
                new_monkey: job.new_monkey,
                notice: job.manual.then_some(settings::handoff::Notice::TimeSet),
            }
        }
        Err(e) => {
            warn!("no se pudo poner en hora: {e:?}");
            if job.manual {
                cx.show(&Screen::SyncFailed {
                    reason: e.message(),
                });
                Timer::after(Duration::from_secs(3)).await;
            }
            HouseStart {
                clock: cx.later(job.fallback.or(cx.clock)),
                new_monkey: job.new_monkey,
                notice: job
                    .manual
                    .then_some(settings::handoff::Notice::TimeFailed(e)),
            }
        }
    };
    handoff::reboot(Next::House(next));
}

/// Se conecta y le pregunta la hora a los servidores, uno por uno.
async fn internet_time(
    controller: &mut WifiController<'static>,
    stack: Stack<'static>,
) -> Result<u64, SyncError> {
    captive_net::connect(controller, stack, 2)
        .await
        .map_err(SyncError::Wifi)?;
    info!("conectado; preguntando la hora");
    let mut rx_meta = [PacketMetadata::EMPTY; 2];
    let mut tx_meta = [PacketMetadata::EMPTY; 2];
    let mut rx = [0u8; 128];
    let mut tx = [0u8; 128];
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
    socket.bind(0).map_err(|_| SyncError::NoServer)?;
    for server in sntp::SERVERS {
        let addrs = match with_timeout(
            Duration::from_secs(4),
            stack.dns_query(server, DnsQueryType::A),
        )
        .await
        {
            Ok(Ok(a)) => a,
            other => {
                warn!("DNS de {server}: {other:?}");
                continue;
            }
        };
        for addr in addrs {
            let IpAddress::Ipv4(_) = addr else { continue };
            if socket
                .send_to(&sntp::request(), (addr, sntp::PORT))
                .await
                .is_err()
            {
                continue;
            }
            let mut buf = [0u8; 64];
            match with_timeout(Duration::from_secs(3), socket.recv_from(&mut buf)).await {
                Ok(Ok((n, _))) => match sntp::parse_reply(&buf[..n]) {
                    Ok(unix) => return Ok(unix),
                    Err(e) => warn!("{server}: respuesta inválida ({e:?})"),
                },
                _ => warn!("{server} no contestó"),
            }
        }
    }
    Err(SyncError::NoServer)
}
