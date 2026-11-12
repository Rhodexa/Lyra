//! The mixer (F9): one strip per mixer track (master + inserts), and the
//! selected track's effect chain on the right.
//!
//! Channels pick their insert in the instrument editor (new channels get the
//! next free one). Faders and knobs: drag (Shift = fine), wheel, double-click
//! to reset. Strip lights mute. Effects run top to bottom.

use crate::app::UiState;
use crate::rack::tracked;
use crate::theme::{self, cr, stroke};
use crate::widgets;
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, StrokeKind, Ui};
use lyra_model::{EffectSlot, History, Project};
use lyra_plugin::{Curve, ParamInfo};

static PAN: ParamInfo = ParamInfo::new(0, "Pan", -1.0, 1.0, 0.0, "pan", Curve::Linear);
const FADER_MAX: f32 = 1.25;

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    let m = theme::m();
    st.mixer_selected = st.mixer_selected.min(p.mixer.len() - 1);
    ui.horizontal_top(|ui| {
        egui::ScrollArea::horizontal().id_salt("mixer-strips").max_width(ui.available_width() - 340.0).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for t in 0..p.mixer.len() {
                    strip(ui, p, h, st, t);
                    if t == 0 {
                        ui.add_space(m.module_gap); // master stands apart
                    }
                }
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_width(320.0);
            chain(ui, p, h, st);
        });
    });
}

fn strip(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState, t: usize) {
    let m = theme::m();
    let selected = st.mixer_selected == t;
    let width = m.knob_small * 2.6;
    let routed: Vec<usize> = (0..p.channels.len()).filter(|&c| p.channels[c].mixer == t).collect();
    let color = routed.first().map_or(theme::MINT, |&c| theme::channel_color(p.channels[c].color));
    let frame = egui::Frame::default()
        .fill(if selected { theme::mix(theme::PANEL_HI, color, 0.10) } else { theme::PANEL_HI })
        .stroke(stroke(1.0, if selected { color.gamma_multiply(0.6) } else { theme::LINE }))
        .corner_radius(cr(m.corner as u8))
        .inner_margin(egui::Margin::same(4));
    let resp = frame
        .show(ui, |ui| {
            ui.set_width(width);
            ui.vertical_centered(|ui| {
                let name = &p.mixer[t].name;
                ui.label(RichText::new(name).small().color(if selected { theme::TEXT } else { theme::TEXT_DIM }));
                // Which channels feed this track, as coloured dots.
                let (r, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
                for (k, &c) in routed.iter().take(8).enumerate() {
                    let x = r.center().x + (k as f32 - (routed.len().min(8) as f32 - 1.0) / 2.0) * 7.0;
                    ui.painter().circle_filled(pos2(x, r.center().y), 2.5, theme::channel_color(p.channels[c].color));
                }
                ui.add_space(2.0);
                let mut pan = p.mixer[t].pan;
                if tracked(&widgets::knob(ui, &mut pan, &PAN, m.knob_small, color), h, p) {
                    p.mixer[t].pan = pan;
                }
                ui.horizontal(|ui| {
                    ui.add_space((width - (m.step_w + 14.0 + 4.0)) / 2.0);
                    meter_v(ui, st.track_meters.get(t).copied().unwrap_or_default(), m.step_h * 5.0);
                    let mut vol = p.mixer[t].volume;
                    if tracked(&fader(ui, &mut vol, m.step_h * 5.0, color), h, p) {
                        p.mixer[t].volume = vol;
                    }
                });
                ui.label(RichText::new(db_text(p.mixer[t].volume)).small().color(theme::TEXT_DIM));
                let r = widgets::led(ui, !p.mixer[t].mute, color, "Mute this track");
                if r.clicked() {
                    h.checkpoint(p);
                    p.mixer[t].mute ^= true;
                }
                let fx = p.mixer[t].effects.len();
                ui.label(
                    RichText::new(if fx == 0 { "·".into() } else { format!("{fx} fx") }).small().color(theme::TEXT_DIM),
                );
            });
        })
        .response;
    if resp.interact(Sense::click()).clicked()
        || ui.input(|i| i.pointer.primary_pressed()) && ui.rect_contains_pointer(resp.rect)
    {
        st.mixer_selected = t;
    }
}

/// Vertical fader. Drag (Shift = fine), wheel, double-click = back to 0 dB-ish (0.8).
fn fader(ui: &mut Ui, value: &mut f32, height: f32, color: Color32) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(14.0, height), Sense::click_and_drag());
    let before = *value;
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        *value -= resp.drag_delta().y / height * FADER_MAX * if fine { 0.2 } else { 1.0 };
    }
    if resp.hovered() {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            *value += wheel.signum() * 0.02;
        }
    }
    if resp.double_clicked() {
        *value = 0.8;
    }
    *value = value.clamp(0.0, FADER_MAX);
    if *value != before {
        resp.mark_changed();
    }
    let p = ui.painter();
    let track = Rect::from_center_size(rect.center(), vec2(4.0, height));
    p.rect_filled(track, cr(2), theme::INSET);
    let y = rect.bottom() - *value / FADER_MAX * height;
    p.rect_filled(Rect::from_min_max(pos2(track.left(), y), track.max), cr(2), color.gamma_multiply(0.5));
    let cap = Rect::from_center_size(pos2(rect.center().x, y), vec2(14.0, 8.0));
    p.rect_filled(cap, cr(2), if resp.hovered() || resp.dragged() { theme::TEXT } else { theme::RAISED });
    p.rect_stroke(cap, cr(2), stroke(1.0, theme::LINE), StrokeKind::Inside);
    resp.on_hover_text(db_text(*value))
}

fn db_text(v: f32) -> String {
    if v <= 0.0001 {
        "-inf dB".into()
    } else {
        format!("{:+.1} dB", 20.0 * v.log10())
    }
}

/// Stereo vertical meter, mint → butter → rose (≈ -48 … 0 dB).
fn meter_v(ui: &mut Ui, level: [f32; 2], height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, height), Sense::hover());
    let p = ui.painter();
    for (c, lvl) in level.iter().enumerate() {
        let col = Rect::from_min_size(pos2(rect.left() + c as f32 * 5.0, rect.top()), vec2(4.0, height));
        p.rect_filled(col, cr(1), theme::INSET);
        let db = 20.0 * lvl.max(1e-6).log10();
        let lit = ((db + 48.0) / 48.0).clamp(0.0, 1.0);
        let top = col.bottom() - lit * height;
        for (from, to, color) in [(0.0, 0.75, theme::MINT), (0.75, 0.92, theme::BUTTER), (0.92, 1.0, theme::ROSE)] {
            let (a, b) = (col.bottom() - to * height, col.bottom() - from * height);
            let seg = Rect::from_min_max(pos2(col.left(), a.max(top)), pos2(col.right(), b));
            if seg.height() > 0.0 {
                p.rect_filled(seg, cr(0), color);
            }
        }
    }
}

/// The selected track's effects, top to bottom, each with its knobs.
fn chain(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    let m = theme::m();
    let t = st.mixer_selected;
    ui.label(RichText::new(&p.mixer[t].name).heading());
    let feeding: Vec<String> = p.channels.iter().filter(|c| c.mixer == t).map(|c| c.name.clone()).collect();
    let from = if feeding.is_empty() && t != 0 {
        "nothing routed here yet".to_string()
    } else if t == 0 {
        "everything ends here".into()
    } else {
        feeding.join(", ")
    };
    ui.label(RichText::new(from).small().color(theme::TEXT_DIM));
    ui.add_space(6.0);

    let mut remove = None;
    let mut swap = None;
    let count = p.mixer[t].effects.len();
    egui::ScrollArea::vertical().id_salt("mixer-chain").max_height(ui.available_height() - 40.0).show(ui, |ui| {
        for i in 0..count {
            let slot = p.mixer[t].effects[i].clone();
            let name = lyra_dsp::effects::info(&slot.effect).map_or("Missing effect", |e| e.name);
            egui::Frame::default()
                .fill(theme::INSET)
                .corner_radius(cr(m.corner as u8))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if widgets::led(ui, !slot.bypass, theme::MINT, "On / bypass").clicked() {
                            h.checkpoint(p);
                            p.mixer[t].effects[i].bypass ^= true;
                        }
                        ui.label(RichText::new(name).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("x").on_hover_text("Remove").clicked() {
                                remove = Some(i);
                            }
                            if i + 1 < count && ui.small_button("v").on_hover_text("Move down").clicked() {
                                swap = Some((i, i + 1));
                            }
                            if i > 0 && ui.small_button("^").on_hover_text("Move up").clicked() {
                                swap = Some((i - 1, i));
                            }
                        });
                    });
                    let infos = lyra_dsp::effects::params(&slot.effect);
                    ui.horizontal_wrapped(|ui| {
                        for info in infos {
                            let id = info.id as usize;
                            let Some(&value) = p.mixer[t].effects[i].params.get(id) else { continue };
                            ui.allocate_ui(vec2(m.knob_large + 18.0, m.knob_large + 2.0 * m.text_small + 12.0), |ui| {
                                ui.vertical_centered(|ui| {
                                    let mut v = value;
                                    if info.is_choice() {
                                        let mut idx = v.round() as usize;
                                        egui::ComboBox::from_id_salt(("fx", t, i, id))
                                            .width(m.knob_large + 12.0)
                                            .selected_text(info.labels[idx.min(info.labels.len() - 1)])
                                            .show_ui(ui, |ui| {
                                                for (k, label) in info.labels.iter().enumerate() {
                                                    ui.selectable_value(&mut idx, k, *label);
                                                }
                                            });
                                        if idx as f32 != v.round() {
                                            h.checkpoint(p);
                                            p.mixer[t].effects[i].params[id] = idx as f32;
                                        }
                                    } else if tracked(
                                        &widgets::knob(ui, &mut v, info, m.knob_large * 0.8, theme::MINT),
                                        h,
                                        p,
                                    ) {
                                        p.mixer[t].effects[i].params[id] = v;
                                    }
                                    ui.label(RichText::new(info.name).small());
                                    ui.label(
                                        RichText::new(info.format(p.mixer[t].effects[i].params[id]))
                                            .small()
                                            .color(theme::TEXT_DIM),
                                    );
                                });
                            });
                        }
                    });
                });
            ui.add_space(4.0);
        }
    });
    if let Some(i) = remove {
        h.checkpoint(p);
        p.mixer[t].effects.remove(i);
    }
    if let Some((a, b)) = swap {
        h.checkpoint(p);
        p.mixer[t].effects.swap(a, b);
    }
    if count < lyra_engine::MAX_FX {
        ui.menu_button("+ add effect", |ui| {
            for e in lyra_dsp::effects::EFFECTS {
                if ui.button(e.name).clicked() {
                    h.checkpoint(p);
                    p.mixer[t].effects.push(EffectSlot {
                        effect: e.id.into(),
                        params: lyra_dsp::effects::default_params(e.id),
                        bypass: false,
                    });
                    ui.close_menu();
                }
            }
        });
    }
}
