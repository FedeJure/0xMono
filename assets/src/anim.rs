//! Animaciones del esqueleto: clips muestreados (de Mixamo, procesados por
//! `tools/blender/process_anims.py`) y capas que los combinan por grupos de
//! huesos.
//!
//! Cada clip guarda, por cuadro, la rotación de cada hueso **respecto de su
//! padre** y el desplazamiento de la cadera. Por eso se pueden mezclar: una
//! capa con las piernas de "caminar" y otra con los brazos de "saludar", o
//! una capa encima que gira la cabeza hacia quien mira.
//!
//! ```ignore
//! let mut pose = AnimPose::REST;
//! pose.layer(&Layer::full(walk, t));                                   // todo el cuerpo
//! pose.layer(&Layer { mask: character.mask_from(spine), ..Layer::full(wave, t) }); // tronco y brazos
//! pose.to_pose(&mut scene_pose);
//! ```

use engine3d::Quat;

use crate::Character;
use crate::scene::{MAX_BONES, Pose};

/// Datos de un hueso en un clip: `len` cuadros desde `start` en el pool del
/// clip. `len == 1`: rotación constante; `len == 0`: pose de reposo.
#[derive(Clone, Copy, Debug)]
pub struct Track {
    pub start: u32,
    pub len: u16,
}

pub struct Clip {
    pub name: &'static str,
    pub fps: u8,
    pub frames: u16,
    pub looping: bool,
    /// Lo que avanza el cuerpo en un ciclo (Q8), quitado de la cadera: el
    /// juego mueve al personaje para que los pies no patinen.
    pub advance: [i16; 3],
    /// Cuaterniones (w, x, y, z) en Q14.
    pub pool: &'static [[i16; 4]],
    /// Uno por hueso del personaje.
    pub tracks: &'static [Track],
    /// Desplazamiento de la cadera por cuadro (Q8).
    pub root: &'static [[i16; 3]],
}

impl Clip {
    pub fn duration_ms(&self) -> u32 {
        self.frames as u32 * 1000 / self.fps as u32
    }

    /// Cuadros entre los que cae `t_ms` y la fracción (Q8) entre ellos. Los
    /// clips que no se repiten se quedan en el último cuadro.
    fn locate(&self, t_ms: u32) -> (usize, usize, i32) {
        let n = self.frames.max(1) as u64;
        let pos = t_ms as u64 * self.fps as u64 * 256 / 1000;
        if self.looping {
            let p = pos % (n * 256);
            let f = (p >> 8) as usize;
            (f, (f + 1) % n as usize, (p & 255) as i32)
        } else if pos >= (n - 1) * 256 {
            let last = (n - 1) as usize;
            (last, last, 0)
        } else {
            let f = (pos >> 8) as usize;
            (f, f + 1, (pos & 255) as i32)
        }
    }

    /// Rotación del hueso `bone` (respecto de su padre) en `t_ms`.
    pub fn rotation(&self, bone: usize, t_ms: u32) -> Quat {
        let Some(tr) = self.tracks.get(bone) else {
            return Quat::IDENTITY;
        };
        match tr.len {
            0 => Quat::IDENTITY,
            1 => Quat::from_i16(self.pool[tr.start as usize]),
            _ => {
                let (a, b, t) = self.locate(t_ms);
                let q = |f: usize| {
                    Quat::from_i16(self.pool[tr.start as usize + f.min(tr.len as usize - 1)])
                };
                if t == 0 { q(a) } else { q(a).nlerp(q(b), t) }
            }
        }
    }

    /// Desplazamiento de la cadera en `t_ms` (Q8).
    pub fn root_offset(&self, t_ms: u32) -> [i32; 3] {
        if self.root.is_empty() {
            return [0; 3];
        }
        let (a, b, t) = self.locate(t_ms);
        let (ra, rb) = (self.root[a], self.root[b.min(self.root.len() - 1)]);
        core::array::from_fn(|k| ra[k] as i32 + (((rb[k] as i32 - ra[k] as i32) * t) >> 8))
    }
}

/// Grupo de huesos: el bit `i` es el hueso `i`.
pub type BoneMask = u128;
pub const ALL_BONES: BoneMask = u128::MAX;

/// Un clip aplicado a un grupo de huesos con un peso.
#[derive(Clone, Copy)]
pub struct Layer<'a> {
    pub clip: &'a Clip,
    pub time_ms: u32,
    pub mask: BoneMask,
    /// 0..=256: cuánto reemplaza a lo que ya había (256 = del todo).
    pub weight: i32,
}

impl<'a> Layer<'a> {
    /// Todo el cuerpo, peso completo.
    pub fn full(clip: &'a Clip, time_ms: u32) -> Self {
        Self {
            clip,
            time_ms,
            mask: ALL_BONES,
            weight: 256,
        }
    }
}

/// Pose en cuaterniones, para ir mezclando capas.
#[derive(Clone)]
pub struct AnimPose {
    pub rot: [Quat; MAX_BONES],
    pub root: [i32; 3],
}

impl AnimPose {
    pub const REST: AnimPose = AnimPose {
        rot: [Quat::IDENTITY; MAX_BONES],
        root: [0; 3],
    };

    /// Aplica una capa sobre lo que ya hay.
    pub fn layer(&mut self, l: &Layer) {
        let w = l.weight.clamp(0, 256);
        if w == 0 {
            return;
        }
        for b in 0..l.clip.tracks.len().min(MAX_BONES) {
            if l.mask & (1 << b) == 0 {
                continue;
            }
            let q = l.clip.rotation(b, l.time_ms);
            self.rot[b] = if w == 256 { q } else { self.rot[b].nlerp(q, w) };
        }
        // La cadera (hueso 0) lleva el desplazamiento del cuerpo.
        if l.mask & 1 != 0 {
            let r = l.clip.root_offset(l.time_ms);
            for (dst, src) in self.root.iter_mut().zip(r) {
                *dst += ((src - *dst) * w) >> 8;
            }
        }
    }

    /// Pasa a matrices para [`crate::scene::CharacterScene::submit`].
    pub fn to_pose(&self, out: &mut Pose) {
        for (m, q) in out.rot.iter_mut().zip(self.rot.iter()) {
            *m = q.to_mat3();
        }
        out.root = self.root;
    }
}

/// Un clip en reproducción.
#[derive(Clone, Copy)]
struct Playing {
    clip: &'static Clip,
    t_ms: u32,
}

/// Fundido de 0 a 256 en `total` ms (sin fundido si `total` es 0).
#[derive(Clone, Copy)]
struct Fade {
    total: u32,
    left: u32,
}

impl Fade {
    const DONE: Fade = Fade { total: 0, left: 0 };

    fn new(ms: u32) -> Fade {
        Fade {
            total: ms,
            left: ms,
        }
    }

    fn weight(&self) -> i32 {
        if self.total == 0 {
            256
        } else {
            (256 - (self.left as u64 * 256 / self.total as u64) as i32).clamp(0, 256)
        }
    }

    fn tick(&mut self, dt: u32) {
        self.left = self.left.saturating_sub(dt);
    }
}

/// Reproduce una animación base con fundidos al cambiarla, y una capa
/// encima (por ejemplo, saludar con el tronco mientras las piernas siguen
/// con lo suyo).
pub struct Animator {
    base: Playing,
    prev: Option<Playing>,
    fade: Fade,
    overlay: Option<(Playing, BoneMask)>,
    overlay_fade: Fade,
    /// La capa se está yendo (fundido hacia afuera).
    overlay_out: bool,
}

impl Animator {
    pub fn new(clip: &'static Clip) -> Self {
        Self {
            base: Playing { clip, t_ms: 0 },
            prev: None,
            fade: Fade::DONE,
            overlay: None,
            overlay_fade: Fade::DONE,
            overlay_out: false,
        }
    }

    pub fn clip(&self) -> &'static Clip {
        self.base.clip
    }

    /// Tiempo de la animación base (ms desde que empezó).
    pub fn time_ms(&self) -> u32 {
        self.base.t_ms
    }

    /// Pasa a `clip` con un fundido de `fade_ms`. Si ya es el actual, sigue.
    pub fn play(&mut self, clip: &'static Clip, fade_ms: u32) {
        if core::ptr::eq(clip, self.base.clip) {
            return;
        }
        self.prev = Some(self.base);
        self.base = Playing { clip, t_ms: 0 };
        self.fade = Fade::new(fade_ms);
    }

    /// Pone `clip` encima, solo en los huesos de `mask`.
    pub fn overlay(&mut self, clip: &'static Clip, mask: BoneMask, fade_ms: u32) {
        if let Some((o, _)) = &self.overlay
            && core::ptr::eq(o.clip, clip)
            && !self.overlay_out
        {
            return;
        }
        self.overlay = Some((Playing { clip, t_ms: 0 }, mask));
        self.overlay_fade = Fade::new(fade_ms);
        self.overlay_out = false;
    }

    /// Saca la capa de encima con un fundido.
    pub fn clear_overlay(&mut self, fade_ms: u32) {
        if self.overlay.is_some() && !self.overlay_out {
            self.overlay_out = true;
            self.overlay_fade = Fade::new(fade_ms);
        }
    }

    /// ¿Terminó la animación base? (las que se repiten nunca terminan)
    pub fn finished(&self) -> bool {
        !self.base.clip.looping && self.base.t_ms >= self.base.clip.duration_ms()
    }

    pub fn update(&mut self, dt_ms: u32) {
        self.base.t_ms = self.base.t_ms.saturating_add(dt_ms);
        if let Some(p) = &mut self.prev {
            p.t_ms = p.t_ms.saturating_add(dt_ms);
        }
        self.fade.tick(dt_ms);
        if self.fade.left == 0 {
            self.prev = None;
        }
        if let Some((o, _)) = &mut self.overlay {
            o.t_ms = o.t_ms.saturating_add(dt_ms);
        }
        self.overlay_fade.tick(dt_ms);
        if self.overlay_out && self.overlay_fade.left == 0 {
            self.overlay = None;
            self.overlay_out = false;
        }
    }

    pub fn pose(&self, out: &mut AnimPose) {
        *out = AnimPose::REST;
        if let Some(p) = &self.prev {
            out.layer(&Layer::full(p.clip, p.t_ms));
        }
        out.layer(&Layer {
            weight: if self.prev.is_some() {
                self.fade.weight()
            } else {
                256
            },
            ..Layer::full(self.base.clip, self.base.t_ms)
        });
        if let Some((o, mask)) = &self.overlay {
            let w = self.overlay_fade.weight();
            out.layer(&Layer {
                clip: o.clip,
                time_ms: o.t_ms,
                mask: *mask,
                weight: if self.overlay_out { 256 - w } else { w },
            });
        }
    }
}

impl Character {
    /// El hueso `bone` y todos los que cuelgan de él.
    pub fn mask_from(&self, bone: usize) -> BoneMask {
        let mut m: BoneMask = 1 << bone;
        for (i, b) in self.bones.iter().enumerate() {
            if let Some(p) = b.parent
                && m & (1 << p) != 0
            {
                m |= 1 << i;
            }
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mono::MONO;
    use crate::mono_anims::{CLIPS, clip};

    #[test]
    fn masks_follow_the_hierarchy() {
        let spine = MONO.bone(":Spine").unwrap();
        let upper = MONO.mask_from(spine);
        let head = MONO.bone(":Head").unwrap();
        let hand = MONO.bone(":LeftHand").unwrap();
        let leg = MONO.bone(":LeftUpLeg").unwrap();
        assert!(upper & (1 << head) != 0 && upper & (1 << hand) != 0);
        assert!(
            upper & (1 << leg) == 0 && upper & 1 == 0,
            "las piernas y la cadera no"
        );
    }

    #[test]
    fn every_clip_matches_the_skeleton() {
        for c in CLIPS.iter() {
            assert_eq!(c.tracks.len(), MONO.bones.len(), "{}", c.name);
            assert_eq!(c.root.len(), c.frames as usize, "{}", c.name);
            for t in c.tracks {
                assert!(t.len == 0 || t.len == 1 || t.len == c.frames, "{}", c.name);
                assert!(t.start as usize + t.len as usize <= c.pool.len());
            }
        }
    }

    #[test]
    fn loops_wrap_and_one_shots_hold_the_last_frame() {
        let idle = &CLIPS[clip::IDLE];
        assert!(idle.looping);
        let d = idle.duration_ms();
        let b = MONO.bone(":Spine1").unwrap();
        assert_eq!(idle.rotation(b, 100), idle.rotation(b, 100 + d));
        let wave = &CLIPS[clip::WAVING];
        assert!(!wave.looping);
        let end = wave.duration_ms();
        assert_eq!(wave.rotation(b, end), wave.rotation(b, end * 3));
    }

    #[test]
    fn layers_mix_by_bone_group() {
        let walk = &CLIPS[clip::WALKING];
        let wave = &CLIPS[clip::WAVING];
        let spine = MONO.bone(":Spine").unwrap();
        let (arm, leg) = (
            MONO.bone(":RightArm").unwrap(),
            MONO.bone(":LeftUpLeg").unwrap(),
        );
        let t = 300;
        let mut p = AnimPose::REST;
        p.layer(&Layer::full(walk, t));
        p.layer(&Layer {
            mask: MONO.mask_from(spine),
            ..Layer::full(wave, t)
        });
        assert_eq!(p.rot[leg], walk.rotation(leg, t), "piernas de caminar");
        assert_eq!(p.rot[arm], wave.rotation(arm, t), "brazos de saludar");
    }

    #[test]
    fn animator_crossfades_and_overlays() {
        let idle = &CLIPS[clip::IDLE];
        let jack = &CLIPS[clip::JUMPING_JACK];
        let wave = &CLIPS[clip::WAVING];
        let arm = MONO.bone(":RightArm").unwrap();
        let mut a = Animator::new(idle);
        a.update(500);
        a.play(jack, 400);
        a.update(200);
        let mut p = AnimPose::REST;
        a.pose(&mut p);
        let (from, to) = (idle.rotation(arm, 700), jack.rotation(arm, 200));
        assert!(
            p.rot[arm] != from && p.rot[arm] != to,
            "a mitad del fundido"
        );
        a.update(300);
        a.pose(&mut p);
        assert_eq!(p.rot[arm], jack.rotation(arm, 500), "terminó el fundido");

        let spine = MONO.bone(":Spine").unwrap();
        a.overlay(wave, MONO.mask_from(spine), 0);
        a.update(100);
        a.pose(&mut p);
        assert_eq!(p.rot[arm], wave.rotation(arm, 100));
        a.clear_overlay(0);
        a.update(1);
        a.pose(&mut p);
        assert_eq!(p.rot[arm], jack.rotation(arm, 601));
    }
}
