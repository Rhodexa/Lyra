//! The top bar: logo, play/stop, position, beat lights, tempo, swing, master.

use crate::app::UiState;
use crate::keys::Action;
use crate::rack::tracked;
use crate::theme::{self, cr};
use crate::widgets::{self, Icon};
use egui::{vec2, Align, Align2, FontId, Layout, Rect, RichText, Sense, Ui};
use lyra_model::{History, Project, STEPS_PER_BAR, TICKS_PER_STEP};
use lyra_plugin::{Curve, ParamInfo};

static SWING: ParamInfo = ParamInfo::new(0, "Swing", 0.0, 1.0, 0.0, "%", Curve::Linear);
static MASTER: ParamInfo = ParamInfo::new(0, "Master", 0.0, 1.25, 0.8, "%", Curve::Linear);

pub fn show(
    ui: &mut Ui,
    p: &mut Project,
    h: &mut History,
    st: &UiState,
    playing: bool,
    playhead: f64,
    logo: egui::TextureId,
    actions: &mut Vec<Action>,
) {
    ui.horizontal_centered(|ui| {
        ui.add_space(4.0);
        widgets::logo(ui, logo);
        ui.label(RichText::new("lyra").size(15.0).color(theme::MINT));
        ui.add_space(16.0);

        if widgets::transport_button(ui, Icon::Play, playing, "Play / stop  (Space)").clicked() {
            actions.push(Action::TogglePlay);
        }
        if widgets::transport_button(ui, Icon::Stop, false, "Stop  (Esc)").clicked() {
            actions.push(Action::Stop);
        }
        ui.add_space(8.0);

        // Position display: bar . beat . step
        let step = if playing { (playhead / TICKS_PER_STEP as f64) as u32 } else { 0 };
        let (r, _) = ui.allocate_exact_size(vec2(70.0, 22.0), Sense::hover());
        ui.painter().rect_filled(r, cr(3), theme::INSET);
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            format!("{}.{}.{}", step / STEPS_PER_BAR + 1, (step % STEPS_PER_BAR) / 4 + 1, step % 4 + 1),
            FontId::monospace(13.0),
            theme::MINT,
        );

        // Beat lights
        let (r, _) = ui.allocate_exact_size(vec2(36.0, 22.0), Sense::hover());
        for b in 0..4u32 {
            let c = r.left_center() + vec2(5.0 + b as f32 * 9.0, 0.0);
            let on = playing && (step % STEPS_PER_BAR) / 4 == b;
            let color = if !on { theme::LINE } else if b == 0 { theme::GOLD } else { theme::MINT };
            ui.painter().circle_filled(c, 2.5, color);
        }
        ui.add_space(8.0);

        // Tempo
        let mut bpm = p.bpm;
        let r = ui
            .add(egui::DragValue::new(&mut bpm).range(40.0..=300.0).speed(0.25).fixed_decimals(1).suffix(" bpm"))
            .on_hover_text("Tempo: drag, or double-click to type");
        if tracked(&r, h, p) {
            p.bpm = bpm;
        }
        ui.add_space(8.0);

        ui.label(RichText::new("swing").small().color(theme::TEXT_DIM));
        let mut swing = p.swing;
        if tracked(&widgets::knob(ui, &mut swing, &SWING, 20.0, theme::LAVENDER), h, p) {
            p.swing = swing;
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(8.0);
            let (r, _) = ui.allocate_exact_size(vec2(112.0, 11.0), Sense::hover());
            let top = Rect::from_min_max(r.min, egui::pos2(r.max.x, r.center().y - 0.5));
            let bottom = Rect::from_min_max(egui::pos2(r.min.x, r.center().y + 0.5), r.max);
            widgets::meter(ui.painter(), top, st.master_meter[0]);
            widgets::meter(ui.painter(), bottom, st.master_meter[1]);
            let mut master = p.master_volume;
            if tracked(&widgets::knob(ui, &mut master, &MASTER, 20.0, theme::MINT), h, p) {
                p.master_volume = master;
            }
            ui.label(RichText::new("master").small().color(theme::TEXT_DIM));
        });
    });
}
