//! Espejo de la pantalla por el cable USB: la placa describe en una línea qué
//! pantalla muestra y con qué datos, y en la PC se dibuja con el mismo
//! [`crate::screen::draw`]. Así se ve exactamente lo que mostraría el display
//! sin tenerlo conectado.
//!
//! Formato: `@pantalla` y los campos separados por tabulaciones (los textos
//! no pueden tener tabs ni saltos de línea: se cambian por espacios).

use core::fmt::{self, Display, Write};

use crate::screen::{Mining, Screen};

pub const PREFIX: &str = "@pantalla";

/// Un texto sin tabs ni saltos de línea.
struct Field<'a>(&'a str);

impl Display for Field<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for c in self.0.chars() {
            f.write_char(if matches!(c, '\t' | '\n' | '\r') {
                ' '
            } else {
                c
            })?;
        }
        Ok(())
    }
}

/// Escribe la línea (sin el salto final).
pub fn describe(s: &Screen, out: &mut impl Write) -> fmt::Result {
    out.write_str(PREFIX)?;
    match *s {
        Screen::Scanning => out.write_str("\tbuscando"),
        Screen::Join { ssid, pass } => write!(out, "\tunirse\t{}\t{}", Field(ssid), Field(pass)),
        Screen::Open => out.write_str("\tabrir"),
        Screen::Saved { ssid } => write!(out, "\tguardado\t{}", Field(ssid)),
        Screen::Connecting { ssid } => write!(out, "\tconectando\t{}", Field(ssid)),
        Screen::Online { ssid, ip } => write!(
            out,
            "\tconectado\t{}\t{}.{}.{}.{}",
            Field(ssid),
            ip[0],
            ip[1],
            ip[2],
            ip[3]
        ),
        Screen::Failed { ssid, reason } => {
            write!(out, "\tfallo\t{}\t{}", Field(ssid), Field(reason))
        }
        Screen::Notice { title, text } => {
            write!(out, "\taviso\t{}\t{}", Field(title), Field(text))
        }
        Screen::Mining(m) => write!(
            out,
            "\tminando\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            Field(m.ssid),
            Field(m.pool),
            m.hashrate,
            m.accepted,
            m.rejected,
            m.best,
            m.uptime_secs,
            m.online as u8
        ),
    }
}

/// Lee una línea escrita por [`describe`] (puede traer basura alrededor,
/// como un `\r` final).
pub fn parse(line: &str) -> Option<Screen<'_>> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut f = line.strip_prefix(PREFIX)?.strip_prefix('\t')?.split('\t');
    let kind = f.next()?;
    let mut next = || f.next();
    let screen = match kind {
        "buscando" => Screen::Scanning,
        "unirse" => Screen::Join {
            ssid: next()?,
            pass: next()?,
        },
        "abrir" => Screen::Open,
        "guardado" => Screen::Saved { ssid: next()? },
        "conectando" => Screen::Connecting { ssid: next()? },
        "conectado" => {
            let ssid = next()?;
            let mut ip = [0u8; 4];
            let mut parts = next()?.split('.');
            for b in &mut ip {
                *b = parts.next()?.parse().ok()?;
            }
            Screen::Online { ssid, ip }
        }
        "fallo" => Screen::Failed {
            ssid: next()?,
            reason: next()?,
        },
        "aviso" => Screen::Notice {
            title: next()?,
            text: next()?,
        },
        "minando" => Screen::Mining(Mining {
            ssid: next()?,
            pool: next()?,
            hashrate: next()?.parse().ok()?,
            accepted: next()?.parse().ok()?,
            rejected: next()?.parse().ok()?,
            best: next()?.parse().ok()?,
            uptime_secs: next()?.parse().ok()?,
            online: next()? == "1",
        }),
        _ => return None,
    };
    Some(screen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_screen_round_trips() {
        let mining = Mining {
            ssid: "WiFi de casa",
            pool: "public-pool.io",
            hashrate: 219_634.5,
            accepted: 3,
            rejected: 1,
            best: 0.003_271_234,
            uptime_secs: 12_345,
            online: true,
        };
        let screens = [
            Screen::Scanning,
            Screen::Join {
                ssid: "Mono-2E48",
                pass: "5wh3nqwk",
            },
            Screen::Open,
            Screen::Saved { ssid: "MiRed" },
            Screen::Connecting { ssid: "MiRed" },
            Screen::Online {
                ssid: "MiRed",
                ip: [192, 168, 1, 153],
            },
            Screen::Failed {
                ssid: "MiRed",
                reason: "La clave del WiFi no es correcta.",
            },
            Screen::Notice {
                title: "Sin pool",
                text: "No llego al pool.",
            },
            Screen::Mining(mining),
        ];
        for s in screens {
            let mut line = std::string::String::new();
            describe(&s, &mut line).unwrap();
            assert!(!line.contains('\n'));
            assert_eq!(parse(&line), Some(s), "{line}");
            line.push_str("\r\n");
            assert_eq!(parse(&line), Some(s));
        }
    }

    #[test]
    fn tabs_in_names_do_not_break_the_line() {
        let s = Screen::Saved { ssid: "mi\tred" };
        let mut line = std::string::String::new();
        describe(&s, &mut line).unwrap();
        assert_eq!(parse(&line), Some(Screen::Saved { ssid: "mi red" }));
    }

    #[test]
    fn other_lines_are_ignored() {
        assert_eq!(parse("INFO - 220.0 kH/s"), None);
        assert_eq!(parse("@pantalla\tminando\tFJ"), None);
        assert_eq!(parse("@pantalla\tdesconocida"), None);
    }
}
