//! Global keyboard: transport, undo, editing commands, and the typing keyboard as a piano.
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
//!
//! Editing keys (arrows, Delete, Ctrl+A/C/X/V/D/B/Q) are *routed* by the app
//! to the window you last clicked in — e.g. arrows move notes in the piano
//! roll, but pick channels in the rack.
//!
//! Arrows: ↑↓ or Shift+↑↓ = one semitone, Ctrl+↑↓ = one octave;
//! ←→ = one snap step, Ctrl+←→ = one beat.

use egui::{Context, Key};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    TogglePlay,
    Stop,
    Undo,
    Redo,
    NoteOn(u8),
    NoteOff(u8),
    /// Arrow keys: (dx, dy) in {-1, 0, 1}; `big` = Ctrl held (octave / beat).
    Arrow {
        dx: i32,
        dy: i32,
        big: bool,
        /// Shift / Alt held (piano roll typing: select / length).
        shift: bool,
        alt: bool,
    },
    Delete,
    SelectAll,
    Deselect,
    Copy,
    Cut,
    Paste,
    /// Duplicate the selection right after itself (Ctrl+D or Ctrl+B).
    Duplicate,
    /// Ctrl+U: the selected audio clips get their own copy of their audio
    /// clip, so changing them no longer changes the others.
    MakeUnique,
    /// Make an automation for the control changed last, on the arrangement.
    Automate,
    /// Add the control changed last to the current automation's targets.
    LinkTweaked,
    Quantize,
    /// Enter: start / stop typing notes in the piano roll.
    ToggleTyping,
    /// Backspace: like Delete, except in step input (back a step and erase).
    Backspace,
    /// Home / End: to the start / the end (step input's cursor).
    Home,
    End,
    /// Remove notes that double another (same moment, same note or an octave
    /// apart), keeping the lowest or the highest of each (piano roll).
    Undouble {
        keep_high: bool,
    },
    /// Move the playhead / start marker to a tick (from clicking a timeline).
    Seek(f64),
    /// Seek to a tick *inside the current pattern* (the piano roll's ruler):
    /// in song mode, to where that pattern plays in the song.
    SeekPattern(f64),
    /// Make this channel (by index) the selected one.
    SelectChannel(usize),
    /// Shift the typing keyboard by this many octaves.
    Octave(i32),
    PrevPattern,
    NextPattern,
    ToggleRack,
    TogglePiano,
    ToggleEditor,
    ToggleDesign,
    /// Open or fold the bottom area (keeping whatever editor it shows).
    ToggleBottom,
    ToggleMixer,
    ToggleBrowser,
    /// Tab: the area under the mouse flips back to its previous editor.
    SwapEditor,
    /// Pattern mode ↔ song mode.
    ToggleSongMode,
    SongMode(bool),
    NewProject,
    OpenProject,
    OpenBackup,
    Save,
    SaveAs,
    /// Copy the files the song uses into its folder's `audio/`.
    Collect,
    /// Render the song to a WAV file (Ctrl+E), or one WAV per channel (Ctrl+Shift+E).
    ExportWav,
    ExportStems,
    /// MIDI, notes, chords and mix measurements in a "<song> data" folder.
    ExportData,
    /// The context menu (F3): what the context bar offers, at the mouse.
    ContextMenu,
    /// The selected notes as a MIDI file (piano roll).
    ExportNotesMidi,
    /// A span of bars, every instrument: the loop region, or the bars the
    /// selected notes are in (piano roll, arrangement).
    ExportBarsMidi,
    /// The whole pattern, every instrument.
    ExportPatternMidi,
    /// Take the loop region away.
    ClearLoop,
    /// Read a MIDI file into the song.
    ImportMidi,
    /// Hear the open audio clip (its window, or the trimmed file) on its own.
    ListenClip,
    /// Hear the recorder's tape.
    ListenTape,
    /// Stop what's being listened to.
    StopListening,
}

#[derive(Default)]
pub struct Keyboard {
    /// Which note each held key started, so octave changes never strand a note.
    held: HashMap<Key, u8>,
    pub octave: i32,
    /// Tab presses taken out of the raw input (see `take_tabs`).
    tabs: usize,
}

/// An arrow key as an action (`big`: Ctrl held).
fn arrow(k: Key, big: bool, m: egui::Modifiers) -> Action {
    let (dx, dy) = match k {
        Key::ArrowUp => (0, 1),
        Key::ArrowDown => (0, -1),
        Key::ArrowLeft => (-1, 0),
        _ => (1, 0),
    };
    Action::Arrow { dx, dy, big, shift: m.shift, alt: m.alt }
}

/// Semitones above the lowest key (Z = C3 at octave 0).
fn semitone(k: Key) -> Option<i32> {
    use Key::*;
    Some(match k {
        Z => 0,
        S => 1,
        X => 2,
        D => 3,
        C => 4,
        V => 5,
        G => 6,
        B => 7,
        H => 8,
        N => 9,
        J => 10,
        M => 11,
        Comma => 12,
        Q => 12,
        Num2 => 13,
        W => 14,
        Num3 => 15,
        E => 16,
        R => 17,
        Num5 => 18,
        T => 19,
        Num6 => 20,
        Y => 21,
        Num7 => 22,
        U => 23,
        I => 24,
        Num9 => 25,
        O => 26,
        Num0 => 27,
        P => 28,
        _ => return None,
    })
}

impl Keyboard {
    /// Tab flips the hovered area's editor. egui would otherwise use it to move
    /// keyboard focus between widgets, and it does that *before* the app sees
    /// the key: focus then landed on some text field and every key after it
    /// went there. So Tab is taken out of the raw input, before egui runs.
    /// (Not while someone is typing in a text field.)
    pub fn take_tabs(&mut self, ctx: &Context, raw: &mut egui::RawInput) {
        if ctx.wants_keyboard_input() {
            return;
        }
        raw.events.retain(|e| match e {
            egui::Event::Key { key: Key::Tab, pressed, repeat, modifiers, .. } if modifiers.is_none() => {
                if *pressed && !*repeat {
                    self.tabs += 1;
                }
                false
            }
            _ => true,
        });
    }

    pub fn poll(&mut self, ctx: &Context) -> Vec<Action> {
        let mut out = Vec::new();
        let tabs = std::mem::take(&mut self.tabs);
        if ctx.wants_keyboard_input() {
            return out; // someone is typing into a text field
        }
        out.extend(std::iter::repeat_n(Action::SwapEditor, tabs));
        let events = ctx.input(|i| i.events.clone());
        // Space means play/stop, always: take it away from egui so it can never
        // also "press" whatever button or menu happens to have focus.
        ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Space));
        for e in events {
            // egui-winit turns Ctrl+C / X / V into these clipboard events instead
            // of key presses, so they must be caught here (this is why copy/paste
            // first "did nothing").
            match e {
                egui::Event::Copy => out.push(Action::Copy),
                egui::Event::Cut => out.push(Action::Cut),
                egui::Event::Paste(_) => out.push(Action::Paste),
                _ => {}
            }
            let egui::Event::Key { key, physical_key, pressed, repeat, modifiers, .. } = e else { continue };
            let k = physical_key.unwrap_or(key);

            if modifiers.command {
                if pressed {
                    match k {
                        Key::Z if modifiers.shift => out.push(Action::Redo),
                        Key::Z => out.push(Action::Undo),
                        Key::Y => out.push(Action::Redo),
                        Key::A => out.push(Action::SelectAll),
                        // Ctrl+C/X/V arrive as Event::Copy/Cut/Paste (above).
                        Key::D | Key::B => out.push(Action::Duplicate),
                        Key::U => out.push(Action::MakeUnique),
                        Key::S if modifiers.shift => out.push(Action::SaveAs),
                        Key::S => out.push(Action::Save),
                        Key::E if modifiers.shift => out.push(Action::ExportStems),
                        Key::E => out.push(Action::ExportWav),
                        Key::O => out.push(Action::OpenProject),
                        Key::Q => out.push(Action::Quantize),
                        Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight => {
                            out.push(arrow(k, true, modifiers));
                        }
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

            if !pressed {
                continue;
            }
            let big = false; // plain or Shift: the small step
            match k {
                // Arrows auto-repeat on purpose: hold to keep nudging.
                Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight => {
                    out.push(arrow(k, big, modifiers));
                }
                _ if repeat => {}
                Key::Space => out.push(Action::TogglePlay),
                Key::Escape => out.push(Action::Deselect),
                Key::Delete => out.push(Action::Delete),
                Key::Backspace => out.push(Action::Backspace),
                Key::Home => out.push(Action::Home),
                Key::Enter => out.push(Action::ToggleTyping),
                Key::End => out.push(Action::End),
                Key::Minus => self.octave = (self.octave - 1).max(-3),
                Key::Equals => self.octave = (self.octave + 1).min(3),
                Key::OpenBracket => out.push(Action::PrevPattern),
                Key::CloseBracket => out.push(Action::NextPattern),
                Key::F6 => out.push(Action::ToggleRack),
                Key::F7 => out.push(Action::TogglePiano),
                Key::F8 => out.push(Action::ToggleEditor),
                Key::F12 => out.push(Action::ToggleDesign),
                Key::F3 => out.push(Action::ContextMenu),
                Key::F4 => out.push(Action::ToggleBrowser),
                Key::F5 => out.push(Action::ToggleBottom),
                Key::F9 => out.push(Action::ToggleMixer),
                Key::L => out.push(Action::ToggleSongMode),
                _ => {}
            }
        }
        out
    }
}
