//! Aritmética de punto fijo.
//!
//! El ESP32-C3 (RV32IMC) no tiene FPU: cada operación `f32` es una llamada a
//! una rutina de software de decenas de ciclos. En cambio `mul`/`mulh` (32×32→64)
//! y `div` (32 bits) son instrucciones de hardware. Todo el motor usa enteros:
//!
//! | Magnitud               | Formato                                  |
//! |------------------------|------------------------------------------|
//! | Ángulos                | `u16`, 65536 = vuelta completa           |
//! | Senos, matrices        | Q14 (`16384` = 1.0)                      |
//! | Posiciones de modelo   | Q8 (`256` = 1 unidad), `i16` en la malla |
//! | Normales               | `i8`, `127` = 1.0                        |
//! | Pantalla               | 16.16 (`65536` = 1 píxel)                |

/// Ángulo: 65536 = 360°.
pub type Angle = u16;

pub const ONE_Q14: i32 = 1 << 14;
pub const ONE_Q8: i32 = 1 << 8;

/// Convierte grados (entero) a [`Angle`].
pub const fn deg(d: i32) -> Angle {
    (d.rem_euclid(360) as u32 * 65536 / 360) as u16
}

const LUT_LEN: usize = 1024;

/// sen(i · 2π / 1024) en Q14, calculado en tiempo de compilación.
static SIN_LUT: [i16; LUT_LEN] = build_sin_lut();

/// sen(x) para x ∈ [0, π/2] con x en Q30, por serie de Taylor en enteros.
const fn sin_q30(x: i64) -> i64 {
    let x2 = (x * x) >> 30;
    let mut term = x;
    let mut sum = x;
    let mut k = 1;
    while k <= 6 {
        term = -((term * x2) >> 30) / ((2 * k) * (2 * k + 1));
        sum += term;
        k += 1;
    }
    sum
}

const fn build_sin_lut() -> [i16; LUT_LEN] {
    const PI_Q30: i64 = 3_373_259_426; // π · 2^30
    let mut lut = [0i16; LUT_LEN];
    let quarter = LUT_LEN / 4;
    let mut i = 0;
    while i < LUT_LEN {
        let q = i / quarter;
        let r = i % quarter;
        let idx = if q & 1 == 0 { r } else { quarter - r };
        let x = idx as i64 * PI_Q30 / (LUT_LEN as i64 / 2);
        let s = (sin_q30(x) + (1 << 15)) >> 16; // Q30 → Q14 redondeando
        lut[i] = if q < 2 { s as i16 } else { -s as i16 };
        i += 1;
    }
    lut
}

/// Seno en Q14 con interpolación lineal entre entradas de la tabla.
#[inline]
pub fn sin(a: Angle) -> i32 {
    let i = (a >> 6) as usize;
    let frac = (a & 63) as i32;
    let s0 = SIN_LUT[i] as i32;
    let s1 = SIN_LUT[(i + 1) & (LUT_LEN - 1)] as i32;
    s0 + (((s1 - s0) * frac) >> 6)
}

#[inline]
pub fn cos(a: Angle) -> i32 {
    sin(a.wrapping_add(16384))
}

/// Raíz cuadrada entera (piso), para normalizar vectores en tiempo de carga.
pub const fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Raíz cuadrada entera de 32 bits, bit a bit: sin divisiones (útil en tiempo
/// de ejecución en el C3, donde `isqrt` de 64 bits dividiría por software).
pub const fn isqrt32(mut n: u32) -> u32 {
    let mut res = 0u32;
    let mut bit = 1u32 << 30;
    while bit > n {
        bit >>= 2;
    }
    while bit != 0 {
        if n >= res + bit {
            n -= res + bit;
            res = (res >> 1) + bit;
        } else {
            res >>= 1;
        }
        bit >>= 2;
    }
    res
}

/// Matriz 3×3 en Q14. Puede incluir escala (hasta ~±8.0 sin desbordar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mat3(pub [[i32; 3]; 3]);

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3([[ONE_Q14, 0, 0], [0, ONE_Q14, 0], [0, 0, ONE_Q14]]);

    pub fn rot_x(a: Angle) -> Mat3 {
        let (s, c) = (sin(a), cos(a));
        Mat3([[ONE_Q14, 0, 0], [0, c, -s], [0, s, c]])
    }

    pub fn rot_y(a: Angle) -> Mat3 {
        let (s, c) = (sin(a), cos(a));
        Mat3([[c, 0, s], [0, ONE_Q14, 0], [-s, 0, c]])
    }

    pub fn rot_z(a: Angle) -> Mat3 {
        let (s, c) = (sin(a), cos(a));
        Mat3([[c, -s, 0], [s, c, 0], [0, 0, ONE_Q14]])
    }

    /// Escala por eje, en Q14.
    pub const fn scale(x: i32, y: i32, z: i32) -> Mat3 {
        Mat3([[x, 0, 0], [0, y, 0], [0, 0, z]])
    }

    pub fn transpose(&self) -> Mat3 {
        let m = &self.0;
        Mat3([
            [m[0][0], m[1][0], m[2][0]],
            [m[0][1], m[1][1], m[2][1]],
            [m[0][2], m[1][2], m[2][2]],
        ])
    }

    pub fn mul(&self, o: &Mat3) -> Mat3 {
        let mut r = [[0i32; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                let acc: i64 = (0..3).map(|k| self.0[i][k] as i64 * o.0[k][j] as i64).sum();
                *cell = (acc >> 14) as i32;
            }
        }
        Mat3(r)
    }

    /// `M · v` con acumulación en 64 bits (mul + mulh en RV32: barato).
    #[inline(always)]
    pub fn apply(&self, v: [i32; 3]) -> [i32; 3] {
        let m = &self.0;
        let row = |r: &[i32; 3]| {
            ((r[0] as i64 * v[0] as i64 + r[1] as i64 * v[1] as i64 + r[2] as i64 * v[2] as i64)
                >> 14) as i32
        };
        [row(&m[0]), row(&m[1]), row(&m[2])]
    }

    /// `M · v` en aritmética de 32 bits. Válido si `|v| < 2^13` por
    /// componente y las entradas de la matriz son < 2^15 (escala < 2): cada
    /// producto cabe en 2^28 y la suma de tres en 2^30.
    #[inline(always)]
    pub fn apply_small(&self, v: [i32; 3]) -> [i32; 3] {
        let m = &self.0;
        let row = |r: &[i32; 3]| (r[0] * v[0] + r[1] * v[1] + r[2] * v[2]) >> 14;
        [row(&m[0]), row(&m[1]), row(&m[2])]
    }
}

/// Transformación afín: rotación/escala (Q14) + traslación (Q8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transform {
    pub m: Mat3,
    pub t: [i32; 3],
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        m: Mat3::IDENTITY,
        t: [0, 0, 0],
    };

    pub const fn new(m: Mat3, t: [i32; 3]) -> Self {
        Self { m, t }
    }

    pub const fn translate(t: [i32; 3]) -> Self {
        Self {
            m: Mat3::IDENTITY,
            t,
        }
    }

    /// `self ∘ child`: primero `child`, luego `self`.
    pub fn then(&self, child: &Transform) -> Transform {
        let t = self.m.apply(child.t);
        Transform {
            m: self.m.mul(&child.m),
            t: [t[0] + self.t[0], t[1] + self.t[1], t[2] + self.t[2]],
        }
    }

    #[inline(always)]
    pub fn apply(&self, p: [i32; 3]) -> [i32; 3] {
        let r = self.m.apply(p);
        [r[0] + self.t[0], r[1] + self.t[1], r[2] + self.t[2]]
    }

    /// Como [`apply`](Self::apply) con las restricciones de [`Mat3::apply_small`].
    #[inline(always)]
    pub fn apply_small(&self, p: [i32; 3]) -> [i32; 3] {
        let r = self.m.apply_small(p);
        [r[0] + self.t[0], r[1] + self.t[1], r[2] + self.t[2]]
    }
}

/// Cuaternión de rotación (w, x, y, z) en Q14. Sirve para interpolar y
/// mezclar rotaciones de animaciones; para transformar se pasa a [`Mat3`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quat(pub [i32; 4]);

impl Quat {
    pub const IDENTITY: Quat = Quat([ONE_Q14, 0, 0, 0]);

    pub const fn from_i16(q: [i16; 4]) -> Quat {
        Quat([q[0] as i32, q[1] as i32, q[2] as i32, q[3] as i32])
    }

    fn dot(&self, o: &Quat) -> i32 {
        (0..4).map(|i| (self.0[i] * o.0[i]) >> 14).sum()
    }

    /// Vuelve a largo 1 (la interpolación lineal lo acorta).
    pub fn normalized(self) -> Quat {
        let len2: u64 = self.0.iter().map(|&c| (c as i64 * c as i64) as u64).sum();
        let len = isqrt(len2).max(1) as i64; // Q14
        Quat(self.0.map(|c| ((c as i64 * ONE_Q14 as i64) / len) as i32))
    }

    /// Interpolación lineal normalizada de `self` a `o`; `t` en Q8 (0..=256).
    /// Toma el camino corto.
    pub fn nlerp(self, o: Quat, t: i32) -> Quat {
        let o = if self.dot(&o) < 0 {
            Quat(o.0.map(|c| -c))
        } else {
            o
        };
        let mut r = [0i32; 4];
        for (i, out) in r.iter_mut().enumerate() {
            *out = self.0[i] + (((o.0[i] - self.0[i]) * t) >> 8);
        }
        Quat(r).normalized()
    }

    pub fn to_mat3(&self) -> Mat3 {
        let [w, x, y, z] = self.0;
        let m = |a: i32, b: i32| (a * b) >> 13; // 2·a·b en Q14
        let (xx, yy, zz) = (m(x, x), m(y, y), m(z, z));
        let (xy, xz, yz) = (m(x, y), m(x, z), m(y, z));
        let (wx, wy, wz) = (m(w, x), m(w, y), m(w, z));
        Mat3([
            [ONE_Q14 - yy - zz, xy - wz, xz + wy],
            [xy + wz, ONE_Q14 - xx - zz, yz - wx],
            [xz - wy, yz + wx, ONE_Q14 - xx - yy],
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_table_is_accurate() {
        for d in (0..360).step_by(7) {
            let exact = (d as f64).to_radians().sin() * 16384.0;
            let got = sin(deg(d)) as f64;
            assert!(
                (exact - got).abs() <= 3.0,
                "sin({d}°) = {got}, esperado {exact}"
            );
        }
        assert_eq!(sin(deg(90)), ONE_Q14);
        assert_eq!(cos(0), ONE_Q14);
    }

    #[test]
    fn rotation_quarter_turn() {
        // Y hacia arriba, Z hacia delante: girar 90° en Y lleva +X a -Z.
        let v = Mat3::rot_y(deg(90)).apply([256, 0, 0]);
        assert!(v[0].abs() <= 1 && (v[2] + 256).abs() <= 1, "{v:?}");
    }

    #[test]
    fn composition_order() {
        let a = Transform::translate([0, 0, 1024]);
        let b = Transform::new(Mat3::scale(2 * ONE_Q14, ONE_Q14, ONE_Q14), [0; 3]);
        // Primero escala, luego traslada.
        assert_eq!(a.then(&b).apply([256, 0, 0]), [512, 0, 1024]);
    }

    #[test]
    fn integer_sqrt() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(99), 9);
        assert_eq!(isqrt(1 << 40), 1 << 20);
        for n in [0u32, 1, 2, 3, 4, 99, 100, 65535, 1 << 20, u32::MAX] {
            assert_eq!(isqrt32(n) as u64, isqrt(n as u64), "isqrt32({n})");
        }
    }

    #[test]
    fn quaternion_matches_rotation_matrix() {
        // 90° alrededor de Y: (cos 45°, 0, sen 45°, 0).
        let h = (ONE_Q14 as f64 * std::f64::consts::FRAC_1_SQRT_2).round() as i32;
        let q = Quat([h, 0, h, 0]);
        let m = q.to_mat3();
        let r = Mat3::rot_y(deg(90));
        for i in 0..3 {
            for j in 0..3 {
                assert!((m.0[i][j] - r.0[i][j]).abs() <= 2, "{:?} vs {:?}", m, r);
            }
        }
        // A mitad de camino entre la identidad y 90°: 45°.
        let half = Quat::IDENTITY.nlerp(q, 128).to_mat3();
        let r45 = Mat3::rot_y(deg(45));
        assert!((half.0[0][0] - r45.0[0][0]).abs() <= 20);
        assert_eq!(Quat::IDENTITY.nlerp(q, 256), q.normalized());
    }
}
