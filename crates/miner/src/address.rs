//! Validación de direcciones de Bitcoin (red principal).
//!
//! Todas las direcciones traen un código de control: con una letra mal
//! copiada la cuenta no cierra. Se revisa antes de guardar para no pasarse
//! meses minando hacia una dirección que no existe.
//!
//! - `1…` (P2PKH) y `3…` (P2SH): Base58Check, con doble SHA-256.
//! - `bc1q…` (SegWit v0, BIP173) y `bc1p…` (Taproot, BIP350): bech32/bech32m.

use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    P2pkh,
    P2sh,
    /// SegWit v0: P2WPKH (20 bytes) o P2WSH (32 bytes).
    SegwitV0,
    Taproot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressError {
    Empty,
    /// Es de testnet/regtest: no cobraría bitcoins de verdad.
    Testnet,
    /// Tiene caracteres que no pueden estar en una dirección.
    BadChars,
    /// El código de control no cierra: casi seguro un error al copiarla.
    BadChecksum,
    /// Formato reconocido pero largo imposible.
    BadLength,
    /// Versión de SegWit futura u otro tipo que los pools no aceptan.
    Unsupported,
}

impl AddressError {
    pub fn message(self) -> &'static str {
        match self {
            AddressError::Empty => "Falta la dirección de Bitcoin.",
            AddressError::Testnet => {
                "Esa dirección es de testnet: no cobraría bitcoins de verdad. Usá una que empiece con bc1, 1 o 3."
            }
            AddressError::BadChars => {
                "La dirección tiene caracteres que no pueden estar. Copiala de nuevo desde tu billetera."
            }
            AddressError::BadChecksum => {
                "La dirección tiene un error (el código de control no cierra). Copiala de nuevo desde tu billetera."
            }
            AddressError::BadLength => {
                "La dirección tiene un largo imposible. Copiala de nuevo desde tu billetera."
            }
            AddressError::Unsupported => {
                "Ese tipo de dirección no lo aceptan los pools. Usá una que empiece con bc1q, bc1p, 1 o 3."
            }
        }
    }
}

pub fn check(addr: &str) -> Result<Kind, AddressError> {
    let addr = addr.trim();
    if addr.is_empty() {
        return Err(AddressError::Empty);
    }
    let lower_prefix = |p: &str| {
        addr.len() >= p.len() && addr.as_bytes()[..p.len()].eq_ignore_ascii_case(p.as_bytes())
    };
    if lower_prefix("bc1") {
        segwit(addr)
    } else if lower_prefix("tb1") || lower_prefix("bcrt1") {
        Err(AddressError::Testnet)
    } else {
        base58(addr)
    }
}

// --- Base58Check -------------------------------------------------------------

const B58: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

fn base58(addr: &str) -> Result<Kind, AddressError> {
    // Versión (1) + hash160 (20) + control (4).
    let mut bytes = [0u8; 25];
    for c in addr.bytes() {
        let mut carry = B58
            .iter()
            .position(|&a| a == c)
            .ok_or(AddressError::BadChars)? as u32;
        for b in bytes.iter_mut().rev() {
            carry += *b as u32 * 58;
            *b = carry as u8;
            carry >>= 8;
        }
        if carry != 0 {
            return Err(AddressError::BadLength);
        }
    }
    // Cada '1' inicial es un byte 0 inicial, ni uno más ni uno menos.
    let ones = addr.bytes().take_while(|&c| c == b'1').count();
    let zeros = bytes.iter().take_while(|&&b| b == 0).count();
    if ones != zeros {
        return Err(AddressError::BadLength);
    }
    let hash = Sha256::digest(Sha256::digest(&bytes[..21]));
    if hash[..4] != bytes[21..] {
        return Err(AddressError::BadChecksum);
    }
    match bytes[0] {
        0x00 => Ok(Kind::P2pkh),
        0x05 => Ok(Kind::P2sh),
        0x6F | 0xC4 => Err(AddressError::Testnet),
        _ => Err(AddressError::Unsupported),
    }
}

// --- Bech32 / Bech32m ----------------------------------------------------------

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const BECH32_CONST: u32 = 1;
const BECH32M_CONST: u32 = 0x2BC8_30A3;

fn polymod(values: impl Iterator<Item = u8>) -> u32 {
    const GEN: [u32; 5] = [
        0x3B6A_57B2,
        0x2650_8E6D,
        0x1EA1_19FA,
        0x3D42_33DD,
        0x2A14_62B3,
    ];
    let mut chk = 1u32;
    for v in values {
        let top = chk >> 25;
        chk = ((chk & 0x01FF_FFFF) << 5) ^ v as u32;
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

fn segwit(addr: &str) -> Result<Kind, AddressError> {
    if addr.len() > 90 {
        return Err(AddressError::BadLength);
    }
    let has_lower = addr.bytes().any(|c| c.is_ascii_lowercase());
    let has_upper = addr.bytes().any(|c| c.is_ascii_uppercase());
    if has_lower && has_upper {
        return Err(AddressError::BadChars);
    }

    // "bc" + '1' + datos (5 bits por carácter), los últimos 6 son el control.
    let data_part = &addr.as_bytes()[3..];
    if data_part.len() < 7 {
        return Err(AddressError::BadLength);
    }
    let mut data = [0u8; 87];
    for (d, &c) in data.iter_mut().zip(data_part) {
        *d = CHARSET
            .iter()
            .position(|&a| a == c.to_ascii_lowercase())
            .ok_or(AddressError::BadChars)? as u8;
    }
    let data = &data[..data_part.len()];

    // Prefijo expandido de "bc": bits altos, separador 0, bits bajos.
    let hrp = [b'b' >> 5, b'c' >> 5, 0, b'b' & 31, b'c' & 31];
    let check = polymod(hrp.iter().chain(data).copied());
    let version = data[0];
    let expected = if version == 0 {
        BECH32_CONST
    } else {
        BECH32M_CONST
    };
    if check != expected {
        return Err(AddressError::BadChecksum);
    }

    // De grupos de 5 bits a bytes; el relleno sobrante tiene que ser cero.
    let mut program_len = 0usize;
    let (mut acc, mut bits) = (0u32, 0u32);
    for &v in &data[1..data.len() - 6] {
        acc = ((acc << 5) | v as u32) & 0xFFF;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            program_len += 1;
        }
    }
    if bits >= 5 || (acc & ((1 << bits) - 1)) != 0 {
        return Err(AddressError::BadLength);
    }

    match (version, program_len) {
        (0, 20 | 32) => Ok(Kind::SegwitV0),
        (0, _) => Err(AddressError::BadLength),
        (1, 32) => Ok(Kind::Taproot),
        (1..=16, 2..=40) => Err(AddressError::Unsupported),
        _ => Err(AddressError::BadLength),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_mainnet_addresses() {
        // Dirección del bloque génesis.
        assert_eq!(check("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"), Ok(Kind::P2pkh));
        assert_eq!(check("3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy"), Ok(Kind::P2sh));
        // Ejemplos de BIP173 y BIP350.
        assert_eq!(
            check("BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4"),
            Ok(Kind::SegwitV0)
        );
        assert_eq!(
            check("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"),
            Ok(Kind::SegwitV0)
        );
        assert_eq!(
            check("bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3"),
            Ok(Kind::SegwitV0)
        );
        assert_eq!(
            check("bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0"),
            Ok(Kind::Taproot)
        );
        // Espacios al copiar y pegar.
        assert_eq!(
            check("  bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4\n"),
            Ok(Kind::SegwitV0)
        );
    }

    #[test]
    fn every_single_typo_is_caught() {
        for addr in [
            "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4",
            "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0",
        ] {
            let alphabet: &[u8] = if addr.starts_with("bc1") {
                CHARSET
            } else {
                B58
            };
            for i in 4..addr.len() {
                for &c in alphabet {
                    let mut typo = addr.as_bytes().to_vec();
                    if typo[i] == c {
                        continue;
                    }
                    typo[i] = c;
                    let typo = String::from_utf8(typo).unwrap();
                    assert!(check(&typo).is_err(), "{typo}");
                }
            }
        }
    }

    #[test]
    fn rejects_testnet_and_wrong_formats() {
        assert_eq!(
            check("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx"),
            Err(AddressError::Testnet)
        );
        assert_eq!(
            check("mipcBbFg9gMiCh81Kj8tqqdgoZub1ZJRfn"),
            Err(AddressError::Testnet)
        );
        // Taproot con el control de bech32 en vez de bech32m (BIP350).
        assert_eq!(
            check("bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqh2y7hd"),
            Err(AddressError::BadChecksum)
        );
        // Programa de 16 bytes en v0 (BIP173).
        assert_eq!(
            check("BC1QR508D6QEJXTDG4Y5R3ZARVARYV98GJ9P"),
            Err(AddressError::BadLength)
        );
        // Mayúsculas y minúsculas mezcladas.
        assert_eq!(
            check("bc1qW508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"),
            Err(AddressError::BadChars)
        );
        assert_eq!(
            check("0A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"),
            Err(AddressError::BadChars)
        );
        assert_eq!(check(""), Err(AddressError::Empty));
        assert_eq!(check("hola"), Err(AddressError::BadChars));
        assert!(check("11A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa").is_err());
    }
}
