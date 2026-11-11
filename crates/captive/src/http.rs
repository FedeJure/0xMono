//! Lo mínimo de HTTP/1.x para servir la página de configuración: leer la
//! cabecera de un pedido y los campos de un formulario.

use heapless::{String, Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request<'a> {
    pub method: Method,
    pub path: &'a str,
    pub query: &'a str,
    /// Sin el puerto. Vacío si el pedido no lo trae.
    pub host: &'a str,
    pub content_length: usize,
    /// Dónde empieza el cuerpo dentro del buffer.
    pub body_start: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Todavía no llegó la cabecera completa.
    Incomplete,
    Bad,
}

/// Lee la cabecera de `buf`. El cuerpo (si hay) va de `body_start` a
/// `body_start + content_length`, y puede no haber llegado entero todavía.
pub fn parse(buf: &[u8]) -> Result<Request<'_>, ParseError> {
    let end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or(ParseError::Incomplete)?;
    let head = core::str::from_utf8(&buf[..end]).map_err(|_| ParseError::Bad)?;
    let mut lines = head.split("\r\n");

    let mut first = lines.next().ok_or(ParseError::Bad)?.split(' ');
    let method = match first.next() {
        Some("GET") | Some("HEAD") => Method::Get,
        Some("POST") => Method::Post,
        Some(_) => Method::Other,
        None => return Err(ParseError::Bad),
    };
    let target = first.next().ok_or(ParseError::Bad)?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));

    let mut host = "";
    let mut content_length = 0;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("host") {
            host = value.split(':').next().unwrap_or("");
        } else if name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse().map_err(|_| ParseError::Bad)?;
        }
    }
    Ok(Request {
        method,
        path,
        query,
        host,
        content_length,
        body_start: end + 4,
    })
}

/// Valor crudo (todavía codificado) de `key` en `a=1&b=2`.
pub fn field<'a>(form: &'a str, key: &str) -> Option<&'a str> {
    form.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (k == key).then_some(v)
    })
}

/// Decodifica `application/x-www-form-urlencoded` (`+` es espacio, `%XX` un
/// byte). Falla si no entra en `N` bytes o no da UTF-8 válido.
pub fn decode<const N: usize>(raw: &str) -> Option<String<N>> {
    let mut out: Vec<u8, N> = Vec::new();
    let mut bytes = raw.bytes();
    while let Some(b) = bytes.next() {
        let b = match b {
            b'+' => b' ',
            b'%' => {
                let hi = hex(bytes.next()?)?;
                let lo = hex(bytes.next()?)?;
                hi << 4 | lo
            }
            b => b,
        };
        out.push(b).ok()?;
    }
    String::from_utf8(out).ok()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_get_with_host_and_query() {
        let req = parse(
            b"GET /generate_204?x=1 HTTP/1.1\r\nHost: connectivitycheck.gstatic.com:80\r\nUser-Agent: a\r\n\r\n",
        )
        .unwrap();
        assert_eq!(req.method, Method::Get);
        assert_eq!(req.path, "/generate_204");
        assert_eq!(req.query, "x=1");
        assert_eq!(req.host, "connectivitycheck.gstatic.com");
        assert_eq!(req.content_length, 0);
    }

    #[test]
    fn parses_post_body_position() {
        let raw =
            b"POST /guardar HTTP/1.1\r\nhost: 192.168.4.1\r\ncontent-length: 7\r\n\r\nssid=ab";
        let req = parse(raw).unwrap();
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.host, "192.168.4.1");
        assert_eq!(&raw[req.body_start..][..req.content_length], b"ssid=ab");
    }

    #[test]
    fn incomplete_and_bad_heads() {
        assert_eq!(
            parse(b"GET / HTTP/1.1\r\nHost: x"),
            Err(ParseError::Incomplete)
        );
        assert_eq!(
            parse(b"POST / HTTP/1.1\r\nContent-Length: mucho\r\n\r\n"),
            Err(ParseError::Bad)
        );
    }

    #[test]
    fn form_fields_are_decoded() {
        let form = "ssid=WiFi+de+casa&pass=%C3%B1and%C3%BA%26%3D%2B&vacio=&solo";
        assert_eq!(
            decode::<32>(field(form, "ssid").unwrap()).unwrap(),
            "WiFi de casa"
        );
        assert_eq!(
            decode::<32>(field(form, "pass").unwrap()).unwrap(),
            "ñandú&=+"
        );
        assert_eq!(field(form, "vacio"), Some(""));
        assert_eq!(field(form, "solo"), Some(""));
        assert_eq!(field(form, "falta"), None);
    }

    #[test]
    fn decode_rejects_overflow_and_garbage() {
        assert!(decode::<3>("abcd").is_none());
        assert!(decode::<8>("%zz").is_none());
        assert!(decode::<8>("%C3").is_none());
        assert!(decode::<8>("%4").is_none());
    }
}
