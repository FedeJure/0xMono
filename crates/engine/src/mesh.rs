//! Formato de malla compacto, pensado para vivir en flash (`&'static`).
//!
//! Los triángulos se enrollan en sentido antihorario vistos desde fuera
//! (producto vectorial = normal hacia fuera), con Y arriba y Z hacia delante.

#[derive(Clone, Copy, Debug)]
pub struct Mesh<'a> {
    /// Posiciones en Q8 (256 = 1 unidad), dentro de ±8191 (±32 unidades):
    /// así la transformación cabe en aritmética de 32 bits.
    pub positions: &'a [[i16; 3]],
    /// Normales por vértice, 127 = 1.0.
    pub normals: &'a [[i8; 3]],
    /// Hueso de cada vértice (skinning rígido). Vacío = todos al hueso 0.
    pub bones: &'a [u8],
    /// Coordenadas de textura por vértice, en texels Q4 (16 = 1 texel).
    /// Vacío = sin UV (solo hace falta si algún material lleva textura).
    pub uvs: &'a [[i16; 2]],
    pub tris: &'a [[u16; 3]],
    /// Normal de cada triángulo (127 = 1.0). Si está, la malla se ilumina por
    /// cara (aspecto low-poly facetado) y las normales por vértice no se usan.
    pub tri_normals: &'a [[i8; 3]],
    /// Material de cada triángulo. Vacío = material 0.
    pub tri_materials: &'a [u8],
}

/// Límite de las coordenadas de posición (exclusivo).
pub const MAX_COORD: i16 = 8192;

impl Mesh<'_> {
    /// Comprueba las restricciones del formato (coordenadas y índices).
    pub fn is_valid(&self) -> bool {
        let n = self.positions.len();
        // Sin normales por vértice vale si se ilumina por cara.
        (self.normals.len() == n || (self.normals.is_empty() && !self.tri_normals.is_empty()))
            && (self.bones.is_empty() || self.bones.len() == n)
            && (self.uvs.is_empty() || self.uvs.len() == n)
            && (self.tri_materials.is_empty() || self.tri_materials.len() == self.tris.len())
            && (self.tri_normals.is_empty() || self.tri_normals.len() == self.tris.len())
            && self.positions.iter().flatten().all(|c| c.abs() < MAX_COORD)
            && self.tris.iter().flatten().all(|&i| (i as usize) < n)
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn tri_count(&self) -> usize {
        self.tris.len()
    }
}
