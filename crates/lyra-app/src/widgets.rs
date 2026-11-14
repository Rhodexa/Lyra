//! Small custom-painted controls: knob, light, buttons with vector icons,
//! toolbar modules, meter, logo. All sizes come from `theme::m()`.
//!
//! Interaction conventions (used everywhere, so hands learn them once):
//! * drag up/down = change, **Shift** = fine
//! * mouse wheel = nudge
//! * double-click = reset to default

use crate::theme::{self, cr, stroke};
use egui::{pos2, vec2, Color32, Pos2, Rect, Response, RichText, Sense, Shape, StrokeKind, Ui};
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
    let n0 = info.to_normalized(*value);
    let mut n = n0;
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
    // Only convert back when the knob actually moved: the round trip through the
    // knob position isn't exact on exponential params (250 Hz came back as 250.00002),
    // which counted as an edit (and an undo step) just from being drawn.
    if n != n0 {
        *value = info.from_normalized(n);
    }
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
    let width = (diameter / 12.0).clamp(1.5, 3.0);
    let hot = resp.hovered() || resp.dragged();
    p.circle_filled(c, r - width - 1.0, if hot { theme::mix(theme::RAISED, color, 0.12) } else { theme::RAISED });
    p.add(Shape::line(arc(c, r - width * 0.5, KNOB_START, KNOB_START + KNOB_SWEEP), stroke(width, theme::LINE)));
    let (a0, a1) = if info.is_bipolar() {
        let mid = info.to_normalized(0.0);
        (KNOB_START + KNOB_SWEEP * mid, KNOB_START + KNOB_SWEEP * n)
    } else {
        (KNOB_START, KNOB_START + KNOB_SWEEP * n)
    };
    if (a1 - a0).abs() > 0.01 {
        p.add(Shape::line(arc(c, r - width * 0.5, a0.min(a1), a0.max(a1)), stroke(width, color)));
    }
    let a = KNOB_START + KNOB_SWEEP * n;
    let dir = vec2(a.cos(), a.sin());
    p.line_segment([c + dir * (r * 0.2), c + dir * (r - width - 2.5)], stroke(width, theme::TEXT));

    resp.on_hover_text(format!("{}: {}", info.name, info.format(*value)))
}

/// A round status light. `on` glows in `color`.
pub fn led(ui: &mut Ui, on: bool, color: Color32, hint: &str) -> Response {
    // click_and_drag: a drag that starts on a light must sweep lights, not move the window.
    let size = theme::m().led;
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let p = ui.painter();
    let c = rect.center();
    let r = size * 0.27;
    if on {
        p.circle_filled(c, r * 1.6, color.gamma_multiply(0.18));
        p.circle_filled(c, r, color);
    } else {
        p.circle_filled(c, r, theme::INSET);
        p.circle_stroke(c, r, stroke(1.0, theme::LINE));
    }
    resp.on_hover_text(hint)
}

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Stop,
    Prev,
    Next,
    Plus,
    Minus,
    Clone,
    Rack,
    Piano,
    Knob,
    Playlist,
    Mixer,
}

/// A captioned, contained group for the toolbar ("module"). Every module has
/// the same height, so toolbars line up no matter what's inside.
pub fn module<R>(ui: &mut Ui, caption: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let m = theme::m();
    egui::Frame::default()
        .fill(theme::INSET)
        .stroke(stroke(1.0, theme::LINE))
        .corner_radius(cr(m.corner as u8))
        .inner_margin(egui::Margin::symmetric(m.module_pad as i8, (m.module_pad * 0.5) as i8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.vertical(|ui| {
                ui.label(RichText::new(caption.to_uppercase()).size(m.text_small - 1.5).color(theme::TEXT_DIM));
                ui.horizontal(|ui| {
                    ui.set_height(m.control_h);
                    add(ui)
                })
                .inner
            })
            .inner
        })
        .inner
}

/// A square icon button, exactly `control_h` tall so it lines up with menus and number boxes.
pub fn icon_button(ui: &mut Ui, icon: Icon, active: bool, hint: &str) -> Response {
    let h = theme::m().control_h;
    let (rect, resp) = ui.allocate_exact_size(vec2(h, h), Sense::click());
    paint_button(ui, rect, &resp, icon, active);
    resp.on_hover_text(hint)
}

/// A wider icon button for play/stop.
pub fn transport_button(ui: &mut Ui, icon: Icon, active: bool, hint: &str) -> Response {
    let h = theme::m().control_h;
    let (rect, resp) = ui.allocate_exact_size(vec2(h * 1.4, h), Sense::click());
    paint_button(ui, rect, &resp, icon, active);
    resp.on_hover_text(hint)
}

fn paint_button(ui: &Ui, rect: Rect, resp: &Response, icon: Icon, active: bool) {
    let p = ui.painter();
    let fill = if active {
        theme::mix(theme::RAISED, theme::MINT, 0.22)
    } else if resp.hovered() {
        theme::mix(theme::RAISED, theme::MINT, 0.08)
    } else {
        theme::RAISED
    };
    p.rect_filled(rect, cr(4), fill);
    p.rect_stroke(rect, cr(4), stroke(1.0, theme::LINE), StrokeKind::Inside);
    let ink = if active {
        theme::MINT
    } else if resp.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    let c = rect.center();
    let u = rect.height() / 22.0; // icons are drawn on a 22-unit grid
    let line = stroke(1.4 * u.max(1.0), ink);
    let tri = |pts: [Pos2; 3]| Shape::convex_polygon(pts.to_vec(), ink, egui::Stroke::NONE);
    match icon {
        Icon::Play => {
            p.add(tri([c + vec2(-4.0, -5.5) * u, c + vec2(6.0, 0.0) * u, c + vec2(-4.0, 5.5) * u]));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(9.0, 9.0) * u), cr(1), ink);
        }
        Icon::Prev => {
            p.add(tri([c + vec2(3.0, -5.0) * u, c + vec2(-4.0, 0.0) * u, c + vec2(3.0, 5.0) * u]));
        }
        Icon::Next => {
            p.add(tri([c + vec2(-3.0, -5.0) * u, c + vec2(4.0, 0.0) * u, c + vec2(-3.0, 5.0) * u]));
        }
        Icon::Plus | Icon::Minus => {
            p.line_segment([c - vec2(5.0, 0.0) * u, c + vec2(5.0, 0.0) * u], line);
            if matches!(icon, Icon::Plus) {
                p.line_segment([c - vec2(0.0, 5.0) * u, c + vec2(0.0, 5.0) * u], line);
            }
        }
        Icon::Clone => {
            let a = Rect::from_center_size(c + vec2(-1.5, -1.5) * u, vec2(8.0, 8.0) * u);
            let b = a.translate(vec2(3.5, 3.5) * u);
            p.rect_stroke(a, cr(1), line, StrokeKind::Middle);
            p.rect_filled(b, cr(1), fill);
            p.rect_stroke(b, cr(1), line, StrokeKind::Middle);
        }
        Icon::Rack => {
            for row in 0..3 {
                for col in 0..3 {
                    let at = c + vec2(col as f32 - 1.0, row as f32 - 1.0) * 4.0 * u;
                    let r = Rect::from_center_size(at, vec2(3.0, 3.0) * u);
                    if (row + col) % 2 == 0 {
                        p.rect_filled(r, cr(0), ink);
                    } else {
                        p.rect_stroke(r, cr(0), stroke(1.0, ink), StrokeKind::Inside);
                    }
                }
            }
        }
        Icon::Piano => {
            let body = Rect::from_center_size(c, vec2(13.0, 10.0) * u);
            p.rect_stroke(body, cr(1), line, StrokeKind::Middle);
            for x in [-2.2, 2.2] {
                let key = Rect::from_min_size(pos2(c.x + (x - 1.0) * u, body.top()), vec2(2.0, 6.0) * u);
                p.rect_filled(key, cr(0), ink);
            }
        }
        Icon::Knob => {
            p.circle_stroke(c, 5.5 * u, line);
            p.line_segment([c, c + vec2(3.0, -3.5) * u], line);
        }
        Icon::Mixer => {
            // Three faders at different heights.
            for (x, cap) in [(-4.5, -2.0), (0.0, 3.0), (4.5, -4.5)] {
                p.line_segment([c + vec2(x, -6.0) * u, c + vec2(x, 6.0) * u], stroke(1.0, ink));
                p.rect_filled(Rect::from_center_size(c + vec2(x, cap) * u, vec2(3.5, 2.5) * u), cr(1), ink);
            }
        }
        Icon::Playlist => {
            // Three tracks with clips on them.
            for (row, (x0, x1)) in [(-4.0, (-6.0, 1.0)), (0.0, (-2.0, 6.0)), (4.0, (-6.0, -1.0))] {
                let r = Rect::from_min_max(c + vec2(x0, row - 1.3) * u, c + vec2(x1, row + 1.3) * u);
                p.rect_filled(r, cr(1), ink);
            }
        }
    }
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

/// A tiny keyboard showing which notes the typing keyboard plays right now:
/// four octaves from C1, with the playable range lit. Clicking left/right of
/// the middle shifts the octave (returned as -1 / +1).
pub fn keyboard_map(ui: &mut Ui, first_key: u8, last_key: u8) -> Option<i32> {
    let m = theme::m();
    let (lo, hi) = (24u8, 96u8); // C1 .. C7
    let white: Vec<u8> = (lo..hi).filter(|k| !lyra_model::theory::is_black(*k)).collect();
    let key_w = 4.0;
    let size = vec2(white.len() as f32 * key_w, m.control_h);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, cr(2), theme::KEY_BLACK);
    let lit = |k: u8| (first_key..=last_key).contains(&k);
    for (i, &k) in white.iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.left() + i as f32 * key_w, rect.top()), vec2(key_w - 1.0, rect.height()));
        p.rect_filled(r, cr(0), if lit(k) { theme::MINT } else { theme::KEY_WHITE.gamma_multiply(0.55) });
    }
    for k in (lo..hi).filter(|k| lyra_model::theory::is_black(*k)) {
        let i = white.iter().position(|&w| w > k).unwrap_or(0);
        let x = rect.left() + i as f32 * key_w - 1.5;
        let r = Rect::from_min_size(pos2(x, rect.top()), vec2(2.5, rect.height() * 0.6));
        p.rect_filled(r, cr(0), if lit(k) { theme::MINT_DEEP } else { theme::KEY_BLACK });
    }
    let resp =
        resp.on_hover_text("Where your typing keyboard plays (Z…P rows). Click left / right to shift an octave.");
    if resp.clicked() {
        let x = resp.interact_pointer_pos().map_or(rect.center().x, |p| p.x);
        return Some(if x < rect.center().x { -1 } else { 1 });
    }
    None
}
