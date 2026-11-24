//! The synths' own look, frozen on 2026-11-25.
//!
//! Fluttersine and Chrysalis are independent projects with their own visual
//! language. Lyra's UI is being redesigned (docs/design/catalog-next.html), and
//! nothing in that redesign may change them. So everything visual they used to
//! borrow from `theme` and `widgets` is copied here, exactly as it was, and they
//! use only this module (plus layout, undo and data helpers that draw nothing):
//!
//! * colours, the eight lights, the colour helpers;
//! * the LED, the LCD panel, the well, the key ink;
//! * the knob painter and its drag behaviour;
//! * the two icons they use;
//! * their logos (moved here: they were only ever theirs);
//! * today's sizes, as constants (F12 no longer reaches them);
//! * today's egui style, captured at startup ([`capture_style`]), and their own
//!   font families, copies of today's defaults ([`install_fonts`]), so a new
//!   font for Lyra leaves them alone. [`freeze`] puts the style on a `Ui`.
//!
//! One thing still follows Lyra: hover hints (egui tooltips take the global
//! style). The popup *frames* follow the editor's style, and their contents
//! call [`freeze`] themselves.
//!
//! Change things here only to change the synths, on purpose.

use egui::{vec2, Color32, ColorImage, Context, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, TextureHandle, Ui};
use lyra_plugin::ParamInfo;
use std::f32::consts::PI;
use std::sync::{Arc, OnceLock};

// ── Colours (theme.rs, 2026-11-25) ──
pub const BG: Color32 = Color32::from_rgb(0x16, 0x16, 0x16);
pub const PANEL: Color32 = Color32::from_rgb(0x29, 0x29, 0x29);
pub const PANEL_HI: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
pub const SEAM: f32 = 2.0;
pub const INSET: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
pub const WELL: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
pub const WELL_EDGE: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
pub const TEXT: Color32 = Color32::from_rgb(0xcf, 0xcf, 0xcf);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x96, 0x96, 0x96);

// ── Sizes (Metrics::DEFAULT, 2026-11-25) ──
#[derive(Clone, Copy)]
pub struct Sizes {
    pub text: f32,
    pub text_small: f32,
    pub control_h: f32,
    pub knob_small: f32,
    pub knob_large: f32,
}

pub const fn m() -> Sizes {
    Sizes { text: 14.0, text_small: 11.0, control_h: 26.0, knob_small: 28.0, knob_large: 40.0 }
}

// ── Helpers ──
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

pub fn cr(r: u8) -> egui::CornerRadius {
    egui::CornerRadius::same(r)
}

/// `Stroke::new` with a concretely typed width (egui's generic width param makes
/// bare float literals ambiguous on newer Rust).
pub fn stroke(width: f32, color: Color32) -> Stroke {
    Stroke::new(width, color)
}

/// A colour from 0xRRGGBB.
pub const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Scale a colour's brightness, keeping hue and saturation (k > 1 = brighter).
pub fn shade(c: Color32, k: f32) -> Color32 {
    let f = |v: u8| (v as f32 * k).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// A multi-stop gradient, like CSS: `stops` are (position 0..1, colour), sorted.
pub fn ramp(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let Some(&(first_t, first)) = stops.first() else { return Color32::TRANSPARENT };
    if t <= first_t {
        return first;
    }
    for w in stops.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if t <= b {
            return mix(ca, cb, if b > a { (t - a) / (b - a) } else { 1.0 });
        }
    }
    stops[stops.len() - 1].1
}

/// One backlight colour: a radial ramp for the face (`hi` at the centre →
/// `edge`) plus the `rim` the pad takes when lit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    pub hi: Color32,
    pub mid: Color32,
    pub low: Color32,
    pub edge: Color32,
    pub rim: Color32,
}

const fn light(hi: u32, mid: u32, low: u32, edge: u32, rim: u32) -> Light {
    Light { hi: hex(hi), mid: hex(mid), low: hex(low), edge: hex(edge), rim: hex(rim) }
}

pub const L_MINT: Light = light(0xd5ffe7, 0xb7f3d3, 0x90dbb8, 0x79c6a4, 0x648f7a);
pub const L_CYAN: Light = light(0xd8fbff, 0xb1edf4, 0x82cfdc, 0x6ab5c4, 0x568895);
pub const L_BLUE: Light = light(0xe0eaff, 0xbfcefb, 0x91adeb, 0x7b94ce, 0x6176a3);
pub const L_LAVENDER: Light = light(0xf2e4ff, 0xdcc6fa, 0xb9a0e7, 0xa28acb, 0x7d6a9e);
pub const L_PINK: Light = light(0xffe5f2, 0xffc5df, 0xed9fc3, 0xd485aa, 0xa36683);
pub const L_PEACH: Light = light(0xffeadc, 0xffd0b4, 0xeeab8d, 0xd48e76, 0xa66d5e);
pub const L_AMBER: Light = light(0xfff2d3, 0xf5dba5, 0xdfbd7b, 0xc3a068, 0x927a55);
pub const L_LIME: Light = light(0xefffd7, 0xd7f4b4, 0xb3d989, 0x96bc72, 0x73915b);

impl Light {
    /// The same light, dimmer (k < 1) or brighter, hue kept.
    pub fn scaled(self, k: f32) -> Light {
        Light {
            hi: shade(self.hi, k),
            mid: shade(self.mid, k),
            low: shade(self.low, k),
            edge: shade(self.edge, k),
            rim: shade(self.rim, k),
        }
    }
}


// ── Drawing (theme.rs, 2026-11-25) ──
/// A filled star-shaped outline coloured per vertex by `color`, in rings so
/// a radial gradient survives the interpolation.
fn radial_fan(
    p: &Painter,
    center: egui::Pos2,
    outline: impl Iterator<Item = egui::Pos2>,
    color: impl Fn(egui::Pos2) -> Color32,
) {
    const RINGS: u32 = 5;
    let outline: Vec<egui::Pos2> = outline.collect();
    let n = outline.len() as u32;
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, color(center));
    for k in 1..=RINGS {
        let f = k as f32 / RINGS as f32;
        for q in &outline {
            let v = center.lerp(*q, f);
            mesh.colored_vertex(v, color(v));
        }
    }
    let at = |k: u32, i: u32| 1 + (k - 1) * n + i % n;
    for i in 0..n {
        mesh.add_triangle(0, at(1, i), at(1, i + 1));
        for k in 1..RINGS {
            mesh.add_triangle(at(k, i), at(k + 1, i), at(k + 1, i + 1));
            mesh.add_triangle(at(k, i), at(k + 1, i + 1), at(k, i + 1));
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

/// A circle with a radial gradient centred at `focus` (`radius` = 100 %).
pub fn radial_circle(p: &Painter, c: egui::Pos2, radius: f32, focus: egui::Pos2, stops: &[(f32, Color32)]) {
    let color = |q: egui::Pos2| ramp(stops, q.distance(focus) / radius.max(0.5));
    let segments = ((radius * 1.2) as usize).clamp(16, 64);
    let outline: Vec<egui::Pos2> = (0..segments)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / segments as f32;
            c + egui::vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    p.circle_filled(c, radius, ramp(stops, 1.0));
    radial_fan(p, focus, outline.iter().map(|q| *q + (c - *q).normalized() * 0.4), color);
}

/// Ink for a legend or icon printed on a key.
pub fn key_ink(lit: bool) -> Color32 {
    if lit {
        hex(0x2c2c2c)
    } else {
        hex(0xa4a4a4)
    }
}

/// A round recessed LED, `d` points across. Lit, its well takes its colour.
pub fn led(p: &Painter, c: egui::Pos2, d: f32, light: Option<Light>, hot: bool) {
    let r = d * 0.5;
    let ring = (d * 0.07).max(1.0);
    let bead = d * 0.3;
    match light {
        Some(l) => {
            p.circle_filled(c, r, shade(l.low, 0.4));
            p.circle_stroke(c, r - ring * 0.5, stroke(ring, shade(l.low, 0.55)));
            let l = if hot { l.scaled(1.05) } else { l };
            let focus = c + egui::vec2(-0.1, -0.16) * bead;
            radial_circle(p, c, bead, focus, &[(0.0, l.hi), (0.55, l.mid), (1.0, l.edge)]);
        }
        None => {
            p.circle_filled(c, r, hex(0x161616));
            p.circle_stroke(c, r - ring * 0.5, stroke(ring, hex(0x1d1d1d)));
            let bead_c = if hot { hex(0x535353) } else { hex(0x484848) };
            p.circle_filled(c, bead, bead_c);
            // A faint top highlight on the bead (inset 0 1px 1px).
            let arc: Vec<egui::Pos2> = (0..=8)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.15 + 0.7 * i as f32 / 8.0);
                    c + egui::vec2(a.cos(), a.sin()) * (bead - 0.8)
                })
                .collect();
            p.add(egui::Shape::line(arc, stroke(0.8, mix(bead_c, hex(0x6b6b6b), 0.35))));
        }
    }
}

/// LCD colours: pale backlight, dark ink.
pub const LCD: Color32 = hex(0xdadada);
pub const LCD_INK: Color32 = hex(0x2f2f2f);
pub const LCD_INK_DIM: Color32 = hex(0x5e5e5e);

/// The LCD panel itself (text goes on top).
pub fn lcd_panel(p: &Painter, r: Rect) {
    let rad = (r.height() * 0.11).clamp(3.0, 7.0);
    p.rect_filled(r, cr(rad as u8), LCD);
    // Inset shadow under the top edge, three pixels deep, fading out.
    let shadow = hex(0x19231e);
    for (k, a) in [(0.0, 0.42), (1.0, 0.2), (2.0, 0.07)] {
        let line = Rect::from_min_size(r.min + egui::vec2(rad * 0.55, k), egui::vec2(r.width() - rad * 1.1, 1.0));
        p.rect_filled(line, cr(0), mix(LCD, shadow, a));
    }
}

/// A recessed well (dropdowns, displays): darker than the panel, an inner
/// shadow along the top.
pub fn well(p: &Painter, r: Rect) {
    let rad = (r.height() * 0.2).clamp(3.0, 7.0);
    p.rect_filled(r, cr(rad as u8), WELL);
    p.rect_stroke(r, cr(rad as u8), stroke(1.0, WELL_EDGE), egui::StrokeKind::Inside);
    let shadow = Rect::from_min_size(r.min + egui::vec2(rad * 0.7, 1.0), egui::vec2(r.width() - rad * 1.4, 1.5));
    p.rect_filled(shadow, cr(0), Color32::from_black_alpha(50));
}


// ── Knobs (widgets.rs, 2026-11-25) ──
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

/// A knob that turns exactly like Lyra's knobs (as of the freeze) but is drawn by `paint(painter,
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
    let lift = |col: Color32| if hot { shade(col, 1.08) } else { col };
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


// ── Icons ──
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Icon {
    Prev,
    Next,
}

/// Draw an icon centred in `r` (widgets::paint_icon, 2026-11-25).
pub fn paint_icon(p: &egui::Painter, r: Rect, icon: Icon, ink: Color32, _bg: Color32) {
    let c = r.center();
    let u = r.height().min(r.width()) / 19.0;
    let line = stroke((1.7 * u).max(1.2), ink);
    let s = if matches!(icon, Icon::Prev) { -1.0 } else { 1.0 };
    let pts = vec![c + vec2(-1.5 * s, -4.5) * u, c + vec2(2.5 * s, 0.0) * u, c + vec2(-1.5 * s, 4.5) * u];
    p.add(Shape::line(pts, line));
}

// ── Logos (moved from theme.rs) ──
//
// Plugin artwork by the user: `assets/fluttersine_logo.png` (the wordmark)
// and `assets/chrysalis_emblem.png`. Both are built into the app, and while
// Lyra runs from the source tree it also watches the files: export over one
// (from Krita, say) and the editor shows it within a second, no restart.
// Transparent margins are trimmed, so draw freely.

const FLUTTERSINE_LOGO_PNG: &[u8] = include_bytes!("../assets/fluttersine_logo.png");
const FLUTTERSINE_LOGO_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fluttersine_logo.png");
const CHRYSALIS_EMBLEM_PNG: &[u8] = include_bytes!("../assets/chrysalis_emblem.png");
const CHRYSALIS_EMBLEM_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/chrysalis_emblem.png");
/// Artwork is scaled down to at most this tall (about twice what the editors show).
const LOGO_MAX_H: usize = 220;

#[derive(Clone)]
struct LogoCache {
    handle: TextureHandle,
    size: [usize; 2],
    modified: Option<std::time::SystemTime>,
    checked: std::time::Instant,
}

/// The Fluttersine logo texture and its pixel size (for the aspect ratio).
pub fn fluttersine_logo(ctx: &Context) -> (egui::TextureId, [usize; 2]) {
    live_logo(ctx, "fluttersine-logo", FLUTTERSINE_LOGO_PNG, FLUTTERSINE_LOGO_FILE)
}

/// The Chrysalis emblem texture and its pixel size.
pub fn chrysalis_emblem(ctx: &Context) -> (egui::TextureId, [usize; 2]) {
    live_logo(ctx, "chrysalis-emblem", CHRYSALIS_EMBLEM_PNG, CHRYSALIS_EMBLEM_FILE)
}

/// A piece of artwork: `file` if it is there (re-read when it changes), else `built_in`.
fn live_logo(ctx: &Context, name: &str, built_in: &[u8], file: &str) -> (egui::TextureId, [usize; 2]) {
    let id = egui::Id::new(name);
    // Copy the cache out first: egui's context lock is not re-entrant.
    let cached = ctx.data(|d| d.get_temp::<LogoCache>(id));
    let now = std::time::Instant::now();
    if let Some(c) = &cached {
        if now.duration_since(c.checked) < std::time::Duration::from_secs(1) {
            return (c.handle.id(), c.size);
        }
    }
    ctx.request_repaint_after(std::time::Duration::from_secs(1));
    let modified = std::fs::metadata(file).and_then(|m| m.modified()).ok();
    if let Some(c) = cached.clone().filter(|c| c.modified == modified) {
        ctx.data_mut(|d| d.insert_temp(id, LogoCache { checked: now, ..c.clone() }));
        return (c.handle.id(), c.size);
    }
    // New or changed: decode the file if it's there (and readable), else the built-in copy.
    let from_file = modified.and_then(|_| std::fs::read(file).ok()).and_then(|b| decode_logo(&b));
    let image =
        from_file.or_else(|| decode_logo(built_in)).unwrap_or_else(|| ColorImage::new([1, 1], Color32::TRANSPARENT));
    let size = image.size;
    let handle = match cached {
        Some(mut c) => {
            c.handle.set(image, egui::TextureOptions::LINEAR);
            c.handle
        }
        None => ctx.load_texture(name, image, egui::TextureOptions::LINEAR),
    };
    ctx.data_mut(|d| d.insert_temp(id, LogoCache { handle: handle.clone(), size, modified, checked: now }));
    (handle.id(), size)
}

/// PNG → RGBA, trimmed to its visible pixels and scaled down to `LOGO_MAX_H`.
fn decode_logo(bytes: &[u8]) -> Option<ColorImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let px: Vec<[u8; 4]> = match info.color_type {
        png::ColorType::Rgba => buf.as_chunks::<4>().0.to_vec(),
        png::ColorType::Rgb => buf.as_chunks::<3>().0.iter().map(|&[r, g, b]| [r, g, b, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.as_chunks::<2>().0.iter().map(|&[g, a]| [g, g, g, a]).collect(),
        png::ColorType::Grayscale => buf.iter().map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    let px = &px[..w * h];
    // Trim transparent margins.
    let visible = |x: usize, y: usize| px[y * w + x][3] > 8;
    let rows: Vec<usize> = (0..h).filter(|&y| (0..w).any(|x| visible(x, y))).collect();
    let cols: Vec<usize> = (0..w).filter(|&x| (0..h).any(|y| visible(x, y))).collect();
    let (&y0, &y1, &x0, &x1) = (rows.first()?, rows.last()?, cols.first()?, cols.last()?);
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    // Area-average down to LOGO_MAX_H (premultiplied, so edges don't go dark).
    let (tw, th) = if ch > LOGO_MAX_H { ((cw * LOGO_MAX_H / ch).max(1), LOGO_MAX_H) } else { (cw, ch) };
    let mut out = Vec::with_capacity(tw * th * 4);
    for ty in 0..th {
        let (sy0, sy1) = (y0 + ty * ch / th, y0 + ((ty + 1) * ch / th).max(ty * ch / th + 1));
        for tx in 0..tw {
            let (sx0, sx1) = (x0 + tx * cw / tw, x0 + ((tx + 1) * cw / tw).max(tx * cw / tw + 1));
            let mut acc = [0u32; 4];
            for y in sy0..sy1 {
                for x in sx0..sx1 {
                    let [r, g, b, a] = px[y * w + x];
                    let a32 = a as u32;
                    acc[0] += r as u32 * a32;
                    acc[1] += g as u32 * a32;
                    acc[2] += b as u32 * a32;
                    acc[3] += a32;
                }
            }
            let n = ((sx1 - sx0) * (sy1 - sy0)) as u32;
            let a = acc[3].max(1);
            out.extend_from_slice(&[(acc[0] / a) as u8, (acc[1] / a) as u8, (acc[2] / a) as u8, (acc[3] / n) as u8]);
        }
    }
    Some(ColorImage::from_rgba_unmultiplied([tw, th], &out))
}


// ── Style and fonts ──
const FONT: &str = "plugin";
const FONT_MONO: &str = "plugin-mono";
static STYLE: OnceLock<Arc<egui::Style>> = OnceLock::new();

/// The synths' font families: copies of egui's defaults. Call once at startup.
pub fn install_fonts(ctx: &Context) {
    let mut defs = egui::FontDefinitions::default();
    let prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    defs.families.insert(FontFamily::Name(FONT.into()), prop);
    defs.families.insert(FontFamily::Name(FONT_MONO.into()), mono);
    ctx.set_fonts(defs);
}

/// Keep today's style (call right after `theme::apply` at startup), with the
/// text styles moved onto the synths' own font families.
pub fn capture_style(ctx: &Context) {
    let mut style = (*ctx.style()).clone();
    for font in style.text_styles.values_mut() {
        font.family = if font.family == FontFamily::Monospace { FontFamily::Name(FONT_MONO.into()) } else { FontFamily::Name(FONT.into()) };
    }
    let _ = STYLE.set(Arc::new(style));
}

/// Put the synths' frozen style on `ui` (and everything inside it).
pub fn freeze(ui: &mut Ui) {
    if let Some(s) = STYLE.get() {
        ui.set_style(s.clone());
    }
}

/// The synths' proportional font at `size`.
pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FONT.into()))
}
