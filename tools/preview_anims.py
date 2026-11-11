"""Hoja de contacto de animaciones procesadas (verificación visual).

    python tools/preview_anims.py <anims.json> <salida.png> [clip ...] [--compare otro.json] [--cols 8]

Reproduce el skinning rígido del motor a partir de las rotaciones locales
de cada hueso y dibuja el personaje de frente y de perfil. Con `--compare`, cada clip
sale dos veces: arriba el otro JSON (por ejemplo sin corregir), abajo este.
"""

import json
import math
import os
import sys

from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def qmat(q):
    w, x, y, z = q
    return [
        [1 - 2 * (y * y + z * z), 2 * (x * y - w * z), 2 * (x * z + w * y)],
        [2 * (x * y + w * z), 1 - 2 * (x * x + z * z), 2 * (y * z - w * x)],
        [2 * (x * z - w * y), 2 * (y * z + w * x), 1 - 2 * (x * x + y * y)],
    ]


def mm(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def mv(a, v):
    return [sum(a[i][k] * v[k] for k in range(3)) for i in range(3)]


def add(a, b):
    return [a[i] + b[i] for i in range(3)]


def sub(a, b):
    return [a[i] - b[i] for i in range(3)]


def pose(mono, frame):
    bones = mono["bones"]
    R, J = [], []
    for i, b in enumerate(bones):
        q = qmat(frame["q"][i])
        p = b["parent"]
        if p is None:
            R.append(q)
            J.append(add(b["head"], frame["root"]))
        else:
            R.append(mm(R[p], q))
            J.append(add(J[p], mv(R[p], sub(b["head"], bones[p]["head"]))))
    return R, J


def draw(mono, frame, size, view):
    R, J = pose(mono, frame)
    verts = []
    for v in mono["verts"]:
        b = v["bone"] or 0
        verts.append(add(mv(R[b], sub(v["co"], mono["bones"][b]["head"])), J[b]))
    img = Image.new("RGB", (size, size), (40, 42, 54))
    d = ImageDraw.Draw(img)
    s = size / 1.25
    # Frente: x a la derecha, z arriba, profundidad -y. Perfil: -y a la derecha.
    proj = (lambda p: (size / 2 + p[0] * s, size * 0.92 - p[2] * s, -p[1])) if view == "front" else \
        (lambda p: (size / 2 - p[1] * s, size * 0.92 - p[2] * s, p[0]))
    light = [-0.4, -0.5, 0.75]
    tris = []
    for t in mono["tris"]:
        a, b, c = (verts[i] for i in t["v"])
        n = [(b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]),
             (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]),
             (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])]
        ln = math.sqrt(sum(x * x for x in n)) or 1
        k = 0.45 + 0.55 * max(0, sum(n[i] * light[i] for i in range(3)) / ln)
        col = tuple(int(ch * k) for ch in t["color"])
        pa, pb, pc = proj(a), proj(b), proj(c)
        tris.append(((pa[2] + pb[2] + pc[2]) / 3, [pa[:2], pb[:2], pc[:2]], col))
    for _, pts, col in sorted(tris, key=lambda x: x[0]):
        d.polygon(pts, fill=col)
    return img


def main():
    args = sys.argv[1:]
    cols = 8
    compare = None
    if "--cols" in args:
        i = args.index("--cols"); cols = int(args[i + 1]); del args[i:i + 2]
    if "--compare" in args:
        i = args.index("--compare"); compare = json.load(open(args[i + 1])); del args[i:i + 2]
    anims = json.load(open(args[0], encoding="utf-8"))
    out = args[1]
    names = args[2:]
    mono = json.load(open(os.path.join(ROOT, "assets", "mono.json"), encoding="utf-8"))
    size = 120
    rows = []
    for clip in anims["clips"]:
        if names and clip["name"] not in names:
            continue
        variants = []
        if compare:
            other = next((c for c in compare["clips"] if c["name"] == clip["name"]), None)
            if other:
                variants.append(other)
        variants.append(clip)
        for var in variants:
            for view in ("front", "side"):
                n = len(var["frames"])
                picks = [round(i * (n - 1) / max(1, cols - 1)) for i in range(cols)]
                rows.append([draw(mono, var["frames"][f], size, view) for f in picks])
    sheet = Image.new("RGB", (cols * size, len(rows) * size), (0, 0, 0))
    for r, row in enumerate(rows):
        for c, im in enumerate(row):
            sheet.paste(im, (c * size, r * size))
    sheet.save(out)
    print(f"{out}: {len(rows)} filas")


if __name__ == "__main__":
    main()
