#!/usr/bin/env python3
"""Imágenes del manual.

    python3 img_tools.py renders <raw> <img>   recorta las fotos del simulador

Las fotos del simulador miden 240 x 240 (la pantalla) más un panel de
depuración abajo, que se descarta. Se agrandan x3 sin suavizar para que se
vean los píxeles nítidos en papel.
"""
import sys
from pathlib import Path

from PIL import Image

SCREEN = 240
SCALE = 3

# Cuadros elegidos de las hojas de --seq (8 cuadros por fila).
FRAMES = {
    "g-fire2": ("seq-fire.png", 18),
    "g-fart": ("seq-fart.png", 20),
    "t-dance": ("seq-dance.png", 9),
    "f-squint": ("seq-faces.png", 41),
}


def up(im: Image.Image) -> Image.Image:
    return im.resize((im.width * SCALE, im.height * SCALE), Image.NEAREST)


def frame(sheet: Image.Image, k: int) -> Image.Image:
    x, y = (k % 8) * SCREEN, (k // 8) * SCREEN
    return sheet.crop((x, y, x + SCREEN, y + SCREEN))


def renders(raw: Path, out: Path) -> None:
    out.mkdir(parents=True, exist_ok=True)
    for f in sorted(raw.glob("*.png")):
        if f.name.startswith("seq-"):
            continue
        im = Image.open(f).convert("RGB").crop((0, 0, SCREEN, SCREEN))
        up(im).save(out / f.name, optimize=True)
        print("  ", f.name)
    for name, (sheet, k) in FRAMES.items():
        p = raw / sheet
        if p.exists():
            up(frame(Image.open(p).convert("RGB"), k)).save(out / f"{name}.png", optimize=True)
            print("  ", name + ".png")


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "renders":
        renders(Path(sys.argv[2]), Path(sys.argv[3]))
    else:
        sys.exit(__doc__)
