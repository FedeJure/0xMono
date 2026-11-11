//! La cámara: quieta en la habitación del personaje (así solo se redibuja
//! lo que cambia) y un paneo corto cuando pasa a otra.

use assets::house::HOUSE;
use assets::world::Eye;

use crate::actor::M;

/// Cámaras de cada habitación, de izquierda a derecha, y la general.
pub const ZONES: [&str; 6] = [
    "cam_bedroom",
    "cam_office",
    "cam_kitchen",
    "cam_reading",
    "cam_garden",
    "cam_house",
];
pub const OVERVIEW: usize = 5;
/// La cámara del jardín (afuera).
pub const OUTDOOR: usize = 4;
/// Duración del paneo entre habitaciones, en segundos.
const PAN_S: f32 = 0.9;

/// Dónde "vale" cada cámara de habitación: adentro, su propia x; la del
/// jardín, desde la puerta.
fn anchor(i: usize) -> f32 {
    if i == 4 {
        return HOUSE.spot("wp_door_out").map_or(0, |s| s.pos[0]) as f32;
    }
    HOUSE.camera(ZONES[i]).map_or(0, |c| c.pos[0]) as f32
}

/// Habitación de la posición `x` (Q8): la del ancla más cercana. Con
/// `current`, solo cambia si se pasó del límite por 15 cm (para no ir y
/// venir en el borde).
pub fn zone_of(x: f32, current: Option<usize>) -> usize {
    let mut nearest = 0;
    for i in 1..OVERVIEW {
        if (x - anchor(i)).abs() < (x - anchor(nearest)).abs() {
            nearest = i;
        }
    }
    match current {
        Some(c) if c < OVERVIEW && c != nearest => {
            let limit = (anchor(c) + anchor(nearest)) / 2.0;
            if (x - limit).abs() > 0.15 * M {
                nearest
            } else {
                c
            }
        }
        _ => nearest,
    }
}

pub fn zone_eye(zone: usize) -> Eye {
    Eye::of(HOUSE.camera(ZONES[zone]).expect("cámara de zona"))
}

pub struct CameraRig {
    pub eye: Eye,
    pub zone: usize,
    pan: Option<(Eye, f32)>,
    pub overview: bool,
    /// Cámara fija elegida a mano (para revisar de cerca), en vez de la de la habitación.
    pub fixed: Option<Eye>,
}

impl CameraRig {
    pub fn new(x: f32) -> Self {
        let zone = zone_of(x, None);
        Self {
            eye: zone_eye(zone),
            zone,
            pan: None,
            overview: false,
            fixed: None,
        }
    }

    /// Directo a la habitación de `x`, sin paneo.
    pub fn snap(&mut self, x: f32) {
        self.zone = if self.overview {
            OVERVIEW
        } else {
            zone_of(x, None)
        };
        self.pan = None;
        self.eye = zone_eye(self.zone);
    }

    pub fn update(&mut self, x: f32, dt: f32) {
        if let Some(eye) = self.fixed {
            self.eye = eye;
            return;
        }
        let zone = if self.overview {
            OVERVIEW
        } else {
            zone_of(x, Some(self.zone))
        };
        if zone != self.zone {
            self.zone = zone;
            self.pan = Some((self.eye, 0.0));
        }
        let target = zone_eye(self.zone);
        self.eye = match &mut self.pan {
            Some((from, t)) => {
                *t += dt;
                let k = (*t / PAN_S).clamp(0.0, 1.0);
                let k = k * k * (3.0 - 2.0 * k);
                let eye = from.lerp(&target, (k * 256.0) as i32);
                if *t >= PAN_S {
                    self.pan = None;
                    target
                } else {
                    eye
                }
            }
            None => target,
        };
    }
}
