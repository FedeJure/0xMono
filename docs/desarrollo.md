# 0xMono por dentro

Notas técnicas para quien quiera entender o cambiar el firmware. El [README](../README.md) tiene lo necesario para armarlo, grabarlo y probarlo; esto es lo que hay debajo.

- [Mapa del código](#mapa-del-código)
- [Comandos](#comandos)
- [Modos de arranque y memoria](#modos-de-arranque-y-memoria)
- [Configuración desde el celular](#configuración-desde-el-celular)
- [Hora por internet](#hora-por-internet)
- [Guardado en la flash](#guardado-en-la-flash)
- [Botones y menú](#botones-y-menú)
- [Letras en pantalla](#letras-en-pantalla)
- [Motor 3D](#motor-3d)
- [Del modelo de Blender al juego](#del-modelo-de-blender-al-juego)
- [La cara](#la-cara)
- [Diálogos](#diálogos)
- [Animaciones (Mixamo)](#animaciones-mixamo)
- [La casa y el jardín](#la-casa-y-el-jardín)
- [La vida en la casa](#la-vida-en-la-casa)
- [Rendimiento en el ESP32-C3](#rendimiento-en-el-esp32-c3)
- [Sonido (DY-SV17F)](#sonido-dy-sv17f)
- [Consumo](#consumo)
- [Modo minero](#modo-minero)

## Mapa del código

| Carpeta | Qué es |
|---|---|
| `crates/companion/` | El personaje: agenda diaria con el reloj real, actividades, personalidad, reacciones, emociones con parpadeo, frases y guardado versionado. |
| `crates/habitat/` | La vida del mono en la casa: a dónde va según la agenda, caminos que esquivan muebles, sentarse corriendo la silla, acostarse, objetos en la mano, gags (pedos, incendios en la cocina, sudor), luz según la hora, cámara por habitación y el globo de diálogo. |
| `crates/engine/` | Motor 3D por software (crate `engine3d`): punto fijo, franjas con z-buffer, materiales toon, texturas de 4 bits y skinning rígido. |
| `assets/` | Personaje y casa exportados de Blender (`src/mono.rs`, `src/house.rs`, `src/mono_anims.rs`, generados) y su puesta en escena: materiales, expresiones, esqueleto, animaciones, mapa de navegación (`nav`) y el dibujo de lo que cambia (`stage`). Imágenes fuente en `assets/face/` y `assets/house/`. |
| `crates/ui/` | El menú (A+C) y las pantallas de la configuración y de la hora, con `embedded-graphics`; dibujo por franjas para la placa. |
| `crates/games/` | Los minijuegos (por ahora, Cocos Rodantes): lógica y dibujo, sin memoria dinámica. |
| `crates/fonts/` | Las letras de la pantalla: Helvetica negrita y Free Universal Bold de U8g2 (`u8g2-fonts`), proporcionales y con todo el Latin-1, en cuatro tamaños pensados para 1,3" (mayúsculas de 12 a 25 px); medir, cortar en renglones por ancho y dibujar salteando lo que no toca la franja. |
| `crates/settings/` | La configuración del aparato: el registro guardado, la página para el celular, husos horarios, SNTP y el pedido que pasa de un arranque al siguiente. |
| `crates/captive/` | Lo que comparten los portales: HTTP mínimo, DNS cautivo, redes encontradas, QR para unirse y por qué falló una conexión. |
| `crates/sound/` | Las tramas UART del reproductor DY-SV17F. |
| `crates/miner/` | Modo minero (experimental): configuración desde el celular, validación de direcciones de Bitcoin, cliente Stratum, guardado y pantallas. |
| `firmware/` | Programa del ESP32-C3 (esp-hal 1.2 + Embassy), target `riscv32imc-unknown-none-elf`, fuera del workspace. Bins: `casa` (el aparato), `miner` y los benchmarks `bench`, `bench_mono`, `bench_habitat`, `bench_sha`. |
| `simulator/` | Ejemplos de escritorio con el mismo código que la placa. |
| `tools/` | Scripts de Blender y de Python para el arte (ver abajo). |

Todo lo de `crates/` es `no_std`, sin hardware y con tests: corre igual en la PC y en el ESP32-C3. Lo que toca pines vive en `firmware/`.

## Comandos

```sh
cargo test --workspace                                   # todos los tests de la PC
cargo test -p companion                                  # la vida del personaje
cargo test -p ui -p settings -p captive                  # menú, página, hora por internet, pedidos entre arranques

# El hábitat
cargo run -p simulator --release --example habitat       # con la hora real (A/S/D hablarle, T acelerar, H +1 hora, N mono nuevo,
                                                         #   C cámara general, R zona redibujada, G un gag)
cargo run -p simulator --release --example habitat -- --png foto.png 14:30 4 --act Cocinando --near 110    # foto de una actividad, de cerca
cargo run -p simulator --release --example habitat -- --png mano.png 14:30 3 --act "Tomando mate" --hand Right 60   # la mano, de muy cerca
cargo run -p simulator --release --example habitat -- --seq hoja.png "Bailando:1,Durmiendo:13,Bailando:10" 10   # actividades encadenadas
cargo run -p simulator --release --example habitat -- --seq hoja.png Cocinando:24 12 --gag 1:Fuego --look 1.75,0.6,0.85,1.5,2.1,0.45  # un gag, cámara fija
cargo run -p simulator --release --example habitat -- --faces "Tomando mate" caras.png   # el repertorio de caras de una actividad
cargo run -p simulator --example week                    # la agenda de la semana del personaje guardado
cargo run -p simulator --example talk [por actividad]    # muestras de lo que dice y piensa
cargo run -p simulator --release --example character     # visor del personaje (E ojos, M boca, Z acercar, L capas)

# Menú, configuración y hora
cargo run -p simulator --example menu                    # ventana (A/S/D = A/B/C; A+D 2 s = A+C)
cargo run -p simulator --example menu -- --png carpeta   # cada pantalla como PNG (+ la página en HTML)
cargo run -p simulator --example menu -- --serve         # la página de configuración en http://127.0.0.1:8080
cargo run -p simulator --example ntp [minutos]           # la hora por internet como la pide el mono, desde la PC
cargo run -p simulator --release --example games         # los minijuegos (--png carpeta: capturas)

# En la placa
cd firmware && cargo run --release                       # `casa`: compila, graba por USB-C y abre el monitor
cd firmware && cargo run --release --bin bench_habitat   # tiempos del hábitat (--features profile: ciclos por etapa)
```

## Modos de arranque y memoria

El WiFi con la red del mono y la página (~80 KB de memoria dinámica) y el renderer 3D (~165 KB) no entran juntos en los 320 KB de RAM del ESP32-C3. Cada arranque es de un solo modo, y para cambiar se reinicia dejando el pedido en la RAM del RTC (`settings::handoff`, que sobrevive al reinicio pero no a desenchufarlo):

- **Casa** (`firmware/src/bin/casa/house.rs`): el mono, el menú y los juegos.
- **Configuración** (`net.rs`): la red del mono con la página para el celular.
- **Hora por internet** (`net.rs`): se conecta al WiFi guardado, pregunta la hora y vuelve a la casa.

| | Casa (3D) | Configuración / hora (WiFi) |
|---|---|---|
| renderer (RAM principal, 164 KB) | el 3D | memoria dinámica |
| RAM que devuelve el arranque (64 KB) | franjas de color y profundidad, hábitat y juego | memoria dinámica |
| pila (19 KB) | sobran ~9 KB | sobran ~10 KB |

En los modos WiFi quedan 233 KB de memoria dinámica (150 KB libres con la red del mono y la página andando). Armar el hábitat necesita ~23 KB de pila de paso, más que la pila entera: se arma una sola vez, al arrancar, usando de pila el lugar de las franjas (`firmware/src/bin/casa/memory.rs`).

Después de un reinicio (guardar en la página, ponerse en hora), la casa arranca con un aviso ("¡En hora!", "No pude ponerme en hora", "¡Hola, soy nuevo!"...), que también viaja en el pedido.

## Configuración desde el celular

**Configuración → Desde el celular** reinicia la placa en modo WiFi: busca las redes, arma la suya, `0xMono-XXXX` (los dos últimos bytes de la MAC) con una clave al azar de 8 letras y números, y muestra un QR para unirse. El DHCP le dice al celular que el DNS es el mono y el DNS contesta a todo con `192.168.4.1`, así el chequeo de internet del teléfono abre la página solo (si no, `http://192.168.4.1`). La página no usa nada de internet (estilos, el mono en SVG y un poco de JavaScript van en la misma respuesta) y tiene:

- **Mi WiFi:** las redes encontradas (de la más fuerte), otra red escrita a mano o "Sin WiFi por ahora", y la clave (vacía = la misma de antes). Solo 2,4 GHz.
- **La hora:** automática por internet (por defecto) con la zona horaria (por defecto Argentina, UTC−3; si el mono es nuevo, toma la del celular), o a mano con fecha y hora precargadas del celular (sin JavaScript se escriben).
- **Empezar de nuevo:** "Mono nuevo" (llega otro personaje, con otra semilla; la configuración no se toca) y "Restablecer de fábrica" (borra el WiFi, la hora, el mono y la configuración del minero). Cada uno pide tildar una confirmación.

Al guardar muestra "¡Listo!" y se reinicia: con WiFi y hora automática, primero se pone en hora; si no, vuelve a la casa con la hora elegida. C en la placa vuelve a la casa sin cambiar nada (a los 10 minutos, también).

El servidor HTTP, el DNS cautivo, la lista de redes y los motivos de una conexión fallida están en `crates/captive/`; la página y el registro de la configuración, en `crates/settings/`; la parte de la placa (red propia, DHCP con `edge-dhcp`, conexión a una red), en `firmware/src/captive_net.rs` y `firmware/src/bin/casa/net.rs`.

## Hora por internet

Sin batería, el reloj se frena al desenchufar. Al enchufarlo, si hay WiFi y la hora automática, la placa arranca en un modo corto: "Poniéndome en hora...", se conecta, pregunta por SNTP (`pool.ntp.org`, `time.google.com`, `time.cloudflare.com`; `settings::sntp`), aplica la zona y vuelve a la casa con la hora. Si en 20 segundos no lo logra (30 si se pidió desde el menú), sigue con la hora guardada y avisa por qué.

Las zonas son desplazamientos fijos (`settings::tz`): no hay horario de verano. `companion::LocalTime` cuenta días desde el 1970-01-01, así que el día de la semana (y la agenda de sábados y domingos) sale de la fecha real. Sin WiFi, mantener B en la casa adelanta el reloj una hora.

## Guardado en la flash

`casa` guarda el personaje (su semilla), la última vez que te vio y la hora: al hablarle, cada 5 minutos y antes de reiniciarse en otro modo. Desenchufado, el reloj se frena y sigue desde la última hora guardada (o la de internet, si hay WiFi). Sin WiFi ni hora puesta, la primera vez arranca a las 8.

El firmware no usa el NVS de ESP-IDF: reparte a mano la partición `nvs` (la de la tabla por defecto de espflash: `0x9000`, 6 sectores de 4 KB) así (`firmware/src/nvs.rs`):

| Sector | Dirección | Qué guarda |
|---|---|---|
| 0 | `0x9000` | configuración del minero (`miner::Config`) |
| 1–2 | `0xA000`–`0xBFFF` | el personaje y la hora (`companion::save::Journal`) |
| 3 | `0xC000` | la configuración del aparato (`settings::Settings`: WiFi, hora automática, zona) |
| 4–5 | `0xD000`–`0xEFFF` | libres |

Grabar un firmware nuevo no toca la partición: el mono sobrevive a las actualizaciones. `espflash erase-flash` sí la borra.

La configuración es un registro con marca, versión y CRC32 (`settings::Settings::encode`): una flash borrada o un guardado cortado no pasan la verificación y queda la de fábrica.

`companion::save::Journal` usa dos sectores. Cada guardado es un registro nuevo con número y CRC. Si se corta la corriente, el registro incompleto no pasa el CRC y se lee el anterior. Cuando un sector se llena se borra el otro, nunca el que tiene lo último. Los tests cortan la corriente de una flash simulada en distintos bytes de escrituras y borrados.

## Botones y menú

`companion::Pad` convierte los tres botones en gestos: toques, B mantenido y A+C mantenidos 2 s (`Input::Chord`), sin disparar los toques de A ni de C.

- **En la casa:** cualquier toque le habla (y, con el módulo de sonido, suena un pitido). Mantener B adelanta el reloj una hora.
- **A+C mantenidos 2 s** abren el menú desde cualquier lado (la casa o un juego). En el menú: **A baja, B sube, C elige**; las listas dan la vuelta.

```text
Menú
├── Volver con el mono
├── Minijuegos
│   ├── (cada juego de games::GameId::ALL)   al terminar vuelve a esta lista
│   └── Volver
└── Configuración
    ├── Desde el celular                     reinicia en modo configuración
    ├── Hora por internet                    reinicia para pedir la hora (sin WiFi: aviso)
    ├── Acerca del 0xMono                    versión, WiFi y modo de la hora
    └── Volver
```

El menú es una lista de datos (`crates/ui/src/menu.rs`): para agregar una opción se suma un `Item` a su página; lo que el menú no resuelve solo (ir a la casa, jugar, reiniciar en otro modo) lo devuelve como `Action` y lo atiende `firmware/src/bin/casa/house.rs`. En el menú el reloj y el guardado siguen andando.

Los juegos se dibujan enteros en cada fotograma, por franjas: ~66 ms por fotograma (15 por segundo), casi todo el envío por SPI a 20 MHz.

## Letras en pantalla

La pantalla mide 1,3": 240 píxeles en 23 mm. Una letra de 7×13 queda de 1 mm y no se lee con el aparato en la mano, así que todo va con fuentes de U8g2 (`crates/fonts/`): Helvetica negrita para textos y Free Universal Bold para títulos, proporcionales y con todo el Latin-1 (á, ñ, ü, ¡, ¿).

| Letra | Mayúscula | Para qué |
|---|---|---|
| `HUGE` | 25 px | la hora grande |
| `TITLE` | 20 px | títulos, la clave del WiFi |
| `BODY` | 14 px | textos, opciones del menú, el globo |
| `SMALL` | 12 px | ayudas ("A baja · B sube · C elige") |

Se dibuja por franjas: `Font::draw` saltea el renglón entero si no toca la franja, así que lo que no se ve no cuesta nada.

## Motor 3D

`crates/engine/` (crate `engine3d`): rasterizador por software en punto fijo (el ESP32-C3 no tiene FPU), por franjas de 240×24 con z-buffer, materiales toon con dithering y skinning rígido.

Caras con expresiones: un material puede llevar una textura de 4 bits (`Texture`) cuyos texels son **índices de otros materiales**, así la cara se ilumina con las mismas rampas que el cuerpo. Las expresiones son celdas de una hoja de sprites; cambiar de expresión es cambiar el desplazamiento (`TexMap::offset`). Ejemplo en `crates/engine/src/demo.rs` (`FACE_SHEET`, `set_face`).

```sh
cargo test -p engine3d
cargo run -p simulator --release --example engine_demo            # ventana 240×240
cargo run -p simulator --release --example engine_demo -- --bench
cd firmware && cargo run --release --bin bench                    # tiempos reales en el ESP32-C3 (solo USB-C)
cd firmware && cargo run --release --bin bench_mono               # el personaje en el ESP32-C3
```

## Del modelo de Blender al juego

1. En Blender (pestaña Scripting), con el personaje abierto:
   ```python
   exec(open(r"<repo>/tools/blender/export_character.py").read(),
        {"OBJECT": "mono", "OUT": r"<repo>/assets/mono.json"})
   ```
   Exporta la pose de reposo triangulada: posiciones, normales por cara, color plano por cara (atributo `Col`), UV de los materiales con textura, hueso dominante por vértice y esqueleto.
2. `python tools/build_assets.py` (agregar `--preview <carpeta>` para ver las texturas convertidas) genera `assets/src/mono.rs`: ejes del motor (1 m = 1024), texturas reducidas al tamaño de pantalla con 4 bits por texel y paleta propia, y los colores unificados con los de la malla. El tamaño final de cada parte con textura se ajusta en `PARTS`.
3. `cargo test -p assets` comprueba que el personaje sea válido.

Convenciones del modelo: color de vértice por esquina (atributo `Col`); materiales con textura con un nodo Image Texture y un nodo UV Map; las tiras de expresiones (`Eye`, `Mouth`) son verticales, con la expresión por defecto arriba y la UV apuntando a esa celda. El orden de las celdas es el de `companion::face::{Eyes, Mouth}`.

El `.blend` del personaje no está en el repo: está su exportación (`assets/mono.json`) y las imágenes de la cara (`assets/face/`), que alcanzan para regenerar todo menos las animaciones. El de la casa sí (`assets/house.blend`).

## La cara

`companion::face`:

- **Repertorio por emoción:** cada `Emotion` tiene combinaciones de ojos y boca con su peso y su duración. Una sonrisa grande dura menos de un segundo y una cara tranquila, unos segundos. `FaceAnimator` va eligiendo la siguiente sin repetir.
- **Transiciones:** la boca pasa por las bocas intermedias en vez de saltar.
- **Al hablar:** en reuniones y por teléfono cambia de sílaba en sílaba.
- **Comiendo y en el ejercicio:** abre la boca para morder y mastica con el cachete inflado, al ritmo de `eat_banana`. Haciendo ejercicio (`Emotion::Exerting`) aprieta los dientes y saca la lengua de cansado. Esas bocas están al final de `mouth.png` (la UV de Blender se hizo con 6 celdas: `build_assets.py` la reescala, ver `uv_cells`).
- **Por actividad:** hay emociones para cada una, como concentrado al programar, juguetón al bailar, relajado con el mate y conversador por teléfono. Mirando el mercado, el humor sube y baja cada pocos minutos.
- **Parpadeo:** intervalos irregulares de 2–6 s, cierre rápido y apertura más lenta, a veces doble, y siempre al cambiar de emoción. Es lento con sueño, escaso concentrado o enojado, y no parpadea dormido.

Para verlo: `cargo run -p simulator --release --example habitat -- --faces "Tomando mate" caras.png`.

## Diálogos

`companion::talk` y `companion::lines`:

- **Frases:** unas 650 reglas escritas a mano, con la voz del mono: dev freaky de las cripto, gamer, fan de las bananas y de los fines de semana tranquilos.
- **Condiciones:** cada regla vale según la actividad, la hora, si es día de semana, sábado o domingo, el humor del día, sus gustos y cómo viene el mercado (`companion::market`). El mercado es el mismo que se le nota en la cara.
- **Huecos:** `{coin}`, `{pct}`, `{bug}`, `{game}`, `{feature}`, `{did}` (algo que hizo hoy), `{next}` (lo que le toca después), etc. Cada regla da muchas variantes.
- **Te contesta:** cuando le hablás, saluda la primera vez y después contesta, con la boca moviéndose.
- **Piensa solo:** cada 40 a 120 segundos piensa en voz alta (globo lila) y no repite las últimas 16.
- **Para agregar frases:** sumá reglas en `crates/companion/src/lines.rs`. Los tests controlan que sean cortas (90 letras), que no queden huecos sin llenar y que entren en el globo con su letra (4 renglones, `habitat::bubble`).
- **El globo en pantalla** (`habitat::bubble`): lo dibuja `Habitat::render` encima de cada franja, así que sale igual en la placa y en el simulador. Va arriba, corrido hacia la cara y con el piquito (o las burbujitas) apuntándole; si la cara está arriba, va abajo. Mientras dura la frase se queda quieto: solo se redibuja su zona cuando aparece, cambia o se va, y cada franja redibujada que lo toca lo vuelve a pintar. La letra es `fonts::BODY` (mayúsculas de 14 px), proporcional: los renglones se cortan por ancho en píxeles (unas 22 letras), hasta 4.

```sh
cargo run -p simulator --example talk [por actividad]   # muestras de lo que dice y piensa en cada actividad
cargo run -p simulator --release --example habitat -- --png globo.png 14:30 3 --act Cocinando --say      # o --think
cargo run -p simulator --release --example habitat -- --seq hoja.png Paseando:16 30 --say 1 --think 9     # aparece, dura y se va
```

## Animaciones (Mixamo)

Las animaciones base son de [Mixamo](https://www.mixamo.com) (Adobe). Sus términos permiten usarlas en un proyecto, pero no redistribuir los archivos, así que el repo trae solo el resultado ya convertido: `assets/mono_anims.json` (procesadas), `assets/mono_anims_extra.json` (las generadas por código) y `assets/src/mono_anims.rs` (lo que usa el juego). Para cambiar algo del juego no hace falta bajarlas; solo para volver a procesarlas o sumar otras.

**Las que usa el mono** (`assets/anims/anim_<nombre>.fbx`, ignoradas por git):

```text
air_squat  angry_gesture  cheering_gesture  clapping  crying  defeated_1  defeated_2
dizzy_idle  excited_gesture  happy_walk  hip_hop_dance  hip_hop_flair  idle  idle_bored
idle_feet  idle_looking_around  idle_to_push_up  jumping_jack  laying_down_from_sitted
left_turn  push_up  right_turn  sit_to_stand  sitting_idle  sitting_typing_computer
sleeping  sleeping_with_movement  stand_to_sit  surprised  victory_1  victory_2  victory_3
walking  waving
```

El nombre de archivo es el de Mixamo en minúsculas y con guiones bajos ("Air Squat" → `anim_air_squat.fbx`); cuando Mixamo tiene varias con el mismo nombre, van numeradas (`victory_1`, `victory_2`...). El nombre del clip es el del archivo sin `anim_`, y el juego busca los clips por ese nombre (`clip("walking")`): si una ya no está en Mixamo, sirve otra parecida guardada con el mismo nombre de archivo.

1. En Mixamo, subir el personaje (exportado del `.blend` como FBX) y bajar cada animación en FBX, **Without Skin**, 30 fps, con "In Place" para las que caminan. Guardarlas en `assets/anims/` como `anim_<nombre>.fbx`.
2. Procesarlas con Blender sin interfaz (no modifica el `.blend`):
   ```sh
   blender -b --factory-startup --python tools/blender/process_anims.py -- <mono.blend> assets/mono.json assets/anims assets/mono_anims.json
   ```
   - **Retarget:** sirven las que vienen en pose T (sin piel) y en pose A (con piel).
   - **Brazos contra la cabeza:** la cabeza (con gorra y orejas) se convierte en una grilla 3D; si un brazo entra, se gira desde el hombro lo mínimo necesario y se suaviza en el tiempo. Imprime cuántos cuadros chocaban antes y después.
   - Se pueden pasar nombres al final para procesar solo esas; `--no-fix` las deja sin corregir.
3. `python tools/build_assets.py` genera `assets/src/mono_anims.rs` (pistas constantes guardadas una vez, dedos fijos, avance de las que caminan separado).
4. Verificar: `python tools/preview_anims.py assets/mono_anims.json hoja.png <clip> --compare sin_corregir.json` o `cargo run -p simulator --release --example character -- --sheet hoja.png <clips>`.

Las animaciones que Mixamo no tiene (comer, cosechar, regar, leer, acostarse de costado...) salen de `tools/make_clips.py`, con cinemática inversa sobre una base.

En el juego, `assets::anim::Animator` reproduce una animación base con fundidos y una capa encima; `AnimPose::layer` combina clips por grupos de huesos (`Character::mask_from(hueso)`), por ejemplo piernas de caminar y tronco de saludar. En el visor (`--example character`), la tecla L lo muestra.

## La casa y el jardín

Modelados por código para que las medidas encajen con las animaciones (asientos a 0,10 m, escritorio y mesa a 0,235 m, colchón a 0,20 m, banana del árbol a 0,75 m: ver `tools/measure_anims.py`):

```sh
python tools/make_house_textures.py                                                   # pantallas: código y gráfico
blender -b --factory-startup --python tools/blender/build_house.py -- assets/house.blend [previews]
blender -b assets/house.blend --python tools/blender/export_scene.py -- assets/house.json
python tools/make_clips.py                                                            # comer, cosechar, regar, leer (cinemática inversa)
python tools/build_assets.py                                                          # -> assets/src/house.rs y mono_anims.rs
```

`assets/house.blend` se puede abrir y retocar en Blender; después alcanza con exportar y convertir. Convenciones: objetos con `kind` (`static`, `prop` para lo que va en la mano con el origen en el punto de agarre, `state:<nombre>` para lo que aparece según el estado), empties `spot_*` (lugares del personaje: posición de los pies y hacia dónde mira) y `wp_*` (puntos de paso), cámaras `cam_*` por zona.

En el juego, `assets::world::WorldScene` dibuja el escenario con descarte por objeto fuera de cámara, estados (manta, banana del árbol, regadera) y texturas animadas (el código que corre en la laptop). `assets::stage::Stage` lo junta con el personaje y decide qué redibujar.

`python tools/nav_preview.py nav.png` dibuja el mapa de navegación visto desde arriba.

## La vida en la casa

`crates/habitat/` convierte la agenda de `companion` en movimiento: `plan` dice, para cada actividad, el lugar, la postura, la animación y lo que tiene en la mano; `actor` lo lleva; `camera`, `light`, `gags`, `fx`, `grip` y `bubble` hacen el resto.

**Agenda.** Cada día es distinto, pero recalcularlo da siempre lo mismo (`companion::routine`): se calcula desde la personalidad y la fecha, no se simula minuto a minuto, así que al enchufarlo después de horas el mono está haciendo justo lo que le tocaba. Cambia según el tipo de día:
- **Días de semana:** trabaja de 9 a 18 (el viernes hasta las 17). Programa, tiene una o dos reuniones y mira el mercado, con pausas para un mate o estirar las piernas. Almuerza, a veces cocinando, y merienda.
- **Sábados:** mate, limpieza, cocina, siesta, salidas, consola y baile.
- **Domingos:** mate largo, almuerzo grande, siesta, llamada a la familia y se acuesta más temprano.
- **Humor del día:** cada día tiene uno (vago, con energía, sociable, concentrado, creativo o de aire libre) que pesa en el tiempo libre. Repetir algo el mismo día le da menos ganas.

**Actividades.** Son 26 (`companion::Activity`). Además de comer, dormir, leer y las de siempre:
- **Trabajo:** reunión por videollamada y mirar el mercado.
- **Cocina y mate:** tomar mate en el sillón y cocinar revolviendo la olla.
- **Música:** guitarra sentado en la cama, en diagonal a la cámara para que se vea, y baile.
- **Movimiento:** elongar en la colchoneta, hablar por teléfono caminando y barrer toda la casa.
- **Noche y sillón:** mirar las estrellas en el jardín, consola y película.

**Movimiento.** `tools/build_nav.py` arma, desde la escena:
- una grilla de 5 cm con los obstáculos agrandados por el radio del mono;
- una red de nodos unidos por rectas libres.

El juego busca el camino en la red y lo estira (`assets::nav`), así el mono nunca atraviesa muebles. Para sentarse a una mesa, llega por el costado de la silla, la corre hacia atrás, se para delante, se sienta y se acerca con la silla. Para irse hace lo mismo al revés. Para acostarse entra por el costado: se sienta en el lado largo de la cama mirando a la cámara, se recuesta de lado hacia la almohada y sube las piernas (`lie_down_side`, generada por `make_clips.py` interpolando cada hueso en el mundo entre estar sentado y el primer cuadro de dormir). Para levantarse hace lo mismo al revés (`get_up_side`). El cambio de lugar (del asiento a la cama) se hace justo cuando las dos poses coinciden, sin fundido, así no se desliza. No camina siempre igual: en cada viaje elige entre `happy_walk` (a los saltitos, más seguido si está contento) y `walking`.

**Manos y objetos.** Las manos del mono son enormes:
- en las animaciones que sostienen algo, `make_clips.py` cierra los dedos en puño o a medias (`FIST`, `HALF`) y orienta la muñeca (el puño con el pulgar arriba al revolver, los dedos hacia abajo para colgar la regadera);
- cada objeto de una mano se pega con un agarre (`habitat::grip`) definido en el marco de la mano: dedos, palma y pulgar, calculado igual en Python y en el juego. Así sigue el giro de la muñeca: el mango de la cuchara atraviesa el puño, la regadera y el matafuego cuelgan de su manija, y el teléfono va en la palma (habla en altavoz: contra la oreja la mano lo tapaba);
- `type_laptop` sube cada mano lo justo para que no se hunda en el teclado, el libro se toma de los bordes y la guitarra, la escoba y el joystick van entre las dos manos (la guitarra y el joystick, más grandes que a escala para que se vean).

**Gags** (`habitat::gags`, efectos en `habitat::fx`), cada uno o dos minutos:
- **durmiendo**, se tira un pedo: sale una nubecita verde de abajo de la manta y sonríe dormido;
- **cocinando**, la olla larga humo (lo espanta con la mano), hace una micro explosión (salta del susto) o se prende fuego: va a buscar el matafuego colgado bajo la pileta, lo apaga con espuma y lo devuelve;
- **haciendo ejercicio**, transpira: gotas que saltan de la frente.

Las partículas usan mallas de la escena (`fx_*`) en varios tamaños, porque el motor no escala mallas, y van con lo que se mueve, así que el redibujado parcial las incluye solas.

**Luz según la hora** (`habitat::light`): el cielo, un tinte para todos los materiales y la dirección del sol cambian a lo largo del día. Al amanecer es rosado, al mediodía neutro, en la hora dorada cálido, al anochecer violeta y de noche azul. De noche, adentro y despierto, prende las lámparas: la luz se vuelve cálida y se encienden las pantallas de las lámparas (el material 64 de 64). Dormido, solo entra la luz de la luna. Cambia de a pasos de 6 minutos, y cada paso redibuja todo.

## Rendimiento en el ESP32-C3

La pantalla guarda la imagen, así que con la cámara quieta solo se dibuja y se envía la zona que cambió (el mono ahora y en el fotograma anterior, objetos que aparecen o desaparecen, la pantalla de la laptop, el globo). La cámara queda fija en cada habitación y hace un paneo corto al cambiar de zona, como en un cuadro. El resultado es idéntico píxel a píxel a dibujar todo (hay tests).

| Escena (`assets::shots`) | completo | solo lo que cambia |
|---|---|---|
| programando | 42 ms | 10,3 ms |
| comiendo | 44 ms | 12,3 ms |
| durmiendo | 35 ms | 9,5 ms |
| leyendo | 51 ms | 13,5 ms |
| ejercicio | 38 ms | 11,9 ms |
| bananero | 36 ms | 9,6 ms |

Medido en la placa sin pantalla (falta sumar el envío por SPI, que va por DMA en paralelo). Con el hábitat completo (`habitat::Habitat`: agenda, caminos, sillas, objetos), la vida cuesta menos de 1 ms por fotograma y el dibujo, en promedio, de 11 a 16 ms. Simulando una mañana a x120 (con la luz del amanecer cambiando 120 veces más rápido), el 94 % de los fotogramas fueron parciales, con un 19 % de la pantalla en promedio.

Qué lo hace posible:
- **Solo lo visible en RAM:** cada malla se proyecta en un búfer de trabajo y se guardan solo los triángulos visibles con sus vértices (20 bytes cada uno). Unos 160 KB en total, aunque la escena tenga 12 mil triángulos.
- **El mundo se conserva:** con la cámara quieta, la lista proyectada del escenario se reutiliza (`Renderer::begin_frame_keeping`) y solo se envía el mono.
- **Objetos partidos en trozos de hasta 80 triángulos:** `build_assets.py` los parte, cada uno con su esfera, para descartar lo que no se ve.
- **Escenario de adelante hacia atrás:** lo tapado falla la prueba de profundidad. El rasterizado es exacto aunque se parta la pantalla en franjas o rectángulos.
- **Tono plano con 4 colores por fila ya elegidos,** y tramos con punteros.
- **Luz en el espacio del objeto:** 3 multiplicaciones por cara.
- **Lo crítico en IRAM:** el rasterizado (feature `iram` de `engine3d`) y las texturas (feature `iram` de `assets`) van en RAM; el C3 ejecuta desde flash a través de una caché de 16 KB.
- **La malla del mono en RAM,** y la flash a 80 MHz (DIO; QIO no arranca en la SuperMini, ver `firmware/.cargo/config.toml`).

```sh
cargo run -p simulator --release --example habitat_bench [carpeta]   # uso de búferes, zona redibujada e imágenes de cada escena
cargo run -p simulator --release --example habitat -- --sim 06:00 120 hoja.png 120   # 2 min a x120: % de fotogramas completos
cd firmware && cargo run --release --bin bench_habitat               # tiempos en la placa (--features profile: ciclos por etapa)
```

## Sonido (DY-SV17F)

El código lo soporta, pero el aparato original se armó sin el módulo (los pines 5V y GPIO10 quedan libres; el cableado está en la sección "Más adelante" de la [guía de cableado](cableado.html)). Sin el módulo, las tramas salen por GPIO10 y nadie las escucha.

El ESP32 no reproduce audio: le dice al módulo qué pista tocar con una trama UART a 9600 baudios (`crates/sound/`). Solo se conecta el RX del módulo al GPIO10 (TX de la UART1); su TX queda **sin conectar**, porque es de 5 V.

```text
AA 07 02 00 01 B4    reproducir 00001.mp3
AA 13 01 0F CD       volumen 15 de 30
AA 04 00 AE          parar
```

Formato: `AA`, comando, longitud, datos y un byte final con la suma de todos los anteriores. Los MP3 se copian al módulo por su propio USB (con la placa **desenchufada**), en la raíz y con nombres de 5 cifras. Hoy se usa solo `00001.mp3`, el pitido al tocar un botón; para sumar sonidos hay que agregar una variante a `sound::Sound` y su número de archivo. Sonidos cortos (menos de 1 s), mono; sirven pitidos 8-bit hechos con sfxr/jsfxr y pasados a MP3.

El modo UART del módulo se elige con CON1–CON3 (con una resistencia de ~10 kΩ; la combinación cambia según la revisión, verificar con su hoja de datos). Altavoz de 8 Ω / 1 W a SPK± y volumen inicial 15/30 (`sound::SAFE_VOLUME`). Si la placa se reinicia cuando suena, va un condensador electrolítico de 220–470 µF / 10 V junto al módulo, entre 5V y GND, con la franja hacia GND.

## Consumo

| Estado | Corriente aprox. |
|---|---|
| ESP32-C3 activo, sin radio | 20–30 mA |
| ESP32-C3 con WiFi | picos de 300–350 mA |
| Pantalla con luz de fondo | 30–60 mA |
| DY-SV17F sonando | picos de 100–300 mA |
| LED rojo de la SuperMini | 1–3 mA siempre |

Son estimaciones; con un cargador de celular de 1 A alcanza. Algunos power banks se apagan solos con poco consumo.

## Modo minero

Experimental y aparte: minado "solo" de Bitcoin por diversión (como el NerdMiner), una lotería con chances casi nulas. No está en el menú del firmware principal: es el programa `miner` (`cd firmware && cargo run --release --bin miner`), que reemplaza a `casa` hasta que se vuelva a grabar `casa`. Comparte la partición de guardado, así que el mono no se pierde.

1. **Configuración** (la primera vez, si falla la conexión o manteniendo C): busca las redes de 2,4 GHz, arma su propia red `Mono-XXXX` con clave al azar y muestra un QR para unirse. Con el mismo DNS cautivo que la configuración del aparato, el celular abre la página solo. Ahí se elige la red, la clave y la dirección de Bitcoin (la pública; frases secretas y claves privadas se rechazan). Se guarda en el primer sector de la partición `nvs`.
2. **En línea:** se conecta al WiFi guardado. Si la clave está mal o no encuentra la red, lo explica y vuelve a configuración con el motivo en la página.
3. **Minando** (`miner::stratum` + `firmware/src/bin/miner/hasher.rs`): cliente Stratum v1 contra el pool (por defecto `public-pool.io:21496`, sin comisión). Por nonce son dos compresiones de SHA-256 en el acelerador del ESP32-C3, escribiendo directo en sus registros: **~231 kH/s** (en software, 12 kH/s). Al arrancar compara el acelerador con el software y, si no coinciden, mina en software. La pantalla muestra kH/s, shares, el mejor tiro y hace cuánto mina. public-pool impone dificultad mínima 1: a esta velocidad, un share cada ~5 horas.

El armado del trabajo (transacción de premio, raíz de Merkle, cabecera) está verificado contra el pool real con el ejemplo `stratum`: 7 shares aceptados de 7.

```sh
cargo test -p miner                                        # direcciones, página, DNS, guardado
cargo run -p simulator --example miner                     # la página de configuración en http://127.0.0.1:8080
cargo run -p simulator --example miner -- --png carpeta    # las pantallas del modo minero como PNG
cargo run -p simulator --example miner_view [/dev/ttyACM0] # la pantalla de la placa en vivo, en la PC (miner::mirror)
cargo run -p simulator --release --example stratum -- <dirección> 300   # el cliente Stratum contra el pool real, minando en la PC
cd firmware && cargo run --release --bin miner             # modo minero en la placa (red Mono-XXXX + QR)
cd firmware && cargo run --release --bin bench_sha         # verifica el SHA por hardware y mide kH/s
```
