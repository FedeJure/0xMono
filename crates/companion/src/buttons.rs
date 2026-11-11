//! Botones: toque corto, mantener apretado y A+C juntos.
//!
//! Cada botón recibe su lectura cruda una vez por fotograma (`true` =
//! apretado) y devuelve un [`Gesture`] cuando se completa uno. El mismo
//! código sirve para el GPIO del ESP32 y para el teclado del simulador.
//!
//! El toque se avisa al *soltar*: mientras el botón sigue abajo todavía puede
//! convertirse en "mantener". El mantener, en cambio, se avisa apenas se
//! cumple [`HOLD_MS`], sin esperar a que lo suelte, para que se note la
//! respuesta mientras el dedo sigue ahí.
//!
//! [`Pad`] junta los tres botones y agrega la combinación A+C mantenida
//! [`CHORD_MS`] (abrir el menú desde cualquier lado). Los botones que forman la
//! combinación no avisan sus propios toques ni mantener.

/// Cuánto hay que mantenerlo apretado para que deje de ser un toque.
pub const HOLD_MS: u32 = 600;

/// Una lectura distinta tiene que sostenerse este tiempo para contar: los
/// rebotes del contacto duran pocos milisegundos.
const DEBOUNCE_MS: u32 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    /// Apretar y soltar antes de [`HOLD_MS`].
    Tap,
    /// Mantenerlo apretado [`HOLD_MS`].
    Hold,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ButtonState {
    /// Estado ya sin rebotes.
    down: bool,
    /// Cuánto lleva la lectura cruda distinta de `down`.
    changing_ms: u32,
    held_ms: u32,
    hold_sent: bool,
}

impl ButtonState {
    pub const fn new() -> Self {
        Self {
            down: false,
            changing_ms: 0,
            held_ms: 0,
            hold_sent: false,
        }
    }

    /// Lectura de este fotograma y el tiempo desde la anterior.
    pub fn update(&mut self, pressed: bool, dt_ms: u32) -> Option<Gesture> {
        if pressed == self.down {
            self.changing_ms = 0;
        } else if self.changing_ms == 0 {
            // Primera lectura distinta: puede ser un rebote, esperar a la próxima.
            self.changing_ms = 1;
        } else {
            self.changing_ms += dt_ms;
            if self.changing_ms > DEBOUNCE_MS {
                self.down = pressed;
                if pressed {
                    // Ya lleva apretado lo que tardó en confirmarse.
                    self.held_ms = self.changing_ms;
                    self.hold_sent = false;
                    self.changing_ms = 0;
                    return None;
                }
                self.changing_ms = 0;
                return (!self.hold_sent).then_some(Gesture::Tap);
            }
        }

        if self.down && !self.hold_sent {
            self.held_ms += dt_ms;
            if self.held_ms >= HOLD_MS {
                self.hold_sent = true;
                return Some(Gesture::Hold);
            }
        }
        None
    }

    pub fn is_down(&self) -> bool {
        self.down
    }

    /// Cuánto lleva apretado, hasta [`HOLD_MS`]; 0 si está suelto o si el
    /// mantener ya se avisó. Sirve para dibujar cómo se va "cargando".
    pub fn hold_progress_ms(&self) -> u32 {
        if self.down && !self.hold_sent {
            self.held_ms
        } else {
            0
        }
    }
}

/// Cuánto hay que mantener A+C juntos: lo bastante para que no salga sin
/// querer jugando o hablándole, y no tanto como para impacientar.
pub const CHORD_MS: u32 = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    A,
    B,
    C,
}

impl Button {
    pub const ALL: [Button; 3] = [Button::A, Button::B, Button::C];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Tap(Button),
    Hold(Button),
    /// A y C mantenidos juntos [`CHORD_MS`].
    Chord,
}

/// Los tres botones juntos.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pad {
    buttons: [ButtonState; 3],
    /// Botones que formaron parte de la combinación: callados hasta soltarse.
    in_chord: [bool; 3],
    chord_ms: u32,
    chord_sent: bool,
}

impl Pad {
    pub const fn new() -> Self {
        Self {
            buttons: [ButtonState::new(); 3],
            in_chord: [false; 3],
            chord_ms: 0,
            chord_sent: false,
        }
    }

    /// Lecturas crudas de A, B y C en este fotograma.
    pub fn update(&mut self, pressed: [bool; 3], dt_ms: u32) -> Option<Input> {
        let mut gestures = [None; 3];
        for (i, b) in self.buttons.iter_mut().enumerate() {
            gestures[i] = b.update(pressed[i], dt_ms);
        }

        let mut out = None;
        if self.buttons[0].is_down() && self.buttons[2].is_down() {
            self.in_chord[0] = true;
            self.in_chord[2] = true;
            self.chord_ms += dt_ms;
            if self.chord_ms >= CHORD_MS && !self.chord_sent {
                self.chord_sent = true;
                out = Some(Input::Chord);
            }
        } else {
            self.chord_ms = 0;
            self.chord_sent = false;
        }

        for (i, g) in gestures.into_iter().enumerate() {
            if self.in_chord[i] {
                if !self.buttons[i].is_down() {
                    self.in_chord[i] = false;
                }
                continue;
            }
            if out.is_none() {
                out = g.map(|g| match g {
                    Gesture::Tap => Input::Tap(Button::ALL[i]),
                    Gesture::Hold => Input::Hold(Button::ALL[i]),
                });
            }
        }
        out
    }

    /// Cuánto lleva la combinación, hasta [`CHORD_MS`] (0 si no está).
    pub fn chord_progress_ms(&self) -> u32 {
        if self.chord_sent {
            0
        } else {
            self.chord_ms.min(CHORD_MS)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: u32 = 33;

    /// Mantiene la lectura `ms` milisegundos y junta lo que salga.
    fn feed(b: &mut ButtonState, pressed: bool, ms: u32) -> Vec<Gesture> {
        let mut out = Vec::new();
        let mut t = 0;
        while t < ms {
            out.extend(b.update(pressed, FRAME));
            t += FRAME;
        }
        out
    }

    #[test]
    fn short_press_is_a_tap_on_release() {
        let mut b = ButtonState::new();
        assert!(feed(&mut b, true, 150).is_empty());
        assert_eq!(feed(&mut b, false, 100), [Gesture::Tap]);
    }

    #[test]
    fn long_press_fires_hold_once_while_down_and_no_tap() {
        let mut b = ButtonState::new();
        assert_eq!(feed(&mut b, true, 3_000), [Gesture::Hold]);
        assert!(feed(&mut b, false, 100).is_empty());
    }

    #[test]
    fn hold_fires_close_to_the_threshold() {
        let mut b = ButtonState::new();
        let mut t = 0;
        while b.update(true, FRAME).is_none() {
            t += FRAME;
            assert!(t < 2 * HOLD_MS);
        }
        assert!(t + FRAME >= HOLD_MS && t <= HOLD_MS + 2 * FRAME, "{t} ms");
    }

    #[test]
    fn single_bounce_reading_is_ignored() {
        let mut b = ButtonState::new();
        // Un fotograma suelto en bajo no es un toque.
        assert_eq!(b.update(true, FRAME), None);
        assert!(feed(&mut b, false, 200).is_empty());

        // Un rebote a mitad de un mantener no lo corta.
        feed(&mut b, true, 300);
        assert_eq!(b.update(false, FRAME), None);
        assert_eq!(feed(&mut b, true, 400), [Gesture::Hold]);
    }

    #[test]
    fn progress_resets_after_hold_and_release() {
        let mut b = ButtonState::new();
        feed(&mut b, true, 300);
        assert!(b.hold_progress_ms() > 200);
        feed(&mut b, true, 400);
        assert_eq!(b.hold_progress_ms(), 0);
        feed(&mut b, false, 100);
        assert_eq!(b.hold_progress_ms(), 0);
    }

    fn pad_feed(p: &mut Pad, pressed: [bool; 3], ms: u32) -> Vec<Input> {
        let mut out = Vec::new();
        let mut t = 0;
        while t < ms {
            out.extend(p.update(pressed, FRAME));
            t += FRAME;
        }
        out
    }

    #[test]
    fn single_buttons_still_work_on_the_pad() {
        let mut p = Pad::new();
        pad_feed(&mut p, [false, true, false], 100);
        assert_eq!(pad_feed(&mut p, [false; 3], 100), [Input::Tap(Button::B)]);
        assert_eq!(
            pad_feed(&mut p, [true, false, false], 1000),
            [Input::Hold(Button::A)]
        );
    }

    #[test]
    fn a_and_c_held_together_is_the_chord_and_nothing_else() {
        let mut p = Pad::new();
        // C llega un poco después que A, como pasa con los dedos.
        pad_feed(&mut p, [true, false, false], 100);
        assert_eq!(
            pad_feed(&mut p, [true, false, true], CHORD_MS + 200),
            [Input::Chord]
        );
        assert_eq!(p.chord_progress_ms(), 0);
        // Seguir apretando no repite, y soltar no da toques ni mantener.
        assert!(pad_feed(&mut p, [true, false, true], 2_000).is_empty());
        assert!(pad_feed(&mut p, [false; 3], 300).is_empty());
        // Después todo vuelve a la normalidad.
        pad_feed(&mut p, [false, false, true], 100);
        assert_eq!(pad_feed(&mut p, [false; 3], 100), [Input::Tap(Button::C)]);
    }

    #[test]
    fn letting_go_early_cancels_the_chord_silently() {
        let mut p = Pad::new();
        pad_feed(&mut p, [true, false, true], CHORD_MS - 500);
        assert!(p.chord_progress_ms() > CHORD_MS - 1_000);
        assert!(pad_feed(&mut p, [false, false, true], 300).is_empty());
        assert!(pad_feed(&mut p, [false; 3], 300).is_empty());
        assert_eq!(p.chord_progress_ms(), 0);
    }
}
