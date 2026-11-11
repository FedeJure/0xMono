//! Mallas procedurales en búferes fijos (sin memoria dinámica). Sirven para
//! pruebas y prototipos hasta tener el conversor de modelos de Blender.

use crate::fixed::{Angle, cos, isqrt, sin};
use crate::mesh::Mesh;

pub struct MeshBuf<const V: usize, const T: usize> {
    pub positions: [[i16; 3]; V],
    pub normals: [[i8; 3]; V],
    pub bones: [u8; V],
    pub uvs: [[i16; 2]; V],
    pub tris: [[u16; 3]; T],
    pub tri_materials: [u8; T],
    nv: usize,
    nt: usize,
}

/// Parámetros de un elipsoide.
#[derive(Clone, Copy, Debug)]
pub struct Ellipsoid {
    /// Centro en Q8.
    pub center: [i32; 3],
    /// Semiejes en Q8.
    pub radii: [i32; 3],
    pub rings: usize,
    pub segments: usize,
    pub material: u8,
    pub bone: u8,
}

impl<const V: usize, const T: usize> Default for MeshBuf<V, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const V: usize, const T: usize> MeshBuf<V, T> {
    pub const fn new() -> Self {
        Self {
            positions: [[0; 3]; V],
            normals: [[0; 3]; V],
            bones: [0; V],
            uvs: [[0; 2]; V],
            tris: [[0; 3]; T],
            tri_materials: [0; T],
            nv: 0,
            nt: 0,
        }
    }

    pub fn mesh(&self) -> Mesh<'_> {
        Mesh {
            positions: &self.positions[..self.nv],
            normals: &self.normals[..self.nv],
            bones: &self.bones[..self.nv],
            uvs: &self.uvs[..self.nv],
            tris: &self.tris[..self.nt],
            tri_materials: &self.tri_materials[..self.nt],
            tri_normals: &[],
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.nv
    }

    pub fn tri_count(&self) -> usize {
        self.nt
    }

    fn push_vertex(&mut self, unit: [i32; 3], e: &Ellipsoid) -> u16 {
        // `unit` es un punto de la esfera unidad en Q14.
        let p = [
            e.center[0] + ((unit[0] * e.radii[0]) >> 14),
            e.center[1] + ((unit[1] * e.radii[1]) >> 14),
            e.center[2] + ((unit[2] * e.radii[2]) >> 14),
        ];
        // Normal del elipsoide: (x/rx², y/ry², z/rz²), normalizada.
        let n = [
            unit[0] as i64 * 65536 / e.radii[0] as i64,
            unit[1] as i64 * 65536 / e.radii[1] as i64,
            unit[2] as i64 * 65536 / e.radii[2] as i64,
        ];
        let len = isqrt((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]) as u64).max(1) as i64;
        let i = self.nv;
        self.positions[i] = [p[0] as i16, p[1] as i16, p[2] as i16];
        self.normals[i] = [
            (n[0] * 127 / len) as i8,
            (n[1] * 127 / len) as i8,
            (n[2] * 127 / len) as i8,
        ];
        self.bones[i] = e.bone;
        self.nv += 1;
        i as u16
    }

    fn push_tri(&mut self, a: u16, b: u16, c: u16, material: u8) {
        self.tris[self.nt] = [a, b, c];
        self.tri_materials[self.nt] = material;
        self.nt += 1;
    }

    /// Añade un elipsoide UV. Devuelve `false` si no cabe.
    pub fn add_ellipsoid(&mut self, e: Ellipsoid) -> bool {
        let (rings, segs) = (e.rings.max(2), e.segments.max(3));
        let verts = 2 + (rings - 1) * segs;
        let tris = 2 * segs * (rings - 1);
        if self.nv + verts > V || self.nt + tris > T {
            return false;
        }

        let top = self.push_vertex([0, 1 << 14, 0], &e);
        let first_ring = self.nv as u16;
        for r in 1..rings {
            let theta = (32768 * r / rings) as Angle; // 0 = polo norte, 32768 = polo sur
            let (st, ct) = (sin(theta), cos(theta));
            for s in 0..segs {
                let phi = (65536 * s / segs) as Angle;
                let unit = [(st * cos(phi)) >> 14, ct, (st * sin(phi)) >> 14];
                self.push_vertex(unit, &e);
            }
        }
        let bottom = self.push_vertex([0, -(1 << 14), 0], &e);

        let at = |r: usize, s: usize| first_ring + (r * segs + s % segs) as u16;
        for s in 0..segs {
            self.push_tri(top, at(0, s + 1), at(0, s), e.material);
        }
        for r in 0..rings - 2 {
            for s in 0..segs {
                let (a, b) = (at(r, s), at(r, s + 1));
                let (c, d) = (at(r + 1, s), at(r + 1, s + 1));
                self.push_tri(a, b, c, e.material);
                self.push_tri(b, d, c, e.material);
            }
        }
        for s in 0..segs {
            self.push_tri(at(rings - 2, s), at(rings - 2, s + 1), bottom, e.material);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_triangles_face_outwards() {
        let mut m = MeshBuf::<200, 400>::new();
        assert!(m.add_ellipsoid(Ellipsoid {
            center: [0, 0, 0],
            radii: [256, 200, 300],
            rings: 8,
            segments: 12,
            material: 0,
            bone: 0,
        }));
        let mesh = m.mesh();
        assert_eq!(mesh.vertex_count(), 2 + 7 * 12);
        assert_eq!(mesh.tri_count(), 2 * 12 * 7);
        for t in mesh.tris {
            let p = |i: u16| mesh.positions[i as usize].map(|v| v as i64);
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let centroid = [a[0] + b[0] + c[0], a[1] + b[1] + c[1], a[2] + b[2] + c[2]];
            let dot: i64 = (0..3).map(|k| n[k] * centroid[k]).sum();
            assert!(dot > 0, "triángulo {t:?} mira hacia dentro");
        }
    }
}
