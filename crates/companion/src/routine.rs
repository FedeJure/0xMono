//! La agenda del día.
//!
//! Un "día de agenda" va desde que se levanta hasta que se levanta al día
//! siguiente, así la noche (que cruza la medianoche) queda entera en un solo
//! día. La agenda sale de la personalidad y la fecha con azar determinista:
//! cada día es distinto, pero recalcularlo da siempre lo mismo.
//!
//! Cada día tiene su forma:
//! - **Día de semana:** trabaja de mañana y de tarde (programa, tiene
//!   reuniones, mira el mercado) con pausas para un mate o estirar las
//!   piernas; almuerza (a veces cocina) y merienda. Al terminar, tiempo libre.
//! - **Sábado:** se levanta tarde, mate, limpia la casa, cocina, siesta y una
//!   tarde y noche libres (más salidas, consola, baile).
//! - **Domingo:** mate largo, almuerzo grande, siesta, llama a la familia y
//!   se acuesta más temprano.
//!
//! Y un **humor** del día (vago, activo, sociable, concentrado, creativo, de
//! aire libre) que pesa en qué elige hacer en el tiempo libre: dos lunes
//! nunca son iguales.

use crate::activity::Activity;
use crate::clock::{DAY_MIN, DAY_SECS, LocalTime, Weekday, weekday};
use crate::personality::Personality;
use crate::rng::{Rng, mix};

pub const MAX_BLOCKS: usize = 64;
/// Nunca duerme menos que esto.
const MIN_SLEEP: i32 = 6 * 60;
/// Huecos más cortos se pegan al bloque anterior.
const MIN_BLOCK: i32 = 10;

/// Una actividad que empieza en `start`: minutos desde la medianoche del día
/// de la agenda (puede pasar de 1440 si ya es de madrugada).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub start: i32,
    pub activity: Activity,
}

pub struct DayPlan {
    pub day: u32,
    pub kind: DayKind,
    pub mood: Mood,
    blocks: [Block; MAX_BLOCKS],
    len: usize,
    /// Hora en que se levanta al día siguiente (fin del último bloque).
    pub end: i32,
}

/// Una actividad concreta de la agenda, con su comienzo y su fin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub activity: Activity,
    pub start: LocalTime,
    pub end: LocalTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayKind {
    Workday,
    Saturday,
    Sunday,
}

pub fn day_kind(day: u32) -> DayKind {
    match weekday(day) {
        Weekday::Sat => DayKind::Saturday,
        Weekday::Sun => DayKind::Sunday,
        _ => DayKind::Workday,
    }
}

/// El humor del día: qué tiene ganas de hacer en su tiempo libre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Lazy,
    Energetic,
    Social,
    Focused,
    Creative,
    Outdoorsy,
}

impl Mood {
    const ALL: [Mood; 6] = [
        Mood::Lazy,
        Mood::Energetic,
        Mood::Social,
        Mood::Focused,
        Mood::Creative,
        Mood::Outdoorsy,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Mood::Lazy => "vago",
            Mood::Energetic => "con energía",
            Mood::Social => "sociable",
            Mood::Focused => "concentrado",
            Mood::Creative => "creativo",
            Mood::Outdoorsy => "de aire libre",
        }
    }

    /// Cuánto pesa `a` con este humor, en %.
    fn factor(self, a: Activity) -> u32 {
        use Activity::*;
        let (more, less): (&[Activity], &[Activity]) = match self {
            Mood::Lazy => (
                &[Daydream, Mate, Movie, Games, Read],
                &[Exercise, Dance, Stretch, Away],
            ),
            Mood::Energetic => (
                &[Exercise, Dance, Stretch, Bananas, Garden],
                &[Daydream, Movie, Mate],
            ),
            Mood::Social => (&[Away, Phone, Dance, Games], &[Read, Code, Daydream]),
            Mood::Focused => (&[Code, Read, Trading], &[Away, Dance, Movie]),
            Mood::Creative => (&[Guitar, Dance, Read, Code], &[Games, Movie]),
            Mood::Outdoorsy => (
                &[Garden, Bananas, Stargaze, Exercise, Away],
                &[Games, Movie, Code],
            ),
        };
        if more.contains(&a) {
            250
        } else if less.contains(&a) {
            40
        } else {
            100
        }
    }
}

fn day_rng(p: &Personality, day: u32, salt: u32) -> Rng {
    Rng::new(mix(mix(p.seed, day), salt))
}

/// El humor del día `day`, según sus gustos y si es fin de semana.
pub fn mood(p: &Personality, day: u32) -> Mood {
    let weekend = day_kind(day) != DayKind::Workday;
    let w = |m: Mood| -> u32 {
        let base = match m {
            Mood::Lazy => p.chill as u32 + 20,
            Mood::Energetic => p.sporty as u32,
            Mood::Social => p.sociable as u32,
            Mood::Focused => p.geeky as u32,
            Mood::Creative => (p.musical as u32 + p.bookish as u32) / 2,
            Mood::Outdoorsy => p.green_thumb as u32,
        };
        match (m, weekend) {
            (Mood::Lazy | Mood::Social | Mood::Outdoorsy, true) => base * 2,
            (Mood::Focused, false) => base * 2,
            _ => base,
        }
    };
    let total: u32 = Mood::ALL.iter().map(|&m| w(m)).sum();
    let mut x = day_rng(p, day, 7).next_u32() % total.max(1);
    for m in Mood::ALL {
        if x < w(m) {
            return m;
        }
        x -= w(m);
    }
    Mood::Lazy
}

/// Hora de levantarse el día `day`, en minutos desde su medianoche.
pub fn wake_time(p: &Personality, day: u32) -> i32 {
    let base = match day_kind(day) {
        DayKind::Workday => 7 * 60 + 30,
        DayKind::Saturday => 9 * 60,
        DayKind::Sunday => 9 * 60 + 30,
    };
    base + p.chronotype as i32 + day_rng(p, day, 1).range(-20, 20)
}

/// Hora de acostarse la noche del día `day`. Viernes y sábado, más tarde.
fn bedtime(p: &Personality, day: u32) -> i32 {
    let late = if weekday(day + 1).is_weekend() { 60 } else { 0 };
    23 * 60 + 30 + late + p.chronotype as i32 + day_rng(p, day, 2).range(-25, 25)
}

const FREE_TIME: [Activity; 16] = [
    Activity::Exercise,
    Activity::Stretch,
    Activity::Read,
    Activity::Code,
    Activity::Bananas,
    Activity::Garden,
    Activity::Wander,
    Activity::Daydream,
    Activity::Away,
    Activity::Dance,
    Activity::Guitar,
    Activity::Games,
    Activity::Movie,
    Activity::Phone,
    Activity::Stargaze,
    Activity::Mate,
];

/// Lo que puede hacer en una pausa del trabajo.
const BREAKS: [Activity; 6] = [
    Activity::Mate,
    Activity::Stretch,
    Activity::Wander,
    Activity::Phone,
    Activity::Daydream,
    Activity::Bananas,
];

/// Qué tan oscuro: el jardín y las plantas, de día; las estrellas, de noche.
const DUSK: i32 = 19 * 60 + 30;
const NIGHT: i32 = 20 * 60 + 30;

/// Ganas de hacer `a` en el minuto `minute` según sus gustos, la hora, el
/// tipo de día y el humor.
fn weight(a: Activity, p: &Personality, minute: i32, kind: DayKind, mood: Mood) -> u32 {
    use Activity::*;
    let half = |x: u8| x as u32 / 2;
    // (gusto, mañana, tarde, noche)
    let (taste, m, af, ev) = match a {
        Exercise => (p.sporty as u32, 3, 2, 1),
        Stretch => (half(p.sporty) + 20, 4, 1, 1),
        Read => (p.bookish as u32, 2, 2, 4),
        // Por gusto (además del trabajo): sobre todo los fines de semana.
        Code => (
            p.geeky as u32 / if kind == DayKind::Workday { 3 } else { 1 },
            1,
            2,
            2,
        ),
        Bananas => (30, 1, 2, 0),
        Garden => (p.green_thumb as u32, 3, 2, 0),
        Wander => (35, 2, 2, 2),
        Daydream => (half(p.chill) + 20, 1, 2, 2),
        Away => (half(p.sociable), 1, 2, 1),
        Dance => (p.musical as u32, 0, 1, 3),
        Guitar => (p.musical as u32, 1, 2, 3),
        Games => (p.gamer as u32, 0, 2, 3),
        Movie => (half(p.gamer) + 20, 0, 0, 4),
        Phone => (p.sociable as u32, 1, 2, 2),
        Stargaze => (half(p.chill) + 30, 0, 0, 4),
        Mate => (p.chill as u32, 3, 3, 0),
        _ => (0, 0, 0, 0),
    };
    let when = if minute < 12 * 60 {
        m
    } else if minute < 19 * 60 {
        af
    } else {
        ev
    };
    // Cosas que solo tienen sentido con luz, o de noche.
    let daylight_only = matches!(a, Garden | Bananas);
    if (daylight_only && minute >= DUSK) || (a == Stargaze && minute < NIGHT) {
        return 0;
    }
    let weekend = match (a, kind) {
        (Away | Games | Dance, DayKind::Saturday) => 2,
        (Away | Read | Guitar, DayKind::Sunday) => 2,
        _ => 1,
    };
    taste * when * weekend * mood.factor(a) / 100
}

/// Duración (mín, máx) en minutos.
fn duration(a: Activity) -> (i32, i32) {
    use Activity::*;
    match a {
        Exercise => (20, 45),
        Stretch => (10, 20),
        Read => (30, 90),
        Code => (40, 100),
        Bananas => (12, 20),
        Garden | Daydream => (15, 30),
        Wander => (15, 40),
        Away => (30, 90),
        Dance => (15, 30),
        Guitar => (20, 45),
        Games => (30, 90),
        Movie => (80, 120),
        Phone => (10, 30),
        Stargaze => (15, 30),
        Mate => (15, 35),
        Meeting => (25, 50),
        Trading => (15, 35),
        Cook => (20, 40),
        Clean => (30, 60),
        _ => (15, 30),
    }
}

/// Arma la agenda bloque a bloque; `t` es el minuto en que termina lo último.
struct Builder<'a> {
    plan: &'a mut DayPlan,
    p: &'a Personality,
    rng: Rng,
    t: i32,
    /// Reuniones que ya tuvo hoy.
    meetings: u32,
    /// Cuántas veces hizo cada actividad hoy: repetir cansa.
    done: [u8; 32],
}

impl Builder<'_> {
    fn add(&mut self, activity: Activity, minutes: i32) {
        let plan = &mut *self.plan;
        // El último lugar queda reservado para dormir: si no entra, el bloque
        // anterior simplemente dura más.
        if plan.len < MAX_BLOCKS - 1 || (activity == Activity::Sleep && plan.len < MAX_BLOCKS) {
            // Dos bloques seguidos iguales se juntan en uno.
            if plan.len > 0 && plan.blocks[plan.len - 1].activity == activity {
                self.t += minutes;
                return;
            }
            plan.blocks[plan.len] = Block {
                start: self.t,
                activity,
            };
            plan.len += 1;
            let k = &mut self.done[activity as usize % 32];
            *k = k.saturating_add(1);
        }
        self.t += minutes;
    }

    fn add_rand(&mut self, activity: Activity, lo: i32, hi: i32) {
        let m = self.rng.range(lo, hi);
        self.add(activity, m);
    }

    fn last(&self) -> Option<Activity> {
        let p = &*self.plan;
        (p.len > 0).then(|| p.blocks[p.len - 1].activity)
    }

    fn pick_from(&mut self, pool: &[Activity], w: impl Fn(Activity) -> u32) -> Option<Activity> {
        let prev = self.last();
        let done = self.done;
        // Lo que ya hizo hoy tiene menos ganas: la segunda vez 40 %, la
        // tercera 25 %... (salvo el trabajo, que es lo que es).
        let w = |a: Activity| {
            if Some(a) == prev {
                return 0;
            }
            let n = if a.is_work() {
                0
            } else {
                done[a as usize % 32] as u32
            };
            w(a) * 2 / (2 + 3 * n)
        };
        let total: u32 = pool.iter().map(|&a| w(a)).sum();
        if total == 0 {
            return None;
        }
        let mut x = self.rng.next_u32() % total;
        for &a in pool {
            let wa = w(a);
            if x < wa {
                return Some(a);
            }
            x -= wa;
        }
        None
    }

    /// Un bloque de largo `(lo, hi)` que no se pase de `to` (si el resto
    /// queda muy corto, se lo come).
    fn add_until(&mut self, a: Activity, (lo, hi): (i32, i32), to: i32) {
        let mut m = self.rng.range(lo, hi).min(to - self.t);
        if to - (self.t + m) < MIN_BLOCK {
            m = to - self.t;
        }
        self.add(a, m);
    }

    /// Tiempo libre hasta el minuto `to`.
    fn fill(&mut self, to: i32) {
        let (p, kind, mood) = (self.p, self.plan.kind, self.plan.mood);
        while to - self.t >= MIN_BLOCK {
            let t = self.t;
            let a = self
                .pick_from(&FREE_TIME, |a| weight(a, p, t, kind, mood))
                .unwrap_or(Activity::Wander);
            self.add_until(a, duration(a), to);
        }
        self.t = self.t.max(to);
    }

    /// Trabajo hasta el minuto `to`: programar, reuniones (una o dos por
    /// día), mirar el mercado y pausas cada hora y media más o menos.
    fn work(&mut self, to: i32) {
        let p = self.p;
        let mut since_break = 0;
        while to - self.t >= MIN_BLOCK {
            if since_break >= 80 && to - self.t >= 30 {
                let (kind, mood, t) = (self.plan.kind, self.plan.mood, self.t);
                let a = self
                    .pick_from(&BREAKS, |a| weight(a, p, t, kind, mood).max(1))
                    .unwrap_or(Activity::Mate);
                self.add_until(a, (10, 20), to);
                since_break = 0;
                continue;
            }
            let meetings = self.meetings;
            let a = self
                .pick_from(
                    &[Activity::Code, Activity::Meeting, Activity::Trading],
                    |a| match a {
                        Activity::Code => 60 + p.geeky as u32 / 2,
                        Activity::Meeting if meetings < 2 => 30,
                        Activity::Trading => 20,
                        _ => 0,
                    },
                )
                .unwrap_or(Activity::Code);
            if a == Activity::Meeting {
                self.meetings += 1;
            }
            let before = self.t;
            self.add_until(a, duration(a), to);
            since_break += self.t - before;
        }
        self.t = self.t.max(to);
    }

    /// Tiempo libre hasta la comida de las `at`, que a veces cocina antes.
    fn meal(&mut self, at: i32, cook_pct: u32, eat: (i32, i32)) {
        if self.rng.chance(cook_pct) {
            let len = self.rng.range(20, 35);
            self.fill(at - len);
            self.add(Activity::Cook, len);
        } else {
            self.fill(at);
        }
        self.add_rand(Activity::Eat, eat.0, eat.1);
    }

    /// Trabajo hasta la comida de las `at`, que a veces cocina antes.
    fn lunch_break(&mut self, at: i32, cook_pct: u32, eat: (i32, i32)) {
        if self.rng.chance(cook_pct) {
            let len = self.rng.range(20, 35);
            self.work(at - len);
            self.add(Activity::Cook, len);
        } else {
            self.work(at);
        }
        self.add_rand(Activity::Eat, eat.0, eat.1);
    }
}

impl DayPlan {
    pub fn generate(p: &Personality, day: u32) -> Self {
        let wake = wake_time(p, day);
        let end = wake_time(p, day + 1) + DAY_MIN as i32;
        let bed = bedtime(p, day).min(end - MIN_SLEEP);
        // Las comidas se corren la mitad que el reloj interno.
        let shift = p.chronotype as i32 / 2;
        let kind = day_kind(day);
        let foodie = p.foodie as u32;

        let mut plan = DayPlan {
            day,
            kind,
            mood: mood(p, day),
            blocks: [Block {
                start: 0,
                activity: Activity::Sleep,
            }; MAX_BLOCKS],
            len: 0,
            end,
        };
        let mut b = Builder {
            plan: &mut plan,
            p,
            rng: day_rng(p, day, 0),
            t: wake,
            meetings: 0,
            done: [0; 32],
        };

        b.add_rand(Activity::WakeUp, 8, 16);
        let lunch = 13 * 60 + shift + b.rng.range(-20, 30);
        let snack = 17 * 60 + shift + b.rng.range(-20, 30);
        let dinner = 21 * 60 + shift + b.rng.range(-30, 30);
        match kind {
            DayKind::Workday => {
                if b.rng.chance(p.sporty as u32 / 2 + 10) {
                    b.add_rand(Activity::Stretch, 8, 15);
                }
                b.add_rand(Activity::Eat, 15, 25);
                if b.rng.chance(p.chill as u32 / 2 + 20) {
                    b.add_rand(Activity::Mate, 10, 20);
                }
                // Trabaja de 9 a 18 (el viernes hasta las 17).
                let start = (9 * 60 + shift / 2 + b.rng.range(-10, 15)).max(b.t);
                b.fill(start);
                b.lunch_break(lunch, foodie / 3, (25, 40));
                let friday = weekday(day) == Weekday::Fri;
                let finish = if friday { 17 * 60 } else { 18 * 60 } + b.rng.range(-20, 20);
                b.work(snack.min(finish - 30));
                b.add_rand(Activity::Snack, 10, 20);
                b.work(finish);
                b.meal(dinner, foodie / 2 + 10, (30, 45));
            }
            DayKind::Saturday => {
                if b.rng.chance(70) {
                    b.add_rand(Activity::Mate, 15, 30);
                }
                b.add_rand(Activity::Eat, 20, 35);
                if b.rng.chance(75) {
                    b.add_rand(Activity::Clean, 30, 60);
                }
                b.meal(lunch.max(b.t + 30), foodie / 2 + 20, (30, 50));
                if b.rng.chance(60) {
                    b.add_rand(Activity::Nap, 30, 60);
                }
                if b.rng.chance(75) {
                    b.fill(snack);
                    b.add_rand(Activity::Snack, 15, 30);
                }
                b.meal(dinner, foodie / 2, (30, 50));
            }
            DayKind::Sunday => {
                b.add_rand(Activity::Mate, 25, 45);
                b.add_rand(Activity::Eat, 20, 35);
                // El almuerzo del domingo casi siempre lo cocina.
                b.meal((lunch + 30).max(b.t + 30), foodie / 2 + 50, (40, 60));
                if b.rng.chance(70) {
                    b.add_rand(Activity::Nap, 40, 80);
                }
                // Llama a la familia a la tarde.
                if b.rng.chance(p.sociable as u32 / 2 + 40) {
                    let at = 17 * 60 + b.rng.range(-60, 30);
                    b.fill(at);
                    b.add_rand(Activity::Phone, 20, 40);
                }
                b.fill(snack.max(b.t));
                b.add_rand(Activity::Snack, 15, 25);
                b.meal(dinner - 15, foodie / 3, (30, 45));
            }
        }

        b.fill(bed);
        b.add(Activity::Sleep, 0);
        plan
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks[..self.len]
    }

    /// Bloque que contiene el minuto `m` y su fin. Antes de levantarse cuenta
    /// como el primer bloque.
    pub fn at(&self, m: i32) -> (Block, i32) {
        let blocks = self.blocks();
        let i = blocks.iter().rposition(|b| b.start <= m).unwrap_or(0);
        let end = blocks.get(i + 1).map_or(self.end, |b| b.start);
        (blocks[i], end)
    }
}

fn local(day: u32, minute: i32) -> LocalTime {
    LocalTime::from_total_secs((day as i64 * DAY_SECS as i64 + minute as i64 * 60).max(0) as u32)
}

/// La agenda del día que contiene el instante `t` y el minuto de `t` en
/// ella (de madrugada sigue siendo la noche del día anterior).
pub fn plan_at(p: &Personality, t: LocalTime) -> (DayPlan, i32) {
    let mut day = t.day;
    let mut m = t.minute() as i32;
    if m < wake_time(p, day) && day > 0 {
        day -= 1;
        m += DAY_MIN as i32;
    }
    (DayPlan::generate(p, day), m)
}

/// Qué le toca hacer en el instante `t`.
pub fn slot_at(p: &Personality, t: LocalTime) -> Slot {
    let (plan, m) = plan_at(p, t);
    let day = plan.day;
    let (block, end) = plan.at(m);
    Slot {
        activity: block.activity,
        start: local(day, block.start),
        end: local(day, end),
    }
}

/// Recorre el día calendario `day` (00:00 a 24:00) entregando
/// `(inicio, fin, actividad)` en minutos desde su medianoche.
pub fn timeline(p: &Personality, day: u32, mut f: impl FnMut(i32, i32, Activity)) {
    let full = DAY_MIN as i32;
    for (plan, offset) in [
        (DayPlan::generate(p, day.saturating_sub(1)), -full),
        (DayPlan::generate(p, day), 0),
    ] {
        let blocks = plan.blocks();
        for (i, b) in blocks.iter().enumerate() {
            let end = blocks.get(i + 1).map_or(plan.end, |n| n.start);
            let (s, e) = ((b.start + offset).max(0), (end + offset).min(full));
            if s < e {
                f(s, e, b.activity);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_day_is_well_formed() {
        let mut most = 0;
        for seed in 0..40 {
            let p = Personality::from_seed(seed);
            for day in 20_000..20_400 {
                let plan = DayPlan::generate(&p, day);
                let b = plan.blocks();
                most = most.max(b.len());
                assert_eq!(b[0].activity, Activity::WakeUp);
                assert_eq!(b[0].start, wake_time(&p, day));
                assert!((4 * 60..=11 * 60 + 30).contains(&b[0].start));
                let last = b.last().unwrap();
                assert_eq!(last.activity, Activity::Sleep);
                assert!(plan.end - last.start >= MIN_SLEEP);
                assert!(b.windows(2).all(|w| w[0].start < w[1].start));
                let meals = b.iter().filter(|x| x.activity == Activity::Eat).count();
                assert_eq!(meals, 3);
            }
        }
        assert!(most < MAX_BLOCKS - 8, "agenda casi llena: {most} bloques");
    }

    #[test]
    fn asleep_at_night_awake_before_noon() {
        for seed in 0..40 {
            let p = Personality::from_seed(seed);
            for day in 20_000..20_060 {
                assert_eq!(
                    slot_at(&p, LocalTime::new(day, 3, 0)).activity,
                    Activity::Sleep
                );
                assert!(
                    !slot_at(&p, LocalTime::new(day, 11, 30))
                        .activity
                        .is_asleep()
                );
            }
        }
    }

    #[test]
    fn same_seed_same_day_same_plan() {
        let p = Personality::from_seed(7);
        assert_eq!(
            DayPlan::generate(&p, 20_100).blocks(),
            DayPlan::generate(&p, 20_100).blocks()
        );
        let q = Personality::from_seed(8);
        assert_ne!(
            DayPlan::generate(&p, 20_100).blocks(),
            DayPlan::generate(&q, 20_100).blocks()
        );
    }

    #[test]
    fn slots_are_continuous() {
        let p = Personality::from_seed(3);
        let mut t = LocalTime::new(20_000, 0, 0);
        let mut prev = slot_at(&p, t);
        for _ in 0..(3 * 1440) {
            let s = slot_at(&p, t);
            assert!(s.start <= t && t < s.end, "{t:?} fuera de {s:?}");
            if s != prev {
                assert_eq!(s.start, prev.end, "hueco entre {prev:?} y {s:?}");
            }
            prev = s;
            t = t.add_secs(60);
        }
    }

    #[test]
    fn timeline_covers_the_whole_day() {
        let p = Personality::from_seed(11);
        let mut next = 0;
        timeline(&p, 20_050, |s, e, _| {
            assert_eq!(s, next);
            next = e;
        });
        assert_eq!(next, DAY_MIN as i32);
    }

    #[test]
    fn workdays_work_and_weekends_rest() {
        for seed in 0..30 {
            let p = Personality::from_seed(seed);
            for day in 20_000..20_028 {
                let plan = DayPlan::generate(&p, day);
                let b = plan.blocks();
                let work: i32 = b
                    .windows(2)
                    .filter(|w| w[0].activity.is_work())
                    .map(|w| w[1].start - w[0].start)
                    .sum();
                match plan.kind {
                    DayKind::Workday => {
                        assert!(work >= 5 * 60, "seed {seed} día {day}: trabajó {work} min");
                        // Trabaja de día: nada de reuniones de noche.
                        assert!(
                            b.iter()
                                .filter(|x| x.activity == Activity::Meeting)
                                .all(|x| (9 * 60 - 60..19 * 60).contains(&x.start)),
                            "reunión fuera de hora"
                        );
                    }
                    _ => assert!(
                        b.iter()
                            .all(|x| !matches!(x.activity, Activity::Meeting | Activity::Trading)),
                        "seed {seed} día {day}: trabaja el fin de semana"
                    ),
                }
            }
        }
    }

    #[test]
    fn night_and_day_things_happen_when_they_should() {
        for seed in 0..30 {
            let p = Personality::from_seed(seed);
            for day in 20_000..20_028 {
                for b in DayPlan::generate(&p, day).blocks() {
                    match b.activity {
                        Activity::Stargaze => {
                            assert!(b.start >= NIGHT, "estrellas a las {}", b.start)
                        }
                        Activity::Garden | Activity::Bananas => {
                            assert!(b.start < DUSK, "jardín a las {}", b.start)
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    #[test]
    fn every_day_is_different_and_the_week_is_varied() {
        let p = Personality::from_seed(5);
        let mut seen = std::collections::HashSet::new();
        let mut plans = std::vec::Vec::new();
        for day in 20_000..20_014 {
            let plan = DayPlan::generate(&p, day);
            let acts: std::vec::Vec<Activity> = plan.blocks().iter().map(|b| b.activity).collect();
            seen.extend(acts.iter().copied());
            plans.push(acts);
        }
        for w in plans.windows(2) {
            assert_ne!(w[0], w[1], "dos días seguidos iguales");
        }
        // En dos semanas hace casi de todo.
        assert!(
            seen.len() >= 20,
            "solo {} actividades distintas: {seen:?}",
            seen.len()
        );
    }
}
