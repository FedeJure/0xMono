"""Vista desde arriba del mapa de navegación del hábitat (para revisar a ojo).

    python tools/nav_preview.py salida.png
"""

import json
import os
import sys

from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from build_nav import ROOT, build_nav  # noqa: E402


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "nav.png"
    scene = json.load(open(os.path.join(ROOT, "assets", "house.json"), encoding="utf-8"))
    nav = build_nav(scene)
    w, h, cell = nav["w"], nav["h"], nav["cell"]
    s = 4
    img = Image.new("RGB", (w * s, h * s), (250, 250, 250))
    d = ImageDraw.Draw(img)

    def px(x, y):
        return ((x - nav["x0"]) / cell * s, (h - (y - nav["y0"]) / cell) * s)

    for j in range(h):
        for i in range(w):
            if nav["solid"][j][i]:
                c = (60, 60, 70)
            elif nav["blocked"][j][i]:
                c = (200, 190, 190)
            else:
                continue
            y = (h - 1 - j) * s
            d.rectangle([i * s, y, i * s + s - 1, y + s - 1], fill=c)
    for a, b in nav["edges"]:
        d.line([px(*nav["nodes"][a]), px(*nav["nodes"][b])], fill=(120, 170, 230))
    for k, (x, y) in enumerate(nav["nodes"]):
        cx, cy = px(x, y)
        named = nav["names"][k]
        r = 4 if named else 2
        d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(220, 60, 60) if named else (40, 100, 200))
        if named:
            d.text((cx + 5, cy - 5), named.replace("spot_", "").replace("wp_", ""), fill=(150, 20, 20))
    for sp in nav["seats"]:
        cx, cy = px(*sp["seat"])
        d.rectangle([cx - 3, cy - 3, cx + 3, cy + 3], outline=(0, 150, 0))
    img.save(out)
    if nav["lost"]:
        print("sin conexión:", nav["lost"])
    print(out, f"{w}x{h} celdas de {cell * 100:.0f} cm, {len(nav['nodes'])} nodos, {len(nav['edges'])} aristas")


if __name__ == "__main__":
    main()
