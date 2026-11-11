//! Lo que un arranque le deja dicho al siguiente. El WiFi y el 3D no entran
//! juntos en la RAM, así que para cambiar de modo el mono se reinicia y el
//! pedido viaja en la RAM del RTC (sobrevive al reinicio por software, no a
//! quedarse sin corriente). Acá está qué se pide y cómo se escribe en tres
//! palabras de 32 bits; el firmware solo las guarda y las lee.

use captive::Failure;
use companion::LocalTime;

use crate::Settings;

/// Al prender, esa memoria tiene basura: sin la marca no hay pedido.
const MAGIC: u32 = 0x304D_5821; // "!XM0"

pub type Words = [u32; 3];

/// Qué hacer en el próximo arranque.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// No hay pedido: se acaba de enchufar (o se reinició solo).
    PowerOn,
    House(HouseStart),
    /// Configuración desde el celular.
    Portal,
    /// Ponerse en hora por internet y después volver a la casa.
    Sync(SyncStart),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HouseStart {
    /// La hora nueva, si se sabe (por internet o puesta a mano). Si no, sigue
    /// desde la guardada.
    pub clock: Option<LocalTime>,
    /// Olvidar al personaje y empezar con otro.
    pub new_monkey: bool,
    /// Qué contarle a la persona al llegar.
    pub notice: Option<Notice>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncStart {
    /// La pidió alguien (desde el menú o la página): se cuenta cómo salió.
    /// Al prender es automática y silenciosa.
    pub manual: bool,
    pub new_monkey: bool,
    /// Si internet falla, esta hora (la del celular), más lo que se tardó.
    pub fallback: Option<LocalTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    Saved,
    NewMonkey,
    Factory,
    /// Se puso en hora por internet.
    TimeSet,
    TimeFailed(SyncError),
    /// Pidieron la hora por internet sin WiFi configurado.
    NoWifi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncError {
    Wifi(Failure),
    /// Conectó al WiFi pero ningún servidor de hora contestó.
    NoServer,
}

impl SyncError {
    pub fn message(self) -> &'static str {
        match self {
            SyncError::Wifi(f) => f.message(),
            SyncError::NoServer => {
                "Me conecté al WiFi pero no pude preguntar la hora. ¿Hay internet?"
            }
        }
    }
}

impl Notice {
    fn code(self) -> (u8, u8) {
        match self {
            Notice::Saved => (1, 0),
            Notice::NewMonkey => (2, 0),
            Notice::Factory => (3, 0),
            Notice::TimeSet => (4, 0),
            Notice::TimeFailed(SyncError::NoServer) => (5, 0),
            Notice::TimeFailed(SyncError::Wifi(f)) => (5, f.code() as u8),
            Notice::NoWifi => (6, 0),
        }
    }

    fn from_code(code: u8, detail: u8) -> Option<Notice> {
        Some(match code {
            1 => Notice::Saved,
            2 => Notice::NewMonkey,
            3 => Notice::Factory,
            4 => Notice::TimeSet,
            5 => Notice::TimeFailed(
                Failure::from_code(detail as u32).map_or(SyncError::NoServer, SyncError::Wifi),
            ),
            6 => Notice::NoWifi,
            _ => return None,
        })
    }
}

const HOUSE: u8 = 1;
const PORTAL: u8 = 2;
const SYNC: u8 = 3;
const NEW_MONKEY: u8 = 1;
const MANUAL: u8 = 2;

fn secs(t: Option<LocalTime>) -> u32 {
    // 0 = sin hora (1970 no es una fecha posible).
    t.map_or(0, |t| t.total_secs())
}

fn time(secs: u32) -> Option<LocalTime> {
    (secs != 0).then(|| LocalTime::from_total_secs(secs))
}

impl Next {
    pub fn encode(self) -> Words {
        let (kind, flags, notice, clock) = match self {
            Next::PowerOn => return [0; 3],
            Next::Portal => (PORTAL, 0, None, None),
            Next::House(h) => (
                HOUSE,
                if h.new_monkey { NEW_MONKEY } else { 0 },
                h.notice,
                h.clock,
            ),
            Next::Sync(s) => (
                SYNC,
                (if s.new_monkey { NEW_MONKEY } else { 0 }) | if s.manual { MANUAL } else { 0 },
                None,
                s.fallback,
            ),
        };
        let (code, detail) = notice.map_or((0, 0), Notice::code);
        [
            MAGIC,
            u32::from_le_bytes([kind, flags, code, detail]),
            secs(clock),
        ]
    }

    pub fn decode(w: Words) -> Next {
        if w[0] != MAGIC {
            return Next::PowerOn;
        }
        let [kind, flags, code, detail] = w[1].to_le_bytes();
        let new_monkey = flags & NEW_MONKEY != 0;
        match kind {
            HOUSE => Next::House(HouseStart {
                clock: time(w[2]),
                new_monkey,
                notice: Notice::from_code(code, detail),
            }),
            PORTAL => Next::Portal,
            SYNC => Next::Sync(SyncStart {
                manual: flags & MANUAL != 0,
                new_monkey,
                fallback: time(w[2]),
            }),
            _ => Next::PowerOn,
        }
    }
}

/// En qué modo arrancar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boot {
    House(HouseStart),
    Portal,
    Sync(SyncStart),
}

/// Decide el modo con el pedido y la configuración guardada. Al enchufarlo,
/// si tiene WiFi y la hora automática, primero se pone en hora.
pub fn boot(next: Next, s: &Settings) -> Boot {
    match next {
        Next::PowerOn if s.wants_sync() => Boot::Sync(SyncStart::default()),
        Next::PowerOn => Boot::House(HouseStart::default()),
        Next::House(h) => Boot::House(h),
        Next::Portal => Boot::Portal,
        Next::Sync(x) if s.has_wifi() => Boot::Sync(x),
        Next::Sync(x) => Boot::House(HouseStart {
            clock: x.fallback,
            new_monkey: x.new_monkey,
            notice: x.manual.then_some(Notice::NoWifi),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> std::vec::Vec<Next> {
        let t = Some(LocalTime::new(20_768, 14, 32));
        let mut v = vec![Next::PowerOn, Next::Portal];
        let notices = [
            None,
            Some(Notice::Saved),
            Some(Notice::NewMonkey),
            Some(Notice::Factory),
            Some(Notice::TimeSet),
            Some(Notice::NoWifi),
            Some(Notice::TimeFailed(SyncError::NoServer)),
            Some(Notice::TimeFailed(SyncError::Wifi(Failure::WrongPassword))),
            Some(Notice::TimeFailed(SyncError::Wifi(Failure::Other))),
        ];
        for notice in notices {
            for new_monkey in [false, true] {
                for clock in [None, t] {
                    v.push(Next::House(HouseStart {
                        clock,
                        new_monkey,
                        notice,
                    }));
                }
            }
        }
        for manual in [false, true] {
            for new_monkey in [false, true] {
                for fallback in [None, t] {
                    v.push(Next::Sync(SyncStart {
                        manual,
                        new_monkey,
                        fallback,
                    }));
                }
            }
        }
        v
    }

    #[test]
    fn every_request_round_trips() {
        for n in all() {
            assert_eq!(Next::decode(n.encode()), n, "{n:?}");
        }
    }

    #[test]
    fn garbage_is_power_on() {
        assert_eq!(Next::decode([0; 3]), Next::PowerOn);
        assert_eq!(Next::decode([0xFFFF_FFFF; 3]), Next::PowerOn);
        assert_eq!(
            Next::decode([0x4D4F_4E4F, 1, 0]),
            Next::PowerOn,
            "el del minero"
        );
        assert_eq!(Next::decode([MAGIC, 99, 0]), Next::PowerOn);
    }

    #[test]
    fn power_on_syncs_only_with_wifi_and_auto_time() {
        let mut s = Settings::default();
        assert_eq!(boot(Next::PowerOn, &s), Boot::House(HouseStart::default()));
        s.ssid = captive::text("Casa").unwrap();
        assert_eq!(boot(Next::PowerOn, &s), Boot::Sync(SyncStart::default()));
        s.auto_time = false;
        assert_eq!(boot(Next::PowerOn, &s), Boot::House(HouseStart::default()));
        // Pedida a mano, aunque la automática esté apagada.
        let manual = SyncStart {
            manual: true,
            ..SyncStart::default()
        };
        assert_eq!(boot(Next::Sync(manual), &s), Boot::Sync(manual));
    }

    #[test]
    fn sync_without_wifi_explains_and_keeps_the_phone_time() {
        let t = Some(LocalTime::new(20_768, 9, 0));
        let b = boot(
            Next::Sync(SyncStart {
                manual: true,
                new_monkey: true,
                fallback: t,
            }),
            &Settings::default(),
        );
        assert_eq!(
            b,
            Boot::House(HouseStart {
                clock: t,
                new_monkey: true,
                notice: Some(Notice::NoWifi)
            })
        );
    }
}
