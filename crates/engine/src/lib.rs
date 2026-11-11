//! Motor 3D por software para microcontroladores sin FPU.
//!
//! Pensado para ESP32-C3 (RISC-V 160 MHz) + ST7789 240×240 RGB565:
//! - todo en punto fijo ([`fixed`]),
//! - sin memoria dinámica: búferes de tamaño fijo con const generics,
//! - rasterizado por franjas con z-buffer ([`raster`]), para no necesitar un
//!   fotograma completo en RAM y poder solapar dibujo y envío por DMA,
//! - materiales toon con dithering ordenado ([`material`]),
//! - skinning rígido: cada vértice pertenece a un hueso ([`mesh`]),
//! - texturas de 4 bits cuyos texels eligen material, para caras con
//!   expresiones intercambiables ([`material::Texture`]).
#![cfg_attr(not(test), no_std)]

pub mod demo;
pub mod fixed;
pub mod material;
pub mod mesh;
pub mod meshgen;
pub mod raster;
pub mod render;

pub use fixed::{Angle, Mat3, Quat, Transform};
pub use material::{Material, PixelOrder, Rgb, TexMap, Texture};
pub use mesh::Mesh;
pub use raster::Band;
pub use render::{Bounds, Camera, FrameStats, Light, Mark, Renderer};
