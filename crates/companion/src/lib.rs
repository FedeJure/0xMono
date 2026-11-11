//! El personaje del cuadro: vive su propia vida siguiendo el reloj real.
//!
//! La agenda de cada día se *calcula* a partir de la personalidad y la fecha
//! ([`routine`]), no se simula minuto a minuto. Así, al encender el aparato
//! después de horas apagado, el personaje está haciendo exactamente lo que le
//! tocaba a esa hora, sin tener que "ponerse al día".
//!
//! Sin hardware, sin memoria dinámica y sin coma flotante: corre igual en el
//! simulador y en el ESP32-C3.
#![cfg_attr(not(test), no_std)]

pub mod activity;
pub mod buttons;
pub mod clock;
pub mod companion;
pub mod face;
pub mod lines;
pub mod personality;
pub mod rng;
pub mod routine;
pub mod save;
pub mod talk;

pub use activity::Activity;
pub use buttons::{Button, ButtonState, Gesture, Input, Pad};
pub use clock::LocalTime;
pub use companion::{Companion, Event, Greeting, Reaction, activity_emotion, market};
pub use face::{Emotion, Eyes, Face, FaceAnimator, Mouth};
pub use personality::Personality;
pub use talk::{Kind as TalkKind, Line};
