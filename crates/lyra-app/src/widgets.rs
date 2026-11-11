//! Small custom-painted controls: knob, LED, transport button, meter.
//!
//! Interaction conventions (used everywhere, so hands learn them once):
//! * drag up/down = change, **Shift** = fine
//! * mouse wheel = nudge
//! * double-click = reset to default

use crate::theme::{self, cr};
use egui::{pos2, vec2, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui};
use lyra_plugin::ParamInfo;
use std::f32::consts::PI;

const KNOB_START: f32 = 0.75 * PI; // 7:30 o'clock
const KNOB_SWEEP: f32 = 1.5 * PI;

fn arc(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let segments = ((to - from).abs() / 0.15).ceil().max(2.0) as usize;
    (0..=segments)
        .map(|i| {
            let a = from + (to - from) * i as f32 / segments as f32;
            center + vec2(a.cos(), a.sin()) * radius
        })
        .collect()
}

/// A knob for any plugin parameter. Returns a response that is `changed()`
/// when the value moved; check `drag_started()` to take an undo checkpoint.
pub fn knob(ui: &mut Ui, value: &mut f32, info: &ParamInfo, diameter: f32, color: Color32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(diameter, diameter), Sense::click_and_drag());
    let mut n = info.to_normalized(*value);
    let before = *value;

    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        n -= resp.drag_delta().y * if fine { 0.0015 } else { 0.006 };
    }
    if resp.hovered() {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            n += wheel.signum() * 0.02;
        }
    }
    *value = info.from_normalized(n);
    if resp.double_clicked() {
        *value = info.default;
    }
    if *value != before {
        resp.mark_changed();
    }

    // Paint
    let n = info.to_normalized(*value);
    let p = ui.painter();
    let c = rect.center();
    let r = diameter * 0.5;
    let hot = resp.hovered() || resp.dragged();
    p.circle_filled(c, r - 2.5, if hot { theme::mix(theme::RAISED, color, 0.12) } else { theme::RAISED });
    p.add(Shape::line(arc(c, r - 1.0, KNOB_START, KNOB_START + KNOB_SWEEP), theme::stroke(1.5, theme::LINE)));
    let (a0, a1) = if info.is_bipolar() {
        let mid = info.to_normalized(0.0);
        (KNOB_START + KNOB_SWEEP * mid, KNOB_START + KNOB_SWEEP * n)
    } else {
        (KNOB_START, KNOB_START + KNOB_SWEEP * n)
    };
    if (a1 - a0).abs() > 0.01 {
        p.add(Shape::line(arc(c, r - 1.0, a0.min(a1), a0.max(a1)), theme::stroke(1.5, color)));
    }
    let a = KNOB_START + KNOB_SWEEP * n;
    let dir = vec2(a.cos(), a.sin());
    p.line_segment([c + dir * (r * 0.2), c + dir * (r - 3.5)], theme::stroke(1.5, theme::TEXT));

    resp.on_hover_text(format!("{}: {}", info.name, info.format(*value)))
}

/// A round status light. `on` glows in `color`.
pub fn led(ui: &mut Ui, on: bool, color: Color32, hint: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::click());
    let p = ui.painter();
    let c = rect.center();
    if on {
        p.circle_filled(c, 5.5, color.gamma_multiply(0.18));
        p.circle_filled(c, 3.5, color);
    } else {
        p.circle_filled(c, 3.5, theme::INSET);
        p.circle_stroke(c, 3.5, theme::stroke(1.0, theme::LINE));
    }
    resp.on_hover_text(hint)
}

pub enum Icon {
    Play,
    Stop,
}

pub fn transport_button(ui: &mut Ui, icon: Icon, active: bool, hint: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 22.0), Sense::click());
    let p = ui.painter();
    let fill = if active {
        theme::mix(theme::RAISED, theme::MINT, 0.22)
    } else if resp.hovered() {
        theme::mix(theme::RAISED, theme::MINT, 0.08)
    } else {
        theme::RAISED
    };
    p.rect_filled(rect, cr(3), fill);
    let ink = if active { theme::MINT } else if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
    let c = rect.center();
    match icon {
        Icon::Play => {
            let pts = vec![c + vec2(-4.0, -5.5), c + vec2(6.0, 0.0), c + vec2(-4.0, 5.5)];
            p.add(Shape::convex_polygon(pts, ink, Stroke::NONE));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(9.0, 9.0)), cr(1), ink);
        }
    }
    resp.on_hover_text(hint)
}

/// Horizontal segmented meter, mint → butter → rose. `level` is linear amplitude.
pub fn meter(p: &egui::Painter, rect: Rect, level: f32) {
    p.rect_filled(rect, cr(1), theme::INSET);
    let segments = ((rect.width() / 4.0) as usize).max(1);
    // Show roughly -48 dB .. 0 dB.
    let db = 20.0 * level.max(1e-6).log10();
    let lit = ((db + 48.0) / 48.0).clamp(0.0, 1.0);
    for i in 0..segments {
        let t = i as f32 / segments as f32;
        if t >= lit {
            break;
        }
        let color = if t > 0.92 {
            theme::ROSE
        } else if t > 0.75 {
            theme::BUTTER
        } else {
            theme::MINT
        };
        let x = rect.min.x + i as f32 * 4.0;
        p.rect_filled(
            Rect::from_min_max(pos2(x, rect.min.y), pos2((x + 3.0).min(rect.max.x), rect.max.y)),
            cr(0),
            color,
        );
    }
}

/// The Lyra mark (assets/icon.png, pre-scaled — see theme::Textures).
pub fn logo(ui: &mut Ui, texture: egui::TextureId) {
    let (rect, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    ui.painter().image(texture, rect, uv, Color32::WHITE);
}
