//! Qué hace y dónde, para cada actividad: el lugar, la postura, la
//! animación, lo que tiene en la mano y si va de un lado a otro.

use assets::mono_anims::clip;
use companion::Activity;

/// Cómo está mientras hace la actividad.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Posture {
    Stand,
    /// Sentado en un asiento (`assets::world::Seat`): llega, se sienta
    /// (corriendo la silla si hay mesa adelante) y al irse se levanta.
    Sit,
    /// Acostado en la cama: se sienta en el costado y se acuesta.
    Lie,
}

/// Lo que tiene en la mano.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prop {
    None,
    Banana,
    Book,
    Can,
    Mate,
    Spoon,
    Guitar,
    Phone,
    Broom,
    Controller,
    Extinguisher,
}

/// A dónde va: un lugar de la escena o un punto cualquiera del piso.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Spot(&'static str),
    /// `[x, z]` en Q8.
    Point([i32; 2]),
}

/// Cómo se mueve mientras hace la actividad.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Roam {
    /// Se queda en su lugar.
    Stay,
    /// Va de un punto a otro de la habitación en la que está.
    Room,
    /// Recorre toda la casa (limpiando).
    House,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Plan {
    pub target: Target,
    pub posture: Posture,
    pub clip: usize,
    pub prop: Prop,
    /// Animación encima, solo en el tronco y los brazos (el teléfono en la
    /// oreja mientras camina).
    pub overlay: Option<usize>,
    pub roam: Roam,
}

/// El plan para `act`. `t` (segundos) alterna variantes, como los ejercicios.
pub fn plan(act: Activity, t: f32) -> Plan {
    let p = |spot, posture, clip, prop| Plan {
        target: Target::Spot(spot),
        posture,
        clip,
        prop,
        overlay: None,
        roam: Roam::Stay,
    };
    let roam = |overlay, prop, roam| Plan {
        target: Target::Spot("wp_kitchen"),
        posture: Posture::Stand,
        clip: clip::IDLE,
        prop,
        overlay,
        roam,
    };
    use Posture::*;
    match act {
        Activity::Sleep | Activity::Nap => p("spot_bed", Lie, clip::SLEEPING, Prop::None),
        Activity::WakeUp => p("wp_bedroom", Stand, clip::IDLE_BORED, Prop::None),
        Activity::Groggy => p("wp_bedroom", Stand, clip::DIZZY_IDLE, Prop::None),
        Activity::Eat | Activity::Snack => p("spot_table", Sit, clip::EAT_BANANA, Prop::Banana),
        Activity::Code => p("spot_desk", Sit, clip::TYPE_LAPTOP, Prop::None),
        Activity::Meeting => p("spot_desk", Sit, clip::VIDEO_CALL, Prop::None),
        Activity::Trading => p("spot_desk", Sit, clip::THINK, Prop::None),
        Activity::Read => p("spot_armchair", Sit, clip::READ_BOOK, Prop::Book),
        Activity::Mate => p("spot_armchair", Sit, clip::SIP_MATE, Prop::Mate),
        Activity::Games => p("spot_armchair", Sit, clip::GAMEPAD, Prop::Controller),
        Activity::Movie => p("spot_armchair", Sit, clip::SITTING_IDLE, Prop::None),
        // En diagonal a la cámara, para que se vea la guitarra.
        Activity::Guitar => p("spot_bed_guitar", Sit, clip::STRUM, Prop::Guitar),
        Activity::Cook => p("spot_stove", Stand, clip::STIR, Prop::Spoon),
        Activity::Garden => p("spot_veggies", Stand, clip::WATER_PLANTS, Prop::Can),
        Activity::Exercise => {
            let k = [clip::JUMPING_JACK, clip::AIR_SQUAT, clip::PUSH_UP][(t / 12.0) as usize % 3];
            p("spot_mat", Stand, k, Prop::None)
        }
        Activity::Stretch => p("spot_mat", Stand, clip::STRETCH, Prop::None),
        Activity::Dance => {
            let k = [
                clip::HIP_HOP_DANCE,
                clip::HIP_HOP_DANCE,
                clip::HIP_HOP_FLAIR,
            ][(t / 9.0) as usize % 3];
            p("spot_dance", Stand, k, Prop::None)
        }
        Activity::Daydream => p("spot_window", Stand, clip::IDLE_LOOKING_AROUND, Prop::None),
        Activity::Stargaze => p("spot_stars", Stand, clip::LOOK_UP, Prop::None),
        Activity::Bananas => p("spot_tree", Stand, clip::PICK_BANANA, Prop::None),
        Activity::Wander => roam(None, Prop::None, Roam::Room),
        Activity::Phone => roam(Some(clip::PHONE_CALL), Prop::Phone, Roam::Room),
        Activity::Clean => roam(Some(clip::SWEEP), Prop::Broom, Roam::House),
        Activity::Away => p("wp_tree", Stand, clip::IDLE, Prop::None),
    }
}
