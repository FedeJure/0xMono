//! Lo que hay que calcular para minar: armar la cabecera del bloque, el
//! "midstate" y comparar hashes contra los objetivos.
//!
//! La cabecera tiene 80 bytes. SHA-256 la procesa en dos bloques de 64: el
//! primero no depende del nonce, así que se calcula una vez por trabajo (el
//! midstate). Por cada nonce quedan dos compresiones: el segundo bloque y el
//! segundo SHA-256 (Bitcoin usa SHA-256 doble).

use sha2::digest::generic_array::GenericArray;
use sha2::{Digest, Sha256};

/// Estado inicial de SHA-256.
pub const H0: [u32; 8] = [
    0x6A09_E667,
    0xBB67_AE85,
    0x3C6E_F372,
    0xA54F_F53A,
    0x510E_527F,
    0x9B05_688C,
    0x1F83_D9AB,
    0x5BE0_CD19,
];

pub fn sha256d(data: &[u8]) -> [u8; 32] {
    Sha256::digest(Sha256::digest(data)).into()
}

/// Comprime un bloque de 64 bytes sobre `state`.
pub fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    sha2::compress256(state, &[*GenericArray::from_slice(block)]);
}

/// Un trabajo listo para probar nonces: lo que el hash necesita y nada más.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Work {
    /// SHA-256 de los primeros 64 bytes de la cabecera.
    pub midstate: [u32; 8],
    /// Bytes 64..76 de la cabecera: final de la raíz de Merkle, hora y bits.
    pub tail: [u8; 12],
    /// Un hash (como número little-endian) gana un share si es <= esto.
    pub share_target: [u8; 32],
    /// ...y un bloque entero si es <= esto.
    pub block_target: [u8; 32],
}

impl Work {
    pub fn from_header(header: &[u8; 80], share_target: [u8; 32]) -> Work {
        let mut midstate = H0;
        compress(&mut midstate, header[..64].try_into().unwrap());
        let nbits = u32::from_le_bytes(header[72..76].try_into().unwrap());
        Work {
            midstate,
            tail: header[64..76].try_into().unwrap(),
            share_target,
            block_target: target_from_nbits(nbits),
        }
    }

    /// El segundo bloque de la cabecera, con el relleno de SHA-256 (80 bytes
    /// = 640 bits).
    pub fn second_block(&self, nonce: u32) -> [u8; 64] {
        let mut b = [0u8; 64];
        b[..12].copy_from_slice(&self.tail);
        b[12..16].copy_from_slice(&nonce.to_le_bytes());
        b[16] = 0x80;
        b[62..].copy_from_slice(&640u16.to_be_bytes());
        b
    }

    /// Hash del bloque con este nonce, en el orden de bytes de SHA (el que
    /// se compara como número little-endian). En software.
    pub fn hash(&self, nonce: u32) -> [u8; 32] {
        let mut s = self.midstate;
        compress(&mut s, &self.second_block(nonce));
        let mut second = [0u8; 64];
        for (chunk, h) in second.as_chunks_mut::<4>().0.iter_mut().zip(s) {
            chunk.copy_from_slice(&h.to_be_bytes());
        }
        second[32] = 0x80;
        second[62..].copy_from_slice(&256u16.to_be_bytes());
        let mut out = H0;
        compress(&mut out, &second);
        let mut digest = [0u8; 32];
        for (chunk, h) in digest.as_chunks_mut::<4>().0.iter_mut().zip(out) {
            chunk.copy_from_slice(&h.to_be_bytes());
        }
        digest
    }
}

/// `hash <= target`, los dos como números de 256 bits little-endian.
pub fn meets(hash: &[u8; 32], target: &[u8; 32]) -> bool {
    for i in (0..32).rev() {
        if hash[i] != target[i] {
            return hash[i] < target[i];
        }
    }
    true
}

/// Valor aproximado de un número de 256 bits little-endian.
fn to_f64(n: &[u8; 32]) -> f64 {
    n.iter().rev().fold(0.0, |acc, &b| acc * 256.0 + b as f64)
}

/// El objetivo de dificultad 1: 0xFFFF × 2^208.
const DIFF1: f64 = 65535.0 * f64::from_bits((1023 + 208) << 52);

/// Dificultad de un hash: cuántas veces más difícil que un share de
/// dificultad 1. Para mostrar "el mejor tiro".
pub fn difficulty(hash: &[u8; 32]) -> f64 {
    let v = to_f64(hash);
    if v == 0.0 { f64::INFINITY } else { DIFF1 / v }
}

/// Objetivo para una dificultad de share (con un pelito de margen, para no
/// mandar shares que el pool cuente como insuficientes por redondeo).
pub fn target_from_difficulty(diff: f64) -> [u8; 32] {
    let mut out = [0u8; 32];
    if diff.is_nan() || diff <= 0.0 {
        return [0xFF; 32];
    }
    let mut v = DIFF1 / diff * 0.999_999;
    // Escribirlo byte a byte desde el más significativo.
    let mut scale = 1.0f64;
    for _ in 0..31 {
        scale *= 256.0;
    }
    if v >= scale * 256.0 {
        return [0xFF; 32];
    }
    for i in (0..32).rev() {
        let byte = (v / scale) as u64;
        let byte = byte.min(255) as u8;
        out[i] = byte;
        v -= byte as f64 * scale;
        scale /= 256.0;
    }
    out
}

/// Objetivo de la red a partir del campo `bits` de la cabecera.
pub fn target_from_nbits(nbits: u32) -> [u8; 32] {
    let exp = (nbits >> 24) as usize;
    let mant = nbits & 0x007F_FFFF;
    let mut out = [0u8; 32];
    for (i, b) in mant.to_le_bytes()[..3].iter().enumerate() {
        if let Some(pos) = (exp + i).checked_sub(3)
            && pos < 32
        {
            out[pos] = *b;
        }
    }
    out
}

/// Decodifica hex en `out` y devuelve cuántos bytes escribió.
pub fn hex_decode(hex: &str, out: &mut [u8]) -> Option<usize> {
    let h = hex.as_bytes();
    if !h.len().is_multiple_of(2) || h.len() / 2 > out.len() {
        return None;
    }
    for (i, pair) in h.as_chunks::<2>().0.iter().enumerate() {
        let d = |c: u8| (c as char).to_digit(16);
        out[i] = (d(pair[0])? << 4 | d(pair[1])?) as u8;
    }
    Some(h.len() / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex<const N: usize>(s: &str) -> [u8; N] {
        let mut out = [0u8; N];
        assert_eq!(hex_decode(s, &mut out), Some(N));
        out
    }

    /// La cabecera del bloque génesis.
    fn genesis() -> [u8; 80] {
        let mut h = [0u8; 80];
        h[..4].copy_from_slice(&1u32.to_le_bytes());
        let mut merkle: [u8; 32] =
            unhex("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b");
        merkle.reverse();
        h[36..68].copy_from_slice(&merkle);
        h[68..72].copy_from_slice(&1_231_006_505u32.to_le_bytes());
        h[72..76].copy_from_slice(&0x1D00_FFFFu32.to_le_bytes());
        h[76..80].copy_from_slice(&2_083_236_893u32.to_le_bytes());
        h
    }

    #[test]
    fn genesis_block_hash() {
        let header = genesis();
        let mut expected: [u8; 32] =
            unhex("000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f");
        expected.reverse();
        assert_eq!(sha256d(&header), expected);

        let work = Work::from_header(&header, target_from_difficulty(1.0));
        assert_eq!(work.hash(2_083_236_893), expected);
        assert!(meets(&expected, &work.block_target));
        assert!(meets(&expected, &work.share_target));
        assert!(!meets(&work.hash(2_083_236_892), &work.share_target));
        // Dificultad del génesis: ~2536 veces la de un share de dificultad 1.
        let d = difficulty(&expected);
        assert!((2536.0..2537.0).contains(&d), "{d}");
    }

    #[test]
    fn targets() {
        let one = target_from_nbits(0x1D00_FFFF);
        let mut expected = [0u8; 32];
        expected[26] = 0xFF;
        expected[27] = 0xFF;
        assert_eq!(one, expected);
        // La dificultad 1 de los pools es el mismo número (con el margen).
        let t = target_from_difficulty(1.0);
        assert_eq!(&t[28..], &[0; 4]);
        assert_eq!(t[27], 0xFF);
        assert_eq!(t[26], 0xFE);
        // Más difícil, objetivo más chico.
        assert!(meets(&target_from_difficulty(2.0), &t));
        assert!(!meets(&target_from_difficulty(0.5), &t));
        assert_eq!(target_from_difficulty(1e-12), [0xFF; 32]);
    }

    #[test]
    fn hex() {
        let mut out = [0u8; 4];
        assert_eq!(hex_decode("00fFa1", &mut out), Some(3));
        assert_eq!(&out[..3], &[0x00, 0xFF, 0xA1]);
        assert_eq!(hex_decode("abc", &mut out), None);
        assert_eq!(hex_decode("zz", &mut out), None);
        assert_eq!(hex_decode("0011223344", &mut out), None);
    }
}
