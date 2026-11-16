//! The browser: the docked panel on the left (F4 shows / hides it).
//!
//! For now it lists what Lyra already has, so it's useful from day one:
//! * **Patterns** — click to edit (and place) that pattern
//! * **Instruments** — click to add one to the instrument rack; open the
//!   arrow for its presets
//! * **Effects** — click to plug one into the selected mixer track
//!
//! Samples (a library folder on disk) join it in the next stage.

use crate::app::{Focus, UiState};
use crate::theme::{self, cr};
use egui::{pos2, vec2, Align2, FontId, Rect, Response, Sense, Ui};
use lyra_model::{EffectSlot, History, Project};

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    let m = theme::m();
    egui::ScrollArea::vertical().id_salt("browser").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;

        // ── Patterns ──
        if section(ui, "Patterns", p.patterns.len()) {
            for i in 0..p.patterns.len() {
                let light = theme::channel_light(i);
                let r = row(ui, &p.patterns[i].name, Some(light), st.pattern == i, 0.0);
                if r.on_hover_text("Click: edit this pattern (and place it in the arranger)").clicked() {
                    st.pattern = i;
                }
            }
        }

        // ── Instruments ──
        if section(ui, "Instruments", lyra_dsp::INSTRUMENTS.len()) {
            for inst in lyra_dsp::INSTRUMENTS {
                let presets = lyra_dsp::presets_for(inst.id);
                let open_id = ui.id().with(("presets", inst.id));
                let mut open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
                let (r, arrow) = row_with_arrow(ui, inst.name, !presets.is_empty(), open);
                if arrow {
                    open ^= true;
                    ui.data_mut(|d| d.insert_temp(open_id, open));
                } else if r.on_hover_text("Click: add to the instrument rack").clicked() {
                    add_instrument(p, h, st, inst.name, inst.id, lyra_dsp::default_params(inst.id));
                }
                if open {
                    for preset in &presets {
                        let r = row(ui, preset.name, None, false, 14.0);
                        if r.on_hover_text("Click: add this preset to the instrument rack").clicked() {
                            add_instrument(p, h, st, preset.name, inst.id, preset.params());
                        }
                    }
                }
            }
        }

        // ── Effects ──
        if section(ui, "Effects", lyra_dsp::effects::EFFECTS.len()) {
            let t = st.mixer_selected.min(p.mixer.len() - 1);
            let full = p.mixer[t].effects.len() >= lyra_engine::MAX_FX;
            for e in lyra_dsp::effects::EFFECTS {
                let hint = if full {
                    format!("{}'s effect rack is full", p.mixer[t].name)
                } else {
                    format!("Click: plug into {} (the selected mixer track)", p.mixer[t].name)
                };
                if row(ui, e.name, None, false, 0.0).on_hover_text(hint).clicked() && !full {
                    h.checkpoint(p);
                    p.mixer[t].effects.push(EffectSlot {
                        effect: e.id.into(),
                        params: lyra_dsp::effects::default_params(e.id),
                        bypass: false,
                    });
                    st.effect = Some((t, p.mixer[t].effects.len() - 1));
                    st.show_mixer = true;
                    st.raise = Some(Focus::Effect);
                }
            }
        }

        ui.add_space(m.text);
        ui.label(egui::RichText::new("Samples arrive in the next stage.").small().color(theme::TEXT_DIM));
    });
}

fn add_instrument(p: &mut Project, h: &mut History, st: &mut UiState, name: &str, id: &str, params: Vec<f32>) {
    h.checkpoint(p);
    p.add_channel(name, id, params);
    st.selected = p.channels.len() - 1;
    st.show_rack = true;
    st.raise = Some(Focus::Rack);
}

/// A section header (click to fold). Returns whether it's open.
fn section(ui: &mut Ui, title: &str, count: usize) -> bool {
    let m = theme::m();
    let id = ui.id().with(("section", title));
    let mut open: bool = ui.data(|d| d.get_temp(id)).unwrap_or(true);
    ui.add_space(6.0);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click());
    if resp.clicked() {
        open ^= true;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    let p = ui.painter();
    chevron(p, pos2(r.left() + 8.0, r.center().y), open, theme::TEXT_DIM);
    p.text(
        r.left_center() + vec2(20.0, 0.0),
        Align2::LEFT_CENTER,
        title.to_uppercase(),
        FontId::proportional(m.text_small),
        if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM },
    );
    p.text(
        r.right_center() - vec2(6.0, 0.0),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::monospace(m.text_small),
        theme::TEXT_DIM,
    );
    open
}

/// One item: an optional light, the name, a highlight when hovered or chosen.
fn row(ui: &mut Ui, name: &str, light: Option<theme::Light>, chosen: bool, indent: f32) -> Response {
    let m = theme::m();
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click());
    paint_row(ui, r, &resp, name, light, chosen, indent);
    resp
}

/// An item with a fold arrow on the right (for instruments with presets).
/// Returns the row's response and whether the arrow was the part clicked.
fn row_with_arrow(ui: &mut Ui, name: &str, has_arrow: bool, open: bool) -> (Response, bool) {
    let m = theme::m();
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click());
    paint_row(ui, r, &resp, name, None, false, 0.0);
    if !has_arrow {
        return (resp, false);
    }
    let arrow = Rect::from_min_max(pos2(r.right() - m.control_h, r.top()), r.max);
    let over = resp.hover_pos().is_some_and(|p| arrow.contains(p));
    if over {
        ui.painter().rect_filled(arrow.shrink(3.0), cr(3), theme::RAISED);
    }
    chevron(ui.painter(), arrow.center(), open, if over { theme::TEXT } else { theme::TEXT_DIM });
    let clicked = resp.clicked() && over;
    (resp.on_hover_text("Presets: the arrow"), clicked)
}

fn paint_row(ui: &Ui, r: Rect, resp: &Response, name: &str, light: Option<theme::Light>, chosen: bool, indent: f32) {
    let m = theme::m();
    let p = ui.painter();
    if chosen || resp.hovered() {
        p.rect_filled(r, cr(3), if chosen { theme::PANEL_HI } else { theme::mix(theme::PANEL, theme::PANEL_HI, 0.6) });
    }
    let mut x = r.left() + 10.0 + indent;
    if let Some(l) = light {
        p.circle_filled(pos2(x + 3.0, r.center().y), 3.5, l.mid);
        x += 14.0;
    }
    p.text(
        pos2(x, r.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(m.text),
        if chosen { theme::TEXT } else { theme::mix(theme::TEXT, theme::TEXT_DIM, 0.35) },
    );
}

/// A small open/closed arrow.
fn chevron(p: &egui::Painter, c: egui::Pos2, open: bool, color: egui::Color32) {
    let pts = if open {
        vec![c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)]
    } else {
        vec![c + vec2(-1.5, -3.5), c + vec2(2.0, 0.0), c + vec2(-1.5, 3.5)]
    };
    p.add(egui::Shape::line(pts, theme::stroke(1.5, color)));
}
