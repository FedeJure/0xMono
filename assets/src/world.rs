//! Escenarios exportados de Blender (el hábitat): objetos estáticos y
//! móviles, lugares del personaje, puntos de paso y cámaras.
//!
//! [`WorldScene`] los pone en el renderer: materiales (a continuación de los
//! del personaje), objetos visibles según su estado, descarte de los que
//! quedan fuera de cámara y texturas animadas (el código que corre en la
//! pantalla de la laptop).

use engine3d::fixed::{Mat3, Quat, Transform};
use engine3d::material::{Material, PixelOrder, Rgb, TexMap};
use engine3d::{Bounds, Camera, Renderer};

use crate::TexturedPart;
use crate::nav::NavMap;
use crate::scene::toon;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Siempre está.
    Static,
    /// Lo mueve el juego (algo en la mano del personaje).
    Prop,
    /// Se muestra según un estado del juego (`"asleep"`, `"on_tree"`...).
    State(&'static str),
}

pub struct WorldObject {
    pub name: &'static str,
    pub kind: Kind,
    pub mesh: engine3d::Mesh<'static>,
    /// Esfera que lo envuelve (Q8), para descartarlo si queda fuera de cámara.
    pub center: [i32; 3],
    pub radius: i32,
}

/// Un lugar del personaje: dónde van sus pies y hacia dónde mira.
pub struct Spot {
    pub name: &'static str,
    /// Q8, ejes del motor.
    pub pos: [i32; 3],
    /// Grados alrededor de Y; 0 = mirando a la cámara.
    pub yaw: i16,
}

pub struct CameraView {
    pub name: &'static str,
    pub pos: [i32; 3],
    /// Orientación de la cámara en el mundo (cuaternión Q14): sus columnas
    /// son derecha, arriba y adelante.
    pub rot: [i16; 4],
    /// Distancia focal en píxeles para 240 de ancho.
    pub focal: i16,
}

/// Un asiento con mesa adelante tiene una silla que se corre hacia atrás
/// para sentarse (y después se acerca con el personaje encima).
pub struct Seat {
    /// El lugar (`spot_*`).
    pub spot: &'static str,
    /// Objeto de la silla, si hay que correrla.
    pub chair: Option<&'static str>,
    /// Cuánto se corre hacia atrás (Q8).
    pub pull: i32,
    /// Punto libre desde donde se acerca (`[x, z]`, Q8).
    pub approach: [i32; 2],
}

pub struct World {
    pub objects: &'static [WorldObject],
    pub colors: &'static [Rgb],
    pub parts: &'static [TexturedPart],
    pub spots: &'static [Spot],
    pub waypoints: &'static [Spot],
    pub cameras: &'static [CameraView],
    pub nav: &'static NavMap,
    pub seats: &'static [Seat],
}

impl World {
    pub fn object(&self, name: &str) -> Option<usize> {
        self.objects.iter().position(|o| o.name == name)
    }

    pub fn spot(&self, name: &str) -> Option<&'static Spot> {
        self.spots
            .iter()
            .chain(self.waypoints.iter())
            .find(|s| s.name == name)
    }

    pub fn camera(&self, name: &str) -> Option<&'static CameraView> {
        self.cameras.iter().find(|c| c.name == name)
    }

    pub fn seat(&self, spot: &str) -> Option<&'static Seat> {
        self.seats.iter().find(|s| s.spot == spot)
    }
}

/// Una cámara del mundo lista para dibujar: posición y orientación.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Eye {
    pub pos: [i32; 3],
    pub rot: Quat,
}

impl Eye {
    pub fn of(c: &CameraView) -> Eye {
        Eye {
            pos: c.pos,
            rot: Quat::from_i16(c.rot).normalized(),
        }
    }

    /// Mezcla dos cámaras; `t` en Q8 (0..=256).
    pub fn lerp(&self, o: &Eye, t: i32) -> Eye {
        Eye {
            pos: core::array::from_fn(|k| {
                self.pos[k] + (((o.pos[k] - self.pos[k]) as i64 * t as i64) >> 8) as i32
            }),
            rot: self.rot.nlerp(o.rot, t),
        }
    }

    /// Transformación mundo → cámara.
    pub fn view(&self) -> Transform {
        let rt = self.rot.to_mat3().transpose();
        let t = rt.apply(self.pos);
        Transform::new(rt, [-t[0], -t[1], -t[2]])
    }
}

/// Objetos máximos de un mundo (los grandes vienen partidos en trozos).
pub const MAX_OBJECTS: usize = 256;

/// Qué objetos del mundo se ven: un bit por objeto.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Visibility([u64; MAX_OBJECTS / 64]);

impl Visibility {
    pub const NONE: Visibility = Visibility([0; MAX_OBJECTS / 64]);

    pub fn get(&self, i: usize) -> bool {
        self.0[i / 64] & (1 << (i % 64)) != 0
    }

    pub fn set(&mut self, i: usize, on: bool) {
        if on {
            self.0[i / 64] |= 1 << (i % 64);
        } else {
            self.0[i / 64] &= !(1 << (i % 64));
        }
    }

    /// Los objetos que están en uno y no en el otro.
    pub fn changed(&self, o: &Visibility) -> Visibility {
        Visibility(core::array::from_fn(|k| self.0[k] ^ o.0[k]))
    }
}

/// Objetos fijos que pueden estar corridos a la vez (sillas).
pub const MAX_MOVED: usize = 4;

pub struct WorldScene {
    pub world: &'static World,
    /// Primer material del mundo en la tabla del renderer.
    pub material_base: u8,
    pub visible: Visibility,
    /// Objetos fijos corridos de su lugar (por nombre, con todos sus trozos)
    /// y cuánto (Q8). Mientras están corridos se dibujan como lo que se
    /// mueve, no con el escenario.
    moved: [Option<(&'static str, [i32; 3])>; MAX_MOVED],
}

impl WorldScene {
    /// `material_base`: dónde empiezan sus materiales (después de los del personaje).
    pub fn new(world: &'static World, material_base: u8) -> Self {
        assert!(
            world.objects.len() <= MAX_OBJECTS,
            "demasiados objetos en el mundo"
        );
        let mut visible = Visibility::NONE;
        for (i, o) in world.objects.iter().enumerate() {
            visible.set(i, o.kind == Kind::Static);
        }
        Self {
            world,
            material_base,
            visible,
            moved: [None; MAX_MOVED],
        }
    }

    /// Corre el objeto `name` (con todos sus trozos) `offset` (Q8) de su
    /// lugar; `[0; 3]` lo devuelve.
    pub fn set_offset(&mut self, name: &'static str, offset: [i32; 3]) {
        let slot = self
            .moved
            .iter()
            .position(|m| m.is_some_and(|(n, _)| n == name));
        match (slot, offset == [0; 3]) {
            (Some(k), true) => self.moved[k] = None,
            (Some(k), false) => self.moved[k] = Some((name, offset)),
            (None, false) => {
                if let Some(k) = self.moved.iter().position(Option::is_none) {
                    self.moved[k] = Some((name, offset));
                }
            }
            (None, true) => {}
        }
    }

    fn is_moved(&self, object: usize) -> bool {
        let name = self.world.objects[object].name;
        self.moved.iter().any(|m| m.is_some_and(|(n, _)| n == name))
    }

    /// Lo que va con el escenario quieto: lo visible menos lo corrido.
    pub fn still(&self) -> Visibility {
        let mut v = self.visible;
        for i in 0..self.world.objects.len() {
            if v.get(i) && self.is_moved(i) {
                v.set(i, false);
            }
        }
        v
    }

    /// Envía los objetos corridos en su posición actual.
    pub fn submit_moved<const V: usize, const T: usize>(
        &self,
        r: &mut Renderer<V, T>,
        view: &Transform,
    ) {
        for &(name, offset) in self.moved.iter().flatten() {
            let tr = view.then(&Transform::translate(offset));
            for (i, o) in self.world.objects.iter().enumerate() {
                if o.name == name && self.visible.get(i) {
                    self.submit_object(r, i, &tr);
                }
            }
        }
    }

    pub fn setup<const V: usize, const T: usize>(&self, r: &mut Renderer<V, T>, order: PixelOrder) {
        let w = self.world;
        let base = self.material_base as usize;
        assert!(
            base + w.colors.len() + w.parts.len() <= r.materials.len(),
            "demasiados materiales"
        );
        for (i, &rgb) in w.colors.iter().enumerate() {
            r.materials[base + i] = toon(rgb, order);
        }
        for (j, p) in w.parts.iter().enumerate() {
            r.materials[base + w.colors.len() + j] = Material::textured(TexMap {
                texture: p.texture,
                offset: [0, 0],
                colors: p.colors.map(|c| c + self.material_base),
            });
        }
    }

    /// Muestra u oculta los objetos con ese estado.
    pub fn set_state(&mut self, state: &str, on: bool) {
        for (i, o) in self.world.objects.iter().enumerate() {
            if matches!(o.kind, Kind::State(k) if k == state) {
                self.visible.set(i, on);
            }
        }
    }

    /// Muestra u oculta un objeto (con todos sus trozos si el exportador lo partió).
    pub fn set_visible(&mut self, object: usize, on: bool) {
        let name = self.world.objects[object].name;
        for (i, o) in self.world.objects.iter().enumerate() {
            if o.name == name {
                self.visible.set(i, on);
            }
        }
    }

    /// Desplaza la textura de la parte `name` (texels), p. ej. código que corre.
    pub fn scroll<const V: usize, const T: usize>(
        &self,
        r: &mut Renderer<V, T>,
        name: &str,
        offset: [i16; 2],
    ) {
        let w = self.world;
        if let Some(j) = w.parts.iter().position(|p| p.name == name)
            && let Some(map) = r.materials[self.material_base as usize + w.colors.len() + j]
                .texture
                .as_mut()
        {
            map.offset = offset;
        }
    }

    /// Zona de pantalla que puede ocupar el objeto (su esfera) con esta vista.
    pub fn screen_rect(&self, object: usize, view: &Transform, cam: &Camera) -> Bounds {
        let o = &self.world.objects[object];
        sphere_rect(view.apply(o.center), o.radius, cam)
    }

    /// Envía los objetos visibles que tocan la zona de recorte del renderer
    /// (toda la pantalla, salvo que se esté redibujando una parte). Van de
    /// adelante hacia atrás: lo tapado falla la prueba de profundidad y no
    /// se pinta. Devuelve cuántos objetos envió.
    pub fn submit<const V: usize, const T: usize>(
        &self,
        r: &mut Renderer<V, T>,
        view: &Transform,
    ) -> usize {
        let (cam, clip) = (r.camera, r.scissor());
        // (distancia, objeto) de los que entran, ordenados por inserción.
        let mut order = [(0i32, 0u8); MAX_OBJECTS];
        let mut n = 0;
        let still = self.still();
        for (i, o) in self.world.objects.iter().enumerate() {
            if !still.get(i) {
                continue;
            }
            let c = view.apply(o.center);
            if !sphere_rect(c, o.radius, &cam).overlaps(&clip) {
                continue;
            }
            // Por el fondo de la esfera: los objetos grandes (paredes, piso)
            // quedan al final aunque su centro esté cerca.
            let key = (c[2] + o.radius, i as u8);
            let mut k = n;
            while k > 0 && order[k - 1].0 > key.0 {
                order[k] = order[k - 1];
                k -= 1;
            }
            order[k] = key;
            n += 1;
        }
        for &(_, i) in &order[..n] {
            let o = &self.world.objects[i as usize];
            r.submit_with_base(&o.mesh, core::slice::from_ref(view), self.material_base);
        }
        n
    }

    /// Envía un objeto (un "prop") con una transformación propia, por
    /// ejemplo pegado a la mano del personaje.
    pub fn submit_object<const V: usize, const T: usize>(
        &self,
        r: &mut Renderer<V, T>,
        object: usize,
        transform: &Transform,
    ) -> bool {
        r.submit_with_base(
            &self.world.objects[object].mesh,
            core::slice::from_ref(transform),
            self.material_base,
        )
    }
}

/// Rectángulo de pantalla que contiene a una esfera (centro en espacio de
/// cámara, Q8). Conservador: proyecta la caja que envuelve a la esfera.
pub fn sphere_rect(c: [i32; 3], radius: i32, cam: &Camera) -> Bounds {
    let screen = Bounds::new(0, 0, cam.width, cam.height);
    let (near, far) = (c[2] - radius, c[2] + radius);
    if far < cam.near {
        return Bounds::EMPTY;
    }
    if near <= cam.near {
        // Cruza el plano cercano: puede caer en cualquier lado.
        return screen;
    }
    // Extremos de v/z sobre la caja: el menor usa la z que lo hace más
    // negativo y el mayor la que lo hace más positivo. En 32 bits (las
    // divisiones de 64 son por software en el C3): focal < 2^10 y las
    // coordenadas de cámara < 2^16 dentro de la escena.
    let f = cam.focal;
    let span = |v: i32| {
        let (lo, hi) = (v - radius, v + radius);
        let lo = f * lo / if lo < 0 { near } else { far };
        let hi = f * hi / if hi > 0 { near } else { far };
        (lo, hi)
    };
    let (x0, x1) = span(c[0]);
    let (y0, y1) = span(c[1]);
    // Pantalla: Y hacia abajo. Un píxel de margen por el redondeo.
    Bounds::new(
        cam.cx + x0 - 1,
        cam.cy - y1 - 1,
        cam.cx + x1 + 2,
        cam.cy - y0 + 2,
    )
    .intersect(&screen)
}

/// Matriz de giro alrededor de Y para un ángulo en grados (yaw del personaje).
pub fn yaw_matrix(deg: i32) -> Mat3 {
    Mat3::rot_y(engine3d::fixed::deg(deg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::house::HOUSE;

    #[test]
    fn house_is_consistent() {
        let w = &HOUSE;
        assert!(w.objects.len() <= MAX_OBJECTS);
        for o in w.objects {
            assert!(o.mesh.is_valid(), "{}", o.name);
            let n = (w.colors.len() + w.parts.len()) as u8;
            assert!(o.mesh.tri_materials.iter().all(|&m| m < n), "{}", o.name);
        }
        for name in [
            "spot_bed",
            "spot_desk",
            "spot_table",
            "spot_tree",
            "wp_door_in",
        ] {
            assert!(w.spot(name).is_some(), "falta {name}");
        }
        assert!(w.camera("cam_office").is_some());
    }

    #[test]
    fn sphere_rect_contains_the_projected_object() {
        // Cada objeto, visto desde cada cámara: todos sus vértices caen
        // dentro de su rectángulo.
        let cam = Camera::new(240, 240, 300, 96);
        let scene = WorldScene::new(&HOUSE, 0);
        for c in HOUSE.cameras {
            let view = Eye::of(c).view();
            for (i, o) in HOUSE.objects.iter().enumerate() {
                let rect = scene.screen_rect(i, &view, &cam);
                for p in o.mesh.positions {
                    let v = view.apply([p[0] as i32, p[1] as i32, p[2] as i32]);
                    if v[2] < cam.near {
                        continue;
                    }
                    let x = cam.cx + (v[0] as i64 * cam.focal as i64 / v[2] as i64) as i32;
                    let y = cam.cy - (v[1] as i64 * cam.focal as i64 / v[2] as i64) as i32;
                    if (0..240).contains(&x) && (0..240).contains(&y) {
                        assert!(
                            (rect.x0..rect.x1).contains(&x) && (rect.y0..rect.y1).contains(&y),
                            "{} desde {}: ({x}, {y}) fuera de {rect:?}",
                            o.name,
                            c.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn camera_looks_at_the_house() {
        // La cámara de la oficina tiene la oficina adelante (z de cámara > 0)
        // y más o menos centrada.
        let cam = Eye::of(HOUSE.camera("cam_office").unwrap());
        let desk = HOUSE.spot("spot_desk").unwrap();
        let p = cam.view().apply(desk.pos);
        assert!(p[2] > 1500, "el escritorio queda detrás o muy cerca: {p:?}");
        assert!(
            p[0].abs() < p[2] / 4,
            "el escritorio no está centrado: {p:?}"
        );
    }
}
