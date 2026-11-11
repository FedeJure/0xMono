//! Pixel art de Cocos Rodantes: una fila por texto, un carácter por pixel.
//! `.` es transparente; los demás colores salen de [`pal`].

use embedded_graphics::pixelcolor::Rgb565;

use crate::gfx::rgb;

pub fn pal(c: u8) -> Rgb565 {
    match c {
        // Mono
        b'D' => rgb(0x3B2414), // contorno
        b'B' => rgb(0x9A6233), // pelo
        b'b' => rgb(0x744620), // pelo en sombra
        b'F' => rgb(0xF6DAB0), // cara y panza
        b'K' => rgb(0x1A1209), // ojos
        b'W' => rgb(0xFFFFFF),
        b'P' => rgb(0xF59AB0), // cachetes
        b'M' => rgb(0x8A3A2A), // boca
        // Coco
        b'C' => rgb(0x8A5630),
        b'c' => rgb(0xC4925E),
        b'k' => rgb(0x2E180A),
        // Tucán
        b'N' => rgb(0x22223A), // plumas
        b'n' => rgb(0x45456A), // ala
        b'T' => rgb(0xFFF3C4), // pechera
        b'O' => rgb(0xF7931E), // pico
        b'Y' => rgb(0xFFD23F), // pico claro y bananas
        b'y' => rgb(0xD9A21B), // banana en sombra
        b'R' => rgb(0xE84545), // punta del pico, cola
        b'U' => rgb(0x5EC8F2), // anillo del ojo
        b'g' => rgb(0x7D86A8), // patas
        b'G' => rgb(0x5DAA3C), // hojas
        b'L' => rgb(0x3E7D2A), // hojas oscuras
        _ => rgb(0xFF00FF),
    }
}

/// Cabeza del mono mirando a la derecha (11 filas). El cuerpo va debajo.
pub const HEAD: &[&str] = &[
    ".....DDDDDD.....",
    "....DBBBBBBD....",
    "...DBBBBBBBBD...",
    "..DBBBFFFBFFFD..",
    ".DDBBFFFFFFFFD..",
    "DFFDBFFKWFFKWFD.",
    "DFFDBFFKKFFKKFD.",
    ".DDBBFFFFFFFFFD.",
    "...DBFPPFMFPPFD.",
    "....DBFFFFFFFD..",
    ".....DDDDDDDD...",
];

/// Contento: ojos cerrados como sonrisa.
pub const HEAD_HAPPY: &[&str] = &[
    ".....DDDDDD.....",
    "....DBBBBBBD....",
    "...DBBBBBBBBD...",
    "..DBBBFFFBFFFD..",
    ".DDBBFFFFFFFFD..",
    "DFFDBFFKKFFKKFD.",
    "DFFDBFKFFKKFFKD.",
    ".DDBBFFFFFFFFFD.",
    "...DBFPPMMMPPFD.",
    "....DBFFMMFFFD..",
    ".....DDDDDDDD...",
];

/// Mareado, después de un cocazo.
pub const HEAD_DIZZY: &[&str] = &[
    ".....DDDDDD.....",
    "....DBBBBBBD....",
    "...DBBBBBBBBD...",
    "..DBBBFFFBFFFD..",
    ".DDBBFKFKFKFKD..",
    "DFFDBFFKFFFKFFD.",
    "DFFDBFKFKFKFKFD.",
    ".DDBBFFFFFFFFFD.",
    "...DBFPPMMFPPFD.",
    "....DBFFMMFFFD..",
    ".....DDDDDDDD...",
];

/// De espaldas, trepando.
pub const HEAD_BACK: &[&str] = &[
    ".....DDDDDD.....",
    "....DBBBBBBD....",
    "...DBBBBBBBBD...",
    "..DBBBBBBBBBBD..",
    ".DDBBBBBBBBBBDD.",
    "DFFDBBBBBBBBDFFD",
    "DFFDBBBBBBBBDFFD",
    ".DDBBbBBBBbBBDD.",
    "..DBBBbbbbBBBD..",
    "...DBBBBBBBBD...",
    "....DDDDDDDD....",
];

/// Cuerpos (7 filas), debajo de la cabeza.
pub const BODY_STAND: &[&str] = &[
    "..D...DBBBBD....",
    ".D...DBFFFFBD...",
    ".D..DBDFFFFDBD..",
    "..D.DFDBBBBDFD..",
    "...DDDBBBBBBD...",
    "......DBDDBD....",
    ".....DDDD.DDDD..",
];

pub const BODY_WALK_A: &[&str] = &[
    "..D...DBBBBD....",
    ".D...DBFFFFBD...",
    ".D..DBDFFFFDBD..",
    "..D.DFDBBBBDFD..",
    "...DDDBBBBBBD...",
    ".....DBD..DBD...",
    "....DDDD..DDDD..",
];

pub const BODY_WALK_B: &[&str] = &[
    "..D...DBBBBD....",
    ".D...DBFFFFBD...",
    ".D...DBFFFFBDD..",
    "..D.DFBBBBBBFD..",
    "...DDDBBBBBBD...",
    ".......DBBD.....",
    "......DDDDDD....",
];

/// Saltando o festejando: brazos arriba, piernas encogidas.
pub const BODY_JUMP: &[&str] = &[
    "DFD...DBBBBD.DFD",
    ".DBD.DBFFFFBDBD.",
    "..DBDBBFFFFBBD..",
    ".D.DBBBFFFFBD...",
    "D..DDBBBBBBBD...",
    ".DD.DBBDDBBD....",
    "....DDD..DDD....",
];

/// Trepando de espaldas (se espeja para el otro paso).
pub const BODY_CLIMB: &[&str] = &[
    "DFD..DBBBBD.....",
    ".DBDDBBBBBBD....",
    "..DDBBBBBBBBDD..",
    "....DBBDDBBBDFD.",
    "....DBBBDBBBD...",
    "...DBBD..DBBD...",
    "...DDD...DDDD...",
];

/// Coco sin las manchas (las manchas giran aparte).
pub const COCO: &[&str] = &[
    "...DDDD...",
    ".DDCCCCDD.",
    ".DCccCCCD.",
    "DCcCCCCCCD",
    "DCCCCCCCCD",
    "DCCCCCCCCD",
    "DCCCCCCCCD",
    ".DCCCCCCD.",
    ".DDCCCCDD.",
    "...DDDD...",
];

/// Tucán mirando a la derecha.
pub const TOUCAN: &[&str] = &[
    ".......NNNNN............",
    "......NNNNNNNN..........",
    ".....NNNNKKKKNN.........",
    ".....NNNNUUUUKOOOOO.....",
    ".....NNNUUKWUUYYYYYYY...",
    ".....NNNTUUUUYYYYYYYYYYK",
    ".....NNTTTTTOOOOOOOOORKK",
    "....NNTTTTTTNOOOOOORRK..",
    "....NNTTTTTTN.RRRRRR....",
    "...NNnTTTTTTN...........",
    "...NnnnTTTTTN...........",
    "..NNnnnnTTTRN...........",
    "..NNNnnnnNNNN...........",
    ".NNNNNnnNNNN............",
    ".NNNNNNNNNN.............",
    "RRNN.NNNNNN.............",
    "RR....NNNN..............",
    "......g..g..............",
    ".....gggggggg...........",
];

/// Tucán empujando un coco: ala adelante, pico abierto.
pub const TOUCAN_THROW: &[&str] = &[
    ".......NNNNN............",
    "......NNNNNNNN.....OOO..",
    ".....NNNNKKKKNN.OOOOOOOK",
    ".....NNNNUUUUKOOOOOOOYYK",
    ".....NNNUUKWUUYYYYYYY...",
    ".....NNNTUUUUYYY........",
    ".....NNTTTTTOOOOO.......",
    "....NNTTTTTTNOOOOOORRK..",
    "....NNTTTTTTNnRRRRRRK...",
    "...NNNTTTTTTNnn.........",
    "...NNNTTTTTTNnnnn.......",
    "..NNNNNNTTTRNnnnnn......",
    "..NNNNNNNNNNNN..........",
    ".NNNNNNNNNNN............",
    ".NNNNNNNNNN.............",
    "RRNN.NNNNNN.............",
    "RR....NNNN..............",
    "......g..g..............",
    ".....gggggggg...........",
];

/// Tucán volando (cuando el mono llega arriba, se va ofendido).
pub const TOUCAN_FLY: &[&str] = &[
    "...nnn..................",
    "..nnnnn.NNNNN...........",
    "...nnnnNNNNNNNN.........",
    "....nnnNNKKKKNN.........",
    ".....nNNNUUUUKOOOOO.....",
    ".....NNNUUKWUUYYYYYYY...",
    ".....NNNTUUUUYYYYYYYYYYK",
    ".....NNTTTTTOOOOOOOOORKK",
    "....NNTTTTTTNOOOOOORRK..",
    "....NNTTTTTTN.RRRRRR....",
    "...NNNTTTTTTN...........",
    "..NNNNNNTTTRN...........",
    ".RNNNNNNNNNN............",
    "RRNNNNNNNNN.............",
    "RR...NNNN...............",
];

pub const BANANA: &[&str] = &[
    "......DD", ".....DyD", "....DYYD", "...DYYYD", "..DYYYyD", "DDYYYyD.", "DyYyyD..", ".DDDD...",
];

/// El racimo que el tucán le robó al mono.
pub const BUNCH: &[&str] = &[
    "......DDD.....",
    "......DbD.....",
    "...DD.DyD.DD..",
    "..DYYDDyDDYYD.",
    ".DYYyDYYYDyYYD",
    ".DYYyDYYYDyYYD",
    "DYYyDYYYYYDyYD",
    "DYYyDYYyYYDyYD",
    "DYYyDYYyYYDyYD",
    "DYyDDYYyYYDDyD",
    ".DyD.DYyYD.DD.",
    "..D..DYyYD....",
    ".....DyyyD....",
    "......DDD.....",
];

/// Vidas en la barra de arriba.
pub const LIFE: &[&str] = &[
    "..DDDDD..",
    ".DBBBBBD.",
    "DFDFFFDFD",
    "DFFKFKFFD",
    ".DFFFFFD.",
    "..DPMPD..",
    "...DDD...",
];
