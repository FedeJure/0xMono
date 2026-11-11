//! Efectos: bocanadas de humo, el pedo, la espuma del matafuego, chispazos,
//! gotas de sudor y las llamas de la olla.
//!
//! Partículas sencillas (posición, velocidad, edad) dibujadas con mallas de
//! la escena (`fx_*` en `tools/blender/build_house.py`). El motor no escala
//! mallas, así que lo que crece (el humo) cambia de malla con la edad. Van
//! con lo que se mueve: el redibujado parcial las incluye solas.

use assets::house::HOUSE;
use assets::stage::Stage;
use engine3d::{Mat3, Renderer, Transform};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Smoke,
    Fart,
    Foam,
    Spark,
    Drop,
}

#[derive(Clone, Copy, Debug)]
struct Particle {
    kind: Kind,
    /// Mundo, Q8.
    pos: [f32; 3],
    /// Q8 por segundo.
    vel: [f32; 3],
    age: f32,
    life: f32,
}

const MAX: usize = 24;

fn obj(name: &str) -> usize {
    HOUSE
        .object(name)
        .unwrap_or_else(|| panic!("falta el efecto {name}"))
}

pub struct Fx {
    parts: [Option<Particle>; MAX],
    /// Llamas en la olla: dónde (mundo, Q8), si las hay.
    pub fire: Option<[f32; 3]>,
    t: f32,
    smoke: [usize; 3],
    fart: [usize; 3],
    foam: [usize; 2],
    spark: [usize; 2],
    flame: [usize; 2],
    drop: usize,
}

impl Default for Fx {
    fn default() -> Self {
        Self::new()
    }
}

impl Fx {
    pub fn new() -> Self {
        Self {
            parts: [None; MAX],
            fire: None,
            t: 0.0,
            smoke: [obj("fx_smoke_0"), obj("fx_smoke_1"), obj("fx_smoke_2")],
            fart: [obj("fx_fart_0"), obj("fx_fart_1"), obj("fx_fart_2")],
            foam: [obj("fx_foam_0"), obj("fx_foam_1")],
            spark: [obj("fx_spark_0"), obj("fx_spark_1")],
            flame: [obj("fx_flame_0"), obj("fx_flame_1")],
            drop: obj("fx_drop"),
        }
    }

    /// Una partícula nueva (si no hay lugar, reemplaza a la más vieja).
    pub fn spawn(&mut self, kind: Kind, pos: [f32; 3], vel: [f32; 3], life: f32) {
        let p = Particle {
            kind,
            pos,
            vel,
            age: 0.0,
            life,
        };
        let slot = self
            .parts
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                let mut oldest = 0;
                for (i, q) in self.parts.iter().enumerate() {
                    if let (Some(q), Some(o)) = (q, &self.parts[oldest])
                        && q.age / q.life > o.age / o.life
                    {
                        oldest = i;
                    }
                }
                oldest
            });
        self.parts[slot] = Some(p);
    }

    pub fn clear(&mut self) {
        self.parts = [None; MAX];
        self.fire = None;
    }

    pub fn is_empty(&self) -> bool {
        self.fire.is_none() && self.parts.iter().all(Option::is_none)
    }

    pub fn update(&mut self, dt: f32) {
        self.t += dt;
        for slot in self.parts.iter_mut() {
            let Some(p) = slot else { continue };
            p.age += dt;
            if p.age >= p.life {
                *slot = None;
                continue;
            }
            match p.kind {
                // El humo sube y se frena; el pedo, más lento y pesado.
                Kind::Smoke | Kind::Fart => {
                    let drag = 1.0 - (1.2 * dt).min(1.0);
                    p.vel[0] *= drag;
                    p.vel[2] *= drag;
                }
                Kind::Foam => {
                    let drag = 1.0 - (2.5 * dt).min(1.0);
                    p.vel = p.vel.map(|v| v * drag);
                    p.vel[1] -= 0.6 * 1024.0 * dt;
                }
                Kind::Drop => p.vel[1] -= 2.2 * 1024.0 * dt,
                Kind::Spark => {}
            }
            for k in 0..3 {
                p.pos[k] += p.vel[k] * dt;
            }
        }
    }

    /// Envía las partículas y las llamas (después del personaje).
    pub fn submit<const V: usize, const T: usize>(
        &self,
        stage: &mut Stage,
        r: &mut Renderer<V, T>,
        view: &Transform,
    ) {
        let at = |p: [f32; 3]| Transform::translate(p.map(|c| c as i32));
        for p in self.parts.iter().flatten() {
            let k = p.age / p.life;
            let mesh = match p.kind {
                Kind::Smoke => self.smoke[grow(k)],
                Kind::Fart => self.fart[grow(k)],
                Kind::Foam => self.foam[(k > 0.3) as usize],
                Kind::Spark => self.spark[(k > 0.35) as usize],
                Kind::Drop => self.drop,
            };
            let mut tr = view.then(&at(p.pos));
            if p.kind == Kind::Spark {
                // Cada chispazo, girado distinto.
                let spin = (p.pos[0] as i32 * 7 + p.pos[2] as i32 * 3) % 360;
                tr = tr.then(&Transform::new(
                    Mat3::rot_y(engine3d::fixed::deg(spin)),
                    [0; 3],
                ));
            }
            stage.submit_prop(r, mesh, &tr);
        }
        if let Some(pos) = self.fire {
            // Dos formas que se alternan: titila.
            let flicker = ((self.t * 9.0) as u32 % 2) as usize;
            stage.submit_prop(r, self.flame[flicker], &view.then(&at(pos)));
        }
    }
}

/// Malla por edad para lo que crece: chica, mediana, grande.
fn grow(k: f32) -> usize {
    if k < 0.25 {
        0
    } else if k < 0.6 {
        1
    } else {
        2
    }
}
