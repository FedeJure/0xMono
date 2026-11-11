//! Pone un [`Character`] en el renderer: materiales toon a partir de sus
//! colores, expresiones (celdas de las tiras de ojos y boca) y pose del
//! esqueleto por cinemática directa.

use engine3d::fixed::{Mat3, Transform};
use engine3d::material::{Material, PixelOrder, Rgb, TexMap};
use engine3d::{Light, Renderer};

use crate::Character;

/// Huesos máximos por personaje (Mixamo usa ~40-65).
pub const MAX_BONES: usize = 72;

/// Rotación local de cada hueso alrededor de su articulación, respecto de la
/// pose de reposo. La identidad en todos es la pose de reposo.
#[derive(Clone)]
pub struct Pose {
    pub rot: [Mat3; MAX_BONES],
    /// Desplazamiento de la cadera respecto de su reposo (Q8): agacharse,
    /// sentarse, saltar.
    pub root: [i32; 3],
}

impl Pose {
    pub const REST: Pose = Pose {
        rot: [Mat3::IDENTITY; MAX_BONES],
        root: [0; 3],
    };
}

impl Default for Pose {
    fn default() -> Self {
        Self::REST
    }
}

/// Luz por defecto: arriba a la izquierda y un poco de frente.
pub const fn light() -> Light {
    Light {
        dir: [-7373, 11796, -8684],
        ambient: 22_000,
    }
}

/// Rampa toon a partir del color base: sombra más oscura y un poco más fría,
/// brillo hacia el blanco.
pub fn toon(base: Rgb, order: PixelOrder) -> Material {
    let shadow = Rgb(
        (base.0 as u32 * 45 / 100 + 8) as u8,
        (base.1 as u32 * 45 / 100 + 10) as u8,
        (base.2 as u32 * 50 / 100 + 18) as u8,
    );
    let highlight = base.lerp(Rgb(255, 250, 240), 1, 3);
    Material::toon(shadow, base, highlight, order)
}

pub struct CharacterScene {
    pub character: &'static Character,
    eyes: Option<usize>,
    mouth: Option<usize>,
}

impl CharacterScene {
    pub fn new(character: &'static Character) -> Self {
        assert!(character.bones.len() <= MAX_BONES, "demasiados huesos");
        Self {
            character,
            eyes: character.part("Eye").map(|(m, _)| m),
            mouth: character.part("Mouth").map(|(m, _)| m),
        }
    }

    /// Carga los materiales del personaje en el renderer.
    pub fn setup<const V: usize, const T: usize>(&self, r: &mut Renderer<V, T>, order: PixelOrder) {
        let c = self.character;
        assert!(
            c.colors.len() + c.parts.len() <= r.materials.len(),
            "demasiados materiales"
        );
        for (i, &rgb) in c.colors.iter().enumerate() {
            r.materials[i] = toon(rgb, order);
        }
        for (j, p) in c.parts.iter().enumerate() {
            r.materials[c.colors.len() + j] = Material::textured(TexMap {
                texture: p.texture,
                offset: [0, 0],
                colors: p.colors,
            });
        }
    }

    /// Elige la celda `cell` de la tira del material `material`.
    pub fn set_cell<const V: usize, const T: usize>(
        &self,
        r: &mut Renderer<V, T>,
        material: usize,
        cell: u8,
    ) {
        let c = self.character;
        let Some(part) = material
            .checked_sub(c.colors.len())
            .and_then(|j| c.parts.get(j))
        else {
            return;
        };
        let cell = cell.min(part.cells.saturating_sub(1)) as i16;
        if let Some(map) = r.materials[material].texture.as_mut() {
            map.offset = [part.cell_step[0] * cell, part.cell_step[1] * cell];
        }
    }

    /// Pone una expresión: celda de la tira de ojos y de la de boca.
    pub fn set_face<const V: usize, const T: usize>(
        &self,
        r: &mut Renderer<V, T>,
        eyes: u8,
        mouth: u8,
    ) {
        if let Some(m) = self.eyes {
            self.set_cell(r, m, eyes);
        }
        if let Some(m) = self.mouth {
            self.set_cell(r, m, mouth);
        }
    }

    /// Transformación de cada hueso: `model` (vista · modelo) por la cadena
    /// de rotaciones de la pose, cada una alrededor de su articulación.
    pub fn bone_transforms(
        &self,
        model: &Transform,
        pose: &Pose,
        out: &mut [Transform; MAX_BONES],
    ) {
        let root = model.then(&Transform::translate(pose.root));
        for (i, b) in self.character.bones.iter().enumerate() {
            let parent = b.parent.map_or(root, |p| out[p as usize]);
            out[i] = if pose.rot[i] == Mat3::IDENTITY {
                parent
            } else {
                let h = b.head;
                parent
                    .then(&Transform::new(pose.rot[i], h))
                    .then(&Transform::translate([-h[0], -h[1], -h[2]]))
            };
        }
    }

    /// Envía el personaje al renderer. Devuelve `false` si no cupo.
    pub fn submit<const V: usize, const T: usize>(
        &self,
        r: &mut Renderer<V, T>,
        model: &Transform,
        pose: &Pose,
    ) -> bool {
        let mut bones = [Transform::IDENTITY; MAX_BONES];
        self.bone_transforms(model, pose, &mut bones);
        r.submit(
            &self.character.mesh,
            &bones[..self.character.bones.len().max(1)],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mono::MONO;
    use engine3d::fixed::deg;

    #[test]
    fn exported_character_is_consistent() {
        let c = &MONO;
        assert!(c.mesh.is_valid());
        assert!(c.colors.len() + c.parts.len() <= engine3d::material::MAX_MATERIALS);
        let n = (c.colors.len() + c.parts.len()) as u8;
        assert!(c.mesh.tri_materials.iter().all(|&m| m < n));
        assert!(c.mesh.bones.iter().all(|&b| (b as usize) < c.bones.len()));
        for (i, b) in c.bones.iter().enumerate() {
            assert!(b.parent.is_none_or(|p| (p as usize) < i));
        }
        let (_, eyes) = c.part("Eye").expect("falta la parte Eye");
        let (_, mouth) = c.part("Mouth").expect("falta la parte Mouth");
        // Las celdas de `companion::face::{Eyes, Mouth}`.
        assert_eq!((eyes.cells, mouth.cells), (6, 10));
        assert!(c.bone("Head").is_some());
    }

    #[test]
    fn rotating_a_bone_moves_its_children_around_the_joint() {
        let scene = CharacterScene::new(&MONO);
        let head = MONO.bone("Head").unwrap();
        let top = MONO.bone("HeadTop_End").unwrap();
        let mut pose = Pose::REST;
        pose.rot[head] = Mat3::rot_z(deg(90));
        let mut t = [Transform::IDENTITY; MAX_BONES];
        scene.bone_transforms(&Transform::IDENTITY, &pose, &mut t);
        // La articulación del cuello no se mueve; la punta de la cabeza gira.
        let h = MONO.bones[head].head;
        let p = t[head].apply(h);
        assert!((0..3).all(|k| (p[k] - h[k]).abs() <= 1), "{p:?} vs {h:?}");
        assert_eq!(t[top], t[head], "los hijos heredan la transformación");
        let tip = t[top].apply(MONO.bones[top].head);
        assert!(
            (tip[1] - h[1]).abs() < 40,
            "la punta quedó de costado: {tip:?}"
        );
    }
}
