//! Prueba del cliente Stratum contra un pool de verdad, minando en la PC con
//! el mismo código que usa el mono (crate `miner`). Si el pool acepta los
//! shares, la cabecera, la raíz de Merkle y los nonces se arman bien.
//!
//! cargo run -p simulator --release --example stratum -- <dirección> [segundos] [dificultad] [pool:puerto]

use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use heapless::String as HString;
use miner::config::{DEFAULT_POOL_HOST, DEFAULT_POOL_PORT};
use miner::stratum::{Client, Event};
use miner::work::meets;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(address) = args.get(1) else {
        eprintln!("uso: stratum <dirección> [segundos] [dificultad] [pool:puerto]");
        return;
    };
    let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);
    let diff: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.01);
    let default_pool = format!("{DEFAULT_POOL_HOST}:{DEFAULT_POOL_PORT}");
    let pool = args.get(4).unwrap_or(&default_pool);

    println!("conectando a {pool}...");
    let mut stream = TcpStream::connect(pool).expect("no se pudo conectar al pool");
    stream.set_nonblocking(true).unwrap();
    let mut client = Client::new(address, "prueba-pc");
    let mut hello: HString<512> = HString::new();
    client.hello(diff, &mut hello).unwrap();
    write_all(&mut stream, hello.as_bytes());

    let start = Instant::now();
    let mut pending = Vec::new();
    let mut work = None;
    let mut nonce = 0u32;
    let mut hashes = 0u64;
    let mut last_report = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        // Lo que haya mandado el pool.
        let mut buf = [0u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => {
                    println!("el pool cerró la conexión");
                    return;
                }
                Ok(n) => pending.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => panic!("{e}"),
            }
        }
        while let Some(pos) = pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = pending.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim();
            let ev = client.handle(line);
            match ev {
                Event::Nothing => println!("  pool: {line}"),
                Event::Rejected => println!("RECHAZADO: {line}"),
                Event::NewJob { clean } => {
                    println!(
                        "trabajo nuevo (limpio: {clean}), dificultad {}",
                        client.difficulty()
                    );
                    if clean || work.is_none() {
                        work = client.next_work();
                        nonce = 0;
                    }
                }
                ev => println!("{ev:?}"),
            }
        }

        let Some(a) = work.as_ref() else {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        };
        // Un tanto de nonces repartidos entre todos los núcleos y volver a
        // mirar la red.
        const PER_THREAD: u32 = 1 << 21;
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()) as u32;
        let base = nonce;
        let found: Vec<(u32, [u8; 32])> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let w = a.work;
                    s.spawn(move || {
                        let from = base.wrapping_add(t * PER_THREAD);
                        let mut found = Vec::new();
                        let mut best = [0xFF; 32];
                        for i in 0..PER_THREAD {
                            let n = from.wrapping_add(i);
                            let h = w.hash(n);
                            if top(&h) < top(&best) {
                                best = h;
                            }
                            if meets(&h, &w.share_target) {
                                found.push((n, h));
                            }
                        }
                        (found, best)
                    })
                })
                .collect();
            let mut all = Vec::new();
            for h in handles {
                let (found, best) = h.join().unwrap();
                client.note(&best);
                all.extend(found);
            }
            all
        });
        for (n, h) in found {
            let mut line: HString<512> = HString::new();
            client.submit(a, n, &h, &mut line).unwrap();
            print!(
                "share! dificultad {:.4} → {line}",
                miner::work::difficulty(&h)
            );
            write_all(&mut stream, line.as_bytes());
        }
        hashes += (threads * PER_THREAD) as u64;
        nonce = base.wrapping_add(threads * PER_THREAD);
        if nonce < base {
            work = client.next_work();
        }
        if last_report.elapsed() > Duration::from_secs(10) {
            println!(
                "{:.2} MH/s, aceptados {}, rechazados {}, mejor {:.4}",
                hashes as f64 / start.elapsed().as_secs_f64() / 1e6,
                client.stats.accepted,
                client.stats.rejected,
                client.stats.best
            );
            last_report = Instant::now();
        }
    }
    println!(
        "fin: aceptados {}, rechazados {}, mejor dificultad {:.4}",
        client.stats.accepted, client.stats.rejected, client.stats.best
    );
}

/// Los 8 bytes más significativos del hash: más chico, mejor.
fn top(h: &[u8; 32]) -> u64 {
    u64::from_le_bytes(h[24..].try_into().unwrap())
}

fn write_all(stream: &mut TcpStream, mut data: &[u8]) {
    while !data.is_empty() {
        match stream.write(data) {
            Ok(n) => data = &data[n..],
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(e) => panic!("{e}"),
        }
    }
}
