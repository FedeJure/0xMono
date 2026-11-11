#!/usr/bin/env bash
# Vuelve a sacar las capturas de pantallas del aparato (menú, configuración,
# hora, minijuego) y de la página de configuración, y las deja en ../img/.
#
#   docs/manual/tools/screens.sh
#
# Las fotos del mono en su casa salen de renders.sh.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
IMG="$HERE/../img"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
TOOLCHAIN="${TOOLCHAIN:-+stable}"

cd "$ROOT"
cargo $TOOLCHAIN run -q -p simulator --example menu -- --png "$WORK/ui" >/dev/null
cargo $TOOLCHAIN run -q -p simulator --release --example games -- --png "$WORK/juego" >/dev/null

# Pantallas del aparato (480 x 480: la pantalla de 240 x 240 a x2).
for name in menu_1_principal menu_3_minijuegos menu_5_configuracion \
            config_2_unirse config_3_abrir hora_1_conectando hora_2_listo hora_3_fallo; do
  cp "$WORK/ui/$name.png" "$IMG/ui-$name.png"
done
for name in 1_portada 3_jugando 4_fin; do
  cp "$WORK/juego/$name.png" "$IMG/cocos-$name.png"
done

# La página de configuración en el celular (ya configurado: red guardada).
LIB="${MANUAL_PYLIB:-$HOME/.cache/0xmono-manual/pylib}"
PYTHONPATH="$LIB${PYTHONPATH:+:$PYTHONPATH}" \
  python3 "$HERE/portal_shot.py" "$WORK/ui/pagina_2_configurada.html" "$IMG"
echo "listo: capturas en $IMG"
