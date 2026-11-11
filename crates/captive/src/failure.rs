//! Por qué no se pudo conectar al WiFi, en palabras para la pantalla y la
//! página. Viaja de un arranque al siguiente como un número.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    WrongPassword,
    NotFound,
    /// Seguridad que el ESP32 no maneja (por ejemplo, solo WPA3).
    Security,
    /// Se asoció pero el router no le dio IP.
    NoIp,
    Other,
}

impl Failure {
    pub const ALL: [Failure; 5] = [
        Failure::WrongPassword,
        Failure::NotFound,
        Failure::Security,
        Failure::NoIp,
        Failure::Other,
    ];

    pub fn message(self) -> &'static str {
        match self {
            Failure::WrongPassword => "La clave del WiFi no es correcta.",
            Failure::NotFound => {
                "No encuentro la red. Ojo: el mono solo ve redes de 2,4 GHz, no las de 5 GHz."
            }
            Failure::Security => {
                "La red usa una seguridad que el mono no maneja (por ejemplo, solo WPA3)."
            }
            Failure::NoIp => "Se conectó, pero el router no le dio una dirección (DHCP).",
            Failure::Other => "El router rechazó la conexión. Probá de nuevo.",
        }
    }

    pub fn code(self) -> u32 {
        Self::ALL.iter().position(|&f| f == self).unwrap_or(0) as u32 + 1
    }

    pub fn from_code(code: u32) -> Option<Failure> {
        Self::ALL.get((code as usize).checked_sub(1)?).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_zero_is_none() {
        for f in Failure::ALL {
            assert_eq!(Failure::from_code(f.code()), Some(f));
        }
        assert_eq!(Failure::from_code(0), None);
        assert_eq!(Failure::from_code(99), None);
    }
}
