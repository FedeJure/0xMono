//! Lo que comparten el modo minero y la configuración del aparato cuando el
//! mono arma su propia red: el DHCP que le dice al celular que el DNS es el
//! mono, el DNS "cautivo" que contesta a todo con su IP, cómo buscar redes y
//! cómo conectarse a una (y explicar por qué no se pudo).

use core::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use captive::{AP_IP, Failure, dns};
use edge_nal_embassy::UdpBuffers;
use embassy_net::Stack;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_time::{Duration, Timer, with_timeout};
use esp_hal::rng::Rng;
use esp_radio::wifi::scan::{ScanConfig, ScanTypeConfig};
use esp_radio::wifi::{ConnectionError, DisconnectReason, WifiController};
use heapless::String;
use log::warn;

/// Semilla para la pila de red (puertos y números de secuencia TCP).
pub fn seed(rng: &Rng) -> u64 {
    (rng.random() as u64) << 32 | rng.random() as u64
}

/// Clave de 8 letras y números sin los que se confunden. Con la radio
/// prendida el generador de números al azar es de verdad.
pub fn random_pass(rng: &Rng) -> String<8> {
    captive::random_pass(|| rng.random())
}

/// Escucha cada canal hasta 200 ms: con los 20 ms de fábrica se pierde la
/// mayoría de las redes.
pub fn scan_config() -> ScanConfig {
    ScanConfig::default()
        .with_max(32)
        .with_scan_type(ScanTypeConfig::Active {
            min: esp_hal::time::Duration::from_millis(60),
            max: esp_hal::time::Duration::from_millis(200),
        })
}

pub struct DhcpBuffers {
    udp: UdpBuffers<2, 1024, 1024, 4>,
    packet: [u8; 1500],
}

impl DhcpBuffers {
    pub fn new() -> Self {
        Self {
            udp: UdpBuffers::new(),
            packet: [0; 1500],
        }
    }
}

/// Servidor DHCP de la red del mono: el DNS que reparte es el mono, así
/// cualquier nombre lleva a la página.
pub async fn dhcp_server(stack: Stack<'_>, bufs: &mut DhcpBuffers) -> ! {
    use edge_dhcp::io::{self, DEFAULT_SERVER_PORT};
    use edge_dhcp::server::{Server, ServerOptions};
    use edge_nal::UdpBind;
    use edge_nal_embassy::Udp;

    let ip = Ipv4Addr::from(AP_IP);
    let mut gateway = [Ipv4Addr::UNSPECIFIED];
    let dns = [ip];
    let mut options = ServerOptions::new(ip, Some(&mut gateway));
    options.dns = &dns;
    options.lease_duration_secs = 600;

    let udp = Udp::new(stack, &bufs.udp);
    let mut socket = udp
        .bind(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            DEFAULT_SERVER_PORT,
        )))
        .await
        .expect("no se pudo abrir el DHCP");
    let mut server = Server::<_, 8>::new_with_et(ip);
    loop {
        if let Err(e) = io::server::run(&mut server, &options, &mut socket, &mut bufs.packet).await
        {
            warn!("DHCP: {e:?}");
        }
        Timer::after(Duration::from_millis(500)).await;
    }
}

pub struct DnsBuffers {
    rx_meta: [PacketMetadata; 4],
    tx_meta: [PacketMetadata; 4],
    rx: [u8; 1024],
    tx: [u8; 1024],
    query: [u8; 512],
    answer: [u8; 512],
}

impl DnsBuffers {
    pub fn new() -> Self {
        Self {
            rx_meta: [PacketMetadata::EMPTY; 4],
            tx_meta: [PacketMetadata::EMPTY; 4],
            rx: [0; 1024],
            tx: [0; 1024],
            query: [0; 512],
            answer: [0; 512],
        }
    }
}

/// DNS "cautivo": a cualquier nombre le contesta con la IP del mono.
pub async fn dns_server(stack: Stack<'_>, b: &mut DnsBuffers) -> ! {
    let mut socket = UdpSocket::new(stack, &mut b.rx_meta, &mut b.rx, &mut b.tx_meta, &mut b.tx);
    socket.bind(53).expect("no se pudo abrir el DNS");
    loop {
        let Ok((n, from)) = socket.recv_from(&mut b.query).await else {
            continue;
        };
        if let Some(len) = dns::reply(&b.query[..n], AP_IP, &mut b.answer) {
            let _ = socket.send_to(&b.answer[..len], from).await;
        }
    }
}

/// Se asocia a la red y espera la IP, con reintentos.
pub async fn connect(
    controller: &mut WifiController<'static>,
    stack: Stack<'static>,
    attempts: u32,
) -> Result<(), Failure> {
    let mut failure = Failure::Other;
    for attempt in 1..=attempts {
        match with_timeout(Duration::from_secs(30), controller.connect_async()).await {
            Ok(Ok(_)) => {
                return match with_timeout(Duration::from_secs(20), stack.wait_config_up()).await {
                    Ok(()) => Ok(()),
                    Err(_) => Err(Failure::NoIp),
                };
            }
            Ok(Err(e)) => {
                warn!("intento {attempt}: {e:?}");
                failure = classify(e);
                // Con la clave mal no tiene sentido insistir.
                if failure == Failure::WrongPassword {
                    break;
                }
            }
            Err(_) => warn!("intento {attempt}: no contestó"),
        }
        Timer::after(Duration::from_secs(2)).await;
    }
    Err(failure)
}

fn classify(e: ConnectionError) -> Failure {
    let ConnectionError::Failed(info) = e else {
        return Failure::Other;
    };
    match info.reason {
        DisconnectReason::FourWayHandshakeTimeout
        | DisconnectReason::HandshakeTimeout
        | DisconnectReason::MicFailure
        | DisconnectReason::AuthenticationFailed
        | DisconnectReason::InvalidPmkid => Failure::WrongPassword,
        DisconnectReason::NoAccessPointFound
        | DisconnectReason::NoAccessPointFoundInRssiThreshold
        | DisconnectReason::BeaconTimeout => Failure::NotFound,
        DisconnectReason::NoAccessPointFoundWithCompatibleSecurity
        | DisconnectReason::NoAccessPointFoundInAuthmodeThreshold => Failure::Security,
        _ => Failure::Other,
    }
}
