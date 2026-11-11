//! El hash de Bitcoin con el acelerador SHA-256 del ESP32-C3, escribiendo
//! directo en sus registros (el driver de esp-hal es genérico y mucho más
//! lento para esto).
//!
//! Por nonce son dos compresiones en hardware:
//! 1. El segundo bloque de la cabecera, continuando desde el midstate.
//! 2. El SHA-256 del resultado (un bloque, desde el estado inicial).
//!
//! La memoria del acelerador guarda las palabras en el orden de bytes del
//! mensaje: escribir `u32::from_le_bytes(bytes)` es poner esos bytes tal cual.
//! Por eso el resultado del primer paso (H) se copia directo como mensaje del
//! segundo.

use core::ptr::{read_volatile, write_volatile};

use esp_hal::peripherals::SHA;
use esp_hal::sha::Sha;
use miner::work::{Work, meets};

const BASE: usize = 0x6003_B000;
const MODE: *mut u32 = BASE as *mut u32;
const START: *mut u32 = (BASE + 0x10) as *mut u32;
const CONTINUE: *mut u32 = (BASE + 0x14) as *mut u32;
const BUSY: *const u32 = (BASE + 0x18) as *const u32;
const H_MEM: *mut u32 = (BASE + 0x40) as *mut u32;
const M_MEM: *mut u32 = (BASE + 0x80) as *mut u32;

const MODE_SHA256: u32 = 2;
/// Relleno: el bit 1 después del mensaje, y el largo en bits al final.
const PAD_80: u32 = 0x0000_0080;
const LEN_640_BITS: u32 = u32::from_le_bytes(640u32.to_be_bytes());
const LEN_256_BITS: u32 = u32::from_le_bytes(256u32.to_be_bytes());

pub struct Hasher {
    /// Se queda con el periférico: así tiene el reloj prendido y nadie más
    /// lo usa.
    _sha: Sha<'static>,
    /// Si el acelerador no dio lo mismo que el software al arrancar, se mina
    /// en software (lento, pero sin mandarle basura al pool).
    software: bool,
}

/// Lo que salió de un tramo de nonces.
pub struct Scan {
    /// El hash más bajo del tramo (para "el mejor tiro").
    pub best: [u8; 32],
}

#[inline(always)]
fn w(reg: *mut u32, i: usize, v: u32) {
    // SAFETY: direcciones del acelerador SHA del ESP32-C3 (TRM, cap. SHA),
    // que este módulo usa en exclusiva.
    unsafe { write_volatile(reg.add(i), v) }
}

#[inline(always)]
fn r(reg: *const u32, i: usize) -> u32 {
    // SAFETY: ver `w`.
    unsafe { read_volatile(reg.add(i)) }
}

#[inline(always)]
fn wait() {
    while r(BUSY, 0) != 0 {}
}

impl Hasher {
    pub fn new(sha: SHA<'static>) -> Self {
        let sha = Sha::new(sha);
        w(MODE, 0, MODE_SHA256);
        // Palabras 9..15 en cero: las dos compresiones las necesitan así y
        // ninguna las cambia (la 15 se reescribe en cada una).
        for i in 0..16 {
            w(M_MEM, i, 0);
        }
        let mut hasher = Self {
            _sha: sha,
            software: false,
        };
        hasher.software = !hasher.self_test();
        hasher
    }

    /// Compara el acelerador con el software en algunos nonces.
    fn self_test(&mut self) -> bool {
        let mut header = [0u8; 80];
        for (i, b) in header.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(37).wrapping_add(11);
        }
        let work = Work::from_header(&header, [0xFF; 32]);
        [0, 1, 0x1234_5678, u32::MAX]
            .iter()
            .all(|&n| self.hash(&work, n) == work.hash(n))
    }

    pub fn is_hardware(&self) -> bool {
        !self.software
    }

    /// Hash de un nonce, como `Work::hash` pero en hardware.
    pub fn hash(&mut self, work: &Work, nonce: u32) -> [u8; 32] {
        self.run(work, &tail_words(work), nonce);
        let mut out = [0u8; 32];
        for (i, chunk) in out.chunks_exact_mut(4).enumerate() {
            chunk.copy_from_slice(&r(H_MEM, i).to_le_bytes());
        }
        out
    }

    #[inline(always)]
    fn run(&mut self, work: &Work, tail: &[u32; 3], nonce: u32) {
        // 1) Segundo bloque de la cabecera desde el midstate.
        for (i, h) in work.midstate.iter().enumerate() {
            w(H_MEM, i, h.swap_bytes());
        }
        w(M_MEM, 0, tail[0]);
        w(M_MEM, 1, tail[1]);
        w(M_MEM, 2, tail[2]);
        w(M_MEM, 3, nonce);
        w(M_MEM, 4, PAD_80);
        w(M_MEM, 5, 0);
        w(M_MEM, 6, 0);
        w(M_MEM, 7, 0);
        w(M_MEM, 8, 0);
        w(M_MEM, 15, LEN_640_BITS);
        w(CONTINUE, 0, 1);
        wait();
        // 2) SHA-256 de esos 32 bytes.
        for i in 0..8 {
            w(M_MEM, i, r(H_MEM, i));
        }
        w(M_MEM, 8, PAD_80);
        w(M_MEM, 15, LEN_256_BITS);
        w(START, 0, 1);
        wait();
    }

    /// Prueba `count` nonces desde `start`; llama a `found` con cada uno que
    /// cumple el objetivo de share.
    pub fn scan(
        &mut self,
        work: &Work,
        start: u32,
        count: u32,
        mut found: impl FnMut(u32, [u8; 32]),
    ) -> Scan {
        let tail = tail_words(work);
        // Los 4 bytes más altos del objetivo: filtro rápido antes de leer todo.
        let target_top = u32::from_le_bytes(work.share_target[28..].try_into().unwrap());
        let mut best_top = u32::MAX;
        let mut best = [0xFF; 32];
        for i in 0..count {
            let nonce = start.wrapping_add(i);
            let soft = self.software.then(|| work.hash(nonce));
            let top = match soft {
                Some(h) => u32::from_le_bytes(h[28..].try_into().unwrap()),
                None => {
                    self.run(work, &tail, nonce);
                    r(H_MEM, 7)
                }
            };
            if top <= target_top || top < best_top {
                let hash = soft.unwrap_or_else(|| {
                    let mut hash = [0u8; 32];
                    for (j, chunk) in hash.chunks_exact_mut(4).enumerate() {
                        chunk.copy_from_slice(&r(H_MEM, j).to_le_bytes());
                    }
                    hash
                });
                if top < best_top || (top == best_top && !meets(&best, &hash)) {
                    best_top = top;
                    best = hash;
                }
                if meets(&hash, &work.share_target) {
                    found(nonce, hash);
                }
            }
        }
        Scan { best }
    }
}

fn tail_words(work: &Work) -> [u32; 3] {
    let t = &work.tail;
    [
        u32::from_le_bytes([t[0], t[1], t[2], t[3]]),
        u32::from_le_bytes([t[4], t[5], t[6], t[7]]),
        u32::from_le_bytes([t[8], t[9], t[10], t[11]]),
    ]
}
