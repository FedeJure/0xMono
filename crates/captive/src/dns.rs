//! DNS "cautivo": a cualquier nombre que pregunte el celular le contesta con la
//! IP del mono. Así el teléfono, al chequear si hay internet, termina en la
//! página de configuración y la abre solo (como el WiFi de un hotel).

const TYPE_A: u16 = 1;
const TYPE_ANY: u16 = 255;
const CLASS_IN: u16 = 1;
/// Corto, para que el celular no se quede con la respuesta trucha al volver a
/// su WiFi.
const TTL_SECS: u32 = 10;

/// Arma en `out` la respuesta a `query` y devuelve su largo, o `None` si no
/// es una consulta que valga la pena contestar.
pub fn reply(query: &[u8], ip: [u8; 4], out: &mut [u8]) -> Option<usize> {
    let header = query.get(..12)?;
    let flags = u16::from_be_bytes([header[2], header[3]]);
    let is_query = flags & 0x8000 == 0;
    let opcode = (flags >> 11) & 0xF;
    let qdcount = u16::from_be_bytes([header[4], header[5]]);
    if !is_query || opcode != 0 || qdcount != 1 {
        return None;
    }

    // Nombre: etiquetas (largo + texto) hasta un largo 0.
    let mut pos = 12;
    loop {
        let len = *query.get(pos)? as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        if len > 63 {
            return None;
        }
        pos += 1 + len;
    }
    let qtype = u16::from_be_bytes([*query.get(pos)?, *query.get(pos + 1)?]);
    let qclass = u16::from_be_bytes([*query.get(pos + 2)?, *query.get(pos + 3)?]);
    let question = &query[12..pos + 4];
    let answer = qclass == CLASS_IN && (qtype == TYPE_A || qtype == TYPE_ANY);

    let len = 12 + question.len() + if answer { 16 } else { 0 };
    let out = out.get_mut(..len)?;
    out[..2].copy_from_slice(&header[..2]);
    // Respuesta, autoritativa, copia "recursión deseada", recursión disponible.
    let flags = 0x8400 | (flags & 0x0100) | 0x0080;
    out[2..4].copy_from_slice(&flags.to_be_bytes());
    out[4..6].copy_from_slice(&1u16.to_be_bytes());
    out[6..8].copy_from_slice(&(answer as u16).to_be_bytes());
    out[8..12].fill(0);
    out[12..12 + question.len()].copy_from_slice(question);
    if answer {
        let a = &mut out[12 + question.len()..];
        // Puntero al nombre de la pregunta (offset 12).
        a[..2].copy_from_slice(&0xC00Cu16.to_be_bytes());
        a[2..4].copy_from_slice(&TYPE_A.to_be_bytes());
        a[4..6].copy_from_slice(&CLASS_IN.to_be_bytes());
        a[6..10].copy_from_slice(&TTL_SECS.to_be_bytes());
        a[10..12].copy_from_slice(&4u16.to_be_bytes());
        a[12..16].copy_from_slice(&ip);
    }
    Some(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(name: &str, qtype: u16) -> Vec<u8> {
        let mut q = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in name.split('.') {
            q.push(label.len() as u8);
            q.extend_from_slice(label.as_bytes());
        }
        q.push(0);
        q.extend_from_slice(&qtype.to_be_bytes());
        q.extend_from_slice(&CLASS_IN.to_be_bytes());
        q
    }

    #[test]
    fn answers_a_with_our_ip() {
        let q = query("captive.apple.com", TYPE_A);
        let mut out = [0u8; 512];
        let n = reply(&q, [192, 168, 4, 1], &mut out).unwrap();
        let r = &out[..n];
        assert_eq!(&r[..2], &[0x12, 0x34]);
        assert_eq!(r[2] & 0x80, 0x80, "es respuesta");
        assert_eq!(&r[6..8], &[0, 1], "una respuesta");
        assert_eq!(&r[12..q.len()], &q[12..]);
        assert_eq!(&r[n - 4..], &[192, 168, 4, 1]);
    }

    #[test]
    fn aaaa_gets_an_empty_answer() {
        let q = query("connectivitycheck.gstatic.com", 28);
        let mut out = [0u8; 512];
        let n = reply(&q, [192, 168, 4, 1], &mut out).unwrap();
        assert_eq!(n, q.len());
        assert_eq!(&out[6..8], &[0, 0]);
    }

    #[test]
    fn ignores_garbage() {
        let mut out = [0u8; 512];
        assert_eq!(reply(&[1, 2, 3], [0; 4], &mut out), None);
        let mut q = query("a.b", TYPE_A);
        q[2] |= 0x80; // ya es una respuesta
        assert_eq!(reply(&q, [0; 4], &mut out), None);
        let q = query("a.b", TYPE_A);
        assert_eq!(reply(&q[..q.len() - 2], [0; 4], &mut out), None);
    }
}
