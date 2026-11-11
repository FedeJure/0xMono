use super::*;

const NONE: Keys = Keys {
    a: false,
    b: false,
    c: false,
};
const A: Keys = Keys { a: true, ..NONE };
const B: Keys = Keys { b: true, ..NONE };
const C: Keys = Keys { c: true, ..NONE };

/// Corre `ms` con esos botones, en cuadros de 33 ms.
fn run(g: &mut Cocos, keys: Keys, ms: u32) -> Status {
    let mut left = ms;
    while left > 0 {
        let dt = left.min(33);
        if g.update(keys, dt) == Status::Exit {
            return Status::Exit;
        }
        left -= dt;
    }
    Status::Playing
}

/// Partida empezada, sin cocos (el tucán espera mucho).
fn playing(seed: u32) -> Cocos {
    let mut g = Cocos::new(seed);
    run(&mut g, NONE, 50);
    run(&mut g, C, 50);
    assert_eq!(g.phase, Phase::Play);
    g
}

fn quiet(seed: u32) -> Cocos {
    let mut g = playing(seed);
    g.spawn_ms = i32::MAX;
    g
}

fn px(v: i32) -> i32 {
    v / FP
}

/// Pone al mono parado en la tabla `p`, en `x` pixeles.
fn place(g: &mut Cocos, p: usize, x: i32) {
    g.m.x = x * FP;
    g.m.y = g.lv.plats[p].surf(x * FP);
    g.m.plat = p;
    g.m.state = MState::Ground;
    g.m.vy = 0;
}

fn put_coco(g: &mut Cocos, p: usize, x: i32) -> usize {
    let i = g.cocos.iter().position(|c| !c.alive).unwrap();
    g.cocos[i] = Coco {
        alive: true,
        x: x * FP,
        y: g.lv.plats[p].surf(x * FP),
        state: CState::Roll,
        plat: p,
        ..Coco::DEAD
    };
    i
}

#[test]
fn sprites_are_rectangular() {
    for s in [
        art::HEAD,
        art::HEAD_HAPPY,
        art::HEAD_DIZZY,
        art::HEAD_BACK,
        art::BODY_STAND,
        art::BODY_WALK_A,
        art::BODY_WALK_B,
        art::BODY_JUMP,
        art::BODY_CLIMB,
        art::COCO,
        art::TOUCAN,
        art::TOUCAN_THROW,
        art::TOUCAN_FLY,
        art::BANANA,
        art::BUNCH,
        art::LIFE,
    ] {
        assert!(s.iter().all(|r| r.len() == s[0].len()), "{s:?}");
    }
}

#[test]
fn geometry_is_consistent() {
    for mirror in [false, true] {
        let lv = Level::build(mirror);
        for l in &lv.ladders {
            assert!(lv.plats[l.lo].contains(l.x * FP), "{l:?}");
            assert!(lv.plats[l.lo + 1].contains(l.x * FP), "{l:?}");
            assert!(lv.top_of(l) < lv.bottom_of(l));
        }
        // Las tablas bajan hacia donde ruedan los cocos.
        for p in &lv.plats[1..TOP] {
            assert_eq!((p.y1 - p.y0).signum(), p.dir, "{p:?}");
        }
        // Siempre hay una escalera trepable entre cada par de tablas.
        for lo in 0..TOP {
            assert!(lv.ladders.iter().any(|l| l.lo == lo && !l.broken));
        }
        for &(x, p) in &lv.bananas {
            assert!(lv.plats[p].contains(x * FP));
        }
    }
}

#[test]
fn title_waits_for_c_released_from_menu() {
    let mut g = Cocos::new(1);
    // C venía apretado desde el menú: no empieza hasta soltarlo.
    run(&mut g, C, 500);
    assert_eq!(g.phase, Phase::Title);
    run(&mut g, NONE, 50);
    run(&mut g, C, 50);
    assert_eq!(g.phase, Phase::Play);
}

#[test]
fn b_on_title_exits() {
    let mut g = Cocos::new(1);
    run(&mut g, NONE, 50);
    assert_eq!(run(&mut g, B, 50), Status::Exit);
}

#[test]
fn gravity_jump_and_land_in_place() {
    let mut g = quiet(2);
    place(&mut g, 0, 150);
    let y0 = g.m.y;
    g.update(B, 10);
    assert_eq!(g.m.state, MState::Air);
    let mut top = y0;
    for _ in 0..100 {
        g.update(B, 10);
        top = top.min(g.m.y);
    }
    let h = px(y0 - top);
    assert!((13..=17).contains(&h), "altura del salto {h}");
    assert_eq!(g.m.state, MState::Ground);
    assert_eq!(g.m.y, y0);
    // Mantener B no vuelve a saltar.
    assert_eq!(g.m.state, MState::Ground);
}

#[test]
fn lands_on_sloped_platform() {
    let mut g = quiet(3);
    g.m.x = 100 * FP;
    g.m.y = 170 * FP;
    g.m.state = MState::Air;
    g.m.vy = 0;
    run(&mut g, NONE, 400);
    assert_eq!(g.m.state, MState::Ground);
    assert_eq!(g.m.plat, 1);
    assert_eq!(g.m.y, g.lv.plats[1].surf(g.m.x));
}

#[test]
fn walking_follows_the_slope_and_falls_off_the_gap() {
    let mut g = quiet(4);
    place(&mut g, 1, 200);
    let y0 = g.m.y;
    run(&mut g, A, 1000);
    assert_eq!(g.m.plat, 1);
    assert!(g.m.y > y0, "la tabla 1 baja hacia la izquierda");
    assert_eq!(g.m.y, g.lv.plats[1].surf(g.m.x));
    // Por el hueco de la izquierda cae al piso, sin lastimarse.
    run(&mut g, A, 3000);
    assert_eq!(g.m.plat, 0);
    assert_eq!(g.m.state, MState::Ground);
    assert_eq!(g.phase, Phase::Play);
    assert_eq!(px(g.m.x), 6, "no se sale de la pantalla");
}

#[test]
fn climbs_ladder_holding_b_and_comes_down_with_a() {
    let mut g = quiet(5);
    place(&mut g, 0, 61);
    assert_eq!(g.ladder_here(), Some(0));
    g.update(B, 20);
    assert_eq!(g.m.state, MState::Climb);
    assert_eq!(px(g.m.x), 64, "se centra en la escalera");
    run(&mut g, B, 400);
    let mid = g.m.y;
    assert!(mid < g.lv.bottom_of(&g.lv.ladders[0]));
    // Soltar B lo deja colgado.
    run(&mut g, NONE, 300);
    assert_eq!(g.m.y, mid);
    // A baja.
    while g.m.state == MState::Climb {
        g.update(A, 20);
    }
    assert_eq!(g.m.plat, 0);
    assert_eq!(g.ladder_here(), Some(0));
    // Y de nuevo arriba hasta la tabla 1.
    run(&mut g, NONE, 50);
    run(&mut g, B, 1200);
    assert_eq!(g.m.state, MState::Ground);
    assert_eq!(g.m.plat, 1);
}

#[test]
fn broken_ladders_cannot_be_climbed() {
    let mut g = quiet(6);
    place(&mut g, 1, 112);
    assert_eq!(g.ladder_here(), None);
    g.update(B, 20);
    assert_eq!(g.m.state, MState::Air, "salta en vez de trepar");
}

#[test]
fn coconut_hit_costs_a_life_and_respawns() {
    let mut g = quiet(7);
    place(&mut g, 0, 150);
    put_coco(&mut g, 0, 170);
    run(&mut g, NONE, 600);
    assert_eq!(g.phase, Phase::Dying);
    assert_eq!(g.lives, 2);
    run(&mut g, NONE, DYING_MS + 100);
    assert_eq!(g.phase, Phase::Play);
    assert_eq!(px(g.m.x), g.lv.start_x);
    assert!(g.cocos.iter().all(|c| !c.alive));
}

#[test]
fn jumping_a_coconut_scores() {
    let mut g = quiet(8);
    place(&mut g, 0, 150);
    put_coco(&mut g, 0, 175);
    // El coco rueda a la izquierda a 42 px/s: saltar en el lugar cuando
    // está a unos 11 px.
    run(&mut g, NONE, 330);
    run(&mut g, B, 700);
    assert_eq!(g.phase, Phase::Play);
    assert_eq!(g.score, PTS_JUMP);
    // Ya no da más puntos.
    run(&mut g, NONE, 1000);
    assert_eq!(g.score, PTS_JUMP);
}

#[test]
fn bananas_give_points_once() {
    let mut g = quiet(9);
    let (bx, bp) = g.lv.bananas[0];
    place(&mut g, bp, bx + 20);
    run(&mut g, A, 600);
    assert!(g.taken[0]);
    assert_eq!(g.score, PTS_BANANA);
    run(&mut g, C, 600);
    assert_eq!(g.score, PTS_BANANA);
}

#[test]
fn reaching_the_top_clears_the_level() {
    let mut g = quiet(10);
    let li = g.lv.ladders.iter().position(|l| l.lo == TOP - 1).unwrap();
    let lx = g.lv.ladders[li].x;
    place(&mut g, TOP - 1, lx);
    run(&mut g, B, 1500);
    assert_eq!(g.phase, Phase::Clear);
    let bonus = g.bonus;
    assert_eq!(g.score, PTS_CLEAR + bonus);
    run(&mut g, NONE, CLEAR_MS + 50);
    assert_eq!(g.phase, Phase::Play);
    assert_eq!(g.level, 2);
    assert!(g.lv.mirror, "el nivel 2 es el espejo");
    assert_eq!(g.lives, LIVES);
    assert!(g.coco_speed() > 42 * FP);
}

#[test]
fn game_over_then_b_exits_and_c_replays_keeping_best() {
    let mut g = quiet(11);
    g.score = 1234;
    for lives in (0..LIVES).rev() {
        place(&mut g, 0, 150);
        g.spawn_ms = i32::MAX;
        put_coco(&mut g, 0, 160);
        run(&mut g, NONE, 300);
        assert_eq!(g.lives, lives);
        run(&mut g, NONE, DYING_MS + 50);
    }
    assert_eq!(g.phase, Phase::Over);
    assert_eq!(g.best, 1234);
    assert!(g.new_best);
    // Un rato de gracia: apretar enseguida no hace nada.
    assert_eq!(run(&mut g, C, 100), Status::Playing);
    assert_eq!(g.phase, Phase::Over);
    run(&mut g, NONE, OVER_GRACE_MS);
    run(&mut g, C, 50);
    assert_eq!(g.phase, Phase::Play);
    assert_eq!(g.score, 0);
    assert_eq!(g.lives, LIVES);
    assert_eq!(g.best, 1234);
    // Otra vez a perder, y B sale.
    for _ in 0..LIVES {
        place(&mut g, 0, 150);
        g.spawn_ms = i32::MAX;
        put_coco(&mut g, 0, 160);
        run(&mut g, NONE, 300 + DYING_MS + 50);
    }
    assert_eq!(g.phase, Phase::Over);
    assert!(!g.new_best);
    run(&mut g, NONE, OVER_GRACE_MS);
    assert_eq!(run(&mut g, B, 50), Status::Exit);
}

#[test]
fn coconuts_roll_down_the_tower() {
    let mut g = playing(12);
    let mut seen = [false; PLATS];
    let mut ladder_falls = 0;
    // El mono quieto en el piso, a la derecha, nunca lo toca nadie.
    for _ in 0..(30_000 / 33) {
        g.update(NONE, 33);
        for c in g.cocos.iter().filter(|c| c.alive) {
            seen[c.plat] = true;
            if c.state == CState::Ladder {
                ladder_falls += 1;
            }
        }
    }
    assert_eq!(seen, [true; PLATS], "pasan por todas las tablas");
    assert!(ladder_falls > 0, "alguno cae por una escalera");
    assert!(g.bonus < BONUS_START);
}

#[test]
fn big_dt_is_clamped() {
    let mut g = quiet(13);
    place(&mut g, 0, 150);
    g.update(C, 10_000);
    let moved = px(g.m.x) - 150;
    assert!((4..=6).contains(&moved), "solo 100 ms: {moved}");
}

/// Mono, puntos, vidas y cocos.
type Snapshot = (i32, i32, u32, u8, [(bool, i32, i32); MAX_COCOS]);

/// Guarda un resumen de la partida para comparar.
fn snapshot(g: &Cocos) -> Snapshot {
    (
        g.m.x,
        g.m.y,
        g.score,
        g.lives,
        g.cocos.map(|c| (c.alive, c.x, c.y)),
    )
}

#[test]
fn deterministic_for_any_dt_split() {
    let script = [(A, 2970), (B, 33), (A, 1980), (NONE, 4950), (C, 3300)];
    let play = |dt: u32| {
        let mut g = playing(77);
        for (k, ms) in script {
            let mut left = ms;
            while left > 0 {
                let d = left.min(dt);
                g.update(k, d);
                left -= d;
            }
        }
        snapshot(&g)
    };
    let a = play(33);
    assert_eq!(a, play(11));
    assert_eq!(a, play(3));
    assert_eq!(a, play(99));
}

/// Un jugador automático sencillo: camina hacia la escalera del camino, salta
/// los cocos que vienen rodando, se corre de los que caen y trepa sin meterse
/// debajo de uno.
fn bot(g: &Cocos, prev_b: bool) -> Keys {
    let m = &g.m;
    let speed = g.coco_speed();
    let alive = || g.cocos.iter().filter(|c| c.alive);
    match m.state {
        MState::Climb => {
            let l = g.lv.ladders[m.ladder];
            let top = g.lv.top_of(&l);
            if alive().any(|c| c.state == CState::Ladder && c.ladder == m.ladder && c.y < m.y) {
                return A;
            }
            // Cerca de arriba, esperar (o bajar) si viene un coco rodando.
            let coming = alive().any(|c| {
                c.plat == l.lo + 1
                    && c.state != CState::Ladder
                    && (c.x - m.x).abs() < 40 * FP
                    && ((c.x - m.x).signum() == -g.lv.plats[c.plat].dir
                        || (c.x - m.x).abs() < 8 * FP)
            });
            match m.y - top {
                _ if !coming => B,
                d if d < 16 * FP => A,
                d if d < 24 * FP => NONE,
                _ => B,
            }
        }
        MState::Air => NONE,
        MState::Ground => {
            let target =
                g.lv.ladders
                    .iter()
                    .find(|l| l.lo == m.plat && !l.broken)
                    .map_or(m.x, |l| l.x * FP);
            let dir = (target - m.x).signum();
            // Un coco que cae (por una escalera o un borde) cerca: correrse.
            for c in alive() {
                let falling = c.state == CState::Fall
                    || (c.state == CState::Ladder && g.lv.ladders[c.ladder].lo == m.plat);
                if falling && (c.x - m.x).abs() < 18 * FP && c.y < m.y && c.y > m.y - 60 * FP {
                    let away = if c.x > m.x { -1 } else { 1 };
                    return dir_keys(away);
                }
            }
            // ¿Viene uno rodando?
            for c in alive() {
                if c.state != CState::Roll || c.plat != m.plat {
                    continue;
                }
                let cd = g.lv.plats[c.plat].dir;
                let dx = c.x - m.x;
                if dx.signum() != -cd || dx == 0 {
                    continue;
                }
                let closing = speed
                    + if dir == dx.signum() {
                        WALK
                    } else {
                        -WALK * dir.abs()
                    };
                if closing <= 0 {
                    continue;
                }
                let t_ms = dx.abs() as i64 * 1000 / closing as i64;
                if (150..=260).contains(&t_ms) {
                    return if prev_b {
                        NONE
                    } else {
                        Keys {
                            b: true,
                            ..dir_keys(dir)
                        }
                    };
                }
            }
            if (target - m.x).abs() <= 2 * FP {
                // Al pie: no subir si un coco está bajando por la escalera o
                // está por pasar por arriba.
                let up = m.plat + 1;
                let busy = alive().any(|c| {
                    (c.state == CState::Ladder && (c.x - target).abs() < 4 * FP)
                        || (c.plat == up
                            && c.state == CState::Roll
                            && (c.x - target).abs() < 50 * FP
                            && (c.x - target).signum() == -g.lv.plats[up].dir)
                });
                return if busy { NONE } else { B };
            }
            dir_keys(dir)
        }
    }
}

fn dir_keys(dir: i32) -> Keys {
    Keys {
        a: dir < 0,
        b: false,
        c: dir > 0,
    }
}

/// Juega el nivel 1 con el bot. Devuelve (llegó arriba, vidas perdidas, ms).
fn bot_level1(seed: u32) -> (bool, u8, u32) {
    let mut g = playing(seed);
    let mut prev_b = false;
    let mut ms = 0;
    while ms < 120_000 {
        let k = if g.phase == Phase::Play {
            bot(&g, prev_b)
        } else {
            NONE
        };
        prev_b = k.b;
        g.update(k, 33);
        ms += 33;
        match g.phase {
            Phase::Clear => return (true, LIVES - g.lives, ms),
            Phase::Over => return (false, LIVES, ms),
            _ => {}
        }
    }
    (false, LIVES - g.lives, ms)
}

#[test]
fn level_one_is_completable_and_fair() {
    let mut cleared = 0;
    let mut flawless = 0;
    let mut total_ms = 0;
    const N: u32 = 60;
    for seed in 0..N {
        let (ok, lost, ms) = bot_level1(seed * 7919 + 1);
        if ok {
            cleared += 1;
            total_ms += ms;
            if lost == 0 {
                flawless += 1;
            }
        }
    }
    // Un jugador sencillo lo pasa siempre y casi siempre sin morir.
    assert!(cleared >= N * 95 / 100, "pasó {cleared}/{N}");
    assert!(flawless >= N * 8 / 10, "sin morir {flawless}/{N}");
    let avg = total_ms / cleared.max(1);
    assert!((15_000..60_000).contains(&avg), "dura {avg} ms");
}

/// Con el mismo jugador, los niveles siguientes cuestan más.
#[test]
fn later_levels_are_harder() {
    let deaths = |level: u32| -> u32 {
        (0..40u32)
            .map(|seed| {
                let mut g = playing(seed * 104_729 + 3);
                g.level = level;
                g.start_level();
                let mut prev_b = false;
                for _ in 0..(90_000 / 33) {
                    let k = if g.phase == Phase::Play {
                        bot(&g, prev_b)
                    } else {
                        NONE
                    };
                    prev_b = k.b;
                    g.update(k, 33);
                    if matches!(g.phase, Phase::Clear | Phase::Over) {
                        break;
                    }
                }
                (LIVES - g.lives) as u32
            })
            .sum()
    };
    let (d1, d5) = (deaths(1), deaths(5));
    assert!(d5 > d1 * 3, "nivel 1: {d1} muertes, nivel 5: {d5}");
}

#[test]
fn bot_also_clears_mirrored_level_two() {
    let mut g = playing(5);
    g.level = 2;
    g.start_level();
    assert!(g.lv.mirror);
    let mut prev_b = false;
    let mut ok = false;
    for _ in 0..(120_000 / 33) {
        let k = if g.phase == Phase::Play {
            bot(&g, prev_b)
        } else {
            NONE
        };
        prev_b = k.b;
        g.update(k, 33);
        if g.phase == Phase::Clear {
            ok = true;
            break;
        }
        if g.phase == Phase::Over {
            break;
        }
    }
    assert!(ok);
}
