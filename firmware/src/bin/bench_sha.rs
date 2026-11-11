#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

//! Hash de Bitcoin en el ESP32-C3: verifica el acelerador SHA contra el
//! software (con el bloque génesis) y mide cuántos hashes por segundo hace
//! cada uno. Sin WiFi ni pantalla.
//!
//! cd firmware && cargo run --release --bin bench_sha

#[path = "miner/hasher.rs"]
mod hasher;

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::Instant;
use esp_println::println;
use miner::work::{Work, target_from_difficulty};

esp_bootloader_esp_idf::esp_app_desc!();

const GENESIS_NONCE: u32 = 2_083_236_893;

fn genesis() -> [u8; 80] {
    let mut h = [0u8; 80];
    h[..4].copy_from_slice(&1u32.to_le_bytes());
    let merkle = *b"\x3b\xa3\xed\xfd\x7a\x7b\x12\xb2\x7a\xc7\x2c\x3e\x67\x76\x8f\x61\
\x7f\xc8\x1b\xc3\x88\x8a\x51\x32\x3a\x9f\xb8\xaa\x4b\x1e\x5e\x4a";
    h[36..68].copy_from_slice(&merkle);
    h[68..72].copy_from_slice(&1_231_006_505u32.to_le_bytes());
    h[72..76].copy_from_slice(&0x1D00_FFFFu32.to_le_bytes());
    h
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let mut hw = hasher::Hasher::new(peripherals.SHA);
    let work = Work::from_header(&genesis(), target_from_difficulty(1.0));

    println!("\n=== Hash de Bitcoin · ESP32-C3 ===");
    println!("autoverificación del acelerador: {}", hw.is_hardware());
    let mut ok = true;
    for nonce in [0, 1, 0xFFFF_FFFF, GENESIS_NONCE, GENESIS_NONCE - 1] {
        let (a, b) = (hw.hash(&work, nonce), work.hash(nonce));
        if a != b {
            println!("DISTINTO en nonce {nonce}: hw {a:02x?} sw {b:02x?}");
            ok = false;
        }
    }
    let mut found = 0;
    hw.scan(&work, GENESIS_NONCE - 500, 1000, |n, _| {
        found += 1;
        if n != GENESIS_NONCE {
            ok = false;
        }
    });
    println!(
        "verificación: {} (nonce del génesis encontrado {found} vez)",
        if ok && found == 1 { "OK" } else { "FALLA" }
    );

    loop {
        let n = 2_000;
        let t = Instant::now();
        let mut x = 0u8;
        for nonce in 0..n {
            x ^= work.hash(nonce)[31];
        }
        let sw = n as u64 * 1_000_000 / t.elapsed().as_micros().max(1);

        let n = 200_000;
        let t = Instant::now();
        let scan = hw.scan(&work, 0, n, |_, _| {});
        let hw_rate = n as u64 * 1_000_000 / t.elapsed().as_micros().max(1);
        println!(
            "software: {:.1} kH/s · hardware: {:.1} kH/s · ({x}{})",
            sw as f32 / 1000.0,
            hw_rate as f32 / 1000.0,
            scan.best[31]
        );
    }
}
