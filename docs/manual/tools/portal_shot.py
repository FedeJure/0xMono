#!/usr/bin/env python3
"""Captura de la página de configuración, como se ve en un celular.

    python3 portal_shot.py <pagina.html> <img_dir>

Toma el HTML real que sirve el mono (lo exporta el simulador:
`cargo run -p simulator --example menu -- --png <dir>`) y lo dibuja con
WeasyPrint en un celular de 390 px de ancho. WeasyPrint no dibuja
formularios ni entiende `:has()` ni `box-shadow`, así que antes de dibujar
se reemplazan los controles por cajitas con el mismo aspecto (radios,
casillas, campos, el desplegable de zona) y se fijan los estados que la
página resuelve con `:has()` (la opción marcada, la hora automática).
Los textos, colores y el orden son los de la página real.

Genera dos pantallas de 390 x 844 px (a x2):
  portal-phone.png    arriba: el saludo y "Mi WiFi"
  portal-phone-2.png  más abajo, hasta "¡Guardar!" y "Empezar de nuevo"
"""
import html as H
import re
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image
from weasyprint import HTML

FONTS = Path(__file__).resolve().parent.parent / "fonts"
WIDTH, SCREEN_H, SCALE = 390, 844, 2

EXTRA_CSS = f"""
@font-face {{ font-family: Nunito; src: url({(FONTS / 'Nunito-VF.ttf').as_uri()}); font-weight: 200 1000; }}
@page {{ size: {WIDTH}px 2600px; margin: 0; }}
html, body {{ background: var(--bg); }}
body {{ font-family: Nunito, sans-serif; }}
.card {{ box-shadow: none; border: 1px solid #f7ebe1; border-bottom: 5px solid var(--line); }}
.go {{ box-shadow: none; border-bottom: 5px solid var(--pinkd); text-align: center; border-radius: 18px; padding: 1rem; font-weight: 800; font-size: 1.1rem; }}
.wifi .opt.on {{ border-color: var(--lav); background: var(--lavl); }}
.hora .opt.on {{ border-color: var(--mint); background: var(--mintl); }}
.hora .mano {{ display: none; }}
details > form {{ display: none; }}
details summary {{ display: flex; }}
.rad {{ display: block; width: 1.25rem; height: 1.25rem; border-radius: 50%; border: 2px solid #c9b8ad; background: #fff; flex: none; }}
.rad.on {{ border-color: var(--pinkd); background: var(--pinkd); box-shadow: none; border-width: 2px; outline: none; }}
.rad.on::after {{ content: ''; display: block; width: .45rem; height: .45rem; margin: .25rem; border-radius: 50%; background: #fff; }}
.box {{ display: block; width: 1.2rem; height: 1.2rem; border-radius: 4px; border: 2px solid #c9b8ad; background: #fff; flex: none; }}
.inp {{ display: block; margin-top: .3rem; padding: .75rem .9rem; border-radius: 14px; border: 2px solid var(--line); background: #fffaf6; color: var(--ink); font-weight: 400; }}
.inp.ph {{ color: #b9a79c; }}
.inp.sel {{ padding-right: 2.2rem; }}
.inp.sel .caret {{ float: right; margin: .55rem -1.3rem 0 0; width: 0; height: 0; border-left: 6px solid transparent; border-right: 6px solid transparent; border-top: 8px solid var(--ink); }}
"""


def control(tag: str) -> str:
    """Un <input> de la página como cajita dibujable."""
    attrs = {}
    for m in re.finditer(r"""([\w-]+)(?:=("[^"]*"|'[^']*'|[^\s>]+))?""", tag[len("<input"):]):
        v = m.group(2) or ""
        if v[:1] in "\"'":
            v = v[1:-1]
        attrs[m.group(1)] = H.unescape(v)
    kind = attrs.get("type", "text")
    if kind == "hidden":
        return ""
    if kind == "radio":
        return f"<em class='rad{' on' if 'checked' in attrs else ''}'></em>"
    if kind == "checkbox":
        return "<em class=box></em>"
    value = attrs.get("value", "")
    if kind == "date" and re.fullmatch(r"\d{4}-\d\d-\d\d", value):
        y, m, d = value.split("-")
        value = f"{d}/{m}/{y}"
    if kind == "password" and value:
        value = "•" * len(value)
    if value:
        return f"<span class=inp>{H.escape(value)}</span>"
    return f"<span class='inp ph'>{H.escape(attrs.get('placeholder', ''))}</span>"


def prepare(src: str) -> str:
    s = re.sub(r"<script>.*?</script>", "", src, flags=re.S)
    # La opción marcada (la página lo hace con :has()).
    s = re.sub(
        r"<label class=opt>(<input[^>]*\bchecked\b[^>]*>)",
        r"<label class='opt on'>\1",
        s,
    )
    s = re.sub(r"<input[^>]*>", lambda m: control(m.group(0)), s)

    def select(m: re.Match) -> str:
        opt = re.search(r"<option[^>]*\bselected\b[^>]*>([^<]*)</option>", m.group(0)) or re.search(
            r"<option[^>]*>([^<]*)</option>", m.group(0)
        )
        text = opt.group(1)
        if len(text) > 28:
            text = text[:27].rstrip(" ,") + "…"
        return f"<span class='inp sel'><span class=caret></span>{text}</span>"

    s = re.sub(r"<select.*?</select>", select, s, flags=re.S)
    s = re.sub(r"<button class=go>(.*?)</button>", r"<div class=go>\1</div>", s)
    return s.replace("</head>", f"<style>{EXTRA_CSS}</style></head>")


def find_bottom(doc, cls: str) -> float:
    """Borde de abajo (px CSS) de la primera caja con esa clase."""

    def walk(box):
        el = getattr(box, "element", None)
        if el is not None and cls in (el.get("class") or "").split():
            return box.position_y + box.margin_height()
        for child in getattr(box, "children", []) or []:
            y = walk(child)
            if y is not None:
                return y
        return None

    return walk(doc.pages[0]._page_box)


def main() -> None:
    page, out = Path(sys.argv[1]), Path(sys.argv[2])
    src = prepare(page.read_text(encoding="utf-8"))
    doc = HTML(string=src, base_url=str(page.parent)).render()
    end = find_bottom(doc, "again") or 1700
    with tempfile.TemporaryDirectory() as tmp:
        pdf = Path(tmp) / "p.pdf"
        doc.write_pdf(pdf)
        subprocess.run(
            ["pdftoppm", "-png", "-singlefile", "-r", str(96 * SCALE), str(pdf), str(Path(tmp) / "p")],
            check=True,
        )
        full = Image.open(Path(tmp) / "p.png").convert("RGB")
    w, h = WIDTH * SCALE, SCREEN_H * SCALE
    full.crop((0, 0, w, h)).save(out / "portal-phone.png", optimize=True)
    top = int((end + 14 - SCREEN_H) * SCALE)
    full.crop((0, top, w, top + h)).save(out / "portal-phone-2.png", optimize=True)
    print("   portal-phone.png, portal-phone-2.png")


if __name__ == "__main__":
    main()
