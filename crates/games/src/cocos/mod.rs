//! Cocos Rodantes: el mono trepa una torre de andamios de la selva para
//! recuperar su racimo de bananas. Arriba, un tucán cascarrabias le tira cocos
//! que ruedan por las tablas inclinadas y a veces caen por las escaleras.
//!
//! Controles: A = izquierda, C = derecha, B = saltar. Parado al pie de una
//! escalera, B la empieza a trepar; en la escalera, mantener B sube y A o C
//! bajan. Saltar un coco da 100 puntos; cada banana suelta, 200; llegar arriba,
//! 1000 más lo que quede del bono. Tres vidas.
//!
//! La física corre en pasos fijos de [`STEP_MS`] con posiciones en 1/256 de
//! pixel: con la misma semilla y los mismos botones, la partida es idéntica
//! aunque cambie el `dt` de cada llamada.

mod art;
mod draw;
#[cfg(test)]
mod tests;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;

use crate::{Keys, Status};

/// Paso fijo de la física.
pub const STEP_MS: u32 = 5;
/// Un `dt` más grande se recorta (la placa se trabó, el simulador se pausó).
const MAX_DT: u32 = 100;
/// Punto fijo: 1 pixel = 256.
const FP: i32 = 256;

// Velocidades en pixeles por segundo (aceleraciones en px/s²).
const WALK: i32 = 55 * FP;
const CLIMB: i32 = 40 * FP;
const JUMP_V: i32 = 130 * FP;
const GRAVITY: i32 = 520 * FP;
const COCO_LADDER_V: i32 = 45 * FP;

/// Un toque de B poco antes de aterrizar igual salta.
const JUMP_BUFFER_MS: u32 = 120;
/// Y se puede saltar un instante después de salirse de una tabla.
const COYOTE_MS: u32 = 80;
/// Soltar A o C hasta tanto antes de saltar cuenta como salto con impulso.
const DIR_GRACE_MS: u32 = 150;
/// Qué tan cerca del pie de una escalera hay que estar para treparla.
const LADDER_GRAB: i32 = 5 * FP;

const X_MIN: i32 = 6 * FP;
const X_MAX: i32 = 234 * FP;

/// Plataformas: 0 es el piso y la última, la del tucán.
pub(crate) const PLATS: usize = 6;
const TOP: usize = PLATS - 1;
const LADDERS: usize = 8;
const BANANAS: usize = 4;
const MAX_COCOS: usize = 12;
const MAX_POPUPS: usize = 4;

const LIVES: u8 = 3;
const BONUS_START: u32 = 3000;
const BONUS_TICK_MS: u32 = 2000;
const DYING_MS: u32 = 1600;
const CLEAR_MS: u32 = 2600;
/// En la pantalla final, B y C se ignoran un rato para no saltearla sin querer.
const OVER_GRACE_MS: u32 = 700;

pub(crate) const PTS_JUMP: u32 = 100;
pub(crate) const PTS_BANANA: u32 = 200;
pub(crate) const PTS_CLEAR: u32 = 1000;

/// Una tabla inclinada: la superficie va de `(x0, y0)` a `(x1, y1)`, en
/// pixeles. `dir` es hacia dónde ruedan los cocos (-1 izquierda, 1 derecha).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Plat {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    pub dir: i32,
}

impl Plat {
    /// ¿`x` (en 1/256 px) está sobre la tabla?
    fn contains(&self, x: i32) -> bool {
        x >= self.x0 * FP && x <= self.x1 * FP
    }

    /// Altura de la superficie en `x` (ambos en 1/256 px).
    pub fn surf(&self, x: i32) -> i32 {
        let x = x.clamp(self.x0 * FP, self.x1 * FP);
        self.y0 * FP + (self.y1 - self.y0) * (x - self.x0 * FP) / (self.x1 - self.x0)
    }
}

/// Una escalera de bambú de la tabla `lo` a la `lo + 1`. Las rotas no se
/// pueden trepar, pero los cocos igual caen por ellas.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ladder {
    pub x: i32,
    pub lo: usize,
    pub broken: bool,
}

/// La torre de un nivel. Los niveles pares son el espejo de los impares.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Level {
    pub plats: [Plat; PLATS],
    pub ladders: [Ladder; LADDERS],
    /// Bananas sueltas: `x` y tabla.
    pub bananas: [(i32, usize); BANANAS],
    pub toucan_x: i32,
    pub bunch_x: i32,
    pub start_x: i32,
    pub mirror: bool,
}

// La torre del nivel 1. Las tablas bajan hacia el hueco por donde caen los
// cocos, alternando: arriba hacia la derecha, después hacia la izquierda...
#[rustfmt::skip]
const BASE_PLATS: [Plat; PLATS] = [
    Plat { x0: 0,  y0: 228, x1: 240, y1: 228, dir: -1 }, // piso
    Plat { x0: 24, y0: 196, x1: 240, y1: 188, dir: -1 },
    Plat { x0: 0,  y0: 152, x1: 216, y1: 160, dir: 1 },
    Plat { x0: 24, y0: 124, x1: 240, y1: 116, dir: -1 },
    Plat { x0: 0,  y0: 80,  x1: 216, y1: 88,  dir: 1 },
    Plat { x0: 8,  y0: 48,  x1: 96,  y1: 48,  dir: 1 },  // la del tucán
];

// Las escaleras del camino quedan en la punta contraria a la anterior: hay
// que cruzar cada tabla contra los cocos.
#[rustfmt::skip]
const BASE_LADDERS: [Ladder; LADDERS] = [
    Ladder { x: 64,  lo: 0, broken: false },
    Ladder { x: 192, lo: 1, broken: false },
    Ladder { x: 112, lo: 1, broken: true },
    Ladder { x: 48,  lo: 2, broken: false },
    Ladder { x: 140, lo: 2, broken: true },
    Ladder { x: 192, lo: 3, broken: false },
    Ladder { x: 100, lo: 3, broken: true },
    Ladder { x: 80,  lo: 4, broken: false },
];

/// Bananas sueltas: `x` y tabla.
const BASE_BANANAS: [(i32, usize); BANANAS] = [(140, 1), (20, 2), (120, 3), (140, 4)];

impl Level {
    pub fn build(mirror: bool) -> Level {
        let fx = |x: i32| if mirror { 240 - x } else { x };
        let mut plats = BASE_PLATS;
        let mut ladders = BASE_LADDERS;
        let mut bananas = BASE_BANANAS;
        if mirror {
            for p in &mut plats {
                *p = Plat {
                    x0: 240 - p.x1,
                    y0: p.y1,
                    x1: 240 - p.x0,
                    y1: p.y0,
                    dir: -p.dir,
                };
            }
            for l in &mut ladders {
                l.x = fx(l.x);
            }
            for b in &mut bananas {
                b.0 = fx(b.0);
            }
        }
        Level {
            plats,
            ladders,
            bananas,
            toucan_x: fx(26),
            bunch_x: fx(56),
            start_x: fx(210),
            mirror,
        }
    }

    fn top_of(&self, l: &Ladder) -> i32 {
        self.plats[l.lo + 1].surf(l.x * FP)
    }

    fn bottom_of(&self, l: &Ladder) -> i32 {
        self.plats[l.lo].surf(l.x * FP)
    }

    /// La tabla donde cae algo que bajó de `old_y` a `new_y` en `x`.
    fn landing(&self, x: i32, old_y: i32, new_y: i32) -> Option<usize> {
        (0..PLATS).rev().find(|&i| {
            let p = &self.plats[i];
            if !p.contains(x) {
                return false;
            }
            let s = p.surf(x);
            old_y <= s && new_y >= s
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Title,
    Play,
    Dying,
    Clear,
    Over,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MState {
    Ground,
    Air,
    Climb,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Monkey {
    /// Centro de los pies, en 1/256 px.
    pub x: i32,
    pub y: i32,
    pub vy: i32,
    pub state: MState,
    /// Tabla donde está parado (o la última que pisó).
    pub plat: usize,
    pub ladder: usize,
    /// 1 mira a la derecha, -1 a la izquierda.
    pub face: i32,
    /// Para animar: milisegundos caminando o trepando.
    pub walk_ms: u32,
    pub moving: bool,
    /// Hacia dónde va en el aire: el impulso del salto se mantiene aunque se
    /// suelte A o C (con tres botones cuesta apretar dos a la vez).
    pub air_dir: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CState {
    Roll,
    Fall,
    Ladder,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Coco {
    pub alive: bool,
    /// Centro de abajo (donde toca la tabla), en 1/256 px.
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    pub state: CState,
    pub plat: usize,
    pub ladder: usize,
    /// La última escalera por la que ya se decidió si bajar o no.
    pub last_ladder: usize,
    /// Ya dio puntos por saltarlo.
    pub scored: bool,
    /// Pixeles rodados, para girar el dibujo.
    pub spin: u32,
    /// Antes de caer por una escalera se tambalea un rato en el borde: el
    /// aviso para el que está trepando.
    pub teeter_ms: u32,
}

const NO_LADDER: usize = usize::MAX;

impl Coco {
    const DEAD: Coco = Coco {
        alive: false,
        x: 0,
        y: 0,
        vx: 0,
        vy: 0,
        state: CState::Roll,
        plat: 0,
        ladder: 0,
        last_ladder: NO_LADDER,
        scored: false,
        spin: 0,
        teeter_ms: 0,
    };
}

/// Un "+100" que sube y se borra.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Popup {
    pub x: i32,
    pub y: i32,
    pub pts: u32,
    pub ms: u32,
}

const POPUP_MS: u32 = 900;

/// El juego entero. Ocupa unos pocos cientos de bytes.
pub struct Cocos {
    rng: u32,
    pub(crate) phase: Phase,
    pub(crate) phase_ms: u32,
    prev: Keys,
    keys: Keys,
    acc: u32,
    /// Milisegundos desde que se creó, para animaciones.
    pub(crate) t: u32,
    pub(crate) score: u32,
    pub(crate) best: u32,
    pub(crate) new_best: bool,
    pub(crate) lives: u8,
    pub(crate) level: u32,
    pub(crate) bonus: u32,
    bonus_ms: u32,
    pub(crate) lv: Level,
    pub(crate) taken: [bool; BANANAS],
    pub(crate) m: Monkey,
    pub(crate) cocos: [Coco; MAX_COCOS],
    /// Cuánto falta para el próximo coco.
    pub(crate) spawn_ms: i32,
    pub(crate) popups: [Option<Popup>; MAX_POPUPS],
    jump_buf: u32,
    coyote: u32,
    /// Última dirección apretada y hace cuánto se soltó.
    last_dir: i32,
    dir_ms: u32,
}

impl Cocos {
    pub fn new(seed: u32) -> Self {
        let lv = Level::build(false);
        let mut g = Cocos {
            rng: seed ^ 0x9E37_79B9,
            phase: Phase::Title,
            phase_ms: 0,
            // Como si todo estuviera apretado: un botón que venía apretado
            // desde el menú no cuenta hasta soltarlo.
            prev: Keys {
                a: true,
                b: true,
                c: true,
            },
            keys: Keys::default(),
            acc: 0,
            t: 0,
            score: 0,
            best: 0,
            new_best: false,
            lives: LIVES,
            level: 1,
            bonus: BONUS_START,
            bonus_ms: 0,
            lv,
            taken: [false; BANANAS],
            m: Monkey {
                x: 0,
                y: 0,
                vy: 0,
                state: MState::Ground,
                plat: 0,
                ladder: 0,
                face: -1,
                walk_ms: 0,
                moving: false,
                air_dir: 0,
            },
            cocos: [Coco::DEAD; MAX_COCOS],
            spawn_ms: 0,
            popups: [None; MAX_POPUPS],
            jump_buf: 0,
            coyote: 0,
            last_dir: 0,
            dir_ms: u32::MAX,
        };
        if g.rng == 0 {
            g.rng = 0x1234_5678;
        }
        g.reset_actors();
        g
    }

    pub fn update(&mut self, keys: Keys, dt_ms: u32) -> Status {
        let dt = dt_ms.min(MAX_DT);
        self.t = self.t.wrapping_add(dt);
        let pressed_b = keys.b && !self.prev.b;
        let pressed_c = keys.c && !self.prev.c;
        self.prev = keys;
        self.keys = keys;
        match self.phase {
            Phase::Title => {
                if pressed_b {
                    return Status::Exit;
                }
                if pressed_c {
                    self.start_game();
                }
            }
            Phase::Over => {
                self.phase_ms = self.phase_ms.saturating_add(dt);
                if self.phase_ms >= OVER_GRACE_MS {
                    if pressed_b {
                        return Status::Exit;
                    }
                    if pressed_c {
                        self.start_game();
                    }
                }
            }
            Phase::Play | Phase::Dying | Phase::Clear => {
                if pressed_b {
                    self.jump_buf = JUMP_BUFFER_MS;
                }
                self.acc += dt;
                while self.acc >= STEP_MS {
                    self.acc -= STEP_MS;
                    self.step();
                }
            }
        }
        Status::Playing
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(&self, d: &mut D) -> Result<(), D::Error> {
        draw::draw(self, d)
    }

    fn rand(&mut self) -> u32 {
        // xorshift32
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn start_game(&mut self) {
        self.score = 0;
        self.new_best = false;
        self.lives = LIVES;
        self.level = 1;
        self.start_level();
    }

    fn start_level(&mut self) {
        self.lv = Level::build(self.level.is_multiple_of(2));
        self.taken = [false; BANANAS];
        self.reset_actors();
        self.phase = Phase::Play;
        self.phase_ms = 0;
    }

    /// Mono al principio, sin cocos, bono lleno.
    fn reset_actors(&mut self) {
        let x = self.lv.start_x * FP;
        self.m = Monkey {
            x,
            y: self.lv.plats[0].surf(x),
            vy: 0,
            state: MState::Ground,
            plat: 0,
            ladder: 0,
            face: if self.lv.mirror { 1 } else { -1 },
            walk_ms: 0,
            moving: false,
            air_dir: 0,
        };
        self.cocos = [Coco::DEAD; MAX_COCOS];
        self.popups = [None; MAX_POPUPS];
        self.spawn_ms = 1200;
        self.bonus = BONUS_START;
        self.bonus_ms = 0;
        self.jump_buf = 0;
        self.coyote = 0;
        self.acc = 0;
    }

    // Lo que cambia con el nivel.

    pub(crate) fn coco_speed(&self) -> i32 {
        (42 + 6 * (self.level as i32 - 1)).min(84) * FP
    }

    fn spawn_interval(&mut self) -> i32 {
        let base = (3000 - 250 * (self.level as i32 - 1)).max(1100);
        base - 400 + (self.rand() % 800) as i32
    }

    fn ladder_chance(&self) -> u32 {
        (18 + 6 * (self.level - 1)).min(55)
    }

    fn teeter_ms(&self) -> u32 {
        400u32.saturating_sub(50 * (self.level - 1)).max(150)
    }

    fn step(&mut self) {
        self.phase_ms = self.phase_ms.saturating_add(STEP_MS);
        for p in self.popups.iter_mut() {
            if let Some(pp) = p {
                pp.ms += STEP_MS;
                if pp.ms >= POPUP_MS {
                    *p = None;
                }
            }
        }
        match self.phase {
            Phase::Play => self.step_play(),
            Phase::Dying => {
                if self.phase_ms >= DYING_MS {
                    if self.lives == 0 {
                        self.game_over();
                    } else {
                        self.reset_actors();
                        self.phase = Phase::Play;
                        self.phase_ms = 0;
                    }
                }
            }
            Phase::Clear => {
                if self.phase_ms >= CLEAR_MS {
                    self.level += 1;
                    self.start_level();
                }
            }
            Phase::Title | Phase::Over => {}
        }
    }

    fn game_over(&mut self) {
        self.new_best = self.score > self.best;
        self.best = self.best.max(self.score);
        self.phase = Phase::Over;
        self.phase_ms = 0;
    }

    fn step_play(&mut self) {
        self.bonus_ms += STEP_MS;
        if self.bonus_ms >= BONUS_TICK_MS {
            self.bonus_ms -= BONUS_TICK_MS;
            self.bonus = self.bonus.saturating_sub(100);
        }
        self.step_monkey();
        if self.phase != Phase::Play {
            return;
        }
        self.step_spawn();
        self.step_cocos();
        self.collide();
    }

    /// La escalera trepable a cuyo pie está parado el mono.
    pub(crate) fn ladder_here(&self) -> Option<usize> {
        if self.m.state != MState::Ground {
            return None;
        }
        self.lv.ladders.iter().position(|l| {
            !l.broken && l.lo == self.m.plat && (self.m.x - l.x * FP).abs() <= LADDER_GRAB
        })
    }

    fn jump(&mut self) {
        // Si soltó A o C recién para apretar B, el salto igual va para ese lado.
        self.m.air_dir = if self.dir_ms <= DIR_GRACE_MS {
            self.last_dir
        } else {
            0
        };
        self.m.vy = -JUMP_V;
        self.m.state = MState::Air;
        self.jump_buf = 0;
        self.coyote = 0;
    }

    fn step_monkey(&mut self) {
        let k = self.keys;
        let dir = k.c as i32 - k.a as i32;
        if dir != 0 {
            self.last_dir = dir;
            self.dir_ms = 0;
        } else {
            self.dir_ms = self.dir_ms.saturating_add(STEP_MS);
        }
        self.jump_buf = self.jump_buf.saturating_sub(STEP_MS);
        let step = |v: i32| v * STEP_MS as i32 / 1000;
        let m = &mut self.m;
        m.moving = false;
        match m.state {
            MState::Ground => {
                // B al pie de una escalera (o mantenerlo parado ahí) trepa.
                if let Some(li) = self.ladder_here()
                    && (self.jump_buf > 0 || (k.b && dir == 0))
                {
                    self.jump_buf = 0;
                    let m = &mut self.m;
                    m.state = MState::Climb;
                    m.ladder = li;
                    m.x = self.lv.ladders[li].x * FP;
                    return;
                }
                if self.jump_buf > 0 {
                    self.jump();
                    return;
                }
                let m = &mut self.m;
                if dir != 0 {
                    m.face = dir;
                    m.moving = true;
                    m.walk_ms += STEP_MS;
                }
                m.x = (m.x + step(dir * WALK)).clamp(X_MIN, X_MAX);
                let p = &self.lv.plats[m.plat];
                if p.contains(m.x) {
                    m.y = p.surf(m.x);
                } else {
                    m.state = MState::Air;
                    m.vy = 0;
                    m.air_dir = dir;
                    self.coyote = COYOTE_MS;
                }
            }
            MState::Air => {
                if self.coyote > 0 {
                    self.coyote = self.coyote.saturating_sub(STEP_MS);
                    if self.jump_buf > 0 {
                        self.jump();
                    }
                }
                let m = &mut self.m;
                if dir != 0 {
                    m.face = dir;
                    m.air_dir = dir;
                }
                m.vy += step(GRAVITY);
                let old_y = m.y;
                m.x = (m.x + step(m.air_dir * WALK)).clamp(X_MIN, X_MAX);
                m.y += step(m.vy);
                if m.vy >= 0
                    && let Some(i) = self.lv.landing(m.x, old_y, m.y)
                {
                    m.y = self.lv.plats[i].surf(m.x);
                    m.plat = i;
                    m.state = MState::Ground;
                    m.vy = 0;
                }
            }
            MState::Climb => {
                let l = self.lv.ladders[m.ladder];
                let (top, bottom) = (self.lv.top_of(&l), self.lv.bottom_of(&l));
                if k.b {
                    m.y -= step(CLIMB);
                    m.moving = true;
                } else if k.a || k.c {
                    m.y += step(CLIMB);
                    m.moving = true;
                }
                if m.moving {
                    m.walk_ms += STEP_MS;
                }
                if m.y <= top {
                    m.y = top;
                    m.plat = l.lo + 1;
                    m.state = MState::Ground;
                    if m.plat == TOP {
                        self.level_clear();
                    }
                } else if m.y >= bottom {
                    m.y = bottom;
                    m.plat = l.lo;
                    m.state = MState::Ground;
                }
            }
        }
    }

    fn level_clear(&mut self) {
        // El cartel muestra los puntos; no hacen falta los "+" flotando.
        self.score = self.score.saturating_add(PTS_CLEAR + self.bonus);
        self.popups = [None; MAX_POPUPS];
        self.phase = Phase::Clear;
        self.phase_ms = 0;
    }

    fn add_points(&mut self, pts: u32, x: i32, y: i32) {
        self.score = self.score.saturating_add(pts);
        if pts == 0 {
            return;
        }
        // Reemplaza el más viejo si no hay lugar.
        let slot = (0..MAX_POPUPS)
            .max_by_key(|&i| self.popups[i].map_or(u32::MAX, |p| p.ms))
            .unwrap_or(0);
        self.popups[slot] = Some(Popup { x, y, pts, ms: 0 });
    }

    fn step_spawn(&mut self) {
        self.spawn_ms -= STEP_MS as i32;
        if self.spawn_ms > 0 {
            return;
        }
        self.spawn_ms = self.spawn_interval();
        if let Some(c) = self.cocos.iter_mut().find(|c| !c.alive) {
            let p = &self.lv.plats[TOP];
            let x = (self.lv.toucan_x + 14 * p.dir) * FP;
            *c = Coco {
                alive: true,
                x,
                y: p.surf(x),
                state: CState::Roll,
                plat: TOP,
                ..Coco::DEAD
            };
        }
    }

    fn step_cocos(&mut self) {
        let speed = self.coco_speed();
        let chance = self.ladder_chance();
        let teeter = self.teeter_ms();
        let step = |v: i32| v * STEP_MS as i32 / 1000;
        for i in 0..MAX_COCOS {
            let mut c = self.cocos[i];
            if !c.alive {
                continue;
            }
            match c.state {
                CState::Roll => {
                    let p = self.lv.plats[c.plat];
                    let old = c.x;
                    c.x += p.dir * step(speed);
                    c.spin = c.spin.wrapping_add(step(speed) as u32);
                    if !p.contains(c.x) {
                        c.state = CState::Fall;
                        c.vx = p.dir * speed / 2;
                        c.vy = 0;
                    } else {
                        c.y = p.surf(c.x);
                        for (li, l) in self.lv.ladders.iter().enumerate() {
                            let lx = l.x * FP;
                            let crossed = (old <= lx && c.x >= lx) || (old >= lx && c.x <= lx);
                            if l.lo + 1 == c.plat && crossed && c.last_ladder != li {
                                c.last_ladder = li;
                                if self.rand() % 100 < chance {
                                    c.state = CState::Ladder;
                                    c.ladder = li;
                                    c.x = lx;
                                    c.teeter_ms = teeter;
                                }
                                break;
                            }
                        }
                    }
                }
                CState::Fall => {
                    c.vy += step(GRAVITY);
                    let old_y = c.y;
                    c.x += step(c.vx);
                    c.y += step(c.vy);
                    c.spin = c.spin.wrapping_add(step(c.vx.abs()) as u32);
                    if let Some(pi) = self.lv.landing(c.x, old_y, c.y) {
                        c.y = self.lv.plats[pi].surf(c.x);
                        c.plat = pi;
                        c.state = CState::Roll;
                        c.last_ladder = NO_LADDER;
                    }
                }
                CState::Ladder => {
                    if c.teeter_ms > 0 {
                        c.teeter_ms = c.teeter_ms.saturating_sub(STEP_MS);
                    } else {
                        c.y += step(COCO_LADDER_V);
                    }
                    let l = self.lv.ladders[c.ladder];
                    let bottom = self.lv.bottom_of(&l);
                    if c.y >= bottom {
                        c.y = bottom;
                        c.plat = l.lo;
                        c.state = CState::Roll;
                        c.last_ladder = NO_LADDER;
                    }
                }
            }
            if c.x < -12 * FP || c.x > 252 * FP || c.y > 250 * FP {
                c.alive = false;
            }
            self.cocos[i] = c;
        }
    }

    fn collide(&mut self) {
        let (mx, my) = (self.m.x, self.m.y);
        for i in 0..MAX_COCOS {
            let c = self.cocos[i];
            if !c.alive {
                continue;
            }
            let dx = (c.x - mx).abs();
            // Cajas más chicas que el dibujo, para que se sienta justo: el
            // mono de 6×13 y el coco de 6×5 (se lo puede rozar un poco).
            let hit = dx < 6 * FP && c.y - 5 * FP < my && my - 13 * FP < c.y;
            if hit {
                self.phase = Phase::Dying;
                self.phase_ms = 0;
                self.lives = self.lives.saturating_sub(1);
                return;
            }
            if self.m.state == MState::Air
                && !c.scored
                && c.state == CState::Roll
                && c.plat == self.m.plat
                && dx < 10 * FP
                && c.y > my
                && c.y - my < 30 * FP
            {
                self.cocos[i].scored = true;
                self.add_points(PTS_JUMP, c.x / FP, c.y / FP - 18);
            }
        }
        for b in 0..BANANAS {
            if self.taken[b] {
                continue;
            }
            let (bx, bp) = self.lv.bananas[b];
            let by = self.lv.plats[bp].surf(bx * FP) - 6 * FP;
            if (mx - bx * FP).abs() < 9 * FP && (my - 7 * FP - by).abs() < 11 * FP {
                self.taken[b] = true;
                self.add_points(PTS_BANANA, bx, by / FP - 10);
            }
        }
    }
}
