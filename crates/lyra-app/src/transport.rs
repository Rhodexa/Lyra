//! The top bar, built from captioned *modules* so everything lines up:
//!
//! `[File] [deck] [tempo] [typing keys] [panels] ……… [master]`
//!
//! Each module is a flat panel mounted in the dark chassis, uncaptioned:
//! the controls say what they are, and every one has a hover hint.

use crate::app::UiState;
use crate::widgets::Hint;
use crate::keys::Action;
use crate::rack::tracked;
use crate::theme;
use crate::widgets::{self, module, Icon, Legend};
use egui::{vec2, Align, Layout, Sense, Ui};
use lyra_model::{History, Project, STEPS_PER_BAR, TICKS_PER_STEP};
use lyra_plugin::{Curve, ParamInfo};

static SWING: ParamInfo = ParamInfo::new(0, "Swing", 0.0, 1.0, 0.0, "%", Curve::Linear);
pub static MASTER: ParamInfo = ParamInfo::new(0, "Master", 0.0, 2.0, 0.8, "gain", Curve::Fader);

#[allow(clippy::too_many_arguments)] // it's a view: it touches many things by design
pub fn show(
    ui: &mut Ui,
    p: &mut Project,
    h: &mut History,
    st: &mut UiState,
    playing: bool,
    playhead: f64,
    octave: i32,
    actions: &mut Vec<Action>,
) {
    // The toolbar is a rack: modules mounted flush, a hairline seam between
    // them, and a blank plate filling whatever is left.
    ui.spacing_mut().item_spacing.x = theme::SEAM;
    ui.horizontal_centered(|ui| {
        let big = widgets::toolbar_key();
        let tk = theme::m().tool_key;
        let small = theme::m().control_h;
        let gap = 4.0;

        // The File menu: everything about the song as a file.
        module(ui, |ui| {
            // A bare button: no plate, the legend in caps (catalog-next, settled).
            let label = egui::RichText::new("FILE").font(theme::label(13.0)).color(theme::LEGEND);
            egui::menu::menu_custom_button(ui, egui::Button::new(label).frame(false), |ui| {
                file_menu(ui, actions);
            })
            .response
            .hint("New, open, save, import, export");
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
            if widgets::choice(ui, vec2(tk * 1.4, tk), &options, chosen, hint).is_some() {
                actions.push(Action::SongMode(!st.song_mode));
            }
            ui.add_space(gap);
            let lit = playing.then_some(theme::L_MINT);
            if widgets::button_sized(ui, vec2(tk, tk), Legend::Icon(Icon::Play), lit, "Play / stop  (Space)").clicked()
            {
                actions.push(Action::TogglePlay);
            }
            let hint = "Stop: back to the start marker";
            if widgets::button_sized(ui, vec2(tk, tk), Legend::Icon(Icon::Stop), None, hint).clicked() {
                actions.push(Action::Stop);
            }
            ui.add_space(gap);
            let step = (playhead / TICKS_PER_STEP as f64) as u32;
            let pos = format!("{}.{}.{}", step / STEPS_PER_BAR + 1, (step % STEPS_PER_BAR) / 4 + 1, step % 4 + 1);
            widgets::lcd_text(ui, &pos, "BAR . BEAT . STEP", vec2(big * 2.6, big))
                .hint("Song position: bar . beat . step");
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

        // Where the typing keyboard plays.
        module(ui, |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            if widgets::button_sized(ui, vec2(small, small), Legend::Icon(Icon::Minus), None, "Octave down  ( - )")
                .clicked()
            {
                actions.push(Action::Octave(-1));
            }
            let first = (48 + 12 * octave).clamp(0, 127) as u8;
            let last = first.saturating_add(28).min(127); // Z … P spans C to E, 2⅓ octaves
            if let Some(d) = widgets::keyboard_map(ui, first, last) {
                actions.push(Action::Octave(d));
            }
            if widgets::button_sized(ui, vec2(small, small), Legend::Icon(Icon::Plus), None, "Octave up  ( = )")
                .clicked()
            {
                actions.push(Action::Octave(1));
            }
        });

        // The layout: a key per panel, lit while it's open (the bottom area
        // keeps whichever editor it shows; F7 / F9 open it on the piano roll / mixer).
        module(ui, |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (icon, open, hint, action) in [
                (Icon::PanelLeft, st.show_browser, "Left panel: the browser  (F4)", Action::ToggleBrowser),
                (Icon::PanelBottom, st.bottom_open, "Bottom area  (F5)", Action::ToggleBottom),
                (Icon::PanelRight, st.show_plugins, "Right panel: racks  (F6)", Action::ToggleRack),
            ] {
                if widgets::button_sized(ui, vec2(tk, tk), Legend::Icon(icon), open.then_some(theme::L_MINT), hint)
                    .clicked()
                {
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

/// The File menu's items.
fn file_menu(ui: &mut Ui, actions: &mut Vec<Action>) {
    for (label, keys, action) in [
        ("New", "", Action::NewProject),
        ("Open…", "Ctrl+O", Action::OpenProject),
        ("Save", "Ctrl+S", Action::Save),
        ("Save as…", "Ctrl+Shift+S", Action::SaveAs),
        ("Collect audio into the song's folder", "", Action::Collect),
        ("Import MIDI…", "", Action::ImportMidi),
        ("Export WAV…", "Ctrl+E", Action::ExportWav),
        ("Export stems…", "Ctrl+Shift+E", Action::ExportStems),
        ("Export data…", "", Action::ExportData),
    ] {
        if ui.add(egui::Button::new(label).shortcut_text(keys)).clicked() {
            actions.push(action);
            ui.close_menu();
        }
    }
    ui.separator();
    if ui.button("Open backup").hint("The copy Lyra keeps of your latest work").clicked() {
        actions.push(Action::OpenBackup);
        ui.close_menu();
    }
}
