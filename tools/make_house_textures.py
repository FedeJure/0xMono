"""Texturas de las pantallas del hábitat (pixel art generado).

    python tools/make_house_textures.py

- assets/house/laptop_code.png (64×128): código con resaltado de sintaxis,
  repetible en vertical; la pantalla muestra la mitad y el juego la desplaza
  para que el código "corra" mientras el mono programa.
- assets/house/monitor_chart.png (64×32): gráfico de velas de un mercado.
"""

import os
import random

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "assets", "house")

BG = (16, 22, 34)
KEYWORD = (98, 160, 255)
TYPE = (190, 140, 255)
STRING = (120, 214, 138)
NUMBER = (255, 168, 76)
COMMENT = (96, 108, 128)
IDENT = (214, 222, 234)
GUTTER = (52, 62, 80)


def code(w=64, h=128, seed=7):
    rnd = random.Random(seed)
    img = Image.new("RGB", (w, h), BG)
    px = img.load()
    line_h = 4
    indent = 0
    for row in range(h // line_h):
        y = row * line_h + 1
        # Número de línea en el margen.
        for x in range(1, 4):
            px[x, y] = GUTTER
        kind = rnd.random()
        if kind < 0.12:
            indent = max(0, indent - 1)
            toks = [(IDENT, 1)]  # "}"
        elif kind < 0.22:
            toks = [(COMMENT, rnd.randint(10, 26))]
        elif kind < 0.30:
            toks = []  # línea en blanco
        else:
            toks = []
            if rnd.random() < 0.5:
                toks.append((KEYWORD, rnd.choice([2, 3, 4])))
            toks.append((rnd.choice([IDENT, TYPE]), rnd.randint(3, 9)))
            for _ in range(rnd.randint(0, 3)):
                toks.append((rnd.choice([IDENT, STRING, NUMBER, IDENT, TYPE]), rnd.randint(2, 8)))
            if rnd.random() < 0.25:
                toks.append((IDENT, 1))  # "{"
                opened = True
            else:
                opened = False
        x = 6 + indent * 3
        for color, n in toks:
            for i in range(n):
                if x + i < w - 1:
                    px[x + i, y] = color
                    px[x + i, y + 1] = color
            x += n + 2
        if kind >= 0.30 and toks and opened:
            indent = min(4, indent + 1)
    return img


def chart(w=64, h=32, seed=3):
    rnd = random.Random(seed)
    img = Image.new("RGB", (w, h), (12, 16, 26))
    px = img.load()
    for y in range(4, h, 7):
        for x in range(0, w, 2):
            px[x, y] = (30, 38, 54)
    price = h * 0.6
    for i in range(14):
        x = 2 + i * 4 + 1
        o = price
        c = max(4, min(h - 4, o + rnd.uniform(-5, 5) - 0.6))
        hi = min(o, c) - rnd.uniform(1, 3)
        lo = max(o, c) + rnd.uniform(1, 3)
        up = c < o
        col = (64, 210, 120) if up else (238, 84, 84)
        for y in range(int(hi), int(lo) + 1):
            if 0 <= y < h:
                px[x + 1, y] = col
        for y in range(int(min(o, c)), int(max(o, c)) + 1):
            for dx in range(3):
                if 0 <= y < h:
                    px[x + dx, y] = col
        price = c
    return img


def main():
    os.makedirs(OUT, exist_ok=True)
    code().save(os.path.join(OUT, "laptop_code.png"))
    chart().save(os.path.join(OUT, "monitor_chart.png"))
    print("texturas en", os.path.relpath(OUT, ROOT))


if __name__ == "__main__":
    main()
