//! Personajes exportados de Blender y su puesta en escena.
//!
//! Flujo: Blender → `tools/blender/export_character.py` → `assets/<nombre>.json`
//! → `tools/build_assets.py` → `assets/src/<nombre>.rs` (datos `static`).
//! [`scene::CharacterScene`] los pone en el renderer: materiales, expresiones
//! de la cara y pose del esqueleto.
//!
//! Animaciones: Mixamo → `tools/blender/process_anims.py` (retarget y
//! corrección de brazos contra la cabeza) → `assets/<nombre>_anims.json` →
//! `tools/build_assets.py` → `assets/src/<nombre>_anims.rs`. Se reproducen y
//! combinan por capas con [`anim`].
#![cfg_attr(not(test), no_std)]

pub mod anim;
pub mod house;
pub mod mono;
pub mod mono_anims;
pub mod nav;
pub mod scene;
pub mod shots;
pub mod stage;
pub mod world;

use engine3d::material::TEX_COLORS;
use engine3d::{Mesh, Rgb, Texture};

/// Una parte con textura: ojos, boca, un logo...
pub struct TexturedPart {
    /// Nombre del material en Blender.
    pub name: &'static str,
    pub texture: Texture,
    /// Material de cada valor de texel (índices de la tabla del personaje).
    pub colors: [u8; TEX_COLORS],
    /// Desplazamiento en texels de una celda (expresión) a la siguiente.
    pub cell_step: [i16; 2],
    /// Celdas de la tira; 1 si no es una tira de expresiones.
    pub cells: u8,
}

pub struct Bone {
    pub name: &'static str,
    /// Siempre aparece antes que sus hijos.
    pub parent: Option<u8>,
    /// Cabeza (articulación) en reposo, en unidades Q8 del motor.
    pub head: [i32; 3],
}

pub struct Character {
    pub mesh: Mesh<'static>,
    /// Colores lisos: el material `i` es `colors[i]`.
    pub colors: &'static [Rgb],
    /// Partes con textura: la `j` es el material `colors.len() + j`.
    pub parts: &'static [TexturedPart],
    pub bones: &'static [Bone],
}

impl Character {
    /// Material y datos de la parte con textura `name`.
    pub fn part(&self, name: &str) -> Option<(usize, &TexturedPart)> {
        self.parts
            .iter()
            .position(|p| p.name == name)
            .map(|j| (self.colors.len() + j, &self.parts[j]))
    }

    /// Índice del hueso cuyo nombre termina en `suffix` (los de Mixamo
    /// empiezan con `mixamorig:`).
    pub fn bone(&self, suffix: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name.ends_with(suffix))
    }
}
