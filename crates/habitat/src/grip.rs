//! Objetos en la mano: cada uno se agarra de una forma ([`Grip`]) definida
//! en el marco de la mano, así acompaña el giro de la muñeca.
//!
//! El marco sale de los huesos en reposo, igual que en
//! `tools/make_clips.py` (`Rig::hand_frame`), que cierra los dedos de las
//! animaciones alrededor del objeto:
//! - `f`: de la muñeca hacia los dedos;
//! - `n`: hacia la palma (hacia donde se doblan los dedos);
//! - `a`: a lo largo de los nudillos, hacia el pulgar.
//!
//! En un puño, lo que se agarra (un mango, el mate) atraviesa la mano a lo
//! largo de `a`: con el pulgar arriba, sale para arriba por el lado del
//! pulgar y para abajo por el del meñique.

use assets::mono::MONO;
use engine3d::{Mat3, Transform};
use libm::sqrtf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
}

/// Cómo se agarra un objeto. Las direcciones son combinaciones de los ejes
/// de la mano `(f, n, a)`; las distancias, en metros.
#[derive(Clone, Copy, Debug)]
pub struct Grip {
    pub side: Side,
    /// Dónde va el origen del objeto, desde la muñeca.
    pub at: [f32; 3],
    /// Hacia dónde apunta el eje Y del objeto (arriba en Blender).
    pub y: [f32; 3],
    /// Hacia dónde apunta su eje Z (atrás en Blender: el frente es −Z).
    pub z: [f32; 3],
}

type V = [f32; 3];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn scale(a: V, k: f32) -> V {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: V) -> V {
    scale(a, 1.0 / sqrtf(dot(a, a)).max(1e-6))
}

/// Ejes de la mano en reposo (espacio del personaje) y su muñeca.
struct HandFrame {
    hand: usize,
    wrist: V,
    f: V,
    n: V,
    a: V,
}

fn joint(name: &str) -> V {
    let b = MONO
        .bone(name)
        .unwrap_or_else(|| panic!("falta el hueso {name}"));
    MONO.bones[b].head.map(|c| c as f32)
}

fn hand_frame(side: Side) -> HandFrame {
    let [hand, index1, index2, index4, thumb2] = match side {
        Side::Left => [
            ":LeftHand",
            ":LeftHandIndex1",
            ":LeftHandIndex2",
            ":LeftHandIndex4",
            ":LeftHandThumb2",
        ],
        Side::Right => [
            ":RightHand",
            ":RightHandIndex1",
            ":RightHandIndex2",
            ":RightHandIndex4",
            ":RightHandThumb2",
        ],
    };
    let wrist = joint(hand);
    let f = norm(sub(joint(index1), wrist));
    let d = sub(joint(index4), joint(index2));
    let n = norm(sub(d, scale(f, dot(d, f))));
    let mut a = norm(cross(f, n));
    if dot(a, sub(joint(thumb2), wrist)) < 0.0 {
        a = scale(a, -1.0);
    }
    HandFrame {
        hand: MONO.bone(hand).expect("mano"),
        wrist,
        f,
        n,
        a,
    }
}

impl Grip {
    /// Transformación del objeto: `bones` son las de cada hueso del último
    /// personaje enviado (espacio de cámara, ver `Stage::bones`).
    pub fn transform(&self, bones: &[Transform]) -> Transform {
        let h = hand_frame(self.side);
        let along = |c: [f32; 3]| {
            [
                h.f[0] * c[0] + h.n[0] * c[1] + h.a[0] * c[2],
                h.f[1] * c[0] + h.n[1] * c[1] + h.a[1] * c[2],
                h.f[2] * c[0] + h.n[2] * c[1] + h.a[2] * c[2],
            ]
        };
        let y = norm(along(self.y));
        let x = norm(cross(y, along(self.z)));
        let z = cross(x, y);
        let q = |v: f32| (v * 16384.0) as i32;
        let m = Mat3([
            [q(x[0]), q(y[0]), q(z[0])],
            [q(x[1]), q(y[1]), q(z[1])],
            [q(x[2]), q(y[2]), q(z[2])],
        ]);
        let o = along(scale(self.at, 1024.0));
        let at = [
            (h.wrist[0] + o[0]) as i32,
            (h.wrist[1] + o[1]) as i32,
            (h.wrist[2] + o[2]) as i32,
        ];
        bones[h.hand].then(&Transform::new(m, at))
    }
}

/// Eje de la mano con signo, para escribir los agarres.
pub const F: [f32; 3] = [1.0, 0.0, 0.0];
pub const N: [f32; 3] = [0.0, 1.0, 0.0];
pub const A: [f32; 3] = [0.0, 0.0, 1.0];

pub const fn neg(v: [f32; 3]) -> [f32; 3] {
    [-v[0], -v[1], -v[2]]
}
