//! Escenas fijas del hábitat para medir: cada una es una cámara con el mono
//! haciendo algo en su lugar (o caminando con la cámara siguiéndolo). Las
//! usan el benchmark del ESP32 (`firmware/src/bin/bench_habitat.rs`) y el
//! simulador (`--example habitat_bench`), así ambos dibujan exactamente lo
//! mismo.

use engine3d::fixed::Transform;
use engine3d::{Bounds, Renderer};

use crate::anim::{AnimPose, Animator};
use crate::house::HOUSE;
use crate::mono_anims::{CLIPS, clip};
use crate::scene::Pose;
use crate::stage::Stage;
use crate::world::{Eye, yaw_matrix};

pub struct Shot {
    pub name: &'static str,
    pub camera: &'static str,
    /// Lugar del personaje; con `walk_to`, de dónde sale.
    pub spot: &'static str,
    pub clip: usize,
    /// Camina hasta este lugar con la cámara siguiéndolo (fotogramas completos).
    pub walk_to: Option<(&'static str, &'static str)>,
    /// Estados del mundo que se muestran (`"asleep"`, `"on_tree"`...).
    pub states: &'static [&'static str],
    /// La pantalla de la laptop muestra código que corre.
    pub laptop: bool,
}

pub const SHOTS: &[Shot] = &[
    Shot {
        name: "programando",
        camera: "cam_office",
        spot: "spot_desk",
        clip: clip::SITTING_TYPING_COMPUTER,
        walk_to: None,
        states: &["awake", "on_tree", "garden"],
        laptop: true,
    },
    Shot {
        name: "comiendo",
        camera: "cam_kitchen",
        spot: "spot_table",
        clip: clip::EAT_BANANA,
        walk_to: None,
        states: &["awake", "on_tree", "garden"],
        laptop: false,
    },
    Shot {
        name: "durmiendo",
        camera: "cam_bedroom",
        spot: "spot_bed",
        clip: clip::SLEEPING,
        walk_to: None,
        states: &["asleep", "on_tree", "garden"],
        laptop: false,
    },
    Shot {
        name: "leyendo",
        camera: "cam_reading",
        spot: "spot_armchair",
        clip: clip::READ_BOOK,
        walk_to: None,
        states: &["awake", "on_tree", "garden"],
        laptop: false,
    },
    Shot {
        name: "ejercicio",
        camera: "cam_garden",
        spot: "spot_mat",
        clip: clip::JUMPING_JACK,
        walk_to: None,
        states: &["awake", "on_tree", "garden"],
        laptop: false,
    },
    Shot {
        name: "bananero",
        camera: "cam_garden",
        spot: "spot_tree",
        clip: clip::PICK_BANANA,
        walk_to: None,
        states: &["awake", "on_tree", "garden"],
        laptop: false,
    },
    Shot {
        name: "caminando",
        camera: "cam_office",
        spot: "wp_office",
        clip: clip::HAPPY_WALK,
        walk_to: Some(("wp_kitchen", "cam_kitchen")),
        states: &["awake", "on_tree", "garden"],
        laptop: false,
    },
    Shot {
        name: "casa entera",
        camera: "cam_house",
        spot: "wp_kitchen",
        clip: clip::IDLE_LOOKING_AROUND,
        walk_to: None,
        states: &["awake", "on_tree", "garden"],
        laptop: false,
    },
];

/// Reproduce una escena cuadro a cuadro.
pub struct ShotPlayer {
    pub shot: &'static Shot,
    anim: Animator,
    anim_pose: AnimPose,
    pose: Pose,
    t_ms: u32,
    laptop_screen: Option<usize>,
}

/// Duración del recorrido de `walk_to`.
const WALK_MS: u32 = 4000;

impl ShotPlayer {
    pub fn new(shot: &'static Shot, stage: &mut Stage) -> Self {
        for name in ["awake", "asleep", "on_tree", "garden"] {
            stage.world.set_state(name, shot.states.contains(&name));
        }
        stage.invalidate_all();
        Self {
            shot,
            anim: Animator::new(&CLIPS[shot.clip]),
            anim_pose: AnimPose::REST,
            pose: Pose::REST,
            t_ms: 0,
            laptop_screen: HOUSE.object("laptop_screen"),
        }
    }

    /// Prepara y envía un fotograma, `dt_ms` después del anterior. Devuelve
    /// la zona a redibujar (después hay que llamar a `stage.draw`).
    pub fn frame<const V: usize, const T: usize>(
        &mut self,
        stage: &mut Stage,
        r: &mut Renderer<V, T>,
        dt_ms: u32,
    ) -> Bounds {
        self.t_ms += dt_ms;
        self.anim.update(dt_ms);
        let shot = self.shot;
        let from = HOUSE.spot(shot.spot).expect("lugar");
        let cam_from = Eye::of(HOUSE.camera(shot.camera).expect("cámara"));

        let (eye, pos, yaw) = match shot.walk_to {
            Some((to, cam_to)) => {
                // Va y vuelve entre los dos puntos; la cámara lo sigue.
                let to = HOUSE.spot(to).expect("lugar");
                let phase = self.t_ms % (2 * WALK_MS);
                let (k, yaw) = if phase < WALK_MS {
                    (phase, -90)
                } else {
                    (2 * WALK_MS - phase, 90)
                };
                let k = (k * 256 / WALK_MS) as i32;
                let lerp = |a: i32, b: i32| a + (b - a) * k / 256;
                let pos = [
                    lerp(from.pos[0], to.pos[0]),
                    0,
                    lerp(from.pos[2], to.pos[2]),
                ];
                let cam_to = Eye::of(HOUSE.camera(cam_to).expect("cámara"));
                (cam_from.lerp(&cam_to, k), pos, yaw)
            }
            None => (cam_from, from.pos, from.yaw as i32),
        };
        let view = eye.view();
        stage.begin(r, &view);

        self.anim.pose(&mut self.anim_pose);
        self.anim_pose.to_pose(&mut self.pose);
        let model = view
            .then(&Transform::translate(pos))
            .then(&Transform::new(yaw_matrix(yaw), [0; 3]));
        // Parpadea y cambia la boca de vez en cuando.
        let eyes = if self.t_ms % 3000 < 150 { 2 } else { 0 };
        stage.chr.set_face(r, eyes, (self.t_ms / 1500 % 2) as u8);
        stage.submit_character(r, &model, &self.pose);

        if shot.laptop
            && let Some(obj) = self.laptop_screen
        {
            let row = 128 - (self.t_ms / 100 % 128) as i16;
            stage.world.scroll(r, "LaptopScreen", [0, row]);
            stage.invalidate_object(r, obj);
        }
        stage.finish(r)
    }
}
