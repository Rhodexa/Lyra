//! Small custom-painted controls: knob, light, buttons with vector icons,
//! toolbar modules, meter, logo. All sizes come from `theme::m()`.
//!
//! Interaction conventions (used everywhere, so hands learn them once):
//! * drag up/down = change, **Shift** = fine
//! * mouse wheel = nudge
//! * double-click = reset to default

use crate::theme::{self, cr, stroke};
use egui::{pos2, vec2, Color32, Pos2, Rect, Response, Sense, Shape, Ui};
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

    let hot = resp.hovered() || resp.dragged();
    paint_knob(ui.painter(), rect, info, *value, color, hot);
    resp.on_hover_text(format!("{}: {}", info.name, info.format(*value)))
}

/// The knob families from the control library, by size.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum KnobStyle {
    /// Pan-knob style: a dark well with an arc on its rim and a dot pointer.
    Pan,
    /// Tiny, flat: a disc with a dot.
    Tiny,
    /// Normal, rubber: a ringed cap with a line pointer.
    Rubber,
    /// Large, polymer: a ringed body with a fixed four-quadrant light and a
    /// rotating pill pointer.
    Polymer,
}

impl KnobStyle {
    pub fn for_size(diameter: f32, bipolar: bool) -> Self {
        match diameter {
            d if d < 34.0 && bipolar => KnobStyle::Pan,
            d if d < 34.0 => KnobStyle::Tiny,
            d if d < 62.0 => KnobStyle::Rubber,
            _ => KnobStyle::Polymer,
        }
    }
}

fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Paint a knob into `rect` for `value`. Angles sweep 270° (7:30 → 4:30);
/// bipolar params light the arc from the centre, choice params show dots.
pub fn paint_knob(p: &egui::Painter, rect: Rect, info: &ParamInfo, value: f32, color: Color32, hot: bool) {
    let n = info.to_normalized(value);
    let bipolar = info.is_bipolar();
    let style = KnobStyle::for_size(rect.width().min(rect.height()), bipolar);
    let c = rect.center();
    // The studies draw on a 300-unit box (the pan knob on 100, scaled to match).
    let u = rect.width().min(rect.height()) / 300.0;
    let lift = |col: Color32| if hot { theme::shade(col, 1.08) } else { col };
    let angle = KNOB_START + KNOB_SWEEP * n;
    let dir = vec2(angle.cos(), angle.sin());
    let (arc_r, arc_w, track) = match style {
        KnobStyle::Pan => (133.5, 15.0, hex(0x1c2929)),
        KnobStyle::Tiny => (119.0, 6.0, hex(0x1a2526)),
        KnobStyle::Rubber => (124.0, 6.0, hex(0x1a2526)),
        KnobStyle::Polymer => (125.0, 7.0, hex(0x171e21)),
    };
    let (arc_r, arc_w) = (arc_r * u, (arc_w * u).max(1.6));

    // Body.
    match style {
        KnobStyle::Pan => {
            p.circle_filled(c, 132.0 * u, hex(0x141b1d));
            p.circle_filled(c, 117.0 * u, lift(hex(0x3e4849)));
            p.circle_filled(c, 99.0 * u, lift(hex(0x465051)));
            p.circle_filled(c, 84.0 * u, lift(hex(0x414a4b)));
        }
        KnobStyle::Tiny => {
            p.circle_filled(c, 94.0 * u, lift(hex(0x30383b)));
            p.circle_filled(c, 85.0 * u, lift(hex(0x343d3f)));
        }
        KnobStyle::Rubber => {
            p.circle_filled(c, 108.0 * u, hex(0x161d20));
            p.circle_filled(c, 98.5 * u, lift(hex(0x596262)));
            p.circle_filled(c, 93.5 * u, lift(hex(0x394143)));
            p.circle_filled(c, 83.0 * u, lift(hex(0x3b4345)));
        }
        KnobStyle::Polymer => {
            p.circle_filled(c, 110.0 * u, hex(0x171d20));
            p.circle_filled(c, 106.0 * u, lift(hex(0x30383b)));
            p.circle_filled(c, 101.0 * u, hex(0x171d20));
            p.circle_filled(c, 96.0 * u, lift(hex(0x353d3f)));
            // The fixed light: four quadrants, lighter top-right and bottom-left.
            let r = 88.0 * u;
            for (q, col) in [(2, 0x41494b), (3, 0x474f51), (1, 0x474f51), (0, 0x41494b)] {
                let from = q as f32 * 0.5 * PI;
                let mut pts = vec![c];
                pts.extend(arc(c, r, from, from + 0.5 * PI));
                p.add(Shape::convex_polygon(pts, lift(hex(col)), egui::Stroke::NONE));
            }
        }
    }

    // Scale: dots for choices, otherwise a recessed track with the lit arc.
    let choices = info.labels.len();
    if (2..=12).contains(&choices) {
        let at = info.to_normalized(value.round());
        for i in 0..choices {
            let t = i as f32 / (choices - 1) as f32;
            let a = KNOB_START + KNOB_SWEEP * t;
            let on = (t - at).abs() < 1e-3;
            let r = if on { 7.0 } else { 5.0 } * u;
            p.circle_filled(
                c + vec2(a.cos(), a.sin()) * arc_r,
                r.max(if on { 1.8 } else { 1.2 }),
                if on { color } else { hex(0x26312f) },
            );
        }
    } else {
        p.add(Shape::line(arc(c, arc_r, KNOB_START, KNOB_START + KNOB_SWEEP), stroke(arc_w, track)));
        let (a0, a1) = if bipolar {
            let mid = KNOB_START + KNOB_SWEEP * info.to_normalized(0.0);
            (mid.min(angle), mid.max(angle))
        } else {
            (KNOB_START, angle)
        };
        if a1 - a0 > 0.02 {
            let mut pts = arc(c, arc_r, a0, a1);
            // Round caps, like the studies' stroke-linecap.
            p.circle_filled(pts[0], arc_w * 0.5, color);
            p.circle_filled(*pts.last().unwrap(), arc_w * 0.5, color);
            pts.shrink_to_fit();
            p.add(Shape::line(pts, stroke(arc_w, color)));
        }
        if bipolar {
            let mid = KNOB_START + KNOB_SWEEP * info.to_normalized(0.0);
            let tick = c + vec2(mid.cos(), mid.sin()) * arc_r;
            p.circle_filled(tick, (8.4 * u).max(1.2), hex(0x91a79e));
        }
    }

    // Pointer.
    match style {
        KnobStyle::Pan => {
            p.circle_filled(c + dir * 93.0 * u, (12.6 * u).max(1.6), hex(0xa6c9b8));
        }
        KnobStyle::Tiny => {
            p.circle_filled(c + dir * 61.0 * u, (7.0 * u).max(1.5), color);
        }
        KnobStyle::Rubber => {
            p.line_segment([c + dir * 47.0 * u, c + dir * 66.0 * u], stroke((8.0 * u).max(1.5), color));
            for end in [47.0, 66.0] {
                p.circle_filled(c + dir * end * u, (4.0 * u).max(0.75), color);
            }
        }
        KnobStyle::Polymer => {
            let w = 10.0 * u;
            let (a, b) = (c + dir * (58.0 * u + w * 0.5), c + dir * (90.0 * u - w * 0.5));
            p.line_segment([a, b], stroke(w, lift(hex(0xa3aca5))));
            p.circle_filled(a, w * 0.5, lift(hex(0xa3aca5)));
            p.circle_filled(b, w * 0.5, lift(hex(0xa3aca5)));
        }
    }
}

/// A round mute/status light in `light`'s colour when on.
pub fn led(ui: &mut Ui, on: bool, light: theme::Light, hint: &str) -> Response {
    // click_and_drag: a drag that starts on a light must sweep lights, not move the window.
    let size = theme::m().led;
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    theme::led(ui.painter(), rect.center(), size, on.then_some(light), resp.hovered());
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

impl Icon {
    /// The Phosphor glyph, and whether it uses the solid weight.
    fn glyph(self) -> (&'static str, bool) {
        use egui_phosphor::bold as ph;
        match self {
            Icon::Play => (ph::PLAY, true),
            Icon::Stop => (ph::STOP, true),
            Icon::Prev => (ph::CARET_LEFT, false),
            Icon::Next => (ph::CARET_RIGHT, false),
            Icon::Plus => (ph::PLUS, false),
            Icon::Minus => (ph::MINUS, false),
            Icon::Clone => (ph::COPY, false),
            Icon::Rack => (ph::DOTS_NINE, false),
            Icon::Piano => (ph::PIANO_KEYS, false),
            Icon::Knob => (ph::WAVE_SINE, false),
            Icon::Playlist => (ph::PLAYLIST, false),
            Icon::Mixer => (ph::FADERS, false),
        }
    }
}

/// A toolbar module: a flat panel mounted in the dark chassis. No caption:
/// what's inside says what it is (hover anything for its name and keys).
pub fn module<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let m = theme::m();
    egui::Frame::default()
        .fill(theme::PANEL)
        .corner_radius(cr(3))
        .inner_margin(egui::Margin::symmetric(m.module_pad as i8, m.module_pad as i8))
        .show(ui, |ui| {
            ui.set_height(toolbar_key());
            ui.horizontal_centered(add).inner
        })
        .inner
}

/// The big toolbar key: as tall as a module's inside.
pub fn toolbar_key() -> f32 {
    let m = theme::m();
    (m.toolbar_h - 2.0 * m.module_pad - 10.0).max(m.control_h)
}

/// What's printed on a key.
#[derive(Clone, Copy)]
pub enum Legend<'a> {
    Icon(Icon),
    Text(&'a str),
}

/// A silicone key that lights in `light` when `on`.
pub fn key(ui: &mut Ui, size: egui::Vec2, legend: Legend, on: bool, light: theme::Light, hint: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let pressed = resp.is_pointer_button_down_on();
    theme::key(p, rect, on.then_some(light), resp.hovered() && !pressed, if pressed { 0.9 } else { 1.0 });
    let face = theme::key_face(rect);
    let ink = theme::key_ink(on);
    match legend {
        Legend::Icon(icon) => {
            let (glyph, solid) = icon.glyph();
            let size = (face.height() * 0.62).round();
            p.text(face.center(), egui::Align2::CENTER_CENTER, glyph, theme::icon_font(size, solid), ink);
        }
        Legend::Text(t) => {
            let size = (face.height() * 0.42).clamp(9.0, 13.0);
            p.text(face.center(), egui::Align2::CENTER_CENTER, t, egui::FontId::monospace(size), ink);
        }
    }
    resp.on_hover_text(hint)
}

/// A square icon key, `control_h` big, lit mint when `active`.
pub fn icon_button(ui: &mut Ui, icon: Icon, active: bool, hint: &str) -> Response {
    let h = theme::m().control_h;
    key(ui, vec2(h, h), Legend::Icon(icon), active, theme::L_MINT, hint)
}

/// A number on a backlit LCD. Hover a digit to pick how fine you tune, then
/// drag up/down or use the wheel; double-click to type a value.
#[allow(clippy::too_many_arguments)]
pub fn lcd_number(
    ui: &mut Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    caption: &str,
    size: egui::Vec2,
    hint: &str,
) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let id = resp.id;
    let before = *value;
    let digit_size = (size.y * 0.5).round();
    let font = egui::FontId::monospace(digit_size);
    let text = format!("{:.*}", decimals, *value);
    let char_w = ui.fonts(|f| f.glyph_width(&font, '0'));
    let text_w = char_w * text.chars().count() as f32;
    let text_center = rect.center() - vec2(0.0, size.y * 0.1);
    let x0 = text_center.x - text_w * 0.5;

    // Which decimal place the pointer is over (10^place), kept during a drag.
    let place_id = id.with("place");
    let place_of = |x: f32| -> i32 {
        let i = (((x - x0) / char_w).floor() as i32).clamp(0, text.len() as i32 - 1) as usize;
        let dot = text.find('.').unwrap_or(text.len());
        let i = if i == dot { i + 1 } else { i }.min(text.len() - 1);
        if i < dot {
            (dot - i - 1) as i32
        } else {
            -((i - dot) as i32)
        }
    };
    let mut place: i32 = ui.data(|d| d.get_temp(place_id)).unwrap_or(0);
    if !resp.dragged() {
        if let Some(pos) = resp.hover_pos() {
            place = place_of(pos.x);
        }
    }
    ui.data_mut(|d| d.insert_temp(place_id, place));
    let step = 10f64.powi(place);

    let editing_id = id.with("edit");
    let mut editing: Option<String> = ui.data(|d| d.get_temp(editing_id));
    if resp.double_clicked() {
        editing = Some(text.clone());
    }
    if resp.dragged() {
        let acc_id = id.with("acc");
        let mut acc: f32 = ui.data(|d| d.get_temp(acc_id)).unwrap_or(0.0);
        acc -= resp.drag_delta().y / 8.0;
        let n = acc.trunc();
        acc -= n;
        ui.data_mut(|d| d.insert_temp(acc_id, acc));
        *value += n as f64 * step;
    }
    if resp.hovered() {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            *value += wheel.signum() as f64 * step;
        }
    }

    let p = ui.painter().clone();
    theme::lcd_panel(&p, rect);
    if let Some(mut buf) = editing.take() {
        let edit = egui::TextEdit::singleline(&mut buf)
            .font(font.clone())
            .text_color(theme::LCD_INK)
            .frame(false)
            .horizontal_align(egui::Align::Center);
        let inner = Rect::from_center_size(text_center, vec2(size.x - 8.0, digit_size + 4.0));
        let r = ui.put(inner, edit);
        r.request_focus();
        let done = ui.input(|i| i.key_pressed(egui::Key::Enter));
        if done || r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                if let Ok(v) = buf.trim().parse::<f64>() {
                    *value = v;
                }
            }
        } else {
            editing = Some(buf);
        }
    } else {
        p.text(text_center, egui::Align2::CENTER_CENTER, &text, font, theme::LCD_INK);
        // Underline the digit being tuned.
        if resp.hovered() || resp.dragged() {
            let dot = text.find('.').unwrap_or(text.len()) as i32;
            let i = if place >= 0 { dot - 1 - place } else { dot - place };
            let x = x0 + i as f32 * char_w;
            let y = text_center.y + digit_size * 0.55;
            p.hline(x + 1.0..=x + char_w - 1.0, y, stroke(1.5, theme::LCD_INK_DIM));
        }
    }
    ui.data_mut(|d| match &editing {
        Some(b) => d.insert_temp(editing_id, b.clone()),
        None => d.remove::<String>(editing_id),
    });
    p.text(
        rect.center_bottom() - vec2(0.0, size.y * 0.15),
        egui::Align2::CENTER_CENTER,
        caption,
        egui::FontId::proportional((size.y * 0.2).clamp(8.0, 10.0)),
        theme::LCD_INK_DIM,
    );

    let factor = 10f64.powi(decimals as i32);
    *value = ((*value * factor).round() / factor).clamp(*range.start(), *range.end());
    if *value != before {
        resp.mark_changed();
    }
    resp.on_hover_text(hint)
}

/// A small read-only LCD (song position and such).
pub fn lcd_text(ui: &mut Ui, text: &str, caption: &str, size: egui::Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    theme::lcd_panel(p, rect);
    let digit = (size.y * 0.42).round();
    p.text(
        rect.center() - vec2(0.0, size.y * 0.1),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::monospace(digit),
        theme::LCD_INK,
    );
    p.text(
        rect.center_bottom() - vec2(0.0, size.y * 0.15),
        egui::Align2::CENTER_CENTER,
        caption,
        egui::FontId::proportional((size.y * 0.2).clamp(8.0, 10.0)),
        theme::LCD_INK_DIM,
    );
    resp
}

/// The meter's fixed full-scale colour: green, lime, amber, rose at the top.
pub const METER_STOPS: [(f32, Color32); 4] = [
    (0.0, Color32::from_rgb(0x9f, 0xe4, 0xa9)),
    (0.7, Color32::from_rgb(0xc8, 0xe5, 0xa3)),
    (0.86, Color32::from_rgb(0xf0, 0xd2, 0x8b)),
    (1.0, Color32::from_rgb(0xee, 0x99, 0x96)),
];

/// Meter level (linear amplitude) to bar fraction: -60 dB .. 0 dB, linear in dB.
pub fn meter_fraction(level: f32) -> f32 {
    let db = 20.0 * level.max(1e-6).log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

/// A bar of the fixed meter gradient lit up to `frac`, horizontal (left → right)
/// or vertical (bottom → top). Each colour stays put; the light only reveals it.
pub fn meter_bar(p: &egui::Painter, r: Rect, frac: f32, vertical: bool) {
    p.rect_filled(r, cr(0), Color32::from_rgb(0x20, 0x2b, 0x2b));
    if frac <= 0.0 {
        return;
    }
    let mut ts: Vec<f32> = METER_STOPS.iter().map(|s| s.0).filter(|t| *t < frac).collect();
    ts.push(frac);
    let mut mesh = egui::Mesh::default();
    for &t in &ts {
        let c = theme::ramp(&METER_STOPS, t);
        if vertical {
            let y = r.bottom() - t * r.height();
            mesh.colored_vertex(pos2(r.left(), y), c);
            mesh.colored_vertex(pos2(r.right(), y), c);
        } else {
            let x = r.left() + t * r.width();
            mesh.colored_vertex(pos2(x, r.top()), c);
            mesh.colored_vertex(pos2(x, r.bottom()), c);
        }
    }
    for i in 0..ts.len().saturating_sub(1) as u32 {
        let (a, b) = (2 * i, 2 * i + 2);
        mesh.add_triangle(a, a + 1, b + 1);
        mesh.add_triangle(a, b + 1, b);
    }
    p.add(Shape::mesh(mesh));
}

/// A horizontal stereo meter in its well (`levels` are linear amplitudes).
pub fn meter_stereo_h(p: &egui::Painter, r: Rect, levels: [f32; 2]) {
    p.rect_filled(r, cr(2), Color32::from_rgb(0x10, 0x17, 0x19));
    let inner = r.shrink(2.0);
    let h = (inner.height() - 2.0) / 2.0;
    for (k, level) in levels.into_iter().enumerate() {
        let bar = Rect::from_min_size(pos2(inner.left(), inner.top() + k as f32 * (h + 2.0)), vec2(inner.width(), h));
        meter_bar(p, bar, meter_fraction(level), false);
    }
}

/// A tiny keyboard showing which notes the typing keyboard plays right now:
/// four octaves from C1, with the playable range lit. Clicking left/right of
/// the middle shifts the octave (returned as -1 / +1).
pub fn keyboard_map(ui: &mut Ui, first_key: u8, last_key: u8) -> Option<i32> {
    let (lo, hi) = (24u8, 96u8); // C1 .. C7
    let white: Vec<u8> = (lo..hi).filter(|k| !lyra_model::theory::is_black(*k)).collect();
    let key_w = 4.0;
    let size = vec2(white.len() as f32 * key_w + 6.0, toolbar_key() * 0.8);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    theme::well(p, rect);
    let keys = rect.shrink2(vec2(3.0, 4.0));
    let lit = |k: u8| (first_key..=last_key).contains(&k);
    for (i, &k) in white.iter().enumerate() {
        let r = Rect::from_min_size(pos2(keys.left() + i as f32 * key_w, keys.top()), vec2(key_w - 1.0, keys.height()));
        let c = if lit(k) { theme::L_MINT.low } else { Color32::from_rgb(0x3e, 0x48, 0x49) };
        p.rect_filled(r, cr(1), c);
    }
    for k in (lo..hi).filter(|k| lyra_model::theory::is_black(*k)) {
        let i = white.iter().position(|&w| w > k).unwrap_or(0);
        let x = keys.left() + i as f32 * key_w - 1.5;
        let r = Rect::from_min_size(pos2(x, keys.top()), vec2(2.5, keys.height() * 0.6));
        p.rect_filled(r, cr(0), if lit(k) { theme::L_MINT.rim } else { theme::WELL });
    }
    let resp =
        resp.on_hover_text("Where your typing keyboard plays (Z…P rows). Click left / right to shift an octave.");
    if resp.clicked() {
        let x = resp.interact_pointer_pos().map_or(rect.center().x, |p| p.x);
        return Some(if x < rect.center().x { -1 } else { 1 });
    }
    None
}
