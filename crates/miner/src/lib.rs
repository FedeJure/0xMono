//! Modo minero del mono: configuración desde el celular, validación de la
//! dirección de Bitcoin y las pantallas de ese modo.
//!
//! Sin hardware ni memoria dinámica: corre igual en el simulador (con tests)
//! y en el ESP32-C3.
#![cfg_attr(not(test), no_std)]

pub mod address;
pub mod config;
pub mod json;
pub mod mirror;
pub mod portal;
pub mod screen;
pub mod stratum;
pub mod work;

pub use captive::{Failure, dns, failure, http};
pub use config::Config;
