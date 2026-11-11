//! Lo que dice (y piensa) el personaje.
//!
//! Frases escritas a mano ([`lines`](crate::lines)) con condiciones: qué
//! está haciendo, la hora, si es día de semana o fin de semana, el humor del
//! día, sus gustos y cómo viene el mercado. Las frases tienen huecos que se
//! llenan al azar o con su día (`{coin}`, `{pct}`, `{game}`, `{did}` lo que
//! hizo hoy, `{next}` lo que va a hacer después...), así que cada regla da
//! muchas variantes. Sin memoria dinámica: corre igual en el ESP32.
//!
//! Dos maneras de hablar: [`Kind::Say`] cuando le hablás, y
//! [`Kind::Think`] cuando piensa en voz alta solo, cada tanto.

use crate::activity::Activity;
use crate::clock::LocalTime;
use crate::companion::Greeting;
use crate::lines::{self, Rule};
use crate::personality::Personality;
use crate::rng::Rng;
use crate::routine::{DayKind, Mood};

/// Largo máximo de una frase ya armada, en bytes (UTF-8).
pub const MAX_LEN: usize = 120;
/// Largo máximo de una frase, en letras: lo que se lee tranquilo en el globo.
/// Cómo se corta en renglones depende de la letra del globo (proporcional):
/// eso lo controla `habitat::bubble`, que prueba todas las reglas.
pub const MAX_CHARS: usize = 90;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Te contesta.
    Say,
    /// Piensa en voz alta (globo de pensamiento).
    Think,
}

/// Una frase armada, en un búfer fijo.
#[derive(Clone, Copy)]
pub struct Line {
    buf: [u8; MAX_LEN],
    len: u8,
    pub kind: Kind,
    /// Regla de la que salió (para no repetirla enseguida).
    pub rule: u16,
}

impl Line {
    pub fn as_str(&self) -> &str {
        // SAFETY de contenido: solo se copian `&str` enteros (ver `push`).
        core::str::from_utf8(&self.buf[..self.len as usize]).unwrap_or("")
    }

    fn new(kind: Kind, rule: u16) -> Self {
        Line {
            buf: [0; MAX_LEN],
            len: 0,
            kind,
            rule,
        }
    }

    /// Agrega `s` entero o nada (nunca corta un carácter a la mitad).
    fn push(&mut self, s: &str) -> bool {
        let (at, n) = (self.len as usize, s.len());
        if at + n > MAX_LEN {
            return false;
        }
        self.buf[at..at + n].copy_from_slice(s.as_bytes());
        self.len += n as u8;
        true
    }

    fn push_num(&mut self, n: u32) -> bool {
        let mut digits = [0u8; 10];
        let mut k = 0;
        let mut n = n;
        loop {
            digits[k] = b'0' + (n % 10) as u8;
            k += 1;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        let mut out = [0u8; 10];
        for i in 0..k {
            out[i] = digits[k - 1 - i];
        }
        self.push(core::str::from_utf8(&out[..k]).unwrap_or(""))
    }

    /// Cuánto tiempo mostrarla (tiempo real): para leerla tranquilo, con
    /// la letra grande que la reparte en más renglones.
    pub fn duration_ms(&self) -> u32 {
        3_000 + self.as_str().chars().count() as u32 * 65
    }
}

impl core::fmt::Debug for Line {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}({:?})", self.kind, self.as_str())
    }
}

/// Todo lo que hace falta saber para elegir qué decir.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    pub activity: Activity,
    pub now: LocalTime,
    pub kind: DayKind,
    pub mood: Mood,
    pub personality: &'a Personality,
    /// Cómo viene el mercado: −2 (se desploma) a 2 (a la luna).
    pub market: i8,
    /// Algo que ya hizo hoy y lo próximo que le toca.
    pub did: Option<Activity>,
    pub next: Option<Activity>,
}

/// ¿La regla vale en este contexto (y tiene con qué llenar sus huecos)?
fn applies(r: &Rule, kind: Kind, c: &Context) -> bool {
    let k = match kind {
        Kind::Say => lines::SAY,
        Kind::Think => lines::THINK,
    };
    if r.kind & k == 0 {
        return false;
    }
    if r.acts != 0 && r.acts & (1 << c.activity as u32) == 0 {
        return false;
    }
    let m = c.now.minute();
    let in_hours = if r.from <= r.to {
        (r.from..r.to).contains(&m)
    } else {
        m >= r.from || m < r.to
    };
    if !in_hours {
        return false;
    }
    let day = match c.kind {
        DayKind::Workday => 1,
        DayKind::Saturday => 2,
        DayKind::Sunday => 4,
    };
    if r.days != 0 && r.days & day == 0 {
        return false;
    }
    if r.moods != 0 && r.moods & (1 << c.mood as u32) == 0 {
        return false;
    }
    let market_ok = match r.market {
        lines::UP => c.market > 0,
        lines::DOWN => c.market < 0,
        lines::FLAT => c.market == 0,
        lines::MOON => c.market >= 2,
        lines::DUMP => c.market <= -2,
        _ => true,
    };
    if !market_ok {
        return false;
    }
    if let Some((taste, min)) = r.taste
        && taste.of(c.personality) < min
    {
        return false;
    }
    (!r.text.contains("{did}") || c.did.is_some_and(|a| lines::did(a).is_some()))
        && (!r.text.contains("{next}") || c.next.is_some_and(|a| lines::next(a).is_some()))
}

fn lines_awake() -> u32 {
    lines::AWAKE
}

/// Elige y arma una frase de `kind` para el contexto, sin repetir las de
/// `recent`. `None` si no hay ninguna que aplique.
pub fn compose(kind: Kind, c: &Context, recent: &[u16], rng: &mut Rng) -> Option<Line> {
    let rules = lines::RULES;
    // Las propias de la actividad pesan más que las generales.
    let weight = |i: usize, r: &Rule| -> u32 {
        if recent.contains(&(i as u16)) || !applies(r, kind, c) {
            0
        } else if r.acts != 0 && r.acts.count_ones() <= 4 {
            // Propias de lo que está haciendo: las más probables.
            r.weight as u32 * 8
        } else if r.acts != 0 && r.acts != lines_awake() {
            r.weight as u32 * 3
        } else {
            r.weight as u32
        }
    };
    let total: u32 = rules.iter().enumerate().map(|(i, r)| weight(i, r)).sum();
    if total == 0 {
        return None;
    }
    let mut x = rng.next_u32() % total;
    let mut pick = 0;
    for (i, r) in rules.iter().enumerate() {
        let w = weight(i, r);
        if x < w {
            pick = i;
            break;
        }
        x -= w;
    }
    Some(fill(&rules[pick], pick as u16, kind, c, rng))
}

/// El saludo según el momento, como una frase más.
pub fn greet(g: Greeting, c: &Context, rng: &mut Rng) -> Line {
    let pool = lines::greetings(g, c.kind);
    let text = pool[rng.next_u32() as usize % pool.len()];
    let rule = Rule {
        text,
        ..lines::GREETING_RULE
    };
    fill(&rule, u16::MAX, Kind::Say, c, rng)
}

/// Arma la regla `i` de [`lines::RULES`], llenando sus huecos al azar (para
/// probar que todas entren en el globo).
pub fn compose_rule(i: usize, kind: Kind, c: &Context, rng: &mut Rng) -> Line {
    fill(&lines::RULES[i], i as u16, kind, c, rng)
}

/// Llena los huecos de la plantilla.
fn fill(r: &Rule, index: u16, kind: Kind, c: &Context, rng: &mut Rng) -> Line {
    let mut line = Line::new(kind, index);
    let mut rest = r.text;
    while let Some(open) = rest.find('{') {
        line.push(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let slot = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        let mut pick = |pool: &[&'static str]| pool[rng.next_u32() as usize % pool.len()];
        match slot {
            "pct" => {
                let n = if c.market.abs() >= 2 {
                    rng.range(15, 60)
                } else {
                    rng.range(2, 14)
                };
                line.push_num(n as u32);
            }
            "n" => {
                line.push_num(rng.range(2, 9) as u32);
            }
            "big" => {
                line.push_num(rng.range(12, 99) as u32);
            }
            "hour" => {
                let (h, m) = (c.now.hour() as u32, (c.now.minute() % 60) as u32);
                line.push_num(h);
                line.push(":");
                if m < 10 {
                    line.push("0");
                }
                line.push_num(m);
            }
            "weekday" => {
                line.push(lines::weekday_name(c.now.weekday()));
            }
            "did" => {
                line.push(c.did.and_then(lines::did).unwrap_or(""));
            }
            "next" => {
                line.push(c.next.and_then(lines::next).unwrap_or(""));
            }
            other => {
                let s = match lines::pool(other) {
                    Some(p) => pick(p),
                    None => "",
                };
                line.push(s);
            }
        }
    }
    line.push(rest);
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routine::{day_kind, mood};

    fn ctx<'a>(p: &'a Personality, act: Activity, h: u8, market: i8) -> Context<'a> {
        let now = LocalTime::new(20_002, h, 15);
        Context {
            activity: act,
            now,
            kind: day_kind(now.day),
            mood: mood(p, now.day),
            personality: p,
            market,
            did: Some(Activity::Clean),
            next: Some(Activity::Cook),
        }
    }

    #[test]
    fn every_line_is_short_and_complete() {
        // Todas las reglas, armadas muchas veces (los huecos más largos
        // salen alguna vez), son cortas y no les queda ningún hueco. Que
        // entren en el globo con su letra lo prueba `habitat::bubble`.
        let p = Personality::from_seed(1);
        let mut rng = Rng::new(3);
        let c = ctx(&p, Activity::Code, 10, 2);
        for (i, r) in lines::RULES.iter().enumerate() {
            assert!(!r.text.is_empty());
            for _ in 0..40 {
                let l = compose_rule(i, Kind::Say, &c, &mut rng);
                assert!(
                    l.as_str().chars().count() <= MAX_CHARS,
                    "muy larga: {:?}",
                    l.as_str()
                );
                assert!(!l.as_str().contains('{'), "hueco sin llenar: {}", r.text);
            }
        }
    }

    #[test]
    fn every_activity_has_things_to_say_and_think() {
        let p = Personality::from_seed(2);
        let mut rng = Rng::new(4);
        for act in Activity::ALL {
            for h in [9, 15, 22] {
                let c = ctx(&p, act, h, 0);
                assert!(
                    compose(Kind::Say, &c, &[], &mut rng).is_some(),
                    "{act:?} {h}h: nada que decir"
                );
            }
            if act.is_present() && !act.is_asleep() {
                let c = ctx(&p, act, 15, 1);
                assert!(
                    compose(Kind::Think, &c, &[], &mut rng).is_some(),
                    "{act:?}: nada que pensar"
                );
            }
        }
    }

    #[test]
    fn lines_match_the_moment() {
        let p = Personality::from_seed(3);
        let mut rng = Rng::new(5);
        // Con el mercado en caída, nada de festejos de suba.
        for _ in 0..200 {
            let c = ctx(&p, Activity::Trading, 11, -2);
            let l = compose(Kind::Say, &c, &[], &mut rng).unwrap();
            let r = &lines::RULES[l.rule as usize];
            assert!(r.market != lines::UP && r.market != lines::MOON, "{l:?}");
        }
        // Programando dice cosas de programar (o generales), no del mate.
        for _ in 0..200 {
            let c = ctx(&p, Activity::Code, 11, 0);
            let l = compose(Kind::Say, &c, &[], &mut rng).unwrap();
            let r = &lines::RULES[l.rule as usize];
            assert!(
                r.acts == 0 || r.acts & (1 << Activity::Code as u32) != 0,
                "{l:?}"
            );
        }
    }

    #[test]
    fn there_is_a_lot_to_say() {
        // Variedad: cientos de reglas y, por actividad, muchas frases distintas.
        assert!(
            lines::RULES.len() >= 400,
            "solo {} reglas",
            lines::RULES.len()
        );
        let p = Personality::from_seed(4);
        let mut rng = Rng::new(6);
        for act in [
            Activity::Code,
            Activity::Trading,
            Activity::Games,
            Activity::Mate,
        ] {
            let mut seen = std::collections::HashSet::new();
            for _ in 0..400 {
                let c = ctx(&p, act, 16, 1);
                let l = compose(Kind::Say, &c, &[], &mut rng).unwrap();
                seen.insert(std::string::String::from(l.as_str()));
            }
            assert!(
                seen.len() >= 60,
                "{act:?}: solo {} frases distintas",
                seen.len()
            );
        }
    }

    #[test]
    fn does_not_repeat_recent_lines() {
        let p = Personality::from_seed(5);
        let mut rng = Rng::new(7);
        let mut recent = std::vec::Vec::new();
        for _ in 0..30 {
            let c = ctx(&p, Activity::Code, 11, 0);
            let l = compose(Kind::Think, &c, &recent, &mut rng).unwrap();
            assert!(!recent.contains(&l.rule));
            recent.push(l.rule);
        }
    }
}
