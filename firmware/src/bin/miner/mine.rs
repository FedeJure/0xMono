//! Una sesión con el pool: conectarse, recibir trabajos, probar nonces con
//! el acelerador SHA y mandar los shares.
//!
//! Todo en la tarea principal: se hashea de a tramos cortos y entre tramo y
//! tramo se atiende la red, los botones y la pantalla.

use companion::Gesture;
use embassy_futures::yield_now;
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpAddress, Stack};
use embassy_time::{Duration, Instant, Timer};
use embedded_io_async::Write as _;
use heapless::{String, Vec};
use log::{info, warn};
use miner::Config;
use miner::screen::{Mining, Screen};
use miner::stratum::{Assignment, Client, Event, SUGGESTED_DIFFICULTY};
use miner::work::{self, meets};

use crate::Buttons;
use crate::display::Panel;
use crate::hasher::Hasher;

/// Nonces por tramo: unos pocos milisegundos de hash entre vistazo y vistazo
/// a la red.
const BATCH: u32 = 4096;
/// Si el pool no manda nada en este tiempo, se da por cortado (los trabajos
/// nuevos llegan cada medio minuto o menos).
const SILENCE: Duration = Duration::from_secs(180);

pub enum End {
    /// Mantuvieron apretado C.
    Setup,
    WifiLost,
    PoolLost,
}

/// Lo que se acumula entre reconexiones.
pub struct Totals {
    pub start: Instant,
    pub accepted: u32,
    pub rejected: u32,
    pub best: f64,
    pub online: bool,
    hashes: u64,
    window: Instant,
    pub hashrate: f32,
}

impl Totals {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            accepted: 0,
            rejected: 0,
            best: 0.0,
            online: false,
            hashes: 0,
            window: Instant::now(),
            hashrate: 0.0,
        }
    }

    pub fn screen<'a>(&self, config: &'a Config) -> Mining<'a> {
        Mining {
            ssid: &config.ssid,
            pool: &config.pool_host,
            hashrate: self.hashrate,
            accepted: self.accepted,
            rejected: self.rejected,
            best: self.best,
            uptime_secs: self.start.elapsed().as_secs() as u32,
            online: self.online,
        }
    }
}

/// Espera `secs` segundos atendiendo los botones. `true` si pidieron
/// configurar.
pub async fn wait_or_setup(buttons: &mut Buttons, secs: u64) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    let mut last = Instant::now();
    while Instant::now() < end {
        Timer::after(Duration::from_millis(crate::TICK_MS)).await;
        let now = Instant::now();
        let dt = now.duration_since(last).as_millis() as u32;
        last = now;
        if buttons.poll_c(dt) == Some(Gesture::Hold) {
            return true;
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
pub async fn session(
    stack: Stack<'static>,
    config: &Config,
    hasher: &mut Hasher,
    panel: &mut Panel,
    buttons: &mut Buttons,
    totals: &mut Totals,
) -> End {
    let host = config.pool_host.as_str();
    let ip = match stack.dns_query(host, DnsQueryType::A).await {
        Ok(ips) => match ips.first() {
            Some(IpAddress::Ipv4(ip)) => *ip,
            _ => return End::PoolLost,
        },
        Err(e) => {
            warn!("no se encontró {host}: {e:?}");
            return End::PoolLost;
        }
    };
    info!("pool {host} = {ip}:{}", config.pool_port);

    let mut rx = [0u8; 4096];
    let mut tx = [0u8; 1024];
    let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
    socket.set_timeout(Some(SILENCE));
    socket.set_keep_alive(Some(Duration::from_secs(60)));
    if let Err(e) = socket.connect((ip, config.pool_port)).await {
        warn!("no se pudo conectar al pool: {e:?}");
        return End::PoolLost;
    }

    let mut client = Client::new(&config.address, &config.worker);
    let mut out: String<640> = String::new();
    let _ = client.hello(SUGGESTED_DIFFICULTY, &mut out);
    if socket.write_all(out.as_bytes()).await.is_err() {
        return End::PoolLost;
    }
    totals.online = true;
    panel.show(&Screen::Mining(totals.screen(config)));

    let mut line: Vec<u8, 4096> = Vec::new();
    let mut job: Option<Assignment> = None;
    let mut nonce = 0u32;
    let mut last = Instant::now();
    let mut last_screen = Instant::now();
    let mut last_log = Instant::now();
    let end = loop {
        // 1) Lo que mandó el pool.
        while socket.can_recv() {
            let mut buf = [0u8; 512];
            let n = match socket.read(&mut buf).await {
                Ok(0) | Err(_) => return lost(totals),
                Ok(n) => n,
            };
            for &b in &buf[..n] {
                if b != b'\n' {
                    if line.push(b).is_err() {
                        warn!("línea del pool demasiado larga");
                        line.clear();
                    }
                    continue;
                }
                let text = core::str::from_utf8(&line).unwrap_or("");
                match client.handle(text) {
                    Event::NewJob { clean } => {
                        if clean || job.is_none() {
                            job = client.next_work();
                            nonce = 0;
                        }
                    }
                    Event::Difficulty(d) => {
                        info!("dificultad del pool: {d}");
                        // Los shares tienen que cumplir la nueva.
                        if job.is_some() {
                            job = client.next_work();
                            nonce = 0;
                        }
                    }
                    Event::Accepted => {
                        totals.accepted += 1;
                        info!("share aceptado ({} en total)", totals.accepted);
                    }
                    Event::Rejected => {
                        totals.rejected += 1;
                        warn!("share rechazado: {text}");
                    }
                    Event::Authorized(ok) => info!("autorizado: {ok}"),
                    Event::Subscribed => info!("suscripto al pool"),
                    Event::Nothing => {}
                }
                line.clear();
            }
        }
        if !socket.may_recv() {
            break lost(totals);
        }

        // 2) Un tramo de nonces.
        if let Some(a) = &job {
            let mut shares: Vec<(u32, [u8; 32]), 4> = Vec::new();
            let scan = hasher.scan(&a.work, nonce, BATCH, |n, h| {
                let _ = shares.push((n, h));
            });
            client.note(&scan.best);
            for (n, h) in shares {
                if meets(&h, &a.work.block_target) {
                    info!("¡¡¡BLOQUE!!! nonce {n:08x}");
                }
                info!("share de dificultad {:.4}", work::difficulty(&h));
                out.clear();
                let _ = client.submit(a, n, &h, &mut out);
                if socket.write_all(out.as_bytes()).await.is_err() {
                    return lost(totals);
                }
            }
            totals.hashes += BATCH as u64;
            let (next, wrapped) = nonce.overflowing_add(BATCH);
            nonce = next;
            if wrapped {
                job = client.next_work();
            }
        } else {
            Timer::after(Duration::from_millis(20)).await;
        }
        totals.best = totals.best.max(client.stats.best);

        // 3) Botones, WiFi, pantalla.
        let now = Instant::now();
        let dt = now.duration_since(last).as_millis() as u32;
        last = now;
        if buttons.poll_c(dt) == Some(Gesture::Hold) {
            break End::Setup;
        }
        if !stack.is_link_up() {
            totals.online = false;
            break End::WifiLost;
        }
        if now.duration_since(last_screen) >= Duration::from_secs(1) {
            let secs = now.duration_since(totals.window).as_micros() as f32 / 1e6;
            totals.hashrate = totals.hashes as f32 / secs;
            totals.hashes = 0;
            totals.window = now;
            last_screen = now;
            panel.update_mining(&totals.screen(config));
        }
        if now.duration_since(last_log) >= Duration::from_secs(10) {
            last_log = now;
            info!(
                "{:.1} kH/s · shares {} ok {} mal · mejor {:.6}",
                totals.hashrate / 1000.0,
                totals.accepted,
                totals.rejected,
                totals.best
            );
        }
        yield_now().await;
    };
    socket.abort();
    let _ = socket.flush().await;
    end
}

fn lost(totals: &mut Totals) -> End {
    totals.online = false;
    End::PoolLost
}
