//! The mixer (F9): one strip per mixer track (master + inserts), and the
//! selected track's effect chain on the right.
//!
//! Channels pick their insert in the instrument editor (new channels get the
//! next free one). Faders and knobs: drag (Shift = fine), wheel, double-click
//! to reset. Strip lights mute. Effects run top to bottom.

use crate::app::UiState;
use crate::rack::tracked;
use crate::theme::{self, cr};
use crate::widgets;
use egui::{pos2, vec2, Align2, Color32, Rect, RichText, Sense, Ui};
use lyra_model::{EffectSlot, History, Project};
use lyra_plugin::{Curve, ParamInfo};

static PAN: ParamInfo = ParamInfo::new(0, "Pan", -1.0, 1.0, 0.0, "pan", Curve::Linear);
const FADER_MAX: f32 = 1.25;

/// Strip geometry, from the single-channel-strip study at about 60 % of its
/// size, with a little more air on the right (the meter side) and a taller
/// lower part for the name.
const FADER_W: f32 = 40.0;
const METER_W: f32 = 22.0;
const PAD_L: f32 = 4.0;
const PAD_R: f32 = 7.0;
const STRIP_W: f32 = PAD_L + FADER_W + METER_W + PAD_R;
const FADER_H: f32 = 210.0;
/// The lower part: vertical name on the left, pan and mute on the right.
const LOWER_H: f32 = 170.0;
const STRIP_H: f32 = 8.0 + FADER_H + 6.0 + LOWER_H + 6.0;

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    st.mixer_selected = st.mixer_selected.min(p.mixer.len() - 1);
    let seam = theme::SEAM;
    ui.horizontal_top(|ui| {
        egui::ScrollArea::horizontal().id_salt("mixer-strips").max_width(ui.available_width() - 340.0).show(ui, |ui| {
            // The window's own background is the rack frame; strips are
            // plates on it, a seam apart.
            let n = p.mixer.len();
            let width = n as f32 * (STRIP_W + seam) - seam;
            let (rack, _) = ui.allocate_exact_size(vec2(width, STRIP_H), Sense::hover());
            for t in 0..n {
                let x = rack.left() + t as f32 * (STRIP_W + seam);
                strip(ui, Rect::from_min_size(pos2(x, rack.top()), vec2(STRIP_W, STRIP_H)), p, h, st, t);
            }
        });
        ui.add_space(seam);
        egui::Frame::default().fill(theme::PANEL).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_width(320.0);
            ui.set_min_height(STRIP_H - 16.0);
            ui.vertical(|ui| chain(ui, p, h, st));
        });
    });
}

fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// One channel strip: fader and stereo meter on top; below, the vertical
/// name beside pan and mute. One flat surface, no outline.
fn strip(ui: &mut Ui, r: Rect, p: &mut Project, h: &mut History, st: &mut UiState, t: usize) {
    let id = ui.id().with(("strip", t));
    let selected = st.mixer_selected == t;
    let routed: Vec<usize> = (0..p.channels.len()).filter(|&c| p.channels[c].mixer == t).collect();
    let light = routed.first().map_or(theme::L_MINT, |&c| theme::channel_light(p.channels[c].color));
    let fill = light.mid;

    // Clicking anywhere on the strip (that isn't a control) selects it.
    let bg = ui.interact(r, id.with("bg"), Sense::click());
    if bg.clicked() {
        st.mixer_selected = t;
    }
    let painter = ui.painter().clone();
    painter.rect_filled(r, cr(0), if selected { theme::PANEL_HI } else { theme::PANEL });
    if selected {
        painter.rect_filled(Rect::from_min_size(r.min, vec2(r.width(), 2.0)), cr(0), fill);
    }

    let fader_r = Rect::from_min_size(pos2(r.left() + PAD_L, r.top() + 8.0), vec2(FADER_W, FADER_H));
    let meter_r = Rect::from_min_size(pos2(fader_r.right(), fader_r.top()), vec2(METER_W, FADER_H));
    let lower =
        Rect::from_min_max(pos2(r.left() + PAD_L, fader_r.bottom() + 6.0), pos2(r.right() - PAD_R, r.bottom() - 6.0));

    let mut vol = p.mixer[t].volume;
    if tracked(&fader(ui, id.with("fader"), fader_r, &mut vol, fill), h, p) {
        p.mixer[t].volume = vol;
    }
    if ui.input(|i| i.pointer.primary_pressed()) && ui.rect_contains_pointer(r) {
        st.mixer_selected = t;
    }
    let levels = st.track_meters.get(t).copied().unwrap_or_default();
    meter(ui, id.with("meter"), meter_r, fader_r, levels);

    // Lower part: name | pan, mute.
    let name_cell = Rect::from_min_max(lower.min, pos2(lower.left() + 24.0, lower.bottom()));
    let ctrl = Rect::from_min_max(pos2(name_cell.right(), lower.top()), lower.max);
    vertical_name(&painter, name_cell, &p.mixer[t].name, selected);
    // Which channels feed this track: small coloured dots above the name.
    for (k, &c) in routed.iter().take(4).enumerate() {
        let at = pos2(name_cell.center().x, name_cell.top() + 5.0 + k as f32 * 6.0);
        painter.circle_filled(at, 2.2, theme::channel_color(p.channels[c].color));
    }

    let label = hex(0xa6a6a6);
    let small = egui::FontId::proportional(9.0);
    let cx = ctrl.center().x;
    let pan_r = Rect::from_center_size(pos2(cx, ctrl.top() + 16.0), vec2(28.0, 28.0));
    let mut pan = p.mixer[t].pan;
    if tracked(&widgets::knob_at(ui, id.with("pan"), pan_r, &mut pan, &PAN, theme::MINT), h, p) {
        p.mixer[t].pan = pan;
    }
    let mut y = pan_r.bottom() + 4.0;
    painter.text(pos2(cx, y), Align2::CENTER_TOP, "PAN", small.clone(), label);
    y += 12.0;
    let pan_text = match (pan * 100.0).round() as i32 {
        0 => "C".to_string(),
        v if v < 0 => format!("L{}", -v),
        v => format!("R{v}"),
    };
    painter.text(pos2(cx, y), Align2::CENTER_TOP, pan_text, egui::FontId::proportional(10.5), theme::MINT);
    y += 22.0;

    // Mute: lit = audible, like every light in Lyra. Alt-click: solo (again: everyone back).
    let led_r = Rect::from_center_size(pos2(cx, y + 11.0), vec2(22.0, 22.0));
    let led = ui.interact(led_r, id.with("mute"), Sense::click());
    theme::led(&painter, led_r.center(), 22.0, (!p.mixer[t].mute).then_some(theme::L_MINT), led.hovered());
    if led.clicked() {
        h.checkpoint(p);
        if ui.input(|i| i.modifiers.alt) {
            solo(p, t);
        } else {
            p.mixer[t].mute ^= true;
        }
    }
    led.on_hover_text("Mute (lit = on) · Alt-click: solo");
    painter.text(pos2(cx, led_r.bottom() + 4.0), Align2::CENTER_TOP, "MUTE", small, label);

    let fx = p.mixer[t].effects.len();
    if fx > 0 {
        painter.text(
            pos2(cx, ctrl.bottom()),
            Align2::CENTER_BOTTOM,
            format!("{fx} fx"),
            egui::FontId::proportional(9.0),
            theme::TEXT_DIM,
        );
    }
}

/// Solo an insert: mute the other inserts (the master is never muted). Soloing
/// the only audible insert brings everybody back.
fn solo(p: &mut Project, t: usize) {
    if t == 0 {
        return;
    }
    let alone = (1..p.mixer.len()).all(|i| (i == t) != p.mixer[i].mute);
    for i in 1..p.mixer.len() {
        p.mixer[i].mute = !alone && i != t;
    }
}

/// The track name, written bottom-to-top, clipped with an ellipsis.
fn vertical_name(p: &egui::Painter, cell: Rect, name: &str, selected: bool) {
    let mut job = egui::text::LayoutJob::single_section(
        name.to_uppercase(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(12.0),
            extra_letter_spacing: 1.2,
            color: if selected { theme::TEXT } else { hex(0xc0c0c0) },
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping {
        max_width: cell.height() - 30.0,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = p.layout_job(job);
    let size = galley.size();
    // Rotated -90°: the text's top-left lands at the bottom-left of its box.
    // Anchored at the bottom, so names line up from the floor of the strip.
    let pos = pos2(cell.center().x - size.y / 2.0, cell.bottom() - 4.0);
    p.add(egui::epaint::TextShape::new(pos, galley, theme::TEXT).with_angle(-std::f32::consts::FRAC_PI_2));
}

/// The study's fader, scaled: ticks on the left, a recessed track lit from
/// the bottom up to a three-layer cap with grip lines. Drag (Shift = fine),
/// wheel, double-click = back to 0.8 (about -2 dB).
fn fader(ui: &mut Ui, id: egui::Id, rect: Rect, value: &mut f32, fill: Color32) -> egui::Response {
    let mut resp = ui.interact(rect, id, Sense::click_and_drag());
    let before = *value;
    let (y0, y1) = (rect.top() + 10.0, rect.bottom() - 9.0);
    let span = (y1 - y0).max(10.0);
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        *value -= resp.drag_delta().y / span * FADER_MAX * if fine { 0.2 } else { 1.0 };
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
    let cx = rect.left() + rect.width() * 0.55;
    for k in 0..=10 {
        let y = y0 + k as f32 * span / 10.0;
        let major = k % 5 == 0;
        let (x1, x2) = (rect.left() + 3.0, rect.left() + if major { 8.0 } else { 6.0 });
        p.hline(x1..=x2, y, theme::stroke(if major { 1.4 } else { 1.0 }, hex(if major { 0x737373 } else { 0x505050 })));
    }
    let outer = Rect::from_min_max(pos2(cx - 4.0, y0), pos2(cx + 4.0, y1));
    p.rect_filled(outer, cr(4), hex(0x161616));
    let inner = outer.shrink2(vec2(2.0, 2.0));
    p.rect_filled(inner, cr(2), hex(0x2b2b2b));
    let y = y1 - *value / FADER_MAX * span;
    if inner.bottom() - y > 0.5 {
        p.rect_filled(Rect::from_min_max(pos2(inner.left(), y.max(inner.top())), inner.max), cr(2), fill);
    }
    // Cap: dark rim, body, face, three grip lines; a soft shadow under it.
    let hot = resp.hovered() || resp.dragged();
    let lift = |c: u32| if hot { theme::shade(hex(c), 1.08) } else { hex(c) };
    let w = rect.width() - 6.0;
    p.rect_filled(Rect::from_center_size(pos2(cx, y + 1.5), vec2(w, 17.0)), cr(4), Color32::from_black_alpha(60));
    p.rect_filled(Rect::from_center_size(pos2(cx, y + 0.5), vec2(w, 17.0)), cr(4), hex(0x202020));
    p.rect_filled(Rect::from_center_size(pos2(cx, y), vec2(w - 2.0, 15.0)), cr(3), lift(0x444444));
    p.rect_filled(Rect::from_center_size(pos2(cx, y), vec2(w - 6.0, 11.0)), cr(3), lift(0x4f4f4f));
    for k in -1..=1 {
        let c =
            theme::mix(lift(0x4f4f4f), hex(if k == 0 { 0x818181 } else { 0x727272 }), if k == 0 { 0.78 } else { 0.55 });
        p.hline(cx - 7.0..=cx + 7.0, y + k as f32 * 2.5, theme::stroke(1.0, c));
    }
    resp.on_hover_text(db_text(*value))
}

fn db_text(v: f32) -> String {
    if v <= 0.0001 {
        "-inf dB".into()
    } else {
        format!("{:+.1} dB", 20.0 * v.log10())
    }
}

/// The study's stereo meter: a dark well, two bars in a fixed full-scale
/// gradient (-60 … 0 dB), and a pale peak-hold line that waits, then falls.
fn meter(ui: &Ui, id: egui::Id, rect: Rect, fader: Rect, levels: [f32; 2]) {
    let p = ui.painter();
    let (y0, y1) = (fader.top() + 10.0, fader.bottom() - 9.0);
    let well = Rect::from_min_max(pos2(rect.left() + 1.0, y0 - 2.0), pos2(rect.right() - 1.0, y1 + 2.0));
    p.rect_filled(well, cr(2), hex(0x151515));
    let inner = well.shrink(2.0);
    p.rect_filled(inner, cr(1), hex(0x1f1f1f));
    let now = ui.input(|i| i.time);
    let bar_w = (inner.width() - 6.0) / 2.0;
    for (k, level) in levels.into_iter().enumerate() {
        let bar = Rect::from_min_size(
            pos2(inner.left() + 2.0 + k as f32 * (bar_w + 2.0), inner.top() + 2.0),
            vec2(bar_w, inner.height() - 4.0),
        );
        let frac = widgets::meter_fraction(level);
        widgets::meter_bar(p, bar, frac, true);
        // Peak hold: (fraction, time it was set).
        let hold_id = id.with(k);
        let (mut held, mut since): (f32, f64) = ui.data(|d| d.get_temp(hold_id)).unwrap_or((0.0, 0.0));
        if frac >= held {
            (held, since) = (frac, now);
        } else if now - since > 0.4 {
            held = (held - 0.2 * ui.input(|i| i.stable_dt).min(0.1)).max(frac);
        }
        ui.data_mut(|d| d.insert_temp(hold_id, (held, since)));
        if held > 0.02 {
            let y = bar.bottom() - held * bar.height();
            p.rect_filled(
                Rect::from_min_size(pos2(bar.left(), y - 1.0), vec2(bar.width(), 2.0)),
                cr(0),
                hex(0xe5e9b7).gamma_multiply(0.9),
            );
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
                .fill(theme::PANEL_HI)
                .stroke(theme::stroke(1.0, theme::LINE))
                .corner_radius(cr(3))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if widgets::led(ui, !slot.bypass, theme::L_MINT, "On / bypass").clicked() {
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
