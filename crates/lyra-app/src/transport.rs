//! The top bar, built from captioned *modules* so everything lines up:
//!
//! `[logo] [deck] [tempo] [pattern] [typing keys] [windows] ……… [master]`
//!
//! Each module is a flat panel mounted in the dark chassis, uncaptioned:
//! the controls say what they are, and every one has a hover hint.

use crate::app::UiState;
use crate::areas::Editor;
use crate::keys::Action;
use crate::rack::tracked;
use crate::theme;
use crate::widgets::{self, module, Icon, Legend};
use egui::{vec2, Align, Align2, FontId, Layout, Sense, Ui};
use lyra_model::{History, Project, STEPS_PER_BAR, TICKS_PER_STEP};
use lyra_plugin::{Curve, ParamInfo};

static SWING: ParamInfo = ParamInfo::new(0, "Swing", 0.0, 1.0, 0.0, "%", Curve::Linear);
static MASTER: ParamInfo = ParamInfo::new(0, "Master", 0.0, 2.0, 0.8, "gain", Curve::Fader);

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
    // The toolbar is a rack: modules mounted flush, a hairline seam between
    // them, and a blank plate filling whatever is left.
    ui.spacing_mut().item_spacing.x = theme::SEAM;
    ui.horizontal_centered(|ui| {
        let big = widgets::toolbar_key();
        let small = (big * 0.9).round();
        let gap = 4.0;

        // The logo module, which is also the project menu.
        module(ui, |ui| {
            let button = egui::Button::image(egui::Image::new((logo, vec2(big, big)))).frame(false);
            egui::menu::menu_custom_button(ui, button, |ui| {
                for (label, keys, action) in [
                    ("New", "", Action::NewProject),
                    ("Open…", "Ctrl+O", Action::OpenProject),
                    ("Save", "Ctrl+S", Action::Save),
                    ("Save as…", "Ctrl+Shift+S", Action::SaveAs),
                    ("Export WAV…", "Ctrl+E", Action::ExportWav),
                    ("Export stems…", "Ctrl+Shift+E", Action::ExportStems),
                ] {
                    if ui.add(egui::Button::new(label).shortcut_text(keys)).clicked() {
                        actions.push(action);
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Open backup").on_hover_text("The copy Lyra keeps of your latest work").clicked() {
                    actions.push(Action::OpenBackup);
                    ui.close_menu();
                }
            })
            .response
            .on_hover_text("Project: new, open, save, export");
        });

        // The deck: what plays, play / stop, where we are, the beat.
        module(ui, |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            // SONG / PAT: shows which mode is on (SONG mint, PAT Lyra's gold).
            // Clicking either face toggles: a click here means "switch", and
            // having to aim for the right face only gets in the way. L too.
            let options = [(Legend::Text("SONG"), theme::L_MINT), (Legend::Text("PAT"), theme::L_AMBER)];
            let hint = "Click to switch: SONG plays the playlist, PAT the current pattern  ( L )";
            let chosen = if st.song_mode { 0 } else { 1 };
            if widgets::radio_keys(ui, vec2(big * 1.3, big), &options, chosen, hint).is_some() {
                actions.push(Action::SongMode(!st.song_mode));
            }
            ui.add_space(gap);
            if widgets::key(
                ui,
                vec2(big * 1.2, big),
                Legend::Icon(Icon::Play),
                playing,
                theme::L_MINT,
                "Play / stop  (Space)",
            )
            .clicked()
            {
                actions.push(Action::TogglePlay);
            }
            let hint = "Stop: back to the start marker";
            if widgets::key(ui, vec2(big * 1.2, big), Legend::Icon(Icon::Stop), false, theme::L_MINT, hint).clicked() {
                actions.push(Action::Stop);
            }
            ui.add_space(gap);
            let step = (playhead / TICKS_PER_STEP as f64) as u32;
            let pos = format!("{}.{}.{}", step / STEPS_PER_BAR + 1, (step % STEPS_PER_BAR) / 4 + 1, step % 4 + 1);
            widgets::lcd_text(ui, &pos, "BAR . BEAT . STEP", vec2(big * 2.6, big))
                .on_hover_text("Song position: bar . beat . step");
            // One beat light: it flashes on every beat, gold on the downbeat (Lyra's eyes).
            let d = (big * 0.36).round();
            let (r, _) = ui.allocate_exact_size(vec2(d + 4.0, big), Sense::hover());
            let beat = (step % STEPS_PER_BAR) / 4;
            let flash = playing && step.is_multiple_of(4);
            let light = if beat == 0 { theme::L_AMBER } else { theme::L_MINT };
            theme::led(ui.painter(), r.center(), d, flash.then_some(light), false);
        });

        // Tempo and swing.
        module(ui, |ui| {
            ui.spacing_mut().item_spacing.x = gap + 2.0;
            let mut bpm = p.bpm;
            let r = widgets::lcd_number(
                ui,
                &mut bpm,
                40.0..=300.0,
                1,
                "BPM",
                vec2(big * 2.3, big),
                "Tempo: hover a digit, then drag or scroll · double-click to type",
            );
            if tracked(&r, h, p) {
                p.bpm = bpm;
            }
            let mut swing = p.swing;
            if tracked(&widgets::knob(ui, &mut swing, &SWING, big, theme::LAVENDER), h, p) {
                p.swing = swing;
            }
        });

        module(ui, |ui| pattern_picker(ui, p, h, st, small, big));

        // Where the typing keyboard plays.
        module(ui, |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            if widgets::key(
                ui,
                vec2(small, small),
                Legend::Icon(Icon::Minus),
                false,
                theme::L_MINT,
                "Octave down  ( - )",
            )
            .clicked()
            {
                actions.push(Action::Octave(-1));
            }
            let first = (48 + 12 * octave).clamp(0, 127) as u8;
            let last = first.saturating_add(28).min(127); // Z … P spans C to E, 2⅓ octaves
            if let Some(d) = widgets::keyboard_map(ui, first, last) {
                actions.push(Action::Octave(d));
            }
            if widgets::key(ui, vec2(small, small), Legend::Icon(Icon::Plus), false, theme::L_MINT, "Octave up  ( = )")
                .clicked()
            {
                actions.push(Action::Octave(1));
            }
        });

        // Windows: a key lights while its window is open.
        module(ui, |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            let vis = |e: Editor| st.visible.contains(&e);
            for (icon, open, hint, action) in [
                (Icon::Browser, st.show_browser, "Browser  (F4)", Action::ToggleBrowser),
                (Icon::Playlist, vis(Editor::Arrangement), "Arrangement  (F5)", Action::TogglePlaylist),
                (Icon::Rack, st.show_plugins, "Plugin rack  (F6)", Action::ToggleRack),
                (Icon::Piano, vis(Editor::PianoRoll), "Piano roll  (F7)", Action::TogglePiano),
                (Icon::Knob, st.show_editor, "Instrument  (F8)", Action::ToggleEditor),
                (Icon::Mixer, vis(Editor::Mixer), "Mixer  (F9)", Action::ToggleMixer),
            ] {
                if widgets::key(ui, vec2(big, big), Legend::Icon(icon), open, theme::L_MINT, hint).clicked() {
                    actions.push(action);
                }
            }
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            module(ui, |ui| {
                // right_to_left: the meter is added first so it lands on the right.
                let (r, _) = ui.allocate_exact_size(vec2(big * 3.6, big * 0.42), Sense::hover());
                widgets::meter_stereo_h(ui.painter(), r, st.master_meter);
                let mut master = p.master_volume;
                if tracked(&widgets::knob(ui, &mut master, &MASTER, big, theme::MINT), h, p) {
                    p.master_volume = master;
                }
            });
            // A blank plate over the empty bays.
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), theme::m().toolbar_h), Sense::hover());
            ui.painter().rect_filled(r, theme::cr(0), theme::PANEL);
        });
    });
}

/// `◀ [Pattern] ▶  +  ⧉` — which pattern the rack and piano roll edit (and
/// what plays). Scroll over the name to flip through patterns; click it for the list.
fn pattern_picker(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState, small: f32, big: f32) {
    ui.spacing_mut().item_spacing.x = 4.0;
    let n = p.patterns.len();
    st.pattern = st.pattern.min(n - 1);
    let key =
        |ui: &mut Ui, icon, hint| widgets::key(ui, vec2(small, small), Legend::Icon(icon), false, theme::L_MINT, hint);
    if key(ui, Icon::Prev, "Previous pattern  ( [ )").clicked() {
        st.pattern = st.pattern.saturating_sub(1);
    }
    let (r, resp) = ui.allocate_exact_size(vec2(big * 3.6, small), Sense::click());
    theme::well(ui.painter(), r);
    let m = theme::m();
    ui.painter().text(
        r.left_center() + vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        &p.patterns[st.pattern].name,
        FontId::proportional(m.text),
        theme::TEXT,
    );
    // A small chevron, like the dropdown in the studies.
    let c = r.right_center() - vec2(13.0, 1.5);
    let chevron = vec![c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)];
    ui.painter().add(egui::Shape::line(chevron, theme::stroke(1.6, theme::TEXT_DIM)));
    let wheel = if resp.hovered() { ui.input(|i| i.raw_scroll_delta.y) } else { 0.0 };
    if wheel > 0.0 {
        st.pattern = st.pattern.saturating_sub(1);
    } else if wheel < 0.0 {
        st.pattern = (st.pattern + 1).min(n - 1);
    }
    let popup = ui.make_persistent_id("pattern-list");
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup));
    }
    let resp = resp.on_hover_text("Pattern: scroll to flip through, click for the list");
    egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(r.width());
        for i in 0..n {
            ui.selectable_value(&mut st.pattern, i, &p.patterns[i].name);
        }
    });
    if key(ui, Icon::Next, "Next pattern  ( ] )").clicked() {
        st.pattern = (st.pattern + 1).min(n - 1);
    }
    ui.add_space(4.0);
    if key(ui, Icon::Plus, "New empty pattern").clicked() {
        h.checkpoint(p);
        st.pattern = p.add_pattern();
    }
    if key(ui, Icon::Clone, "Duplicate this pattern").clicked() {
        h.checkpoint(p);
        st.pattern = p.clone_pattern(st.pattern);
    }
}
