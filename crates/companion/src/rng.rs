//! Azar determinista: la misma semilla da siempre la misma secuencia. Así la
//! agenda de un día es reproducible sin tener que guardarla.

/// Mezcla dos valores en uno bien distribuido (finalizador de murmur3).
pub fn mix(a: u32, b: u32) -> u32 {
    let mut h = a ^ b.wrapping_mul(0x9E37_79B9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 16)
}

/// xorshift32: rápido en un RISC-V de 32 bits y suficiente para esto.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        // xorshift no puede arrancar en 0.
        Self(mix(seed, 0x6A09_E667) | 1)
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Entero en `lo..=hi`.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        debug_assert!(lo <= hi);
        lo + (self.next_u32() % (hi - lo + 1) as u32) as i32
    }

    /// `true` con probabilidad `pct`%.
    pub fn chance(&mut self, pct: u32) -> bool {
        self.next_u32() % 100 < pct
    }
}
