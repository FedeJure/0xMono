//! El hábitat en pantalla: el personaje y el mundo, redibujando solo lo que
//! cambia.
//!
//! La pantalla guarda la imagen (el ST7789 tiene su propia memoria), así que
//! con la cámara quieta alcanza con dibujar y enviar la zona que cambió: la
//! que ocupa el personaje ahora y en el fotograma anterior, los objetos que
//! aparecieron o desaparecieron y lo que el juego marque con
//! [`Stage::invalidate`] (la pantalla de la laptop, un globo de texto...).
//! Si la cámara se mueve o cambia la luz, se redibuja todo.
//!
//! Además, con la cámara quieta el mundo proyectado no cambia: se conserva
//! en el renderer y solo se envía de nuevo el personaje (ver
//! [`Renderer::begin_frame_keeping`]).
//!
//! Cada fotograma (los cambios de estado del mundo, antes de `begin`):
//!
//! ```ignore
//! stage.begin(r, &view);                      // envía el mundo si hace falta
//! stage.submit_character(r, &model, &pose);  // lo que se mueve
//! stage.submit_prop(r, banana, &in_hand);
//! let area = stage.finish(r);                 // decide la zona a redibujar
//! stage.draw(r, &mut color, &mut depth, |band| send(band));
//! ```
//!
//! El resultado es idéntico píxel a píxel a dibujar todo (el rasterizado no
//! depende de cómo se parte la pantalla), así que no quedan costuras.

use engine3d::fixed::Transform;
use engine3d::material::PixelOrder;
use engine3d::raster::fill16;
use engine3d::{Band, Bounds, Light, Mark, Renderer};

use crate::scene::{CharacterScene, MAX_BONES, Pose};
use crate::world::{Visibility, WorldScene};

/// Filas máximas de pantalla (para el cielo).
pub const MAX_ROWS: usize = 240;

pub struct Stage {
    pub chr: CharacterScene,
    pub world: WorldScene,
    /// Transformación de cada hueso del último personaje enviado (vista ·
    /// modelo · hueso): para pegarle objetos a las manos.
    pub bones: [Transform; MAX_BONES],
    /// Color de fondo de cada fila (el cielo). Si cambia, `invalidate_all`.
    pub sky: [u16; MAX_ROWS],
    view: Option<Transform>,
    /// Zona del personaje en el fotograma anterior y en este.
    actor_before: Bounds,
    actor: Bounds,
    /// Visibilidad de los objetos en la pantalla y en el mundo conservado.
    drawn: Visibility,
    /// El mundo conservado en el renderer, con la luz con la que se envió.
    kept: Option<(Mark, Light)>,
    pending: Bounds,
    full: bool,
    area: Bounds,
}

impl Stage {
    pub fn new(chr: CharacterScene, world: WorldScene) -> Self {
        Self {
            chr,
            world,
            bones: [Transform::IDENTITY; MAX_BONES],
            sky: [0; MAX_ROWS],
            view: None,
            actor_before: Bounds::EMPTY,
            actor: Bounds::EMPTY,
            drawn: Visibility::NONE,
            kept: None,
            pending: Bounds::EMPTY,
            full: true,
            area: Bounds::EMPTY,
        }
    }

    /// Carga los materiales del personaje y del mundo.
    pub fn setup<const V: usize, const T: usize>(&self, r: &mut Renderer<V, T>, order: PixelOrder) {
        assert!(r.camera.height as usize <= MAX_ROWS);
        self.chr.setup(r, order);
        self.world.setup(r, order);
    }

    /// El próximo fotograma se dibuja entero (cambió la luz, un material, el cielo...).
    pub fn invalidate_all(&mut self) {
        self.full = true;
    }

    /// La vista (mundo → cámara) del último `begin`.
    pub fn view(&self) -> Option<Transform> {
        self.view
    }

    /// El próximo fotograma redibuja también `rect`.
    pub fn invalidate(&mut self, rect: Bounds) {
        self.pending = self.pending.union(&rect);
    }

    /// El próximo fotograma redibuja la zona del objeto `object` (p. ej. una
    /// textura que se movió). Usa la vista del último `begin`.
    pub fn invalidate_object<const V: usize, const T: usize>(
        &mut self,
        r: &Renderer<V, T>,
        object: usize,
    ) {
        if let Some(view) = self.view {
            let rect = self.world.screen_rect(object, &view, &r.camera);
            self.invalidate(rect);
        }
    }

    /// Empieza un fotograma con la vista (mundo → cámara) `view`. Envía el
    /// mundo si cambió algo desde el que está conservado.
    pub fn begin<const V: usize, const T: usize>(
        &mut self,
        r: &mut Renderer<V, T>,
        view: &Transform,
    ) {
        if self.view != Some(*view) {
            self.full = true;
        }
        self.view = Some(*view);
        let still = self.world.still();
        let same_world = still == self.drawn;
        match self.kept {
            Some((mark, light)) if !self.full && same_world && light == r.light => {
                r.begin_frame_keeping(mark);
            }
            _ => {
                // Objetos que aparecieron o desaparecieron (con la vista de
                // antes, que es la misma si no se redibuja todo).
                let changed = still.changed(&self.drawn);
                for i in 0..self.world.world.objects.len() {
                    if changed.get(i) {
                        self.pending = self
                            .pending
                            .union(&self.world.screen_rect(i, view, &r.camera));
                    }
                }
                if self.kept.is_some_and(|(_, l)| l != r.light) {
                    self.full = true;
                }
                self.drawn = still;
                r.begin_frame();
                self.world.submit(r, view);
                self.kept = Some((r.keep_so_far(), r.light));
            }
        }
        // Lo corrido de su lugar (una silla) va con lo que se mueve.
        self.world.submit_moved(r, view);
    }

    /// Envía el personaje: `model` es vista · posición en el mundo.
    pub fn submit_character<const V: usize, const T: usize>(
        &mut self,
        r: &mut Renderer<V, T>,
        model: &Transform,
        pose: &Pose,
    ) -> bool {
        let c = self.chr.character;
        self.chr.bone_transforms(model, pose, &mut self.bones);
        r.submit(&c.mesh, &self.bones[..c.bones.len().max(1)])
    }

    /// Envía un objeto del mundo que se mueve con el personaje (`transform`
    /// ya en espacio de cámara).
    pub fn submit_prop<const V: usize, const T: usize>(
        &mut self,
        r: &mut Renderer<V, T>,
        object: usize,
        transform: &Transform,
    ) -> bool {
        self.world.submit_object(r, object, transform)
    }

    /// Decide qué zona redibujar: lo enviado después de `begin` (personaje
    /// y objetos en la mano) es "lo que se mueve". Devuelve la zona (vacía
    /// si no cambió nada).
    pub fn finish<const V: usize, const T: usize>(&mut self, r: &mut Renderer<V, T>) -> Bounds {
        self.actor_before = self.actor;
        self.actor = r.take_bounds();
        let area = if self.full {
            r.screen()
        } else {
            self.actor
                .union(&self.actor_before)
                .union(&self.pending)
                .intersect(&r.screen())
        };
        self.full = false;
        self.pending = Bounds::EMPTY;
        self.area = area;
        area
    }

    /// Zona que se redibuja en este fotograma.
    pub fn area(&self) -> Bounds {
        self.area
    }

    /// Dibuja la zona en franjas usando `color` y `depth` como búferes (del
    /// mismo tamaño; cuanto más grandes, menos franjas) y entrega cada franja
    /// a `flush` para enviarla a la pantalla (o dibujarle algo encima antes).
    pub fn draw<const V: usize, const T: usize>(
        &self,
        r: &mut Renderer<V, T>,
        color: &mut [u16],
        depth: &mut [u16],
        mut flush: impl FnMut(&mut Band),
    ) {
        let area = self.area;
        if area.is_empty() {
            return;
        }
        let w = area.width() as usize;
        let rows = (color.len().min(depth.len()) / w).max(1);
        let mut y0 = area.y0 as usize;
        while y0 < area.y1 as usize {
            let h = rows.min(area.y1 as usize - y0);
            let mut band = Band {
                color: &mut color[..w * h],
                depth: &mut depth[..w * h],
                x0: area.x0 as usize,
                width: w,
                y0,
                height: h,
            };
            for row in 0..h {
                fill16(&mut band.color[row * w..(row + 1) * w], self.sky[y0 + row]);
            }
            if r.touches_rows(y0, y0 + h) {
                r.clear_depth(&mut band);
                r.render_band(&mut band);
            }
            flush(&mut band);
            y0 += h;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::house::HOUSE;
    use crate::mono::MONO;
    use crate::scene;
    use crate::world::{Eye, yaw_matrix};
    use engine3d::Camera;

    const W: usize = 240;
    type R = Renderer<4096, 4096>;

    fn stage() -> (Box<R>, Stage) {
        let mut r = Box::new(R::new(Camera::new(240, 240, 300, 96), scene::light()));
        let chr = CharacterScene::new(&MONO);
        let base = (MONO.colors.len() + MONO.parts.len()) as u8;
        let mut world = WorldScene::new(&HOUSE, base);
        world.set_state("awake", true);
        let mut st = Stage::new(chr, world);
        st.setup(&mut r, PixelOrder::Native);
        for (y, s) in st.sky.iter_mut().enumerate() {
            *s = 0x1000 + y as u16;
        }
        (r, st)
    }

    /// Un fotograma con el mono en `x` (Q8, en el pasillo de la cocina)
    /// dibujado sobre `screen` (la memoria de la pantalla).
    fn frame(r: &mut R, st: &mut Stage, x: i32, screen: &mut [u16]) -> Bounds {
        let view = Eye::of(HOUSE.camera("cam_kitchen").unwrap()).view();
        let at = HOUSE.spot("wp_kitchen").unwrap().pos;
        st.begin(r, &view);
        let model = view
            .then(&Transform::translate([x, 0, at[2]]))
            .then(&Transform::new(yaw_matrix(30), [0; 3]));
        st.submit_character(r, &model, &Pose::REST);
        let area = st.finish(r);
        let (mut color, mut depth) = ([0u16; W * 20], [0u16; W * 20]);
        st.draw(r, &mut color, &mut depth, |b| {
            for row in 0..b.height {
                let dst = (b.y0 + row) * W + b.x0;
                screen[dst..dst + b.width].copy_from_slice(&b.color[row * b.width..][..b.width]);
            }
        });
        area
    }

    #[test]
    fn partial_redraw_matches_a_full_frame() {
        let at = HOUSE.spot("wp_kitchen").unwrap().pos;
        let (mut r, mut st) = stage();
        let mut screen = vec![0u16; W * W];
        let first = frame(&mut r, &mut st, at[0], &mut screen);
        assert_eq!(first, r.screen(), "el primer fotograma es completo");
        // El mono se corre: solo se redibuja su zona (la de antes y la de ahora).
        let second = frame(&mut r, &mut st, at[0] + 200, &mut screen);
        assert!(second.area() < first.area() / 2, "{second:?}");
        assert!(!second.is_empty());

        // Igual a dibujarlo todo de nuevo.
        let (mut r2, mut st2) = stage();
        let mut fresh = vec![0u16; W * W];
        frame(&mut r2, &mut st2, at[0] + 200, &mut fresh);
        assert!(screen == fresh, "la zona redibujada deja costuras");

        // Quieto: solo su zona (siempre se redibuja: puede estar animándose).
        let still = frame(&mut r, &mut st, at[0] + 200, &mut screen);
        assert!(still.area() < second.area(), "{still:?}");
        // Un objeto que desaparece se borra.
        let fridge = HOUSE.object("fridge").unwrap();
        st.world.set_visible(fridge, false);
        let hidden = frame(&mut r, &mut st, at[0] + 200, &mut screen);
        assert!(!hidden.is_empty());
        let mut fresh = vec![0u16; W * W];
        let (mut r2, mut st2) = stage();
        st2.world.set_visible(fridge, false);
        frame(&mut r2, &mut st2, at[0] + 200, &mut fresh);
        assert!(screen == fresh, "la heladera no se borró bien");

        // Una silla que se corre: se borra de su lugar y aparece en el nuevo.
        st.world.set_offset("dining_chair", [0, 0, 300]);
        let moved = frame(&mut r, &mut st, at[0] + 200, &mut screen);
        assert!(moved.area() < r.screen().area());
        let mut fresh = vec![0u16; W * W];
        let (mut r2, mut st2) = stage();
        st2.world.set_visible(fridge, false);
        st2.world.set_offset("dining_chair", [0, 0, 300]);
        frame(&mut r2, &mut st2, at[0] + 200, &mut fresh);
        assert!(screen == fresh, "la silla corrida deja rastros");
        // Y vuelve.
        st.world.set_offset("dining_chair", [0; 3]);
        frame(&mut r, &mut st, at[0] + 200, &mut screen);
        let mut fresh = vec![0u16; W * W];
        let (mut r2, mut st2) = stage();
        st2.world.set_visible(fridge, false);
        frame(&mut r2, &mut st2, at[0] + 200, &mut fresh);
        assert!(screen == fresh, "la silla no volvió bien");
    }
}
