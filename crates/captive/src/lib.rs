//! Lo que comparten los portales de configuración del mono (el del minero y
//! el del aparato): la red propia con DNS "cautivo", lo mínimo de HTTP, la
//! lista de redes encontradas, el QR para unirse y cómo se explica una
//! conexión que falló.
//!
//! Sin hardware ni memoria dinámica: corre igual en el simulador (con tests)
//! y en el ESP32-C3.
#![cfg_attr(not(test), no_std)]

pub mod dns;
pub mod failure;
pub mod http;

use core::fmt::{self, Write};

use heapless::{String, Vec};

pub use failure::Failure;

/// IP del mono en su propia red.
pub const AP_IP: [u8; 4] = [192, 168, 4, 1];
pub const AP_HOST: &str = "192.168.4.1";
pub const AP_URL: &str = "http://192.168.4.1/";

/// Respuesta para todo lo que no es la página: el celular la sigue y termina
/// abriendo el formulario.
pub const REDIRECT: &str = "HTTP/1.1 302 Found\r\nLocation: http://192.168.4.1/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

pub type Ssid = String<32>;
/// WPA2 acepta frases de 8 a 63 caracteres (o 64 dígitos hexadecimales).
pub type Pass = String<64>;

/// Copia `s` en un `String<N>`, o `None` si no entra.
pub fn text<const N: usize>(s: &str) -> Option<String<N>> {
    let mut out = String::new();
    out.push_str(s).ok()?;
    Some(out)
}

/// CRC-32 (IEEE), bit a bit: son unos pocos cientos de bytes.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

/// Si la clave sirve para WPA2: vacía (red abierta), de 8 a 63 caracteres o
/// 64 dígitos hexadecimales.
pub fn valid_pass(pass: &str) -> bool {
    let hex_key = pass.len() == 64 && pass.bytes().all(|b| b.is_ascii_hexdigit());
    pass.is_empty() || (8..=63).contains(&pass.len()) || hex_key
}

/// Cabecera de una respuesta HTML de `len` bytes.
pub fn head(w: &mut impl Write, len: usize) -> fmt::Result {
    write!(
        w,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {len}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
    )
}

// --- Redes encontradas ---------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    pub ssid: Ssid,
    /// dBm: -50 es excelente, -80 apenas llega.
    pub rssi: i8,
    pub open: bool,
}

/// Agrega una red a la lista, sin repetidos (varios routers con el mismo
/// nombre) y ordenada de la señal más fuerte a la más débil.
pub fn add_network<const N: usize>(list: &mut Vec<Network, N>, ssid: &str, rssi: i8, open: bool) {
    let Some(ssid) = text(ssid) else { return };
    if ssid.is_empty() {
        return;
    }
    if let Some(i) = list.iter().position(|n| n.ssid == ssid) {
        if list[i].rssi >= rssi {
            return;
        }
        list.remove(i);
    }
    let at = list
        .iter()
        .position(|n| n.rssi < rssi)
        .unwrap_or(list.len());
    if list.is_full() {
        if at == list.len() {
            return;
        }
        list.pop();
    }
    let _ = list.insert(at, Network { ssid, rssi, open });
}

/// Rayitas de señal para la página.
pub fn bars(rssi: i8) -> &'static str {
    match rssi {
        -55.. => "▂▄▆█",
        -67..=-56 => "▂▄▆",
        -78..=-68 => "▂▄",
        _ => "▂",
    }
}

/// Texto del QR para unirse a una red WiFi (lo entienden las cámaras de
/// Android y iPhone).
pub fn wifi_qr_text(ssid: &str, pass: &str) -> String<220> {
    fn esc(out: &mut String<220>, s: &str) {
        for c in s.chars() {
            if matches!(c, '\\' | ';' | ',' | ':' | '"') {
                let _ = out.push('\\');
            }
            let _ = out.push(c);
        }
    }
    let mut out = String::new();
    let _ = out.push_str("WIFI:T:WPA;S:");
    esc(&mut out, ssid);
    let _ = out.push_str(";P:");
    esc(&mut out, pass);
    let _ = out.push_str(";;");
    out
}

/// Clave de 8 letras y números sin los que se confunden (0/O, 1/l/I), a
/// partir de un generador al azar cualquiera.
pub fn random_pass(mut random: impl FnMut() -> u32) -> String<8> {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut pass = String::new();
    for _ in 0..8 {
        let c = ALPHABET[random() as usize % ALPHABET.len()];
        let _ = pass.push(c as char);
    }
    pass
}

// --- HTML ----------------------------------------------------------------------

/// Escribe texto escapado para HTML.
pub struct Esc<'a>(pub &'a str);

impl fmt::Display for Esc<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.chars() {
            match c {
                '&' => f.write_str("&amp;")?,
                '<' => f.write_str("&lt;")?,
                '>' => f.write_str("&gt;")?,
                '"' => f.write_str("&quot;")?,
                '\'' => f.write_str("&#39;")?,
                c => f.write_char(c)?,
            }
        }
        Ok(())
    }
}

/// `fmt::Write` sobre un buffer fijo: falla si no entra.
pub struct BufWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl<'a> BufWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Write for BufWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.len + s.len();
        self.buf
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn networks_are_unique_and_sorted() {
        let mut list: Vec<Network, 3> = Vec::new();
        add_network(&mut list, "Casa", -70, false);
        add_network(&mut list, "Vecino", -50, false);
        add_network(&mut list, "Casa", -60, false);
        add_network(&mut list, "", -40, true);
        add_network(&mut list, "Bar", -80, true);
        add_network(&mut list, "Lejos", -90, true);
        add_network(&mut list, "Cerca", -45, true);
        let names: std::vec::Vec<&str> = list.iter().map(|n| n.ssid.as_str()).collect();
        assert_eq!(names, ["Cerca", "Vecino", "Casa"]);
    }

    #[test]
    fn crc32_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn wifi_qr_escapes_specials() {
        assert_eq!(
            wifi_qr_text("Mono-4F2A", "a;b:c\\d"),
            r"WIFI:T:WPA;S:Mono-4F2A;P:a\;b\:c\\d;;"
        );
    }

    #[test]
    fn passwords() {
        assert!(valid_pass(""));
        assert!(!valid_pass("corta"));
        assert!(valid_pass("12345678"));
        assert!(valid_pass(&"a".repeat(63)));
        assert!(!valid_pass(&"g".repeat(64)));
        assert!(valid_pass(&"f".repeat(64)));
        let mut n = 0u32;
        let p = random_pass(|| {
            n = n.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            n >> 8
        });
        assert_eq!(p.len(), 8);
        assert!(p.bytes().all(|b| !b"01lIO".contains(&b)));
    }

    #[test]
    fn escapes_html() {
        let mut s: String<64> = String::new();
        write!(s, "{}", Esc("<a href=\"x\">'&'</a>")).unwrap();
        assert_eq!(s, "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;");
    }
}
