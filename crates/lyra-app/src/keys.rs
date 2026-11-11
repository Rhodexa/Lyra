//! Global keyboard: transport, undo, and the typing keyboard as a piano.
//!
//! Keys are read by *physical position*, so the piano layout works on
//! QWERTY, AZERTY, Dvorak… alike:
//!
//! ```text
//!   2 3   5 6 7   9 0          (black keys)
//!  Q W E R T Y U I O P         C4 … E5
//!   S D   G H J                (black keys)
//!  Z X C V B N M ,             C3 … C4
//! ```

use egui::{Context, Key};
use std::collections::HashMap;

pub enum Action {
    TogglePlay,
    Stop,
    Undo,
    Redo,
    NoteOn(u8),
    NoteOff(u8),
    SelectPrev,
    SelectNext,
}

#[derive(Default)]
pub struct Keyboard {
    /// Which note each held key started, so octave changes never strand a note.
    held: HashMap<Key, u8>,
    pub octave: i32,
}

/// Semitones above the lowest key (Z = C3 at octave 0).
fn semitone(k: Key) -> Option<i32> {
    use Key::*;
    Some(match k {
        Z => 0, S => 1, X => 2, D => 3, C => 4, V => 5, G => 6, B => 7, H => 8, N => 9, J => 10, M => 11, Comma => 12,
        Q => 12, Num2 => 13, W => 14, Num3 => 15, E => 16, R => 17, Num5 => 18, T => 19, Num6 => 20,
        Y => 21, Num7 => 22, U => 23, I => 24, Num9 => 25, O => 26, Num0 => 27, P => 28,
        _ => return None,
    })
}

impl Keyboard {
    pub fn poll(&mut self, ctx: &Context) -> Vec<Action> {
        let mut out = Vec::new();
        if ctx.wants_keyboard_input() {
            return out; // someone is typing into a text field
        }
        let events = ctx.input(|i| i.events.clone());
        for e in events {
            let egui::Event::Key { key, physical_key, pressed, repeat, modifiers, .. } = e else { continue };
            let k = physical_key.unwrap_or(key);

            if modifiers.command {
                if pressed {
                    match k {
                        Key::Z if modifiers.shift => out.push(Action::Redo),
                        Key::Z => out.push(Action::Undo),
                        Key::Y => out.push(Action::Redo),
                        _ => {}
                    }
                }
                continue;
            }

            if let Some(semi) = semitone(k) {
                if pressed && !repeat {
                    let note = (48 + semi + 12 * self.octave).clamp(0, 127) as u8;
                    self.held.insert(k, note);
                    out.push(Action::NoteOn(note));
                } else if !pressed {
                    if let Some(note) = self.held.remove(&k) {
                        out.push(Action::NoteOff(note));
                    }
                }
                continue;
            }

            if !pressed || repeat {
                continue;
            }
            match k {
                Key::Space => out.push(Action::TogglePlay),
                Key::Escape => out.push(Action::Stop),
                Key::ArrowUp => out.push(Action::SelectPrev),
                Key::ArrowDown => out.push(Action::SelectNext),
                Key::Minus => self.octave = (self.octave - 1).max(-3),
                Key::Equals => self.octave = (self.octave + 1).min(3),
                _ => {}
            }
        }
        out
    }
}
