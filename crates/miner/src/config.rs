//! Lo que el usuario configura desde el celular y se guarda en la flash.
//!
//! Formato del registro: `MONO`, versión, los textos como (largo, bytes), el
//! puerto y un CRC32 al final. La flash borrada se lee como `0xFF`, así que un
//! aparato nuevo (o un guardado cortado a la mitad) no pasa la verificación y
//! arranca en modo configuración.

use heapless::String;

pub use captive::{Pass, Ssid, crc32, text};
/// El largo máximo de una dirección bech32.
pub type Address = String<90>;
pub type Host = String<64>;
pub type Worker = String<32>;

/// Pool "solo" por defecto: sin comisión y con una página para ver al mono
/// trabajando (web.public-pool.io, buscando la dirección).
pub const DEFAULT_POOL_HOST: &str = "public-pool.io";
pub const DEFAULT_POOL_PORT: u16 = 21496;
pub const DEFAULT_WORKER: &str = "mono";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub ssid: Ssid,
    /// Vacía para redes abiertas.
    pub pass: Pass,
    /// Dirección de Bitcoin que cobra si encuentra un bloque. Solo la pública:
    /// la clave privada nunca pasa por el aparato.
    pub address: Address,
    pub pool_host: Host,
    pub pool_port: u16,
    /// Nombre con el que el pool lista a este minero (`direccion.worker`).
    pub worker: Worker,
}

const MAGIC: &[u8; 4] = b"MONO";
const VERSION: u8 = 1;

/// Espacio máximo que ocupa un [`Config`] codificado.
pub const MAX_ENCODED: usize = 4 + 1 + (1 + 32) + (1 + 64) + (1 + 90) + (1 + 64) + (1 + 32) + 2 + 4;

impl Config {
    /// Escribe el registro en `out` y devuelve cuántos bytes usó.
    pub fn encode(&self, out: &mut [u8; MAX_ENCODED]) -> usize {
        let mut n = 0;
        let mut put = |bytes: &[u8]| {
            out[n..n + bytes.len()].copy_from_slice(bytes);
            n += bytes.len();
        };
        put(MAGIC);
        put(&[VERSION]);
        for s in [
            self.ssid.as_str(),
            self.pass.as_str(),
            self.address.as_str(),
            self.pool_host.as_str(),
            self.worker.as_str(),
        ] {
            put(&[s.len() as u8]);
            put(s.as_bytes());
        }
        put(&self.pool_port.to_le_bytes());
        let crc = crc32(&out[..n]);
        out[n..n + 4].copy_from_slice(&crc.to_le_bytes());
        n + 4
    }

    pub fn decode(data: &[u8]) -> Option<Config> {
        let mut r = Reader { data, pos: 0 };
        if r.take(4)? != MAGIC || r.take(1)? != [VERSION] {
            return None;
        }
        let ssid = r.text()?;
        let pass = r.text()?;
        let address = r.text()?;
        let pool_host = r.text()?;
        let worker = r.text()?;
        let pool_port = u16::from_le_bytes(r.take(2)?.try_into().ok()?);
        let end = r.pos;
        let crc = u32::from_le_bytes(r.take(4)?.try_into().ok()?);
        (crc == crc32(&data[..end])).then_some(Config {
            ssid,
            pass,
            address,
            pool_host,
            pool_port,
            worker,
        })
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.data.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(s)
    }

    fn text<const N: usize>(&mut self) -> Option<String<N>> {
        let len = self.take(1)?[0] as usize;
        text(core::str::from_utf8(self.take(len)?).ok()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Config {
        Config {
            ssid: text("WiFi de casa").unwrap(),
            pass: text("ñandú-2026!").unwrap(),
            address: text("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4").unwrap(),
            pool_host: text(DEFAULT_POOL_HOST).unwrap(),
            pool_port: DEFAULT_POOL_PORT,
            worker: text(DEFAULT_WORKER).unwrap(),
        }
    }

    #[test]
    fn round_trip() {
        let mut buf = [0u8; MAX_ENCODED];
        let n = sample().encode(&mut buf);
        assert_eq!(Config::decode(&buf[..n]), Some(sample()));
    }

    #[test]
    fn longest_fields_fit() {
        let mut c = sample();
        c.ssid = text(&"s".repeat(32)).unwrap();
        c.pass = text(&"p".repeat(64)).unwrap();
        c.address = text(&"a".repeat(90)).unwrap();
        c.pool_host = text(&"h".repeat(64)).unwrap();
        c.worker = text(&"w".repeat(32)).unwrap();
        let mut buf = [0u8; MAX_ENCODED];
        assert_eq!(c.encode(&mut buf), MAX_ENCODED);
        assert_eq!(Config::decode(&buf), Some(c));
    }

    #[test]
    fn erased_flash_and_corruption_are_rejected() {
        assert_eq!(Config::decode(&[0xFF; MAX_ENCODED]), None);

        let mut buf = [0u8; MAX_ENCODED];
        let n = sample().encode(&mut buf);
        for i in 0..n {
            let mut bad = buf;
            bad[i] ^= 0x04;
            assert_eq!(Config::decode(&bad[..n]), None, "byte {i}");
        }
        assert_eq!(Config::decode(&buf[..n - 1]), None);
    }
}
