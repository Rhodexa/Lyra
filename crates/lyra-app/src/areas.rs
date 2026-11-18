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
//! flips the area under the mouse back to its previous editor.

use crate::piano_roll::PianoRoll;
use crate::playlist::Playlist;
use crate::theme::{self, cr};
use crate::widgets::{paint_icon, Icon};
use egui::{pos2, vec2, Align2, FontId, Rect, Sense, Ui};

/// What an area can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editor {
    Arrangement,
    PianoRoll,
    Mixer,
    /// The old channel rack's step grid. Not part of the plugin rack any more;
    /// kept as an editor for people who like programming beats in steps.
    Steps,
}

impl Editor {
    pub const ALL: [Editor; 4] = [Editor::Arrangement, Editor::PianoRoll, Editor::Mixer, Editor::Steps];

    pub fn name(self) -> &'static str {
        match self {
            Editor::Arrangement => "Arrangement",
            Editor::PianoRoll => "Piano roll",
            Editor::Mixer => "Mixer",
            Editor::Steps => "Step sequencer",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Editor::Arrangement => Icon::Playlist,
            Editor::PianoRoll => Icon::Piano,
            Editor::Mixer => Icon::Mixer,
            Editor::Steps => Icon::Rack,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Editor::Arrangement => "F5",
            Editor::PianoRoll => "F7",
            Editor::Mixer => "F9",
            Editor::Steps => "",
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
    /// (pattern, channel id) this area's piano roll last framed its view on.
    pub framed: Option<(usize, u32)>,
}

impl Area {
    pub fn new(editor: Editor) -> Self {
        Self { editor, prev: None, playlist: Playlist::default(), piano: PianoRoll::default(), framed: None }
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
    let m = theme::m();
    let h = m.control_h;
    let (r, resp) = ui.allocate_exact_size(vec2(h * 5.6, h), Sense::click());
    let p = ui.painter();
    theme::well(p, r);
    let icon = Rect::from_center_size(pos2(r.left() + h * 0.55, r.center().y), vec2(h * 0.7, h * 0.7));
    let ink = if resp.hovered() { theme::TEXT } else { theme::mix(theme::TEXT, theme::TEXT_DIM, 0.3) };
    paint_icon(p, icon, editor.icon(), ink, theme::WELL);
    p.text(
        pos2(r.left() + h * 1.05, r.center().y),
        Align2::LEFT_CENTER,
        editor.name(),
        FontId::proportional(m.text),
        ink,
    );
    let c = r.right_center() - vec2(11.0, 1.0);
    let chevron = vec![c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)];
    p.add(egui::Shape::line(chevron, theme::stroke(1.5, theme::TEXT_DIM)));

    let popup = ui.make_persistent_id("editor-picker");
    if resp.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup));
    }
    let resp = resp.on_hover_text("Which editor this area shows");
    egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(r.width());
        for e in Editor::ALL {
            let (row, item) = ui.allocate_exact_size(vec2(r.width(), h), Sense::click());
            let chosen = *editor == e;
            if chosen || item.hovered() {
                ui.painter().rect_filled(row, cr(3), if chosen { theme::PANEL_HI } else { theme::RAISED });
            }
            let icon = Rect::from_center_size(pos2(row.left() + h * 0.55, row.center().y), vec2(h * 0.7, h * 0.7));
            paint_icon(ui.painter(), icon, e.icon(), theme::TEXT, theme::PANEL);
            ui.painter().text(
                pos2(row.left() + h * 1.05, row.center().y),
                Align2::LEFT_CENTER,
                e.name(),
                FontId::proportional(m.text),
                theme::TEXT,
            );
            ui.painter().text(
                row.right_center() - vec2(8.0, 0.0),
                Align2::RIGHT_CENTER,
                e.key(),
                FontId::monospace(m.text_small),
                theme::TEXT_DIM,
            );
            if item.clicked() {
                *editor = e;
            }
        }
    });
}
