//! La hora por internet como la pide el 0xMono (crate `settings::sntp`), pero
//! desde la PC: pregunta a los mismos servidores, convierte a la hora local
//! con la zona elegida y la compara con el reloj de la PC.
//!
//! ```text
//! cargo run -p simulator --example ntp            # Argentina (UTC−3)
//! cargo run -p simulator --example ntp -- 60      # otra zona, en minutos
//! ```

use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use companion::LocalTime;
use settings::date::{Clock, DayLabel};
use settings::sntp;
use settings::tz::Offset;

fn main() {
    let offset: i16 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(settings::DEFAULT_UTC_OFFSET_MIN);
    let socket = UdpSocket::bind("0.0.0.0:0").expect("no se pudo abrir el puerto UDP");
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    for server in sntp::SERVERS {
        let Ok(addrs) = (*server, sntp::PORT).to_socket_addrs() else {
            println!("{server}: no resuelve");
            continue;
        };
        for addr in addrs.filter(|a| a.is_ipv4()) {
            let sent = SystemTime::now();
            if socket.send_to(&sntp::request(), addr).is_err() {
                continue;
            }
            let mut buf = [0u8; 64];
            let Ok((n, _)) = socket.recv_from(&mut buf) else {
                println!("{server} ({addr}): no contestó");
                continue;
            };
            let rtt = sent.elapsed().unwrap_or_default();
            match sntp::parse_reply(&buf[..n]) {
                Ok(unix) => {
                    let pc = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let t = LocalTime::from_unix(unix, offset);
                    println!(
                        "{server} ({addr}) en {} ms: {} {} en {} (día {} desde 1970, {:?})",
                        rtt.as_millis(),
                        DayLabel(t),
                        Clock(t),
                        Offset(offset),
                        t.day,
                        t.weekday(),
                    );
                    println!(
                        "  diferencia con el reloj de la PC: {} s",
                        unix as i64 - pc as i64
                    );
                }
                Err(e) => println!("{server}: respuesta inválida ({e:?})"),
            }
            break;
        }
    }
}
