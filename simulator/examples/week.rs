//! La agenda de la semana del personaje guardado (o de una semilla).
//!
//! ```text
//! cargo run -p simulator --example week [semilla]
//! ```

use chrono::Local;
use companion::clock::{LocalTime, weekday};
use companion::routine::DayPlan;
use companion::{Personality, save};

fn main() {
    let seed = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .or_else(|| {
            std::fs::read("companion.bin")
                .ok()
                .and_then(|b| save::decode(&b))
                .map(|d| d.seed)
        })
        .unwrap_or(5);
    let p = Personality::from_seed(seed);
    println!("semilla {seed}: {p:?}\n");
    let today = LocalTime::from_unix(Local::now().timestamp() as u64, 0).day;
    for day in today..today + 7 {
        let plan = DayPlan::generate(&p, day);
        println!(
            "{} ({:?}, humor {})",
            weekday(day).short_name(),
            plan.kind,
            plan.mood.label()
        );
        let b = plan.blocks();
        for (i, x) in b.iter().enumerate() {
            let end = b.get(i + 1).map_or(plan.end, |n| n.start);
            let hm = |m: i32| format!("{:02}:{:02}", m / 60 % 24, m % 60);
            println!("  {}-{}  {}", hm(x.start), hm(end), x.activity.label());
        }
        println!();
    }
}
