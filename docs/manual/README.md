# Manual de 0xMono

Cuadernillo A5 de 16 páginas, para regalar junto con el aparato.

| Archivo | Para qué |
|---|---|
| `0xMono-manual.pdf` | Páginas A5 en orden de lectura: para verlo en pantalla o mandarlo. |
| `0xMono-manual-imprimir.pdf` | Para imprimir: 4 hojas A4 apaisadas, dos páginas por cara, ya ordenadas para doblar y abrochar al medio. |

## Cómo imprimirlo

1. **Papel:** A4 común de 80–90 g. Si querés una tapa más firme, la primera hoja (la que lleva la tapa y la contratapa) puede ir en papel de 120–160 g o en ilustración mate.
2. **Doble faz, girando por el borde corto** ("voltear en borde corto" / "flip on short edge"). Tamaño real, al 100 % (sin "ajustar a la página"). A color.
3. Sin impresora doble faz: imprimí las caras impares (1, 3, 5, 7), dá vuelta la pila sin cambiarle el orden y metela de nuevo, y después las pares (2, 4, 6, 8). Probá antes con una sola hoja para ver de qué lado entra el papel.
4. **Armado:** poné las 4 hojas una dentro de la otra, en orden (la hoja 1, la de la tapa, va por fuera), doblalas juntas al medio y marcá bien el doblez con una regla o la uña.
5. **Abrochado (opcional):** dos broches en el doblez, a unos 5 cm de cada borde. Con una abrochadora común: abrí el cuadernillo boca abajo sobre una goma o un cartón, abrochá desde el lomo y cerrá las patitas a mano.

Las páginas dejan 16 mm del lado del doblez y unos 12 mm afuera, así que entra bien en cualquier impresora hogareña.

## Cómo rearmarlo

```sh
docs/manual/build.sh                       # los dos PDF
docs/manual/build.sh --png /tmp/vista      # y además una imagen por página y por hoja, para revisar
```

- Usa **WeasyPrint** (HTML + CSS a PDF) y **pypdf** (imposición del cuadernillo). Si no están, `build.sh` los instala una sola vez con pip en `~/.cache/0xmono-manual` (se puede cambiar con `MANUAL_PYLIB`). WeasyPrint necesita Pango, que viene en cualquier Linux de escritorio. Las vistas previas usan `pdftoppm` (poppler-utils).
- La fuente es `manual.html` + `manual.css`. Las tipografías van incluidas en `fonts/` (licencia SIL OFL, ver `fonts/OFL-*.txt`): Space Grotesk (títulos), Inter (texto) y JetBrains Mono (detalles). Nunito solo la usa `tools/portal_shot.py`, porque es la que muestra la página de configuración.
- La cantidad de páginas tiene que ser múltiplo de 4. Si al editar sobra o falta texto, `build.sh` avisa y rellena el cuadernillo con páginas en blanco: mejor revisar las vistas previas y acomodar.

## Imágenes (`img/`)

- **Fotos del mono** (`s-*`, `t-*`, `g-*`, `l-*`, `f-*`): salen del simulador con `tools/renders.sh` (usa el personaje fijo de `tools/mono-semilla.bin` y `tools/img_tools.py` para recortar y agrandar x3 sin suavizar).
- **Pantallas del aparato** (`ui-*`: menú, configuración, hora; `cocos-*`: el minijuego) y **la página de configuración en el celular** (`portal-phone.png`, `portal-phone-2.png`): salen de `tools/screens.sh`, que corre los ejemplos `menu` y `games` del simulador y dibuja con `tools/portal_shot.py` el HTML real de la página (WeasyPrint no dibuja formularios, así que reemplaza los controles por cajitas iguales; textos y colores son los de la página).
- **Dibujos** (`*.svg`): bananas, hojas, estrellas, cocos, corazón, el aparato de frente y de atrás. El QR de GitHub es `qr-github.svg`.

Si cambia el firmware (textos del menú, la página o el juego), correr `tools/screens.sh`, después `build.sh --png`, y revisar las páginas 4 y 9 a 12.
