//! Cada personaje nace con una semilla y de ella sale su personalidad: si es
//! madrugador o noctámbulo y cuánto le gusta cada cosa. Solo hace falta
//! guardar la semilla.

use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Personality {
    pub seed: u32,
    /// Minutos que corre su reloj interno: negativo = madrugador.
    pub chronotype: i16,
    /// Gustos de 10 (poco) a 100 (mucho).
    pub sporty: u8,
    pub bookish: u8,
    /// Ganas de programar (además de lo que ya programa por trabajo).
    pub geeky: u8,
    pub green_thumb: u8,
    pub sociable: u8,
    /// Guitarra y baile.
    pub musical: u8,
    /// Consola y películas.
    pub gamer: u8,
    /// Cocinar.
    pub foodie: u8,
    /// Mate, mirar por la ventana, no hacer nada.
    pub chill: u8,
}

impl Personality {
    pub fn from_seed(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let mut taste = || rng.range(10, 100) as u8;
        // El orden importa: los gustos nuevos van al final para que los
        // personajes guardados conserven los de antes.
        let (sporty, bookish, geeky, green_thumb, sociable) =
            (taste(), taste(), taste(), taste(), taste());
        let (musical, gamer, foodie, chill) = (taste(), taste(), taste(), taste());
        Self {
            seed,
            chronotype: Rng::new(seed ^ 0xC10C).range(-60, 75) as i16,
            sporty,
            bookish,
            geeky,
            green_thumb,
            sociable,
            musical,
            gamer,
            foodie,
            chill,
        }
    }
}
