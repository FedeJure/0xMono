//! Dibujo de Cocos Rodantes. Todo son rectángulos y sprites chicos, y cada
//! cosa se saltea si no toca la franja que se está dibujando.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use fonts::{BODY, SMALL, TITLE};

use core::fmt::Write;

use super::art::{self, pal};
use super::{CState, Cocos, FP, Level, MState, Phase, Plat, TOP};
use crate::gfx::{Line, panel, rect, rgb, sprite, text, text_c, touches};

const SKY: [Rgb565; 6] = [
    rgb(0xA6DCF2),
    rgb(0xB4E2F1),
    rgb(0xC3E8EE),
    rgb(0xD0EDE6),
    rgb(0xDBF1DC),
    rgb(0xE4F4D2),
];
const HILL: Rgb565 = rgb(0xA9D79A);
const HILL_LIGHT: Rgb565 = rgb(0xBDE2A8);
const CLOUD: Rgb565 = rgb(0xFFFFFF);
const GRASS: Rgb565 = rgb(0x5DA34A);
const GRASS_DARK: Rgb565 = rgb(0x3E7D2A);
const VINE: Rgb565 = rgb(0x4E9A3A);
const LEAF: Rgb565 = rgb(0x6CC04A);

const WOOD_TOP: Rgb565 = rgb(0xEDBB7E);
const WOOD: Rgb565 = rgb(0xC48749);
const WOOD_GRAIN: Rgb565 = rgb(0xA86D37);
const WOOD_DARK: Rgb565 = rgb(0x6E4220);
const ROPE: Rgb565 = rgb(0xEAD6A0);
const ROPE_DARK: Rgb565 = rgb(0xB89C5E);

const BAMBOO: Rgb565 = rgb(0x8CC152);
const BAMBOO_NODE: Rgb565 = rgb(0x5E9A35);
const RUNG: Rgb565 = rgb(0xB0D46A);

const HUD_BG: Rgb565 = rgb(0x5A3A1E);
const HUD_LINE: Rgb565 = rgb(0x3B2414);
const CREAM: Rgb565 = rgb(0xFFF3D6);
const INK: Rgb565 = rgb(0x3B2414);
const GOLD: Rgb565 = rgb(0xFFD23F);
const STAR: Rgb565 = rgb(0xFFE14D);
const RED: Rgb565 = rgb(0xD93A3A);

/// Altura de la barra de arriba.
const HUD_H: i32 = 18;
/// Grosor de las tablas.
const PLANK: i32 = 6;

pub fn draw<D: DrawTarget<Color = Rgb565>>(g: &Cocos, d: &mut D) -> Result<(), D::Error> {
    let vis = d.bounding_box();
    background(d, &vis)?;
    if g.phase == Phase::Title {
        rect(d, &vis, 0, 0, 240, HUD_H as u32, SKY[0])?;
        return title(g, d, &vis);
    }
    scene(g, d, &vis)?;
    hud(g, d, &vis)?;
    match g.phase {
        Phase::Play if g.phase_ms < 1400 => banner(g, d, &vis),
        Phase::Clear => cleared(g, d, &vis),
        Phase::Over => game_over(g, d, &vis),
        _ => Ok(()),
    }
}

/// Cielo en franjas, nubes, lomas y el pasto del piso.
fn background<D: DrawTarget<Color = Rgb565>>(d: &mut D, vis: &Rectangle) -> Result<(), D::Error> {
    let band = (240 - HUD_H) / SKY.len() as i32 + 1;
    for (i, c) in SKY.iter().enumerate() {
        rect(d, vis, 0, HUD_H + i as i32 * band, 240, band as u32, *c)?;
    }
    for (x, y) in [(150, 24), (196, 62), (30, 104), (176, 140)] {
        cloud(d, vis, x, y)?;
    }
    // Lomas de la selva, detrás del piso.
    for (cx, w, h) in [(20, 70, 26), (84, 56, 18), (150, 80, 30), (222, 60, 20)] {
        let x = cx - w / 2;
        rect(d, vis, x, 228 - h, w as u32, h as u32, HILL)?;
        rect(d, vis, x + 5, 228 - h - 3, (w - 10) as u32, 3, HILL)?;
        rect(d, vis, x + 12, 228 - h - 5, (w - 24) as u32, 2, HILL)?;
        rect(
            d,
            vis,
            x + 14,
            228 - h - 2,
            (w - 40).max(4) as u32,
            2,
            HILL_LIGHT,
        )?;
    }
    // Lianas que cuelgan de arriba.
    for (x, len) in [(110, 22), (160, 34), (230, 18)] {
        rect(d, vis, x, HUD_H, 1, len, VINE)?;
        for k in (4..len as i32).step_by(7) {
            let side = if (k / 7) % 2 == 0 { -3 } else { 1 };
            rect(d, vis, x + side, HUD_H + k, 3, 2, LEAF)?;
        }
    }
    rect(
        d,
        vis,
        0,
        228 + PLANK,
        240,
        240 - (228 + PLANK) as u32,
        GRASS,
    )?;
    for x in (6..240).step_by(17) {
        rect(d, vis, x, 228 + PLANK, 2, 2, GRASS_DARK)?;
    }
    Ok(())
}

fn cloud<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    vis: &Rectangle,
    x: i32,
    y: i32,
) -> Result<(), D::Error> {
    if !touches(vis, x, y, 30, 10) {
        return Ok(());
    }
    rect(d, vis, x + 6, y, 10, 3, CLOUD)?;
    rect(d, vis, x + 3, y + 3, 22, 3, CLOUD)?;
    rect(d, vis, x, y + 6, 30, 4, CLOUD)
}

/// La torre, las bananas, el tucán, los cocos y el mono.
fn scene<D: DrawTarget<Color = Rgb565>>(
    g: &Cocos,
    d: &mut D,
    vis: &Rectangle,
) -> Result<(), D::Error> {
    let lv = &g.lv;
    for l in &lv.ladders {
        let top = lv.plats[l.lo + 1].surf(l.x * FP) / FP;
        let bottom = lv.plats[l.lo].surf(l.x * FP) / FP;
        ladder(d, vis, l.x, top, bottom, l.broken)?;
    }
    for p in &lv.plats {
        plank(d, vis, p)?;
    }
    for (i, &(bx, bp)) in lv.bananas.iter().enumerate() {
        if !g.taken[i] {
            let by = lv.plats[bp].surf(bx * FP) / FP;
            let bob = ((g.t / 300) % 2) as i32;
            sprite(
                d,
                vis,
                art::BANANA,
                bx - 4,
                by - 10 - bob,
                lv.mirror,
                1,
                pal,
            )?;
        }
    }
    toucan(g, d, vis)?;
    if g.phase != Phase::Clear {
        let ty = lv.plats[TOP].y0;
        sprite(d, vis, art::BUNCH, lv.bunch_x - 7, ty - 14, false, 1, pal)?;
    }
    for c in g.cocos.iter().filter(|c| c.alive) {
        let (mut cx, cy) = (c.x / FP, c.y / FP);
        if c.state == CState::Ladder && c.teeter_ms > 0 {
            // Se tambalea en el borde de la escalera, con un "!" de aviso.
            cx += if (g.t / 70).is_multiple_of(2) { -1 } else { 1 };
            rect(d, vis, c.x / FP, cy - 19, 2, 5, RED)?;
            rect(d, vis, c.x / FP, cy - 13, 2, 2, RED)?;
        }
        coco(d, vis, cx, cy, c.spin, coco_dir(lv, c))?;
    }
    monkey(g, d, vis)?;
    for p in g.popups.iter().flatten() {
        let s = line_num('+', p.pts);
        text_c(
            d,
            vis,
            s.as_str(),
            p.x,
            p.y - 4 - (p.ms / 45) as i32,
            &SMALL,
            CREAM,
            Some(INK),
        )?;
    }
    Ok(())
}

fn coco_dir(lv: &Level, c: &super::Coco) -> i32 {
    match c.state {
        CState::Fall => c.vx.signum(),
        _ => lv.plats[c.plat].dir,
    }
}

/// Una tabla inclinada, en escalones de un pixel, con amarres de soga.
fn plank<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    vis: &Rectangle,
    p: &Plat,
) -> Result<(), D::Error> {
    let ymin = p.y0.min(p.y1);
    let ymax = p.y0.max(p.y1) + PLANK;
    if !touches(
        vis,
        p.x0,
        ymin - 1,
        (p.x1 - p.x0) as u32,
        (ymax - ymin + 2) as u32,
    ) {
        return Ok(());
    }
    // Tramos de igual altura.
    let mut xa = p.x0;
    while xa < p.x1 {
        let y = p.surf(xa * FP) / FP;
        let mut xb = xa + 1;
        while xb < p.x1 && p.surf(xb * FP) / FP == y {
            xb += 1;
        }
        let w = (xb - xa) as u32;
        rect(d, vis, xa, y, w, 1, WOOD_TOP)?;
        rect(d, vis, xa, y + 1, w, 2, WOOD)?;
        rect(d, vis, xa, y + 3, w, 1, WOOD_GRAIN)?;
        rect(d, vis, xa, y + 4, w, 1, WOOD)?;
        rect(d, vis, xa, y + 5, w, 1, WOOD_DARK)?;
        xa = xb;
    }
    // Amarres.
    let mut x = p.x0 + 10;
    while x < p.x1 - 4 {
        let y = p.surf(x * FP) / FP;
        rect(d, vis, x, y - 1, 3, PLANK as u32 + 2, ROPE)?;
        rect(d, vis, x + 1, y + 1, 1, PLANK as u32 - 2, ROPE_DARK)?;
        x += 36;
    }
    Ok(())
}

/// Escalera de bambú. Las rotas tienen un hueco en el medio.
fn ladder<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    vis: &Rectangle,
    x: i32,
    top: i32,
    bottom: i32,
    broken: bool,
) -> Result<(), D::Error> {
    if !touches(vis, x - 6, top, 13, (bottom - top + PLANK) as u32) {
        return Ok(());
    }
    let h = bottom - top;
    let (gap_a, gap_b) = if broken {
        (top + h * 3 / 10, top + h * 6 / 10)
    } else {
        (bottom, bottom)
    };
    for (rx, cut) in [(x - 6, 0), (x + 4, 3)] {
        // Barandas en dos tramos (el segundo vacío si no está rota).
        let (a, b) = (top, gap_a + cut);
        rect(d, vis, rx, a, 2, (b - a).max(0) as u32, BAMBOO)?;
        if broken {
            let a2 = gap_b - cut;
            rect(d, vis, rx, a2, 2, (bottom - a2) as u32, BAMBOO)?;
        }
        let mut ny = top + 8;
        while ny < bottom {
            if ny < gap_a + cut || ny >= gap_b - cut {
                rect(d, vis, rx, ny, 2, 1, BAMBOO_NODE)?;
            }
            ny += 9;
        }
    }
    let mut ry = bottom - 4;
    while ry > top + 1 {
        if ry < gap_a || ry >= gap_b {
            rect(d, vis, x - 4, ry, 8, 2, RUNG)?;
            rect(d, vis, x - 4, ry + 1, 8, 1, BAMBOO_NODE)?;
        }
        ry -= 6;
    }
    Ok(())
}

/// Coco rodando: el cuerpo fijo y tres manchas que giran.
fn coco<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    vis: &Rectangle,
    cx: i32,
    cy: i32,
    spin: u32,
    dir: i32,
) -> Result<(), D::Error> {
    const RING: [(i32, i32); 8] = [
        (2, 0),
        (1, 1),
        (0, 2),
        (-2, 1),
        (-2, 0),
        (-2, -2),
        (0, -2),
        (1, -2),
    ];
    let (x, y) = (cx - 5, cy - 10);
    if !touches(vis, x, y, 10, 10) {
        return Ok(());
    }
    sprite(d, vis, art::COCO, x, y, false, 1, pal)?;
    let mut a = ((spin / FP as u32) / 4 % 8) as usize;
    if dir < 0 {
        a = (8 - a) % 8;
    }
    for k in [0, 3, 5] {
        let (ox, oy) = RING[(a + k) % 8];
        rect(d, vis, cx - 1 + ox, cy - 6 + oy, 2, 2, pal(b'k'))?;
    }
    Ok(())
}

fn toucan<D: DrawTarget<Color = Rgb565>>(
    g: &Cocos,
    d: &mut D,
    vis: &Rectangle,
) -> Result<(), D::Error> {
    let lv = &g.lv;
    let flip = lv.plats[TOP].dir < 0;
    let ty = lv.plats[TOP].y0;
    let (frame, dx, dy) = match g.phase {
        Phase::Clear => {
            // Se va volando, ofendido.
            let up = (g.phase_ms / 14) as i32;
            let away = (g.phase_ms / 30) as i32 * if flip { 1 } else { -1 };
            (art::TOUCAN_FLY, away, -up + 4)
        }
        Phase::Play if g.spawn_ms < 300 => (art::TOUCAN_THROW, 0, 0),
        _ => (art::TOUCAN, 0, 0),
    };
    let x = lv.toucan_x - 12 + dx;
    let y = ty - frame.len() as i32 + dy;
    // Que no tape la barra de arriba al irse.
    if y + frame.len() as i32 <= HUD_H {
        return Ok(());
    }
    sprite(d, vis, frame, x, y, flip, 1, pal)
}

fn monkey<D: DrawTarget<Color = Rgb565>>(
    g: &Cocos,
    d: &mut D,
    vis: &Rectangle,
) -> Result<(), D::Error> {
    let m = &g.m;
    let (mx, my) = (m.x / FP, m.y / FP);
    let mut flip = m.face < 0;
    let mut lift = 0;
    let (head, body) = match (g.phase, m.state) {
        (Phase::Dying | Phase::Over, _) => (art::HEAD_DIZZY, art::BODY_STAND),
        (Phase::Clear, _) => {
            // Saltitos de alegría.
            let t = (g.phase_ms / 10 % 40) as i32;
            lift = if t < 20 { t / 3 } else { (40 - t) / 3 };
            (art::HEAD_HAPPY, art::BODY_JUMP)
        }
        (_, MState::Climb) => {
            flip = (m.walk_ms / 160) % 2 == 1;
            (art::HEAD_BACK, art::BODY_CLIMB)
        }
        (_, MState::Air) => (art::HEAD, art::BODY_JUMP),
        (_, MState::Ground) if m.moving => (
            art::HEAD,
            if (m.walk_ms / 110).is_multiple_of(2) {
                art::BODY_WALK_A
            } else {
                art::BODY_WALK_B
            },
        ),
        _ => (art::HEAD, art::BODY_STAND),
    };
    let (x, y) = (mx - 8, my - 18 - lift);
    if touches(vis, x - 8, y - 22, 32, 44) {
        sprite(d, vis, head, x, y, flip, 1, pal)?;
        sprite(d, vis, body, x, y + 11, flip, 1, pal)?;
        match g.phase {
            Phase::Clear => sprite(d, vis, art::BUNCH, mx - 7, y - 13, false, 1, pal)?,
            Phase::Dying | Phase::Over => {
                // Estrellitas girando alrededor de la cabeza.
                const RING: [(i32, i32); 6] = [(8, 0), (4, 3), (-4, 3), (-8, 0), (-4, -3), (4, -3)];
                let a = (g.t / 90) as usize;
                for k in [0, 2, 4] {
                    let (ox, oy) = RING[(a + k) % 6];
                    let (sx, sy) = (mx + ox, y - 2 + oy);
                    rect(d, vis, sx - 1, sy, 3, 1, STAR)?;
                    rect(d, vis, sx, sy - 1, 1, 3, STAR)?;
                }
            }
            Phase::Play
                // Flechita: "acá podés trepar".
                if g.ladder_here().is_some() && (g.t / 250).is_multiple_of(2) => {
                    let (ax, ay) = (mx, y - 7);
                    rect(d, vis, ax, ay, 1, 1, INK)?;
                    rect(d, vis, ax - 1, ay + 1, 3, 1, INK)?;
                    rect(d, vis, ax - 2, ay + 2, 5, 1, INK)?;
                    rect(d, vis, ax, ay + 3, 1, 2, INK)?;
                }
            _ => {}
        }
    }
    Ok(())
}

/// Barra de arriba: puntos, nivel y vidas. Y el bono, del lado sin tucán.
fn hud<D: DrawTarget<Color = Rgb565>>(
    g: &Cocos,
    d: &mut D,
    vis: &Rectangle,
) -> Result<(), D::Error> {
    if touches(vis, 0, 0, 240, HUD_H as u32) {
        rect(d, vis, 0, 0, 240, HUD_H as u32 - 1, HUD_BG)?;
        rect(d, vis, 0, HUD_H - 1, 240, 1, HUD_LINE)?;
        let mut s = Line::new();
        let _ = write!(s, "{:06}", g.score);
        text(d, s.as_str(), 4, 3, &SMALL, CREAM)?;
        let mut s = Line::new();
        let _ = write!(s, "NIVEL {}", g.level);
        text_c(d, vis, s.as_str(), 120, 3, &SMALL, GOLD, None)?;
        for i in 0..g.lives as i32 {
            sprite(d, vis, art::LIFE, 227 - i * 11, 5, false, 1, pal)?;
        }
    }
    // "BONO 2600" en un cartelito, con el número grande.
    let (w, h) = (104, 24);
    let bx = if g.lv.mirror { 6 } else { 240 - 6 - w };
    if touches(vis, bx, 21, w as u32, h as u32) {
        panel(d, vis, bx, 21, w as u32, h as u32, CREAM, WOOD_DARK)?;
        let mut s = Line::new();
        let _ = write!(s, "{}", g.bonus);
        let c = if g.bonus <= 1000 { RED } else { INK };
        let (lw, nw) = (SMALL.width("BONO"), BODY.width(s.as_str()));
        let x = bx + (w - lw - 5 - nw) / 2 + 1;
        let y = 21 + 5 + BODY.cap - SMALL.cap;
        text(d, "BONO", x, y, &SMALL, WOOD_DARK)?;
        text(d, s.as_str(), x + lw + 5, 21 + 5, &BODY, c)?;
    }
    Ok(())
}

fn banner<D: DrawTarget<Color = Rgb565>>(
    g: &Cocos,
    d: &mut D,
    vis: &Rectangle,
) -> Result<(), D::Error> {
    if !touches(vis, 60, 96, 120, 34) {
        return Ok(());
    }
    panel(d, vis, 60, 96, 120, 34, CREAM, WOOD_DARK)?;
    let mut s = Line::new();
    let _ = write!(s, "Nivel {}", g.level);
    text_c(d, vis, s.as_str(), 120, 103, &TITLE, INK, None)
}

fn cleared<D: DrawTarget<Color = Rgb565>>(
    g: &Cocos,
    d: &mut D,
    vis: &Rectangle,
) -> Result<(), D::Error> {
    if !touches(vis, 8, 90, 224, 52) {
        return Ok(());
    }
    panel(d, vis, 8, 90, 224, 52, CREAM, WOOD_DARK)?;
    text_c(d, vis, "¡Bananas recuperadas!", 120, 97, &BODY, INK, None)?;
    // El bono queda congelado mientras dura el festejo.
    let mut s = Line::new();
    let _ = write!(s, "+{}  bono +{}", super::PTS_CLEAR, g.bonus);
    text_c(d, vis, s.as_str(), 120, 119, &BODY, WOOD_DARK, None)
}

fn game_over<D: DrawTarget<Color = Rgb565>>(
    g: &Cocos,
    d: &mut D,
    vis: &Rectangle,
) -> Result<(), D::Error> {
    let (x, y, w, h) = (10, 56, 220, 128);
    if !touches(vis, x, y, w, h) {
        return Ok(());
    }
    panel(d, vis, x, y, w, h, CREAM, WOOD_DARK)?;
    text_c(d, vis, "Fin del juego", 120, y + 9, &TITLE, INK, None)?;
    let s = line_lbl("Puntos:", g.score);
    text_c(d, vis, s.as_str(), 120, y + 42, &BODY, INK, None)?;
    let s = line_lbl("Mejor:", g.best);
    text_c(d, vis, s.as_str(), 120, y + 62, &BODY, WOOD_DARK, None)?;
    if g.new_best && g.score > 0 && (g.t / 400).is_multiple_of(2) {
        text_c(d, vis, "¡Nuevo récord!", 120, y + 84, &SMALL, RED, None)?;
    }
    text_c(
        d,
        vis,
        "C: jugar otra vez · B: salir",
        120,
        y + 106,
        &SMALL,
        INK,
        None,
    )
}

fn line_num(prefix: char, n: u32) -> Line {
    let mut s = Line::new();
    let _ = write!(s, "{prefix}{n}");
    s
}

fn line_lbl(label: &str, n: u32) -> Line {
    let mut s = Line::new();
    let _ = write!(s, "{label} {n}");
    s
}

/// Portada: cartel, el tucán con el racimo y el mono saltando un coco.
fn title<D: DrawTarget<Color = Rgb565>>(
    g: &Cocos,
    d: &mut D,
    vis: &Rectangle,
) -> Result<(), D::Error> {
    // Cartel de madera.
    if touches(vis, 12, 10, 216, 42) {
        rect(d, vis, 50, 10, 3, 8, ROPE_DARK)?;
        rect(d, vis, 187, 10, 3, 8, ROPE_DARK)?;
        panel(d, vis, 12, 16, 216, 38, WOOD, WOOD_DARK)?;
        rect(d, vis, 14, 18, 212, 2, WOOD_TOP)?;
        text_c(d, vis, "Cocos Rodantes", 120, 24, &TITLE, CREAM, Some(INK))?;
    }
    // Rama con el tucán y las bananas robadas.
    let floor = 168;
    let branch = 104;
    rect(d, vis, 140, branch, 100, 4, WOOD)?;
    rect(d, vis, 140, branch + 4, 100, 2, WOOD_DARK)?;
    for (lx, ly) in [(146, 99), (230, 100), (200, 100)] {
        rect(d, vis, lx, ly, 6, 3, pal(b'G'))?;
        rect(d, vis, lx + 2, ly + 3, 4, 1, pal(b'L'))?;
    }
    let throw = (g.t / 600) % 4 == 3;
    let tf = if throw {
        art::TOUCAN_THROW
    } else {
        art::TOUCAN
    };
    sprite(d, vis, tf, 216 - 48, branch - 38, true, 2, pal)?;
    sprite(d, vis, art::BUNCH, 150, branch - 14, false, 1, pal)?;
    // Piso y un coco que va y viene; el mono lo salta.
    plank(
        d,
        vis,
        &Plat {
            x0: 0,
            y0: floor,
            x1: 240,
            y1: floor,
            dir: -1,
        },
    )?;
    let period = 300;
    let p = (g.t / 10 % period) as i32;
    let cx = 260 - p;
    coco(d, vis, cx, floor, (p as u32) * FP as u32 * 2, -1)?;
    let mx = 70;
    let dx = cx - mx;
    let hop = if dx.abs() < 34 {
        (34 * 34 - dx * dx) / 30
    } else {
        0
    };
    let (head, body) = if hop > 0 {
        (art::HEAD_HAPPY, art::BODY_JUMP)
    } else {
        (art::HEAD, art::BODY_STAND)
    };
    let (x, y) = (mx - 24, floor - 54 - hop);
    sprite(d, vis, head, x, y, false, 3, pal)?;
    sprite(d, vis, body, x, y + 33, false, 3, pal)?;
    // Cómo se juega.
    if touches(vis, 8, 176, 224, 64) {
        panel(d, vis, 8, 176, 224, 64, CREAM, WOOD_DARK)?;
        // Titila: empezar (en rojo) y salir, de a uno.
        if (g.t / 600).is_multiple_of(2) {
            text_c(d, vis, "C para empezar", 120, 182, &BODY, RED, None)?;
        } else {
            text_c(d, vis, "B para salir", 120, 182, &BODY, INK, None)?;
        }
        small(d, vis, "A/C: caminar   B: saltar", 203)?;
        small(d, vis, "Escalera: B sube, A/C baja", 221)?;
    }
    Ok(())
}

fn small<D: DrawTarget<Color = Rgb565>>(
    d: &mut D,
    vis: &Rectangle,
    s: &str,
    y: i32,
) -> Result<(), D::Error> {
    text_c(d, vis, s, 120, y, &SMALL, INK, None)
}
