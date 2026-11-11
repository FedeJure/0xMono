//! Modo configuración: el mono arma su propia red WiFi y sirve la página.
//!
//! 1. Busca las redes de alrededor (antes de prender su red, para no cortarle
//!    la conexión al celular después).
//! 2. Prende la red `Mono-XXXX` con una clave al azar y muestra el QR.
//! 3. DHCP le da una IP al celular y le dice que el DNS es el mono; el DNS
//!    contesta "el mono" a todo, así el chequeo de internet del teléfono cae en
//!    la página y se abre sola.
//! 4. Al guardar: flash y reinicio, que arranca en modo en línea.

use core::fmt::Write;
use core::net::Ipv4Addr;

use companion::Gesture;
use embassy_executor::Spawner;
use embassy_futures::select::{Either3, select3};
use embassy_net::{Ipv4Cidr, Stack, StackResources, StaticConfigV4, tcp::TcpSocket};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use embedded_io_async::Write as _;
use esp_hal::peripherals::WIFI;
use esp_hal::rng::Rng;
use esp_radio::wifi::ap::{AccessPointConfig, EventInfo};
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethod, AuthenticationMethodConfig, Config as WifiConfig, ControllerConfig,
    Interface, WifiController,
};
use heapless::{String, Vec};
use log::{error, info, warn};
use miner::Config;
use miner::http::{self, ParseError};
use miner::portal::{self, AP_IP, BufWriter, Network};
use miner::screen::Screen;
use static_cell::StaticCell;

use crate::captive_net::{self, DhcpBuffers, DnsBuffers, seed};
use crate::display::Panel;
use crate::handoff::{self, Request};
use crate::store::Store;
use crate::{Buttons, TICK_MS, net_task};

/// Si ya había algo guardado y nadie configura nada en este tiempo, vuelve a
/// lo de antes.
const TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Lo que ve cada pedido a la página. No cambia mientras dura el modo.
struct Portal {
    networks: Vec<Network, 16>,
    current: Option<Config>,
    notice: Option<String<200>>,
}

static PORTAL: StaticCell<Portal> = StaticCell::new();
static RESOURCES: StaticCell<StackResources<8>> = StaticCell::new();
/// La configuración que mandó el celular.
static SAVED: Signal<CriticalSectionRawMutex, Config> = Signal::new();
/// Cuántos teléfonos hay conectados a la red del mono.
static PHONES: Signal<CriticalSectionRawMutex, u32> = Signal::new();

#[allow(clippy::too_many_arguments)]
pub async fn run(
    spawner: Spawner,
    wifi: WIFI<'static>,
    saved: Option<Config>,
    request: Request,
    mut panel: Panel,
    mut buttons: Buttons,
    mut store: Store,
    rng: Rng,
) -> ! {
    panel.show(&Screen::Scanning);

    let ap = Interface::access_point();
    let mac = ap.mac_address();
    let mut controller = WifiController::new(
        wifi,
        ControllerConfig::default()
            .with_initial_config(WifiConfig::Station(StationConfig::default())),
    )
    .expect("no arrancó el WiFi");

    let mut networks = Vec::new();
    match controller.scan_async(&captive_net::scan_config()).await {
        Ok(found) => {
            for n in found {
                info!("  {} ({} dBm)", n.ssid.as_str(), n.signal_strength);
                let open = n.auth_method == Some(AuthenticationMethod::None);
                portal::add_network(&mut networks, n.ssid.as_str(), n.signal_strength, open);
            }
        }
        Err(e) => warn!("no se pudo buscar redes: {e:?}"),
    }
    info!("{} redes encontradas", networks.len());

    // Con la radio prendida el generador de números al azar es de verdad.
    let mut ssid: String<16> = String::new();
    let _ = write!(ssid, "Mono-{:02X}{:02X}", mac[4], mac[5]);
    let pass = captive_net::random_pass(&rng);
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

    let mut notice = None;
    if let Request::Setup(Some(failure)) = request {
        let mut s: String<200> = String::new();
        let who = saved.as_ref().map_or("tu red", |c| c.ssid.as_str());
        let _ = write!(s, "No pude conectar a «{who}»: {}", failure.message());
        notice = Some(s);
    }
    let has_config = saved.is_some();
    let portal: &'static Portal = PORTAL.init(Portal {
        networks,
        current: saved,
        notice,
    });

    let ip = Ipv4Addr::from(AP_IP);
    let (stack, runner) = embassy_net::new(
        ap,
        embassy_net::Config::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(ip, 24),
            gateway: Some(ip),
            dns_servers: Default::default(),
        }),
        RESOURCES.uninit().write(StackResources::new()),
        seed(&rng),
    );
    spawner.spawn(net_task(runner).unwrap());
    spawner.spawn(phones_task(controller).unwrap());
    spawner.spawn(dhcp_task(stack).unwrap());
    spawner.spawn(dns_task(stack).unwrap());
    for _ in 0..2 {
        spawner.spawn(http_task(stack, portal).unwrap());
    }

    panel.show(&Screen::Join {
        ssid: &ssid,
        pass: &pass,
    });
    let start = Instant::now();
    let mut last = Instant::now();
    loop {
        match select3(
            SAVED.wait(),
            PHONES.wait(),
            Timer::after(Duration::from_millis(TICK_MS)),
        )
        .await
        {
            Either3::First(config) => {
                info!("guardando: red «{}»", config.ssid);
                panel.show(&Screen::Saved { ssid: &config.ssid });
                if !store.save(&config) {
                    error!("no se pudo guardar en la flash");
                }
                // Que la respuesta llegue al celular antes de apagar la red.
                Timer::after(Duration::from_secs(2)).await;
                handoff::reboot();
            }
            Either3::Second(phones) => {
                info!("teléfonos conectados: {phones}");
                if phones > 0 {
                    panel.show(&Screen::Open);
                } else {
                    panel.show(&Screen::Join {
                        ssid: &ssid,
                        pass: &pass,
                    });
                }
            }
            Either3::Third(()) => {
                let now = Instant::now();
                let dt = now.duration_since(last).as_millis() as u32;
                last = now;
                // C cancela y vuelve a lo guardado (si hay algo).
                let cancel = buttons.poll_c(dt) == Some(Gesture::Tap);
                if has_config && (cancel || now.duration_since(start) > TIMEOUT) {
                    info!("configuración cancelada");
                    handoff::reboot();
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
async fn dhcp_task(stack: Stack<'static>) {
    let mut bufs = DhcpBuffers::new();
    captive_net::dhcp_server(stack, &mut bufs).await
}

#[embassy_executor::task]
async fn dns_task(stack: Stack<'static>) {
    let mut bufs = DnsBuffers::new();
    captive_net::dns_server(stack, &mut bufs).await
}

#[embassy_executor::task(pool_size = 2)]
async fn http_task(stack: Stack<'static>, portal: &'static Portal) {
    let mut rx = [0u8; 1536];
    let mut tx = [0u8; 2048];
    let mut request = [0u8; 3072];
    let mut head = [0u8; 256];
    let mut body = [0u8; 10 * 1024];
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(Duration::from_secs(5)));
        if socket.accept(80).await.is_err() {
            continue;
        }
        if let Some(config) = serve(&mut socket, portal, &mut request, &mut head, &mut body).await {
            SAVED.signal(config);
        }
        let _ = socket.flush().await;
        socket.close();
        Timer::after(Duration::from_millis(50)).await;
        socket.abort();
    }
}

/// Lee un pedido, contesta y devuelve la configuración si se guardó una.
async fn serve(
    socket: &mut TcpSocket<'_>,
    portal: &Portal,
    request: &mut [u8],
    head: &mut [u8],
    body: &mut [u8],
) -> Option<Config> {
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
    let mut head = BufWriter::new(head);
    let mut body = BufWriter::new(body);
    let saved = portal::respond(
        &req,
        body_in,
        &portal.networks,
        portal.current.as_ref(),
        portal.notice.as_deref(),
        &mut head,
        &mut body,
    )
    .ok()?;
    socket.write_all(head.as_bytes()).await.ok()?;
    socket.write_all(body.as_bytes()).await.ok()?;
    saved
}
