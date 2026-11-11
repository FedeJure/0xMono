"""Modela el hábitat del mono: casa en corte (dormitorio, oficina de
programador, cocina-comedor, rincón de lectura) y jardín con un bananero.

    blender -b --factory-startup --python tools/blender/build_house.py -- <salida.blend> [carpeta_previews]

Low poly con bordes biselados, color plano por cara (atributo `Col`, como el
mono) y paleta acotada. Medidas en metros, Z arriba, la cámara mira hacia +Y
(el frente abierto de la casa está en y = 0). El mono mide 1 m.

Las alturas salen de las animaciones (`tools/measure_anims.py`):
- sentado, la cola apoya a 0,10 m: asientos a 0,10;
- tecleando, las manos quedan a 0,25-0,265 m y 0,21-0,235 m delante de la
  cadera, y las rodillas llegan a 0,15 m: escritorio y mesa a 0,235 m con el
  borde a 0,17 m de la cadera;
- acostado, el cuerpo baja 0,207 m bajo su origen: colchón a 0,20 m y el
  personaje se apoya 0,407 m arriba.

Objetos:
- `kind = "static"`: escenario.
- `kind = "prop"`: los mueve el juego (banana en la mano, regadera, libro).
- `kind = "state:<nombre>"`: se muestran según el estado (manta estirada o
  tapándolo, banana colgando del árbol o ya cosechada).

Empties `spot_*`: lugares del personaje (posición de sus pies y hacia dónde
mira: rotación Z, con 0 = mirando a la cámara). `wp_*`: puntos de paso para
caminar. Cámaras `cam_*`: una por zona y `cam_house` general.
"""

import math
import os
import sys

import bmesh
import bpy
from mathutils import Euler, Matrix, Vector

# ---------------------------------------------------------------------------
# Paleta (sRGB). Pocos colores bien elegidos: madera cálida, paredes crema,
# acentos azul petróleo y terracota, verdes de plantas.
# ---------------------------------------------------------------------------
PAL = {
    "wall": (238, 228, 208),
    "wainscot": (150, 170, 146),
    "trim": (247, 243, 234),
    "floor_a": (184, 132, 88),
    "floor_b": (166, 116, 76),
    "wood_dark": (104, 68, 44),
    "wood": (150, 102, 64),
    "wood_light": (204, 162, 116),
    "teal": (56, 132, 136),
    "teal_dark": (36, 92, 98),
    "navy": (52, 70, 108),
    "terracotta": (196, 104, 70),
    "cream": (243, 236, 220),
    "white": (246, 246, 242),
    "metal_dark": (54, 58, 68),
    "metal": (176, 182, 190),
    "black": (28, 30, 36),
    "glass": (168, 214, 236),
    # Pantallas de las lámparas: el juego las "prende" de noche.
    "lamp": (250, 232, 190),
    "screen": (16, 22, 34),
    "green": (84, 158, 86),
    "green_dark": (48, 112, 66),
    "green_light": (150, 196, 92),
    "grass": (116, 176, 84),
    "grass_dark": (94, 150, 70),
    "soil": (110, 78, 54),
    "stone": (172, 168, 158),
    "banana": (244, 208, 64),
    "banana_green": (170, 190, 70),
    "trunk": (128, 140, 74),
    "flower_red": (214, 74, 74),
    "flower_purple": (128, 86, 160),
    "orange": (240, 148, 44),
    "roof": (152, 72, 58),
    "hill": (104, 150, 90),
    "hill_far": (132, 170, 128),
}

# Desplazamientos útiles.
WALL_H = 2.0
BACK_Y = 2.6
LEFT_X = -3.6
RIGHT_X = 3.6


def M(loc=(0, 0, 0), rot=(0, 0, 0), scale=(1, 1, 1)):
    r = Euler([math.radians(a) for a in rot], "XYZ").to_matrix().to_4x4()
    return Matrix.Translation(Vector(loc)) @ r @ Matrix.Diagonal((*scale, 1.0))


# ---------------------------------------------------------------------------
# Construcción de objetos: cada uno acumula vértices, caras, color por cara
# y (opcional) UV por cara.
# ---------------------------------------------------------------------------
class Obj:
    def __init__(self, name, kind="static"):
        self.name = name
        self.kind = kind
        self.v = []
        self.f = []
        self.c = []
        self.uv = []
        self.material = None

    def add(self, bm, color, m=Matrix(), uv=None):
        bm.verts.index_update()
        base = len(self.v)
        for v in bm.verts:
            self.v.append(m @ v.co)
        for face in bm.faces:
            self.f.append([base + v.index for v in face.verts])
            self.c.append(PAL[color])
            self.uv.append(uv(face) if uv else None)
        bm.free()
        return self


OBJS = []


def obj(name, kind="static"):
    o = Obj(name, kind)
    OBJS.append(o)
    return o


def cube(sx, sy, sz, bevel=0.0, seg=1):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    for v in bm.verts:
        v.co = Vector((v.co.x * sx, v.co.y * sy, v.co.z * sz))
    if bevel > 0 and min(sx, sy, sz) >= 0.03 and max(sx, sy, sz) >= 0.12:
        b = min(bevel, 0.45 * min(sx, sy, sz))
        bmesh.ops.bevel(bm, geom=bm.edges[:], offset=b, segments=seg, profile=0.5,
                        affect="EDGES", clamp_overlap=True)
    return bm


def B(o, x0, x1, y0, y1, z0, z1, color, bevel=0.0, seg=1, rot=(0, 0, 0)):
    """Caja entre dos esquinas (rotación alrededor de su centro)."""
    bm = cube(x1 - x0, y1 - y0, z1 - z0, bevel, seg)
    c = ((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2)
    return o.add(bm, color, M(c, rot))


def cyl(r, h, seg=8, r_top=None, caps=True):
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=caps, cap_tris=False, segments=seg, radius1=r,
                          radius2=r if r_top is None else r_top, depth=h)
    for v in bm.verts:
        v.co.z += h / 2
    return bm


def C(o, x, y, z, r, h, color, seg=8, r_top=None, rot=(0, 0, 0)):
    """Cilindro (o cono) con la base en (x, y, z)."""
    return o.add(cyl(r, h, seg, r_top), color, M((x, y, z), rot))


def ico(r, sub=1):
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=sub, radius=r)
    return bm


def S(o, x, y, z, r, color, sub=1, scale=(1, 1, 1)):
    return o.add(ico(r, sub), color, M((x, y, z), (0, 0, 0), scale))


def double_sided(bm):
    dup = bmesh.ops.duplicate(bm, geom=bm.faces[:] + bm.edges[:] + bm.verts[:])
    faces = [g for g in dup["geom"] if isinstance(g, bmesh.types.BMFace)]
    bmesh.ops.reverse_faces(bm, faces=faces)
    return bm


def quad(pts):
    bm = bmesh.new()
    vs = [bm.verts.new(p) for p in pts]
    bm.faces.new(vs)
    return bm


def poly(n, r, z=0.0):
    """Polígono plano de n lados (mirando hacia +Z)."""
    bm = bmesh.new()
    vs = [bm.verts.new((r * math.cos(2 * math.pi * i / n), r * math.sin(2 * math.pi * i / n), z))
          for i in range(n)]
    bm.faces.new(vs)
    return bm


# ---------------------------------------------------------------------------
# Piezas orgánicas: bananas, hojas.
# ---------------------------------------------------------------------------
def banana_bm(length=0.17, radius=0.018, bend=40.0, sides=5, segs=3):
    """Banana curva: tubo de `sides` lados (las aristas de la fruta) que se
    afina en las puntas, sobre un arco en el plano XZ."""
    bm = bmesh.new()
    rings = []
    ang = math.radians(bend)
    R = length / ang
    for i in range(segs + 1):
        t = i / segs
        a = -ang / 2 + ang * t
        center = Vector((R * math.sin(a), 0, R * (1 - math.cos(a)) - R * (1 - math.cos(ang / 2))))
        tangent = Vector((math.cos(a), 0, math.sin(a)))
        normal = Vector((-math.sin(a), 0, math.cos(a)))
        side = Vector((0, 1, 0))
        rad = radius * (0.35 + 0.65 * math.sin(math.pi * min(1, max(0, (t * 0.9 + 0.05)))))
        ring = []
        for k in range(sides):
            phi = 2 * math.pi * k / sides
            ring.append(bm.verts.new(center + (normal * math.cos(phi) + side * math.sin(phi)) * rad))
        rings.append((ring, center, tangent))
    for i in range(segs):
        a, b = rings[i][0], rings[i + 1][0]
        for k in range(sides):
            bm.faces.new((a[k], a[(k + 1) % sides], b[(k + 1) % sides], b[k]))
    # Puntas.
    tip0 = bm.verts.new(rings[0][1] - rings[0][2] * radius * 0.8)
    tip1 = bm.verts.new(rings[-1][1] + rings[-1][2] * radius * 1.4)
    for k in range(sides):
        r0 = rings[0][0]
        bm.faces.new((tip0, r0[(k + 1) % sides], r0[k]))
        r1 = rings[-1][0]
        bm.faces.new((tip1, r1[k], r1[(k + 1) % sides]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    return bm


def leaf_bm(length, width, droop=0.5, segs=6, fold=0.15, curl=0.0):
    """Hoja grande de bananero: nervio central curvado que cae hacia la
    punta, con el limbo plegado en V. A lo largo de +X, doble cara."""
    bm = bmesh.new()
    mid, left, right = [], [], []
    for i in range(segs + 1):
        t = i / segs
        x = length * t
        z = -droop * length * t * t
        w = width * math.sin(math.pi * min(1.0, 0.12 + 0.95 * t)) * (1.0 - 0.15 * t)
        y_curl = curl * length * t * t
        m = Vector((x, y_curl, z))
        mid.append(bm.verts.new(m))
        left.append(bm.verts.new(m + Vector((0, w, -fold * w))))
        right.append(bm.verts.new(m + Vector((0, -w, -fold * w))))
    for i in range(segs):
        bm.faces.new((mid[i], left[i], left[i + 1], mid[i + 1]))
        bm.faces.new((mid[i], mid[i + 1], right[i + 1], right[i]))
    return double_sided(bm)


def blade_bm(h, w, lean=0.1):
    """Hoja angosta y alta (sansevieria, pasto), doble cara."""
    bm = bmesh.new()
    a = bm.verts.new((-w / 2, 0, 0))
    b = bm.verts.new((w / 2, 0, 0))
    c = bm.verts.new((w / 3 + lean * h, 0, h * 0.6))
    d = bm.verts.new((lean * h * 1.6, 0, h))
    e = bm.verts.new((-w / 3 + lean * h, 0, h * 0.6))
    bm.faces.new((a, b, c, e))
    bm.faces.new((e, c, d))
    return double_sided(bm)


# ---------------------------------------------------------------------------
# Casa.
# ---------------------------------------------------------------------------
def wall_x(o, y0, y1, x0, x1, holes, z_split=0.55):
    """Pared a lo largo de X (entre y0 e y1) con huecos (x0, x1, z0, z1):
    zócalo de color abajo y crema arriba."""
    cuts = sorted({x0, x1, *[h[0] for h in holes], *[h[1] for h in holes]})
    for a, b in zip(cuts, cuts[1:]):
        hole = next((h for h in holes if h[0] <= a and b <= h[1]), None)
        spans = [(0.0, WALL_H)] if hole is None else [(0.0, hole[2]), (hole[3], WALL_H)]
        for z0, z1 in spans:
            if z1 - z0 < 1e-4:
                continue
            if z0 < z_split < z1:
                B(o, a, b, y0, y1, z0, z_split, "wainscot")
                B(o, a, b, y0, y1, z_split, z1, "wall")
            else:
                B(o, a, b, y0, y1, z0, z1, "wainscot" if z1 <= z_split else "wall")


def wall_y(o, x0, x1, y0, y1, holes, z_split=0.55):
    """Pared a lo largo de Y (entre x0 y x1) con huecos (y0, y1, z0, z1)."""
    cuts = sorted({y0, y1, *[h[0] for h in holes], *[h[1] for h in holes]})
    for a, b in zip(cuts, cuts[1:]):
        hole = next((h for h in holes if h[0] <= a and b <= h[1]), None)
        spans = [(0.0, WALL_H)] if hole is None else [(0.0, hole[2]), (hole[3], WALL_H)]
        for z0, z1 in spans:
            if z1 - z0 < 1e-4:
                continue
            if z0 < z_split < z1:
                B(o, x0, x1, a, b, z0, z_split, "wainscot")
                B(o, x0, x1, a, b, z_split, z1, "wall")
            else:
                B(o, x0, x1, a, b, z0, z1, "wainscot" if z1 <= z_split else "wall")


def window_back(o, x0, x1, z0, z1, y=BACK_Y):
    """Ventana en la pared del fondo: marco, travesaños, vidrio y alféizar."""
    t = 0.035
    B(o, x0 - t, x1 + t, y - 0.02, y + 0.03, z1, z1 + t, "trim", 0.006)
    B(o, x0 - t, x1 + t, y - 0.02, y + 0.03, z0 - t, z0, "trim", 0.006)
    B(o, x0 - t, x0, y - 0.02, y + 0.03, z0, z1, "trim", 0.006)
    B(o, x1, x1 + t, y - 0.02, y + 0.03, z0, z1, "trim", 0.006)
    xm, zm = (x0 + x1) / 2, (z0 + z1) / 2
    B(o, xm - 0.012, xm + 0.012, y, y + 0.02, z0, z1, "trim")
    B(o, x0, x1, y, y + 0.02, zm - 0.012, zm + 0.012, "trim")
    B(o, x0, x1, y + 0.04, y + 0.05, z0, z1, "glass")
    B(o, x0 - 0.06, x1 + 0.06, y - 0.07, y, z0 - t - 0.025, z0 - t, "trim", 0.008)


def curtains(o, x0, x1, z0, z1, color, y=BACK_Y - 0.06):
    """Cortinas plegadas a los costados de una ventana (zigzag, doble cara)."""
    for side in (-1, 1):
        xa = x0 - 0.16 if side < 0 else x1 + 0.02
        bm = bmesh.new()
        n = 6
        top, bot = [], []
        for i in range(n + 1):
            x = xa + 0.14 * i / n
            yy = y + (0.025 if i % 2 else 0.0)
            top.append(bm.verts.new((x, yy, z1 + 0.08)))
            bot.append(bm.verts.new((x + (0.01 if side > 0 else -0.01), yy, z0 - 0.12)))
        for i in range(n):
            bm.faces.new((bot[i], bot[i + 1], top[i + 1], top[i]))
        o.add(double_sided(bm), color)
    B(o, x0 - 0.2, x1 + 0.2, y - 0.01, y + 0.01, z1 + 0.09, z1 + 0.11, "wood_dark")
    C(o, x0 - 0.21, y, z1 + 0.085, 0.015, 0.03, "wood_dark", 6)
    C(o, x1 + 0.21, y, z1 + 0.085, 0.015, 0.03, "wood_dark", 6)


def build_shell():
    floor = obj("floor")
    # Tablas a lo largo de X, de largos variados, en dos tonos.
    import random
    rnd = random.Random(4)
    y = 0.0
    row = 0
    while y < BACK_Y - 1e-6:
        y1 = min(BACK_Y, y + 0.16)
        x = LEFT_X + rnd.uniform(-0.6, 0.0)
        k = row
        while x < RIGHT_X:
            x1 = min(RIGHT_X, x + rnd.uniform(0.9, 1.5))
            x0 = max(LEFT_X, x)
            if x1 - x0 > 0.02:
                floor.add(quad([(x0, y, 0), (x1, y, 0), (x1, y1, 0), (x0, y1, 0)]),
                          "floor_a" if k % 2 == 0 else "floor_b")
            x = x1
            k += 1
        y = y1
        row += 1
    B(floor, LEFT_X - 0.1, RIGHT_X + 0.1, -0.06, BACK_Y + 0.1, -0.1, -0.001, "wood_dark")

    walls = obj("walls")
    wall_x(walls, BACK_Y, BACK_Y + 0.1, LEFT_X - 0.1, RIGHT_X + 0.1,
           [(-3.05, -2.25, 0.85, 1.5), (2.05, 2.7, 0.85, 1.45)])
    wall_y(walls, LEFT_X - 0.1, LEFT_X, 0.0, BACK_Y, [])
    wall_y(walls, RIGHT_X, RIGHT_X + 0.1, 0.0, BACK_Y, [(1.15, 1.95, 0.0, 1.5)])
    # Molduras: zócalo blanco y guardasilla.
    for (x0, x1) in ((LEFT_X, -3.05), (-2.25, 2.05), (2.7, RIGHT_X)):
        B(walls, x0, x1, BACK_Y - 0.015, BACK_Y, 0.0, 0.07, "trim")
    B(walls, LEFT_X, RIGHT_X, BACK_Y - 0.02, BACK_Y, 0.54, 0.57, "trim", 0.005)
    B(walls, LEFT_X, LEFT_X + 0.015, 0.0, BACK_Y, 0.0, 0.07, "trim")
    B(walls, LEFT_X, LEFT_X + 0.02, 0.0, BACK_Y, 0.54, 0.57, "trim", 0.005)
    for (y0, y1) in ((0.0, 1.15), (1.95, BACK_Y)):
        B(walls, RIGHT_X - 0.015, RIGHT_X, y0, y1, 0.0, 0.07, "trim")
        B(walls, RIGHT_X - 0.02, RIGHT_X, y0, y1, 0.54, 0.57, "trim", 0.005)
    window_back(walls, -3.05, -2.25, 0.85, 1.5)
    window_back(walls, 2.05, 2.7, 0.85, 1.45)

    # Puerta al jardín (abierta hacia adentro) con su marco.
    door = obj("door")
    t = 0.04
    B(door, RIGHT_X - 0.03, RIGHT_X + 0.13, 1.15 - t, 1.15, 0.0, 1.5 + t, "trim", 0.006)
    B(door, RIGHT_X - 0.03, RIGHT_X + 0.13, 1.95, 1.95 + t, 0.0, 1.5 + t, "trim", 0.006)
    B(door, RIGHT_X - 0.03, RIGHT_X + 0.13, 1.15 - t, 1.95 + t, 1.5, 1.5 + t, "trim", 0.006)
    leaf = Obj("tmp")
    B(leaf, 0.0, 0.79, -0.02, 0.02, 0.0, 1.48, "teal", 0.008)
    for zz in (0.15, 0.75):
        B(leaf, 0.08, 0.71, -0.03, -0.02, zz, zz + 0.5, "teal_dark", 0.004)
    C(leaf, 0.68, -0.035, 0.72, 0.02, 0.03, "metal", 6, rot=(90, 0, 0))
    hinge = Vector((RIGHT_X + 0.12, 1.95, 0.0))
    m = M(hinge, (0, 0, 88))
    for v, f, c in zip([None], [None], [None]):
        pass
    base = len(door.v)
    door.v.extend(m @ v for v in leaf.v)
    door.f.extend([[base + i for i in f] for f in leaf.f])
    door.c.extend(leaf.c)
    door.uv.extend(leaf.uv)

    # Marco del "cuadro": viga superior y columnas al frente, techo y tejado.
    frame = obj("frame")
    B(frame, LEFT_X - 0.12, RIGHT_X + 0.12, -0.08, 0.06, WALL_H, WALL_H + 0.14, "wood_dark", 0.012)
    for x in (LEFT_X - 0.12, RIGHT_X + 0.02):
        B(frame, x, x + 0.1, -0.08, 0.04, -0.1, WALL_H, "wood_dark", 0.012)
    B(frame, LEFT_X - 0.1, RIGHT_X + 0.1, 0.0, BACK_Y + 0.1, WALL_H, WALL_H + 0.06, "trim")
    roof = obj("roof")
    ridge_y, ridge_z = BACK_Y / 2, WALL_H + 0.75
    for (ya, yb) in ((-0.25, ridge_y), (BACK_Y + 0.35, ridge_y)):
        dy, dz = yb - ya, ridge_z - (WALL_H + 0.1)
        ang = math.degrees(math.atan2(dz, abs(dy)))
        length = math.hypot(dy, dz)
        cy = (ya + yb) / 2
        cz = (WALL_H + 0.1 + ridge_z) / 2
        rot = (ang if dy > 0 else -ang, 0, 0)
        roof.add(cube(RIGHT_X - LEFT_X + 0.5, length, 0.07, 0.01), "roof", M((0, cy, cz), rot))
    # Exterior de la pared derecha: tablas de madera horizontales.
    ext = obj("exterior")
    for i in range(8):
        z0 = i * 0.26
        B(ext, RIGHT_X + 0.1, RIGHT_X + 0.125, -0.02, 1.15 - 0.04, z0, z0 + 0.25, "cream")
        B(ext, RIGHT_X + 0.1, RIGHT_X + 0.125, 1.95 + 0.04, BACK_Y + 0.12, z0, z0 + 0.25, "cream")
        if z0 >= 1.5:
            B(ext, RIGHT_X + 0.1, RIGHT_X + 0.125, 1.15 - 0.04, 1.95 + 0.04, z0, z0 + 0.25, "cream")


# ---------------------------------------------------------------------------
# Dormitorio.
# ---------------------------------------------------------------------------
def build_bedroom():
    bed = obj("bed")
    x0, x1, y0, y1 = -3.25, -2.05, 1.45, 2.25
    for (lx, ly) in ((x0 + 0.03, y0 + 0.03), (x1 - 0.06, y0 + 0.03), (x0 + 0.03, y1 - 0.06), (x1 - 0.06, y1 - 0.06)):
        B(bed, lx, lx + 0.03, ly, ly + 0.03, 0.0, 0.07, "wood_dark")
    B(bed, x0, x1, y0, y1, 0.06, 0.13, "wood", 0.01)
    B(bed, x0 + 0.03, x1 - 0.03, y0 + 0.03, y1 - 0.03, 0.13, 0.20, "white", 0.02)
    B(bed, x0 - 0.07, x0, y0 - 0.02, y1 + 0.02, 0.0, 0.58, "wood", 0.015)
    B(bed, x0 - 0.075, x0 + 0.005, y0 - 0.025, y1 + 0.025, 0.58, 0.62, "wood_dark", 0.012)
    for k in range(4):
        yy = y0 + 0.08 + k * 0.17
        B(bed, x0 - 0.002, x0 + 0.004, yy, yy + 0.12, 0.25, 0.5, "wood_dark")
    B(bed, x1, x1 + 0.05, y0 - 0.02, y1 + 0.02, 0.0, 0.30, "wood", 0.012)
    B(bed, -3.22, -2.96, 1.56, 2.14, 0.20, 0.265, "cream", 0.025)
    flat = obj("blanket_flat", "state:awake")
    B(flat, -2.92, x1 - 0.01, y0 + 0.01, y1 - 0.01, 0.20, 0.225, "navy", 0.008)
    B(flat, -2.98, -2.86, y0 + 0.005, y1 - 0.005, 0.20, 0.232, "cream", 0.008)
    sleep = obj("blanket_sleep", "state:asleep")
    B(sleep, -2.84, x1 - 0.01, y0 - 0.02, y1 + 0.02, 0.20, 0.50, "navy", 0.07, 2)
    B(sleep, -2.92, -2.80, y0 - 0.025, y1 + 0.025, 0.40, 0.52, "cream", 0.035, 2)

    rug = obj("rug_bedroom")
    B(rug, -3.45, -1.85, 0.75, 2.45, 0.0, 0.006, "terracotta")
    B(rug, -3.35, -1.95, 0.85, 2.35, 0.006, 0.009, "cream")
    B(rug, -3.30, -2.0, 0.90, 2.30, 0.009, 0.011, "terracotta")

    ns = obj("nightstand")
    B(ns, -1.98, -1.70, 2.14, 2.48, 0.0, 0.30, "wood", 0.01)
    B(ns, -1.95, -1.73, 2.13, 2.14, 0.17, 0.28, "wood_light", 0.004)
    B(ns, -1.95, -1.73, 2.13, 2.14, 0.03, 0.15, "wood_light", 0.004)
    for zz in (0.225, 0.09):
        C(ns, -1.84, 2.125, zz, 0.01, 0.012, "metal", 6, rot=(90, 0, 0))
    C(ns, -1.80, 2.32, 0.30, 0.045, 0.02, "metal_dark", 8)
    C(ns, -1.80, 2.32, 0.32, 0.008, 0.15, "metal_dark", 6)
    C(ns, -1.80, 2.32, 0.43, 0.075, 0.1, "lamp", 8, r_top=0.045)
    B(ns, -1.94, -1.86, 2.20, 2.25, 0.30, 0.36, "terracotta", 0.01)
    B(ns, -1.93, -1.87, 2.195, 2.20, 0.31, 0.35, "cream")

    win = obj("bedroom_window_curtains")
    curtains(win, -3.05, -2.25, 0.85, 1.5, "teal")

    art = obj("bedroom_art")
    # Cuadro en la pared izquierda: paisaje con bananero.
    B(art, LEFT_X, LEFT_X + 0.03, 0.85, 1.45, 0.95, 1.35, "wood_dark", 0.008)
    B(art, LEFT_X + 0.03, LEFT_X + 0.035, 0.89, 1.41, 0.99, 1.31, "glass")
    B(art, LEFT_X + 0.035, LEFT_X + 0.04, 0.89, 1.41, 0.99, 1.11, "green")
    B(art, LEFT_X + 0.035, LEFT_X + 0.045, 1.22, 1.30, 1.18, 1.26, "banana")
    B(art, LEFT_X + 0.04, LEFT_X + 0.05, 1.00, 1.03, 1.08, 1.22, "trunk")
    for a in (-40, 30, 100, 160):
        art.add(leaf_bm(0.09, 0.025, 0.4, 3), "green_dark",
                M((LEFT_X + 0.05, 1.015, 1.22), (0, 0, 0)) @ M((0, 0, 0), (a, 0, 90)))

    gh = obj("guitar_home", "state:guitar_home")
    # Parada sobre el costado del cuerpo, el mástil hacia arriba y el frente
    # hacia la habitación, apoyada en la pared.
    guitar(gh, M((LEFT_X + 0.08, 1.15, 0.16), (0, 0, 90)) @ M((0, 0, 0), (0, -78, 0)) @ M(scale=(GUITAR,) * 3))

    plant = obj("plant_snake")
    C(plant, -3.42, 0.35, 0.0, 0.07, 0.13, "terracotta", 8, r_top=0.085)
    C(plant, -3.42, 0.35, 0.125, 0.075, 0.005, "soil", 8)
    import random
    rnd = random.Random(2)
    for i in range(7):
        a = i * 360 / 7
        h = rnd.uniform(0.28, 0.42)
        plant.add(blade_bm(h, 0.045, rnd.uniform(0.04, 0.12)), "green_dark" if i % 2 else "green",
                  M((-3.42 + 0.03 * math.cos(math.radians(a)), 0.35 + 0.03 * math.sin(math.radians(a)), 0.12),
                    (0, 0, a)))


# ---------------------------------------------------------------------------
# Oficina del programador.
# ---------------------------------------------------------------------------
DESK_SEAT = (-0.55, 1.55)
TABLE_SEAT = (1.75, 1.40)
ARMCHAIR_SEAT = (3.12, 1.02)
DESK_H = 0.235
SEAT_H = 0.10


def chair_office(o, sx, sy):
    B(o, sx - 0.14, sx + 0.14, sy - 0.12, sy + 0.14, SEAT_H - 0.035, SEAT_H, "black", 0.012)
    C(o, sx, sy + 0.01, 0.03, 0.014, SEAT_H - 0.065, "metal", 6)
    for k in range(5):
        a = 90 + k * 72
        dx, dy = math.cos(math.radians(a)), math.sin(math.radians(a))
        o.add(cube(0.13, 0.022, 0.014), "metal_dark",
              M((sx + dx * 0.065, sy + 0.01 + dy * 0.065, 0.022), (0, 0, a)))
        C(o, sx + dx * 0.13, sy + 0.01 + dy * 0.13, 0.0, 0.012, 0.015, "black", 6)
    B(o, sx - 0.015, sx + 0.015, sy + 0.14, sy + 0.16, SEAT_H - 0.02, SEAT_H + 0.08, "metal_dark")
    B(o, sx - 0.13, sx + 0.13, sy + 0.15, sy + 0.19, SEAT_H + 0.06, SEAT_H + 0.36, "black", 0.02)


def chair_wood(o, sx, sy, rot=0):
    m = M((sx, sy, 0), (0, 0, rot))
    t = Obj("tmp")
    B(t, -0.14, 0.14, -0.12, 0.14, SEAT_H - 0.03, SEAT_H, "wood_light", 0.008)
    for lx, ly in ((-0.12, -0.10), (0.10, -0.10), (-0.12, 0.12), (0.10, 0.12)):
        B(t, lx, lx + 0.022, ly, ly + 0.022, 0.0, SEAT_H - 0.03, "wood")
    for lx in (-0.12, 0.10):
        B(t, lx, lx + 0.022, 0.12, 0.142, SEAT_H, SEAT_H + 0.34, "wood")
    for zz in (SEAT_H + 0.20, SEAT_H + 0.30):
        B(t, -0.12, 0.122, 0.122, 0.14, zz, zz + 0.05, "wood", 0.006)
    base = len(o.v)
    o.v.extend(m @ v for v in t.v)
    o.f.extend([[base + i for i in f] for f in t.f])
    o.c.extend(t.c)
    o.uv.extend(t.uv)


def books(o, x0, x1, y0, y1, z, rnd, colors=("teal", "terracotta", "navy", "wood_light", "cream", "green_dark", "orange")):
    x = x0
    while x < x1 - 0.02:
        w = rnd.uniform(0.024, 0.042)
        h = rnd.uniform(0.11, 0.17)
        lean = 0 if rnd.random() < 0.85 else rnd.choice((-8, 8))
        B(o, x, x + w, y0, y1, z, z + h, rnd.choice(colors), rot=(0, lean, 0))
        x += w + 0.003


def screen_uv(v0, v1):
    """UV de una cara de pantalla: la mitad de arriba de la textura."""
    def uv(face):
        n = len(face.verts)
        base = [(0, v0), (1, v0), (1, v1), (0, v1)]
        return base[:n]
    return uv


def build_office():
    import random
    rnd = random.Random(9)
    sx, sy = DESK_SEAT
    desk = obj("desk")
    dy0, dy1 = sy - 0.17 - 0.42, sy - 0.17
    B(desk, sx - 0.5, sx + 0.5, dy0, dy1, DESK_H - 0.02, DESK_H, "wood_light", 0.008)
    for lx in (sx - 0.48, sx + 0.45):
        for ly in (dy0 + 0.02, dy1 - 0.05):
            B(desk, lx, lx + 0.03, ly, ly + 0.03, 0.0, DESK_H - 0.02, "wood")
    B(desk, sx + 0.25, sx + 0.47, dy0 + 0.03, dy1 - 0.03, 0.0, DESK_H - 0.02, "wood", 0.006)
    for zz in (0.02, 0.115):
        B(desk, sx + 0.265, sx + 0.455, dy0 + 0.02, dy0 + 0.03, zz, zz + 0.085, "wood_light", 0.004)
        C(desk, sx + 0.36, dy0 + 0.015, zz + 0.04, 0.008, 0.01, "metal", 6, rot=(90, 0, 0))
    # Taza, cuaderno, suculenta y lámpara.
    C(desk, sx + 0.33, dy0 + 0.12, DESK_H, 0.024, 0.05, "white", 8)
    C(desk, sx + 0.33, dy0 + 0.12, DESK_H + 0.047, 0.021, 0.004, "wood_dark", 8)
    B(desk, sx + 0.352, sx + 0.364, dy0 + 0.114, dy0 + 0.126, DESK_H + 0.012, DESK_H + 0.04, "white", 0.003)
    B(desk, sx + 0.12, sx + 0.25, dy0 + 0.06, dy0 + 0.24, DESK_H, DESK_H + 0.012, "terracotta", 0.003, rot=(0, 0, 8))
    B(desk, sx + 0.125, sx + 0.245, dy0 + 0.065, dy0 + 0.235, DESK_H + 0.012, DESK_H + 0.014, "cream", rot=(0, 0, 8))
    C(desk, sx - 0.42, dy0 + 0.08, DESK_H, 0.035, 0.05, "terracotta", 8, r_top=0.04)
    for i in range(6):
        a = i * 60
        desk.add(cyl(0.012, 0.035, 5, 0.0), "green_light" if i % 2 else "green",
                 M((sx - 0.42 + 0.015 * math.cos(math.radians(a)), dy0 + 0.08 + 0.015 * math.sin(math.radians(a)),
                    DESK_H + 0.045), (math.cos(math.radians(a)) * 25, math.sin(math.radians(a)) * -25, 0)))
    lamp_x, lamp_y = sx + 0.42, dy1 - 0.08
    C(desk, lamp_x, lamp_y, DESK_H, 0.04, 0.012, "metal_dark", 8)
    desk.add(cube(0.012, 0.012, 0.2), "metal_dark", M((lamp_x, lamp_y - 0.02, DESK_H + 0.09), (-18, 0, 0)))
    desk.add(cube(0.012, 0.012, 0.16), "metal_dark", M((lamp_x, lamp_y - 0.08, DESK_H + 0.23), (55, 0, 0)))
    desk.add(cyl(0.035, 0.06, 8, 0.015), "orange", M((lamp_x, lamp_y - 0.15, DESK_H + 0.2), (180 + 30, 0, 0)))

    chair = obj("desk_chair")
    chair_office(chair, sx, sy)

    lap = obj("laptop")
    lx0, lx1 = sx - 0.13, sx + 0.13
    ly0, ly1 = sy - 0.35, sy - 0.17
    B(lap, lx0, lx1, ly0, ly1, DESK_H, DESK_H + 0.012, "metal", 0.003)
    B(lap, lx0 + 0.015, lx1 - 0.015, ly0 + 0.04, ly1 - 0.035, DESK_H + 0.012, DESK_H + 0.0125, "metal_dark")
    B(lap, sx - 0.035, sx + 0.035, ly1 - 0.03, ly1 - 0.008, DESK_H + 0.012, DESK_H + 0.0125, "metal_dark")
    # Tapa: se abre hacia la cámara (la pantalla mira al mono, la tapa a la cámara).
    hinge = (sx, ly0 + 0.005, DESK_H + 0.012)
    lid = M(hinge, (12, 0, 0))
    lap.add(cube(0.26, 0.008, 0.17, 0.003), "metal", lid @ M((0, 0, 0.085)))
    # Sticker en la tapa: un hexágono naranja (bloque de una cadena).
    lap.add(poly(6, 0.03), "orange", lid @ M((0, -0.0045, 0.09), (90, 0, 0)))
    scr = obj("laptop_screen", "static")
    scr.material = ("LaptopScreen", os.path.join("house", "laptop_code.png"))
    scr.add(quad([(-0.118, 0.0045, 0.01), (0.118, 0.0045, 0.01), (0.118, 0.0045, 0.16), (-0.118, 0.0045, 0.16)]),
            "screen", lid, uv=screen_uv(0.5, 1.0))
    # La cara tiene que mirar a +Y (hacia el mono).
    scr.f = [list(reversed(f)) for f in scr.f]
    scr.uv = [list(reversed(u)) if u else None for u in scr.uv]

    mon = obj("monitor")
    mx, my = sx - 0.32, dy0 + 0.17
    mrot = (0, 0, 28)
    mm = M((mx, my, DESK_H), mrot)
    mon.add(cube(0.14, 0.09, 0.008, 0.003), "metal_dark", mm @ M((0, 0, 0.004)))
    mon.add(cube(0.025, 0.02, 0.13), "metal_dark", mm @ M((0, 0.01, 0.07)))
    mon.add(cube(0.34, 0.022, 0.21, 0.006), "black", mm @ M((0, 0.0, 0.24)))
    ms = obj("monitor_screen", "static")
    ms.material = ("MonitorScreen", os.path.join("house", "monitor_chart.png"))
    ms.add(quad([(0.16, -0.0115, 0.145), (-0.16, -0.0115, 0.145), (-0.16, -0.0115, 0.335), (0.16, -0.0115, 0.335)]),
           "screen", mm, uv=screen_uv(0.0, 1.0))

    shelf = obj("office_shelves")
    for zz in (0.95, 1.25):
        B(shelf, -1.05, -0.1, BACK_Y - 0.2, BACK_Y, zz - 0.025, zz, "wood", 0.006)
        for bx in (-1.0, -0.17):
            B(shelf, bx, bx + 0.02, BACK_Y - 0.12, BACK_Y, zz - 0.09, zz - 0.025, "metal_dark")
    books(shelf, -1.03, -0.55, BACK_Y - 0.17, BACK_Y - 0.03, 0.95, rnd)
    books(shelf, -0.6, -0.4, BACK_Y - 0.17, BACK_Y - 0.03, 1.25, rnd)
    # Billetera de hardware, moneda de trofeo y un router con luces.
    C(shelf, -0.25, BACK_Y - 0.1, 0.95, 0.035, 0.015, "wood_dark", 8)
    C(shelf, -0.25, BACK_Y - 0.1, 0.965, 0.045, 0.012, "orange", 10, rot=(90, 0, 0))
    B(shelf, -1.0, -0.78, BACK_Y - 0.16, BACK_Y - 0.04, 1.25, 1.29, "black", 0.006)
    for k in range(4):
        B(shelf, -0.97 + k * 0.03, -0.96 + k * 0.03, BACK_Y - 0.161, BACK_Y - 0.159, 1.265, 1.275,
          "green_light" if k % 2 == 0 else "orange")
    C(shelf, -0.36, BACK_Y - 0.1, 1.25, 0.035, 0.06, "white", 8, r_top=0.04)
    for i in range(5):
        a = i * 72
        shelf.add(leaf_bm(0.09, 0.02, 0.9, 3), "green",
                  M((-0.36, BACK_Y - 0.1, 1.31), (0, -20, a)))

    poster = obj("blockchain_poster")
    px0, px1, pz0, pz1 = -0.95, -0.25, 1.38, 1.8
    B(poster, px0, px1, BACK_Y - 0.012, BACK_Y, pz0, pz1, "navy", 0.004)
    B(poster, px0 + 0.02, px1 - 0.02, BACK_Y - 0.013, BACK_Y - 0.012, pz0 + 0.02, pz1 - 0.02, "screen")
    cols = ("orange", "teal", "green_light", "orange")
    for k in range(4):
        cx = px0 + 0.1 + k * 0.165
        cz = pz0 + 0.21 + (0.03 if k % 2 else -0.02)
        poster.add(cube(0.075, 0.03, 0.075, 0.008), cols[k], M((cx, BACK_Y - 0.03, cz), (0, 0, 0)))
        poster.add(cube(0.05, 0.031, 0.012), "screen", M((cx, BACK_Y - 0.031, cz + 0.015)))
        if k < 3:
            nz = pz0 + 0.21 + (-0.02 if k % 2 else 0.03)
            poster.add(cube(0.1, 0.01, 0.01), "cream",
                       M((cx + 0.0825, BACK_Y - 0.025, (cz + nz) / 2), (0, math.degrees(math.atan2(cz - nz, 0.165)), 0)))

    case = obj("bookcase")
    bx0, bx1 = 0.05, 0.42
    B(case, bx0, bx1, BACK_Y - 0.28, BACK_Y, 0.0, 1.32, "wood", 0.01)
    B(case, bx0 + 0.02, bx1 - 0.02, BACK_Y - 0.281, BACK_Y - 0.27, 0.02, 1.30, "wood_dark")
    for k in range(5):
        zz = 0.04 + k * 0.255
        B(case, bx0 + 0.02, bx1 - 0.02, BACK_Y - 0.285, BACK_Y - 0.02, zz, zz + 0.02, "wood", 0.004)
        if k < 4:
            books(case, bx0 + 0.03, bx1 - 0.04, BACK_Y - 0.25, BACK_Y - 0.06, zz + 0.02, rnd)

    rug = obj("rug_office")
    B(rug, -1.3, 0.2, 0.85, 2.05, 0.0, 0.006, "teal_dark")
    B(rug, -1.22, 0.12, 0.93, 1.97, 0.006, 0.008, "teal")

    plant = obj("plant_monstera")
    C(plant, -1.45, 2.25, 0.0, 0.11, 0.2, "white", 8, r_top=0.12)
    C(plant, -1.45, 2.25, 0.195, 0.115, 0.006, "soil", 8)
    for i in range(7):
        a = i * 51 + 10
        tilt = rnd.uniform(-35, -10)
        L = rnd.uniform(0.32, 0.48)
        stem = M((-1.45, 2.25, 0.2), (0, 0, a)) @ M((0, 0, 0), (0, tilt - 30, 0))
        plant.add(cube(0.012, 0.012, L), "green_dark", stem @ M((0, 0, L / 2)))
        plant.add(leaf_bm(0.2, 0.09, 0.35, 4, 0.05), "green" if i % 2 else "green_dark",
                  stem @ M((0, 0, L), (0, 40, 0)))


# ---------------------------------------------------------------------------
# Cocina y comedor.
# ---------------------------------------------------------------------------
def build_kitchen():
    import random
    rnd = random.Random(5)
    fr = obj("fridge")
    B(fr, 0.62, 1.12, 2.06, BACK_Y - 0.02, 0.0, 1.15, "white", 0.03)
    B(fr, 0.63, 1.11, 2.055, 2.06, 0.74, 0.755, "metal")
    for z0, z1 in ((0.78, 1.08), (0.15, 0.68)):
        B(fr, 1.04, 1.06, 2.03, 2.055, z0, z1, "metal", 0.005)
    for k, col in enumerate(("orange", "teal", "flower_red")):
        B(fr, 0.72 + k * 0.08, 0.78 + k * 0.08, 2.054, 2.056, 0.9 - k * 0.04, 0.96 - k * 0.04, col)

    ctr = obj("counter")
    cx0, cx1, cy0 = 1.15, 2.85, 2.14
    B(ctr, cx0, cx1, cy0, BACK_Y, 0.0, 0.43, "teal", 0.006)
    B(ctr, cx0, cx1, cy0 + 0.02, BACK_Y, 0.0, 0.05, "teal_dark")
    for k in range(4):
        dx0 = cx0 + 0.02 + k * 0.42
        B(ctr, dx0, dx0 + 0.4, cy0 - 0.012, cy0, 0.07, 0.41, "teal", 0.006)
        B(ctr, dx0 + 0.18, dx0 + 0.22, cy0 - 0.03, cy0 - 0.012, 0.36, 0.375, "metal", 0.004)
    B(ctr, cx0 - 0.02, cx1 + 0.02, cy0 - 0.03, BACK_Y, 0.43, 0.46, "stone", 0.006)
    # Horno con puerta de vidrio bajo las hornallas.
    B(ctr, 1.21, 1.55, cy0 - 0.014, cy0 - 0.012, 0.12, 0.33, "black")
    for k, (bx, by) in enumerate(((1.28, 2.27), (1.46, 2.27), (1.28, 2.45), (1.46, 2.45))):
        C(ctr, bx, by, 0.46, 0.055 if k < 2 else 0.045, 0.006, "black", 8)
    # Olla en la hornalla de adelante a la izquierda (donde cocina).
    # Bien adelante: los brazos del mono son cortos.
    C(ctr, 1.28, 2.22, 0.466, 0.058, 0.065, "metal_dark", 10)
    C(ctr, 1.28, 2.22, 0.52, 0.05, 0.004, "terracotta", 10)
    for side in (-1, 1):
        B(ctr, 1.28 + side * 0.058 - 0.012, 1.28 + side * 0.058 + 0.012, 2.212, 2.228, 0.515, 0.525, "black")
    # Pileta y canilla bajo la ventana.
    B(ctr, 2.2, 2.6, 2.22, 2.5, 0.46, 0.462, "metal_dark")
    C(ctr, 2.4, 2.55, 0.46, 0.012, 0.16, "metal", 6)
    ctr.add(cube(0.014, 0.1, 0.014), "metal", M((2.4, 2.5, 0.615)))
    # Pava, frascos y planta de albahaca en el alféizar.
    C(ctr, 1.46, 2.45, 0.466, 0.06, 0.09, "flower_red", 10, r_top=0.045)
    C(ctr, 1.46, 2.45, 0.556, 0.02, 0.02, "black", 8)
    ctr.add(cyl(0.012, 0.07, 6, 0.007), "flower_red", M((1.4, 2.45, 0.5), (0, -55, 0)))
    for k, col in enumerate(("cream", "wood_light", "orange")):
        C(ctr, 1.75 + k * 0.08, 2.5, 0.46, 0.03, 0.08 + 0.02 * k, "glass", 8)
        C(ctr, 1.75 + k * 0.08, 2.5, 0.46, 0.028, 0.05 + 0.02 * k, col, 8)
        C(ctr, 1.75 + k * 0.08, 2.5, 0.54 + 0.02 * k, 0.031, 0.015, "wood", 8)
    C(ctr, 2.62, 2.58, 0.8, 0.045, 0.07, "terracotta", 8, r_top=0.05)
    for i in range(6):
        ctr.add(ico(0.035, 1), "green_light" if i % 2 else "green",
                M((2.62 + 0.03 * math.cos(i), 2.58 + 0.02 * math.sin(i), 0.89 + 0.03 * (i % 3))))

    broom = obj("broom_home", "state:broom_home")
    bm = M((0.53, BACK_Y - 0.06, 0.0), (8, 0, 0))
    broom.add(cyl(0.008, 0.5, 6), "wood_light", bm @ M((0, 0, 0.07)))
    broom.add(cube(0.12, 0.03, 0.075, 0.006), "orange", bm @ M((0, 0, 0.04)))
    broom.add(cube(0.125, 0.034, 0.012), "wood", bm @ M((0, 0, 0.08)))

    # Matafuego colgado del frente del bajo mesada, bajo la pileta (por
    # si se le prende fuego la olla), a la vista de la cámara de la cocina.
    ext = obj("extinguisher_home", "state:extinguisher_home")
    extinguisher(ext, M((2.3, cy0 - 0.075, 0.08)))
    B(ext, 2.27, 2.33, cy0 - 0.036, cy0 - 0.03, 0.13, 0.25, "metal_dark")

    up = obj("upper_cabinets")
    B(up, 1.15, 1.6, BACK_Y - 0.32, BACK_Y, 0.98, 1.08, "metal", 0.008)     # campana
    up.add(cube(0.2, 0.18, 0.5), "metal", M((1.375, BACK_Y - 0.12, 1.33)))
    B(up, 1.62, 2.0, BACK_Y - 0.3, BACK_Y, 1.0, 1.4, "teal", 0.006)
    B(up, 1.63, 1.81, BACK_Y - 0.31, BACK_Y - 0.3, 1.02, 1.38, "teal", 0.005)
    B(up, 1.82, 1.99, BACK_Y - 0.31, BACK_Y - 0.3, 1.02, 1.38, "teal", 0.005)
    for hx in (1.79, 1.84):
        B(up, hx, hx + 0.012, BACK_Y - 0.325, BACK_Y - 0.31, 1.06, 1.12, "metal")
    win = obj("kitchen_curtain")
    B(win, 2.0, 2.75, BACK_Y - 0.05, BACK_Y - 0.03, 1.36, 1.5, "terracotta", 0.01)

    sx, sy = TABLE_SEAT
    tbl = obj("dining_table")
    ty0, ty1 = sy - 0.17 - 0.5, sy - 0.17
    B(tbl, sx - 0.38, sx + 0.38, ty0, ty1, DESK_H - 0.025, DESK_H, "wood", 0.01)
    for lx in (sx - 0.34, sx + 0.31):
        for ly in (ty0 + 0.03, ty1 - 0.06):
            B(tbl, lx, lx + 0.03, ly, ly + 0.03, 0.0, DESK_H - 0.025, "wood_dark")
    B(tbl, sx - 0.3, sx + 0.3, ty0 + 0.04, ty1 - 0.04, DESK_H, DESK_H + 0.002, "cream")   # mantel
    # Plato frente a su lugar, vaso, frutera con bananas y servilleta.
    px, py = sx - 0.07, sy - 0.27
    C(tbl, px, py, DESK_H + 0.002, 0.06, 0.008, "white", 12, r_top=0.075)
    C(tbl, px, py, DESK_H + 0.009, 0.045, 0.002, "cream", 12)
    C(tbl, sx + 0.17, py + 0.02, DESK_H + 0.002, 0.025, 0.07, "glass", 8)
    C(tbl, sx + 0.17, py + 0.02, DESK_H + 0.002, 0.022, 0.045, "orange", 8)
    bowl_x, bowl_y = sx + 0.12, ty0 + 0.12
    C(tbl, bowl_x, bowl_y, DESK_H + 0.002, 0.04, 0.05, "wood_light", 10, r_top=0.085)
    for k in range(4):
        a = k * 90 + 20
        tbl.add(banana_bm(0.15, 0.017, 45), "banana",
                M((bowl_x + 0.02 * math.cos(math.radians(a)), bowl_y + 0.02 * math.sin(math.radians(a)),
                   DESK_H + 0.07), (12 * (k % 2), 0, a)))
    B(tbl, px + 0.1, px + 0.16, py - 0.05, py + 0.05, DESK_H + 0.002, DESK_H + 0.006, "teal", rot=(0, 0, 10))

    ch = obj("dining_chair")
    chair_wood(ch, sx, sy, 0)
    ch2 = obj("dining_chair_2")
    chair_wood(ch2, sx + 0.58, sy - 0.42, 90)

    rug = obj("rug_dining")
    rug.add(poly(8, 0.62), "navy", M((sx, sy - 0.32, 0.004), (0, 0, 22.5)))
    rug.add(poly(8, 0.54), "cream", M((sx, sy - 0.32, 0.006), (0, 0, 22.5)))
    rug.add(poly(8, 0.5), "navy", M((sx, sy - 0.32, 0.008), (0, 0, 22.5)))

    lamp = obj("pendant_lamp")
    C(lamp, sx, sy - 0.42, 1.25, 0.006, WALL_H - 1.25, "black", 4)
    C(lamp, sx, sy - 0.42, 1.12, 0.13, 0.13, "orange", 10, r_top=0.03)
    S(lamp, sx, sy - 0.42, 1.12, 0.035, "lamp", 1)

    clock = obj("clock")
    cm = M((0.87, BACK_Y - 0.02, 1.52), (90, 0, 0))
    clock.add(cyl(0.11, 0.025, 14), "wood_dark", cm)
    clock.add(cyl(0.095, 0.027, 14), "cream", cm)
    clock.add(cube(0.008, 0.03, 0.07), "black", M((0.87, BACK_Y - 0.05, 1.55)))
    clock.add(cube(0.05, 0.03, 0.008), "black", M((0.89, BACK_Y - 0.05, 1.52)))


# ---------------------------------------------------------------------------
# Rincón de lectura junto a la puerta.
# ---------------------------------------------------------------------------
def build_reading():
    sx, sy = ARMCHAIR_SEAT
    ac = obj("armchair")
    B(ac, sx - 0.2, sx + 0.2, sy - 0.13, sy + 0.2, 0.03, SEAT_H - 0.02, "teal_dark", 0.02)
    B(ac, sx - 0.17, sx + 0.17, sy - 0.15, sy + 0.17, SEAT_H - 0.03, SEAT_H, "teal", 0.025)
    B(ac, sx - 0.2, sx + 0.2, sy + 0.14, sy + 0.24, 0.05, SEAT_H + 0.38, "teal", 0.04)
    for side in (-1, 1):
        x = sx + side * 0.21
        B(ac, x - 0.045, x + 0.045, sy - 0.16, sy + 0.22, 0.03, SEAT_H + 0.12, "teal", 0.03)
    for lx in (sx - 0.22, sx + 0.2):
        for ly in (sy - 0.15, sy + 0.2):
            C(ac, lx, ly, 0.0, 0.012, 0.035, "wood_dark", 6)
    B(ac, sx - 0.12, sx + 0.12, sy + 0.08, sy + 0.15, SEAT_H, SEAT_H + 0.14, "orange", 0.03, rot=(-15, 0, 0))
    fl = obj("floor_lamp")
    C(fl, sx + 0.38, sy + 0.25, 0.0, 0.08, 0.015, "metal_dark", 10)
    C(fl, sx + 0.38, sy + 0.25, 0.015, 0.01, 0.95, "metal_dark", 6)
    C(fl, sx + 0.38, sy + 0.25, 0.9, 0.12, 0.16, "lamp", 10, r_top=0.08)
    st = obj("side_table")
    C(st, sx - 0.38, sy + 0.05, 0.0, 0.1, 0.012, "wood_dark", 10)
    C(st, sx - 0.38, sy + 0.05, 0.012, 0.012, 0.2, "wood_dark", 6)
    C(st, sx - 0.38, sy + 0.05, 0.21, 0.12, 0.02, "wood", 12)
    C(st, sx - 0.36, sy + 0.04, 0.23, 0.025, 0.045, "terracotta", 8)
    B(st, sx - 0.46, sx - 0.36, sy + 0.06, sy + 0.13, 0.23, 0.255, "navy", 0.004, rot=(0, 0, -15))
    # Termo para el mate.
    C(st, sx - 0.31, sy + 0.12, 0.23, 0.022, 0.1, "green_dark", 8)
    C(st, sx - 0.31, sy + 0.12, 0.33, 0.016, 0.02, "metal_dark", 8)
    plant = obj("plant_fiddle")
    C(plant, 3.42, 2.35, 0.0, 0.09, 0.16, "terracotta", 8, r_top=0.1)
    C(plant, 3.42, 2.35, 0.16, 0.012, 0.45, "trunk", 5)
    import random
    rnd = random.Random(11)
    for i in range(9):
        z = 0.32 + i * 0.04
        a = i * 137
        plant.add(leaf_bm(0.11, 0.05, 0.2, 3, 0.05), "green_dark" if i % 3 else "green",
                  M((3.42, 2.35, z), (0, rnd.uniform(-10, 25), a)))


# ---------------------------------------------------------------------------
# Jardín con bananero.
# ---------------------------------------------------------------------------
TREE = (6.35, 2.55)
# Racimo y banana para cosechar: a 0,75 m, al alcance de la mano derecha del
# mono parado (hombro a 0,53 m, brazo + antebrazo 0,255 m).
BUNCH = (TREE[0] - 0.05, TREE[1] - 0.30, 0.75)
PICK = (BUNCH[0] - 0.07, BUNCH[1] - 0.04, BUNCH[2])
TREE_YAW = 100.0
# La banana tiene que quedar en (-0,07; -0,09) respecto de sus pies (adelante
# y a la derecha), mirando hacia el árbol.
_c, _s = math.cos(math.radians(TREE_YAW)), math.sin(math.radians(TREE_YAW))
TREE_SPOT = (PICK[0] - (-0.07 * _c + 0.09 * _s), PICK[1] - (-0.07 * _s - 0.09 * _c))


def build_garden():
    import random
    rnd = random.Random(21)
    ground = obj("garden_ground")
    gx0, gx1, gy0, gy1 = RIGHT_X + 0.1, 7.9, -0.6, 3.7
    xs = [gx0, 4.4, 5.2, 6.0, 6.8, gx1]
    ys = [gy0, 0.4, 1.4, 2.4, gy1]
    for i in range(len(xs) - 1):
        for j in range(len(ys) - 1):
            ground.add(quad([(xs[i], ys[j], 0), (xs[i + 1], ys[j], 0), (xs[i + 1], ys[j + 1], 0), (xs[i], ys[j + 1], 0)]),
                       "grass")
    for k in range(7):
        ground.add(poly(7, rnd.uniform(0.25, 0.5)), "grass_dark",
                   M((rnd.uniform(gx0 + 0.4, gx1 - 0.4), rnd.uniform(gy0 + 0.3, gy1 - 0.5), 0.002), (0, 0, rnd.uniform(0, 50)),
                     (1.0, rnd.uniform(0.5, 0.8), 1.0)))
    B(ground, gx0, gx1, gy0, gy1, -0.12, -0.001, "soil")
    # Mechones de pasto.
    for k in range(18):
        x, y = rnd.uniform(gx0 + 0.2, gx1 - 0.2), rnd.uniform(gy0 + 0.2, gy1 - 0.4)
        for b in range(3):
            ground.add(blade_bm(rnd.uniform(0.06, 0.1), 0.02, 0.05), "green_light",
                       M((x + 0.02 * b, y, 0), (0, 0, rnd.uniform(0, 180))))

    path = obj("garden_path")
    pts = [(3.95, 1.55), (4.3, 1.45), (4.65, 1.5), (5.0, 1.65), (5.35, 1.85), (5.7, 2.05)]
    for (x, y) in pts:
        path.add(cyl(rnd.uniform(0.1, 0.13), 0.02, 8), "stone", M((x, y, 0.0), (0, 0, rnd.uniform(0, 45))))

    tree = obj("banana_tree")
    tx, ty = TREE
    # Pseudotallo: vainas superpuestas, más angostas arriba.
    z = 0.0
    for k, (r0, r1, h, col) in enumerate(((0.15, 0.125, 0.45, "trunk"), (0.125, 0.105, 0.45, "green_dark"),
                                          (0.105, 0.085, 0.4, "trunk"), (0.085, 0.065, 0.25, "green_dark"))):
        C(tree, tx, ty, z, r0, h + 0.02, col, 7, r_top=r1, rot=(0, 0, k * 20))
        z += h
    top = z
    for i in range(8):
        a = i * 45 + rnd.uniform(-12, 12)
        L = rnd.uniform(1.0, 1.3)
        tree.add(leaf_bm(L, rnd.uniform(0.2, 0.26), rnd.uniform(0.35, 0.6), 6, 0.12, rnd.uniform(-0.05, 0.05)),
                 "green" if i % 2 else "green_light",
                 M((tx, ty, top - 0.02), (0, 0, a)) @ M((0, 0, 0), (0, -rnd.uniform(25, 45), 0)))
        tree.add(cube(L * 0.5, 0.015, 0.015), "banana_green",
                 M((tx, ty, top - 0.02), (0, 0, a)) @ M((0, 0, 0), (0, -rnd.uniform(25, 40), 0)) @ M((L * 0.25, 0, 0.005)))
    # Racimo: tallo que cae hacia el frente y manos de bananas.
    # El tallo del racimo sale de la copa y cae por delante del tronco.
    bx, by, bz = BUNCH
    p0 = Vector((tx, ty - 0.08, top - 0.05))
    p1 = Vector((bx, by, bz + 0.12))
    d = p1 - p0
    tree.add(cyl(0.022, d.length, 6, 0.018), "trunk",
             Matrix.Translation(p0) @ d.to_track_quat("Z", "Y").to_matrix().to_4x4())
    tree.add(cyl(0.02, 0.2, 6, 0.016), "trunk", M((bx, by, bz - 0.12)))
    for tier in range(3):
        zz = bz + 0.06 - tier * 0.065
        for k in range(5):
            a = k * 72 + tier * 36
            ca, sa = math.cos(math.radians(a)), math.sin(math.radians(a))
            tree.add(banana_bm(0.16, 0.02, 50), "banana" if tier < 2 else "banana_green",
                     M((bx + 0.04 * ca, by + 0.04 * sa, zz), (0, 0, a)) @ M((0.06, 0, 0), (0, -70, 0)))
    tree.add(cyl(0.045, 0.12, 6, 0.0), "flower_purple", M((bx, by, bz - 0.13), (180, 0, 0)))
    pick = obj("banana_pick", "state:on_tree")
    pick.add(banana_bm(0.16, 0.02, 50), "banana", M(PICK, (0, 0, 200)) @ M((0, 0, 0), (0, -60, 0)))

    fence = obj("fence")
    def picket(x, y, rot):
        m = M((x, y, 0), (0, 0, rot))
        fence.add(cube(0.07, 0.022, 0.5), "white", m @ M((0, 0, 0.25)))
        fence.add(cyl(0.05, 0.06, 4, 0.0), "white", m @ M((0, 0, 0.5), (0, 0, 45)))
    x = gx0 + 0.15
    while x < gx1 - 0.05:
        picket(x, gy1 - 0.15, 0)
        x += 0.26
    y = gy0 + 0.1
    while y < gy1 - 0.2:
        picket(gx1 - 0.12, y, 90)
        y += 0.26
    for zz in (0.12, 0.36):
        B(fence, gx0, gx1 - 0.08, gy1 - 0.19, gy1 - 0.17, zz, zz + 0.05, "white", 0.004)
        B(fence, gx1 - 0.16, gx1 - 0.14, gy0, gy1 - 0.15, zz, zz + 0.05, "white", 0.004)

    bed = obj("veggie_bed")
    vx0, vx1, vy0, vy1 = 4.55, 5.55, 2.65, 3.25
    B(bed, vx0, vx1, vy0, vy1, 0.0, 0.16, "wood", 0.008)
    B(bed, vx0 + 0.03, vx1 - 0.03, vy0 + 0.03, vy1 - 0.03, 0.14, 0.155, "soil")
    for i in range(5):
        for j in range(2):
            x = vx0 + 0.12 + i * 0.19
            y = vy0 + 0.17 + j * 0.26
            for b in range(3):
                bed.add(blade_bm(0.09, 0.03, 0.06), "green" if (i + j) % 2 else "green_light",
                        M((x, y, 0.155), (0, 0, b * 60 + i * 13)))
            if (i + j) % 3 == 0:
                bed.add(cyl(0.012, 0.03, 5, 0.0), "orange", M((x + 0.03, y, 0.16), (180, 0, 0)))

    bushes = obj("bushes")
    for (x, y, r) in ((4.05, 3.15, 0.22), (7.4, 1.0, 0.25), (7.45, 2.5, 0.2), (4.2, -0.2, 0.15)):
        S(bushes, x, y, r * 0.75, r, "green_dark", 1, (1.0, 0.85, 0.75))
        S(bushes, x + r * 0.5, y - 0.05, r * 0.6, r * 0.7, "green", 1, (1.0, 0.85, 0.8))
    flowers = obj("flowers")
    for k in range(9):
        x, y = rnd.uniform(6.6, 7.4), rnd.uniform(-0.3, 0.9) if k < 5 else rnd.uniform(3.0, 3.3)
        if k >= 5:
            x = rnd.uniform(5.7, 7.2)
        h = rnd.uniform(0.12, 0.22)
        flowers.add(cube(0.008, 0.008, h), "green_dark", M((x, y, h / 2)))
        S(flowers, x, y, h, 0.025, ("flower_red", "banana", "flower_purple")[k % 3], 0)

    mat = obj("yoga_mat")
    B(mat, 4.9, 5.7, 1.05, 1.45, 0.0, 0.008, "flower_purple", 0.003)
    B(mat, 4.9, 5.7, 1.24, 1.26, 0.008, 0.009, "cream")
    C(mat, 5.8, 1.3, 0.0, 0.035, 0.015, "metal_dark", 8)

    can = obj("watering_can", "state:garden")
    wx, wy = 5.75, 2.75
    C(can, wx, wy, 0.0, 0.06, 0.12, "teal", 10, r_top=0.055)
    can.add(cyl(0.012, 0.13, 6, 0.008), "teal", M((wx + 0.05, wy, 0.04), (0, 55, 0)))
    can.add(cube(0.012, 0.012, 0.09), "teal_dark", M((wx - 0.04, wy, 0.15), (0, -30, 0)))

    far = obj("far_hills", "static")
    for (x0, x1, h, y, col) in ((2.5, 6.5, 0.9, 6.0, "hill_far"), (5.0, 9.5, 1.3, 6.8, "hill_far"),
                                (3.5, 7.0, 0.6, 5.2, "hill")):
        bm = bmesh.new()
        n = 6
        base = [bm.verts.new((x0 + (x1 - x0) * i / n, y, 0.0)) for i in range(n + 1)]
        tops = [bm.verts.new((x0 + (x1 - x0) * i / n, y, h * math.sin(math.pi * i / n) ** 0.8 * rnd.uniform(0.85, 1.0)))
                for i in range(n + 1)]
        for i in range(n):
            bm.faces.new((base[i], base[i + 1], tops[i + 1], tops[i]))
        far.add(bm, col)
    for (x, y, r) in ((4.6, 4.6, 0.4), (7.1, 4.9, 0.5), (8.3, 4.2, 0.35)):
        C(far, x, y, 0.0, 0.05, r * 1.2, "wood_dark", 5)
        S(far, x, y, r * 1.6, r, "green_dark", 1, (1, 1, 1.1))


# ---------------------------------------------------------------------------
# Objetos para las manos. El origen es el punto de agarre; el juego los pega
# a la mano del personaje.
# ---------------------------------------------------------------------------
# La guitarra y el joystick, más grandes que a escala: las manos del mono son
# enormes y los taparían.
GUITAR = 1.3
PAD = 1.45


def guitar(o, m):
    """Guitarra criolla (0,34 m sin escalar). Origen en la boca, mástil hacia
    +X y el frente hacia -Y."""
    for x, r in ((-0.045, 0.075), (0.045, 0.058)):
        o.add(cyl(r, 0.034, 12), "wood_light", m @ M((x, 0.017, 0), (90, 0, 0)))
        o.add(cyl(r + 0.002, 0.03, 12), "wood", m @ M((x, 0.019, 0), (90, 0, 0)))
    o.add(cyl(0.022, 0.002, 10), "black", m @ M((0, -0.0175, 0), (90, 0, 0)))
    o.add(cube(0.03, 0.004, 0.06), "wood_dark", m @ M((-0.09, -0.018, 0)))
    o.add(cube(0.17, 0.014, 0.022), "wood_dark", m @ M((0.17, 0.004, 0)))
    o.add(cube(0.045, 0.012, 0.03), "wood", m @ M((0.275, 0.006, 0), (0, 0, -8)))
    for k in (-1, 1):
        o.add(cube(0.24, 0.002, 0.0015), "metal", m @ M((0.06, -0.012, k * 0.005)))


def extinguisher(o, m):
    """Matafuego chico (0,24 m). Origen en la base, la manija arriba y la
    manguera hacia -Y."""
    o.add(cyl(0.042, 0.19, 10), "flower_red", m)
    o.add(cyl(0.042, 0.02, 10, 0.02), "flower_red", m @ M((0, 0, 0.19)))
    o.add(cyl(0.012, 0.03, 6), "metal", m @ M((0, 0, 0.205)))
    o.add(cube(0.012, 0.07, 0.01), "metal_dark", m @ M((0, -0.01, 0.235)))
    o.add(cube(0.07, 0.003, 0.03), "white", m @ M((0, -0.042, 0.1)))
    o.add(cyl(0.007, 0.13, 5), "black", m @ M((0, -0.045, 0.21), (150, 0, 0)))
    o.add(cyl(0.011, 0.04, 6, 0.016), "black", m @ M((0, -0.11, 0.1), (150, 0, 0)))


def build_fx():
    """Efectos: bocanadas, llamas, chispas y gotas que el juego pone, mueve y
    saca (humo de la olla, el pedo dormido, el matafuego, el sudor). Varios
    tamaños de la misma forma: el motor no escala mallas (cambiaría la luz)."""
    for k, r in enumerate((0.022, 0.036, 0.055)):
        obj(f"fx_smoke_{k}", "prop").add(ico(r, 0), "stone", M(scale=(1, 1, 0.85)))
        obj(f"fx_fart_{k}", "prop").add(ico(r * 0.9, 0), "banana_green", M(scale=(1, 1, 0.8)))
    for k, r in enumerate((0.03, 0.05)):
        obj(f"fx_foam_{k}", "prop").add(ico(r, 0), "white")
    # Llamas: dos formas que se alternan (titilan). Origen en la base.
    for k, (h, lean) in enumerate(((0.16, 8), (0.13, -10))):
        f = obj(f"fx_flame_{k}", "prop")
        for i in range(5):
            a = i * 72 + k * 30
            x, y = 0.03 * math.cos(math.radians(a)), 0.03 * math.sin(math.radians(a))
            hh = h * (0.55 + 0.45 * ((i + k) % 3) / 2)
            f.add(cyl(0.022, hh, 5, 0.0), "orange", M((x, y, 0), (lean * (1 if i % 2 else -1), 0, 0)))
        f.add(cyl(0.028, h * 1.1, 5, 0.0), "flower_red", M((0, 0, -0.005), (0, lean, 0)))
        f.add(cyl(0.02, h * 0.6, 5, 0.0), "banana", M((0.005, 0, 0.0)))
    # Chispazo de una micro explosión: rayos alrededor de un centro.
    for k, r in enumerate((0.08, 0.15)):
        sp = obj(f"fx_spark_{k}", "prop")
        sp.add(ico(r * 0.35, 0), "banana")
        for i in range(8):
            a = i * 45
            col = ("orange", "banana", "flower_red")[i % 3]
            sp.add(cyl(r * 0.16, r, 4, 0.0), col, M(rot=(0, 90 - (i % 2) * 40, a)))
    # Gota de sudor: punta arriba (blanca: celeste se pierde contra el cielo).
    d = obj("fx_drop", "prop")
    d.add(ico(0.02, 1), "white", M(scale=(1, 1, 1.1)))
    d.add(cyl(0.018, 0.035, 6, 0.0), "white", M((0, 0, 0.006)))


def build_props():
    mate = obj("prop_mate", "prop")
    # Origen en la mano: el mate (calabaza) arriba del puño, la bombilla al costado.
    mate.add(ico(0.03, 1), "wood", M((0, 0, 0.035), scale=(1, 1, 1.15)))
    mate.add(cyl(0.022, 0.012, 8), "wood_dark", M((0, 0, 0.062)))
    mate.add(cyl(0.019, 0.003, 8), "green", M((0, 0, 0.073)))
    mate.add(cyl(0.003, 0.07, 5), "metal", M((0.008, 0, 0.05), (0, 18, 0)))
    spoon = obj("prop_spoon", "prop")
    # Cuchara de madera colgando del puño (para revolver la olla).
    spoon.add(cyl(0.006, 0.17, 6), "wood_light", M((0, 0, -0.14)))
    spoon.add(ico(0.018, 1), "wood_light", M((0, 0, -0.15), scale=(1, 0.5, 1.3)))
    g = obj("prop_guitar", "prop")
    guitar(g, M(scale=(GUITAR,) * 3))
    phone = obj("prop_phone", "prop")
    # A escala del mono (manos enormes): un teléfono grande, para que se vea.
    phone.add(cube(0.05, 0.009, 0.095, 0.003), "black", M())
    phone.add(cube(0.044, 0.001, 0.084), "teal", M((0, -0.005, 0.002)))
    broom = obj("prop_broom", "prop")
    # Origen entre las manos, a media altura del palo.
    broom.add(cyl(0.008, 0.56, 6), "wood_light", M((0, 0, -0.33)))
    broom.add(cube(0.125, 0.034, 0.012), "wood", M((0, 0, -0.32)))
    broom.add(cube(0.12, 0.03, 0.075, 0.006), "orange", M((0, 0, -0.365)))
    pad = obj("prop_controller", "prop")
    pm = M(scale=(PAD,) * 3)
    pad.add(cube(0.07, 0.03, 0.018, 0.006), "metal_dark", pm)
    for side in (-1, 1):
        pad.add(ico(0.016, 1), "metal_dark", pm @ M((side * 0.033, 0.006, -0.006)))
        pad.add(cyl(0.005, 0.006, 6), "black", pm @ M((side * 0.018, -0.01, 0.006)))
    pad.add(cube(0.012, 0.002, 0.004), "flower_red", pm @ M((0.008, -0.016, 0.004)))
    pad.add(cube(0.008, 0.002, 0.008), "teal", pm @ M((-0.008, -0.016, 0.004)))

    ban = obj("prop_banana", "prop")
    ban.add(banana_bm(0.16, 0.02, 45), "banana", M((0, 0, 0.06), (0, -90, 0)))
    ban.add(cyl(0.008, 0.02, 5, 0.006), "wood_dark", M((0, 0, -0.025)))
    book = obj("prop_book", "prop")
    # Libro abierto: dos tapas en V suave y las hojas.
    for side in (-1, 1):
        book.add(cube(0.095, 0.007, 0.13), "terracotta", M((side * 0.048, 0, 0), (0, 0, side * -12)))
        book.add(cube(0.088, 0.009, 0.12), "cream", M((side * 0.045, -0.007, 0), (0, 0, side * -12)))
    ext = obj("prop_extinguisher", "prop")
    # Origen en la manija (donde lo agarra la mano).
    extinguisher(ext, M((0, 0, -0.235)))
    can = obj("prop_can", "prop")
    # Origen en la manija de arriba, que va de adelante hacia atrás (en el
    # puño); el cuerpo cuelga debajo y el pico sale hacia adelante (-Y).
    can.add(cube(0.012, 0.09, 0.012), "teal_dark", M((0, 0.005, 0)))
    for yy in (-0.035, 0.045):
        can.add(cube(0.012, 0.012, 0.08), "teal_dark", M((0, yy, -0.04)))
    can.add(cyl(0.055, 0.11, 10, 0.05), "teal", M((0, 0.005, -0.19)))
    can.add(cyl(0.011, 0.14, 6, 0.007), "teal", M((0, -0.045, -0.165), (60, 0, 0)))
    can.add(cyl(0.02, 0.012, 8, 0.024), "teal_dark", M((0, -0.168, -0.094), (60, 0, 0)))


# ---------------------------------------------------------------------------
# Lugares, puntos de paso y cámaras.
# ---------------------------------------------------------------------------
SPOTS = {
    # nombre: (x, y, z, mira hacia: grados alrededor de Z; 0 = a la cámara)
    # Acostado de costado, con la cabeza en la almohada y de cara a la cámara.
    "spot_bed": (-2.62, 1.80, 0.407, 0),
    # Se sienta en el costado largo de la cama, mirando a la cámara, se
    # recuesta de lado hacia la almohada y sube las piernas
    # (`lie_down_side` en tools/make_clips.py).
    "spot_bed_side": (-2.58, 1.53, 0.10, 0),
    # Para tocar la guitarra: en el mismo costado, en diagonal a la cámara
    # para que se vea la guitarra.
    "spot_bed_guitar": (-2.32, 1.53, 0.10, 22),
    "spot_desk": (*DESK_SEAT, 0.0, 0),
    "spot_table": (*TABLE_SEAT, 0.0, 0),
    "spot_armchair": (*ARMCHAIR_SEAT, 0.0, 0),
    "spot_window": (2.38, 1.78, 0.0, 180),
    # Frente a la olla, mirando a la pared.
    # Con el hombro derecho frente a la olla (mirando a +Y, su derecha es +X).
    "spot_stove": (1.16, 2.0, 0.0, 180),
    # Sobre la alfombra del dormitorio, de frente.
    "spot_dance": (-2.60, 1.0, 0.0, 0),
    # En el jardín, mirando al cielo (de espaldas a la cámara).
    "spot_stars": (4.95, 2.2, 0.0, 180),
    "spot_tree": (*TREE_SPOT, 0.0, TREE_YAW),
    "spot_veggies": (5.78, 2.95, 0.0, -90),
    "spot_mat": (5.3, 1.25, 0.0, 0),
    "spot_bookcase": (0.25, 1.9, 0.0, 180),
    # No es un lugar del personaje: la olla, de donde salen el humo y las
    # llamas cuando algo sale mal.
    "spot_pot": (1.28, 2.22, 0.53, 0),
    # Frente al matafuego (colgado del bajo mesada).
    "spot_extinguisher": (2.3, 1.84, 0.0, 180),
}
WAYPOINTS = {
    "wp_bedroom": (-2.4, 0.95),
    "wp_office": (-0.55, 0.78),
    "wp_kitchen": (1.2, 0.62),
    "wp_reading": (2.75, 0.75),
    "wp_door_in": (3.35, 1.55),
    "wp_door_out": (3.95, 1.55),
    "wp_garden": (5.0, 1.75),
    "wp_tree": (5.7, 1.9),
}
CAMERAS = {
    # nombre: (x, y, z, mira a (x, y, z), lente mm)
    "cam_bedroom": (-2.5, -1.25, 0.9, (-2.5, 1.9, 0.48), 36),
    "cam_office": (-0.45, -1.25, 0.9, (-0.45, 1.9, 0.48), 36),
    "cam_kitchen": (1.75, -1.25, 0.9, (1.75, 1.9, 0.48), 36),
    "cam_reading": (2.95, -1.25, 0.9, (2.95, 1.8, 0.48), 36),
    "cam_garden": (5.45, -1.6, 1.05, (5.6, 2.4, 0.6), 32),
    "cam_house": (1.0, -7.5, 2.2, (1.0, 1.4, 0.9), 40),
}


def make_scene(out_blend, preview_dir):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    coll = bpy.data.collections.new("Habitat")
    scene.collection.children.link(coll)
    mats = {}
    for o in OBJS:
        me = bpy.data.meshes.new(o.name)
        me.from_pydata([tuple(v) for v in o.v], [], o.f)
        me.update()
        attr = me.color_attributes.new("Col", "BYTE_COLOR", "CORNER")
        for poly, col in zip(me.polygons, o.c):
            for li in poly.loop_indices:
                attr.data[li].color_srgb = (col[0] / 255, col[1] / 255, col[2] / 255, 1.0)
        me.color_attributes.active_color = attr
        ob = bpy.data.objects.new(o.name, me)
        ob["kind"] = o.kind
        coll.objects.link(ob)
        if o.material:
            name, rel = o.material
            if name not in mats:
                m = bpy.data.materials.new(name)
                m.use_nodes = True
                nt = m.node_tree
                img = bpy.data.images.load(os.path.join(ASSETS, rel))
                tex = nt.nodes.new("ShaderNodeTexImage")
                tex.image = img
                tex.interpolation = "Closest"
                tex.extension = "REPEAT"
                uvn = nt.nodes.new("ShaderNodeUVMap")
                uvn.uv_map = "UVScreen"
                nt.links.new(uvn.outputs["UV"], tex.inputs["Vector"])
                bsdf = nt.nodes["Principled BSDF"]
                nt.links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
                nt.links.new(tex.outputs["Color"], bsdf.inputs["Emission Color"])
                bsdf.inputs["Emission Strength"].default_value = 1.0
                mats[name] = m
            me.materials.append(mats[name])
            uvl = me.uv_layers.new(name="UVScreen")
            for poly, uv in zip(me.polygons, o.uv):
                for li, (u, v) in zip(poly.loop_indices, uv or [(0, 0)] * len(poly.loop_indices)):
                    uvl.uv[li].vector = (u, v)
    for name, (x, y, z, rot) in {**SPOTS, **{k: (x, y, 0.0, 0) for k, (x, y) in WAYPOINTS.items()}}.items():
        e = bpy.data.objects.new(name, None)
        e.empty_display_type = "SINGLE_ARROW" if name.startswith("spot") else "PLAIN_AXES"
        e.empty_display_size = 0.15
        e.location = (x, y, z)
        e.rotation_euler = (0, 0, math.radians(rot))
        coll.objects.link(e)
    for name, (x, y, z, target, lens) in CAMERAS.items():
        cd = bpy.data.cameras.new(name)
        cd.lens = lens
        cd.sensor_fit = "HORIZONTAL"
        cd.sensor_width = 36
        cd.clip_start = 0.1
        cd.clip_end = 30
        cam = bpy.data.objects.new(name, cd)
        cam.location = (x, y, z)
        d = Vector(target) - Vector((x, y, z))
        cam.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()
        coll.objects.link(cam)

    world = bpy.data.worlds.new("World")
    world.color = (0.55, 0.75, 0.95)
    scene.world = world
    bpy.ops.wm.save_as_mainfile(filepath=out_blend)

    if preview_dir:
        os.makedirs(preview_dir, exist_ok=True)
        scene.render.engine = "BLENDER_WORKBENCH"
        sh = scene.display.shading
        sh.light = "STUDIO"
        sh.color_type = "VERTEX"
        sh.show_shadows = True
        sh.show_cavity = False
        scene.display.shading.show_object_outline = False
        scene.render.resolution_x = 480
        scene.render.resolution_y = 480
        for name in CAMERAS:
            scene.camera = bpy.data.objects[name]
            scene.render.filepath = os.path.join(preview_dir, name + ".png")
            bpy.ops.render.render(write_still=True)

    tris = 0
    for o in OBJS:
        tris += sum(len(f) - 2 for f in o.f)
    print(f"hábitat: {len(OBJS)} objetos, {tris} triángulos, {len(PAL)} colores", flush=True)
    for o in OBJS:
        print(f"  {o.name}: {sum(len(f) - 2 for f in o.f)} triángulos ({o.kind})")


ASSETS = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "assets")


def main():
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = args[0] if args else os.path.join(ASSETS, "house.blend")
    preview = args[1] if len(args) > 1 else None
    build_shell()
    build_bedroom()
    build_office()
    build_kitchen()
    build_reading()
    build_garden()
    build_props()
    build_fx()
    make_scene(os.path.abspath(out), preview)


main()
