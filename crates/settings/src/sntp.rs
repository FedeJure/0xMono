//! SNTP (RFC 4330): un paquete UDP de 48 bytes al puerto 123 y la respuesta
//! trae la hora UTC. El reloj del mono va de a segundos, así que alcanza con
//! la hora en que el servidor contestó, sin corregir la demora de la red.

/// Servidores a probar, en orden.
pub const SERVERS: &[&str] = &["pool.ntp.org", "time.google.com", "time.cloudflare.com"];
pub const PORT: u16 = 123;
pub const PACKET_LEN: usize = 48;

/// Segundos entre 1900-01-01 (el cero de NTP) y 1970-01-01 (el de Unix).
const NTP_TO_UNIX: u64 = 2_208_988_800;

/// El pedido: versión 4, modo cliente y nada más.
pub fn request() -> [u8; PACKET_LEN] {
    let mut p = [0u8; PACKET_LEN];
    p[0] = 0b00_100_011; // LI = 0, VN = 4, modo 3 (cliente)
    p
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// No es una respuesta de servidor que se pueda usar.
    Bad,
    /// El servidor dice que no está en hora (o pide que no le pregunten).
    Unsynchronized,
}

/// Segundos Unix de la respuesta, redondeados.
pub fn parse_reply(p: &[u8]) -> Result<u64, Error> {
    if p.len() < PACKET_LEN {
        return Err(Error::Bad);
    }
    let li = p[0] >> 6;
    let mode = p[0] & 0b111;
    let stratum = p[1];
    if mode != 4 && mode != 5 {
        return Err(Error::Bad);
    }
    if li == 3 || stratum == 0 || stratum > 15 {
        return Err(Error::Unsynchronized);
    }
    let secs = u32::from_be_bytes([p[40], p[41], p[42], p[43]]);
    let frac = u32::from_be_bytes([p[44], p[45], p[46], p[47]]);
    if secs == 0 && frac == 0 {
        return Err(Error::Bad);
    }
    Ok(ntp_to_unix(secs) + (frac >= 0x8000_0000) as u64)
}

/// NTP cuenta con 32 bits y da la vuelta en 2036: lo que da un número chico
/// es de la vuelta siguiente (el mono no viaja al pasado).
pub fn ntp_to_unix(secs: u32) -> u64 {
    if secs & 0x8000_0000 != 0 {
        secs as u64 - NTP_TO_UNIX
    } else {
        secs as u64 + (1u64 << 32) - NTP_TO_UNIX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(secs: u32, frac: u32) -> [u8; PACKET_LEN] {
        let mut p = [0u8; PACKET_LEN];
        p[0] = 0b00_100_100; // LI 0, v4, servidor
        p[1] = 2;
        p[40..44].copy_from_slice(&secs.to_be_bytes());
        p[44..48].copy_from_slice(&frac.to_be_bytes());
        p
    }

    #[test]
    fn request_is_a_v4_client_packet() {
        let r = request();
        assert_eq!(r[0], 0x23);
        assert!(r[1..].iter().all(|&b| b == 0));
    }

    #[test]
    fn known_times() {
        // 2026-11-11 15:00:00 UTC = 1 794 409 200 Unix.
        let unix = 1_794_409_200u64;
        let ntp = (unix + NTP_TO_UNIX) as u32;
        assert_eq!(parse_reply(&reply(ntp, 0)), Ok(unix));
        assert_eq!(parse_reply(&reply(ntp, 0x8000_0000)), Ok(unix + 1));
        // Después de la vuelta de 2036 (7 de febrero de 2036, 06:28:16 UTC).
        assert_eq!(ntp_to_unix(0), 2_085_978_496);
        assert_eq!(ntp_to_unix(10), 2_085_978_506);
    }

    #[test]
    fn rejects_bad_replies() {
        let ok = reply(0xEC00_0000, 0);
        assert!(parse_reply(&ok[..47]).is_err());
        let mut client = ok;
        client[0] = 0x23;
        assert_eq!(parse_reply(&client), Err(Error::Bad));
        let mut kiss = ok;
        kiss[1] = 0;
        assert_eq!(parse_reply(&kiss), Err(Error::Unsynchronized));
        let mut alarm = ok;
        alarm[0] |= 0b1100_0000;
        assert_eq!(parse_reply(&alarm), Err(Error::Unsynchronized));
        assert_eq!(parse_reply(&reply(0, 0)), Err(Error::Bad));
    }
}
