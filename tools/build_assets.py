"""Convierte un personaje exportado de Blender al formato del motor.

    python tools/build_assets.py                 # assets/mono.json -> assets/src/mono.rs
                                                 # assets/mono_anims.json -> assets/src/mono_anims.rs
    python tools/build_assets.py --preview DIR   # además, PNG de las texturas convertidas

Entrada: el JSON de `tools/blender/export_character.py` y las imágenes que
usan sus materiales. Salida: un módulo Rust con datos `static` (van a flash):

- malla en el sistema del motor: Y arriba, Z hacia delante (la cámara mira
  hacia +Z), 1 m = 1024 unidades Q8, con la normal de cada cara para la
  iluminación facetada;
- colores lisos (una entrada de la tabla de materiales por color);
- partes con textura (ojos, boca, logos): imagen reducida al tamaño que
  tiene en pantalla, 4 bits por texel y paleta propia de hasta 16 colores
  que apuntan a la tabla de materiales;
- esqueleto: nombre, padre y cabeza de cada hueso.

Los colores de las texturas parecidos a los de la malla se unifican, así un
texel "piel" usa exactamente la rampa de la piel y no se ven costuras.

Animaciones (de `tools/blender/process_anims.py`): rotación local de cada
hueso por cuadro en cuaterniones Q14 y desplazamiento de la cadera. Las
pistas que no cambian se guardan una vez, los huesos sin vértices no se
guardan, los dedos quedan fijos (a 240×240 la mano mide unos píxeles) y lo
que avanza el cuerpo en las animaciones que caminan se quita de la cadera y
se guarda aparte, para que el juego mueva al personaje.
"""

import argparse
import json
import math
import os
from collections import Counter

from PIL import Image

from build_nav import build_nav

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Unidades Q8 por metro.
SCALE = 1024
# Distancia RGB por debajo de la cual un color de textura reutiliza uno existente.
MERGE_DIST = 34
# Un texel con menos opacidad que esto (tras reducir) deja ver el color de la cara.
ALPHA_MIN = 64
TEX_COLORS = 16
MAX_MATERIALS = 32

# Animaciones: una pista es constante si nunca se aparta más que esto (grados).
CONST_DEG = 0.5
# Dedos fijos (pose del cuadro del medio de cada clip).
FIXED_FINGERS = True
FINGERS = ("Thumb", "Index", "Middle", "Ring", "Pinky")
# Avance horizontal por ciclo a partir del cual se separa del cuerpo (m).
DRIFT_MIN = 0.05

# Escenario: se corre el origen para que todo quepa en ±8 m (mallas en i16 Q8).
SCENE_ORIGIN = (2.9, 0.0, 0.0)
SCENE_PARTS = {
    "LaptopScreen": {"size": (64, 128), "colors": 8},
    "MonitorScreen": {"size": (64, 32), "colors": 6},
}

# Tamaño final de cada parte con textura. `cells`: tira vertical de
# expresiones (la UV de la malla apunta a la primera). `uv_cells`: celdas
# que tenía la tira cuando se hizo la UV en Blender, si después se le
# agregaron expresiones abajo (la UV se reescala para seguir en la primera).
PARTS = {
    "Eye": {"size": (32, 32 * 6), "cells": 6, "colors": 8},
    "Mouth": {"size": (64, 32 * 10), "cells": 10, "uv_cells": 6, "colors": 11},
    "WonderLogo": {"size": (32, 32), "colors": 3},
    "Pant": {"size": (32, 32), "colors": 3},
}


def next_pow2(n):
    return 1 << max(0, math.ceil(math.log2(max(1, n))))


def dist(a, b):
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))


def nearest(c, palette):
    return min(range(len(palette)), key=lambda i: dist(c, palette[i]))


def downscale(img, size):
    # Promedio con alfa premultiplicado: los bordes no se oscurecen.
    return img.convert("RGBa").resize(size, Image.Resampling.BOX).convert("RGBA")


def median_cut(pixels, k):
    if not pixels:
        return []
    im = Image.new("RGB", (len(pixels), 1))
    im.putdata(pixels)
    q = im.quantize(colors=min(k, len(set(pixels))), method=Image.Quantize.MEDIANCUT)
    pal = q.getpalette()
    used = sorted(q.getcolors(), reverse=True)
    return [tuple(pal[3 * i:3 * i + 3]) for _, i in used]


class Builder:
    def __init__(self, data, json_dir):
        self.data = data
        self.json_dir = json_dir
        self.colors = []      # tabla de colores lisos (material i)
        self.parts = []       # partes con textura

    def color_index(self, c, merge=False):
        c = tuple(c)
        if c in self.colors:
            return self.colors.index(c)
        if merge and self.colors:
            i = nearest(c, self.colors)
            if dist(c, self.colors[i]) < MERGE_DIST:
                return i
        self.colors.append(c)
        return len(self.colors) - 1

    def build(self):
        d = self.data
        mats = d["materials"]
        tris = d["tris"]

        # 1) Colores de la malla, del más usado al menos usado.
        for c, _ in Counter(tuple(t["color"]) for t in tris).most_common():
            self.color_index(c)

        # 2) Texturas.
        textured = {}
        for mi, m in enumerate(mats):
            if not m.get("image"):
                continue
            cfg = PARTS.get(m["name"])
            if cfg is None:
                raise SystemExit(f"falta la configuración de la parte con textura {m['name']!r} en PARTS")
            mtris = [t for t in tris if t["mat"] == mi]
            base = Counter(tuple(t["color"]) for t in mtris).most_common(1)[0][0]
            textured[mi] = self.build_part(m, cfg, mtris, base)

        n_mats = len(self.colors) + len(self.parts)
        if n_mats > MAX_MATERIALS:
            raise SystemExit(f"{n_mats} materiales: el motor admite {MAX_MATERIALS}")

        # 3) Vértices: se duplican solo donde una cara con textura necesita otra UV.
        verts = d["verts"]
        out_v, index = [], {}
        out_t = []
        for t in tris:
            part = textured.get(t["mat"])
            ids = []
            for k, vi in enumerate(t["v"]):
                uv = part["uv"](t["uv"][k]) if part else None
                key = (vi, uv)
                if key not in index:
                    index[key] = len(out_v)
                    out_v.append((verts[vi], uv))
                ids.append(index[key])
            if part:
                mat = len(self.colors) + part["slot"]
            else:
                mat = self.colors.index(tuple(t["color"]))
            # Cambiar Y por Z invierte la orientación: se da vuelta el orden.
            out_t.append(((ids[0], ids[2], ids[1]), mat, t["n"]))
        if len(out_v) > 65535:
            raise SystemExit("demasiados vértices")

        bones = d["bones"]
        for i, b in enumerate(bones):
            if b["parent"] is not None and b["parent"] >= i:
                raise SystemExit(f"el hueso {b['name']} aparece antes que su padre")
        return out_v, out_t, bones

    def build_part(self, m, cfg, mtris, base):
        path = m["image"]
        if not os.path.isabs(path):
            path = os.path.join(self.json_dir, path)
        img = downscale(Image.open(path).convert("RGBA"), cfg["size"])
        w, h = img.size
        # Alto de la imagen para la que se hizo la UV.
        uv_h = h * cfg.get("uv_cells", cfg.get("cells", 1)) // cfg.get("cells", 1)
        px = list(img.get_flattened_data() if hasattr(img, "get_flattened_data") else img.getdata())

        # Paleta: el color de la cara para lo transparente + los de la imagen.
        opaque = [p[:3] for p in px if p[3] >= ALPHA_MIN]
        palette = [base]
        for c in median_cut(opaque, cfg.get("colors", 6)):
            g = self.colors[self.color_index(c, merge=True)]
            if g not in palette:
                palette.append(g)
        if len(palette) > TEX_COLORS:
            raise SystemExit(f"{m['name']}: {len(palette)} colores")

        # Posición del contenido dentro de un lienzo potencia de 2. Si la UV se
        # sale de la imagen (modo Clip de Blender), el lienzo deja margen
        # transparente: el motor repite las texturas en vez de recortarlas.
        tx = lambda uv: (uv[0] * w, (1 - uv[1]) * uv_h)
        pts = [tx(uv) for t in mtris for uv in t["uv"]]
        x0, y0 = (0, 0)
        cw, ch = next_pow2(w), next_pow2(h)
        if m.get("extension") == "CLIP":
            minx = min(p[0] for p in pts); maxx = max(p[0] for p in pts)
            miny = min(p[1] for p in pts); maxy = max(p[1] for p in pts)
            left = max(0, math.ceil(-minx) + 1); right = max(0, math.ceil(maxx - w) + 1)
            top = max(0, math.ceil(-miny) + 1); bottom = max(0, math.ceil(maxy - h) + 1)
            cw = next_pow2(w + 2 * max(left, right))
            ch = next_pow2(h + 2 * max(top, bottom))
            x0, y0 = (cw - w) // 2, (ch - h) // 2
        elif m.get("extension") == "REPEAT" and (w != cw or h != ch):
            raise SystemExit(f"{m['name']}: una textura que se repite necesita lados potencia de 2")

        texels = [0] * (cw * ch)
        for y in range(h):
            for x in range(w):
                p = px[y * w + x]
                k = nearest(p[:3], palette) if p[3] >= ALPHA_MIN else 0
                texels[(y + y0) * cw + x + x0] = k

        slot = len(self.parts)
        cells = cfg.get("cells", 1)
        part = {
            "name": m["name"],
            "w": cw, "h": ch,
            "texels": texels,
            "colors": [self.colors.index(c) for c in palette],
            "palette": palette,
            "cells": cells,
            "cell_step": (0, h // cells) if cells > 1 else (0, 0),
            "slot": slot,
            # UV de Blender -> texels Q4 de este lienzo.
            "uv": lambda uv: (round((uv[0] * w + x0) * 16), round(((1 - uv[1]) * uv_h + y0) * 16)),
        }
        self.parts.append(part)
        return part


def q8(v):
    return [round(v[0] * SCALE), round(v[2] * SCALE), round(v[1] * SCALE)]


def n8(n):
    return [max(-127, min(127, round(c * 127))) for c in (n[0], n[2], n[1])]


def rows(items, per_line, fmt):
    lines = []
    for i in range(0, len(items), per_line):
        lines.append("    " + " ".join(fmt(x) for x in items[i:i + per_line]))
    return "\n".join(lines)


def pack(texels, w, h):
    out = []
    for i in range(0, w * h, 2):
        out.append(texels[i] | (texels[i + 1] << 4))
    return out


def emit(name, builder, verts, tris, bones, src):
    n, t = len(verts), len(tris)
    up = name.upper()
    o = []
    o.append(f"//! Personaje `{name}`, generado por `tools/build_assets.py` desde")
    o.append(f"//! `{src}`. No editar a mano: volver a exportar desde Blender.")
    o.append("")
    o.append("#![cfg_attr(rustfmt, rustfmt::skip)]")
    o.append("")
    o.append("use crate::{Bone, Character, TexturedPart};")
    o.append("use engine3d::{Mesh, Rgb, Texture};")
    o.append("")
    # La malla del personaje se lee en cada fotograma: en el ESP32 va en RAM
    # (desde flash, cada fallo de caché cuesta cientos de ciclos). Las normales
    # por vértice no se usan (iluminación por cara) y quedan en flash.
    o.append('#[cfg_attr(feature = "iram", unsafe(link_section = ".data"))]')
    o.append(f"static POSITIONS: [[i16; 3]; {n}] = [")
    o.append(rows([q8(v["co"]) for v, _ in verts], 6, lambda p: f"[{p[0]}, {p[1]}, {p[2]}],"))
    o.append("];")
    o.append(f"static NORMALS: [[i8; 3]; {n}] = [")
    o.append(rows([n8(v["n"]) for v, _ in verts], 8, lambda p: f"[{p[0]}, {p[1]}, {p[2]}],"))
    o.append("];")
    o.append('#[cfg_attr(feature = "iram", unsafe(link_section = ".data"))]')
    o.append(f"static BONES: [u8; {n}] = [")
    o.append(rows([v["bone"] or 0 for v, _ in verts], 24, lambda b: f"{b},"))
    o.append("];")
    o.append('#[cfg_attr(feature = "iram", unsafe(link_section = ".data"))]')
    o.append(f"static UVS: [[i16; 2]; {n}] = [")
    o.append(rows([uv or (0, 0) for _, uv in verts], 8, lambda p: f"[{p[0]}, {p[1]}],"))
    o.append("];")
    o.append('#[cfg_attr(feature = "iram", unsafe(link_section = ".data"))]')
    o.append(f"static TRIS: [[u16; 3]; {t}] = [")
    o.append(rows([tr[0] for tr in tris], 8, lambda p: f"[{p[0]}, {p[1]}, {p[2]}],"))
    o.append("];")
    o.append('#[cfg_attr(feature = "iram", unsafe(link_section = ".data"))]')
    o.append(f"static TRI_MATERIALS: [u8; {t}] = [")
    o.append(rows([tr[1] for tr in tris], 24, lambda m: f"{m},"))
    o.append("];")
    o.append('#[cfg_attr(feature = "iram", unsafe(link_section = ".data"))]')
    o.append(f"static TRI_NORMALS: [[i8; 3]; {t}] = [")
    o.append(rows([n8(tr[2]) for tr in tris], 8, lambda p: f"[{p[0]}, {p[1]}, {p[2]}],"))
    o.append("];")
    o.append("")
    o.append("/// Colores lisos: el material `i` es `COLORS[i]`.")
    o.append(f"static COLORS: [Rgb; {len(builder.colors)}] = [")
    o.append(rows(builder.colors, 4, lambda c: f"Rgb({c[0]}, {c[1]}, {c[2]}),"))
    o.append("];")
    for p in builder.parts:
        data = pack(p["texels"], p["w"], p["h"])
        o.append("")
        o.append(f"/// Textura `{p['name']}`: {p['w']}×{p['h']}, 4 bits por texel. En el ESP32 va en RAM.")
        o.append('#[cfg_attr(feature = "iram", unsafe(link_section = ".data"))]')
        o.append(f"static TEX_{p['name'].upper()}: [u8; {len(data)}] = [")
        o.append(rows(data, 16, lambda b: f"0x{b:02x},"))
        o.append("];")
    o.append("")
    o.append("/// Partes con textura: la `j` es el material `COLORS.len() + j`.")
    o.append(f"static PARTS: [TexturedPart; {len(builder.parts)}] = [")
    for p in builder.parts:
        colors = p["colors"] + [p["colors"][0]] * (TEX_COLORS - len(p["colors"]))
        o.append("    TexturedPart {")
        o.append(f"        name: \"{p['name']}\",")
        o.append(f"        texture: Texture::new(&TEX_{p['name'].upper()}, {int(math.log2(p['w']))}, {int(math.log2(p['h']))}),")
        o.append(f"        colors: [{', '.join(str(c) for c in colors)}],")
        o.append(f"        cell_step: [{p['cell_step'][0]}, {p['cell_step'][1]}],")
        o.append(f"        cells: {p['cells']},")
        o.append("    },")
    o.append("];")
    o.append("")
    o.append(f"static SKELETON: [Bone; {len(bones)}] = [")
    for b in bones:
        parent = "None" if b["parent"] is None else f"Some({b['parent']})"
        h = q8(b["head"])
        o.append(f"    Bone {{ name: \"{b['name']}\", parent: {parent}, head: [{h[0]}, {h[1]}, {h[2]}] }},")
    o.append("];")
    o.append("")
    o.append(f"pub static {up}: Character = Character {{")
    o.append("    mesh: Mesh {")
    o.append("        positions: &POSITIONS,")
    o.append("        normals: &NORMALS,")
    o.append("        bones: &BONES,")
    o.append("        uvs: &UVS,")
    o.append("        tris: &TRIS,")
    o.append("        tri_normals: &TRI_NORMALS,")
    o.append("        tri_materials: &TRI_MATERIALS,")
    o.append("    },")
    o.append("    colors: &COLORS,")
    o.append("    parts: &PARTS,")
    o.append("    bones: &SKELETON,")
    o.append("};")
    o.append("")
    return "\n".join(o)


def quat_to_engine(q):
    """Cuaternión en ejes de Blender (Z arriba) -> ejes del motor (Y arriba).
    Cambiar Y por Z es una reflexión: el eje se refleja y el giro se invierte."""
    w, x, y, z = q
    return (w, -x, -z, -y)


def q_angle(a, b):
    d = min(1.0, abs(sum(x * y for x, y in zip(a, b))))
    return math.degrees(2 * math.acos(d))


def build_anims(anims, mono):
    bones = mono["bones"]
    names = [b["name"] for b in bones]
    if anims["bones"] != names:
        raise SystemExit("las animaciones son de otro esqueleto: volver a correr process_anims.py")
    nb = len(bones)
    # Huesos que mueven algo: tienen vértices o algún descendiente los tiene.
    needed = {v["bone"] for v in mono["verts"] if v["bone"] is not None}
    for i in reversed(range(nb)):
        if i in needed and bones[i]["parent"] is not None:
            needed.add(bones[i]["parent"])
    finger = [FIXED_FINGERS and any(f in n.split(":")[-1] for f in FINGERS) for n in names]
    clips = []
    for c in anims["clips"]:
        # Las transiciones ("idle_to_push_up", "..._from_...") no se repiten.
        c["loop"] = c["loop"] and "_to_" not in c["name"] and "from" not in c["name"]
        frames = c["frames"]
        n = len(frames)
        roots = [list(f["root"]) for f in frames]
        advance = [0.0, 0.0, 0.0]
        if c["loop"] and n > 1:
            d = [roots[-1][k] - roots[0][k] for k in range(3)]
            if math.hypot(d[0], d[1]) > DRIFT_MIN:
                v = [d[0] / (n - 1), d[1] / (n - 1)]
                for f, r in enumerate(roots):
                    r[0] -= v[0] * f
                    r[1] -= v[1] * f
                advance = [v[0] * n, v[1] * n, 0.0]
        pool, tracks = [], []
        for b in range(nb):
            qs = [quat_to_engine(f["q"][b]) for f in frames]
            if b not in needed:
                tracks.append((0, 0))
                continue
            if finger[b]:
                qs = [qs[n // 2]]
            elif all(q_angle(qs[0], q) < CONST_DEG for q in qs):
                qs = [qs[0]]
            tracks.append((len(pool), len(qs)))
            pool.extend(tuple(max(-32767, min(32767, round(x * 16384))) for x in q) for q in qs)
        clips.append({
            "name": c["name"],
            "fps": c["fps"],
            "frames": n,
            "loop": c["loop"],
            "advance": q8(advance),
            "pool": pool,
            "tracks": tracks,
            "root": [q8(r) for r in roots],
        })
    return clips


def emit_anims(name, clips, src):
    o = []
    o.append(f"//! Animaciones de `{name}`, generadas por `tools/build_assets.py` desde")
    o.append(f"//! `{src}`. No editar a mano: volver a procesarlas.")
    o.append("")
    o.append("#![cfg_attr(rustfmt, rustfmt::skip)]")
    o.append("")
    o.append("use crate::anim::{Clip, Track};")
    for c in clips:
        up = c["name"].upper()
        o.append("")
        o.append(f"static POOL_{up}: [[i16; 4]; {len(c['pool'])}] = [")
        o.append(rows(c["pool"], 6, lambda q: f"[{q[0]}, {q[1]}, {q[2]}, {q[3]}],"))
        o.append("];")
        o.append(f"static TRACKS_{up}: [Track; {len(c['tracks'])}] = [")
        o.append(rows(c["tracks"], 4, lambda t: f"Track {{ start: {t[0]}, len: {t[1]} }},"))
        o.append("];")
        o.append(f"static ROOT_{up}: [[i16; 3]; {len(c['root'])}] = [")
        o.append(rows(c["root"], 8, lambda r: f"[{r[0]}, {r[1]}, {r[2]}],"))
        o.append("];")
    o.append("")
    o.append(f"pub static CLIPS: [Clip; {len(clips)}] = [")
    for c in clips:
        up = c["name"].upper()
        a = c["advance"]
        o.append(f"    Clip {{ name: \"{c['name']}\", fps: {c['fps']}, frames: {c['frames']}, "
                 f"looping: {'true' if c['loop'] else 'false'}, advance: [{a[0]}, {a[1]}, {a[2]}], "
                 f"pool: &POOL_{up}, tracks: &TRACKS_{up}, root: &ROOT_{up} }},")
    o.append("];")
    o.append("")
    o.append("/// Índice de cada clip en [`CLIPS`].")
    o.append("pub mod clip {")
    for i, c in enumerate(clips):
        o.append(f"    pub const {c['name'].upper()}: usize = {i};")
    o.append("}")
    o.append("")
    o.append("/// El clip con ese nombre.")
    o.append("pub fn find(name: &str) -> Option<&'static Clip> {")
    o.append("    CLIPS.iter().find(|c| c.name == name)")
    o.append("}")
    o.append("")
    return "\n".join(o)


# ---------------------------------------------------------------------------
# Escenario (hábitat).
# ---------------------------------------------------------------------------
def q8s(v):
    """Punto del escenario (Blender) -> motor, con el origen corrido."""
    x, y, z = v[0] - SCENE_ORIGIN[0], v[1] - SCENE_ORIGIN[1], v[2] - SCENE_ORIGIN[2]
    return [round(x * SCALE), round(z * SCALE), round(y * SCALE)]


def quat_from_matrix(m):
    """Cuaternión (w, x, y, z) de una matriz de rotación 3×3 (filas)."""
    tr = m[0][0] + m[1][1] + m[2][2]
    if tr > 0:
        s = math.sqrt(tr + 1.0) * 2
        return (0.25 * s, (m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s)
    if m[0][0] > m[1][1] and m[0][0] > m[2][2]:
        s = math.sqrt(1.0 + m[0][0] - m[1][1] - m[2][2]) * 2
        return ((m[2][1] - m[1][2]) / s, 0.25 * s, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s)
    if m[1][1] > m[2][2]:
        s = math.sqrt(1.0 + m[1][1] - m[0][0] - m[2][2]) * 2
        return ((m[0][2] - m[2][0]) / s, (m[0][1] + m[1][0]) / s, 0.25 * s, (m[1][2] + m[2][1]) / s)
    s = math.sqrt(1.0 + m[2][2] - m[0][0] - m[1][1]) * 2
    return ((m[1][0] - m[0][1]) / s, (m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, 0.25 * s)


def matrix_from_quat(q):
    w, x, y, z = q
    return [[1 - 2 * (y * y + z * z), 2 * (x * y - w * z), 2 * (x * z + w * y)],
            [2 * (x * y + w * z), 1 - 2 * (x * x + z * z), 2 * (y * z - w * x)],
            [2 * (x * z - w * y), 2 * (y * z + w * x), 1 - 2 * (x * x + y * y)]]


# Los objetos fijos grandes se parten en trozos de hasta tantos triángulos,
# cada uno con su esfera: así la cámara (y el redibujado parcial) descartan
# lo que no se ve en vez de transformar el objeto entero.
CHUNK_TRIS = 80


def bounding_sphere(pts):
    lo = [min(p[k] for p in pts) for k in range(3)]
    hi = [max(p[k] for p in pts) for k in range(3)]
    c = [(lo[k] + hi[k]) // 2 for k in range(3)]
    return c, math.ceil(max(math.dist(c, p) for p in pts)) + 1


def split_chunks(verts, tris, limit=CHUNK_TRIS):
    """Parte los triángulos por la mediana del eje más largo hasta que cada
    trozo tenga como mucho `limit`. Devuelve [(verts, tris)] con los índices
    rehechos para cada trozo."""
    def centroid(t):
        return [sum(verts[i][0][k] for i in t[0]) / 3 for k in range(3)]

    def rec(ts):
        if len(ts) <= limit:
            return [ts]
        cs = [centroid(t) for t in ts]
        ext = [max(c[k] for c in cs) - min(c[k] for c in cs) for k in range(3)]
        axis = ext.index(max(ext))
        order = sorted(range(len(ts)), key=lambda i: cs[i][axis])
        half = len(ts) // 2
        return rec([ts[i] for i in order[:half]]) + rec([ts[i] for i in order[half:]])

    out = []
    for ts in rec(tris):
        index, vs, nts = {}, [], []
        for ids, mat, n in ts:
            new = []
            for i in ids:
                if i not in index:
                    index[i] = len(vs)
                    vs.append(verts[i])
                new.append(index[i])
            nts.append((tuple(new), mat, n))
        out.append((vs, nts))
    return out


def build_scene(data, json_dir):
    b = Builder({"materials": [], "tris": [], "verts": [], "bones": []}, json_dir)
    objects = data["objects"]
    # 1) Colores lisos, del más usado al menos usado.
    counts = Counter(tuple(t["color"]) for o in objects for t in o["tris"]
                     if t["mat"] < 0 or not o["materials"][t["mat"]].get("image"))
    for c, _ in counts.most_common():
        b.color_index(c)
    # 2) Partes con textura (pantallas), una por material.
    parts = {}
    for o in objects:
        for mi, m in enumerate(o["materials"]):
            if not m.get("image") or m["name"] in parts:
                continue
            cfg = SCENE_PARTS.get(m["name"])
            if cfg is None:
                raise SystemExit(f"falta la configuración de {m['name']!r} en SCENE_PARTS")
            mtris = [t for oo in objects for t in oo["tris"]
                     if t["mat"] >= 0 and oo["materials"][t["mat"]]["name"] == m["name"]]
            base = Counter(tuple(t["color"]) for t in mtris).most_common(1)[0][0]
            parts[m["name"]] = b.build_part(m, cfg, mtris, base)
    # 3) Objetos.
    out = []
    for o in objects:
        # Los objetos para las manos tienen el origen en su punto de agarre:
        # no se les corre el origen del escenario.
        conv = q8 if o["kind"] == "prop" else q8s
        verts, index, tris = [], {}, []
        for t in o["tris"]:
            m = o["materials"][t["mat"]] if t["mat"] >= 0 else None
            part = parts.get(m["name"]) if m and m.get("image") else None
            ids = []
            for k, vi in enumerate(t["v"]):
                uv = part["uv"](t["uv"][k]) if part else None
                key = (vi, uv)
                if key not in index:
                    index[key] = len(verts)
                    verts.append((conv(o["verts"][vi]), uv))
                ids.append(index[key])
            mat = ("part", part["slot"]) if part else ("color", b.colors.index(tuple(t["color"])))
            tris.append(((ids[0], ids[2], ids[1]), mat, n8(t["n"])))
        for p, _ in verts:
            if max(abs(x) for x in p) >= 8192:
                raise SystemExit(f"{o['name']}: coordenadas fuera de ±8 m: mover SCENE_ORIGIN")
        chunks = split_chunks(verts, tris) if o["kind"] == "static" else [(verts, tris)]
        for k, (vs, ts) in enumerate(chunks):
            c, r = bounding_sphere([v for v, _ in vs])
            sym = o["name"].upper() + (f"_{k}" if len(chunks) > 1 else "")
            out.append({"name": o["name"], "sym": sym, "kind": o["kind"], "verts": vs, "tris": ts,
                        "center": c, "radius": r})
    n_colors = len(b.colors)
    for o in out:
        o["tris"] = [(v, n_colors + m[1] if m[0] == "part" else m[1], nn) for v, m, nn in o["tris"]]
    # 4) Lugares, puntos de paso y cámaras.
    spots = [{"name": s["name"], "pos": q8s(s["pos"]), "yaw": round(-s["yaw"])} for s in data["spots"]]
    wps = [{"name": s["name"], "pos": q8s(s["pos"]), "yaw": round(-s["yaw"])} for s in data["waypoints"]]
    cams = []
    for c in data["cameras"]:
        rb = matrix_from_quat(c["rot"])
        col = lambda k: [rb[0][k], rb[1][k], rb[2][k]]
        flip = lambda v: [v[0], v[2], v[1]]
        right, up, fwd = flip(col(0)), flip(col(1)), flip([-x for x in col(2)])
        re = [[right[i], up[i], fwd[i]] for i in range(3)]
        q = quat_from_matrix(re)
        cams.append({"name": c["name"], "pos": q8s(c["pos"]), "rot": [round(x * 16384) for x in q],
                     "focal": round(c["lens"] / c["sensor"] * 240)})
    return b, out, spots, wps, cams


def emit_nav(nav):
    """Grilla de bits, nodos y vecinos (en ejes del motor) y asientos."""
    o = []
    w, h = nav["w"], nav["h"]
    bits = bytearray((w * h + 7) // 8)
    for j in range(h):
        for i in range(w):
            if nav["blocked"][j][i]:
                k = j * w + i
                bits[k // 8] |= 1 << (k % 8)

    def xz(p):
        return [round((p[0] - SCENE_ORIGIN[0]) * 1024), round((p[1] - SCENE_ORIGIN[1]) * 1024)]

    adj = [[] for _ in nav["nodes"]]
    for a, b in nav["edges"]:
        adj[a].append(b)
        adj[b].append(a)
    first, links = [0], []
    for nb in adj:
        links += sorted(nb)
        first.append(len(links))
    o.append("")
    o.append(f"/// Celdas bloqueadas ({w}×{h}, {nav['cell'] * 100:.0f} cm), un bit cada una.")
    o.append(f"static NAV_BLOCKED: [u8; {len(bits)}] = [")
    o.append(rows(list(bits), 16, lambda x: f"0x{x:02x},"))
    o.append("];")
    o.append(f"static NAV_NODES: [[i32; 2]; {len(nav['nodes'])}] = [")
    o.append(rows([xz(p) for p in nav["nodes"]], 6, lambda p: f"[{p[0]}, {p[1]}],"))
    o.append("];")
    o.append(f"static NAV_FIRST: [u16; {len(first)}] = [")
    o.append(rows(first, 16, lambda x: f"{x},"))
    o.append("];")
    o.append(f"static NAV_LINKS: [u16; {len(links)}] = [")
    o.append(rows(links, 16, lambda x: f"{x},"))
    o.append("];")
    x0 = xz((nav["x0"], nav["y0"]))
    o.append("static NAV: NavMap = NavMap {")
    o.append(f"    x0: {x0[0]},")
    o.append(f"    z0: {x0[1]},")
    o.append(f"    per_m: {round(1 / nav['cell'])},")
    o.append(f"    w: {w},")
    o.append(f"    h: {h},")
    o.append("    blocked: &NAV_BLOCKED,")
    o.append("    nodes: &NAV_NODES,")
    o.append("    first: &NAV_FIRST,")
    o.append("    links: &NAV_LINKS,")
    o.append("};")
    o.append(f"static SEATS: [Seat; {len(nav['seats'])}] = [")
    for st in nav["seats"]:
        chair = f'Some("{st["chair"]}")' if st["chair"] else "None"
        ap = xz(st["approach"])
        o.append(f'    Seat {{ spot: "{st["name"]}", chair: {chair}, pull: {round(st["pull"] * 1024)}, '
                 f'approach: [{ap[0]}, {ap[1]}] }},')
    o.append("];")
    return o


def emit_scene(name, b, objects, spots, wps, cams, src, nav):
    o = []
    o.append(f"//! Escenario `{name}`, generado por `tools/build_assets.py` desde")
    o.append(f"//! `{src}`. No editar a mano: volver a exportar desde Blender.")
    o.append("")
    o.append("#![cfg_attr(rustfmt, rustfmt::skip)]")
    o.append("")
    o.append("use crate::TexturedPart;")
    o.append("use crate::nav::NavMap;")
    o.append("use crate::world::{CameraView, Kind, Seat, Spot, World, WorldObject};")
    o.append("use engine3d::{Mesh, Rgb, Texture};")
    for ob in objects:
        up = ob["sym"]
        n, t = len(ob["verts"]), len(ob["tris"])
        o.append("")
        o.append(f"static {up}_POS: [[i16; 3]; {n}] = [")
        o.append(rows([v for v, _ in ob["verts"]], 6, lambda p: f"[{p[0]}, {p[1]}, {p[2]}],"))
        o.append("];")
        if any(uv for _, uv in ob["verts"]):
            o.append(f"static {up}_UV: [[i16; 2]; {n}] = [")
            o.append(rows([uv or (0, 0) for _, uv in ob["verts"]], 8, lambda p: f"[{p[0]}, {p[1]}],"))
            o.append("];")
        o.append(f"static {up}_TRIS: [[u16; 3]; {t}] = [")
        o.append(rows([tr[0] for tr in ob["tris"]], 8, lambda p: f"[{p[0]}, {p[1]}, {p[2]}],"))
        o.append("];")
        o.append(f"static {up}_MAT: [u8; {t}] = [")
        o.append(rows([tr[1] for tr in ob["tris"]], 24, lambda m: f"{m},"))
        o.append("];")
        o.append(f"static {up}_NRM: [[i8; 3]; {t}] = [")
        o.append(rows([tr[2] for tr in ob["tris"]], 8, lambda p: f"[{p[0]}, {p[1]}, {p[2]}],"))
        o.append("];")
    o.append("")
    o.append(f"static COLORS: [Rgb; {len(b.colors)}] = [")
    o.append(rows(b.colors, 4, lambda c: f"Rgb({c[0]}, {c[1]}, {c[2]}),"))
    o.append("];")
    for p in b.parts:
        data = pack(p["texels"], p["w"], p["h"])
        o.append("")
        o.append(f"/// Textura `{p['name']}`: {p['w']}×{p['h']}, 4 bits por texel. En el ESP32 va en RAM.")
        o.append('#[cfg_attr(feature = "iram", unsafe(link_section = ".data"))]')
        o.append(f"static TEX_{p['name'].upper()}: [u8; {len(data)}] = [")
        o.append(rows(data, 16, lambda x: f"0x{x:02x},"))
        o.append("];")
    o.append("")
    o.append(f"static PARTS: [TexturedPart; {len(b.parts)}] = [")
    for p in b.parts:
        colors = p["colors"] + [p["colors"][0]] * (TEX_COLORS - len(p["colors"]))
        o.append("    TexturedPart {")
        o.append(f"        name: \"{p['name']}\",")
        o.append(f"        texture: Texture::new(&TEX_{p['name'].upper()}, {int(math.log2(p['w']))}, {int(math.log2(p['h']))}),")
        o.append(f"        colors: [{', '.join(str(c) for c in colors)}],")
        o.append("        cell_step: [0, 0],")
        o.append("        cells: 1,")
        o.append("    },")
    o.append("];")
    o.append("")
    o.append(f"static OBJECTS: [WorldObject; {len(objects)}] = [")
    for ob in objects:
        up = ob["sym"]
        kind = ob["kind"]
        k = "Kind::Static" if kind == "static" else "Kind::Prop" if kind == "prop" else f"Kind::State(\"{kind.split(':', 1)[1]}\")"
        uv = f"&{up}_UV" if any(u for _, u in ob["verts"]) else "&[]"
        c = ob["center"]
        o.append(f"    WorldObject {{ name: \"{ob['name']}\", kind: {k}, center: [{c[0]}, {c[1]}, {c[2]}], radius: {ob['radius']},")
        o.append(f"        mesh: Mesh {{ positions: &{up}_POS, normals: &[], bones: &[], uvs: {uv}, tris: &{up}_TRIS, "
                 f"tri_normals: &{up}_NRM, tri_materials: &{up}_MAT }} }},")
    o.append("];")
    o.append("")
    for label, items in (("SPOTS", spots), ("WAYPOINTS", wps)):
        o.append(f"static {label}: [Spot; {len(items)}] = [")
        for sp in items:
            p = sp["pos"]
            o.append(f"    Spot {{ name: \"{sp['name']}\", pos: [{p[0]}, {p[1]}, {p[2]}], yaw: {sp['yaw']} }},")
        o.append("];")
    o.append(f"static CAMERAS: [CameraView; {len(cams)}] = [")
    for c in cams:
        p, q = c["pos"], c["rot"]
        o.append(f"    CameraView {{ name: \"{c['name']}\", pos: [{p[0]}, {p[1]}, {p[2]}], "
                 f"rot: [{q[0]}, {q[1]}, {q[2]}, {q[3]}], focal: {c['focal']} }},")
    o.append("];")
    o.append("")
    o += emit_nav(nav)
    o.append("")
    o.append(f"pub static {name.upper()}: World = World {{")
    o.append("    objects: &OBJECTS,")
    o.append("    colors: &COLORS,")
    o.append("    parts: &PARTS,")
    o.append("    spots: &SPOTS,")
    o.append("    waypoints: &WAYPOINTS,")
    o.append("    cameras: &CAMERAS,")
    o.append("    nav: &NAV,")
    o.append("    seats: &SEATS,")
    o.append("};")
    o.append("")
    return "\n".join(o)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", default=os.path.join(ROOT, "assets", "mono.json"))
    ap.add_argument("--out", default=os.path.join(ROOT, "assets", "src", "mono.rs"))
    ap.add_argument("--anims", default=os.path.join(ROOT, "assets", "mono_anims.json"))
    ap.add_argument("--anims-out", default=os.path.join(ROOT, "assets", "src", "mono_anims.rs"))
    ap.add_argument("--scene", default=os.path.join(ROOT, "assets", "house.json"))
    ap.add_argument("--scene-out", default=os.path.join(ROOT, "assets", "src", "house.rs"))
    ap.add_argument("--preview", help="carpeta donde guardar PNG de las texturas convertidas")
    args = ap.parse_args()

    data = json.load(open(args.json, encoding="utf-8"))
    name = os.path.splitext(os.path.basename(args.json))[0]
    b = Builder(data, os.path.dirname(args.json))
    verts, tris, bones = b.build()
    src = os.path.relpath(args.json, ROOT).replace("\\", "/")
    with open(args.out, "w", encoding="utf-8", newline="\n") as f:
        f.write(emit(name, b, verts, tris, bones, src))

    print(f"{name}: {len(verts)} vértices, {len(tris)} triángulos, {len(bones)} huesos")
    print(f"  colores lisos: {len(b.colors)} {b.colors}")
    for p in b.parts:
        print(f"  {p['name']}: {p['w']}×{p['h']}, {len(p['palette'])} colores -> materiales {p['colors']}")
        if args.preview:
            os.makedirs(args.preview, exist_ok=True)
            im = Image.new("RGB", (p["w"], p["h"]))
            im.putdata([p["palette"][k] for k in p["texels"]])
            im.resize((p["w"] * 4, p["h"] * 4), Image.Resampling.NEAREST).save(
                os.path.join(args.preview, f"{name}_{p['name']}.png"))
    print(f"  -> {os.path.relpath(args.out, ROOT)}")

    if os.path.exists(args.scene):
        sdata = json.load(open(args.scene, encoding="utf-8"))
        sname = os.path.splitext(os.path.basename(args.scene))[0]
        sb, sobjs, spots, wps, cams = build_scene(sdata, os.path.dirname(args.scene))
        nav = build_nav(sdata)
        if nav["lost"]:
            raise SystemExit(f"navegación: lugares sin conexión: {nav['lost']} (ver tools/nav_preview.py)")
        ssrc = os.path.relpath(args.scene, ROOT).replace("\\", "/")
        with open(args.scene_out, "w", encoding="utf-8", newline="\n") as f:
            f.write(emit_scene(sname, sb, sobjs, spots, wps, cams, ssrc, nav))
        ntris = sum(len(o["tris"]) for o in sobjs)
        nverts = sum(len(o["verts"]) for o in sobjs)
        print(f"{sname}: {len(sobjs)} objetos, {nverts} vértices, {ntris} triángulos, "
              f"{len(sb.colors)} colores, {len(sb.parts)} texturas, {len(spots)} lugares, {len(cams)} cámaras")
        print(f"  navegación: {nav['w']}×{nav['h']} celdas, {len(nav['nodes'])} nodos, {len(nav['edges'])} aristas")
        for p in sb.parts:
            print(f"  {p['name']}: {p['w']}×{p['h']}, {len(p['palette'])} colores")
            if args.preview:
                im = Image.new("RGB", (p["w"], p["h"]))
                im.putdata([p["palette"][k] for k in p["texels"]])
                im.resize((p["w"] * 4, p["h"] * 4), Image.Resampling.NEAREST).save(
                    os.path.join(args.preview, f"{sname}_{p['name']}.png"))
        print(f"  -> {os.path.relpath(args.scene_out, ROOT)}")

    if os.path.exists(args.anims):
        anims = json.load(open(args.anims, encoding="utf-8"))
        # Animaciones generadas con cinemática inversa (tools/make_clips.py).
        extra = os.path.join(os.path.dirname(args.anims), name + "_anims_extra.json")
        if os.path.exists(extra):
            more = json.load(open(extra, encoding="utf-8"))
            if more["bones"] != anims["bones"]:
                raise SystemExit("las animaciones extra son de otro esqueleto: correr tools/make_clips.py")
            anims["clips"] += more["clips"]
        clips = build_anims(anims, data)
        asrc = os.path.relpath(args.anims, ROOT).replace("\\", "/")
        with open(args.anims_out, "w", encoding="utf-8", newline="\n") as f:
            f.write(emit_anims(name, clips, asrc))
        total = 0
        for c in clips:
            size = len(c["pool"]) * 8 + len(c["root"]) * 6 + len(c["tracks"]) * 8
            total += size
            moving = sum(1 for t in c["tracks"] if t[1] > 1)
            extra = f", avanza {c['advance']}" if any(c["advance"]) else ""
            print(f"  {c['name']}: {c['frames']} cuadros, {moving} huesos animados, {size / 1024:.1f} KB{extra}")
        print(f"  animaciones: {len(clips)}, {total / 1024:.0f} KB -> {os.path.relpath(args.anims_out, ROOT)}")


if __name__ == "__main__":
    main()
