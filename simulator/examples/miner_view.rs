//! Visor en vivo de la pantalla de la placa: lee por el USB las líneas
//! `@pantalla` que manda el firmware `miner` y las dibuja con el mismo código
//! que la ST7789 (crate `miner`). El resto del registro se muestra en la
//! consola.
//!
//! cargo run -p simulator --example miner_view [COM3]

use std::convert::Infallible;
use std::io::{ErrorKind, Read};
use std::sync::mpsc;
use std::time::Duration;

use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use miner::mirror;
use miner::screen::{self, SIZE, Screen};
use minifb::{Key, Scale, Window, WindowOptions};

struct Framebuffer(Vec<u32>);

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
                // De 5/6/5 bits a 8, como se ve en la pantalla.
                let (r, g, b) = (
                    (c.r() << 3 | c.r() >> 2) as u32,
                    (c.g() << 2 | c.g() >> 4) as u32,
                    (c.b() << 3 | c.b() >> 2) as u32,
                );
                self.0[(p.y as u32 * SIZE + p.x as u32) as usize] = r << 16 | g << 8 | b;
            }
        }
        Ok(())
    }
}

/// Saca los códigos de color ANSI del registro.
fn plain(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port_name = args.get(1).map_or("COM3", String::as_str);
    let mut port = match serialport::new(port_name, 115_200)
        .timeout(Duration::from_millis(200))
        .open()
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("no se pudo abrir {port_name}: {e}");
            eprintln!(
                "¿Está enchufada la placa? ¿Otro programa (espflash, un monitor) tiene el puerto?"
            );
            return;
        }
    };
    // Sin tocar estas líneas no se reinicia la placa al abrir el puerto.
    let _ = port.write_data_terminal_ready(false);
    let _ = port.write_request_to_send(false);
    println!("escuchando {port_name}...");

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            match port.read(&mut buf) {
                Ok(n) => pending.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::TimedOut => continue,
                Err(e) => {
                    eprintln!("se cortó el puerto: {e}");
                    return;
                }
            }
            while let Some(pos) = pending.iter().position(|&b| b == b'\n') {
                let raw: Vec<u8> = pending.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&raw).trim_end().to_string();
                if line.starts_with(mirror::PREFIX) {
                    if tx.send(line).is_err() {
                        return;
                    }
                } else if !line.is_empty() {
                    println!("{}", plain(&line));
                }
            }
        }
    });

    let mut window = Window::new(
        "Pantalla del mono (en vivo) - Esc para salir",
        SIZE as usize,
        SIZE as usize,
        WindowOptions {
            scale: Scale::X2,
            ..WindowOptions::default()
        },
    )
    .expect("no se pudo abrir la ventana");
    window.set_target_fps(30);

    let mut fb = Framebuffer(vec![0; (SIZE * SIZE) as usize]);
    screen::draw(
        &mut fb,
        &Screen::Notice {
            title: "Esperando a la placa",
            text: "La pantalla aparece apenas la placa dibuje algo (minando, cada segundo).",
        },
    )
    .unwrap();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Solo importa la última.
        if let Some(line) = rx.try_iter().last()
            && let Some(s) = mirror::parse(&line)
        {
            screen::draw(&mut fb, &s).unwrap();
        }
        window
            .update_with_buffer(&fb.0, SIZE as usize, SIZE as usize)
            .unwrap();
    }
}
