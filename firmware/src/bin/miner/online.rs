//! Modo en línea: se conecta al WiFi guardado y mina ([`crate::mine`]). Si no
//! puede conectarse, explica por qué y vuelve al modo configuración; mantener
//! C también lleva ahí.

use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_time::{Duration, Timer};
use esp_hal::peripherals::{SHA, WIFI};
use esp_hal::rng::Rng;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config as WifiConfig, ControllerConfig, Interface, WifiController,
};
use log::{info, warn};
use miner::Config;
use miner::screen::Screen;
use static_cell::StaticCell;

use crate::captive_net::{self, seed};
use crate::display::Panel;
use crate::hasher::Hasher;
use crate::mine::{self, End, Totals};
use crate::{Buttons, handoff, net_task};

const ATTEMPTS: u32 = 3;

static RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();

pub async fn run(
    spawner: Spawner,
    wifi: WIFI<'static>,
    sha: SHA<'static>,
    config: Config,
    mut panel: Panel,
    mut buttons: Buttons,
    rng: Rng,
) -> ! {
    let ssid = config.ssid.as_str();
    let auth = if config.pass.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        AuthenticationMethodConfig::Wpa2Personal(config.pass.as_str().try_into().unwrap())
    };
    let station = StationConfig::default()
        .with_ssid(ssid.try_into().unwrap())
        .with_authentication(auth);
    let sta = Interface::station();
    let mut controller = WifiController::new(
        wifi,
        ControllerConfig::default().with_initial_config(WifiConfig::Station(station)),
    )
    .expect("no arrancó el WiFi");

    let (stack, runner) = embassy_net::new(
        sta,
        embassy_net::Config::dhcpv4(Default::default()),
        RESOURCES.uninit().write(StackResources::new()),
        seed(&rng),
    );
    spawner.spawn(net_task(runner).unwrap());
    let mut hasher = Hasher::new(sha);
    if hasher.is_hardware() {
        info!("acelerador SHA verificado");
    } else {
        warn!("el acelerador SHA no coincide con el software: se mina en software");
    }
    let mut totals = Totals::new();

    loop {
        panel.show(&Screen::Connecting { ssid });
        if let Err(failure) = captive_net::connect(&mut controller, stack, ATTEMPTS).await {
            warn!("no se pudo conectar: {failure:?}");
            panel.show(&Screen::Failed {
                ssid,
                reason: failure.message(),
            });
            Timer::after(Duration::from_secs(4)).await;
            handoff::reboot_into_setup(Some(failure));
        }
        let ip = stack
            .config_v4()
            .map_or([0; 4], |c| c.address.address().octets());
        info!("conectado a «{ssid}» con IP {ip:?}");
        panel.show(&Screen::Online { ssid, ip });

        // Minar hasta que se corte el WiFi (y reconectar) o pidan configurar.
        loop {
            let end = mine::session(
                stack,
                &config,
                &mut hasher,
                &mut panel,
                &mut buttons,
                &mut totals,
            )
            .await;
            match end {
                End::Setup => handoff::reboot_into_setup(None),
                End::WifiLost => {
                    warn!("se cortó el WiFi");
                    break;
                }
                End::PoolLost => {
                    warn!("sin conexión con el pool");
                    panel.show(&Screen::Notice {
                        title: "Sin pool",
                        text: "No llego al pool. Reintento en 15 segundos.",
                    });
                    if mine::wait_or_setup(&mut buttons, 15).await {
                        handoff::reboot_into_setup(None);
                    }
                    if !stack.is_link_up() {
                        break;
                    }
                }
            }
        }
    }
}
