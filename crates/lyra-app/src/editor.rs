//! The instrument editor ("plugin window") for the selected channel.
//!
//! It is built entirely from the plugin's own description — `params()` and
//! `groups()` — so any future instrument (or hosted CLAP plugin) gets an
//! editor for free. Hand-made editors can replace it per instrument later.

use crate::rack::tracked;
use crate::theme::{self, cr, stroke};
use crate::widgets;
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, Ui};
use lyra_model::{History, Project};
use lyra_plugin::ParamInfo;

const CELL_W: f32 = 50.0;
const KNOB: f32 = 30.0;

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, ch: usize) {
    let instrument = p.channels[ch].instrument.clone();
    let params = lyra_dsp::params(&instrument);
    let groups = lyra_dsp::groups(&instrument);
    let color = theme::channel_color(p.channels[ch].color);
    // Older projects may lack newer parameters: fill them with defaults.
    if p.channels[ch].params.len() < params.len() {
        let defaults = lyra_dsp::default_params(&instrument);
        let have = p.channels[ch].params.len();
        p.channels[ch].params.extend_from_slice(&defaults[have..]);
    }

    // ── Header: name, instrument, preset, choke ──
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 4.0, color);
        let mut name = p.channels[ch].name.clone();
        let edit = ui.add(egui::TextEdit::singleline(&mut name).desired_width(120.0).font(egui::TextStyle::Body));
        if edit.changed() {
            if edit.gained_focus() || !h_has_name_checkpoint(ui) {
                h.checkpoint(p);
                set_name_checkpoint(ui, true);
            }
            p.channels[ch].name = name;
        }
        if edit.lost_focus() {
            set_name_checkpoint(ui, false);
        }
        let label = lyra_dsp::info(&instrument).map_or(instrument.as_str(), |i| i.name);
        ui.label(RichText::new(label).small().color(theme::TEXT_DIM));

        let presets: Vec<_> = lyra_dsp::presets_for(&instrument).collect();
        if !presets.is_empty() {
            ui.add_space(8.0);
            ui.menu_button("Presets", |ui| {
                for preset in presets {
                    if ui.button(preset.name).clicked() {
                        h.checkpoint(p);
                        p.channels[ch].params = preset.params();
                        ui.close_menu();
                    }
                }
            });
        }
        if !lyra_dsp::info(&instrument).is_some_and(|i| i.melodic) {
            ui.add_space(8.0);
            ui.label(RichText::new("choke").small().color(theme::TEXT_DIM));
            let mut choke = p.channels[ch].choke;
            let text = if choke == 0 { "off".to_string() } else { format!("group {choke}") };
            egui::ComboBox::from_id_salt(("choke", ch)).width(70.0).selected_text(text).show_ui(ui, |ui| {
                ui.selectable_value(&mut choke, 0, "off");
                for g in 1..=4 {
                    ui.selectable_value(&mut choke, g, format!("group {g}"));
                }
            })
            .response
            .on_hover_text("Channels in the same group cut each other off (closed hat stops open hat).");
            if choke != p.channels[ch].choke {
                h.checkpoint(p);
                p.channels[ch].choke = choke;
            }
        }
    });
    ui.add_space(6.0);

    // ── Parameters, group by group ──
    let all = [lyra_plugin::ParamGroup { name: "", ids: 0..params.len() as u32 }];
    let groups = if groups.is_empty() { &all[..] } else { groups };
    for row in groups.chunks(2) {
        ui.horizontal_top(|ui| {
            for group in row {
                ui.vertical(|ui| {
                    if !group.name.is_empty() {
                        ui.label(RichText::new(group.name.to_uppercase()).small().color(theme::TEXT_DIM));
                    }
                    egui::Frame::default().fill(theme::INSET).corner_radius(cr(4)).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            for id in group.ids.clone() {
                                if let Some(info) = params.get(id as usize) {
                                    param_cell(ui, p, h, ch, info, color);
                                }
                            }
                        });
                    });
                });
                ui.add_space(4.0);
            }
        });
        ui.add_space(2.0);
    }

    if instrument == "lyra.synth" {
        ui.add_space(6.0);
        envelope(ui, &p.channels[ch].params, color);
    }
}

fn param_cell(ui: &mut Ui, p: &mut Project, h: &mut History, ch: usize, info: &ParamInfo, color: Color32) {
    let id = info.id as usize;
    let mut v = p.channels[ch].params[id];
    ui.allocate_ui(vec2(CELL_W, 64.0), |ui| {
        ui.vertical_centered(|ui| {
            if info.is_choice() {
                ui.add_space(4.0);
                let mut idx = v.round() as usize;
                egui::ComboBox::from_id_salt(("param", ch, id))
                    .width(CELL_W - 4.0)
                    .selected_text(info.labels[idx.min(info.labels.len() - 1)])
                    .show_ui(ui, |ui| {
                        for (i, label) in info.labels.iter().enumerate() {
                            ui.selectable_value(&mut idx, i, *label);
                        }
                    });
                if idx as f32 != v.round() {
                    h.checkpoint(p);
                    p.channels[ch].params[id] = idx as f32;
                }
                ui.add_space(6.0);
            } else {
                let r = widgets::knob(ui, &mut v, info, KNOB, color);
                if tracked(&r, h, p) {
                    p.channels[ch].params[id] = v;
                }
            }
            ui.label(RichText::new(info.name).small());
            ui.label(RichText::new(info.format(p.channels[ch].params[id])).small().color(theme::TEXT_DIM));
        });
    });
}

/// Draw the amp envelope (attack, decay to sustain, hold, release).
fn envelope(ui: &mut Ui, params: &[f32], color: Color32) {
    use lyra_dsp::synth::p;
    let (a, d, s, r) = (params[p::ATTACK], params[p::DECAY], params[p::SUSTAIN], params[p::RELEASE]);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(420.0), 64.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, cr(4), theme::INSET);
    let inner = rect.shrink(6.0);
    // Square-root time axis: short attacks stay visible next to long releases.
    let (ta, td, th, tr) = (a.sqrt(), d.sqrt(), 0.5, r.sqrt());
    let total = ta + td + th + tr;
    let x = |t: f32| inner.left() + t / total * inner.width();
    let y = |level: f32| inner.bottom() - level * inner.height();
    let mut pts = vec![pos2(x(0.0), y(0.0)), pos2(x(ta), y(1.0))];
    for i in 1..=16 {
        let f = i as f32 / 16.0;
        let level = s + (1.0 - s) * (-6.9 * f).exp();
        pts.push(pos2(x(ta + td * f), y(level)));
    }
    pts.push(pos2(x(ta + td + th), y(s)));
    for i in 1..=16 {
        let f = i as f32 / 16.0;
        pts.push(pos2(x(ta + td + th + tr * f), y(s * (-6.9 * f).exp())));
    }
    let mut fill = pts.clone();
    fill.push(pos2(x(total), y(0.0)));
    for w in fill.windows(2) {
        // Thin vertical slivers: a cheap filled area under the curve.
        let r = Rect::from_min_max(pos2(w[0].x, w[0].y.min(w[1].y)), pos2(w[1].x.max(w[0].x + 0.5), inner.bottom()));
        painter.rect_filled(r, cr(0), color.gamma_multiply(0.12));
    }
    painter.add(egui::Shape::line(pts, stroke(1.5, color)));
    for (t, label) in [(ta * 0.5, "A"), (ta + td * 0.5, "D"), (ta + td + th * 0.5, "S"), (ta + td + th + tr * 0.5, "R")] {
        painter.text(pos2(x(t), inner.top() + 2.0), egui::Align2::CENTER_TOP, label, egui::FontId::proportional(9.0), theme::TEXT_DIM);
    }
}

// One undo checkpoint per rename (not one per keystroke), remembered in egui's temp memory.
fn h_has_name_checkpoint(ui: &Ui) -> bool {
    ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new("rename-checkpoint")).unwrap_or(false))
}
fn set_name_checkpoint(ui: &Ui, v: bool) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("rename-checkpoint"), v));
}
