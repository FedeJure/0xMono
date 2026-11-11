#!/usr/bin/env bash
# Arma los PDF del manual.
#
#   docs/manual/build.sh                 los dos PDF
#   docs/manual/build.sh --png carpeta   y además una imagen por página
#
# Necesita python3 y pip. Si faltan weasyprint o pypdf, los instala una vez
# en ~/.cache/0xmono-manual (no toca el python del sistema). Para las vistas
# previas hace falta pdftoppm (paquete poppler-utils).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="${MANUAL_PYLIB:-$HOME/.cache/0xmono-manual/pylib}"
export PYTHONPATH="$LIB${PYTHONPATH:+:$PYTHONPATH}"

if ! python3 -c "import weasyprint, pypdf" 2>/dev/null; then
  echo "instalando weasyprint y pypdf en $LIB ..."
  python3 -m pip install --quiet --target "$LIB" "weasyprint>=62" "pypdf>=4"
fi

python3 "$HERE/build.py" "$@"
