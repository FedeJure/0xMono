//! El simulador son ejemplos: cada uno abre una parte del 0xMono en la PC,
//! con el mismo código que corre en la placa. Este programa solo los lista.
//!
//! ```text
//! cargo run -p simulator
//! ```

const EXAMPLES: &[(&str, &str)] = &[
    (
        "habitat",
        "el mono en su casa 3D, con la hora real (A/S/D le hablan, T acelera)",
    ),
    (
        "menu",
        "el menú y las pantallas de la configuración (--serve: la página en el navegador)",
    ),
    ("games", "los minijuegos"),
    ("talk", "lo que dice y piensa en cada actividad"),
    ("week", "la agenda de la semana del personaje guardado"),
    ("character", "visor del personaje y sus animaciones"),
    ("ntp", "la hora por internet, como la pide el mono"),
    ("engine_demo", "demo y medición del motor 3D"),
    ("habitat_bench", "las escenas de medición del hábitat"),
    ("miner", "modo minero: la página y las pantallas"),
    (
        "miner_view",
        "modo minero: la pantalla de la placa en vivo, por USB",
    ),
    (
        "stratum",
        "modo minero: el cliente Stratum contra un pool real",
    ),
];

fn main() {
    println!("Simulador del 0xMono. Elegí un ejemplo:\n");
    for (name, what) in EXAMPLES {
        println!("  cargo run -p simulator --release --example {name:<14} # {what}");
    }
    println!("\nCada ejemplo explica sus opciones al principio de simulator/examples/<nombre>.rs.");
}
