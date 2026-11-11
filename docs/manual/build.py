#!/usr/bin/env python3
"""Arma el manual de 0xMono.

    python3 build.py [--png CARPETA] [--dpi 110]

Genera, al lado de este archivo:
  0xMono-manual.pdf           páginas A5 en orden de lectura (para pantalla)
  0xMono-manual-imprimir.pdf  cuadernillo: A4 apaisado, dos A5 por cara, en
                              orden de abrochado al medio (doble faz, borde corto)

Con --png, además deja una imagen por página A5 y por cara de hoja A4 en
CARPETA, para revisar cómo quedó.
"""
import argparse
import subprocess
import sys
from pathlib import Path

from pypdf import PageObject, PdfReader, PdfWriter, Transformation
from weasyprint import HTML

HERE = Path(__file__).resolve().parent
SRC = HERE / "manual.html"
OUT = HERE / "0xMono-manual.pdf"
BOOKLET = HERE / "0xMono-manual-imprimir.pdf"

MM = 72 / 25.4
A5_W, A5_H = 148 * MM, 210 * MM
A4_W, A4_H = 297 * MM, 210 * MM  # apaisada


def render() -> int:
    doc = HTML(filename=str(SRC)).render()
    doc.write_pdf(str(OUT))
    return len(doc.pages)


def saddle_order(n: int) -> list[tuple[int, int]]:
    """Pares (izquierda, derecha) de cada cara, en orden de impresión.

    Para n páginas (múltiplo de 4), la hoja i lleva adelante n-2i | 2i+1 y
    atrás 2i+2 | n-2i-1 (contando desde 1). Con doble faz por el borde corto
    el dorso sale derecho.
    """
    assert n % 4 == 0
    sides = []
    for i in range(n // 4):
        sides.append((n - 2 * i, 2 * i + 1))
        sides.append((2 * i + 2, n - 2 * i - 1))
    return sides


def impose(pages: int) -> None:
    reader = PdfReader(str(OUT))
    n = (pages + 3) // 4 * 4
    writer = PdfWriter()
    for left, right in saddle_order(n):
        sheet = PageObject.create_blank_page(width=A4_W, height=A4_H)
        for num, x in ((left, 0), (right, A5_W)):
            if num <= pages:
                sheet.merge_transformed_page(
                    reader.pages[num - 1], Transformation().translate(tx=x, ty=0)
                )
        writer.add_page(sheet)
    writer.add_metadata({"/Title": "0xMono · Manual para imprimir"})
    with open(BOOKLET, "wb") as f:
        writer.write(f)


def previews(folder: Path, dpi: int) -> None:
    folder.mkdir(parents=True, exist_ok=True)
    for pdf, prefix in ((OUT, "pagina"), (BOOKLET, "hoja")):
        subprocess.run(
            ["pdftoppm", "-png", "-r", str(dpi), str(pdf), str(folder / prefix)], check=True
        )


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--png", type=Path, help="carpeta para las vistas previas")
    ap.add_argument("--dpi", type=int, default=110)
    args = ap.parse_args()

    pages = render()
    print(f"{OUT.name}: {pages} páginas A5")
    if pages % 4:
        print(f"  ojo: {pages} no es múltiplo de 4, el cuadernillo lleva páginas en blanco",
              file=sys.stderr)
    impose(pages)
    print(f"{BOOKLET.name}: {(pages + 3) // 4} hojas A4 doble faz")
    if args.png:
        previews(args.png, args.dpi)
        print(f"vistas previas en {args.png}")


if __name__ == "__main__":
    main()
