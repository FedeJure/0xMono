//! Cómo se mueve por la casa: caminos que esquivan los muebles, sentarse
//! corriendo la silla, acostarse en la cama y levantarse.
//!
//! Sentarse a una mesa: llega por un costado de la silla, la corre hacia
//! atrás, se para delante, se sienta y se acerca a la mesa con la silla.
//! Para irse hace lo mismo al revés, y la silla vuelve a su lugar cuando
//! ya se alejó. Acostarse: se sienta en el costado largo de la cama
//! mirando a la cámara, se recuesta de lado hacia la almohada y sube las
//! piernas (`lie_down_side`); para levantarse, al revés (`get_up_side`).
//!
//! Posiciones `[x, z]` en Q8 (1024 = 1 m) como `f32`; ángulos en grados
//! del motor (0 = mirando a la cámara).

use assets::anim::Animator;
use assets::house::HOUSE;
use assets::mono_anims::{CLIPS, clip};
use assets::nav::{MAX_PATH, Point};
use assets::world::{Spot, WorldScene};
use libm::{atan2f, cosf, sinf, sqrtf};

use crate::plan::{Plan, Posture, Target};

pub const M: f32 = 1024.0;
/// Segundos para correr la silla (o acercarla con él sentado).
const PULL_S: f32 = 0.7;
/// Giro máximo, grados por segundo.
const TURN: f32 = 360.0;
/// Velocidad de los pasos cortos (meterse delante de la silla), Q8 por segundo.
const STEP_SPEED: f32 = 0.35 * M;
/// `walking` viene en el lugar (sin avance): su velocidad, medida por el pie
/// apoyado en el piso (va hacia atrás a 0,59 m/s).
const WALKING_SPEED: f32 = 0.59 * M;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// Caminando por el camino calculado.
    Walk,
    /// Al costado de la silla, corriéndola hacia atrás.
    PullChair,
    /// Pasos cortos hasta quedar delante del asiento.
    StepIn,
    SitDown,
    /// Sentado, acercándose a la mesa con la silla.
    SlideIn,
    Seated,
    SlideOut,
    StandUp,
    /// Pasos cortos de delante del asiento al costado.
    StepOut,
    /// Del costado de la cama a acostado.
    LieDown,
    Lying,
    /// De acostado a sentado en el costado de la cama.
    SitUp,
    /// Parado en su lugar.
    Standing,
}

/// Un asiento (o el costado de la cama) ya resuelto en coordenadas.
#[derive(Clone, Copy, Debug)]
struct Seat {
    /// Donde queda sentado con la silla en su lugar.
    seat: [f32; 2],
    /// Donde se sienta con la silla corrida.
    out: [f32; 2],
    /// Donde está parado justo antes de sentarse y después de levantarse.
    stand: [f32; 2],
    /// Punto libre al costado, desde donde se acerca.
    approach: [f32; 2],
    yaw: f32,
    /// Altura del asiento respecto de una silla común (Q8): el borde de la cama es más alto.
    lift: f32,
    chair: Option<&'static str>,
    /// Desplazamiento de la silla corrida del todo (Q8).
    back: [f32; 2],
}

/// Dónde dibujarlo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub pos: [f32; 2],
    pub lift: f32,
    pub yaw: f32,
}

pub struct Actor {
    pub pos: [f32; 2],
    pub yaw: f32,
    pub lift: f32,
    pub step: Step,
    /// Segundos en el paso actual.
    pub step_t: f32,
    /// A dónde fue (o está yendo) y con qué postura.
    target: Option<Target>,
    posture: Posture,
    seat: Option<Seat>,
    path: [Point; MAX_PATH],
    path_len: usize,
    path_i: usize,
    /// La silla corrida (nombre, desplazamiento completo) y cuánto (0..1).
    chair: Option<(&'static str, [f32; 2])>,
    chair_k: f32,
    /// Con qué caminata va (`happy_walk` o `walking`): se elige en cada viaje.
    walk_clip: usize,
    /// De buen humor camina más seguido dando saltitos (lo pone el hábitat).
    pub cheerful: bool,
    seed: u32,
}

fn spot(name: &str) -> &'static Spot {
    HOUSE
        .spot(name)
        .unwrap_or_else(|| panic!("falta el lugar {name}"))
}

fn xz(s: &Spot) -> [f32; 2] {
    [s.pos[0] as f32, s.pos[2] as f32]
}

pub fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dz) = (a[0] - b[0], a[1] - b[1]);
    sqrtf(dx * dx + dz * dz)
}

fn lerp2(a: [f32; 2], b: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k]
}

/// Ángulo (grados del motor) para mirar en la dirección (dx, dz).
pub fn heading(dx: f32, dz: f32) -> f32 {
    atan2f(-dx, -dz).to_degrees()
}

pub fn wrap_deg(a: f32) -> f32 {
    let a = (a + 180.0) % 360.0;
    if a < 0.0 { a + 180.0 } else { a - 180.0 }
}

/// Un vector local del personaje (`[x, z]`, mirando a −z) girado `yaw`.
fn rotate(v: [f32; 2], yaw: f32) -> [f32; 2] {
    let (s, c) = (sinf(yaw.to_radians()), cosf(yaw.to_radians()));
    [v[0] * c + v[1] * s, -v[0] * s + v[1] * c]
}

fn ease(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// Corrimiento con el que `stand_to_sit` termina igual que `sitting_idle`
/// (local, Q8): la transición se dibuja corrida para que encaje en el asiento.
fn sit_shift() -> [f32; 2] {
    let (sit, tr) = (&CLIPS[clip::SITTING_IDLE], &CLIPS[clip::STAND_TO_SIT]);
    let end = tr.root_offset(tr.duration_ms());
    let goal = sit.root_offset(0);
    [(goal[0] - end[0]) as f32, (goal[2] - end[2]) as f32]
}

/// Dónde quedan los pies al empezar `stand_to_sit` (local, desde el asiento).
fn stand_ahead() -> [f32; 2] {
    let d = sit_shift();
    let s0 = CLIPS[clip::STAND_TO_SIT].root_offset(0);
    [d[0] + s0[0] as f32, d[1] + s0[2] as f32]
}

fn resolve_seat(name: &str) -> Seat {
    let sp = spot(name);
    let info = HOUSE
        .seat(name)
        .unwrap_or_else(|| panic!("{name} no es un asiento"));
    let yaw = sp.yaw as f32;
    let seat = xz(sp);
    let fwd = rotate([0.0, -1.0], yaw);
    let pull = info.pull as f32;
    let back = [-fwd[0] * pull, -fwd[1] * pull];
    let out = [seat[0] + back[0], seat[1] + back[1]];
    let ahead = rotate(stand_ahead(), yaw);
    Seat {
        seat,
        out,
        stand: [out[0] + ahead[0], out[1] + ahead[1]],
        approach: [info.approach[0] as f32, info.approach[1] as f32],
        yaw,
        lift: sp.pos[1] as f32,
        chair: info.chair,
        back,
    }
}

/// Asiento desde donde se acuesta (las animaciones de acostarse y
/// levantarse van de este lugar a `spot_bed` y vuelta).
const BED_SIDE: &str = "spot_bed_side";

/// Asiento de cada postura: el costado de la cama para acostarse.
fn seat_for(target: Target, posture: Posture) -> Option<Seat> {
    match (target, posture) {
        (Target::Spot(_), Posture::Lie) => Some(resolve_seat(BED_SIDE)),
        (Target::Spot(s), Posture::Sit) => Some(resolve_seat(s)),
        _ => None,
    }
}

fn target_point(t: Target) -> [f32; 2] {
    match t {
        Target::Spot(s) => xz(spot(s)),
        Target::Point(p) => [p[0] as f32, p[1] as f32],
    }
}

impl Actor {
    /// Parado en `at`.
    pub fn new(at: [f32; 2]) -> Self {
        Self {
            pos: at,
            yaw: 0.0,
            lift: 0.0,
            step: Step::Standing,
            step_t: 0.0,
            target: None,
            posture: Posture::Stand,
            seat: None,
            path: [[0; 2]; MAX_PATH],
            path_len: 0,
            path_i: 0,
            chair: None,
            chair_k: 0.0,
            walk_clip: clip::HAPPY_WALK,
            cheerful: false,
            seed: 0xC0FFEE,
        }
    }

    /// Velocidad de la caminata actual (Q8 por segundo): la que avanza el
    /// clip en un ciclo, así los pies no patinan.
    fn walk_speed(&self) -> f32 {
        let w = &CLIPS[self.walk_clip];
        let a = w.advance;
        if a == [0; 3] {
            return WALKING_SPEED;
        }
        let (x, z) = (a[0] as f32, a[2] as f32);
        sqrtf(x * x + z * z) / w.duration_ms() as f32 * 1000.0
    }

    fn walk(&self) -> &'static assets::anim::Clip {
        &CLIPS[self.walk_clip]
    }

    /// Lo pone directamente haciendo `plan` (para fotos y al arrancar).
    pub fn teleport(&mut self, plan: &Plan, anim: &mut Animator) {
        self.target = Some(plan.target);
        self.posture = plan.posture;
        self.seat = seat_for(plan.target, plan.posture);
        self.chair = None;
        self.chair_k = 0.0;
        match (plan.posture, plan.target) {
            (Posture::Lie, Target::Spot(s)) => {
                let sp = spot(s);
                self.pos = xz(sp);
                self.lift = sp.pos[1] as f32;
                self.yaw = sp.yaw as f32;
                self.step = Step::Lying;
            }
            (Posture::Sit, _) => {
                let s = self.seat.unwrap();
                self.pos = s.seat;
                self.lift = s.lift;
                self.yaw = s.yaw;
                self.step = Step::Seated;
            }
            _ => {
                self.pos = target_point(plan.target);
                self.lift = 0.0;
                if let Target::Spot(s) = plan.target {
                    self.yaw = spot(s).yaw as f32;
                }
                self.step = Step::Standing;
            }
        }
        self.step_t = 0.0;
        *anim = Animator::new(&CLIPS[plan.clip]);
    }

    /// ¿Ya está haciendo la actividad (no yendo ni acomodándose)?
    pub fn settled(&self) -> bool {
        matches!(self.step, Step::Seated | Step::Lying | Step::Standing)
    }

    pub fn is_walking(&self) -> bool {
        matches!(self.step, Step::Walk | Step::StepIn | Step::StepOut)
    }

    /// ¿Está sentado o acostado haciendo la actividad (para reacciones)?
    pub fn seated(&self) -> bool {
        matches!(self.step, Step::Seated | Step::Lying | Step::SlideIn)
    }

    pub fn target(&self) -> Option<Target> {
        self.target
    }

    /// Dónde dibujarlo ahora.
    pub fn placement(&self) -> Placement {
        let mut p = Placement {
            pos: self.pos,
            lift: self.lift,
            yaw: self.yaw,
        };
        if self.step == Step::SitDown {
            let d = rotate(sit_shift(), self.yaw);
            p.pos = [p.pos[0] + d[0], p.pos[1] + d[1]];
        }
        p
    }

    fn set_step(&mut self, step: Step) {
        self.step = step;
        self.step_t = 0.0;
    }

    fn turn_to(&mut self, target: f32, max_step: f32) {
        let d = wrap_deg(target - self.yaw);
        self.yaw = wrap_deg(self.yaw + d.clamp(-max_step, max_step));
    }

    /// Avanza hacia `to` hasta `step` Q8; devuelve lo que sobró (> 0 si llegó).
    fn move_toward(&mut self, to: [f32; 2], step: f32, turn: f32) -> f32 {
        let d = dist(self.pos, to);
        if d <= step {
            self.pos = to;
            return step - d + f32::MIN_POSITIVE;
        }
        let dir = [to[0] - self.pos[0], to[1] - self.pos[1]];
        self.pos[0] += dir[0] * step / d;
        self.pos[1] += dir[1] * step / d;
        self.turn_to(heading(dir[0], dir[1]), turn);
        0.0
    }

    /// Sale caminando hacia `plan` desde donde está.
    fn start_walk(&mut self, plan: &Plan, anim: &mut Animator) {
        self.target = Some(plan.target);
        self.posture = plan.posture;
        self.seat = seat_for(plan.target, plan.posture);
        let goal = match self.seat {
            Some(s) => s.approach,
            None => target_point(plan.target),
        };
        let from = [self.pos[0] as i32, self.pos[1] as i32];
        let to = [goal[0] as i32, goal[1] as i32];
        self.path_len = HOUSE.nav.path(from, to, &mut self.path);
        if self.path_len == 0 {
            // Sin camino (no debería pasar): directo.
            self.path[0] = to;
            self.path_len = 1;
        }
        self.path_i = 0;
        self.lift = 0.0;
        // No camina siempre igual: de buen humor, casi siempre a los saltitos.
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let happy_pct = if self.cheerful { 75 } else { 30 };
        self.walk_clip = if (self.seed >> 16) % 100 < happy_pct {
            clip::HAPPY_WALK
        } else {
            clip::WALKING
        };
        self.set_step(Step::Walk);
        anim.play(self.walk(), 300);
    }

    /// Llegó al final del camino.
    fn arrive(&mut self, plan: &Plan, anim: &mut Animator) {
        match self.seat {
            Some(s) => {
                if let Some(chair) = s.chair {
                    self.chair = Some((chair, s.back));
                    self.set_step(Step::PullChair);
                    anim.play(&CLIPS[clip::IDLE], 250);
                } else {
                    self.set_step(Step::StepIn);
                }
            }
            None => {
                self.set_step(Step::Standing);
                anim.play(&CLIPS[plan.clip], 350);
            }
        }
    }

    /// Un paso de simulación de `dt` segundos hacia `plan`.
    pub fn update(&mut self, plan: &Plan, dt: f32, anim: &mut Animator, world: &mut WorldScene) {
        self.step_t += dt;
        let changed = self.target != Some(plan.target) || self.posture != plan.posture;
        let walk_speed = self.walk_speed();

        match self.step {
            Step::Walk => {
                if changed {
                    self.start_walk(plan, anim);
                }
                let mut step = walk_speed * dt;
                while step > 0.0 && self.path_i < self.path_len {
                    let p = self.path[self.path_i];
                    let left = self.move_toward([p[0] as f32, p[1] as f32], step, TURN * dt);
                    if left > 0.0 {
                        self.path_i += 1;
                        step = left;
                    } else {
                        step = 0.0;
                    }
                }
                anim.play(self.walk(), 250);
                if self.path_i >= self.path_len {
                    self.arrive(plan, anim);
                }
            }
            Step::PullChair => {
                let s = self.seat.unwrap();
                let d = [s.stand[0] - self.pos[0], s.stand[1] - self.pos[1]];
                self.turn_to(heading(d[0], d[1]), TURN * dt);
                self.chair_k = (self.chair_k + dt / PULL_S).min(1.0);
                if changed {
                    self.start_walk(plan, anim);
                } else if self.chair_k >= 1.0 {
                    self.set_step(Step::StepIn);
                }
            }
            Step::StepIn => {
                let s = self.seat.unwrap();
                anim.play(self.walk(), 200);
                if changed {
                    self.set_step(Step::StepOut);
                } else if self.move_toward(s.stand, STEP_SPEED * dt, TURN * dt) > 0.0 {
                    // Delante del asiento: girar y sentarse.
                    self.turn_to(s.yaw, TURN * dt);
                    if wrap_deg(s.yaw - self.yaw).abs() < 5.0 {
                        self.pos = s.out;
                        self.yaw = s.yaw;
                        self.set_step(Step::SitDown);
                        anim.play(&CLIPS[clip::STAND_TO_SIT], 200);
                    }
                }
            }
            Step::SitDown => {
                let s = self.seat.unwrap();
                let dur = CLIPS[clip::STAND_TO_SIT].duration_ms() as f32 / 1000.0;
                self.lift = s.lift * (self.step_t / dur).min(1.0);
                if anim.finished() || self.step_t > dur + 0.3 {
                    self.lift = s.lift;
                    match (self.posture, s.chair) {
                        (Posture::Lie, _) => {
                            self.set_step(Step::LieDown);
                            anim.play(&CLIPS[clip::LIE_DOWN_SIDE], 250);
                        }
                        (_, Some(_)) => {
                            self.set_step(Step::SlideIn);
                            anim.play(&CLIPS[plan.clip], 300);
                        }
                        (_, None) => {
                            self.set_step(Step::Seated);
                            anim.play(&CLIPS[plan.clip], 300);
                        }
                    }
                }
            }
            Step::SlideIn => {
                let s = self.seat.unwrap();
                self.chair_k = (self.chair_k - dt / PULL_S).max(0.0);
                self.pos = lerp2(s.seat, s.out, self.chair_k);
                anim.play(&CLIPS[plan.clip], 300);
                if changed {
                    self.set_step(Step::SlideOut);
                } else if self.chair_k <= 0.0 {
                    self.set_step(Step::Seated);
                }
            }
            Step::Seated => {
                let s = self.seat.unwrap();
                self.turn_to(s.yaw, TURN * dt);
                if changed {
                    if let Some(chair) = s.chair {
                        self.chair = Some((chair, s.back));
                        self.set_step(Step::SlideOut);
                    } else {
                        self.set_step(Step::StandUp);
                        anim.play(&CLIPS[clip::SIT_TO_STAND], 200);
                    }
                } else {
                    anim.play(&CLIPS[plan.clip], 400);
                }
            }
            Step::SlideOut => {
                let s = self.seat.unwrap();
                self.chair_k = (self.chair_k + dt / PULL_S).min(1.0);
                self.pos = lerp2(s.seat, s.out, self.chair_k);
                anim.play(&CLIPS[clip::SITTING_IDLE], 300);
                if self.chair_k >= 1.0 {
                    self.set_step(Step::StandUp);
                    anim.play(&CLIPS[clip::SIT_TO_STAND], 200);
                }
            }
            Step::StandUp => {
                let s = self.seat.unwrap();
                let dur = CLIPS[clip::SIT_TO_STAND].duration_ms() as f32 / 1000.0;
                self.lift = s.lift * (1.0 - self.step_t / dur).max(0.0);
                if anim.finished() || self.step_t > dur + 0.3 {
                    // Termina parado delante del asiento.
                    self.pos = s.stand;
                    self.lift = 0.0;
                    self.set_step(Step::StepOut);
                    anim.play(self.walk(), 200);
                }
            }
            Step::StepOut => {
                let s = self.seat.unwrap();
                anim.play(self.walk(), 200);
                if self.move_toward(s.approach, STEP_SPEED * dt, TURN * dt) > 0.0 {
                    if changed {
                        self.start_walk(plan, anim);
                    } else {
                        // Volvió a querer sentarse ahí mismo.
                        self.arrive(plan, anim);
                    }
                }
            }
            Step::LieDown => {
                // `lie_down_side` termina en el primer cuadro de dormir, ya
                // en la cama: se pasa al lugar de la cama sin que se note.
                let dur = CLIPS[clip::LIE_DOWN_SIDE].duration_ms() as f32 / 1000.0;
                if anim.finished() || self.step_t > dur + 0.3 {
                    if let Target::Spot(name) = plan.target {
                        let sp = spot(name);
                        self.pos = xz(sp);
                        self.lift = sp.pos[1] as f32;
                        self.yaw = sp.yaw as f32;
                    }
                    self.set_step(Step::Lying);
                    anim.play(&CLIPS[plan.clip], 0);
                }
            }
            Step::Lying => {
                if changed {
                    // Baja las piernas y se sienta en el costado (la
                    // animación va relativa a la cama).
                    self.seat = Some(resolve_seat(BED_SIDE));
                    self.set_step(Step::SitUp);
                    anim.play(&CLIPS[clip::GET_UP_SIDE], 450);
                } else {
                    anim.play(&CLIPS[plan.clip], 400);
                }
            }
            Step::SitUp => {
                let dur = CLIPS[clip::GET_UP_SIDE].duration_ms() as f32 / 1000.0;
                if anim.finished() || self.step_t > dur + 0.3 {
                    // Ya sentado en el costado: al lugar del asiento.
                    let s = self.seat.unwrap();
                    self.pos = s.seat;
                    self.lift = s.lift;
                    self.yaw = s.yaw;
                    self.set_step(Step::StandUp);
                    // `get_up_side` va relativa a la cama: no se funde con
                    // ella en el lugar nuevo. Termina en la pose de
                    // `sitting_idle`, que sí se funde con pararse.
                    anim.play(&CLIPS[clip::SITTING_IDLE], 0);
                    anim.play(&CLIPS[clip::SIT_TO_STAND], 200);
                }
            }
            Step::Standing => {
                if changed {
                    self.start_walk(plan, anim);
                } else {
                    if let Target::Spot(s) = plan.target {
                        self.turn_to(spot(s).yaw as f32, TURN * dt);
                    }
                    anim.play(&CLIPS[plan.clip], 400);
                }
            }
        }

        // La silla vuelve a su lugar cuando ya se alejó de ella.
        let holding = matches!(
            self.step,
            Step::PullChair
                | Step::StepIn
                | Step::SitDown
                | Step::SlideIn
                | Step::Seated
                | Step::SlideOut
                | Step::StandUp
                | Step::StepOut
        );
        if !holding && self.chair_k > 0.0 {
            self.chair_k = (self.chair_k - dt / PULL_S).max(0.0);
        }
        if let Some((name, back)) = self.chair {
            let k = ease(self.chair_k);
            world.set_offset(name, [(back[0] * k) as i32, 0, (back[1] * k) as i32]);
            if self.chair_k <= 0.0 && !holding {
                self.chair = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::plan;
    use companion::Activity;

    fn world() -> WorldScene {
        WorldScene::new(&HOUSE, 0)
    }

    /// Lo lleva de `from` a hacer `act` y devuelve los pasos por los que pasó.
    fn run(from: &str, act: Activity, secs: f32) -> (Actor, std::vec::Vec<Step>) {
        let mut a = Actor::new(xz(spot(from)));
        let mut anim = Animator::new(&CLIPS[clip::IDLE]);
        let mut w = world();
        let p = plan(act, 0.0);
        let mut steps = std::vec::Vec::new();
        let mut t = 0.0;
        while t < secs {
            a.update(&p, 1.0 / 30.0, &mut anim, &mut w);
            anim.update(33);

            if steps.last() != Some(&a.step) {
                steps.push(a.step);
            }
            t += 1.0 / 30.0;
        }
        (a, steps)
    }

    #[test]
    fn walks_to_the_table_pulls_the_chair_and_sits() {
        let (a, steps) = run("wp_bedroom", Activity::Eat, 40.0);

        assert_eq!(a.step, Step::Seated, "{steps:?} en {:?}", a.pos);
        assert_eq!(
            steps,
            [
                Step::Walk,
                Step::PullChair,
                Step::StepIn,
                Step::SitDown,
                Step::SlideIn,
                Step::Seated
            ]
        );
        let s = resolve_seat("spot_table");
        assert!(dist(a.pos, s.seat) < 1.0);
    }

    #[test]
    fn a_whole_day_of_moves_never_walks_through_furniture() {
        let mut a = Actor::new(xz(spot("wp_kitchen")));
        let mut anim = Animator::new(&CLIPS[clip::IDLE]);
        let mut w = world();
        let nav = HOUSE.nav;
        // Todas las actividades, y algunas otra vez desde otro lado.
        let mut day = std::vec::Vec::from(Activity::ALL);
        day.extend([
            Activity::Eat,
            Activity::Guitar,
            Activity::Code,
            Activity::Sleep,
            Activity::Stargaze,
            Activity::Cook,
            Activity::Trading,
        ]);
        for act in day {
            let p = plan(act, 0.0);
            let start = a.pos;
            let mut settled_for = 0.0;
            let mut t = 0.0;
            while settled_for < 1.0 {
                a.update(&p, 1.0 / 30.0, &mut anim, &mut w);
                anim.update(33);
                t += 1.0 / 30.0;
                assert!(t < 60.0, "{act:?}: no llegó ({:?} en {:?})", a.step, a.pos);
                if a.settled() {
                    settled_for += 1.0 / 30.0;
                }
                if a.step == Step::Walk {
                    // Caminando, siempre por lugares libres (salvo al salir
                    // de un lugar pegado a un mueble o al llegar a uno).
                    let end = [
                        a.path[a.path_len - 1][0] as f32,
                        a.path[a.path_len - 1][1] as f32,
                    ];
                    let near_ends = dist(a.pos, start) < 0.4 * M || dist(a.pos, end) < 0.4 * M;
                    // Se tolera rozar el borde agrandado (2,5 cm de 17).
                    let p = [a.pos[0] as i32, a.pos[1] as i32];
                    let grazing = [[25, 0], [-25, 0], [0, 25], [0, -25]]
                        .iter()
                        .any(|d| nav.is_free([p[0] + d[0], p[1] + d[1]]));
                    assert!(
                        near_ends || nav.is_free(p) || grazing,
                        "{act:?}: atraviesa un mueble en {:?}, camino {:?} desde {start:?}",
                        a.pos,
                        &a.path[..a.path_len]
                    );
                }
            }
            // La silla, de vuelta en su lugar si no está sentado a la mesa.
            if !matches!(act, Activity::Eat | Activity::Snack) && !act.is_work() {
                assert!(
                    a.chair.is_none() || a.chair_k == 0.0,
                    "{act:?}: silla corrida"
                );
            }
        }
    }
}
