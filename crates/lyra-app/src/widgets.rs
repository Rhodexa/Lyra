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

fn knob_with(ui: &mut Ui, resp: Response, rect: Rect, value: &mut f32, info: &ParamInfo, color: Color32) -> Response {
    let (resp, hot) = knob_turn(ui, resp, value, info);
    paint_knob(ui.painter(), rect, info, *value, color, hot);
    resp.hint(format!("{}: {}", info.name, info.format(*value)))
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
    resp.hint(hint)
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
    /// A floating window's close key.
    Close,
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
        Icon::Close => {
            p.line_segment([c - vec2(4.0, 4.0) * u, c + vec2(4.0, 4.0) * u], line);
            p.line_segment([c + vec2(-4.0, 4.0) * u, c + vec2(4.0, -4.0) * u], line);
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

/// A floating window's title row (its own, not egui's): the name in caps on
/// the label font, a close key at the right, an engraved line under it like
/// the editors' headers. The window drags by its background.
pub fn window_title(ui: &mut Ui, title: &str, open: &mut bool) {
    let h = theme::m().control_h;
    ui.horizontal(|ui| {
        ui.set_height(h);
        let g = legend_galley(ui, title, 13.0, false, theme::INK);
        let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
        ui.painter().galley(r.min, g, theme::INK);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if button_sized(ui, vec2(h, h), Legend::Icon(Icon::Close), None, "Close").clicked() {
                *open = false;
            }
        });
    });
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 2.0), Sense::hover());
    // Edge to edge: across the window's 8 pt margin too.
    let wide = line.expand2(vec2(8.0, 0.0));
    ui.ctx().layer_painter(ui.layer_id()).rect_filled(wide, cr(0), theme::GROUND);
    ui.add_space(4.0);
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

/// LCD digits (catalog-next, settled): each character in its own cell (a
/// digit 0.9 of the font size wide, a dot or a colon half that), so every
/// digit is a big target. Returns each cell's x range, centred on `center_x`.
fn lcd_cells(text: &str, size: f32, center_x: f32) -> Vec<(char, f32, f32)> {
    let w = |c: char| if c == '.' || c == ':' { size * 0.45 } else { size * 0.9 };
    let total: f32 = text.chars().map(w).sum();
    let mut x = center_x - total * 0.5;
    text.chars()
        .map(|c| {
            let cell = (c, x, x + w(c));
            x += w(c);
            cell
        })
        .collect()
}

/// Paints LCD text in its cells, vertically centred on `y`; `aim`: the cell
/// shown in inverse video, as tall as `screen`.
fn paint_lcd(p: &egui::Painter, cells: &[(char, f32, f32)], size: f32, y: f32, screen: Rect, aim: Option<usize>) {
    let font = egui::FontId::monospace(size);
    for (i, &(c, x0, x1)) in cells.iter().enumerate() {
        let on = aim == Some(i);
        if on {
            let cell = Rect::from_min_max(pos2(x0, screen.top() + 4.0), pos2(x1, screen.bottom() - 4.0));
            p.rect_filled(cell, cr(2), theme::LCD_INK);
        }
        p.text(pos2((x0 + x1) * 0.5, y), egui::Align2::CENTER_CENTER, c, font.clone(), if on { theme::LCD } else { theme::LCD_INK });
    }
}

/// Height of an LCD's label line, under the screen.
const LCD_LABEL_H: f32 = 12.0;

/// The screen part of an LCD widget: the label goes under it.
fn lcd_screen(rect: Rect, caption: &str) -> Rect {
    if caption.is_empty() {
        rect
    } else {
        Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - LCD_LABEL_H))
    }
}

/// The label under an LCD: small caps on the surface, quiet.
fn lcd_caption(ui: &Ui, rect: Rect, caption: &str) {
    if caption.is_empty() {
        return;
    }
    let text: String = caption.split_whitespace().collect::<Vec<_>>().join(" ");
    let job = egui::text::LayoutJob::single_section(
        text.to_uppercase(),
        egui::TextFormat { font_id: theme::label(9.0), color: theme::INK_DIM, extra_letter_spacing: 0.9, ..Default::default() },
    );
    let g = ui.fonts(|f| f.layout_job(job));
    ui.painter().galley(pos2(rect.center().x - g.size().x * 0.5, rect.bottom() - LCD_LABEL_H + 2.0), g, theme::INK_DIM);
}

/// The digits sit in the middle of the screen.
fn lcd_digits_center(screen: Rect) -> Pos2 {
    screen.center()
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
    let screen = lcd_screen(rect, caption);
    let digit_size = (screen.height() * 0.55).round();
    let text = format!("{:.*}", decimals, *value);
    let center = lcd_digits_center(screen);
    let cells = lcd_cells(&text, digit_size, center.x);
    // Each character's x range: its cell.
    let glyphs: Vec<(f32, f32)> = cells.iter().map(|&(_, a, b)| (a, b)).collect();
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
    theme::lcd_panel(&p, screen);
    if let Some(mut buf) = editing.take() {
        let edit = egui::TextEdit::singleline(&mut buf)
            .font(egui::FontId::monospace(digit_size))
            .text_color(theme::LCD_INK)
            .frame(false)
            .horizontal_align(egui::Align::Center);
        let inner = Rect::from_center_size(center, vec2(screen.width() - 8.0, digit_size + 4.0));
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
        // The digit being tuned, in inverse video.
        let aim = (resp.hovered() || resp.dragged())
            .then(|| if place >= 0 { dot as i32 - 1 - place } else { dot as i32 - place })
            .filter(|i| *i >= 0)
            .map(|i| i as usize);
        paint_lcd(&p, &cells, digit_size, center.y, screen, aim);
    }
    ui.data_mut(|d| match &editing {
        Some(b) => d.insert_temp(editing_id, b.clone()),
        None => d.remove::<String>(editing_id),
    });
    lcd_caption(ui, rect, caption);

    let factor = 10f64.powi(decimals as i32);
    *value = ((*value * factor).round() / factor).clamp(*range.start(), *range.end());
    if *value != before {
        resp.mark_changed();
    }
    resp.hint(hint)
}

/// A small read-only LCD (song position and such).
pub fn lcd_text(ui: &mut Ui, text: &str, caption: &str, size: egui::Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let screen = lcd_screen(rect, caption);
    theme::lcd_panel(ui.painter(), screen);
    let size = (screen.height() * 0.5).round();
    let c = lcd_digits_center(screen);
    paint_lcd(ui.painter(), &lcd_cells(text, size, c.x), size, c.y, screen, None);
    lcd_caption(ui, rect, caption);
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

/// The standard meter (catalog-next, settled), vertical or horizontal: a dark
/// well with an inner bed, one bar per channel in the fixed full-scale
/// gradient (-60 … 0 dB), and a pale peak line that waits 0.4 s, then falls.
/// `levels` are linear amplitudes; `well` is the whole meter.
pub fn meter(ui: &Ui, id: egui::Id, well: Rect, levels: &[f32], vertical: bool) {
    let p = ui.painter();
    p.rect_filled(well, cr(2), Color32::from_rgb(0x15, 0x15, 0x15));
    let inner = well.shrink(2.0);
    p.rect_filled(inner, cr(1), Color32::from_rgb(0x1f, 0x1f, 0x1f));
    let n = levels.len().max(1) as f32;
    let bed = inner.shrink(2.0);
    let across = if vertical { bed.width() } else { bed.height() };
    let thick = ((across - 2.0 * (n - 1.0)) / n).max(1.0);
    let now = ui.input(|i| i.time);
    for (k, &level) in levels.iter().enumerate() {
        let off = k as f32 * (thick + 2.0);
        let bar = if vertical {
            Rect::from_min_size(pos2(bed.left() + off, bed.top()), vec2(thick, bed.height()))
        } else {
            Rect::from_min_size(pos2(bed.left(), bed.top() + off), vec2(bed.width(), thick))
        };
        let frac = meter_fraction(level);
        meter_bar(p, bar, frac, vertical);
        // Peak hold: (fraction, time it was set).
        let hold_id = id.with(("peak", k));
        let (mut held, mut since): (f32, f64) = ui.data(|d| d.get_temp(hold_id)).unwrap_or((0.0, 0.0));
        if frac >= held {
            (held, since) = (frac, now);
        } else if now - since > 0.4 {
            held = (held - 0.2 * ui.input(|i| i.stable_dt).min(0.1)).max(frac);
        }
        ui.data_mut(|d| d.insert_temp(hold_id, (held, since)));
        if held > 0.02 {
            let peak = if vertical {
                Rect::from_min_size(pos2(bar.left(), bar.bottom() - held * bar.height() - 1.0), vec2(bar.width(), 2.0))
            } else {
                Rect::from_min_size(pos2(bar.left() + held * bar.width() - 1.0, bar.top()), vec2(2.0, bar.height()))
            };
            p.rect_filled(peak, cr(0), Color32::from_rgb(0xe5, 0xe9, 0xb7).gamma_multiply(0.9));
        }
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
        resp.hint("Where your typing keyboard plays (Z…P rows). Click left / right to shift an octave.");
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
    let h = theme::m().control_h;
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    ui.painter().rect_filled(r, cr(0), theme::GROUND);
    let (fold, rest) = if fold_on_left {
        (Rect::from_min_size(r.min, vec2(h, h)), Rect::from_min_max(pos2(r.left() + h + theme::SEAM, r.top()), r.max))
    } else {
        (
            Rect::from_min_size(pos2(r.right() - h, r.top()), vec2(h, h)),
            Rect::from_min_max(r.min, pos2(r.right() - h - theme::SEAM, r.bottom())),
        )
    };
    let id = ui.id().with(("panel-header", tabs.first().copied().unwrap_or("")));
    let clicked = tab_row(ui, rest, tabs, chosen, id);
    // The fold key: a tab that isn't chosen, with a chevron.
    let resp = ui.interact(fold, id.with("fold"), Sense::click());
    let body = if resp.hovered() { TAB_HOVER } else { TAB_OFF };
    paint_tab(ui.painter(), fold, body, false, fold_on_left, !fold_on_left);
    let icon = if fold_on_left { Icon::Next } else { Icon::Prev };
    let ink = if resp.hovered() { theme::INK } else { theme::INK_DIM };
    paint_icon(ui.painter(), fold.shrink(h * 0.26).translate(vec2(0.0, -1.0)), icon, ink, body);
    let folded = resp.hint(fold_hint).clicked();
    (clicked, folded)
}

/// The colour of a tab that isn't chosen (it sits back, darker).
const TAB_OFF: Color32 = Color32::from_rgb(0x20, 0x20, 0x20);
const TAB_HOVER: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);

/// One folder tab in `cell` (catalog-next.html, settled): square top, rounded
/// bottom unless chosen; the chosen one is the panel's own colour and flows
/// into it. Neighbours share one outline (half of it each); no outline on the
/// top (the seam is the edge) nor on the row's outer ends.
fn paint_tab(p: &egui::Painter, cell: Rect, body: Color32, on: bool, first: bool, last: bool) {
    let bw = theme::OUTLINE_W;
    let r = if on { 0 } else { theme::CORNER as u8 };
    let corners = egui::CornerRadius { nw: 0, ne: 0, sw: r, se: r };
    p.rect_filled(cell, corners, theme::outline_of(body));
    let inner = Rect::from_min_max(
        pos2(cell.left() + if first { 0.0 } else { bw * 0.5 }, cell.top()),
        pos2(cell.right() - if last { 0.0 } else { bw * 0.5 }, cell.bottom() - if on { 0.0 } else { bw }),
    );
    let ri = r.saturating_sub(bw as u8);
    p.rect_filled(inner, egui::CornerRadius { nw: 0, ne: 0, sw: ri, se: ri }, body);
}

/// Caps text that fits `max_w`, ending in "…" if it has to.
fn fitted_caps(ui: &Ui, text: &str, size: f32, max_w: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    let full = legend_galley(ui, text, size, false, color);
    if full.size().x <= max_w {
        return full;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 1 {
        n -= 1;
        let t: String = chars[..n].iter().collect::<String>().trim_end().to_string() + "…";
        let g = legend_galley(ui, &t, size, false, color);
        if g.size().x <= max_w {
            return g;
        }
    }
    legend_galley(ui, "…", size, false, color)
}

/// Folder tabs: equal cells across `r` on the ground strip, the chosen one in
/// the panel's colour (it joins what's below it), the others darker with
/// rounded bottoms. The one tab look, for panel headers and any view's own
/// tabs. Returns the tab clicked.
pub fn tab_row(ui: &mut Ui, r: Rect, tabs: &[&str], chosen: usize, id: egui::Id) -> Option<usize> {
    ui.painter().rect_filled(r, cr(0), theme::GROUND);
    let mut clicked = None;
    let n = tabs.len();
    for (i, cell) in crate::flex::even_row(r, n, 0.0).into_iter().enumerate() {
        let resp = ui.interact(cell, id.with(i), Sense::click());
        let on = i == chosen;
        let body = if on {
            theme::SURFACE
        } else if resp.hovered() {
            TAB_HOVER
        } else {
            TAB_OFF
        };
        paint_tab(ui.painter(), cell, body, on, i == 0, i + 1 == n);
        let ink = if on || resp.hovered() { theme::INK } else { theme::INK_DIM };
        let g = fitted_caps(ui, tabs[i], legend_size(cell.height()), cell.width() - 10.0, ink);
        let at = cell.center() - g.size() * 0.5 - vec2(0.0, if on { 0.0 } else { theme::OUTLINE_W * 0.5 });
        ui.painter().galley(at, g, ink);
        let resp = if fitted_caps(ui, tabs[i], legend_size(cell.height()), f32::MAX, ink).size().x > cell.width() - 10.0 {
            resp.hint(tabs[i])
        } else {
            resp
        };
        if resp.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

// ── Buttons ──
//
// Lyra's one button (catalog-next.html, settled): a flat plate with a 2 px
// outline in its own colour, half as bright; corners 3 · 4 · 5 by size; the
// legend in caps, Roboto Condensed. Lighter under the mouse, darker while
// pressed. ON = backlit: the body takes the light's low tone and the legend
// becomes a dark stencil tinted by the light, bold.
// Sizes: S (28, `control_h`) everywhere, M (40, `tool_key`) in the toolbar.

/// Legend size for a button `h` tall: S 12, M 13, L 15.
fn legend_size(h: f32) -> f32 {
    if h < 34.0 {
        12.0
    } else if h < 46.0 {
        13.0
    } else {
        15.0
    }
}

/// A caps legend, letter-spaced, in the label font.
fn legend_galley(ui: &Ui, text: &str, size: f32, bold: bool, color: Color32) -> std::sync::Arc<egui::Galley> {
    let font = if bold { theme::label_bold(size) } else { theme::label(size) };
    let job = egui::text::LayoutJob::single_section(
        text.to_uppercase(),
        egui::TextFormat { font_id: font, color, extra_letter_spacing: size * 0.04, ..Default::default() },
    );
    ui.fonts(|f| f.layout_job(job))
}

/// Paints a button's plate and legend in `r` for its response.
fn paint_button(ui: &Ui, r: Rect, resp: &Response, legend: Legend, lit: Option<theme::Light>) {
    let p = ui.painter();
    let pressed = resp.is_pointer_button_down_on();
    let hovered = resp.hovered();
    let body = match (lit, pressed, hovered) {
        (Some(l), true, _) => l.edge,
        (Some(l), false, true) => l.mid,
        (Some(l), false, false) => l.low,
        (None, true, _) => theme::mix(theme::PLATE, Color32::BLACK, 0.18),
        (None, false, true) => theme::mix(theme::PLATE, Color32::WHITE, 0.09),
        (None, false, false) => theme::PLATE,
    };
    let k = r.height() / theme::SIZE_M;
    theme::outlined(p, r, (theme::CORNER * k).clamp(3.0, 5.0), body);
    let ink = match lit {
        Some(l) => theme::mix(l.rim, Color32::BLACK, 0.68),
        None if hovered => Color32::WHITE,
        None => theme::LEGEND,
    };
    paint_legend(ui, r, legend, ink, body, lit.is_some());
}

/// A button's legend (icon, or caps text) centred in `r`.
fn paint_legend(ui: &Ui, r: Rect, legend: Legend, ink: Color32, body: Color32, bold: bool) {
    let p = ui.painter();
    match legend {
        Legend::Icon(icon) => {
            let side = r.height().min(r.width());
            paint_icon(p, Rect::from_center_size(r.center(), vec2(side, side)).shrink(side * 0.2), icon, ink, body);
        }
        Legend::Text(t) => {
            let g = legend_galley(ui, t, legend_size(r.height()), bold, ink);
            p.with_clip_rect(r.shrink(theme::OUTLINE_W)).galley(r.center() - g.size() * 0.5, g, ink);
        }
    }
}

/// A bare button (catalog-next, settled): no plate and no outline until the
/// mouse is on it, the same size and target as any button. For menu-like
/// buttons (FILE) and quiet actions (a dialog's "Don't save"). `on_lcd`: it
/// sits on an LCD panel (dark legend, a grey plate under the mouse).
pub fn bare_button(ui: &mut Ui, text: &str, on_lcd: bool, hint: &str) -> Response {
    bare_button_tall(ui, text, theme::m().control_h, on_lcd, hint)
}

/// A [`bare_button`] `h` tall (FILE fills the toolbar's height).
pub fn bare_button_tall(ui: &mut Ui, text: &str, h: f32, on_lcd: bool, hint: &str) -> Response {
    let m = theme::m();
    let w = (legend_galley(ui, text, legend_size(m.control_h), false, Color32::WHITE).size().x + m.control_h * 0.8).max(h);
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let p = ui.painter();
    let (hover_fill, ink, ink_hot) = if on_lcd {
        (theme::mix(theme::LCD, Color32::BLACK, 0.12), theme::LCD_INK, theme::LCD_INK)
    } else {
        (theme::PLATE, theme::LEGEND, Color32::WHITE)
    };
    if resp.hovered() {
        let f = if resp.is_pointer_button_down_on() { theme::mix(hover_fill, Color32::BLACK, 0.12) } else { hover_fill };
        p.rect_filled(r, cr(3), f);
    }
    let g = legend_galley(ui, text, legend_size(m.control_h), false, if resp.hovered() { ink_hot } else { ink });
    p.galley(r.center() - g.size() * 0.5, g, ink);
    if hint.is_empty() {
        resp
    } else {
        resp.hint(hint)
    }
}

/// A button sized to its legend: square for an icon, the text plus room
/// for words. `lit`: it is on (shown in that light).
pub fn button(ui: &mut Ui, legend: Legend, lit: Option<theme::Light>, hint: &str) -> Response {
    let m = theme::m();
    let w = match legend {
        Legend::Icon(_) => m.control_h,
        Legend::Text(t) => {
            let text_w = legend_galley(ui, t, legend_size(m.control_h), true, Color32::WHITE).size().x;
            (text_w + m.control_h * 0.8).max(m.control_h)
        }
    };
    button_sized(ui, vec2(w, m.control_h), legend, lit, hint)
}

/// A button of an exact size (toolbar keys, a row's full width).
pub fn button_sized(ui: &mut Ui, size: egui::Vec2, legend: Legend, lit: Option<theme::Light>, hint: &str) -> Response {
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    paint_button(ui, r, &resp, legend, lit);
    if hint.is_empty() {
        resp
    } else {
        resp.hint(hint)
    }
}

/// A button drawn at `size` that reacts across `reach` height (centred on it):
/// small keys in a tall toolbar are still big targets.
pub fn button_reach(ui: &mut Ui, size: egui::Vec2, reach: f32, legend: Legend, lit: Option<theme::Light>, hint: &str) -> Response {
    let (full, resp) = ui.allocate_exact_size(vec2(size.x, reach.max(size.y)), Sense::click());
    paint_button(ui, Rect::from_center_size(full.center(), size), &resp, legend, lit);
    if hint.is_empty() {
        resp
    } else {
        resp.hint(hint)
    }
}

/// A button in a rectangle laid out elsewhere (flex): no allocation.
pub fn button_at(
    ui: &mut Ui,
    r: Rect,
    id: egui::Id,
    legend: Legend,
    lit: Option<theme::Light>,
    hint: &str,
) -> Response {
    let resp = ui.interact(r, id, Sense::click());
    paint_button(ui, r, &resp, legend, lit);
    if hint.is_empty() {
        resp
    } else {
        resp.hint(hint)
    }
}

/// The mode switch (catalog-next.html, settled): one choice among a few. A
/// sunken track; the chosen option is one lit plate, in its own light, that
/// slides there; each option reacts across its whole share of the track. With
/// two options any click switches (SONG / PAT). `size` is one option's cell.
/// Returns the option to switch to, when clicked.
pub fn choice(
    ui: &mut Ui,
    size: egui::Vec2,
    options: &[(Legend, theme::Light)],
    chosen: usize,
    hint: &str,
) -> Option<usize> {
    let n = options.len().max(1);
    let bw = theme::OUTLINE_W;
    let (rect, _) = ui.allocate_exact_size(vec2(size.x * n as f32 + 2.0 * bw, size.y), Sense::hover());
    let id = ui.id().with(("mode-switch", rect.min.x as i32, rect.min.y as i32));
    let k = rect.height() / theme::SIZE_M;
    let radius = (theme::CORNER * k).clamp(3.0, 5.0);
    let inner = rect.shrink(bw);
    let cell_w = inner.width() / n as f32;
    let cell = |i: usize| Rect::from_min_size(pos2(inner.left() + i as f32 * cell_w, inner.top()), vec2(cell_w, inner.height()));
    let mut clicked = None;
    let mut hovered = None;
    for i in 0..n {
        let resp = ui.interact(cell(i), id.with(i), Sense::click());
        let resp = if hint.is_empty() { resp } else { resp.hint(hint) };
        if resp.hovered() {
            hovered = Some(i);
        }
        if resp.clicked() {
            clicked = Some(if n == 2 { 1 - chosen.min(1) } else { i });
        }
    }
    let p = ui.painter();
    theme::outlined(p, rect, radius + bw, theme::SUNKEN);
    // The lit plate slides to the chosen cell.
    let chosen = chosen.min(n - 1);
    let x = ui.ctx().animate_value_with_time(id.with("x"), cell(chosen).left(), 0.08);
    let plate = Rect::from_min_size(pos2(x, inner.top()), vec2(cell_w, inner.height()));
    let light = options[chosen].1;
    let body = if hovered == Some(chosen) { light.mid } else { light.low };
    theme::outlined(p, plate, radius, body);
    for (i, (legend, l)) in options.iter().enumerate() {
        let on = i == chosen;
        let ink = if on {
            theme::mix(l.rim, Color32::BLACK, 0.68)
        } else if hovered == Some(i) {
            theme::INK
        } else {
            theme::INK_DIM
        };
        paint_legend(ui, cell(i), *legend, ink, if on { body } else { theme::SUNKEN }, on);
    }
    clicked
}

/// Hints (catalog-next.html, settled): an LCD box under the thing you hover,
/// after 0.4 s, gone as soon as you press. A key written at the end after two
/// spaces, like "Play / stop  (Space)", shows as a dark tag. Lyra's own hint
/// instead of egui's tooltip, whose frame is shared with windows.
pub trait Hint {
    fn hint(self, text: impl Into<String>) -> Self;
}

impl Hint for Response {
    fn hint(self, text: impl Into<String>) -> Self {
        let text = text.into();
        // No hints while a drawer is open: they'd sit on top of its rows.
        if text.is_empty()
            || !self.hovered()
            || self.ctx.input(|i| i.pointer.any_down())
            || self.ctx.memory(|m| m.any_popup_open())
        {
            return self;
        }
        let ctx = self.ctx.clone();
        let key = egui::Id::new("lyra-hint");
        let (now, pass) = (ctx.input(|i| i.time), ctx.cumulative_pass_nr());
        // (whose hint, hovered since, last pass seen): a new widget, or one
        // hovered again after a break, starts the wait over.
        let (who, since, last): (egui::Id, f64, u64) = ctx.data(|d| d.get_temp(key)).unwrap_or((egui::Id::NULL, now, 0));
        let since = if who == self.id && last + 1 >= pass { since } else { now };
        ctx.data_mut(|d| d.insert_temp(key, (self.id, since, pass)));
        if now - since < 0.4 {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(0.4 - (now - since) + 0.01));
            return self;
        }
        show_hint(&ctx, self.id, self.rect, &text);
        self
    }
}

fn show_hint(ctx: &egui::Context, id: egui::Id, under: Rect, text: &str) {
    // "Text  (Key)" → the text and a key tag.
    let (body, key) = match text.rfind("  (") {
        Some(i) if text.ends_with(')') && text.len() - i <= 16 => (&text[..i], Some(&text[i + 3..text.len() - 1])),
        _ => (text, None),
    };
    let mut job = egui::text::LayoutJob::single_section(
        body.to_string(),
        egui::TextFormat { font_id: theme::read(12.5), color: theme::LCD_INK, ..Default::default() },
    );
    job.wrap.max_width = 260.0;
    let g = ctx.fonts(|f| f.layout_job(job));
    let tag = key.map(|k| {
        ctx.fonts(|f| f.layout_no_wrap(k.trim().to_uppercase(), theme::label(10.0), theme::INK))
    });
    let pad = vec2(9.0, 6.0);
    let tag_w = tag.as_ref().map_or(0.0, |t| t.size().x + 10.0 + 8.0);
    let size = vec2(g.size().x + tag_w, g.size().y.max(16.0)) + pad * 2.0;
    let screen = ctx.screen_rect();
    let mut at = under.left_bottom() + vec2(0.0, 6.0);
    if at.y + size.y > screen.bottom() {
        at.y = under.top() - 6.0 - size.y;
    }
    at.x = at.x.min(screen.right() - size.x - 4.0).max(screen.left() + 4.0);
    egui::Area::new(id.with("lyra-hint"))
        .order(egui::Order::Tooltip)
        .fixed_pos(at)
        .interactable(false)
        .show(ctx, |ui| {
            let r = Rect::from_min_size(at, size);
            ui.allocate_rect(r, Sense::hover());
            let p = ui.painter();
            p.rect_filled(r.translate(vec2(0.0, 4.0)).expand(2.0), cr(7), Color32::from_black_alpha(70));
            p.rect_filled(r, cr(5), theme::mix(theme::LCD_INK, Color32::BLACK, 0.5));
            p.rect_filled(r.shrink(theme::OUTLINE_W), cr(3), theme::LCD);
            let ty = r.top() + pad.y + (size.y - pad.y * 2.0 - g.size().y) * 0.5;
            p.galley(pos2(r.left() + pad.x, ty), g.clone(), theme::LCD_INK);
            if let Some(t) = tag {
                let tr = Rect::from_min_size(
                    pos2(r.right() - pad.x - t.size().x - 10.0, r.center().y - (t.size().y + 4.0) * 0.5),
                    vec2(t.size().x + 10.0, t.size().y + 4.0),
                );
                p.rect_filled(tr, cr(3), theme::LCD_INK);
                p.galley(tr.center() - t.size() * 0.5, t, theme::INK);
            }
        });
}

/// A plate's title (catalog-next, settled: engraved + tag): a tag in the
/// plate's top-left corner, the ground's colour, square where it meets the
/// plate's edges and rounded at its free corner, the title in caps. Returns
/// how tall it is (lay the plate's fields out below it).
pub fn plate_tag(ui: &Ui, plate: Rect, title: &str) -> f32 {
    let g = ui.fonts(|f| {
        f.layout_job(egui::text::LayoutJob::single_section(
            title.to_uppercase(),
            egui::TextFormat { font_id: theme::label(11.0), color: theme::INK_DIM, extra_letter_spacing: 1.1, ..Default::default() },
        ))
    });
    // The engraved line the tag hangs from, across the plate's top.
    let line = Rect::from_min_size(plate.min, vec2(plate.width(), theme::OUTLINE_W));
    ui.painter().rect_filled(line, cr(0), theme::GROUND);
    let r = Rect::from_min_size(plate.min, g.size() + vec2(16.0, 8.0));
    ui.painter().rect_filled(r, egui::CornerRadius { nw: 0, ne: 0, sw: 0, se: 5 }, theme::GROUND);
    ui.painter().galley(r.min + vec2(8.0, 4.0), g, theme::INK_DIM);
    r.height()
}

/// A field's label (catalog-next, settled): small caps above its control.
pub fn field_label(ui: &Ui, at: Pos2, text: &str) {
    let g = ui.fonts(|f| {
        f.layout_job(egui::text::LayoutJob::single_section(
            text.to_uppercase(),
            egui::TextFormat { font_id: theme::label(10.0), color: theme::INK_DIM, extra_letter_spacing: 0.8, ..Default::default() },
        ))
    });
    ui.painter().galley(at, g, theme::INK_DIM);
}

/// The slider (catalog-next, settled: SL1), for settings (the F12 panel): a
/// recessed track lit up to a small fader cap, then a quiet number and the
/// label. Press to jump there and drag (Shift = fine), the wheel steps, a
/// double-click goes back to `default`. The value stays continuous; only the
/// readout rounds to `decimals`. Returns the row's response (`changed()`).
#[allow(clippy::too_many_arguments)]
pub fn slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
    decimals: usize,
    width: f32,
    label: &str,
) -> Response {
    let h = theme::m().control_h;
    let (lo, hi) = (*range.start(), *range.end());
    let span = (hi - lo).max(1e-6);
    let readout_w = 44.0;
    let label_g = ui.fonts(|f| f.layout_no_wrap(label.to_string(), theme::read(13.0), theme::INK));
    let total = vec2(width + 10.0 + readout_w + 10.0 + label_g.size().x, h);
    let id = ui.next_auto_id(); // labels repeat across sections ("knob size")
    let (row, _) = ui.allocate_exact_size(total, Sense::hover());
    let track = Rect::from_min_size(row.min, vec2(width, h));
    let mut resp = ui.interact(track, id.with("track"), Sense::click_and_drag());
    let before = *value;
    // The cap's centre travels between these (it never hangs off the track).
    let (x0, x1) = (track.left() + 7.5, track.right() - 7.5);
    if resp.double_clicked() {
        *value = default;
    } else if resp.drag_started() && !ui.input(|i| i.modifiers.shift) {
        if let Some(p) = resp.interact_pointer_pos() {
            *value = lo + ((p.x - x0) / (x1 - x0)).clamp(0.0, 1.0) * span;
        }
    } else if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) { 0.1 } else { 1.0 };
        *value += resp.drag_delta().x / (x1 - x0) * span * fine;
    }
    if resp.hovered() {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            *value += wheel.signum() * span / 100.0;
        }
    }
    *value = value.clamp(lo, hi);
    if *value != before {
        resp.mark_changed();
    }
    let t = (*value - lo) / span;
    let cx = x0 + t * (x1 - x0);
    let p = ui.painter();
    let mid_y = track.center().y;
    let outer = Rect::from_min_max(pos2(track.left(), mid_y - 4.0), pos2(track.right(), mid_y + 4.0));
    p.rect_filled(outer, cr(4), Color32::from_gray(0x16));
    let groove = outer.shrink(2.0);
    p.rect_filled(groove, cr(2), Color32::from_gray(0x2b));
    p.rect_filled(Rect::from_min_max(groove.min, pos2(cx, groove.max.y)), cr(2), theme::L_MINT.mid);
    let hot = resp.hovered() || resp.dragged();
    let cap = Rect::from_center_size(pos2(cx, mid_y), vec2(15.0, 24.0));
    p.rect_filled(cap.translate(vec2(0.0, 1.5)), cr(4), Color32::from_black_alpha(64));
    let body = if hot { Color32::from_gray(0x4b) } else { Color32::from_gray(0x44) };
    p.rect_filled(cap, cr(4), Color32::from_gray(0x20));
    p.rect_filled(cap.shrink(1.0), cr(3), theme::mix(body, Color32::WHITE, 0.06));
    p.rect_filled(cap.shrink(3.0), cr(2), body);
    p.vline(cx, (cap.top() + 5.0)..=(cap.bottom() - 5.0), stroke(1.0, Color32::from_gray(0x76)));
    // The readout: a quiet number, fixed width, right-aligned; then the label.
    let ink = if hot { theme::INK } else { theme::INK_DIM };
    let num = format!("{:.*}", decimals, *value);
    let g = ui.fonts(|f| f.layout_no_wrap(num, egui::FontId::monospace(12.0), ink));
    let rx = track.right() + 10.0 + readout_w;
    p.galley(pos2(rx - g.size().x, mid_y - g.size().y * 0.5), g, ink);
    p.galley(pos2(rx + 10.0, mid_y - label_g.size().y * 0.5), label_g, theme::INK);
    resp
}

/// The checkbox (catalog-next, settled: C2): a sunken box, backlit with a dark
/// tick when on; the label in Roboto beside it; the whole row (box and label,
/// S tall) is the target. Returns the response (`changed()` when flipped).
#[allow(dead_code)] // settled in the catalog; the View menu became toggle rows
pub fn checkbox(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let h = theme::m().control_h;
    let g = ui.fonts(|f| f.layout_no_wrap(label.to_string(), theme::read(13.0), theme::INK));
    let (r, mut resp) = ui.allocate_exact_size(vec2(20.0 + 8.0 + g.size().x + 4.0, h), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let light = theme::L_MINT;
    let b = Rect::from_min_size(pos2(r.left(), r.center().y - 10.0), vec2(20.0, 20.0));
    let body = match (*on, resp.hovered()) {
        (true, true) => light.mid,
        (true, false) => light.low,
        (false, true) => theme::mix(theme::SUNKEN, Color32::WHITE, 0.05),
        (false, false) => theme::SUNKEN,
    };
    theme::outlined(ui.painter(), b, 4.0, body);
    if *on {
        let ink = theme::mix(light.rim, Color32::BLACK, 0.68);
        let c = b.center();
        let tick = vec![c + vec2(-4.5, 0.5), c + vec2(-1.5, 3.5), c + vec2(4.5, -3.5)];
        ui.painter().add(Shape::line(tick, stroke(2.4, ink)));
    }
    ui.painter().galley(pos2(b.right() + 8.0, r.center().y - g.size().y * 0.5), g, theme::INK);
    resp
}

/// One row of a [`dropdown`].
#[derive(Clone, Default)]
pub struct DropRow {
    pub label: String,
    pub icon: Option<Icon>,
    /// Shown dim on the right (a shortcut, a count).
    pub detail: String,
    /// A section title (a menu's DRUMS / INSTRUMENTS): dim caps, not pickable.
    pub heading: bool,
    /// A toggle (a menu's view option): a box on the right, ticked when on.
    /// Picking it keeps the drawer open.
    pub check: Option<bool>,
}

impl DropRow {
    pub fn text(label: impl Into<String>) -> Self {
        DropRow { label: label.into(), ..Default::default() }
    }

    pub fn action(label: impl Into<String>, shortcut: impl Into<String>) -> Self {
        DropRow { label: label.into(), detail: shortcut.into(), ..Default::default() }
    }

    pub fn heading(label: impl Into<String>) -> Self {
        DropRow { label: label.into(), heading: true, ..Default::default() }
    }

    pub fn toggle(label: impl Into<String>, on: bool) -> Self {
        DropRow { label: label.into(), check: Some(on), ..Default::default() }
    }
}

/// A menu as an LCD drawer (the dropdown's, for every menu: "everything
/// standardized"): opens under `resp` when it's clicked. `rows` are actions
/// (a shortcut in `detail`), headings and toggles. Returns the row picked.
pub fn menu_drawer(ui: &mut Ui, resp: &Response, id_salt: impl std::hash::Hash, width: f32, rows: &[DropRow]) -> Option<usize> {
    let id = ui.make_persistent_id(("menu", id_salt));
    if resp.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(id));
    }
    drawer(ui, id, resp.rect, width, rows, usize::MAX, true)
}

/// Rows per column before a drawer wraps into the next column.
const DRAWER_ROWS: usize = 12;

/// The dropdown (catalog-next.html, settled): closed, a dark well with an
/// optional icon, the chosen label and a chevron; open, a drawer in the LCD's
/// colours (rows S tall, the chosen one slightly grey and bold) that wraps into
/// columns past 12 rows. `names`: labels are names (tracks, devices, files) and
/// keep their case in the reading font; otherwise they're caps labels.
/// Returns the row picked.
pub fn dropdown(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    width: f32,
    rows: &[DropRow],
    chosen: usize,
    names: bool,
    hint: &str,
) -> Option<usize> {
    let h = theme::m().control_h;
    let (r, resp) = ui.allocate_exact_size(vec2(width, h), Sense::click());
    let id = ui.make_persistent_id(("dropdown", id_salt));
    let enabled = ui.is_enabled();
    let open = ui.memory(|mem| mem.is_popup_open(id));
    let p = ui.painter();
    let body = if resp.hovered() && enabled { theme::mix(theme::SUNKEN, Color32::WHITE, 0.03) } else { theme::SUNKEN };
    let k = h / theme::SIZE_M;
    theme::outlined(p, r, (theme::CORNER * k).clamp(3.0, 5.0), body);
    let ink = if !enabled { theme::INK_FAINT } else if resp.hovered() || open { Color32::WHITE } else { theme::INK };
    let row = rows.get(chosen.min(rows.len().saturating_sub(1)));
    let mut x = r.left() + 9.0;
    if let Some(icon) = row.and_then(|r| r.icon) {
        let ir = Rect::from_center_size(pos2(x + h * 0.25, r.center().y), vec2(h * 0.6, h * 0.6));
        paint_icon(p, ir, icon, ink, body);
        x += h * 0.5 + 6.0;
    }
    let max_w = r.right() - 22.0 - x;
    if let Some(row) = row {
        let g = drop_label(ui, &row.label, names, false, max_w, ink);
        ui.painter().galley(pos2(x, r.center().y - g.size().y * 0.5), g, ink);
    }
    let c = r.right_center() - vec2(12.0, 0.0);
    let (a, b) = if open { (1.5, -2.0) } else { (-1.5, 2.0) };
    let chevron = vec![c + vec2(-3.5, a), c + vec2(0.0, b), c + vec2(3.5, a)];
    ui.painter().add(Shape::line(chevron, stroke(1.6, if enabled { theme::INK_DIM } else { theme::INK_FAINT })));
    // No hint while its own drawer is open (it would cover the list).
    let resp = if hint.is_empty() || open { resp } else { resp.hint(hint) };
    if resp.clicked() && enabled {
        ui.memory_mut(|mem| mem.toggle_popup(id));
    }
    drawer(ui, id, r, width, rows, chosen, names)
}

/// A dropdown's drawer (the LCD's colours), open while `id`'s popup is open,
/// under `anchor`. Returns the row picked.
fn drawer(ui: &mut Ui, id: egui::Id, anchor: Rect, width: f32, rows: &[DropRow], chosen: usize, names: bool) -> Option<usize> {
    if !ui.memory(|mem| mem.is_popup_open(id)) {
        return None;
    }
    let h = theme::m().control_h;
    let r = anchor;
    let mut picked = None;
    let cols = rows.len().div_ceil(DRAWER_ROWS).max(1);
    let per_col = rows.len().div_ceil(cols).max(1);
    let col_w = if cols == 1 { width.max(120.0) } else { (width * 0.8).max(130.0) };
    // Under the anchor, or over it when there's no room below (a key at the
    // bottom of a panel).
    let full_h = per_col as f32 * h + 2.0 * (theme::OUTLINE_W + 2.0);
    let screen = ui.ctx().screen_rect();
    let below = r.left_bottom() + vec2(0.0, 3.0);
    let drawer_pos = if below.y + full_h > screen.bottom() && r.top() - 3.0 - full_h >= screen.top() {
        r.left_top() - vec2(0.0, 3.0 + full_h)
    } else {
        below
    };
    let area = egui::Area::new(id.with("drawer"))
        .order(egui::Order::Foreground)
        .fixed_pos(drawer_pos)
        .show(ui.ctx(), |ui| {
            let pad = theme::OUTLINE_W + 2.0;
            let bg = ui.painter().add(Shape::Noop);
            let top = ui.cursor().min + vec2(pad, pad);
            for (i, row) in rows.iter().enumerate() {
                let (c, k) = (i / per_col, i % per_col);
                let rr = Rect::from_min_size(top + vec2(c as f32 * col_w, k as f32 * h), vec2(col_w, h));
                if row.heading {
                    let g = legend_galley(ui, &row.label, 11.0, false, theme::LCD_INK_DIM);
                    ui.painter().galley(pos2(rr.left() + 8.0, rr.bottom() - 5.0 - g.size().y), g, theme::LCD_INK_DIM);
                    continue;
                }
                let it = ui.interact(rr, id.with(("row", i)), Sense::click());
                let on = i == chosen;
                let fill = if on {
                    Some(theme::mix(theme::LCD_DIM, Color32::BLACK, 0.12))
                } else if it.hovered() {
                    Some(theme::mix(theme::LCD_DIM, Color32::BLACK, 0.06))
                } else {
                    None
                };
                if let Some(f) = fill {
                    ui.painter().rect_filled(rr, cr(3), f);
                }
                let mut tx = rr.left() + 8.0;
                if let Some(icon) = row.icon {
                    let ir = Rect::from_center_size(pos2(tx + h * 0.25, rr.center().y), vec2(h * 0.55, h * 0.55));
                    paint_icon(ui.painter(), ir, icon, theme::LCD_INK, theme::LCD_DIM);
                    tx += h * 0.5 + 6.0;
                }
                let mut right = rr.right();
                if let Some(on) = row.check {
                    let b = Rect::from_center_size(pos2(right - 8.0 - 7.0, rr.center().y), vec2(14.0, 14.0));
                    ui.painter().rect_stroke(b, cr(3), stroke(1.6, theme::LCD_INK), egui::StrokeKind::Inside);
                    if on {
                        let c = b.center();
                        let tick = vec![c + vec2(-3.5, 0.5), c + vec2(-1.0, 3.0), c + vec2(3.5, -3.0)];
                        ui.painter().add(Shape::line(tick, stroke(2.0, theme::LCD_INK)));
                    }
                    right -= 22.0;
                }
                let rr_text = Rect::from_min_max(rr.min, pos2(right, rr.max.y));
                let rr = rr_text;
                let dw = if row.detail.is_empty() {
                    0.0
                } else {
                    let g = ui.fonts(|f| {
                        f.layout_no_wrap(row.detail.clone(), egui::FontId::monospace(11.0), theme::LCD_INK_DIM)
                    });
                    let w = g.size().x;
                    ui.painter().galley(pos2(rr.right() - 8.0 - w, rr.center().y - g.size().y * 0.5), g, theme::LCD_INK_DIM);
                    w + 10.0
                };
                let g = drop_label(ui, &row.label, names, on, rr.right() - 8.0 - dw - tx, theme::LCD_INK);
                ui.painter().galley(pos2(tx, rr.center().y - g.size().y * 0.5), g, theme::LCD_INK);
                if it.clicked() {
                    picked = Some(i);
                }
            }
            let inner = Rect::from_min_size(top, vec2(cols as f32 * col_w, per_col as f32 * h));
            ui.allocate_rect(inner.expand(pad), Sense::hover());
            let outer = inner.expand(pad);
            // Outline first, then the fill inside it; dividers between columns.
            let mut shapes = vec![
                Shape::rect_filled(outer.translate(vec2(0.0, 4.0)).expand(2.0), cr(7), Color32::from_black_alpha(70)),
                Shape::rect_filled(outer, cr(5), theme::mix(theme::LCD_INK, Color32::BLACK, 0.5)),
                Shape::rect_filled(outer.shrink(theme::OUTLINE_W), cr(3), theme::LCD_DIM),
            ];
            for c in 1..cols {
                let x = inner.left() + c as f32 * col_w;
                shapes.push(Shape::line_segment(
                    [pos2(x, inner.top() + 4.0), pos2(x, inner.bottom() - 4.0)],
                    stroke(2.0, theme::mix(theme::LCD_DIM, Color32::BLACK, 0.15)),
                ));
            }
            ui.painter().set(bg, Shape::Vec(shapes));
        });
    // Close: on a pick, on Esc, or on a click outside the drawer and its well.
    let outside = ui.input(|i| {
        i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !r.contains(p))
    });
    let toggled = picked.is_some_and(|i| rows[i].check.is_some());
    if (picked.is_some() && !toggled) || outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.memory_mut(|mem| mem.close_popup());
    }
    picked
}

/// A compact dropdown for a short value in a tight row (a mixer insert): a
/// small well with the value; the wheel and an up/down drag step through the
/// rows, a click opens the drawer. Returns the row chosen (by any of them).
pub fn dropdown_compact(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    size: egui::Vec2,
    rows: &[DropRow],
    chosen: usize,
    drawer_w: f32,
) -> (Response, Option<usize>) {
    let (r, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let id = ui.make_persistent_id(("dropdown-compact", id_salt));
    let n = rows.len().max(1);
    let mut next = None;
    if resp.dragged() {
        let acc_id = id.with("acc");
        let mut acc: f32 = ui.data(|d| d.get_temp(acc_id)).unwrap_or(0.0);
        acc -= resp.drag_delta().y / 10.0;
        let k = acc.trunc();
        acc -= k;
        ui.data_mut(|d| d.insert_temp(acc_id, acc));
        if k != 0.0 {
            next = Some((chosen as i64 + k as i64).clamp(0, n as i64 - 1) as usize);
        }
    }
    if resp.hovered() {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            next = Some((chosen as i64 + wheel.signum() as i64).clamp(0, n as i64 - 1) as usize);
        }
    }
    let open = ui.memory(|mem| mem.is_popup_open(id));
    let hot = resp.hovered() || resp.dragged() || open;
    let body = if hot { theme::mix(theme::SUNKEN, Color32::WHITE, 0.03) } else { theme::SUNKEN };
    theme::outlined(ui.painter(), r, 3.0, body);
    let shown = next.unwrap_or(chosen).min(n - 1);
    let ink = if hot { Color32::WHITE } else { theme::INK };
    if let Some(row) = rows.get(shown) {
        let g = legend_galley(ui, &row.label, 12.0, false, ink);
        ui.painter().galley(r.center() - g.size() * 0.5 - vec2(2.0, 0.0), g, ink);
    }
    // A tiny chevron in the corner: "there's a list".
    let c = pos2(r.right() - 6.0, r.bottom() - 6.0);
    ui.painter().add(Shape::convex_polygon(vec![c + vec2(-3.0, -1.5), c + vec2(3.0, -1.5), c + vec2(0.0, 2.0)], theme::INK_DIM, egui::Stroke::NONE));
    if resp.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(id));
    }
    let picked = drawer(ui, id, r, drawer_w, rows, shown, false);
    (resp, picked.or(next))
}

/// A dropdown label: a name as written (reading font) or caps (label font),
/// cut with "…" to fit `max_w`.
fn drop_label(ui: &Ui, text: &str, names: bool, bold: bool, max_w: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    if !names {
        return if bold {
            legend_galley(ui, text, 12.0, true, color)
        } else {
            fitted_caps(ui, text, 12.0, max_w.max(10.0), color)
        };
    }
    let font = if bold { theme::read_bold(13.0) } else { theme::read(13.0) };
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat { font_id: font, color, ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: max_w.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    ui.fonts(|f| f.layout_job(job))
}

/// A big key across the whole width (a rack's "+ New …"): easy to hit.
pub fn wide_key(ui: &mut Ui, label: &str, hint: &str) -> Response {
    let m = theme::m();
    button_sized(ui, vec2(ui.available_width(), m.control_h), Legend::Text(label), None, hint)
}

/// How a mixer insert is written in a number well: M for the master.
pub fn insert_label(n: usize) -> String {
    if n == 0 {
        "M".into()
    } else {
        n.to_string()
    }
}

/// The text field (catalog-next.html, settled): a sunken well; while you type
/// it becomes the LCD (pale, dark text), since there are no focus rings. Text
/// is the reading font, as typed. `search`: a search mark on the left and,
/// once there's text, a clear key the field's whole height.
pub fn text_field_at(ui: &mut Ui, r: Rect, text: &mut String, id: egui::Id, hint: &str, search: bool) -> Response {
    let focused = ui.memory(|m| m.has_focus(id));
    let hover = ui.rect_contains_pointer(r);
    let bw = theme::OUTLINE_W;
    let rad = (theme::CORNER * r.height() / theme::SIZE_M).clamp(3.0, 5.0);
    let (fill, edge) = if focused {
        (theme::LCD_DIM, theme::mix(theme::LCD_INK, Color32::BLACK, 0.5))
    } else {
        let f = if hover { theme::mix(theme::SUNKEN, Color32::WHITE, 0.03) } else { theme::SUNKEN };
        (f, theme::outline_of(f))
    };
    let p = ui.painter();
    p.rect_filled(r, cr(rad as u8), edge);
    p.rect_filled(r.shrink(bw), cr((rad - bw).max(1.0) as u8), fill);
    let ink = if focused { theme::LCD_INK } else { theme::INK };
    let dim = if focused { theme::LCD_INK_DIM } else { theme::INK_DIM };
    let mut x0 = r.left() + 9.0;
    if search {
        let c = pos2(x0 + 5.0, r.center().y - 1.0);
        p.circle_stroke(c, 4.2, stroke(1.5, dim));
        p.line_segment([c + vec2(3.0, 3.0), c + vec2(6.5, 6.5)], stroke(1.6, dim));
        x0 += 18.0;
    }
    let clear = search && !text.is_empty();
    let clear_r = Rect::from_min_max(pos2(r.right() - bw - (r.height() - 2.0 * bw), r.top() + bw), r.max - vec2(bw, bw));
    let right = if clear { clear_r.left() - 2.0 } else { r.right() - 8.0 };
    let edit_r = Rect::from_min_max(pos2(x0, r.top() + bw), pos2(right, r.bottom() - bw));
    let resp = ui
        .scope(|ui| {
            let v = ui.visuals_mut();
            v.override_text_color = Some(ink);
            v.extreme_bg_color = Color32::TRANSPARENT;
            v.text_cursor.stroke = stroke(2.0, if focused { theme::LCD_INK } else { theme::L_MINT.mid });
            v.selection.bg_fill = if focused { theme::mix(theme::LCD_DIM, theme::LCD_INK, 0.35) } else { theme::L_MINT.rim };
            v.selection.stroke = stroke(1.0, ink);
            let edit = egui::TextEdit::singleline(text)
                .id(id)
                .frame(false)
                .margin(egui::Margin::ZERO)
                .vertical_align(egui::Align::Center)
                .font(theme::read(13.0))
                .hint_text(egui::RichText::new(hint).color(if focused { theme::LCD_INK_DIM } else { theme::INK_FAINT }))
                .desired_width(edit_r.width());
            ui.put(edit_r, edit)
        })
        .inner;
    if clear {
        let c = ui.interact(clear_r, id.with("clear"), Sense::click());
        if c.hovered() {
            let hov = if focused { theme::mix(theme::LCD_DIM, Color32::BLACK, 0.12) } else { theme::PLATE };
            ui.painter().rect_filled(clear_r, egui::CornerRadius { nw: 0, sw: 0, ne: (rad - bw) as u8, se: (rad - bw) as u8 }, hov);
        }
        let (cc, k) = (clear_r.center(), 3.5);
        let xi = if c.hovered() && !focused { theme::INK } else { dim };
        ui.painter().line_segment([cc - vec2(k, k), cc + vec2(k, k)], stroke(1.6, xi));
        ui.painter().line_segment([cc + vec2(-k, k), cc + vec2(k, -k)], stroke(1.6, xi));
        if c.hint("Clear").clicked() {
            text.clear();
            ui.memory_mut(|m| m.request_focus(id));
        }
    }
    resp
}

/// A text field `width` wide and S tall, laid out in the flow.
pub fn text_field(ui: &mut Ui, text: &mut String, width: f32, id_salt: impl std::hash::Hash, hint: &str) -> Response {
    let (r, _) = ui.allocate_exact_size(vec2(width, theme::m().control_h), Sense::hover());
    let id = ui.make_persistent_id(("text-field", id_salt));
    text_field_at(ui, r, text, id, hint, false)
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
    let edit = text_field(ui, &mut text, width, ui.next_auto_id(), "");
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

