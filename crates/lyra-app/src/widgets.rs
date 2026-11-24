//! Small custom-painted controls: knob, light, buttons with vector icons,
//! toolbar modules, meter, logo. All sizes come from `theme::m()`.
//!
//! Interaction conventions (used everywhere, so hands learn them once):
//! * drag up/down = change, **Shift** = fine
//! * mouse wheel = nudge
//! * double-click = reset to default

use crate::theme::{self, cr, hex, stroke};
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
    let (rect, resp) = ui.allocate_exact_size(vec2(diameter, diameter), Sense::click_and_drag());
    knob_with(ui, resp, rect, value, info, color)
}

/// A knob in a rect you already have (for custom-painted panels); `id` must be unique.
pub fn knob_at(ui: &mut Ui, id: egui::Id, rect: Rect, value: &mut f32, info: &ParamInfo, color: Color32) -> Response {
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    knob_with(ui, resp, rect, value, info, color)
}

/// A knob that feels exactly like [`knob`] but is drawn by `paint(painter,
/// rect, value, hot)`, with no tooltip: for instruments with their own look
/// (Chrysalis), which show the value themselves.
pub fn knob_painted(
    ui: &mut Ui,
    value: &mut f32,
    info: &ParamInfo,
    diameter: f32,
    paint: impl FnOnce(&egui::Painter, Rect, f32, bool),
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(diameter, diameter), Sense::click_and_drag());
    let (resp, hot) = knob_turn(ui, resp, value, info);
    paint(ui.painter(), rect, *value, hot);
    // No tooltip: an editor with its own look shows the value its own way.
    resp
}

fn knob_with(ui: &mut Ui, resp: Response, rect: Rect, value: &mut f32, info: &ParamInfo, color: Color32) -> Response {
    let (resp, hot) = knob_turn(ui, resp, value, info);
    paint_knob(ui.painter(), rect, info, *value, color, hot);
    resp.on_hover_text(format!("{}: {}", info.name, info.format(*value)))
}

/// What every knob does with the mouse; returns the response and whether it is "hot".
fn knob_turn(ui: &mut Ui, mut resp: Response, value: &mut f32, info: &ParamInfo) -> (Response, bool) {
    let n0 = info.to_normalized(*value);
    let mut n = n0;
    let before = *value;

    // During a drag the knob keeps its own raw, continuous position and only
    // the value it outputs is quantised (choice knobs snap to steps). Rounding
    // the position itself every frame threw away any movement smaller than
    // half a step per frame, so stepped knobs barely moved.
    let raw_id = resp.id.with("raw");
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let raw: f32 = if resp.drag_started() { n0 } else { ui.data(|d| d.get_temp(raw_id)).unwrap_or(n0) };
        n = (raw - resp.drag_delta().y * if fine { 0.0015 } else { 0.006 }).clamp(0.0, 1.0);
        ui.data_mut(|d| d.insert_temp(raw_id, n));
    }
    if resp.hovered() {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            n = if info.is_choice() {
                // One notch = one step.
                info.to_normalized((value.round() + wheel.signum()).clamp(info.min, info.max))
            } else {
                n + wheel.signum() * 0.02
            };
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
    (resp, hot)
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
            d if d < 44.0 && bipolar => KnobStyle::Pan,
            d if d < 34.0 => KnobStyle::Tiny,
            d if d < 62.0 => KnobStyle::Rubber,
            _ => KnobStyle::Polymer,
        }
    }
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
        KnobStyle::Pan => (133.5, 15.0, hex(0x252525)),
        KnobStyle::Tiny => (119.0, 6.0, hex(0x222222)),
        KnobStyle::Rubber => (124.0, 6.0, hex(0x222222)),
        KnobStyle::Polymer => (125.0, 7.0, hex(0x1c1c1c)),
    };
    let (arc_r, arc_w) = (arc_r * u, (arc_w * u).max(1.6));

    // Body.
    match style {
        KnobStyle::Pan => {
            p.circle_filled(c, 132.0 * u, hex(0x191919));
            p.circle_filled(c, 117.0 * u, lift(hex(0x454545)));
            p.circle_filled(c, 99.0 * u, lift(hex(0x4d4d4d)));
            p.circle_filled(c, 84.0 * u, lift(hex(0x474747)));
        }
        KnobStyle::Tiny => {
            p.circle_filled(c, 94.0 * u, lift(hex(0x363636)));
            p.circle_filled(c, 85.0 * u, lift(hex(0x3b3b3b)));
        }
        KnobStyle::Rubber => {
            p.circle_filled(c, 108.0 * u, hex(0x1b1b1b));
            p.circle_filled(c, 98.5 * u, lift(hex(0x5f5f5f)));
            p.circle_filled(c, 93.5 * u, lift(hex(0x3f3f3f)));
            p.circle_filled(c, 83.0 * u, lift(hex(0x414141)));
        }
        KnobStyle::Polymer => {
            p.circle_filled(c, 110.0 * u, hex(0x1c1c1c));
            p.circle_filled(c, 106.0 * u, lift(hex(0x363636)));
            p.circle_filled(c, 101.0 * u, hex(0x1c1c1c));
            p.circle_filled(c, 96.0 * u, lift(hex(0x3b3b3b)));
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
                if on { color } else { hex(0x2d2d2d) },
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
            p.line_segment([a, b], stroke(w, lift(hex(0xa9a9a9))));
            p.circle_filled(a, w * 0.5, lift(hex(0xa9a9a9)));
            p.circle_filled(b, w * 0.5, lift(hex(0xa9a9a9)));
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
    /// The recorder's keys.
    Record,
    Pause,
    Prev,
    Next,
    Plus,
    Minus,
    Rack,
    Piano,
    Playlist,
    Mixer,
    Wave,
    /// Arrangement tools.
    Pointer,
    Cut,
    Slide,
    /// An automation curve.
    Curve,
    /// The window's layout with one side filled in: the left panel, the
    /// bottom area, the right panel (like a code editor's layout keys).
    PanelLeft,
    PanelBottom,
    PanelRight,
}

/// Draw an icon centred in `r` (a key's face). Hand-drawn vectors until the
/// user's own icon set lands; `bg` is the face colour (for cut-outs).
pub fn paint_icon(p: &egui::Painter, r: Rect, icon: Icon, ink: Color32, _bg: Color32) {
    let c = r.center();
    let u = r.height().min(r.width()) / 19.0; // icons are drawn on a ±6-unit grid
    let line = stroke((1.7 * u).max(1.2), ink);
    let tri = |pts: [Pos2; 3]| Shape::convex_polygon(pts.to_vec(), ink, egui::Stroke::NONE);
    match icon {
        Icon::Play => {
            p.add(tri([c + vec2(-4.0, -5.5) * u, c + vec2(6.0, 0.0) * u, c + vec2(-4.0, 5.5) * u]));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(9.0, 9.0) * u), cr(2), ink);
        }
        Icon::Record => {
            p.circle_filled(c, 5.0 * u, ink);
        }
        Icon::Pause => {
            for x in [-2.6, 2.6] {
                p.rect_filled(Rect::from_center_size(c + vec2(x, 0.0) * u, vec2(3.0, 10.0) * u), cr(1), ink);
            }
        }
        Icon::Prev | Icon::Next => {
            let s = if matches!(icon, Icon::Prev) { -1.0 } else { 1.0 };
            let pts = vec![c + vec2(-1.5 * s, -4.5) * u, c + vec2(2.5 * s, 0.0) * u, c + vec2(-1.5 * s, 4.5) * u];
            p.add(Shape::line(pts, line));
        }
        Icon::Plus | Icon::Minus => {
            p.line_segment([c - vec2(5.0, 0.0) * u, c + vec2(5.0, 0.0) * u], line);
            if matches!(icon, Icon::Plus) {
                p.line_segment([c - vec2(0.0, 5.0) * u, c + vec2(0.0, 5.0) * u], line);
            }
        }
        Icon::Rack => {
            for row in 0..3 {
                for col in 0..3 {
                    let at = c + vec2(col as f32 - 1.0, row as f32 - 1.0) * 4.2 * u;
                    let r = Rect::from_center_size(at, vec2(3.0, 3.0) * u);
                    if (row + col) % 2 == 0 {
                        p.rect_filled(r, cr(1), ink);
                    } else {
                        p.rect_stroke(r, cr(1), stroke(u.max(1.0), ink), egui::StrokeKind::Inside);
                    }
                }
            }
        }
        Icon::Piano => {
            // From the user's reference: a rounded frame, three black keys, key lines below.
            let body = Rect::from_center_size(c, vec2(14.0, 10.5) * u);
            p.rect_stroke(body, cr((2.0 * u) as u8), line, egui::StrokeKind::Middle);
            for i in 1..4 {
                let x = body.left() + body.width() * i as f32 / 4.0;
                p.vline(x, body.center().y..=body.bottom(), stroke(line.width * 0.75, ink));
                let key =
                    Rect::from_min_max(pos2(x - 1.3 * u, body.top()), pos2(x + 1.3 * u, body.center().y + 0.8 * u));
                p.rect_filled(key, cr(1), ink);
            }
        }
        Icon::Mixer => {
            // Three faders at different heights.
            for (x, cap) in [(-4.5, -2.0), (0.0, 3.0), (4.5, -4.5)] {
                p.line_segment([c + vec2(x, -6.0) * u, c + vec2(x, 6.0) * u], stroke(u.max(1.0), ink));
                p.rect_filled(Rect::from_center_size(c + vec2(x, cap) * u, vec2(3.6, 2.6) * u), cr(1), ink);
            }
        }
        Icon::PanelLeft | Icon::PanelBottom | Icon::PanelRight => {
            let frame = Rect::from_center_size(c, vec2(14.0, 11.0) * u);
            let part = match icon {
                Icon::PanelLeft => Rect::from_min_max(frame.min, pos2(frame.left() + 4.5 * u, frame.bottom())),
                Icon::PanelRight => Rect::from_min_max(pos2(frame.right() - 4.5 * u, frame.top()), frame.max),
                _ => Rect::from_min_max(pos2(frame.left(), frame.bottom() - 4.0 * u), frame.max),
            };
            p.rect_filled(part, cr((1.5 * u) as u8), ink);
            p.rect_stroke(frame, cr((1.5 * u) as u8), line, egui::StrokeKind::Middle);
        }
        Icon::Curve => {
            // A curve rising through three points.
            let pts: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let x = i as f32 / 12.0;
                    c + vec2(-6.0 + 12.0 * x, 5.0 - 10.0 * x * x * (3.0 - 2.0 * x)) * u
                })
                .collect();
            p.add(Shape::line(pts, line));
            for (x, y) in [(-6.0, 5.0), (0.0, 0.0), (6.0, -5.0)] {
                p.circle_filled(c + vec2(x, y) * u, 1.8 * u, ink);
            }
        }
        Icon::Pointer => {
            // An arrow cursor.
            let pts = [(-3.5, -6.0), (4.5, 2.0), (0.8, 2.2), (2.8, 6.0), (1.0, 6.8), (-1.0, 3.0), (-3.5, 5.5)];
            let poly: Vec<Pos2> = pts.iter().map(|&(x, y)| c + vec2(x, y) * u).collect();
            p.add(Shape::closed_line(poly, line));
        }
        Icon::Cut => {
            // A blade coming down through a clip.
            let clip = Rect::from_center_size(c + vec2(0.0, 2.5) * u, vec2(13.0, 6.0) * u);
            p.rect_stroke(clip, cr((1.5 * u) as u8), line, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(0.0, -7.0) * u, c + vec2(0.0, 7.5) * u], stroke(line.width * 1.2, ink));
            p.add(tri([c + vec2(-2.2, -7.0) * u, c + vec2(2.2, -7.0) * u, c + vec2(0.0, -3.5) * u]));
        }
        Icon::Slide => {
            // A clip frame with what's inside moving sideways.
            let clip = Rect::from_center_size(c, vec2(14.0, 9.0) * u);
            p.rect_stroke(clip, cr((1.5 * u) as u8), line, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(-4.0, 0.0) * u, c + vec2(4.0, 0.0) * u], line);
            p.add(tri([c + vec2(-5.5, 0.0) * u, c + vec2(-3.0, -2.2) * u, c + vec2(-3.0, 2.2) * u]));
            p.add(tri([c + vec2(5.5, 0.0) * u, c + vec2(3.0, -2.2) * u, c + vec2(3.0, 2.2) * u]));
        }
        Icon::Wave => {
            // A short waveform: bars of different heights around a centre line.
            for (i, hgt) in [2.0, 5.0, 9.0, 6.0, 11.0, 7.0, 3.5].iter().enumerate() {
                let x = c.x + (i as f32 - 3.0) * 2.0 * u;
                p.line_segment(
                    [pos2(x, c.y - hgt * 0.5 * u), pos2(x, c.y + hgt * 0.5 * u)],
                    stroke((1.3 * u).max(1.0), ink),
                );
            }
        }
        Icon::Playlist => {
            // Three tracks with clips on them.
            for (row, (x0, x1)) in [(-4.0, (-6.0, 1.0)), (0.0, (-2.0, 6.0)), (4.0, (-6.0, -1.0))] {
                let r = Rect::from_min_max(c + vec2(x0, row - 1.4) * u, c + vec2(x1, row + 1.4) * u);
                p.rect_filled(r, cr(1), ink);
            }
        }
    }
}

/// A toolbar module: a flat, square-cornered plate mounted flush in the rack,
/// as tall as the toolbar. No caption: what's inside says what it is (hover
/// anything for its name and keys).
pub fn module<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let m = theme::m();
    egui::Frame::default()
        .fill(theme::PANEL)
        .inner_margin(egui::Margin::symmetric(m.module_pad as i8, 0))
        .show(ui, |ui| {
            ui.set_height(m.toolbar_h);
            ui.horizontal_centered(add).inner
        })
        .inner
}

/// The big toolbar key: the toolbar's height less a module's padding.
pub fn toolbar_key() -> f32 {
    let m = theme::m();
    (m.toolbar_h - 2.0 * m.module_pad).max(m.control_h)
}

/// What's printed on a key.
#[derive(Clone, Copy)]
pub enum Legend<'a> {
    Icon(Icon),
    Text(&'a str),
}

fn paint_legend(p: &egui::Painter, face: Rect, legend: Legend, lit: Option<theme::Light>) {
    let ink = theme::key_ink(lit.is_some());
    match legend {
        Legend::Icon(icon) => {
            let bg = lit.map_or(theme::KEY_FACE, |l| l.mid);
            paint_icon(p, face.shrink(face.height() * 0.12), icon, ink, bg);
        }
        Legend::Text(t) => {
            let size = (face.height() * 0.42).clamp(9.0, 13.0);
            p.text(face.center(), egui::Align2::CENTER_CENTER, t, egui::FontId::monospace(size), ink);
        }
    }
}

/// A silicone key that lights in `light` when `on`.
pub fn key(ui: &mut Ui, size: egui::Vec2, legend: Legend, on: bool, light: theme::Light, hint: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let pressed = resp.is_pointer_button_down_on();
    let lit = on.then_some(light);
    theme::key(p, rect, lit, resp.hovered() && !pressed, if pressed { 0.9 } else { 1.0 });
    paint_legend(p, theme::key_face(rect), legend, lit);
    resp.on_hover_text(hint)
}

/// Radio keys: one recess holding several faces; the chosen one is lit in
/// its own light. Returns the index clicked this frame.
pub fn radio_keys(
    ui: &mut Ui,
    size: egui::Vec2,
    options: &[(Legend, theme::Light)],
    chosen: usize,
    hint: &str,
) -> Option<usize> {
    let total = vec2(size.x * options.len() as f32, size.y);
    let (rect, resp) = ui.allocate_exact_size(total, Sense::click());
    let p = ui.painter();
    let hover = resp.hover_pos().map(|pos| (((pos.x - rect.left()) / size.x) as usize).min(options.len() - 1));
    let pressed = resp.is_pointer_button_down_on();
    let faces = theme::radio_keys(p, rect, options.len(), |i| {
        let lit = (i == chosen).then_some(options[i].1);
        let hot = hover == Some(i) && !pressed;
        (lit, hot)
    });
    for (i, face) in faces.into_iter().enumerate() {
        paint_legend(p, face, options[i].0, (i == chosen).then_some(options[i].1));
    }
    let clicked = resp.clicked().then_some(()).and(hover);
    resp.on_hover_text(hint);
    clicked
}

/// LCD text, laid out tight like the studies (letter-spacing -0.06em).
fn lcd_galley(ui: &Ui, text: &str, size: f32) -> std::sync::Arc<egui::Galley> {
    let job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::monospace(size),
            extra_letter_spacing: -0.06 * size,
            color: theme::LCD_INK,
            ..Default::default()
        },
    );
    ui.fonts(|f| f.layout_job(job))
}

fn lcd_caption(p: &egui::Painter, rect: Rect, caption: &str) {
    p.text(
        rect.center_bottom() - vec2(0.0, rect.height() * 0.16),
        egui::Align2::CENTER_CENTER,
        caption,
        egui::FontId::proportional((rect.height() * 0.23).clamp(9.0, 11.0)),
        theme::LCD_INK_DIM,
    );
}

/// The digits sit a little above centre, leaving room for the caption.
fn lcd_digits_center(rect: Rect) -> Pos2 {
    rect.center() - vec2(0.0, rect.height() * 0.11)
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
    let digit_size = (size.y * 0.6).round();
    let text = format!("{:.*}", decimals, *value);
    let galley = lcd_galley(ui, &text, digit_size);
    let center = lcd_digits_center(rect);
    let origin = center - galley.size() * 0.5;
    // Each character's x range, from the laid-out glyphs.
    let glyphs: Vec<(f32, f32)> =
        galley.rows[0].glyphs.iter().map(|g| (origin.x + g.pos.x, origin.x + g.pos.x + g.advance_width)).collect();
    let dot = text.find('.').unwrap_or(text.len());

    // Which decimal place the pointer is over (10^place), kept during a drag.
    let place_id = id.with("place");
    let place_of = |x: f32| -> i32 {
        let mut i = glyphs.iter().position(|&(_, x1)| x < x1).unwrap_or(glyphs.len() - 1);
        if i == dot {
            i += 1;
        }
        let i = i.min(text.len() - 1);
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
            .font(egui::FontId::monospace(digit_size))
            .text_color(theme::LCD_INK)
            .frame(false)
            .horizontal_align(egui::Align::Center);
        let inner = Rect::from_center_size(center, vec2(size.x - 8.0, digit_size + 4.0));
        let r = ui.put(inner, edit);
        r.request_focus();
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) || r.lost_focus() || escape {
            if !escape {
                if let Ok(v) = buf.trim().parse::<f64>() {
                    *value = v;
                }
            }
        } else {
            editing = Some(buf);
        }
    } else {
        p.galley(origin, galley.clone(), theme::LCD_INK);
        // Underline the digit being tuned.
        if resp.hovered() || resp.dragged() {
            let i = if place >= 0 { dot as i32 - 1 - place } else { dot as i32 - place };
            if let Some(&(x0, x1)) = glyphs.get(i.max(0) as usize) {
                let y = origin.y + galley.size().y - 1.0;
                p.hline(x0 + 1.0..=x1 - 1.0, y, stroke(1.5, theme::LCD_INK_DIM));
            }
        }
    }
    ui.data_mut(|d| match &editing {
        Some(b) => d.insert_temp(editing_id, b.clone()),
        None => d.remove::<String>(editing_id),
    });
    lcd_caption(&p, rect, caption);

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
    let galley = lcd_galley(ui, text, (size.y * 0.5).round());
    p.galley(lcd_digits_center(rect) - galley.size() * 0.5, galley, theme::LCD_INK);
    lcd_caption(p, rect, caption);
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
    let db = lyra_plugin::gain_to_db(level);
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

/// A bar of the fixed meter gradient lit up to `frac`, horizontal (left → right)
/// or vertical (bottom → top). Each colour stays put; the light only reveals it.
pub fn meter_bar(p: &egui::Painter, r: Rect, frac: f32, vertical: bool) {
    p.rect_filled(r, cr(0), Color32::from_rgb(0x28, 0x28, 0x28));
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
    p.rect_filled(r, cr(2), Color32::from_rgb(0x15, 0x15, 0x15));
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
    let size = vec2(white.len() as f32 * key_w + 6.0, toolbar_key() * 0.9);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    theme::well(p, rect);
    let keys = rect.shrink2(vec2(3.0, 4.0));
    let lit = |k: u8| (first_key..=last_key).contains(&k);
    for (i, &k) in white.iter().enumerate() {
        let r = Rect::from_min_size(pos2(keys.left() + i as f32 * key_w, keys.top()), vec2(key_w - 1.0, keys.height()));
        let c = if lit(k) { theme::TEXT } else { Color32::from_rgb(0x45, 0x45, 0x45) };
        p.rect_filled(r, cr(1), c);
    }
    for k in (lo..hi).filter(|k| lyra_model::theory::is_black(*k)) {
        let i = white.iter().position(|&w| w > k).unwrap_or(0);
        let x = keys.left() + i as f32 * key_w - 1.5;
        let r = Rect::from_min_size(pos2(x, keys.top()), vec2(2.5, keys.height() * 0.6));
        p.rect_filled(r, cr(0), if lit(k) { Color32::from_rgb(0x6e, 0x6e, 0x6e) } else { theme::WELL });
    }
    let resp =
        resp.on_hover_text("Where your typing keyboard plays (Z…P rows). Click left / right to shift an octave.");
    if resp.clicked() {
        let x = resp.interact_pointer_pos().map_or(rect.center().x, |p| p.x);
        return Some(if x < rect.center().x { -1 } else { 1 });
    }
    None
}

/// What a sweepable toggle should do this frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sweep {
    /// A gesture starts here: take an undo checkpoint, then set the value.
    Begin(bool),
    /// The gesture passed over this toggle: set the value (same undo step).
    Continue(bool),
}

/// Press-and-sweep for rows of toggles (mute lights, bypass lights, …):
/// press one to flip it, keep the button down and sweep over its neighbours
/// to set them the same way. One gesture is one undo step.
///
/// `group` names the row of toggles that belong together (toggles in other
/// groups ignore the sweep). Call it for every toggle, every frame, with the
/// toggle's rect and current state. Uses the pointer directly, because egui
/// doesn't report hovers on other widgets while a drag is in progress.
pub fn sweep(ui: &Ui, rect: Rect, group: &str, on: bool) -> Option<Sweep> {
    let key = egui::Id::new("lyra-sweep");
    let (pressed, down, pos) =
        ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.interact_pos()));
    let over = pos.is_some_and(|p| rect.contains(p)) && ui.rect_contains_pointer(rect);
    if pressed && over {
        ui.data_mut(|d| d.insert_temp(key, (group.to_string(), !on)));
        return Some(Sweep::Begin(!on));
    }
    let gesture: Option<(String, bool)> = ui.data(|d| d.get_temp(key));
    match gesture {
        Some(_) if !down && !pressed => {
            ui.data_mut(|d| d.remove::<(String, bool)>(key));
            None
        }
        Some((g, target)) if over && g == group && on != target => Some(Sweep::Continue(target)),
        _ => None,
    }
}

/// A docked panel's header: its tabs edge to edge, the current one in the
/// panel's own colour (so it joins the panel below, like a tab), the others
/// darker; and a fold key (a chevron) on the panel's inner side. Returns the
/// tab clicked, and whether the fold key was.
pub fn panel_header(
    ui: &mut Ui,
    tabs: &[&str],
    chosen: usize,
    fold_on_left: bool,
    fold_hint: &str,
) -> (Option<usize>, bool) {
    let m = theme::m();
    let h = m.control_h + 4.0;
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    ui.painter().rect_filled(r, cr(0), theme::BG);
    let (fold, rest) = if fold_on_left {
        (Rect::from_min_size(r.min, vec2(h, h)), Rect::from_min_max(pos2(r.left() + h + theme::SEAM, r.top()), r.max))
    } else {
        (
            Rect::from_min_size(pos2(r.right() - h, r.top()), vec2(h, h)),
            Rect::from_min_max(r.min, pos2(r.right() - h - theme::SEAM, r.bottom())),
        )
    };
    let id = ui.id().with(("panel-header", tabs.first().copied().unwrap_or("")));
    let off = theme::mix(theme::BG, theme::PANEL, 0.55);
    let mut clicked = None;
    for (i, cell) in crate::flex::even_row(rest, tabs.len(), theme::SEAM).into_iter().enumerate() {
        let resp = ui.interact(cell, id.with(i), Sense::click());
        let on = i == chosen;
        let fill = if on {
            theme::PANEL
        } else if resp.hovered() {
            theme::mix(off, theme::PANEL, 0.5)
        } else {
            off
        };
        ui.painter().rect_filled(cell, cr(0), fill);
        let ink = if on || resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
        ui.painter().with_clip_rect(cell).text(
            cell.center(),
            egui::Align2::CENTER_CENTER,
            tabs[i],
            egui::FontId::proportional(m.text_small + 1.0),
            ink,
        );
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    let resp = ui.interact(fold, id.with("fold"), Sense::click());
    ui.painter().rect_filled(fold, cr(0), if resp.hovered() { theme::mix(off, theme::PANEL, 0.5) } else { off });
    let icon = if fold_on_left { Icon::Next } else { Icon::Prev };
    let ink = if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
    paint_icon(ui.painter(), fold.shrink(h * 0.22), icon, ink, off);
    let folded = resp.on_hover_text(fold_hint).clicked();
    (clicked, folded)
}

/// A big key across the whole width (a rack's "+ New …"): easy to hit.
pub fn wide_key(ui: &mut Ui, label: &str, hint: &str) -> Response {
    let m = theme::m();
    key(ui, vec2(ui.available_width(), (m.step_h * 1.4).round()), Legend::Text(label), false, theme::L_MINT, hint)
}

/// How a mixer insert is written in a number well: M for the master.
pub fn insert_label(n: usize) -> String {
    if n == 0 {
        "M".into()
    } else {
        n.to_string()
    }
}

/// A name typed into [`name_field`].
pub struct Renamed {
    pub text: String,
    /// The first change since the field got focus: take the undo step now,
    /// so a whole rename is one step, not one per keystroke.
    pub first: bool,
}

/// A one-line name field. Returns the new text when it changed.
pub fn name_field(ui: &mut Ui, current: &str, width: f32) -> Option<Renamed> {
    let mut text = current.to_string();
    let edit = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width).font(egui::TextStyle::Body));
    let step = edit.id.with("undo-step");
    if edit.lost_focus() {
        ui.data_mut(|d| d.remove::<bool>(step));
    }
    if !edit.changed() {
        return None;
    }
    let first = !ui.data(|d| d.get_temp::<bool>(step).unwrap_or(false));
    ui.data_mut(|d| d.insert_temp(step, true));
    Some(Renamed { text, first })
}

/// A small number in a well: drag up/down or scroll to change it,
/// double-click for `default`. `label` turns the value into text.
pub fn number_well(
    ui: &mut Ui,
    value: &mut usize,
    range: std::ops::RangeInclusive<usize>,
    default: usize,
    size: egui::Vec2,
    label: impl Fn(usize) -> String,
) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let before = *value;
    let mut v = *value as i64;
    if resp.dragged() {
        let acc_id = resp.id.with("acc");
        let mut acc: f32 = ui.data(|d| d.get_temp(acc_id)).unwrap_or(0.0);
        acc -= resp.drag_delta().y / 10.0;
        let n = acc.trunc();
        acc -= n;
        ui.data_mut(|d| d.insert_temp(acc_id, acc));
        v += n as i64;
    }
    if resp.hovered() {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            v += wheel.signum() as i64;
        }
    }
    if resp.double_clicked() {
        v = default as i64;
    }
    *value = (v.max(*range.start() as i64) as usize).min(*range.end());
    if *value != before {
        resp.mark_changed();
    }
    let p = ui.painter();
    theme::well(p, rect);
    let hot = resp.hovered() || resp.dragged();
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label(*value),
        egui::FontId::monospace(theme::m().text_small + 1.0),
        if hot { theme::TEXT } else { theme::TEXT_DIM },
    );
    resp
}
