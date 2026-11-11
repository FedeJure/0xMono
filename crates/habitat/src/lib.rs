//! La vida del mono en el hábitat 3D: lo que dice su agenda
//! ([`companion`]) hecho movimiento en la casa y el jardín.
//!
//! - [`plan`]: a dónde va, en qué postura y con qué animación para cada actividad.
//! - [`actor`]: cómo llega (caminos que esquivan muebles), se sienta
//!   corriendo la silla, se acuesta y se levanta.
//! - [`camera`]: cámara quieta por habitación, con paneo al cambiar.
//! - [`grip`]: cómo agarra cada objeto con la mano.
//! - [`light`]: la luz según la hora (amanecer, atardecer, las lámparas
//!   de noche).
//! - [`gags`]: lo gracioso que le pasa (un pedo dormido, la olla que se
//!   prende fuego, el sudor) y sus efectos ([`fx`]).
//! - [`bubble`]: el globo con lo que dice o piensa, encima del 3D.
//! - [`Habitat`]: junta todo, con los objetos en la mano, la luz del día y
//!   el dibujo (solo lo que cambia, ver `assets::stage`).
//!
//! Sin `std`: corre igual en el simulador y en el ESP32.
#![cfg_attr(not(test), no_std)]

pub mod actor;
pub mod bubble;
pub mod camera;
pub mod fx;
pub mod gags;
pub mod grip;
pub mod light;
pub mod plan;

use assets::anim::{AnimPose, Animator, BoneMask};
use assets::house::HOUSE;
use assets::mono::MONO;
use assets::mono_anims::{CLIPS, clip};
use assets::scene::{CharacterScene, Pose, toon};
use assets::stage::Stage;
use assets::world::WorldScene;
use companion::{Activity, Companion, Emotion, FaceAnimator, Greeting, LocalTime, Mouth, Reaction};
use engine3d::fixed::deg;
use engine3d::{Band, Bounds, Camera, Mat3, PixelOrder, Renderer, Rgb, Transform};
use libm::{atan2f, cosf, sinf, sqrtf};

use actor::{Actor, M, dist};
use bubble::{Bubble, Spot};
use camera::CameraRig;
use gags::{Gag, Gags};
use plan::{Plan, Prop, Roam, Target, plan};

/// Pantalla animada: parte con textura, su objeto y el desplazamiento con
/// que se dibujó.
type Screen = (&'static str, Option<usize>, Option<[i16; 2]>);

/// Los encargos de varios pasos: ir a buscar una banana y volver a comerla.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Errand {
    None,
    Picking,
    Returning,
}

pub use light::daylight;

/// Pantallas de las lámparas (`lamp` en `tools/blender/build_house.py`) y
/// cómo se ven encendidas.
const LAMP_SHADE: Rgb = Rgb(250, 232, 190);
const LAMP_ON: Rgb = Rgb(255, 248, 214);
/// Radio de la cabeza alrededor de la cara (1024 = 1 m),
/// para que el globo no la tape.
const HEAD_R: i32 = 200;

pub struct Habitat {
    pub stage: Stage,
    pub actor: Actor,
    pub camera: CameraRig,
    order: PixelOrder,
    anim: Animator,
    face: FaceAnimator,
    pose: Pose,
    upper_body: BoneMask,
    plan: Plan,
    errand: Errand,
    banana_picked_day: Option<u32>,
    /// Paseo: el punto al que va y desde cuándo está ahí.
    wander: Option<[i32; 2]>,
    wander_seed: u32,
    /// Segundos desde que arrancó (para animaciones que no dependen de la agenda).
    pub t: f32,
    /// Paso de luz con el que están los materiales: tramo de la hora, si la
    /// cámara está afuera y si está despierto (lámparas).
    light_step: Option<(u32, bool, bool)>,
    /// Pantallas animadas: parte con textura, su objeto y el desplazamiento
    /// con que se dibujó (la laptop con código, el monitor con el gráfico).
    screens: [Screen; 2],
    glass: Option<usize>,
    /// El material de las pantallas de las lámparas (se prenden de noche).
    lamp: Option<usize>,
    obj_banana: usize,
    obj_book: usize,
    obj_can: usize,
    obj_mate: usize,
    obj_spoon: usize,
    obj_guitar: usize,
    obj_phone: usize,
    obj_broom: usize,
    obj_controller: usize,
    obj_extinguisher: usize,
    pub gags: Gags,
    /// La olla (mundo, Q8): de ahí salen el humo y las llamas.
    pot: [i32; 3],
    /// Para revisar: hace esta actividad en vez de la de la agenda.
    pub force: Option<Activity>,
    /// El globo con lo que dice (se dibuja en cada franja, ver [`bubble`]).
    pub bubble: Bubble,
}

fn obj(name: &str) -> usize {
    HOUSE
        .object(name)
        .unwrap_or_else(|| panic!("falta el objeto {name}"))
}

impl Habitat {
    pub fn new<const V: usize, const T: usize>(r: &mut Renderer<V, T>, order: PixelOrder) -> Self {
        let base = (MONO.colors.len() + MONO.parts.len()) as u8;
        let stage = Stage::new(CharacterScene::new(&MONO), WorldScene::new(&HOUSE, base));
        stage.setup(r, order);
        let start = HOUSE.spot("wp_kitchen").expect("wp_kitchen");
        let at = [start.pos[0] as f32, start.pos[2] as f32];
        let spine = MONO.bone(":Spine").expect("falta Spine");
        let glass = HOUSE
            .colors
            .iter()
            .position(|c| *c == Rgb(168, 214, 236))
            .map(|i| base as usize + i);
        let lamp = HOUSE
            .colors
            .iter()
            .position(|c| *c == LAMP_SHADE)
            .map(|i| base as usize + i);
        let mut h = Self {
            stage,
            actor: Actor::new(at),
            camera: CameraRig::new(at[0]),
            order,
            anim: Animator::new(&CLIPS[clip::IDLE]),
            face: FaceAnimator::new(Emotion::Content, 0xFACE),
            pose: Pose::REST,
            upper_body: MONO.mask_from(spine),
            plan: plan(Activity::Wander, 0.0),
            errand: Errand::None,
            banana_picked_day: None,
            wander: None,
            wander_seed: 0x5EED,
            t: 0.0,
            light_step: None,
            screens: [
                ("LaptopScreen", HOUSE.object("laptop_screen"), None),
                ("MonitorScreen", HOUSE.object("monitor_screen"), None),
            ],
            glass,
            lamp,
            obj_banana: obj("prop_banana"),
            obj_book: obj("prop_book"),
            obj_can: obj("prop_can"),
            obj_mate: obj("prop_mate"),
            obj_spoon: obj("prop_spoon"),
            obj_guitar: obj("prop_guitar"),
            obj_phone: obj("prop_phone"),
            obj_broom: obj("prop_broom"),
            obj_controller: obj("prop_controller"),
            obj_extinguisher: obj("prop_extinguisher"),
            gags: Gags::new(),
            pot: HOUSE.spot("spot_pot").expect("spot_pot").pos,
            force: None,
            bubble: Bubble::new(),
        };
        for state in [
            "awake",
            "on_tree",
            "garden",
            "guitar_home",
            "broom_home",
            "extinguisher_home",
        ] {
            let on = true;
            h.stage.world.set_state(state, on);
        }
        h
    }

    /// Lo que quiere hacer ahora (según la actividad y los encargos).
    fn current_plan(&mut self, c: &Companion) -> Plan {
        let act = self.activity(c);
        let mut p = plan(act, self.t);
        if act == Activity::Bananas && self.errand == Errand::Returning {
            p = plan(Activity::Eat, self.t);
        }
        if p.roam != Roam::Stay {
            // Va de un punto a otro: paseando se queda un rato en cada uno,
            // limpiando o hablando por teléfono casi no para.
            let pause = if act == Activity::Wander { 6.0 } else { 1.5 };
            let due = self.actor.settled() && self.actor.step_t > pause;
            if self.wander.is_none() || due {
                self.wander = Some(self.wander_point(p.roam));
            }
            p.target = Target::Point(self.wander.unwrap());
        } else {
            self.wander = None;
        }
        if let Some(Reaction::Grumpy) = c.reaction() {
            p.clip = clip::ANGRY_GESTURE;
        }
        // Un incendio en la cocina cambia los planes.
        self.gags.plan(p)
    }

    /// Dispara un gag ya, si la actividad lo permite (para revisar).
    pub fn trigger(&mut self, gag: Gag) {
        self.gags.trigger(gag);
    }

    fn activity(&self, c: &Companion) -> Activity {
        self.force.unwrap_or(c.activity())
    }

    /// Un nodo de la red de navegación: cerca (en la misma habitación) o,
    /// para limpiar, en cualquier parte de adentro de la casa.
    fn wander_point(&mut self, roam: Roam) -> [i32; 2] {
        let nav = HOUSE.nav;
        let here = self.actor.pos;
        // La pared de la puerta: más allá es el jardín.
        let door = HOUSE.spot("wp_door_in").map_or(i32::MAX, |s| s.pos[0]);
        let (near, far) = match roam {
            Roam::House => (1.0 * M, 5.0 * M),
            _ => (0.6 * M, 1.6 * M),
        };
        let mut best = nav.nodes[0];
        for _ in 0..32 {
            self.wander_seed = self
                .wander_seed
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            let n = nav.nodes[(self.wander_seed >> 8) as usize % nav.nodes.len()];
            let d = dist(here, [n[0] as f32, n[1] as f32]);
            let inside = n[0] < door;
            let outside_now = here[0] > door as f32;
            if (near..far).contains(&d)
                && (roam != Roam::House || inside)
                && (inside != outside_now || roam == Roam::House)
            {
                return n;
            }
            best = n;
        }
        best
    }

    /// Lo pone directamente en su lugar (para fotos y al arrancar).
    pub fn teleport(&mut self, c: &Companion) {
        self.plan = self.current_plan(c);
        self.actor.teleport(&self.plan, &mut self.anim);
        self.camera.snap(self.actor.pos[0]);
    }

    pub fn update(&mut self, c: &Companion, now: LocalTime, dt_ms: u32) {
        let dt = dt_ms as f32 / 1000.0;
        self.t += dt;
        if self.activity(c) != Activity::Bananas {
            self.errand = Errand::None;
        }
        if self.banana_picked_day.is_some_and(|d| d != now.day) {
            self.banana_picked_day = None;
            self.stage.world.set_state("on_tree", true);
        }
        self.plan = self.current_plan(c);
        self.actor
            .update(&self.plan, dt, &mut self.anim, &mut self.stage.world);

        // Bananas: cosechar, volver a la mesa y comer.
        if self.activity(c) == Activity::Bananas && self.actor.settled() {
            let picking = self.anim.clip().name == "pick_banana";
            if self.errand == Errand::None && picking {
                self.errand = Errand::Picking;
            }
            if self.errand == Errand::Picking && self.actor.step_t > 1.3 {
                self.stage.world.set_state("on_tree", false);
                self.banana_picked_day = Some(now.day);
            }
            if self.errand == Errand::Picking && self.anim.finished() {
                self.errand = Errand::Returning;
            }
        }

        // Lo gracioso que le pasa (con las posiciones del último fotograma).
        let scene = gags::Scene {
            act: self.activity(c),
            settled: self.actor.settled(),
            still_t: self.actor.step_t,
            target: self.actor.target(),
            lying: self.actor.step == actor::Step::Lying,
            hips: self.bone_position(":Hips"),
            // La frente, a los dos lados (la cabeza es enorme: más adentro
            // las gotas no se verían).
            brow: [
                self.bone_point(":Head", [-150, 280, -200]),
                self.bone_point(":Head", [150, 280, -200]),
            ],
            nozzle: self.bone_position(":LeftHandIndex1"),
            pot: self.pot,
        };
        self.gags.update(&scene, dt);

        // Encima, con el tronco y los brazos: las reacciones, los sustos de
        // los gags o lo que hace mientras camina (el teléfono, la escoba).
        match (c.reaction(), self.gags.overlay().or(self.plan.overlay)) {
            (Some(Reaction::Greet(Greeting::MissedYou)), _) => {
                self.anim
                    .overlay(&CLIPS[clip::CHEERING_GESTURE], self.upper_body, 250)
            }
            (Some(Reaction::Greet(_)), _) if !self.actor.seated() => {
                self.anim
                    .overlay(&CLIPS[clip::WAVING], self.upper_body, 250)
            }
            (_, Some(k)) => self.anim.overlay(&CLIPS[k], self.upper_body, 300),
            _ => self.anim.clear_overlay(400),
        }
        self.anim.update(dt_ms);

        let emotion = match self.force {
            Some(act) if c.reaction().is_none() => {
                companion::activity_emotion(act, c.personality.seed, now)
            }
            _ => c.emotion(),
        };
        self.face.set_emotion(emotion);
        self.actor.cheerful =
            matches!(emotion, Emotion::Happy | Emotion::Joyful | Emotion::Playful);
        self.face.update(dt_ms);

        // Estado de los objetos de la escena.
        let asleep = self.activity(c).is_asleep() && self.actor.step == actor::Step::Lying;
        self.stage.world.set_state("asleep", asleep);
        self.stage.world.set_state("awake", !asleep);
        // Lo que está en la mano no está en su lugar.
        let held = self.held();
        self.stage.world.set_state("garden", held != Prop::Can);
        self.stage
            .world
            .set_state("guitar_home", held != Prop::Guitar);
        self.stage
            .world
            .set_state("broom_home", held != Prop::Broom);
        self.stage
            .world
            .set_state("extinguisher_home", !self.gags.carrying);

        self.camera.update(self.actor.pos[0], dt);
    }

    /// Qué tiene en la mano ahora.
    fn held(&self) -> Prop {
        if self.gags.carrying {
            return Prop::Extinguisher;
        }
        if self.plan.roam != Roam::Stay {
            // Lo lleva consigo mientras va de un lado a otro.
            return self.plan.prop;
        }
        if !self.actor.settled() {
            return Prop::None;
        }
        if self.anim.clip().name == "pick_banana" {
            return if self.actor.step_t > 1.3 {
                Prop::Banana
            } else {
                Prop::None
            };
        }
        self.plan.prop
    }

    /// Dibuja el fotograma: solo la zona que cambió. `color` y `depth` son
    /// los búferes de las franjas; `flush` recibe cada franja terminada (ya
    /// con el globo de lo que dice encima, ver [`bubble`]). Devuelve la zona
    /// redibujada.
    pub fn render<const V: usize, const T: usize>(
        &mut self,
        r: &mut Renderer<V, T>,
        c: &Companion,
        now: LocalTime,
        color: &mut [u16],
        depth: &mut [u16],
        mut flush: impl FnMut(&mut Band),
    ) -> Bounds {
        // La luz cambia de a pasos (cada 6 minutos, al pasar al jardín o al
        // prender las lámparas): cada paso retiñe los materiales y redibuja
        // todo; entre pasos se redibuja solo lo que se mueve.
        let outdoor = self.camera.zone == camera::OUTDOOR;
        let awake = !self.activity(c).is_asleep();
        let step = (now.secs / 360, outdoor, awake);
        if self.light_step != Some(step) {
            self.light_step = Some(step);
            let look = light::look(step.0 as f32 * 6.0 + 3.0, outdoor, awake);
            r.light.ambient = look.ambient;
            r.light.dir = look.dir;
            let base = self.stage.world.material_base as usize;
            for (i, &rgb) in MONO.colors.iter().enumerate() {
                r.materials[i] = toon(light::tinted(rgb, look.tint), self.order);
            }
            for (i, &rgb) in HOUSE.colors.iter().enumerate() {
                r.materials[base + i] = toon(light::tinted(rgb, look.tint), self.order);
            }
            if let Some(g) = self.glass {
                // Por las ventanas se ve el cielo.
                r.materials[g] = toon(look.sky_low, self.order);
            }
            if let Some(l) = self.lamp
                && look.lamps
            {
                r.materials[l] = toon(LAMP_ON, self.order);
            }
            let (top, horizon) = (look.sky_top, look.sky_low);
            let rows = r.camera.height as u32;
            for (y, px) in self.stage.sky.iter_mut().enumerate() {
                *px = self
                    .order
                    .encode(top.lerp(horizon, (y as u32).min(rows), rows).to_565());
            }
            self.stage.invalidate_all();
        }
        // El código corre en la laptop mientras programa y el gráfico avanza
        // en el monitor mientras mira el mercado.
        let settled = self.actor.settled();
        let offsets = [
            (self.plan.clip == clip::TYPE_LAPTOP && settled)
                .then(|| [0, (128 - (self.t * 10.0) as i32 % 128) as i16]),
            (self.plan.clip == clip::THINK && settled)
                .then_some([((self.t * 4.0) as i32 % 64) as i16, 0]),
        ];
        for (k, off) in offsets.iter().enumerate() {
            if let Some(o) = off
                && *off != self.screens[k].2
            {
                self.stage.world.scroll(r, self.screens[k].0, *o);
            }
        }
        let mut face = self.gags.face().unwrap_or(self.face.face());
        if self.anim.clip().name == "eat_banana" && settled {
            // Abre la boca para morder y mastica con el cachete inflado
            // hasta el próximo bocado.
            let cycle = self.anim.time_ms() % 2600;
            const CHEW: [Mouth; 4] = [Mouth::Chew1, Mouth::Chew2, Mouth::Chew3, Mouth::Chew2];
            match cycle {
                850..=1050 => face.mouth = Mouth::SmileBig,
                1051.. => face.mouth = CHEW[(cycle - 1051) as usize / 170 % 4],
                // Al principio del ciclo todavía está tragando.
                0..=250 => face.mouth = Mouth::Chew1,
                _ => {}
            }
        }

        let view = self.camera.eye.view();
        // Con la cámara moviéndose se redibuja todo: el globo puede
        // acomodarse a la cara sin costo.
        let panning = self.stage.view() != Some(view);
        self.stage.begin(r, &view);
        for (k, off) in offsets.iter().enumerate() {
            if *off != self.screens[k].2 {
                self.screens[k].2 = *off;
                if let Some(o) = self.screens[k].1 {
                    self.stage.invalidate_object(r, o);
                }
            }
        }

        let visible = self.activity(c).is_present() || self.actor.is_walking();
        if visible {
            let mut p = AnimPose::REST;
            self.anim.pose(&mut p);
            p.to_pose(&mut self.pose);
            let pl = self.actor.placement();
            let pos = [pl.pos[0] as i32, pl.lift as i32, pl.pos[1] as i32];
            let model = view
                .then(&Transform::translate(pos))
                .then(&Transform::new(Mat3::rot_y(deg(pl.yaw as i32)), [0; 3]));
            self.stage
                .chr
                .set_face(r, face.eyes as u8, face.mouth as u8);
            self.stage.submit_character(r, &model, &self.pose);
            self.submit_prop(r, &model);
        }
        self.gags.fx.submit(&mut self.stage, r, &view);
        // El globo: si aparece, cambia o se va, se redibuja su lugar.
        let face = if visible {
            self.face_on_screen(&r.camera)
        } else {
            None
        };
        let dirty = self.bubble.update(c.speech(), face, r.screen(), panning);
        self.stage.invalidate(dirty);
        let area = self.stage.finish(r);
        let (bubble, order) = (&self.bubble, self.order);
        self.stage.draw(r, color, depth, |band| {
            bubble.draw(band, order);
            flush(band);
        });
        area
    }

    /// El objeto en la mano: los de una mano, agarrados en el puño
    /// ([`grips`]); el libro, la guitarra, la escoba y el joystick, entre
    /// las dos.
    fn submit_prop<const V: usize, const T: usize>(
        &mut self,
        r: &mut Renderer<V, T>,
        model: &Transform,
    ) {
        let held = self.held();
        let stage = &mut self.stage;
        let hand = |side: &str| match side {
            "Left" => MONO.bone(":LeftHand").expect("LeftHand"),
            _ => MONO.bone(":RightHand").expect("RightHand"),
        };
        let joint = |b: &[Transform], i: usize| b[i].apply(MONO.bones[i].head);
        match held {
            Prop::None => {}
            Prop::Banana
            | Prop::Mate
            | Prop::Spoon
            | Prop::Phone
            | Prop::Can
            | Prop::Extinguisher => {
                let (object, grip) = match held {
                    Prop::Banana => (self.obj_banana, grips::BANANA),
                    Prop::Mate => (self.obj_mate, grips::MATE),
                    Prop::Spoon => (self.obj_spoon, grips::SPOON),
                    Prop::Phone => (self.obj_phone, grips::PHONE),
                    Prop::Can => (self.obj_can, grips::CAN),
                    _ => (self.obj_extinguisher, grips::EXTINGUISHER),
                };
                let tr = grip.transform(&stage.bones);
                stage.submit_prop(r, object, &tr);
            }
            Prop::Book => {
                let (l, rt) = (
                    joint(&stage.bones, hand("Left")),
                    joint(&stage.bones, hand("Right")),
                );
                // Entre las manos (que lo toman de los bordes, a ±0,10 m), lo
                // bastante adelante para que los dedos no lo atraviesen.
                let ahead = model.m.apply([0, 30, -80]);
                let mid = [
                    (l[0] + rt[0]) / 2 + ahead[0],
                    (l[1] + rt[1]) / 2 + ahead[1],
                    (l[2] + rt[2]) / 2 + ahead[2],
                ];
                let local = Mat3::rot_y(deg(180)).mul(&Mat3::rot_x(deg(-25)));
                let tr = Transform::new(model.m.mul(&local), mid);
                stage.submit_prop(r, self.obj_book, &tr);
            }
            Prop::Guitar | Prop::Broom | Prop::Controller => {
                let (l, rt) = (
                    joint(&stage.bones, hand("Left")),
                    joint(&stage.bones, hand("Right")),
                );
                let d = [l[0] - rt[0], l[1] - rt[1], l[2] - rt[2]];
                let mid = [(l[0] + rt[0]) / 2, (l[1] + rt[1]) / 2, (l[2] + rt[2]) / 2];
                let tr = match held {
                    Prop::Guitar => {
                        // La boca un poco debajo de la mano que rasguea y
                        // apenas adelante de la panza; el mástil sube hasta
                        // la mano izquierda y el frente mira hacia afuera.
                        let fwd = model.m.apply([0, 0, -1024]);
                        let at = [
                            rt[0] + fwd[0] * 3 / 100,
                            rt[1] - 75,
                            rt[2] + fwd[2] * 3 / 100,
                        ];
                        let neck = [l[0] - at[0], l[1] - at[1], l[2] - at[2]];
                        Transform::new(basis(neck, fwd.map(|c| -c)), at)
                    }
                    // El palo va de una mano a la otra (la izquierda arriba).
                    Prop::Broom => Transform::new(align_y(d), mid),
                    // El joystick, apenas adelante de las manos e inclinado
                    // hacia la cara.
                    _ => {
                        let fwd = model.m.apply([0, 15, -80]);
                        let at = [mid[0] + fwd[0], mid[1] + fwd[1], mid[2] + fwd[2]];
                        Transform::new(model.m.mul(&Mat3::rot_x(deg(-35))), at)
                    }
                };
                let object = match held {
                    Prop::Guitar => self.obj_guitar,
                    Prop::Broom => self.obj_broom,
                    _ => self.obj_controller,
                };
                stage.submit_prop(r, object, &tr);
            }
        }
    }

    /// Dónde está la cara en el mundo (Q8), según la pose del último
    /// fotograma dibujado: para encuadrarla de cerca.
    pub fn face_position(&self) -> [i32; 3] {
        let h = MONO.bone(":Head").expect("Head");
        let j = MONO.bones[h].head;
        // La cara: un poco arriba y adelante de la articulación del cuello.
        let v = self.stage.bones[h].apply([j[0], j[1] + 150, j[2] - 60]);
        let eye = self.camera.eye;
        let w = eye.rot.to_mat3().apply(v);
        [w[0] + eye.pos[0], w[1] + eye.pos[1], w[2] + eye.pos[2]]
    }

    /// Dónde está la cara en pantalla (centro y radio), según la pose
    /// recién enviada. `None` si queda detrás de la cámara.
    fn face_on_screen(&self, cam: &Camera) -> Option<Spot> {
        let h = MONO.bone(":Head")?;
        let j = MONO.bones[h].head;
        let v = self.stage.bones[h].apply([j[0], j[1] + 150, j[2] - 60]);
        if v[2] < cam.near {
            return None;
        }
        Some(Spot {
            x: cam.cx + cam.focal * v[0] / v[2],
            y: cam.cy - cam.focal * v[1] / v[2],
            r: cam.focal * HEAD_R / v[2],
        })
    }

    /// Dónde está la articulación del hueso `name` en el mundo (Q8), según
    /// la pose del último fotograma dibujado.
    pub fn bone_position(&self, name: &str) -> Option<[i32; 3]> {
        self.bone_point(name, [0; 3])
    }

    /// Un punto pegado al hueso `name`: `offset` (Q8) desde su articulación,
    /// en la pose de reposo (Y arriba, el personaje mira a −Z).
    pub fn bone_point(&self, name: &str, offset: [i32; 3]) -> Option<[i32; 3]> {
        let h = MONO.bone(name)?;
        let j = MONO.bones[h].head;
        let v = self.stage.bones[h].apply([j[0] + offset[0], j[1] + offset[1], j[2] + offset[2]]);
        let eye = self.camera.eye;
        let w = eye.rot.to_mat3().apply(v);
        Some([w[0] + eye.pos[0], w[1] + eye.pos[1], w[2] + eye.pos[2]])
    }

    /// La animación base que está sonando (para depurar).
    pub fn clip_name(&self) -> &'static str {
        self.anim.clip().name
    }

    /// ¿Hay que dibujar la vista general de la casa?
    pub fn set_overview(&mut self, on: bool) {
        self.camera.overview = on;
    }
}

/// Cómo agarra cada objeto de una mano (ver [`grip`]).
mod grips {
    use crate::grip::{A, F, Grip, N, Side, neg};

    /// En el puño, asomando por el lado del pulgar.
    pub const BANANA: Grip = Grip {
        side: Side::Right,
        at: [0.075, 0.03, -0.035],
        y: A,
        z: N,
    };
    /// La calabaza en el puño, con la bombilla para arriba.
    pub const MATE: Grip = Grip {
        side: Side::Right,
        at: [0.07, 0.04, 0.005],
        y: A,
        z: N,
    };
    /// El mango en el puño (asomando arriba del pulgar) y la cuchara
    /// saliendo por el lado del meñique.
    pub const SPOON: Grip = Grip {
        side: Side::Right,
        at: [0.075, 0.025, 0.06],
        y: A,
        z: N,
    };
    /// Apoyado en la palma, a lo largo de los dedos, con la pantalla hacia
    /// afuera de la palma (habla en altavoz, con la palma hacia la cara).
    pub const PHONE: Grip = Grip {
        side: Side::Right,
        at: [0.075, 0.045, 0.0],
        y: F,
        z: neg(N),
    };
    /// La manija de arriba en el puño (de adelante hacia atrás, a lo largo
    /// de los nudillos) y el cuerpo colgando más allá de los dedos.
    pub const CAN: Grip = Grip {
        side: Side::Right,
        at: [0.075, 0.025, 0.0],
        y: neg(F),
        z: neg(A),
    };
    pub const EXTINGUISHER: Grip = CAN;
}

/// Rotación con el eje X hacia `x` y el Z lo más cerca posible de `z`.
fn basis(x: [i32; 3], z: [i32; 3]) -> Mat3 {
    let norm = |v: [f32; 3]| {
        let l = sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).max(1e-6);
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let x = norm(x.map(|c| c as f32));
    let y = norm(cross(z.map(|c| c as f32), x));
    let z = cross(x, y);
    let q = |f: f32| (f * 16384.0) as i32;
    Mat3([
        [q(x[0]), q(y[0]), q(z[0])],
        [q(x[1]), q(y[1]), q(z[1])],
        [q(x[2]), q(y[2]), q(z[2])],
    ])
}

/// Matriz que lleva el eje +Y a la dirección `d` (giro mínimo).
fn align_y(d: [i32; 3]) -> Mat3 {
    let (x, y, z) = (d[0] as f32, d[1] as f32, d[2] as f32);
    let l = sqrtf(x * x + y * y + z * z).max(1.0);
    let v = [d[0] as f32 / l, d[1] as f32 / l, d[2] as f32 / l];
    let (ax, ay, az) = (v[2], 0.0f32, -v[0]); // y × v
    let s = sqrtf(ax * ax + az * az);
    let c = v[1];
    if s < 1e-6 {
        return if c > 0.0 {
            Mat3::IDENTITY
        } else {
            Mat3::rot_x(deg(180))
        };
    }
    let (x, y, z) = (ax / s, ay / s, az / s);
    let ang = atan2f(s, c);
    let (sn, cs) = (sinf(ang), cosf(ang));
    let t = 1.0 - cs;
    let q = |f: f32| (f * 16384.0) as i32;
    Mat3([
        [
            q(t * x * x + cs),
            q(t * x * y - sn * z),
            q(t * x * z + sn * y),
        ],
        [
            q(t * x * y + sn * z),
            q(t * y * y + cs),
            q(t * y * z - sn * x),
        ],
        [
            q(t * x * z - sn * y),
            q(t * y * z + sn * x),
            q(t * z * z + cs),
        ],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::scene;
    use companion::Personality;

    const W: usize = 240;
    type R = Renderer<4608, 2816>;

    /// Dibuja un fotograma con franjas de `rows` filas sobre `screen` (la
    /// memoria de la pantalla) y devuelve la zona redibujada.
    fn frame(
        hab: &mut Habitat,
        r: &mut R,
        c: &Companion,
        now: LocalTime,
        rows: usize,
        screen: &mut [u16],
    ) -> Bounds {
        let (mut color, mut depth) = (vec![0u16; W * rows], vec![0u16; W * rows]);
        hab.render(r, c, now, &mut color, &mut depth, |b| {
            for row in 0..b.height {
                let dst = (b.y0 + row) * W + b.x0;
                screen[dst..dst + b.width].copy_from_slice(&b.color[row * b.width..][..b.width]);
            }
        })
    }

    /// El globo aparece, cambia, se va y vuelve como pensamiento mientras
    /// el mono camina (y la cámara pasa a otra habitación), con zonas que lo
    /// cortan por la mitad: la pantalla redibujada de a partes (en franjas
    /// de 7 filas) queda siempre igual a dibujarla entera.
    #[test]
    fn the_bubble_survives_partial_redraws() {
        std::thread::Builder::new()
            .stack_size(64 << 20)
            .spawn(|| {
                let now = LocalTime::new(20_002, 14, 0);
                let mut c = Companion::new(Personality::from_seed(11), None, now);
                let mut r = Box::new(R::new(Camera::new(240, 240, 300, 96), scene::light()));
                let mut hab = Habitat::new(&mut r, PixelOrder::Swapped);
                hab.force = Some(Activity::Wander);
                hab.teleport(&c);
                let mut screen = vec![0u16; W * W];
                let mut fresh = vec![0u16; W * W];
                let (mut seen, mut partial, mut gone) = (0, 0, false);
                for f in 0..70 {
                    match f {
                        3 | 8 => {
                            c.poke(now);
                        }
                        _ => {}
                    }
                    if f > 8 && c.speech().is_none() && !gone {
                        // Se fue el globo: ahora piensa algo.
                        gone = true;
                        c.think(now);
                        assert!(c.speech().is_some(), "no pensó nada");
                    }
                    c.update(now, 250);
                    hab.update(&c, now, 250);
                    // Algo que se redibuja cortando el globo por la mitad
                    // (como el mono pasando por debajo).
                    if f % 3 == 1 {
                        let x = (f * 17) % 150;
                        hab.stage.invalidate(Bounds::new(x, 20 + f % 9, x + 61, 47));
                    }
                    let area = frame(&mut hab, &mut r, &c, now, 7, &mut screen);
                    if area != r.screen() {
                        partial += 1;
                    }
                    if !hab.bubble.area().is_empty() {
                        seen += 1;
                    }
                    hab.stage.invalidate_all();
                    frame(&mut hab, &mut r, &c, now, 40, &mut fresh);
                    if let Some(i) = screen.iter().zip(&fresh).position(|(a, b)| a != b) {
                        panic!(
                            "fotograma {f}: píxel ({}, {}) viejo; globo {:?} en {:?}",
                            i % W,
                            i / W,
                            hab.bubble.text(),
                            hab.bubble.area()
                        );
                    }
                }
                assert!(gone, "el globo nunca se fue");
                assert!(seen > 10 && seen < 70, "globo en {seen} fotogramas");
                assert!(partial > 30, "solo {partial} fotogramas parciales");
                assert!(hab.bubble.area().is_empty(), "quedó un globo");
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
