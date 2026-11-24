//! The mixer (F9): one strip per mixer track (master + inserts), and the
//! selected track's effect rack on the right: eight fixed slots, each opening
//! its effect in the effect window (like channel rack → instrument).
//!
//! Channels pick their insert in the instrument editor (new channels get the
//! next free one). Faders and knobs: drag (Shift = fine), wheel, double-click
//! to reset. Strip lights mute. Effects run top to bottom.

use crate::app::UiState;
use crate::widgets::Hint;
use crate::rack::tracked;
use crate::theme::{self, cr, hex};
use crate::widgets;
use egui::{pos2, vec2, Align2, Color32, Rect, RichText, Sense, Ui};
use lyra_model::{EffectSlot, History, Project};
use lyra_plugin::{Curve, ParamInfo};

static PAN: ParamInfo = ParamInfo::new(0, "Pan", -1.0, 1.0, 0.0, "pan", Curve::Linear);
/// +6 dB at the top; the travel follows `lyra_plugin::fader_gain` (unity near 79 %).
const FADER_MAX: f32 = 2.0;

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
/// Room at the strip's top for its index tag.
const TOP: f32 = 22.0;
pub const STRIP_H: f32 = TOP + FADER_H + 6.0 + LOWER_H + 6.0;

/// Width of the effect rack beside the strips (inside its padding).
const RACK_W: f32 = 240.0;
const RACK_PAD: f32 = 8.0;

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    st.mixer_selected = st.mixer_selected.min(p.mixer.len() - 1);
    let seam = theme::SEAM;
    ui.horizontal_top(|ui| {
        // Every width here adds up exactly to what the window offers. (When it
        // didn't, the content asked for a few pixels more each frame and the
        // window crept back open after you shrank it.)
        ui.spacing_mut().item_spacing.x = 0.0;
        let strips_w = (ui.available_width() - seam - RACK_W - 2.0 * RACK_PAD).max(STRIP_W);
        // Strips fit the height they're given (the bottom area can be resized):
        // the fader keeps its size, the lower part takes what's left, so the
        // names, anchored at the bottom, always end inside the strip.
        let gutter = ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_inner_margin + ui.spacing().scroll.bar_outer_margin;
        let strip_h = (ui.available_height() - gutter).clamp(TOP + FADER_H + 6.0 + 110.0, STRIP_H);
        egui::ScrollArea::horizontal().id_salt("mixer-strips").max_width(strips_w).show(ui, |ui| {
            // The window's own background is the rack frame; strips are
            // plates on it, a seam apart.
            let n = p.mixer.len();
            let width = n as f32 * (STRIP_W + seam) - seam;
            let (rack, _) = ui.allocate_exact_size(vec2(width, strip_h), Sense::hover());
            for t in 0..n {
                let x = rack.left() + t as f32 * (STRIP_W + seam);
                strip(ui, Rect::from_min_size(pos2(x, rack.top()), vec2(STRIP_W, strip_h)), p, h, st, t);
            }
        });
        ui.add_space(seam);
        // The effect rack sits above the strips: it casts a soft shadow onto them.
        let edge = ui.cursor().left() - seam;
        let top = ui.cursor().top();
        let shadow = Rect::from_min_max(pos2(edge - 10.0, top), pos2(edge, top + STRIP_H));
        let mut mesh = egui::Mesh::default();
        for (x, a) in [(shadow.left(), 0u8), (shadow.right(), 110u8)] {
            mesh.colored_vertex(pos2(x, shadow.top()), Color32::from_black_alpha(a));
            mesh.colored_vertex(pos2(x, shadow.bottom()), Color32::from_black_alpha(a));
        }
        mesh.add_triangle(0, 1, 3);
        mesh.add_triangle(0, 3, 2);
        ui.painter().add(egui::Shape::mesh(mesh));
        egui::Frame::default().fill(theme::PANEL).inner_margin(egui::Margin::same(RACK_PAD as i8)).show(ui, |ui| {
            ui.set_width(RACK_W);
            ui.set_height(STRIP_H - 2.0 * RACK_PAD);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                effect_rack(ui, p, h, st);
            });
        });
    });
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
    // The insert's index (M, 1, 2, …): a tag hanging from the top centre.
    let g = ui.fonts(|f| f.layout_no_wrap(widgets::insert_label(t), theme::label(11.0), theme::INK_DIM));
    let tag = Rect::from_center_size(pos2(r.center().x, r.top() + 8.5), vec2((g.size().x + 14.0).max(26.0), 17.0));
    painter.rect_filled(tag, egui::CornerRadius { nw: 0, ne: 0, sw: 5, se: 5 }, theme::GROUND);
    painter.galley(tag.center() - g.size() * 0.5, g, if selected { theme::INK } else { theme::INK_DIM });
    if selected {
        painter.rect_filled(Rect::from_min_size(r.min, vec2(r.width(), 2.0)), cr(0), fill);
    }

    let fader_r = Rect::from_min_size(pos2(r.left() + PAD_L, r.top() + TOP), vec2(FADER_W, FADER_H));
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
    painter.text(pos2(cx, y), Align2::CENTER_TOP, pan_text, egui::FontId::proportional(10.5), theme::TEXT);
    y += 22.0;

    // Mute: lit = audible, like every light in Lyra. Alt-click: solo (again: everyone back).
    let led_r = Rect::from_center_size(pos2(cx, y + 11.0), vec2(22.0, 22.0));
    let led = ui.interact(led_r, id.with("mute"), Sense::click_and_drag());
    theme::led(&painter, led_r.center(), 22.0, (!p.mixer[t].mute).then_some(theme::L_MINT), led.hovered());
    if ui.input(|i| i.modifiers.alt) {
        if led.clicked() {
            h.checkpoint(p);
            solo(p, t);
        }
    } else {
        match widgets::sweep(ui, led_r, "mixer-mute", !p.mixer[t].mute) {
            Some(widgets::Sweep::Begin(on)) => {
                h.checkpoint(p);
                p.mixer[t].mute = !on;
            }
            Some(widgets::Sweep::Continue(on)) => p.mixer[t].mute = !on,
            None => {}
        }
    }
    led.hint("Mute (lit = on) · Alt-click: solo");
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
    // Work in travel (where the cap is), not gain, so the feel matches the drawing.
    let mut travel = lyra_plugin::fader_travel(*value, FADER_MAX);
    let moved_from = travel;
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        travel -= resp.drag_delta().y / span * if fine { 0.2 } else { 1.0 };
    }
    if resp.hovered() {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            travel += wheel.signum() * 0.015; // about half a dB near unity
        }
    }
    if travel != moved_from {
        *value = lyra_plugin::fader_gain(travel, FADER_MAX);
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
    let y = y1 - lyra_plugin::fader_travel(*value, FADER_MAX) * span;
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
    resp.hint(db_text(*value))
}

fn db_text(v: f32) -> String {
    if v <= 0.0001 {
        "-inf dB".into()
    } else {
        format!("{:+.1} dB", lyra_plugin::gain_to_db(v))
    }
}

/// The strip's meter: the standard one, standing beside the fader, its bars
/// as tall as the fader's travel.
fn meter(ui: &Ui, id: egui::Id, rect: Rect, fader: Rect, levels: [f32; 2]) {
    let (y0, y1) = (fader.top() + 10.0, fader.bottom() - 9.0);
    let well = Rect::from_min_max(pos2(rect.left() + 1.0, y0 - 4.0), pos2(rect.right() - 1.0, y1 + 4.0));
    widgets::meter(ui, id, well, &levels, true);
}

/// The selected track's effect rack: always eight slots, in signal order,
/// so every effect has a fixed place. Click a slot to open its effect; click
/// an empty slot to plug one in; drag a slot to move it; right-click for more.
fn effect_rack(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    let m = theme::m();
    let t = st.mixer_selected;
    let light = p.channels.iter().find(|c| c.mixer == t).map_or(theme::L_MINT, |c| theme::channel_light(c.color));
    ui.label(RichText::new(&p.mixer[t].name).size(m.text + 1.0).color(theme::TEXT));
    let feeding: Vec<&str> = p.channels.iter().filter(|c| c.mixer == t).map(|c| c.name.as_str()).collect();
    let from = match (t, feeding.is_empty()) {
        (0, _) => "everything ends here".to_string(),
        (_, true) => "nothing routed here yet".to_string(),
        _ => feeding.join(", "),
    };
    ui.add(egui::Label::new(RichText::new(from).small().color(theme::TEXT_DIM)).truncate());
    ui.add_space(6.0);

    let (pointer, released) = ui.input(|i| (i.pointer.hover_pos(), i.pointer.any_released()));
    let row_h = m.control_h + 4.0;
    let mut action: Option<SlotAction> = None;
    let mut drop_at: Option<usize> = None;
    for i in 0..lyra_engine::MAX_FX {
        let slot = p.mixer[t].effects.get(i).cloned();
        let (row, _) = ui.allocate_exact_size(vec2(RACK_W, row_h), Sense::hover());
        let painter = ui.painter().clone();
        // Slot number: the signal runs 1 → 8.
        painter.text(
            row.left_center() + vec2(7.0, 0.0),
            Align2::CENTER_CENTER,
            format!("{}", i + 1),
            egui::FontId::monospace(m.text_small),
            theme::TEXT_DIM,
        );
        let led_c = pos2(row.left() + 26.0, row.center().y);
        let plate = Rect::from_min_max(pos2(row.left() + 40.0, row.top() + 2.0), pos2(row.right(), row.bottom() - 2.0));
        let resp = ui.interact(plate, ui.id().with(("fx-slot", t, i)), Sense::click_and_drag());
        let open = st.effect == Some((t, i));

        match &slot {
            Some(s) => {
                let led_r = Rect::from_center_size(led_c, vec2(m.led, m.led));
                let led = ui.interact(led_r, ui.id().with(("fx-led", t, i)), Sense::click_and_drag());
                theme::led(&painter, led_c, m.led, (!s.bypass).then_some(light), led.hovered());
                match widgets::sweep(ui, led_r, "fx-bypass", !s.bypass) {
                    Some(widgets::Sweep::Begin(on)) => action = Some(SlotAction::Bypass(i, on, true)),
                    Some(widgets::Sweep::Continue(on)) => action = Some(SlotAction::Bypass(i, on, false)),
                    None => {}
                }
                led.hint("On / bypass");
                let name = lyra_catalog::effects::info(&s.effect).map_or("Missing effect", |e| e.name);
                let fill = if open || resp.hovered() {
                    theme::PANEL_HI
                } else {
                    theme::mix(theme::PANEL, theme::PANEL_HI, 0.5)
                };
                painter.rect_filled(plate, cr(3), fill);
                painter.rect_stroke(plate, cr(3), theme::stroke(1.0, theme::LINE), egui::StrokeKind::Inside);
                if open {
                    painter.rect_filled(Rect::from_min_size(plate.min, vec2(3.0, plate.height())), cr(1), light.mid);
                }
                painter.text(
                    plate.left_center() + vec2(10.0, 0.0),
                    Align2::LEFT_CENTER,
                    name,
                    egui::FontId::proportional(m.text),
                    if s.bypass { theme::TEXT_DIM } else { theme::TEXT },
                );
                if resp.clicked() {
                    action = Some(SlotAction::Open(i));
                }
                if resp.drag_started() {
                    st.fx_drag = Some(i);
                }
                resp.context_menu(|ui| {
                    if ui.button("Open").clicked() {
                        action = Some(SlotAction::Open(i));
                        ui.close_menu();
                    }
                    ui.menu_button("Replace with", |ui| {
                        if let Some(id) = effect_menu(ui) {
                            action = Some(SlotAction::Put(i, id));
                            ui.close_menu();
                        }
                    });
                    if i > 0 && ui.button("Move up").clicked() {
                        action = Some(SlotAction::Move(i, i - 1));
                        ui.close_menu();
                    }
                    if i + 1 < p.mixer[t].effects.len() && ui.button("Move down").clicked() {
                        action = Some(SlotAction::Move(i, i + 1));
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Remove").clicked() {
                        action = Some(SlotAction::Remove(i));
                        ui.close_menu();
                    }
                });
                resp.hint("Click: open · drag: move · right-click: more");
            }
            None => {
                theme::led(&painter, led_c, m.led * 0.7, None, false);
                theme::well(&painter, plate);
                if resp.hovered() {
                    painter.text(
                        plate.center(),
                        Align2::CENTER_CENTER,
                        "+",
                        egui::FontId::proportional(m.text + 2.0),
                        theme::TEXT_DIM,
                    );
                }
                let popup = ui.id().with(("fx-add", t, i));
                if resp.clicked() {
                    ui.memory_mut(|mem| mem.toggle_popup(popup));
                }
                egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                    ui.set_min_width(plate.width());
                    if let Some(id) = effect_menu(ui) {
                        action = Some(SlotAction::Put(i, id));
                        ui.memory_mut(|mem| mem.close_popup());
                    }
                });
            }
        }

        // Dragging a slot: show where it would land.
        if let (Some(from), Some(pos)) = (st.fx_drag, pointer) {
            if row.contains(pos) && from != i {
                let last = p.mixer[t].effects.len().saturating_sub(1);
                let to = i.min(last);
                let y = if i > last {
                    // Past the end: the line goes under the last effect (rows are evenly spaced).
                    plate.bottom() + 1.0 - (i - last) as f32 * (row_h + ui.spacing().item_spacing.y)
                } else if to > from {
                    plate.bottom() + 1.0
                } else {
                    plate.top() - 1.0
                };
                painter.hline(plate.x_range(), y, theme::stroke(2.0, light.mid));
                if released {
                    drop_at = Some(to);
                }
            }
        }
    }
    if released {
        if let (Some(from), Some(to)) = (st.fx_drag, drop_at) {
            action = Some(SlotAction::Move(from, to));
        }
        st.fx_drag = None;
    }

    let fx = &mut p.mixer[t].effects;
    match action {
        Some(SlotAction::Open(i)) => {
            st.effect = Some((t, i));
            st.raise = Some(crate::app::Focus::Effect);
        }
        Some(SlotAction::Bypass(i, on, begin)) => {
            if begin {
                h.checkpoint(p);
            }
            p.mixer[t].effects[i].bypass = !on;
        }
        Some(SlotAction::Put(i, id)) => {
            h.checkpoint(p);
            let fx = &mut p.mixer[t].effects;
            let slot = EffectSlot::new(id, lyra_catalog::effects::default_params(id));
            if i < fx.len() {
                fx[i] = slot;
            } else {
                fx.push(slot); // effects are packed: an empty slot means "add at the end"
            }
            st.effect = Some((t, i.min(fx.len() - 1)));
            st.raise = Some(crate::app::Focus::Effect);
        }
        Some(SlotAction::Move(from, to)) if from != to && from < fx.len() && to < fx.len() => {
            h.checkpoint(p);
            let fx = &mut p.mixer[t].effects;
            let e = fx.remove(from);
            fx.insert(to, e);
            // The open effect follows its slot.
            if let Some((et, ei)) = st.effect.filter(|&(et, _)| et == t) {
                let moved = if ei == from {
                    to
                } else if from < ei && ei <= to {
                    ei - 1
                } else if to <= ei && ei < from {
                    ei + 1
                } else {
                    ei
                };
                st.effect = Some((et, moved));
            }
        }
        Some(SlotAction::Remove(i)) => {
            h.checkpoint(p);
            p.mixer[t].effects.remove(i);
            st.effect = match st.effect {
                Some((et, ei)) if et == t && ei == i => None,
                Some((et, ei)) if et == t && ei > i => Some((et, ei - 1)),
                other => other,
            };
        }
        _ => {}
    }
}

enum SlotAction {
    Open(usize),
    /// (slot, turn on, first in the gesture)
    Bypass(usize, bool, bool),
    Put(usize, &'static str),
    Move(usize, usize),
    Remove(usize),
}

/// Every built-in effect, once. Returns the id picked.
fn effect_menu(ui: &mut Ui) -> Option<&'static str> {
    let mut picked = None;
    for e in lyra_catalog::effects::all() {
        if ui.button(e.name).clicked() {
            picked = Some(e.id);
        }
    }
    picked
}

/// One effect's controls, in a fixed grid (the effect window).
pub fn effect_editor(ui: &mut Ui, p: &mut Project, h: &mut History, t: usize, i: usize) {
    let m = theme::m();
    let Some(slot) = p.mixer[t].effects.get(i).cloned() else { return };
    let light = p.channels.iter().find(|c| c.mixer == t).map_or(theme::L_MINT, |c| theme::channel_light(c.color));
    ui.horizontal(|ui| {
        if widgets::led(ui, !slot.bypass, light, "On / bypass").clicked() {
            h.checkpoint(p);
            p.mixer[t].effects[i].bypass ^= true;
        }
        ui.label(RichText::new(format!("slot {}", i + 1)).small().color(theme::TEXT_DIM));
    });
    ui.add_space(4.0);
    let infos = lyra_catalog::effects::params(&slot.effect);
    let cell = vec2(m.knob_large + 30.0, m.knob_large + 2.0 * m.text_small + 14.0);
    egui::Grid::new(("fx-grid", t, i)).spacing(vec2(4.0, 6.0)).show(ui, |ui| {
        for (k, info) in infos.iter().enumerate() {
            let id = info.id as usize;
            let Some(&value) = p.mixer[t].effects[i].params.get(id) else { continue };
            ui.allocate_ui(cell, |ui| {
                ui.set_min_size(cell);
                ui.vertical_centered(|ui| {
                    // Where automation has it while the song plays, else its own value.
                    let addr = lyra_engine::Addr::Effect { track: t, slot: i, id: id as u32 };
                    let mut v = crate::automation::shown(addr).unwrap_or(value);
                    if tracked(&widgets::knob(ui, &mut v, info, m.knob_large, light.mid), h, p) {
                        p.mixer[t].effects[i].params[id] = v;
                    }
                    ui.label(RichText::new(info.name).small());
                    ui.label(RichText::new(info.format(v)).small().color(theme::TEXT_DIM));
                });
            });
            if k % 5 == 4 {
                ui.end_row();
            }
        }
    });
}
