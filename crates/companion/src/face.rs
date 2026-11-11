//! La cara: qué ojos y qué boca pone el personaje según la emoción, cómo
//! cambia de expresión y el parpadeo encima.
//!
//! [`Eyes`] y [`Mouth`] son las celdas de las tiras de expresiones
//! (`assets/face/eyes.png` y `mouth.png`), en el mismo orden.
//!
//! Una cara viva no se queda quieta: cada [`Emotion`] tiene un repertorio de
//! expresiones ([`Emotion::palette`]) con lo que suele durar cada una (una
//! sonrisa grande, menos de un segundo; una cara tranquila, unos segundos),
//! y [`FaceAnimator`] va eligiendo la siguiente al azar según su peso. La
//! boca pasa por las bocas intermedias en vez de saltar (de la sonrisa
//! grande a la línea pasa por las sonrisas más chicas), salvo al hablar,
//! donde cambia de sílaba en sílaba. Y parpadea como una persona: a
//! intervalos irregulares, cerrando rápido y abriendo más lento, a veces dos
//! veces seguidas y siempre al cambiar de emoción (tapa el salto).

use crate::rng::Rng;

/// Celdas de la tira de ojos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Eyes {
    Normal = 0,
    /// Cejas arriba: contento, interesado.
    Happy = 1,
    Closed = 2,
    Mad = 3,
    /// Párpados a media altura: desconfiado, con sueño, o a mitad de un parpadeo.
    Suspicious = 4,
    Crying = 5,
}

/// Celdas de la tira de bocas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Mouth {
    NaturalSmile = 0,
    Smile = 1,
    SmileBig = 2,
    Neutral = 3,
    /// Mueca con los dientes juntos: incómodo, triste, una sílaba.
    Sad = 4,
    Mad = 5,
    /// Masticando: la boca de costado y el cachete que se infla y desinfla
    /// (`Chew2` es el cachete lleno).
    Chew1 = 6,
    Chew2 = 7,
    Chew3 = 8,
    /// Agotado: boca abierta con la lengua afuera (después del ejercicio).
    Exhausted = 9,
}

impl Mouth {
    /// Escalón de la boca, de los dientes apretados a la sonrisa grande:
    /// para cambiar de una a otra se pasa por las del medio.
    const fn rung(self) -> i32 {
        match self {
            Mouth::Mad => 0,
            Mouth::Sad => 1,
            Mouth::Neutral => 2,
            Mouth::NaturalSmile => 3,
            Mouth::Smile => 4,
            Mouth::SmileBig | Mouth::Exhausted => 5,
            // Cerrada, como la línea.
            Mouth::Chew1 | Mouth::Chew2 | Mouth::Chew3 => 2,
        }
    }

    const LADDER: [Mouth; 6] = [
        Mouth::Mad,
        Mouth::Sad,
        Mouth::Neutral,
        Mouth::NaturalSmile,
        Mouth::Smile,
        Mouth::SmileBig,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Face {
    pub eyes: Eyes,
    pub mouth: Mouth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emotion {
    /// Tranquilo, con una sonrisa leve: la cara por defecto.
    Content,
    Happy,
    /// Muy contento: te saluda, te extrañó.
    Joyful,
    Neutral,
    /// Esforzándose (limpiar, elongar).
    Determined,
    /// Haciendo ejercicio: esfuerzo, dientes apretados y la lengua afuera.
    Exerting,
    Sad,
    Crying,
    Angry,
    Suspicious,
    /// Medio dormido: párpados pesados y parpadeo lento.
    Sleepy,
    Asleep,
    /// Concentrado (programar, leer): serio, con alguna sonrisa cuando algo
    /// sale y algún ceño fruncido cuando no.
    Focused,
    /// Divirtiéndose (bailar, jugar, una película): sonrisas y caras pícaras.
    Playful,
    /// Relajado (mate, mirar el cielo): ojos entornados, sonrisa tranquila.
    Relaxed,
    /// Hablando (reunión, teléfono): la boca va de sílaba en sílaba.
    Chatty,
}

/// Una expresión del repertorio de una emoción.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Expr {
    pub eyes: Eyes,
    pub mouth: Mouth,
    /// Cuánto se elige frente a las otras.
    pub weight: u16,
    /// Cuánto dura (mín, máx), en milisegundos.
    pub hold: (u16, u16),
    /// Cambia de golpe (sílabas al hablar) en vez de pasar por las bocas
    /// intermedias.
    pub snap: bool,
}

const fn x(eyes: Eyes, mouth: Mouth, weight: u16, hold: (u16, u16)) -> Expr {
    Expr {
        eyes,
        mouth,
        weight,
        hold,
        snap: false,
    }
}

/// Una sílaba: de golpe y corta.
const fn syl(mouth: Mouth, weight: u16) -> Expr {
    Expr {
        eyes: Eyes::Normal,
        mouth,
        weight,
        hold: (90, 200),
        snap: true,
    }
}

use Eyes::{Closed, Crying as Tear, Happy as Up, Mad as Brow, Normal as Open, Suspicious as Half};
use Mouth::{
    Exhausted, Mad as Grit, NaturalSmile as Soft, Neutral as Line, Sad as Grimace, Smile,
    SmileBig as Grin,
};

const CONTENT: &[Expr] = &[
    x(Open, Soft, 40, (2000, 6000)),
    x(Open, Line, 18, (1500, 4000)),
    x(Open, Smile, 12, (800, 2500)),
    x(Up, Smile, 8, (600, 1800)),
    x(Up, Soft, 8, (800, 2200)),
    x(Half, Line, 6, (700, 1600)),
    x(Half, Soft, 5, (900, 2200)),
    x(Open, Grin, 3, (300, 800)),
];
const HAPPY: &[Expr] = &[
    x(Up, Smile, 28, (1000, 3000)),
    x(Open, Smile, 20, (1200, 3500)),
    x(Open, Soft, 18, (1500, 3500)),
    x(Up, Grin, 12, (400, 1300)),
    x(Up, Soft, 10, (1000, 2500)),
    x(Open, Grin, 7, (400, 1000)),
    x(Half, Smile, 5, (600, 1500)),
];
const JOYFUL: &[Expr] = &[
    x(Up, Grin, 40, (700, 2000)),
    x(Up, Smile, 30, (600, 1600)),
    x(Open, Grin, 18, (500, 1400)),
    x(Open, Smile, 12, (500, 1200)),
];
const NEUTRAL: &[Expr] = &[
    x(Open, Line, 45, (2000, 6000)),
    x(Open, Soft, 20, (1500, 4000)),
    x(Half, Line, 15, (800, 2000)),
    x(Up, Line, 8, (600, 1500)),
    x(Open, Smile, 5, (500, 1200)),
    x(Up, Soft, 5, (600, 1500)),
    x(Open, Grimace, 2, (300, 700)),
];
const DETERMINED: &[Expr] = &[
    x(Brow, Grin, 28, (800, 2500)),
    x(Brow, Line, 24, (1000, 3000)),
    x(Open, Grin, 14, (600, 1600)),
    x(Brow, Grit, 12, (400, 1100)),
    x(Half, Line, 8, (600, 1500)),
    x(Open, Line, 8, (800, 2000)),
    x(Brow, Grimace, 6, (400, 900)),
];
const EXERTING: &[Expr] = &[
    x(Brow, Grit, 26, (700, 2000)),
    x(Half, Exhausted, 22, (800, 2200)),
    x(Brow, Grin, 16, (500, 1500)),
    x(Closed, Exhausted, 12, (400, 1100)),
    x(Brow, Exhausted, 10, (500, 1400)),
    x(Brow, Grimace, 8, (400, 900)),
    x(Half, Line, 6, (500, 1200)),
];
const SAD: &[Expr] = &[
    x(Open, Grimace, 35, (2000, 5000)),
    x(Half, Grimace, 20, (1500, 4000)),
    x(Open, Line, 20, (1500, 4000)),
    x(Tear, Grimace, 10, (800, 2000)),
    x(Half, Line, 10, (1000, 2500)),
    x(Open, Soft, 5, (400, 900)),
];
const CRYING: &[Expr] = &[
    x(Tear, Grimace, 55, (1500, 4000)),
    x(Tear, Grit, 15, (500, 1200)),
    x(Open, Grimace, 15, (800, 2000)),
    x(Half, Grimace, 15, (800, 2000)),
];
const ANGRY: &[Expr] = &[
    x(Brow, Grit, 45, (1200, 3500)),
    x(Brow, Line, 25, (1000, 3000)),
    x(Half, Grit, 15, (600, 1500)),
    x(Brow, Grimace, 10, (500, 1200)),
    x(Open, Grit, 5, (400, 900)),
];
const SUSPICIOUS: &[Expr] = &[
    x(Half, Line, 40, (1500, 4000)),
    x(Half, Grimace, 15, (600, 1500)),
    x(Open, Line, 18, (1000, 3000)),
    x(Brow, Line, 12, (800, 2000)),
    x(Half, Soft, 10, (600, 1500)),
    x(Up, Line, 5, (400, 900)),
];
const SLEEPY: &[Expr] = &[
    x(Half, Line, 45, (2000, 5000)),
    x(Closed, Line, 12, (600, 1800)),
    x(Open, Line, 13, (800, 2000)),
    x(Half, Grimace, 10, (700, 1500)),
    x(Half, Soft, 12, (1000, 2500)),
    x(Up, Line, 8, (300, 800)),
];
const ASLEEP: &[Expr] = &[
    x(Closed, Line, 65, (4000, 12000)),
    x(Closed, Soft, 22, (2000, 6000)),
    x(Closed, Grimace, 8, (800, 2000)),
    x(Closed, Smile, 5, (600, 1500)),
];
const FOCUSED: &[Expr] = &[
    x(Open, Line, 40, (2000, 5000)),
    x(Half, Line, 12, (1000, 3000)),
    x(Brow, Line, 10, (800, 2000)),
    x(Open, Soft, 10, (800, 2000)),
    x(Half, Grimace, 6, (500, 1200)),
    x(Up, Line, 6, (400, 1000)),
    x(Up, Smile, 5, (500, 1300)),
    x(Brow, Grit, 3, (300, 800)),
    x(Open, Grin, 2, (300, 700)),
];
const PLAYFUL: &[Expr] = &[
    x(Up, Grin, 22, (600, 1800)),
    x(Up, Smile, 20, (800, 2200)),
    x(Open, Grin, 14, (500, 1400)),
    x(Open, Smile, 14, (800, 2000)),
    x(Brow, Grin, 10, (500, 1300)),
    x(Half, Smile, 10, (600, 1500)),
    x(Half, Soft, 6, (800, 1800)),
    x(Open, Grimace, 4, (300, 700)),
];
const RELAXED: &[Expr] = &[
    x(Open, Soft, 38, (2000, 5000)),
    x(Closed, Soft, 18, (900, 2500)),
    x(Up, Soft, 14, (1000, 2500)),
    x(Open, Line, 12, (1500, 4000)),
    x(Half, Soft, 8, (1000, 2500)),
    x(Open, Smile, 6, (600, 1500)),
    x(Up, Smile, 4, (500, 1200)),
];
const CHATTY: &[Expr] = &[
    // Sílabas...
    syl(Line, 30),
    syl(Soft, 22),
    syl(Grimace, 18),
    syl(Smile, 16),
    syl(Grin, 4),
    // ...y pausas: escucha, se ríe, piensa.
    x(Open, Soft, 8, (500, 1500)),
    x(Up, Smile, 4, (500, 1300)),
    x(Half, Line, 4, (500, 1200)),
    x(Up, Line, 3, (400, 900)),
];

/// Cómo parpadea con cada emoción. Tiempos en milisegundos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlinkStyle {
    /// Tiempo con los ojos abiertos entre parpadeos (mín, máx).
    pub interval: (u32, u32),
    /// Bajando el párpado (medio cerrado).
    pub closing: u32,
    /// Cerrado del todo (mín, máx).
    pub closed: (u32, u32),
    /// Subiendo el párpado: más lento que bajarlo.
    pub opening: u32,
    /// Probabilidad (%) de un segundo parpadeo enseguida.
    pub double_pct: u32,
}

impl Emotion {
    /// Las expresiones que pone con esta emoción.
    pub const fn palette(self) -> &'static [Expr] {
        match self {
            Emotion::Content => CONTENT,
            Emotion::Happy => HAPPY,
            Emotion::Joyful => JOYFUL,
            Emotion::Neutral => NEUTRAL,
            Emotion::Determined => DETERMINED,
            Emotion::Exerting => EXERTING,
            Emotion::Sad => SAD,
            Emotion::Crying => CRYING,
            Emotion::Angry => ANGRY,
            Emotion::Suspicious => SUSPICIOUS,
            Emotion::Sleepy => SLEEPY,
            Emotion::Asleep => ASLEEP,
            Emotion::Focused => FOCUSED,
            Emotion::Playful => PLAYFUL,
            Emotion::Relaxed => RELAXED,
            Emotion::Chatty => CHATTY,
        }
    }

    /// La expresión típica (la primera del repertorio).
    pub const fn face(self) -> Face {
        let e = self.palette()[0];
        Face {
            eyes: e.eyes,
            mouth: e.mouth,
        }
    }

    /// `None`: no parpadea (ya tiene los ojos cerrados).
    pub const fn blink(self) -> Option<BlinkStyle> {
        const NORMAL: BlinkStyle = BlinkStyle {
            interval: (2_000, 6_000),
            closing: 50,
            closed: (60, 90),
            opening: 80,
            double_pct: 12,
        };
        Some(match self {
            Emotion::Asleep => return None,
            Emotion::Sleepy => BlinkStyle {
                interval: (1_500, 4_000),
                closing: 120,
                closed: (250, 700),
                opening: 200,
                double_pct: 20,
            },
            // Enojado o concentrado: mira fijo y parpadea poco.
            Emotion::Angry
            | Emotion::Determined
            | Emotion::Exerting
            | Emotion::Suspicious
            | Emotion::Focused => BlinkStyle {
                interval: (4_000, 9_000),
                double_pct: 4,
                ..NORMAL
            },
            Emotion::Sad | Emotion::Crying => BlinkStyle {
                interval: (2_500, 7_000),
                closed: (100, 180),
                opening: 120,
                ..NORMAL
            },
            Emotion::Relaxed => BlinkStyle {
                interval: (3_000, 7_000),
                closed: (90, 160),
                opening: 120,
                ..NORMAL
            },
            // Hablando y divirtiéndose se parpadea más.
            Emotion::Chatty | Emotion::Playful | Emotion::Joyful => BlinkStyle {
                interval: (1_500, 4_500),
                double_pct: 18,
                ..NORMAL
            },
            _ => NORMAL,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Open,
    Closing,
    Closed,
    Opening,
}

/// Pasos máximos de una transición (la boca de punta a punta, 5 escalones).
const MAX_STEPS: usize = 6;
/// Duración de cada paso intermedio de la boca (mín, máx), ms.
const STEP_MS: (u32, u32) = (60, 110);

pub struct FaceAnimator {
    emotion: Emotion,
    /// La expresión que se ve (sin el parpadeo) y lo que le queda.
    shown: Face,
    hold_ms: u32,
    /// Transición en curso hacia la próxima expresión: caras intermedias y
    /// lo que le queda a la actual.
    path: [Face; MAX_STEPS],
    path_len: usize,
    path_i: usize,
    step_ms: u32,
    /// Duración de la expresión a la que se está yendo.
    next_hold: u32,
    phase: Phase,
    /// Lo que falta para pasar a la siguiente fase del parpadeo.
    left_ms: u32,
    double: bool,
    rng: Rng,
}

impl FaceAnimator {
    pub fn new(emotion: Emotion, seed: u32) -> Self {
        let mut a = Self {
            emotion,
            shown: emotion.face(),
            hold_ms: 0,
            path: [emotion.face(); MAX_STEPS],
            path_len: 0,
            path_i: 0,
            step_ms: 0,
            next_hold: 0,
            phase: Phase::Open,
            left_ms: 0,
            double: false,
            rng: Rng::new(seed),
        };
        a.hold_ms = a.rand((1_000, 3_000));
        a.left_ms = a.next_interval();
        a
    }

    pub fn emotion(&self) -> Emotion {
        self.emotion
    }

    /// Cambia de emoción. El cambio se hace con un parpadeo, como cuando
    /// alguien cambia de cara: tapa el salto entre expresiones.
    pub fn set_emotion(&mut self, emotion: Emotion) {
        if emotion == self.emotion {
            return;
        }
        let was_closed = self.emotion.blink().is_none();
        self.emotion = emotion;
        // La primera expresión de la emoción nueva, enseguida.
        self.pick_next(Some(emotion.palette()[0]));
        if emotion.blink().is_some() && !was_closed && self.phase == Phase::Open {
            self.start_blink();
        } else if self.phase == Phase::Open {
            self.left_ms = self.next_interval();
        }
    }

    /// Avanza el tiempo y devuelve la cara a mostrar.
    pub fn update(&mut self, dt_ms: u32) -> Face {
        self.update_expression(dt_ms);
        self.update_blink(dt_ms);
        self.face()
    }

    fn update_expression(&mut self, dt_ms: u32) {
        let mut dt = dt_ms;
        while dt > 0 {
            if self.path_i < self.path_len {
                if dt < self.step_ms {
                    self.step_ms -= dt;
                    break;
                }
                dt -= self.step_ms;
                self.shown = self.path[self.path_i];
                self.path_i += 1;
                if self.path_i < self.path_len {
                    self.step_ms = self.rand(STEP_MS);
                } else {
                    self.hold_ms = self.next_hold;
                }
                continue;
            }
            if dt < self.hold_ms {
                self.hold_ms -= dt;
                break;
            }
            dt -= self.hold_ms;
            self.pick_next(None);
        }
    }

    /// Elige la próxima expresión (o va a `forced`) y arma el camino hasta ella.
    fn pick_next(&mut self, forced: Option<Expr>) {
        let palette = self.emotion.palette();
        let e = forced.unwrap_or_else(|| {
            // Al azar según el peso, sin repetir la cara que ya tiene.
            let shown = self.shown;
            let same = |e: &Expr| e.eyes == shown.eyes && e.mouth == shown.mouth;
            let total: u32 = palette
                .iter()
                .filter(|e| !same(e))
                .map(|e| e.weight as u32)
                .sum();
            let mut r = self.rng.next_u32() % total.max(1);
            let mut pick = palette[0];
            for &e in palette.iter().filter(|e| !same(e)) {
                pick = e;
                if r < e.weight as u32 {
                    break;
                }
                r -= e.weight as u32;
            }
            pick
        });
        let target = Face {
            eyes: e.eyes,
            mouth: e.mouth,
        };
        self.next_hold = self.rand((e.hold.0 as u32, e.hold.1 as u32));
        // La boca pasa por los escalones del medio; los ojos cambian a mitad
        // de camino.
        let (a, b) = (self.shown.mouth.rung(), target.mouth.rung());
        let steps = if e.snap {
            1
        } else {
            ((b - a).unsigned_abs() as usize).max(1)
        };
        let steps = steps.min(MAX_STEPS);
        for k in 1..=steps {
            let rung = if e.snap || k == steps {
                b
            } else {
                a + (b - a).signum() * k as i32
            };
            self.path[k - 1] = Face {
                eyes: if k * 2 > steps {
                    target.eyes
                } else {
                    self.shown.eyes
                },
                mouth: Mouth::LADDER[rung as usize],
            };
        }
        self.path[steps - 1] = target;
        self.path_len = steps;
        self.path_i = 0;
        self.step_ms = if e.snap || forced.is_some() {
            0
        } else {
            self.rand(STEP_MS)
        };
    }

    fn update_blink(&mut self, dt_ms: u32) {
        let mut dt = dt_ms;
        // Un `dt` grande puede atravesar varias fases.
        while dt > 0 {
            let Some(style) = self.emotion.blink() else {
                self.phase = Phase::Open;
                break;
            };
            if dt < self.left_ms {
                self.left_ms -= dt;
                break;
            }
            dt -= self.left_ms;
            match self.phase {
                Phase::Open => self.start_blink(),
                Phase::Closing => {
                    self.phase = Phase::Closed;
                    self.left_ms = self.rand(style.closed);
                }
                Phase::Closed => {
                    self.phase = Phase::Opening;
                    self.left_ms = style.opening;
                }
                Phase::Opening => {
                    self.phase = Phase::Open;
                    self.left_ms = if self.double {
                        self.double = false;
                        // Segundo parpadeo casi enseguida.
                        self.rand((120, 250))
                    } else {
                        self.next_interval()
                    };
                }
            }
        }
    }

    pub fn face(&self) -> Face {
        let base = self.shown;
        let eyes = match self.phase {
            Phase::Open => base.eyes,
            Phase::Closed => Eyes::Closed,
            // A mitad de camino, párpado a media altura; con cejas de enojo o
            // ojos ya entornados se pasa directo a cerrado.
            Phase::Closing | Phase::Opening => match base.eyes {
                Eyes::Normal | Eyes::Happy | Eyes::Crying => Eyes::Suspicious,
                _ => Eyes::Closed,
            },
        };
        Face { eyes, ..base }
    }

    fn start_blink(&mut self) {
        let Some(style) = self.emotion.blink() else {
            return;
        };
        self.phase = Phase::Closing;
        self.left_ms = style.closing;
        self.double = self.rng.chance(style.double_pct);
    }

    fn next_interval(&mut self) -> u32 {
        match self.emotion.blink() {
            Some(style) => self.rand(style.interval),
            None => u32::MAX,
        }
    }

    fn rand(&mut self, (lo, hi): (u32, u32)) -> u32 {
        self.rng.range(lo as i32, hi as i32) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Emotion; 16] = [
        Emotion::Content,
        Emotion::Happy,
        Emotion::Joyful,
        Emotion::Neutral,
        Emotion::Determined,
        Emotion::Exerting,
        Emotion::Sad,
        Emotion::Crying,
        Emotion::Angry,
        Emotion::Suspicious,
        Emotion::Sleepy,
        Emotion::Asleep,
        Emotion::Focused,
        Emotion::Playful,
        Emotion::Relaxed,
        Emotion::Chatty,
    ];

    /// Simula `ms` a 30 fps y devuelve la cara de cada fotograma.
    fn run(a: &mut FaceAnimator, ms: u32) -> std::vec::Vec<Face> {
        (0..ms / 33).map(|_| a.update(33)).collect()
    }

    fn blinks(a: &mut FaceAnimator, ms: u32) -> usize {
        let mut n = 0;
        let mut was = a.phase;
        for _ in 0..ms / 33 {
            a.update(33);
            if was == Phase::Open && a.phase != Phase::Open {
                n += 1;
            }
            was = a.phase;
        }
        n
    }

    #[test]
    fn palettes_are_sane() {
        for e in ALL {
            for x in e.palette() {
                assert!(x.weight > 0 && x.hold.0 <= x.hold.1, "{e:?} {x:?}");
            }
        }
        assert!(
            Emotion::Asleep
                .palette()
                .iter()
                .all(|x| x.eyes == Eyes::Closed)
        );
    }

    #[test]
    fn blinks_like_a_person() {
        let mut a = FaceAnimator::new(Emotion::Content, 1);
        let n = blinks(&mut a, 60_000);
        // Persona tranquila: ~12-25 por minuto (contando dobles).
        assert!((10..=30).contains(&n), "{n} parpadeos por minuto");
        let frames = run(&mut a, 60_000);
        let open = frames.iter().filter(|f| f.eyes != Eyes::Closed).count();
        assert!(
            open * 100 / frames.len() > 90,
            "abiertos {open} de {}",
            frames.len()
        );
    }

    #[test]
    fn a_blink_is_short_and_opening_is_slower() {
        let mut a = FaceAnimator::new(Emotion::Content, 2);
        while a.phase == Phase::Open {
            a.update(1);
        }
        let mut ms = 1;
        while a.phase != Phase::Open {
            a.update(1);
            ms += 1;
        }
        let s = Emotion::Content.blink().unwrap();
        assert!(
            ms >= s.closing + s.closed.0 + s.opening
                && ms <= s.closing + s.closed.1 + s.opening + 1
        );
        assert!(s.opening > s.closing);
    }

    #[test]
    fn asleep_never_opens_and_sleepy_lingers_closed() {
        let mut a = FaceAnimator::new(Emotion::Asleep, 3);
        assert!(run(&mut a, 20_000).iter().all(|f| f.eyes == Eyes::Closed));

        let mut sleepy = FaceAnimator::new(Emotion::Sleepy, 3);
        let mut awake = FaceAnimator::new(Emotion::Content, 3);
        let closed = |f: &[Face]| f.iter().filter(|f| f.eyes == Eyes::Closed).count();
        let (s, w) = (
            closed(&run(&mut sleepy, 60_000)),
            closed(&run(&mut awake, 60_000)),
        );
        assert!(
            s > 3 * w,
            "con sueño {s} fotogramas cerrados, despierto {w}"
        );
    }

    #[test]
    fn angry_stares() {
        let mut angry = FaceAnimator::new(Emotion::Angry, 4);
        let mut calm = FaceAnimator::new(Emotion::Content, 4);
        assert!(blinks(&mut angry, 120_000) < blinks(&mut calm, 120_000));
    }

    #[test]
    fn changing_emotion_blinks_and_moves_to_its_face() {
        let mut a = FaceAnimator::new(Emotion::Content, 5);
        a.update(10);
        a.set_emotion(Emotion::Angry);
        let f = a.update(1);
        assert_ne!(f.eyes, Eyes::Mad, "los ojos cambian tras el parpadeo");
        let frames = run(&mut a, 400);
        assert_eq!(*frames.last().unwrap(), Emotion::Angry.face());

        // Al despertarse no hace falta parpadear: abre los ojos.
        let mut b = FaceAnimator::new(Emotion::Asleep, 6);
        b.set_emotion(Emotion::Sleepy);
        assert_eq!(b.update(1).eyes, Eyes::Suspicious);
    }

    #[test]
    fn the_face_keeps_changing_within_its_mood() {
        for e in ALL {
            let mut a = FaceAnimator::new(e, 7);
            let palette = e.palette();
            let longest = palette.iter().map(|x| x.hold.1 as u32).max().unwrap();
            let mut changes = 0;
            let mut distinct = std::vec::Vec::new();
            let (mut held, mut last) = (0u32, a.shown);
            for _ in 0..(120_000 / 33) {
                a.update(33);
                if a.shown != last {
                    changes += 1;
                    held = 0;
                    last = a.shown;
                    if !distinct.contains(&last) {
                        distinct.push(last);
                    }
                } else {
                    held += 33;
                    // Ninguna expresión se queda más de lo que dice el repertorio.
                    assert!(held <= longest + 200, "{e:?}: {last:?} {held} ms");
                }
                // Lo que se ve es del repertorio (o un paso intermedio de la boca).
                if a.path_i >= a.path_len {
                    assert!(
                        palette
                            .iter()
                            .any(|x| x.eyes == a.shown.eyes && x.mouth == a.shown.mouth),
                        "{e:?}: {:?} no está en su repertorio",
                        a.shown
                    );
                }
            }
            let min = if e == Emotion::Asleep { 6 } else { 24 };
            assert!(changes >= min, "{e:?}: {changes} cambios en 2 minutos");
            assert!(distinct.len() >= 3, "{e:?}: solo {} caras", distinct.len());
        }
    }

    #[test]
    fn the_mouth_moves_one_step_at_a_time() {
        for e in ALL {
            if e == Emotion::Chatty {
                continue;
            }
            let mut a = FaceAnimator::new(e, 8);
            let frames = run(&mut a, 60_000);
            for w in frames.windows(2) {
                let d = (w[0].mouth.rung() - w[1].mouth.rung()).abs();
                assert!(
                    d <= 1,
                    "{e:?}: {:?} -> {:?} de golpe",
                    w[0].mouth,
                    w[1].mouth
                );
            }
        }
    }

    #[test]
    fn talking_moves_the_mouth_all_the_time() {
        let mut a = FaceAnimator::new(Emotion::Chatty, 9);
        let frames = run(&mut a, 60_000);
        let changes = frames
            .windows(2)
            .filter(|w| w[0].mouth != w[1].mouth)
            .count();
        assert!(changes > 150, "{changes} cambios de boca en un minuto");
    }

    #[test]
    fn same_seed_same_face() {
        let mut a = FaceAnimator::new(Emotion::Content, 9);
        let mut b = FaceAnimator::new(Emotion::Content, 9);
        assert_eq!(run(&mut a, 30_000), run(&mut b, 30_000));
    }
}
