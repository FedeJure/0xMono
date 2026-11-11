//! El personaje en sí: sigue su agenda y reacciona cuando le hablas.
//!
//! Como los cuadros de Harry Potter, no está ahí para que lo cuides: hace su
//! vida. Si está despierto te saluda; si duerme, se da vuelta y sigue; si
//! insistes lo despiertas (y se queja); si salió de visita, el marco está vacío.

use crate::activity::Activity;
use crate::clock::LocalTime;
use crate::face::Emotion;
use crate::personality::Personality;
use crate::rng::{Rng, mix};
use crate::routine::{self, Slot};
use crate::talk::{self, Context, Kind, Line};

/// Toques seguidos que hacen falta para despertarlo.
const WAKE_POKES: u8 = 3;
/// Los toques cuentan como "seguidos" dentro de esta ventana.
const POKE_WINDOW_SECS: u32 = 20;
/// Tiempo que queda medio dormido antes de volver a la cama.
const GROGGY_SECS: u32 = 3 * 60;
/// Si lo saludaste hace menos de esto, no repite el saludo.
const SEEN_AGAIN_SECS: u32 = 10 * 60;
/// Más de esto sin verte y te extraña.
const MISSED_SECS: u32 = 24 * 3600;
/// Cada cuánto piensa en voz alta (tiempo real, mín y máx).
const THINK_MS: (i32, i32) = (40_000, 120_000);
/// Frases recientes que no repite.
const RECENT: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Greeting {
    /// La primera vez que te ve.
    Hello,
    Morning,
    Afternoon,
    Evening,
    MissedYou,
    /// Lo saludaste hace un rato.
    Again,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reaction {
    Greet(Greeting),
    /// Dormido: se mueve un poco y sigue durmiendo.
    Stir,
    /// Lo despertaste.
    Grumpy,
    /// No está en el cuadro.
    NotHome,
}

impl Reaction {
    /// Cuánto dura en pantalla (tiempo real, no acelerado).
    pub fn duration_ms(self) -> u32 {
        match self {
            Reaction::Greet(_) => 3_500,
            Reaction::Stir => 2_500,
            Reaction::Grumpy => 4_000,
            Reaction::NotHome => 3_000,
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Reaction::Greet(Greeting::Hello) => "¡Hola! Mucho gusto.",
            Reaction::Greet(Greeting::Morning) => "¡Buen día!",
            Reaction::Greet(Greeting::Afternoon) => "¡Buenas tardes!",
            Reaction::Greet(Greeting::Evening) => "¡Buenas noches!",
            Reaction::Greet(Greeting::MissedYou) => "¡Volviste! Te extrañé.",
            Reaction::Greet(Greeting::Again) => "¿Sí?",
            Reaction::Stir => "Mmm... zzz",
            Reaction::Grumpy => "¿Qué pasa? Estaba durmiendo...",
            Reaction::NotHome => "«Vuelvo enseguida»",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Pasó a hacer otra cosa (para sonidos o transiciones).
    ActivityChanged { from: Activity, to: Activity },
}

pub struct Companion {
    pub personality: Personality,
    /// Último momento en que te saludó (se guarda en flash).
    pub last_seen: Option<LocalTime>,
    slot: Slot,
    groggy_until: Option<LocalTime>,
    reaction: Option<(Reaction, u32)>,
    pokes: u8,
    first_poke: LocalTime,
    /// El último instante de `update` (el mercado cambia con el tiempo).
    now: LocalTime,
    /// Lo que está diciendo o pensando y cuánto le queda en pantalla.
    speech: Option<(Line, u32)>,
    /// Hasta el próximo pensamiento en voz alta (tiempo real).
    think_in: u32,
    recent: [u16; RECENT],
    recent_at: usize,
    rng: Rng,
}

impl Companion {
    pub fn new(personality: Personality, last_seen: Option<LocalTime>, now: LocalTime) -> Self {
        Self {
            slot: routine::slot_at(&personality, now),
            personality,
            last_seen,
            groggy_until: None,
            reaction: None,
            pokes: 0,
            first_poke: now,
            now,
            speech: None,
            think_in: 20_000,
            recent: [u16::MAX; RECENT],
            recent_at: 0,
            rng: Rng::new(mix(personality.seed, 0x7A1C)),
        }
    }

    /// Lo que está diciendo (o pensando) ahora, para el globo.
    pub fn speech(&self) -> Option<&Line> {
        self.speech.as_ref().map(|(l, _)| l)
    }

    /// Todo lo que hace falta para elegir qué decir ahora.
    fn context(&mut self, now: LocalTime) -> Context<'_> {
        let (plan, minute) = routine::plan_at(&self.personality, now);
        let blocks = plan.blocks();
        let here = blocks.iter().rposition(|b| b.start <= minute).unwrap_or(0);
        // Algo que ya hizo hoy (al azar entre lo que tiene para contar) y
        // lo próximo que le toca.
        let mut told = [Activity::Sleep; 16];
        let mut n = 0;
        for b in &blocks[..here] {
            if n < told.len()
                && crate::lines::did(b.activity).is_some()
                && !told[..n].contains(&b.activity)
            {
                told[n] = b.activity;
                n += 1;
            }
        }
        let did = (n > 0).then(|| told[self.rng.next_u32() as usize % n]);
        let next = blocks.get(here + 1).map(|b| b.activity);
        Context {
            activity: self.activity(),
            now,
            kind: plan.kind,
            mood: plan.mood,
            personality: &self.personality,
            market: market(self.personality.seed, now),
            did,
            next,
        }
    }

    /// Dice (o piensa) algo de `kind` según el momento.
    fn speak(&mut self, kind: Kind, now: LocalTime, greeting: Option<Greeting>) {
        let mut rng = Rng::new(self.rng.next_u32());
        let recent = self.recent;
        let ctx = self.context(now);
        let line = match greeting {
            Some(g) => Some(talk::greet(g, &ctx, &mut rng)),
            None => talk::compose(kind, &ctx, &recent, &mut rng),
        };
        if let Some(line) = line {
            if line.rule != u16::MAX {
                self.recent[self.recent_at] = line.rule;
                self.recent_at = (self.recent_at + 1) % RECENT;
            }
            self.speech = Some((line, line.duration_ms()));
        }
    }

    /// Lo que está haciendo ahora (la agenda, salvo que lo hayas despertado).
    pub fn activity(&self) -> Activity {
        if self.groggy_until.is_some() {
            Activity::Groggy
        } else {
            self.slot.activity
        }
    }

    /// La actividad de la agenda en curso, con su horario.
    pub fn slot(&self) -> Slot {
        self.slot
    }

    pub fn reaction(&self) -> Option<Reaction> {
        self.reaction.map(|(r, _)| r)
    }

    /// La emoción que muestra la cara: la de la reacción en curso si hay
    /// una, si no la de lo que está haciendo.
    pub fn emotion(&self) -> Emotion {
        match self.reaction() {
            // Te está contestando: cara de hablar (más abajo).
            Some(Reaction::Greet(Greeting::Again)) => {}
            Some(Reaction::Greet(_)) => return Emotion::Joyful,
            Some(Reaction::Grumpy) => return Emotion::Angry,
            Some(Reaction::Stir) | Some(Reaction::NotHome) | None => {}
        }
        let act = self.activity();
        if let Some((line, _)) = &self.speech
            && line.kind == Kind::Say
            && !act.is_asleep()
            && act.is_present()
            && act != Activity::Groggy
        {
            return Emotion::Chatty;
        }
        activity_emotion(self.activity(), self.personality.seed, self.now)
    }

    /// Avanza al instante `now`; `dt_ms` es tiempo real (para las reacciones).
    pub fn update(&mut self, now: LocalTime, dt_ms: u32) -> Option<Event> {
        self.now = now;
        // El globo se va solo; cada tanto piensa en voz alta (si está en casa
        // y no está durmiendo profundo).
        if let Some((line, left)) = self.speech {
            let left = left.saturating_sub(dt_ms);
            self.speech = (left > 0).then_some((line, left));
        } else {
            self.think_in = self.think_in.saturating_sub(dt_ms);
            if self.think_in == 0 {
                let act = self.activity();
                if act.is_present() && (!act.is_asleep() || self.rng.chance(25)) {
                    self.speak(Kind::Think, now, None);
                }
                self.think_in = self.rng.range(THINK_MS.0, THINK_MS.1) as u32;
            }
        }
        if let Some((r, left)) = self.reaction {
            let left = left.saturating_sub(dt_ms);
            self.reaction = (left > 0).then_some((r, left));
        }

        let before = self.activity();
        if self.groggy_until.is_some_and(|until| now >= until) {
            self.groggy_until = None;
        }
        // También cubre que el reloj salte hacia atrás (se corrigió la hora).
        if now >= self.slot.end || now < self.slot.start {
            self.slot = routine::slot_at(&self.personality, now);
        }
        let after = self.activity();
        (before != after).then_some(Event::ActivityChanged {
            from: before,
            to: after,
        })
    }

    /// Piensa en voz alta ya, sin esperar su momento (para revisar el globo).
    pub fn think(&mut self, now: LocalTime) {
        self.speak(Kind::Think, now, None);
    }

    /// Le hablas (cualquier botón).
    pub fn poke(&mut self, now: LocalTime) -> Reaction {
        let reaction = match self.activity() {
            Activity::Away => Reaction::NotHome,
            Activity::Groggy => Reaction::Grumpy,
            a if a.is_asleep() => {
                let since = now
                    .total_secs()
                    .saturating_sub(self.first_poke.total_secs());
                if self.pokes == 0 || since > POKE_WINDOW_SECS {
                    self.pokes = 0;
                    self.first_poke = now;
                }
                self.pokes += 1;
                if self.pokes >= WAKE_POKES {
                    self.pokes = 0;
                    self.groggy_until = Some(now.add_secs(GROGGY_SECS));
                    Reaction::Grumpy
                } else {
                    Reaction::Stir
                }
            }
            _ => {
                let g = self.greeting(now);
                self.last_seen = Some(now);
                Reaction::Greet(g)
            }
        };
        self.reaction = Some((reaction, reaction.duration_ms()));
        // Lo que te dice: el saludo la primera vez y, si seguís hablándole,
        // algo de lo que está haciendo (dormido, lo que murmura en sueños;
        // si no está, el cartel de la puerta).
        let greeting = match reaction {
            Reaction::Greet(Greeting::Again) => None,
            Reaction::Greet(g) => Some(g),
            _ => None,
        };
        self.speak(Kind::Say, now, greeting);
        self.think_in = self.think_in.max(15_000);
        reaction
    }

    fn greeting(&self, now: LocalTime) -> Greeting {
        let Some(seen) = self.last_seen else {
            return Greeting::Hello;
        };
        let gap = now.total_secs().saturating_sub(seen.total_secs());
        if gap < SEEN_AGAIN_SECS {
            Greeting::Again
        } else if gap >= MISSED_SECS {
            Greeting::MissedYou
        } else {
            match now.hour() {
                5..12 => Greeting::Morning,
                12..20 => Greeting::Afternoon,
                _ => Greeting::Evening,
            }
        }
    }
}

/// La emoción de cada actividad (sin reacciones). `seed` y `now` hacen que
/// el mercado suba y baje distinto para cada personaje.
pub fn activity_emotion(activity: Activity, seed: u32, now: LocalTime) -> Emotion {
    match activity {
        Activity::Sleep | Activity::Nap => Emotion::Asleep,
        Activity::WakeUp | Activity::Groggy => Emotion::Sleepy,
        Activity::Eat
        | Activity::Snack
        | Activity::Garden
        | Activity::Bananas
        | Activity::Cook
        | Activity::Guitar => Emotion::Happy,
        Activity::Exercise => Emotion::Exerting,
        Activity::Clean | Activity::Stretch => Emotion::Determined,
        Activity::Read | Activity::Code => Emotion::Focused,
        Activity::Meeting | Activity::Phone => Emotion::Chatty,
        Activity::Dance | Activity::Games | Activity::Movie => Emotion::Playful,
        Activity::Mate | Activity::Daydream | Activity::Stargaze => Emotion::Relaxed,
        Activity::Wander | Activity::Away => Emotion::Content,
        // El mercado sube y baja cada pocos minutos, y se le nota.
        Activity::Trading => match market(seed, now) {
            2 => Emotion::Joyful,
            1 => Emotion::Happy,
            0 => Emotion::Focused,
            -1 => Emotion::Suspicious,
            _ => Emotion::Sad,
        },
    }
}

/// Cómo viene el mercado para este personaje: de −2 (se desploma) a 2 (a la
/// luna). Cambia cada dos minutos y medio, con rachas de varios ticks.
pub fn market(seed: u32, now: LocalTime) -> i8 {
    let tick = now.total_secs() / 150;
    // Tendencia lenta (media hora) más ruido.
    let trend = (mix(seed, tick / 12) % 5) as i32 - 2;
    let noise = (mix(seed ^ 0xB7C, tick) % 3) as i32 - 1;
    (trend + noise).clamp(-2, 2) as i8
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u32 = 20_000;

    fn at(h: u8, m: u8) -> LocalTime {
        LocalTime::new(DAY, h, m)
    }

    fn companion(now: LocalTime) -> Companion {
        Companion::new(Personality::from_seed(1), None, now)
    }

    #[test]
    fn sleeper_stirs_then_wakes_grumpy_then_goes_back_to_bed() {
        let night = at(3, 0);
        let mut c = companion(night);
        assert_eq!(c.activity(), Activity::Sleep);
        assert_eq!(c.poke(night), Reaction::Stir);
        assert_eq!(c.poke(night.add_secs(2)), Reaction::Stir);
        assert_eq!(c.activity(), Activity::Sleep);
        assert_eq!(c.poke(night.add_secs(4)), Reaction::Grumpy);
        assert_eq!(c.activity(), Activity::Groggy);

        assert_eq!(c.update(night.add_secs(60), 16), None);
        assert_eq!(
            c.update(night.add_secs(GROGGY_SECS + 5), 16),
            Some(Event::ActivityChanged {
                from: Activity::Groggy,
                to: Activity::Sleep
            })
        );
    }

    #[test]
    fn spaced_pokes_do_not_wake_it() {
        let mut t = at(3, 0);
        let mut c = companion(t);
        for _ in 0..6 {
            assert_eq!(c.poke(t), Reaction::Stir);
            t = t.add_secs(POKE_WINDOW_SECS + 1);
        }
        assert_eq!(c.activity(), Activity::Sleep);
    }

    #[test]
    fn greetings_depend_on_time_and_absence() {
        let t = at(11, 0);
        let mut c = companion(t);
        assert!(!c.activity().is_asleep());
        if c.activity() == Activity::Away {
            return; // con esta semilla justo salió; lo cubre otro test
        }
        assert_eq!(c.poke(t), Reaction::Greet(Greeting::Hello));
        assert_eq!(c.poke(t.add_secs(60)), Reaction::Greet(Greeting::Again));
        assert_eq!(c.poke(t.add_secs(1800)), Reaction::Greet(Greeting::Morning));

        c.last_seen = Some(LocalTime::new(DAY - 3, 11, 0));
        assert_eq!(c.poke(t), Reaction::Greet(Greeting::MissedYou));
    }

    #[test]
    fn reaction_fades_in_real_time() {
        let t = at(3, 0);
        let mut c = companion(t);
        c.poke(t);
        c.update(t, Reaction::Stir.duration_ms() - 1);
        assert_eq!(c.reaction(), Some(Reaction::Stir));
        c.update(t, 1);
        assert_eq!(c.reaction(), None);
    }

    #[test]
    fn follows_the_schedule_and_reports_changes() {
        let mut t = at(0, 0);
        let mut c = companion(t);
        let mut changes = 0;
        for _ in 0..1440 {
            t = t.add_secs(60);
            if let Some(Event::ActivityChanged { from, to }) = c.update(t, 16) {
                assert_ne!(from, to);
                changes += 1;
            }
            assert_eq!(c.activity(), routine::slot_at(&c.personality, t).activity);
        }
        assert!(changes >= 6, "solo {changes} cambios en un día");
    }

    #[test]
    fn emotion_follows_activity_and_reactions() {
        let night = at(3, 0);
        let mut c = companion(night);
        assert_eq!(c.emotion(), Emotion::Asleep);
        c.poke(night);
        assert_eq!(c.emotion(), Emotion::Asleep, "se da vuelta sin despertarse");
        c.poke(night.add_secs(1));
        c.poke(night.add_secs(2));
        assert_eq!(c.emotion(), Emotion::Angry, "lo despertaste");
        c.update(night.add_secs(30), Reaction::Grumpy.duration_ms());
        assert_eq!(c.emotion(), Emotion::Sleepy);
    }

    #[test]
    fn talks_when_poked_and_thinks_aloud_alone() {
        let t = at(11, 0);
        let mut c = companion(t);
        if !c.activity().is_present() || c.activity().is_asleep() {
            return;
        }
        // Le hablás: saluda la primera vez y después contesta algo.
        c.poke(t);
        let hello = c.speech().expect("saluda").as_str().to_string();
        assert!(!hello.is_empty());
        c.poke(t.add_secs(30));
        let line = *c.speech().expect("contesta");
        assert_eq!(line.kind, crate::talk::Kind::Say);
        assert_eq!(c.emotion(), Emotion::Chatty, "habla moviendo la boca");
        // Solo, piensa en voz alta cada tanto, y no repite.
        let mut thoughts = std::vec::Vec::new();
        let mut now = t.add_secs(60);
        for _ in 0..(20 * 60 * 10) {
            now = now.add_secs(0);
            c.update(now, 100);
            if let Some(l) = c.speech()
                && l.kind == crate::talk::Kind::Think
                && thoughts.last() != Some(&l.rule)
            {
                thoughts.push(l.rule);
            }
        }
        assert!(
            thoughts.len() >= 8,
            "pensó {} veces en 20 minutos",
            thoughts.len()
        );
        let mut unique = thoughts.clone();
        unique.sort();
        unique.dedup();
        assert!(
            unique.len() * 10 >= thoughts.len() * 8,
            "repite mucho: {thoughts:?}"
        );
    }

    #[test]
    fn market_has_ups_and_downs() {
        let mut seen = [0u32; 5];
        for k in 0..2000 {
            let m = market(7, LocalTime::from_total_secs(1_700_000_000 + k * 150));
            seen[(m + 2) as usize] += 1;
        }
        assert!(seen.iter().all(|&n| n > 50), "{seen:?}");
    }

    #[test]
    fn empty_frame_when_away() {
        // Buscar un momento en que algún personaje haya salido de visita.
        for seed in 0..20 {
            let p = Personality::from_seed(seed);
            let mut t = LocalTime::new(DAY, 0, 0);
            for _ in 0..(7 * 96) {
                if routine::slot_at(&p, t).activity == Activity::Away {
                    let mut c = Companion::new(p, None, t);
                    assert_eq!(c.poke(t), Reaction::NotHome);
                    assert_eq!(c.last_seen, None);
                    return;
                }
                t = t.add_secs(15 * 60);
            }
        }
        panic!("nadie salió de visita en una semana");
    }
}
