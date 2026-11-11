#!/usr/bin/env bash
# Vuelve a sacar las fotos del mono con el simulador y las deja en ../img/.
#
#   docs/manual/tools/renders.sh
#
# Corre el simulador en una carpeta temporal con un personaje fijo
# (mono-semilla.bin), así las fotos salen siempre del mismo mono.
# Las capturas del menú, la configuración y el minijuego NO salen de acá
# (ver README.md del manual).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
IMG="$HERE/../img"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

TOOLCHAIN="${TOOLCHAIN:-+stable}"
(cd "$ROOT" && cargo $TOOLCHAIN build -q -p simulator --release --example habitat)
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BIN="$TARGET_DIR/release/examples/habitat"

cp "$HERE/mono-semilla.bin" "$WORK/companion.bin"
cd "$WORK"
mkdir raw

# foto <nombre> <HH:MM> <segundos> <actividad> [ángulo de cerca]
foto() {
  local name="$1" time="$2" secs="$3" act="$4" near="${5:-}"
  if [[ -n "$near" ]]; then
    "$BIN" --png "raw/$name.png" "$time" "$secs" --act "$act" --near "$near" >/dev/null
  else
    "$BIN" --png "raw/$name.png" "$time" "$secs" --act "$act" >/dev/null
  fi
}

# Tapa y bienvenida
foto s-cover     10:30 4 "Programando" 0
foto s-hola      08:30 3 "Comiendo" -20

# Un día en la vida
foto t-wake      07:30 4 "Despertándose"
foto t-stretch   08:00 4 "Elongando"
foto t-breakfast 08:30 6 "Comiendo" 0
foto t-mate      09:15 4 "Tomando mate" 0
foto t-code      10:30 4 "Programando"
foto t-meeting   11:30 4 "En una reunión" -20
foto t-cook      12:45 4 "Cocinando"
foto t-trading   15:00 6 "Mirando el mercado" 0
foto t-nap       15:30 4 "Siesta"
foto t-snack     17:00 6 "Merendando" 30
foto t-bananas   17:30 4 "Buscando bananas"
foto t-garden    18:00 6 "Regando plantas" 0
foto t-guitar    19:30 6 "Tocando la guitarra" 0
foto t-read      23:00 4 "Leyendo" -20
foto t-games     21:30 6 "Jugando a la consola" 0
foto t-movie     22:30 4 "Viendo una película"
foto t-stars     23:30 4 "Mirando las estrellas"
foto t-sleep     01:00 4 "Durmiendo"
foto t-away      15:00 4 "De visita"

# Hablarle: globo de diálogo (blanco) y de pensamiento (lila)
"$BIN" --png raw/s-say.png 21:30 4 --act "Jugando a la consola" --near 0 --say >/dev/null
"$BIN" --png raw/s-think.png 10:30 4 --act "Programando" --near 0 --think >/dev/null

# Cosas que pasan
foto g-sweat     09:00 6 "Ejercicio" 0

# La luz según la hora (el jardín)
foto l-0640      06:40 4 "Regando plantas"
foto l-1230      12:30 4 "Regando plantas"
foto l-1910      19:10 4 "Regando plantas"
foto l-2330      23:30 4 "Regando plantas"

# Gags: hojas de cuadros (8 por fila), después se elige uno.
"$BIN" --seq raw/seq-fire.png Cocinando:24 12 --gag 1:Fuego --look 1.75,0.6,0.85,1.5,2.1,0.45 >/dev/null
"$BIN" --seq raw/seq-fart.png Siesta:8 8 15:00 --near 20 --gag 3:Pedo >/dev/null
"$BIN" --faces "Hablando por teléfono" raw/seq-faces.png >/dev/null
"$BIN" --seq raw/seq-dance.png Bailando:16 15 18:00 >/dev/null

python3 "$HERE/img_tools.py" renders raw "$IMG"
echo "listo: fotos en $IMG"
