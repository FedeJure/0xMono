//! El modo minero en la PC: la página de configuración y las pantallas, con el
//! mismo código que corre en el ESP32 (crate `miner`).
//!
//! cargo run -p simulator --example miner               # página en http://127.0.0.1:8080
//! cargo run -p simulator --example miner -- --png dir  # cada pantalla como PNG

use std::convert::Infallible;
use std::io::{Read, Write};
use std::net::TcpListener;

use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use heapless::Vec as HVec;
use miner::portal::{self, AP_HOST, BufWriter, Network};
use miner::screen::{self, Mining, SIZE, Screen};
use miner::{Config, http};

const SAVE_PATH: &str = "miner.bin";
const SCALE: u32 = 2;

struct Framebuffer(Vec<Rgb565>);

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

fn save_png(path: &std::path::Path, fb: &Framebuffer) {
    let side = SIZE * SCALE;
    let mut data = Vec::with_capacity((side * side * 3) as usize);
    for y in 0..side {
        for x in 0..side {
            let c = fb.0[((y / SCALE) * SIZE + x / SCALE) as usize];
            // De 5/6/5 bits a 8, como se ve en la pantalla.
            data.extend([
                c.r() << 3 | c.r() >> 2,
                c.g() << 2 | c.g() >> 4,
                c.b() << 3 | c.b() >> 2,
            ]);
        }
    }
    let file = std::fs::File::create(path).expect("no se pudo crear el PNG");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), side, side);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&data).unwrap();
}

fn export_screens(dir: &str) {
    std::fs::create_dir_all(dir).expect("no se pudo crear la carpeta");
    let screens = [
        (
            "1_unirse",
            Screen::Join {
                ssid: "Mono-4F2A",
                pass: "kd7wq3mz",
            },
        ),
        ("2_abrir", Screen::Open),
        (
            "3_guardado",
            Screen::Saved {
                ssid: "WiFi de casa",
            },
        ),
        (
            "4_conectando",
            Screen::Connecting {
                ssid: "WiFi de casa",
            },
        ),
        (
            "5_conectado",
            Screen::Online {
                ssid: "WiFi de casa",
                ip: [192, 168, 0, 23],
            },
        ),
        (
            "6_fallo",
            Screen::Failed {
                ssid: "WiFi de casa",
                reason: "La clave del WiFi no es correcta.",
            },
        ),
        ("7_minando", Screen::Mining(MINING)),
        (
            "8_sin_pool",
            Screen::Notice {
                title: "Sin pool",
                text: "No llego al pool. Reintento en 15 segundos.",
            },
        ),
    ];
    for (name, s) in screens {
        let mut fb = Framebuffer(vec![Rgb565::BLACK; (SIZE * SIZE) as usize]);
        screen::draw(&mut fb, &s).unwrap();
        let path = std::path::Path::new(dir).join(format!("{name}.png"));
        save_png(&path, &fb);
        println!("{}", path.display());
    }
}

const MINING: Mining = Mining {
    ssid: "WiFi de casa",
    pool: "public-pool.io",
    hashrate: 228_400.0,
    accepted: 2,
    rejected: 0,
    best: 0.003_17,
    uptime_secs: 3 * 3600 + 27 * 60,
    online: true,
};

fn fake_networks() -> HVec<Network, 16> {
    let mut list = HVec::new();
    for (ssid, rssi, open) in [
        ("WiFi de casa", -48, false),
        ("Fibertel WiFi482 2.4GHz", -71, false),
        ("WiFi de casa", -60, false),
        ("Personal-WiFi-7A1", -80, false),
        ("Café Libre", -66, true),
        ("", -55, false),
    ] {
        portal::add_network(&mut list, ssid, rssi, open);
    }
    list
}

fn serve() {
    let listener = TcpListener::bind("127.0.0.1:8080").expect("no se pudo abrir el puerto 8080");
    println!("Página de configuración en http://127.0.0.1:8080 (Ctrl+C para salir)");
    let networks = fake_networks();
    let mut current = std::fs::read(SAVE_PATH)
        .ok()
        .and_then(|b| Config::decode(&b));

    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut buf = vec![0u8; 4096];
        let mut len = 0;
        // Cabecera completa y, si es un POST, el cuerpo entero.
        let req_ok = loop {
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
        if !req_ok {
            continue;
        }
        let mut req = http::parse(&buf[..len]).unwrap();
        // En la PC el navegador pide 127.0.0.1: que cuente como el mono.
        req.host = AP_HOST;
        println!("{:?} {}", req.method, req.path);

        let body_in = &buf[req.body_start..req.body_start + req.content_length];
        let mut head_buf = [0u8; 256];
        let mut body_buf = vec![0u8; 12 * 1024];
        let mut head = BufWriter::new(&mut head_buf);
        let mut body = BufWriter::new(&mut body_buf);
        let saved = portal::respond(
            &req,
            body_in,
            &networks,
            current.as_ref(),
            None,
            &mut head,
            &mut body,
        )
        .expect("la página no entra en el buffer");
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(body.as_bytes());

        if let Some(config) = saved {
            let mut out = [0u8; miner::config::MAX_ENCODED];
            let n = config.encode(&mut out);
            std::fs::write(SAVE_PATH, &out[..n]).expect("no se pudo guardar");
            println!(
                "guardado en {SAVE_PATH}: red «{}», clave de {} caracteres, dirección {}, pool {}:{}, nombre {}",
                config.ssid,
                config.pass.chars().count(),
                config.address,
                config.pool_host,
                config.pool_port,
                config.worker
            );
            current = Some(config);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.iter().position(|a| a == "--png") {
        Some(i) => export_screens(args.get(i + 1).map_or("miner_png", String::as_str)),
        None => serve(),
    }
}
