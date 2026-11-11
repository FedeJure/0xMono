//! Cosas graciosas que le pasan de vez en cuando:
//!
//! - **Durmiendo**, se tira un pedo: una nubecita verde sale de abajo de la
//!   manta y sonríe dormido.
//! - **Cocinando**, la olla a veces larga humo (lo espanta con la mano),
//!   hace una micro explosión (salta del susto) o se prende fuego: se
//!   asusta, va a buscar el matafuego a la pared, lo apaga y lo devuelve.
//! - **Haciendo ejercicio**, transpira: gotas que saltan de la cabeza.
//!
//! [`Gags`] decide cuándo (al azar, cada uno o dos minutos), cambia el plan
//! mientras dura (ir a buscar el matafuego), pone la reacción encima del
//! cuerpo y la cara, y maneja los efectos ([`Fx`]).

use assets::mono_anims::clip;
use companion::{Activity, Eyes, Face, Mouth};

use crate::fx::{Fx, Kind};
use crate::plan::{Plan, Posture, Prop, Target};

const M: f32 = 1024.0;

/// Lo que pasa en la cocina.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Cook {
    Calm,
    /// La olla larga humo (segundos desde que empezó).
    Smoke(f32),
    /// Micro explosión.
    Pop(f32),
    /// Se prendió fuego: el susto.
    Burning(f32),
    /// Va a buscar el matafuego.
    Fetch,
    /// Vuelve a la olla con el matafuego.
    Back,
    Spray(f32),
    /// Devuelve el matafuego a la pared.
    Return,
}

/// Un gag para disparar a mano (tecla en el simulador, o para revisar).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gag {
    Fart,
    Smoke,
    Pop,
    Fire,
}

/// Lo que los gags necesitan saber del mono en este momento.
pub struct Scene {
    pub act: Activity,
    /// Ya está haciendo la actividad (no yendo ni acomodándose).
    pub settled: bool,
    /// Segundos que lleva quieto en su lugar.
    pub still_t: f32,
    pub target: Option<Target>,
    pub lying: bool,
    /// Posiciones en el mundo (Q8) del último fotograma dibujado.
    pub hips: Option<[i32; 3]>,
    /// La frente, a la izquierda y a la derecha.
    pub brow: [Option<[i32; 3]>; 2],
    /// La boquilla del matafuego: la mano izquierda, que sostiene la manguera.
    pub nozzle: Option<[i32; 3]>,
    /// La olla (mundo, Q8).
    pub pot: [i32; 3],
}

pub struct Gags {
    pub fx: Fx,
    cook: Cook,
    /// Tiene el matafuego en la mano.
    pub carrying: bool,
    /// Segundos hasta el próximo gag de la actividad actual.
    next: f32,
    act: Option<Activity>,
    /// Cara que pone (y hasta cuándo, segundos).
    face: Option<(Face, f32)>,
    /// Reacción encima del cuerpo (clip) y hasta cuándo.
    overlay: Option<(usize, f32)>,
    /// Pedo en curso: segundos desde que empezó.
    fart: Option<f32>,
    /// Para soltar partículas a intervalos.
    emit: f32,
    want: Option<Gag>,
    seed: u32,
    t: f32,
}

impl Default for Gags {
    fn default() -> Self {
        Self::new()
    }
}

impl Gags {
    pub fn new() -> Self {
        Self {
            fx: Fx::new(),
            cook: Cook::Calm,
            carrying: false,
            next: 60.0,
            act: None,
            face: None,
            overlay: None,
            fart: None,
            emit: 0.0,
            want: None,
            seed: 0xF4A7,
            t: 0.0,
        }
    }

    /// Dispara un gag ya (si la actividad lo permite).
    pub fn trigger(&mut self, gag: Gag) {
        self.want = Some(gag);
    }

    fn rand(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rand()
    }

    fn say(&mut self, eyes: Eyes, mouth: Mouth, secs: f32) {
        self.face = Some((Face { eyes, mouth }, self.t + secs));
    }

    fn react(&mut self, clip: usize, secs: f32) {
        self.overlay = Some((clip, self.t + secs));
    }

    /// Cada cuánto, en segundos, le pasa algo en esta actividad.
    fn interval(&mut self, act: Activity) -> f32 {
        match act {
            Activity::Sleep | Activity::Nap => self.range(50.0, 160.0),
            _ => self.range(35.0, 100.0),
        }
    }

    pub fn update(&mut self, s: &Scene, dt: f32) {
        self.t += dt;
        self.fx.update(dt);
        if self.act != Some(s.act) {
            self.act = Some(s.act);
            self.next = self.interval(s.act);
            self.fart = None;
            if !self.carrying {
                // Dejó de cocinar: el fuego (si lo había) se apagó solo.
                self.cook = Cook::Calm;
                self.fx.fire = None;
            }
        }
        if self.face.is_some_and(|(_, until)| self.t > until) {
            self.face = None;
        }
        if self.overlay.is_some_and(|(_, until)| self.t > until) {
            self.overlay = None;
        }
        let want = self.want.take();
        let due = s.settled && {
            self.next -= dt;
            self.next <= 0.0
        };

        match s.act {
            Activity::Sleep | Activity::Nap => {
                if s.lying && s.settled && (due || want == Some(Gag::Fart)) && self.fart.is_none() {
                    self.fart = Some(0.0);
                    self.next = self.interval(s.act);
                }
            }
            Activity::Exercise if s.settled => self.sweat(s, dt),
            _ => {}
        }
        self.update_fart(s, dt);
        self.update_cook(s, dt, due, want);
    }

    fn update_fart(&mut self, s: &Scene, dt: f32) {
        let Some(t) = self.fart else { return };
        let Some(hips) = s.hips else { return };
        self.fart = Some(t + dt);
        self.emit -= dt;
        // Unas bocanadas en el primer segundo, desde la cola (abajo de la
        // manta) hacia arriba.
        if t < 1.0 && self.emit <= 0.0 {
            self.emit = 0.22;
            let (dx, dz) = (self.range(-0.06, 0.06), self.range(-0.04, 0.04));
            let pos = [
                hips[0] as f32 + 0.08 * M + dx * M,
                hips[1] as f32 + 0.1 * M,
                hips[2] as f32 + 0.06 * M + dz * M,
            ];
            let vel = [
                self.range(0.02, 0.1) * M,
                0.12 * M,
                self.range(-0.03, 0.03) * M,
            ];
            self.fx.spawn(Kind::Fart, pos, vel, 2.6);
        }
        if t > 0.3 && t < 0.4 {
            // Sonríe dormido, aliviado.
            self.say(Eyes::Closed, Mouth::Smile, 2.5);
        }
        if t > 3.5 {
            self.fart = None;
        }
    }

    fn sweat(&mut self, s: &Scene, dt: f32) {
        self.emit -= dt;
        if self.emit > 0.0 {
            return;
        }
        self.emit = self.range(0.18, 0.4);
        // De la frente, saltando hacia los costados.
        let k = (self.rand() < 0.5) as usize;
        let (Some(a), Some(b)) = (s.brow[k], s.brow[1 - k]) else {
            return;
        };
        // Hacia afuera: del otro lado de la frente a este.
        let out = [(a[0] - b[0]) as f32, (a[2] - b[2]) as f32];
        let len = libm::sqrtf(out[0] * out[0] + out[1] * out[1]).max(1.0);
        let speed = self.range(0.25, 0.45) * M;
        let pos = [
            a[0] as f32,
            a[1] as f32 + self.range(-0.04, 0.04) * M,
            a[2] as f32,
        ];
        let vel = [
            out[0] / len * speed,
            self.range(0.3, 0.6) * M,
            out[1] / len * speed,
        ];
        self.fx.spawn(Kind::Drop, pos, vel, 0.7);
    }

    fn smoke_puff(&mut self, at: [i32; 3], strong: bool) {
        let pos = [
            at[0] as f32 + self.range(-0.03, 0.03) * M,
            at[1] as f32 + 0.03 * M,
            at[2] as f32 + self.range(-0.03, 0.03) * M,
        ];
        let up = if strong { 0.4 } else { 0.25 };
        let vel = [
            self.range(-0.05, 0.05) * M,
            up * M,
            self.range(-0.05, 0.05) * M,
        ];
        let life = self.range(1.6, 2.4);
        self.fx.spawn(Kind::Smoke, pos, vel, life);
    }

    fn update_cook(&mut self, s: &Scene, dt: f32, due: bool, want: Option<Gag>) {
        let at_stove = s.target == Some(Target::Spot("spot_stove"));
        let at_ext = s.target == Some(Target::Spot("spot_extinguisher"));
        let pot = s.pot;
        if self.cook != Cook::Calm {
            self.emit -= dt;
        }
        self.cook = match self.cook {
            Cook::Calm => {
                let pick = match want {
                    Some(g @ (Gag::Smoke | Gag::Pop | Gag::Fire)) => Some(g),
                    _ if due => Some(match self.rand() {
                        r if r < 0.45 => Gag::Smoke,
                        r if r < 0.75 => Gag::Pop,
                        _ => Gag::Fire,
                    }),
                    _ => None,
                };
                match pick {
                    Some(g) if s.act == Activity::Cook && at_stove && s.settled => {
                        self.next = self.interval(s.act);
                        match g {
                            Gag::Smoke => Cook::Smoke(0.0),
                            Gag::Pop => Cook::Pop(0.0),
                            _ => Cook::Burning(0.0),
                        }
                    }
                    _ => Cook::Calm,
                }
            }
            Cook::Smoke(t) => {
                if t < 6.0 && self.emit <= 0.0 {
                    self.emit = 0.18;
                    self.smoke_puff(pot, false);
                }
                if (1.2..1.3).contains(&t) {
                    // Lo espanta con la mano, con cara de "¿y esto?".
                    self.react(clip::WAVING, 4.0);
                    self.say(Eyes::Suspicious, Mouth::Sad, 4.0);
                }
                if t > 8.0 {
                    Cook::Calm
                } else {
                    Cook::Smoke(t + dt)
                }
            }
            Cook::Pop(t) => {
                if t == 0.0 {
                    let up = [pot[0] as f32, pot[1] as f32 + 0.08 * M, pot[2] as f32];
                    self.fx.spawn(Kind::Spark, up, [0.0; 3], 0.55);
                    let side = [up[0] + 0.07 * M, up[1] + 0.05 * M, up[2] - 0.04 * M];
                    self.fx.spawn(Kind::Spark, side, [0.0; 3], 0.4);
                    for _ in 0..4 {
                        self.smoke_puff(pot, true);
                    }
                    self.react(clip::SURPRISED, 2.5);
                    self.say(Eyes::Happy, Mouth::Mad, 1.4);
                }
                if t > 3.0 {
                    Cook::Calm
                } else {
                    Cook::Pop(t + dt)
                }
            }
            Cook::Burning(t) => {
                if t == 0.0 {
                    let up = [pot[0] as f32, pot[1] as f32 + 0.06 * M, pot[2] as f32];
                    self.fx.spawn(Kind::Spark, up, [0.0; 3], 0.4);
                    self.react(clip::SURPRISED, 2.2);
                    self.say(Eyes::Happy, Mouth::Mad, 2.2);
                }
                self.fx.fire = Some([pot[0] as f32, pot[1] as f32, pot[2] as f32]);
                if self.emit <= 0.0 {
                    self.emit = 0.35;
                    self.smoke_puff(pot, true);
                }
                if t > 2.0 {
                    Cook::Fetch
                } else {
                    Cook::Burning(t + dt)
                }
            }
            Cook::Fetch => {
                if self.emit <= 0.0 {
                    self.emit = 0.35;
                    self.smoke_puff(pot, true);
                }
                if at_ext && s.settled && s.still_t > 0.5 {
                    self.carrying = true;
                    Cook::Back
                } else {
                    Cook::Fetch
                }
            }
            Cook::Back => {
                if self.emit <= 0.0 {
                    self.emit = 0.35;
                    self.smoke_puff(pot, true);
                }
                if at_stove && s.settled && s.still_t > 0.4 {
                    self.say(Eyes::Mad, Mouth::Mad, 3.0);
                    Cook::Spray(0.0)
                } else {
                    Cook::Back
                }
            }
            Cook::Spray(t) => {
                if let Some(n) = s.nozzle
                    && t < 2.6
                    && self.emit <= 0.0
                {
                    self.emit = 0.06;
                    let from = n.map(|c| c as f32);
                    let to = [pot[0] as f32, pot[1] as f32 + 0.05 * M, pot[2] as f32];
                    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
                    let k = 2.2;
                    let vel = [
                        d[0] * k + self.range(-0.1, 0.1) * M,
                        d[1] * k + 0.25 * M,
                        d[2] * k + self.range(-0.1, 0.1) * M,
                    ];
                    self.fx.spawn(Kind::Foam, from, vel, 0.55);
                }
                if t >= 2.4 && self.fx.fire.is_some() {
                    // Apagado: una última humareda.
                    self.fx.fire = None;
                    for _ in 0..5 {
                        self.smoke_puff(pot, true);
                    }
                    self.say(Eyes::Suspicious, Mouth::Sad, 2.5);
                }
                if t > 3.6 {
                    Cook::Return
                } else {
                    Cook::Spray(t + dt)
                }
            }
            Cook::Return => {
                if at_ext && s.settled && s.still_t > 0.5 {
                    self.carrying = false;
                    self.say(Eyes::Happy, Mouth::NaturalSmile, 2.0);
                    Cook::Calm
                } else {
                    Cook::Return
                }
            }
        };
    }

    /// Cambia el plan mientras dura el incendio (ir y volver con el matafuego).
    pub fn plan(&self, p: Plan) -> Plan {
        let stand = |spot, clip, prop| Plan {
            target: Target::Spot(spot),
            posture: Posture::Stand,
            clip,
            prop,
            overlay: None,
            roam: crate::plan::Roam::Stay,
        };
        match self.cook {
            Cook::Fetch => stand("spot_extinguisher", clip::IDLE, Prop::None),
            Cook::Back | Cook::Spray(_) => stand("spot_stove", clip::SPRAY, Prop::Extinguisher),
            Cook::Return => stand("spot_extinguisher", clip::IDLE, Prop::Extinguisher),
            _ => p,
        }
    }

    /// La reacción encima del cuerpo (espantar el humo, el susto).
    pub fn overlay(&self) -> Option<usize> {
        self.overlay.map(|(k, _)| k)
    }

    /// La cara que pone por el gag, si hay.
    pub fn face(&self) -> Option<Face> {
        self.face.map(|(f, _)| f)
    }
}
