//! Minijuegos del 0xMono.
//!
//! Sin hardware ni memoria dinámica: corren igual en el simulador y en el
//! ESP32-C3. El firmware (y el simulador) solo hacen tres cosas:
//!
//! 1. [`Running::new`] con el juego elegido en el menú ([`GameId::ALL`]).
//! 2. Cada fotograma, [`Running::update`] con los botones y los milisegundos
//!    que pasaron. Si devuelve [`Status::Exit`], se vuelve al menú.
//! 3. [`Running::draw`] sobre cualquier `DrawTarget` de 240×240. En la placa
//!    se llama una vez por franja (el `DrawTarget` recorta y desplaza), así
//!    que dibujar tiene que ser barato y no depender del orden de llamadas.
//!
//! Para agregar un juego: un módulo nuevo, una variante en [`GameId`] y otra
//! en [`Running`].
#![no_std]

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;

pub mod cocos;
mod gfx;

/// Tamaño de la pantalla.
pub const W: u32 = 240;
pub const H: u32 = 240;

/// Los botones como los ve un juego: si están apretados ahora.
/// A = izquierda, B = saltar / trepar, C = derecha.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub a: bool,
    pub b: bool,
    pub c: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    /// El jugador eligió salir: volver al menú de minijuegos.
    Exit,
}

/// Los juegos que aparecen en el menú.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameId {
    Cocos,
}

impl GameId {
    pub const ALL: &'static [GameId] = &[GameId::Cocos];

    /// Nombre para el menú.
    pub fn title(self) -> &'static str {
        match self {
            GameId::Cocos => "Cocos Rodantes",
        }
    }
}

/// El juego que se está jugando.
pub enum Running {
    Cocos(cocos::Cocos),
}

impl Running {
    /// `seed` viene del generador al azar de la placa.
    pub fn new(id: GameId, seed: u32) -> Self {
        match id {
            GameId::Cocos => Running::Cocos(cocos::Cocos::new(seed)),
        }
    }

    pub fn update(&mut self, keys: Keys, dt_ms: u32) -> Status {
        match self {
            Running::Cocos(g) => g.update(keys, dt_ms),
        }
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(&self, d: &mut D) -> Result<(), D::Error> {
        match self {
            Running::Cocos(g) => g.draw(d),
        }
    }
}
