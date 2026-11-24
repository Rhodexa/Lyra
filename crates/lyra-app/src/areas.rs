//! Areas, Blender-style: the centre of the window is split into a top and a
//! bottom area, and each area shows one *editor*, picked from the little
//! menu in its top-left corner.
//!
//! Every area keeps its own memory for every editor (scroll, zoom,
//! selection), so switching an area away and back finds it as you left it,
//! and two areas can show the same editor side by side.
//!
//! Keyboard editing keys (Delete, Ctrl+A, arrows…) go to the area under the
//! mouse. The F-keys open the bottom area on an editor (F5 arrangement, F7
//! piano roll, F9 mixer); pressing the same key again folds it away. Tab
//! flips the area under the mouse back to its previous editor. Double-clicking
//! an audio clip opens the audio-clip editor (in the bottom area if neither
//! area shows it yet).

use crate::audio_editor::AudioEditor;
use crate::automation_editor::AutomationEditor;
use crate::piano_roll::PianoRoll;
use crate::playlist::Playlist;
use crate::theme;
use crate::widgets::{self, Icon};
use egui::Ui;

/// What an area can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editor {
    Arrangement,
    PianoRoll,
    Mixer,
    /// The old channel rack's step grid. Not part of the plugin rack any more;
    /// kept as an editor for people who like programming beats in steps.
    Steps,
    /// One audio clip, big: trim, gain, fades.
    Audio,
    /// One automation's curve.
    Automation,
}

impl Editor {
    pub const ALL: [Editor; 6] =
        [Editor::Arrangement, Editor::PianoRoll, Editor::Audio, Editor::Automation, Editor::Mixer, Editor::Steps];

    pub fn name(self) -> &'static str {
        match self {
            Editor::Arrangement => "Arrangement",
            Editor::PianoRoll => "Piano roll",
            Editor::Mixer => "Mixer",
            Editor::Steps => "Step sequencer",
            Editor::Audio => "Audio clip",
            Editor::Automation => "Automation",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Editor::Arrangement => Icon::Playlist,
            Editor::PianoRoll => Icon::Piano,
            Editor::Mixer => Icon::Mixer,
            Editor::Steps => Icon::Rack,
            Editor::Audio => Icon::Wave,
            Editor::Automation => Icon::Curve,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Editor::Arrangement => "F5",
            Editor::PianoRoll => "F7",
            Editor::Mixer => "F9",
            Editor::Steps | Editor::Audio | Editor::Automation => "",
        }
    }
}

/// One area: the editor it shows, plus every editor's own state.
pub struct Area {
    pub editor: Editor,
    /// The editor shown before this one (Tab flips back to it).
    pub prev: Option<Editor>,
    pub playlist: Playlist,
    pub piano: PianoRoll,
    pub audio: AudioEditor,
    pub automation: AutomationEditor,
    /// (pattern, channel id) this area's piano roll last framed its view on.
    pub framed: Option<(usize, u32)>,
}

impl Area {
    pub fn new(editor: Editor) -> Self {
        Self {
            editor,
            prev: None,
            playlist: Playlist::default(),
            piano: PianoRoll::default(),
            audio: AudioEditor::default(),
            automation: AutomationEditor::default(),
            framed: None,
        }
    }

    /// Show `editor`, remembering the one it replaces (for Tab).
    pub fn set(&mut self, editor: Editor) {
        if editor != self.editor {
            self.prev = Some(self.editor);
            self.editor = editor;
        }
    }

    /// Tab: back to the previous editor (and Tab again comes back here).
    pub fn swap(&mut self) {
        if let Some(prev) = self.prev {
            self.set(prev);
        }
    }
}

/// The editor picker: a small well showing the editor's icon and name; click
/// it for the list.
pub fn picker(ui: &mut Ui, area: &mut Area) {
    let mut editor = area.editor;
    picker_list(ui, &mut editor);
    area.set(editor);
}

fn picker_list(ui: &mut Ui, editor: &mut Editor) {
    let h = theme::m().control_h;
    let rows: Vec<widgets::DropRow> = Editor::ALL
        .iter()
        .map(|e| widgets::DropRow { label: e.name().into(), icon: Some(e.icon()), detail: e.key().into() })
        .collect();
    let chosen = Editor::ALL.iter().position(|e| e == editor).unwrap_or(0);
    if let Some(i) = widgets::dropdown(ui, "editor-picker", h * 5.6, &rows, chosen, false, "Which editor this area shows") {
        *editor = Editor::ALL[i];
    }
}
