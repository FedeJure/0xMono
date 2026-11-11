//! Muestras de lo que dice y piensa el mono en cada actividad.
//!
//! ```text
//! cargo run -p simulator --example talk [por actividad] [semilla]
//! ```

use companion::clock::LocalTime;
use companion::rng::Rng;
use companion::routine::{day_kind, mood};
use companion::talk::{Context, Kind, compose};
use companion::{Activity, Personality, market};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let per: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(6);
    let seed: u32 = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(244_792_600);
    let p = Personality::from_seed(seed);
    let mut rng = Rng::new(seed ^ 0x51);
    println!(
        "{} reglas · personaje {seed}\n",
        companion::lines::RULES.len()
    );
    for act in Activity::ALL {
        println!("== {} ==", act.label());
        let mut recent = Vec::new();
        for k in 0..per * 2 {
            // Distintos días y horas (semana y finde, mañana, tarde y noche).
            let day = 20_000 + (k as u32 % 7);
            let hour = [9u8, 13, 17, 22][k % 4];
            let now = LocalTime::new(day, hour, (k * 7 % 60) as u8);
            let kind = if k % 2 == 0 { Kind::Say } else { Kind::Think };
            let ctx = Context {
                activity: act,
                now,
                kind: day_kind(day),
                mood: mood(&p, day),
                personality: &p,
                market: market(seed, now.add_secs(k as u32 * 997)),
                did: Some(Activity::ALL[k * 5 % 26]),
                next: Some(Activity::ALL[(k * 3 + 7) % 26]),
            };
            if let Some(l) = compose(kind, &ctx, &recent, &mut rng) {
                recent.push(l.rule);
                let tag = if kind == Kind::Say {
                    "dice   "
                } else {
                    "piensa "
                };
                println!(
                    "  {} {:<9} {tag}{}",
                    now.weekday().short_name(),
                    format!("{:02}h", hour),
                    l.as_str()
                );
            }
        }
        println!();
    }
}
