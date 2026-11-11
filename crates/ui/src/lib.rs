//! Lo que el 0xMono dibuja fuera de la casa en 3D: el menú (A+C mantenidos
//! desde cualquier lado), los avisos y las pantallas de la configuración y de
//! la hora por internet.
//!
//! Todo se dibuja con `embedded-graphics` sobre cualquier `DrawTarget` de
//! 240×240. En la placa se dibuja por franjas ([`band`]) que van directo a la
//! pantalla; en el simulador, sobre una imagen entera.
#![cfg_attr(not(test), no_std)]

pub mod band;
pub mod menu;
pub mod paint;
pub mod screens;

pub use band::Band;
pub use menu::{Action, Menu, Message, Nav};
pub use screens::Screen;

/// Lado de la pantalla.
pub const SIZE: u32 = 240;
