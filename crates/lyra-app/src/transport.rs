//! The top bar, built from captioned *modules* so everything lines up:
//!
//! `[logo] [TRANSPORT] [TEMPO] [PATTERN] [TYPING KEYS] [WINDOWS] ……… [MASTER]`

use crate::app::UiState;
use crate::keys::Action;
use crate::rack::tracked;
use crate::theme::{self, cr};
use crate::widgets::{self, module, Icon};
use egui::{vec2, Align, Align2, FontId, Layout, Rect, RichText, Sense, Ui};
use lyra_model::{History, Project, STEPS_PER_BAR, TICKS_PER_STEP};
use lyra_plugin::{Curve, ParamInfo};

static SWING: ParamInfo = ParamInfo::new(0, "Swing", 0.0, 1.0, 0.0, "%", Curve::Linear);
static MASTER: ParamInfo = ParamInfo::new(0, "Master", 0.0, 1.25, 0.8, "%", Curve::Linear);

#[allow(clippy::too_many_arguments)] // it's a view: it touches many things by design
pub fn show(
    ui: &mut Ui,
    p: &mut Project,
    h: &mut History,
    st: &mut UiState,
    playing: bool,
    playhead: f64,
    logo: egui::TextureId,
    octave: i32,
    actions: &mut Vec<Action>,
) {
    let m = theme::m();
    ui.spacing_mut().item_spacing.x = m.module_gap;
    ui.horizontal_centered(|ui| {
        widgets::logo(ui, logo, m.control_h + 8.0);

        module(ui, "transport", |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            // What Space plays: the current pattern, or the whole song (L toggles).
            for (label, song) in [("PAT", false), ("SONG", true)] {
                let on = st.song_mode == song;
                let text =
                    RichText::new(label).size(m.text_small).color(if on { theme::INSET } else { theme::TEXT_DIM });
                let fill = if on {
                    if song {
                        theme::GOLD
                    } else {
                        theme::MINT
                    }
                } else {
                    theme::RAISED
                };
                let b = egui::Button::new(text).fill(fill).min_size(vec2(m.control_h * 1.6, m.control_h));
                if ui.add(b).on_hover_text("Play the current pattern / the whole playlist  ( L )").clicked() {
                    actions.push(Action::SongMode(song));
                }
            }
            ui.add_space(4.0);
            if widgets::transport_button(ui, Icon::Play, playing, "Play / stop  (Space)").clicked() {
                actions.push(Action::TogglePlay);
            }
            if widgets::transport_button(ui, Icon::Stop, false, "Stop: back to the start marker").clicked() {
                actions.push(Action::Stop);
            }
            ui.add_space(4.0);
            let step = (playhead / TICKS_PER_STEP as f64) as u32;
            let (r, _) = ui.allocate_exact_size(vec2(m.text * 5.5, m.control_h), Sense::hover());
            ui.painter().rect_filled(r, cr(3), theme::PANEL);
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                format!("{}.{}.{}", step / STEPS_PER_BAR + 1, (step % STEPS_PER_BAR) / 4 + 1, step % 4 + 1),
                FontId::monospace(m.text + 1.0),
                if playing { theme::MINT } else { theme::TEXT_DIM },
            );
            let (r, _) = ui.allocate_exact_size(vec2(m.control_h * 1.6, m.control_h), Sense::hover());
            for b in 0..4u32 {
                let c = r.left_center() + vec2(r.width() * (b as f32 + 0.5) / 4.0, 0.0);
                let on = playing && (step % STEPS_PER_BAR) / 4 == b;
                let color = if !on {
                    theme::LINE
                } else if b == 0 {
                    theme::GOLD
                } else {
                    theme::MINT
                };
                ui.painter().circle_filled(c, m.control_h * 0.12, color);
            }
        });

        module(ui, "tempo", |ui| {
            let mut bpm = p.bpm;
            let r = ui
                .add_sized(
                    [m.text * 6.0, m.control_h],
                    egui::DragValue::new(&mut bpm).range(40.0..=300.0).speed(0.25).fixed_decimals(1).suffix(" bpm"),
                )
                .on_hover_text("Tempo: drag, or double-click to type");
            if tracked(&r, h, p) {
                p.bpm = bpm;
            }
            ui.label(RichText::new("swing").small().color(theme::TEXT_DIM));
            let mut swing = p.swing;
            if tracked(&widgets::knob(ui, &mut swing, &SWING, m.control_h, theme::LAVENDER), h, p) {
                p.swing = swing;
            }
        });

        module(ui, "pattern", |ui| pattern_picker(ui, p, h, st, m.control_h));

        module(ui, "typing keys", |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            if widgets::icon_button(ui, Icon::Minus, false, "Octave down  ( - )").clicked() {
                actions.push(Action::Octave(-1));
            }
            let first = (48 + 12 * octave).clamp(0, 127) as u8;
            let last = first.saturating_add(28).min(127); // Z … P spans C to E, 2⅓ octaves
            if let Some(d) = widgets::keyboard_map(ui, first, last) {
                actions.push(Action::Octave(d));
            }
            if widgets::icon_button(ui, Icon::Plus, false, "Octave up  ( = )").clicked() {
                actions.push(Action::Octave(1));
            }
        });

        module(ui, "windows", |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            if widgets::icon_button(ui, Icon::Playlist, st.show_playlist, "Playlist  (F5)").clicked() {
                actions.push(Action::TogglePlaylist);
            }
            if widgets::icon_button(ui, Icon::Rack, st.show_rack, "Channel rack  (F6)").clicked() {
                actions.push(Action::ToggleRack);
            }
            if widgets::icon_button(ui, Icon::Piano, st.show_piano, "Piano roll  (F7)").clicked() {
                actions.push(Action::TogglePiano);
            }
            if widgets::icon_button(ui, Icon::Knob, st.show_editor, "Instrument  (F8)").clicked() {
                actions.push(Action::ToggleEditor);
            }
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            module(ui, "master", |ui| {
                let mut master = p.master_volume;
                if tracked(&widgets::knob(ui, &mut master, &MASTER, m.control_h, theme::MINT), h, p) {
                    p.master_volume = master;
                }
                let (r, _) = ui.allocate_exact_size(vec2(m.control_h * 5.0, m.control_h * 0.55), Sense::hover());
                let top = Rect::from_min_max(r.min, egui::pos2(r.max.x, r.center().y - 1.0));
                let bottom = Rect::from_min_max(egui::pos2(r.min.x, r.center().y + 1.0), r.max);
                widgets::meter(ui.painter(), top, st.master_meter[0]);
                widgets::meter(ui.painter(), bottom, st.master_meter[1]);
            });
        });
    });
}

/// `◀ [Pattern ▾] ▶  +  ⧉` — which pattern the rack and piano roll edit (and what plays).
fn pattern_picker(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState, height: f32) {
    ui.spacing_mut().item_spacing.x = 3.0;
    let n = p.patterns.len();
    st.pattern = st.pattern.min(n - 1);
    if widgets::icon_button(ui, Icon::Prev, false, "Previous pattern  ( [ )").clicked() {
        st.pattern = st.pattern.saturating_sub(1);
    }
    egui::ComboBox::from_id_salt("pattern-picker")
        .width(height * 5.0)
        .height(400.0)
        .selected_text(RichText::new(&p.patterns[st.pattern].name).color(theme::MINT))
        .show_ui(ui, |ui| {
            for i in 0..n {
                ui.selectable_value(&mut st.pattern, i, &p.patterns[i].name);
            }
        });
    if widgets::icon_button(ui, Icon::Next, false, "Next pattern  ( ] )").clicked() {
        st.pattern = (st.pattern + 1).min(n - 1);
    }
    ui.add_space(4.0);
    if widgets::icon_button(ui, Icon::Plus, false, "New empty pattern").clicked() {
        h.checkpoint(p);
        st.pattern = p.add_pattern();
    }
    if widgets::icon_button(ui, Icon::Clone, false, "Duplicate this pattern").clicked() {
        h.checkpoint(p);
        st.pattern = p.clone_pattern(st.pattern);
    }
}
