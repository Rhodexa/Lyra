//! Lyra's look, in one place.
//!
//! The spec is docs/design/catalog-next.html (its "Settled" index): neutral
//! charcoal surfaces on a dark ground, flat plates with a dark outline in their
//! own colour, backlit "on", the dim LCD for things you change or that pop up,
//! Roboto Condensed for labels and Roboto for reading text. The "Foundations"
//! section below holds its tokens; older constants are being replaced widget by
//! widget during the port. Fluttersine and Chrysalis don't use this module
//! (they have their own frozen look in plugin_look.rs).

use egui::{Color32, Context, FontFamily, FontId, Painter, Rect, Stroke, TextStyle};

// ── Two levels: background and surface, both neutral charcoal ──
// The keys from the studies are neutral greys, so every surface is too: a
// tinted surface next to a neutral key looked "off".
/// The background: the workspace, the rack frame behind toolbar modules and
/// mixer strips (seen as thin seams between them).
pub const BG: Color32 = Color32::from_rgb(0x16, 0x16, 0x16);
/// The surface: windows, toolbar modules, mixer strips.
pub const PANEL: Color32 = Color32::from_rgb(0x29, 0x29, 0x29);
pub const PANEL_HI: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
/// Toolbars and status bar sit on the background.
pub const TOOLBAR: Color32 = BG;
/// Hairline between rack modules / mixer strips.
pub const SEAM: f32 = 2.0;
/// Small flat buttons (M / S, menus).
pub const RAISED: Color32 = Color32::from_rgb(0x3b, 0x3b, 0x3b);
pub const INSET: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
/// Recessed wells: dropdowns, displays.
pub const WELL: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
pub const WELL_EDGE: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
/// Borders and details are *darker* than the surfaces they sit on (engraved,
/// not glowing). Only selected/active things get bright edges.
pub const LINE: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);

// ── Text ──
pub const TEXT: Color32 = Color32::from_rgb(0xcf, 0xcf, 0xcf);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x96, 0x96, 0x96);

// ── Accents (the lights' mid tones; see `Light` below) ──
pub const MINT: Color32 = Color32::from_rgb(0xa6, 0xe7, 0xc9);
pub const MINT_DEEP: Color32 = Color32::from_rgb(0x55, 0x79, 0x6b);
/// Lyra's eyes. Use sparingly.
pub const GOLD: Color32 = Color32::from_rgb(0xf5, 0xdb, 0xa5);
pub const ROSE: Color32 = Color32::from_rgb(0xff, 0xc5, 0xdf);
pub const LAVENDER: Color32 = Color32::from_rgb(0xdc, 0xc6, 0xfa);

/// A channel's colour (its light's mid tone), cycled.
pub fn channel_color(i: usize) -> Color32 {
    channel_light(i).mid
}

// ── Piano roll ──
pub const ROLL_WHITE_ROW: Color32 = Color32::from_rgb(0x28, 0x28, 0x28);
pub const ROLL_BLACK_ROW: Color32 = Color32::from_rgb(0x21, 0x21, 0x21);
/// Rows outside the song's scale (when highlighting is on).
pub const ROLL_OUT_ROW: Color32 = Color32::from_rgb(0x1b, 0x1b, 0x1b);
pub const KEY_WHITE: Color32 = Color32::from_rgb(0xc9, 0xc9, 0xc9);
pub const KEY_BLACK: Color32 = Color32::from_rgb(0x1d, 0x1d, 0x1d);
pub const KEY_EDGE: Color32 = Color32::from_rgb(0x94, 0x94, 0x94);

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

// ── Sizes ──────────────────────────────────────────────────────────────────
//
// Every size that shapes how Lyra *feels* lives in `Metrics`, so it can be
// tuned live in the Design panel (F12) and copied back here as code.

/// All the sizes, in points (1 point = 1 pixel at 100% zoom).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub text: f32,
    pub text_small: f32,
    /// Height of buttons, menus, number boxes.
    pub control_h: f32,
    pub toolbar_h: f32,
    /// Inner padding of toolbar modules.
    pub module_pad: f32,
    /// Space between toolbar modules.
    pub module_gap: f32,
    pub corner: f32,
    /// Knobs in the channel rack / in the instrument editor.
    /// The toolbar's buttons (square, icon ones).
    pub tool_key: f32,
    pub knob_small: f32,
    pub knob_large: f32,
    pub led: f32,
    pub step_w: f32,
    pub step_h: f32,
    pub name_w: f32,
    /// Piano roll: overview scrollbar, ruler, keyboard width, row height.
    pub overview_h: f32,
    pub ruler_h: f32,
    pub keys_w: f32,
    pub row_h: f32,
    /// How far from a note's right end (inside it) a drag resizes instead of moving.
    pub edge_grab: f32,
    /// …and how far *past* the end it still counts.
    pub edge_reach: f32,
}

impl Metrics {
    pub const DEFAULT: Metrics = Metrics {
        text: 14.0,
        text_small: 11.0,
        control_h: 28.0,
        toolbar_h: 64.0,
        module_pad: 6.0,
        module_gap: 8.0,
        corner: 5.0,
        tool_key: 40.0,
        knob_small: 28.0,
        knob_large: 40.0,
        led: 18.0,
        step_w: 24.0,
        step_h: 30.0,
        name_w: 132.0,
        overview_h: 34.0,
        ruler_h: 24.0,
        keys_w: 64.0,
        row_h: 16.0,
        edge_grab: 10.0,
        edge_reach: 8.0,
    };

    /// The metrics as Rust source, for pasting back into `Metrics::DEFAULT`.
    pub fn as_code(&self) -> String {
        // Debug's pretty form of a plain struct is already a valid Rust literal.
        format!("pub const DEFAULT: Metrics = {self:#?};")
    }
}

static METRICS: std::sync::RwLock<Metrics> = std::sync::RwLock::new(Metrics::DEFAULT);

/// Current sizes (UI thread; cheap copy).
pub fn m() -> Metrics {
    *METRICS.read().unwrap_or_else(|e| e.into_inner())
}

pub fn set_metrics(ctx: &Context, metrics: Metrics) {
    *METRICS.write().unwrap_or_else(|e| e.into_inner()) = metrics;
    apply(ctx);
}

// ── Foundations (catalog-next.html, settled 2026-11-25) ─────────────────────
//
// Widgets ask for a role, never a colour. Separation lines inside panels are the
// ground colour; a control's outline is its own colour mixed toward black.

/// Behind everything; shows only in the gaps between panels.
pub const GROUND: Color32 = BG;
/// Panels, strips, plates.
pub const SURFACE: Color32 = PANEL;
/// A selected surface (a strip, a row).
#[allow(dead_code)] // waiting for the port of its widgets (step 3)
pub const SURFACE_SEL: Color32 = PANEL_HI;
/// A control's body: the button plate.
pub const PLATE: Color32 = Color32::from_rgb(0x45, 0x45, 0x45);
/// Wells: dropdowns, fields, tracks.
pub const SUNKEN: Color32 = WELL;
pub const INK: Color32 = TEXT;
pub const INK_DIM: Color32 = TEXT_DIM;
/// Idle legends on plates.
pub const LEGEND: Color32 = Color32::from_rgb(0xb0, 0xb0, 0xb0);
/// Disabled.
pub const INK_FAINT: Color32 = Color32::from_rgb(0x5c, 0x5c, 0x5c);
/// Text on a lit body or an LCD.
#[allow(dead_code)] // waiting for the port of its widgets (step 3)
pub const INK_ON_LIGHT: Color32 = Color32::from_rgb(0x2c, 0x2c, 0x2c);
/// The dim LCD (settled): pale panel, dark ink.
pub const LCD_DIM: Color32 = LCD;

/// Sizes: S · M · L. 28 is the floor for anything drawn or clicked.
#[allow(dead_code)] // waiting for the port of its widgets (step 3)
pub const SIZE_S: f32 = 28.0;
pub const SIZE_M: f32 = 40.0;
#[allow(dead_code)] // waiting for the port of its widgets (step 3)
pub const SIZE_L: f32 = 52.0;
/// Outline width of every control.
pub const OUTLINE_W: f32 = 2.0;
/// Corner radius of a control at M (S × 0.75, L × 1.25).
pub const CORNER: f32 = 4.0;

/// A control's outline: its own colour, half as bright.
pub fn outline_of(c: Color32) -> Color32 {
    mix(c, Color32::BLACK, 0.5)
}

/// A flat body with its outline: the outline shape first, then the fill inside
/// it, never one on top of the other (no light fringe at rounded corners).
pub fn outlined(p: &Painter, r: Rect, radius: f32, fill: Color32) {
    p.rect_filled(r, cr(radius.round() as u8), outline_of(fill));
    let inner = (radius - OUTLINE_W).max(0.0);
    p.rect_filled(r.shrink(OUTLINE_W), cr(inner.round() as u8), fill);
}

// ── Fonts ──
// Roboto Condensed for labels (buttons, tabs, caps labels: it fits the most and
// stays readable), Roboto for reading text, egui's monospace for numbers. Both
// are bundled (SIL OFL, assets/fonts/OFL.txt), so Lyra looks the same everywhere.
const LABEL: &str = "label";
const LABEL_BOLD: &str = "label-bold";
const READ_BOLD: &str = "read-bold";

/// The label font (Roboto Condensed SemiBold). Labels are written in caps.
pub fn label(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(LABEL.into()))
}

/// The bold label font (Roboto Condensed Bold): a backlit "on" legend.
pub fn label_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(LABEL_BOLD.into()))
}

/// Reading text (Roboto).
pub fn read(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Bold reading text (Roboto SemiBold).
pub fn read_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(READ_BOLD.into()))
}

/// Lyra's fonts, with the synths' families (copies of egui's own defaults)
/// kept as they were. Call once at startup.
pub fn install_fonts(ctx: &Context) {
    use egui::{FontData, FontDefinitions};
    use std::sync::Arc;
    let mut defs = FontDefinitions::default();
    crate::plugin_look::add_font_families(&mut defs);
    let fonts: [(&str, &'static [u8]); 4] = [
        ("Roboto-Regular", include_bytes!("../assets/fonts/Roboto-Regular.ttf")),
        ("Roboto-SemiBold", include_bytes!("../assets/fonts/Roboto-SemiBold.ttf")),
        ("RobotoCondensed-SemiBold", include_bytes!("../assets/fonts/RobotoCondensed-SemiBold.ttf")),
        ("RobotoCondensed-Bold", include_bytes!("../assets/fonts/RobotoCondensed-Bold.ttf")),
    ];
    for (name, bytes) in fonts {
        defs.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    // egui's own fonts stay behind ours as fallbacks (emoji, symbols Roboto lacks).
    let fallback = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let with = |first: &str| std::iter::once(first.to_string()).chain(fallback.iter().cloned()).collect::<Vec<_>>();
    defs.families.insert(FontFamily::Proportional, with("Roboto-Regular"));
    defs.families.insert(FontFamily::Name(READ_BOLD.into()), with("Roboto-SemiBold"));
    defs.families.insert(FontFamily::Name(LABEL.into()), with("RobotoCondensed-SemiBold"));
    defs.families.insert(FontFamily::Name(LABEL_BOLD.into()), with("RobotoCondensed-Bold"));
    ctx.set_fonts(defs);
}

/// A DAW is a cockpit: dense, but every target comfortably hittable.
pub fn apply(ctx: &Context) {
    let m = m();
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(m.text_small)),
        (TextStyle::Body, FontId::proportional(m.text)),
        (TextStyle::Button, FontId::proportional(m.text)),
        (TextStyle::Heading, FontId::proportional(m.text + 3.0)),
        (TextStyle::Monospace, FontId::monospace(m.text - 0.5)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(5.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.interact_size = egui::vec2(m.control_h, m.control_h);
    style.spacing.combo_height = 320.0;
    // Panel and window edges are easier to grab, and the docked panels' grab
    // zone now reaches their content (padding is 8 pt): aiming at the split
    // line can't land in the arranger and place a clip by accident.
    style.interaction.resize_grab_radius_side = 9.0;
    // Scroll bars: Lyra's own (a recessed well, a flat thumb), always there
    // and always the same size; egui's default ones hide, shrink and flash.
    style.spacing.scroll = egui::style::ScrollStyle {
        // 20 px in all, like the catalog's plain bar (16 + 2 + 2).
        bar_width: 16.0,
        handle_min_length: 28.0,
        bar_inner_margin: 2.0,
        bar_outer_margin: 2.0,
        dormant_background_opacity: 1.0,
        active_background_opacity: 1.0,
        interact_background_opacity: 1.0,
        dormant_handle_opacity: 1.0,
        active_handle_opacity: 1.0,
        interact_handle_opacity: 1.0,
        ..egui::style::ScrollStyle::solid()
    };

    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = INSET;
    v.faint_bg_color = PANEL_HI;
    v.window_stroke = stroke(1.0, LINE);
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    v.selection.bg_fill = MINT_DEEP;
    v.selection.stroke = stroke(1.0, MINT);
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.bg_stroke = stroke(1.0, LINE);
    // Plain buttons: the small flat M / S buttons from the mixer study.
    let edge = stroke(1.0, Color32::from_rgb(0x1b, 0x1b, 0x1b));
    let corner = cr(4);
    for (w, fill) in [
        (&mut v.widgets.inactive, RAISED),
        (&mut v.widgets.hovered, Color32::from_rgb(0x44, 0x44, 0x44)),
        (&mut v.widgets.active, MINT_DEEP),
        (&mut v.widgets.open, Color32::from_rgb(0x44, 0x44, 0x44)),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = edge;
        w.corner_radius = corner;
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.corner_radius = corner;
    v.slider_trailing_fill = true;
    ctx.set_style(style);
}

/// The window icon: raw RGBA pixels, pre-converted from
/// `docs/artwork/lyra/logo_simple_color.png` so the app needs no
/// image-decoding library. Regenerate with Pillow if the logo changes: trim to
/// the drawing, pad square, then `.convert("RGBA").resize((128, 128),
/// Image.LANCZOS).tobytes()` (and 256 as `icon_256.png` for
/// `lyra --install-desktop`).
pub const ICON_128: &[u8] = include_bytes!("../assets/icon_128.rgba");

// ── Material: the control library ───────────────────────────────────────────
//
// Ported from the user's HTML studies (docs/design/HTMLSamples15112026, the
// latest of each: control-library-mint-v2, multicolor-button-rack,
// single-channel-strip-v2). Everything is seen straight from the top, lit
// uniformly from above:
//
// * **Silicone key**: a thin dark recess, a bezel pad (brighter at the top,
//   a constant band through the middle, darker at the bottom), and a flat
//   face. Lit, the face glows with a soft radial light in its own colour
//   and the pad takes the light's rim colour. Zero gloss.
// * **LED**: round, recessed; lit, the well is tinted by its own light.
// * **LCD**: a pale backlit panel with dark ink.
//
// Meshes here have no anti-aliasing of their own, so each gradient sits on
// an anti-aliased plain shape of the same outline, which smooths its edge.

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

/// A colour from 0xRRGGBB.
pub const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
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

/// Channel lights, cycled. The first six keep the order of the old channel
/// colours, so saved projects keep their look.
pub const LIGHTS: [Light; 8] = [L_MINT, L_PINK, L_BLUE, L_LAVENDER, L_PEACH, L_AMBER, L_CYAN, L_LIME];

pub fn channel_light(i: usize) -> Light {
    LIGHTS[i % LIGHTS.len()]
}

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

/// Corner radii for the left pair and the right pair of corners. Keys that
/// sit flush against a neighbour (radio keys) square off the touching side.
#[derive(Clone, Copy, Debug)]
pub struct Sides {
    pub left: f32,
    pub right: f32,
}

impl Sides {
    /// Keep `left` / `right` rounded, square the other side.
    pub fn only(r: f32, left: bool, right: bool) -> Self {
        Sides { left: if left { r } else { 0.0 }, right: if right { r } else { 0.0 } }
    }
    fn clamped(self, r: Rect) -> Self {
        let max = (r.width() * 0.5).min(r.height() * 0.5);
        Sides { left: self.left.clamp(0.0, max), right: self.right.clamp(0.0, max) }
    }
    fn shrunk(self, d: f32) -> Self {
        Sides { left: (self.left - d).max(0.0), right: (self.right - d).max(0.0) }
    }
    fn corners(self) -> egui::CornerRadius {
        let (l, r) = (self.left.round() as u8, self.right.round() as u8);
        egui::CornerRadius { nw: l, sw: l, ne: r, se: r }
    }
}

/// Left/right extent of a rounded rect at height `y`.
fn rounded_span(r: Rect, rad: Sides, y: f32) -> (f32, f32) {
    let inset = |rad: f32| {
        let dy = if y < r.top() + rad {
            r.top() + rad - y
        } else if y > r.bottom() - rad {
            y - (r.bottom() - rad)
        } else {
            0.0
        };
        rad - (rad * rad - dy.min(rad).powi(2)).max(0.0).sqrt()
    };
    (r.left() + inset(rad.left), r.right() - inset(rad.right))
}

/// A rounded rect filled with a vertical gradient (exact at every stop).
pub fn vgradient_sides(p: &Painter, r: Rect, rad: Sides, stops: &[(f32, Color32)]) {
    let rad = rad.clamped(r);
    // Anti-aliased base in the middle colour hides the mesh's hard edge.
    p.rect_filled(r, rad.corners(), ramp(stops, 0.5));
    let r = r.shrink(0.4);
    let rad = rad.shrunk(0.4);
    let mut ys: Vec<f32> = Vec::with_capacity(32);
    const ARC: usize = 5;
    for radius in [rad.left, rad.right] {
        for i in 0..=ARC {
            let f = i as f32 / ARC as f32;
            ys.push(r.top() + radius * (1.0 - (f * std::f32::consts::FRAC_PI_2).cos()));
            ys.push(r.bottom() - radius * (1.0 - (f * std::f32::consts::FRAC_PI_2).cos()));
        }
    }
    ys.extend(stops.iter().map(|(t, _)| r.top() + t * r.height()));
    ys.retain(|y| (r.top()..=r.bottom()).contains(y));
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    let mut mesh = egui::Mesh::default();
    for &y in &ys {
        let (l, rt) = rounded_span(r, rad, y);
        let c = ramp(stops, (y - r.top()) / r.height().max(1.0));
        mesh.colored_vertex(egui::pos2(l, y), c);
        mesh.colored_vertex(egui::pos2(rt, y), c);
    }
    for i in 0..ys.len().saturating_sub(1) as u32 {
        let (a, b) = (2 * i, 2 * i + 2);
        mesh.add_triangle(a, a + 1, b + 1);
        mesh.add_triangle(a, b + 1, b);
    }
    p.add(egui::Shape::mesh(mesh));
}

/// Outline of a rounded rect, clockwise from the left end of the top edge.
fn rounded_loop(r: Rect, rad: Sides, arc: usize) -> Vec<egui::Pos2> {
    use std::f32::consts::{FRAC_PI_2, PI};
    let mut out = Vec::with_capacity(4 * (arc + 1) + 4);
    let (l, rr) = (rad.left, rad.right);
    let corners = [
        (egui::pos2(r.left() + l, r.top() + l), PI, l),
        (egui::pos2(r.right() - rr, r.top() + rr), 1.5 * PI, rr),
        (egui::pos2(r.right() - rr, r.bottom() - rr), 0.0, rr),
        (egui::pos2(r.left() + l, r.bottom() - l), FRAC_PI_2, l),
    ];
    for (c, from, radius) in corners {
        for i in 0..=arc {
            let a = from + FRAC_PI_2 * i as f32 / arc as f32;
            out.push(c + egui::vec2(a.cos(), a.sin()) * radius);
        }
    }
    out.dedup_by(|a, b| a.distance(*b) < 0.01);
    // Extra points halfway along each side keep long sides smooth inside.
    let n = out.len();
    let mut with_mids = Vec::with_capacity(n * 2);
    for i in 0..n {
        let (a, b) = (out[i], out[(i + 1) % n]);
        with_mids.push(a);
        if a.distance(b) > 4.0 {
            with_mids.push(a.lerp(b, 0.5));
        }
    }
    with_mids
}

/// A rounded rect filled with an elliptical radial gradient, like CSS
/// `radial-gradient(ellipse rx% ry% at cx% cy%, stops…)`. `at` and `radii`
/// are fractions of the rect's size.
pub fn radial_rect(p: &Painter, r: Rect, radius: Sides, at: egui::Vec2, radii: egui::Vec2, stops: &[(f32, Color32)]) {
    let rad = radius.clamped(r);
    let focus = r.min + r.size() * at;
    let (rx, ry) = ((r.width() * radii.x).max(1.0), (r.height() * radii.y).max(1.0));
    let color = |q: egui::Pos2| ramp(stops, (((q.x - focus.x) / rx).powi(2) + ((q.y - focus.y) / ry).powi(2)).sqrt());
    let outline = rounded_loop(r, rad, 5);
    p.rect_filled(r, rad.corners(), color(outline[0]));
    radial_fan(p, focus, outline.iter().map(|q| *q + (focus - *q).normalized() * 0.4), color);
}

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

/// Unlit key greys (neutral, from the studies).
pub const KEY_RECESS: Color32 = hex(0x141414);
const PAD_TOP: Color32 = hex(0x494949);
const PAD_MID: Color32 = hex(0x383838);
const PAD_BOTTOM: Color32 = hex(0x292929);
pub const KEY_FACE: Color32 = hex(0x292929);

/// Size of each layer for a key whose short side is `s` points.
struct KeyGeom {
    recess: f32,
    pad: f32,
    r_out: f32,
    r_pad: f32,
    r_face: f32,
}

fn key_geom(s: f32) -> KeyGeom {
    let recess = (s * 0.03).clamp(1.0, 2.0);
    let pad = (s * 0.045).clamp(1.5, 5.0);
    let r_out = (s * 0.12 + 1.5).min(11.0);
    let r_pad = (r_out - recess).max(1.0);
    KeyGeom { recess, pad, r_out, r_pad, r_face: (r_pad - pad * 0.8).max(1.5) }
}

/// The face rect of a key (where its icon or legend goes).
pub fn key_face(rect: Rect) -> Rect {
    let g = key_geom(rect.width().min(rect.height()));
    rect.shrink(g.recess + g.pad)
}

/// A silicone key. `light` = lit in that colour; `hot` = a touch brighter
/// (hover, or the step under the playhead); `tone` scales the unlit greys
/// (1.0 = normal; steps use it to group beats).
pub fn key(p: &Painter, rect: Rect, light: Option<Light>, hot: bool, tone: f32) {
    let g = key_geom(rect.width().min(rect.height()));
    p.rect_filled(rect, cr(g.r_out.round() as u8), KEY_RECESS);
    let pad = rect.shrink(g.recess);
    key_layers(p, pad, pad.shrink(g.pad), &g, (true, true), light, hot, tone);
}

/// Pad and face of a key (everything inside the recess). `round` says which
/// sides (left, right) keep their rounded corners.
#[allow(clippy::too_many_arguments)]
fn key_layers(
    p: &Painter,
    pad: Rect,
    face: Rect,
    g: &KeyGeom,
    (l, r): (bool, bool),
    light: Option<Light>,
    hot: bool,
    tone: f32,
) {
    let r_pad = Sides::only(g.r_pad, l, r);
    let r_face = Sides::only(g.r_face, l, r);
    match light {
        Some(lt) => {
            let lt = if hot { lt.scaled(1.06) } else { lt };
            vgradient_sides(p, pad, r_pad, &[(0.0, lt.rim), (0.12, lt.edge), (0.88, lt.edge), (1.0, lt.rim)]);
            radial_rect(
                p,
                face,
                r_face,
                egui::vec2(0.5, 0.49),
                egui::vec2(0.85, 0.82),
                &[(0.0, lt.hi), (0.38, lt.mid), (0.76, lt.low), (1.0, lt.edge)],
            );
        }
        None => {
            let k = tone * if hot { 1.14 } else { 1.0 };
            let (top, mid, bottom) = (shade(PAD_TOP, k), shade(PAD_MID, k), shade(PAD_BOTTOM, k));
            vgradient_sides(p, pad, r_pad, &[(0.0, top), (0.12, mid), (0.88, mid), (1.0, bottom)]);
            p.rect_filled(face, r_face.corners(), shade(KEY_FACE, k));
        }
    }
}

/// A flat key for dense grids (the channel rack's steps): the dark recess
/// outline and a flat fill, no gradients. Lit = the light's flat colour.
pub fn flat_key(p: &Painter, rect: Rect, light: Option<Light>, hot: bool, tone: f32) {
    let r_out = (rect.width().min(rect.height()) * 0.12 + 1.5).min(6.0);
    p.rect_filled(rect, cr(r_out.round() as u8), KEY_RECESS);
    let fill = match light {
        Some(l) => {
            if hot {
                shade(l.mid, 1.05)
            } else {
                l.mid
            }
        }
        None => shade(hex(0x303030), tone * if hot { 1.15 } else { 1.0 }),
    };
    p.rect_filled(rect.shrink(1.0), cr((r_out - 1.0).max(1.0).round() as u8), fill);
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
/// The dim LCD (catalog-next, settled): easier on the eyes than the old #dadada.
pub const LCD: Color32 = hex(0xbcbcbc);
pub const LCD_INK: Color32 = hex(0x2f2f2f);
pub const LCD_INK_DIM: Color32 = hex(0x4e4e4e);

/// The LCD panel itself (text goes on top): the dim screen, an outline the ink
/// darkened (darker than any surface), a corner radius that adds back the
/// outline's width, and an inner shadow along the top, so it sits *in* the panel.
pub fn lcd_panel(p: &Painter, r: Rect) {
    let k = (r.height() / SIZE_M).clamp(0.7, 1.3);
    let rad = CORNER * k + OUTLINE_W;
    p.rect_filled(r, cr(rad as u8), mix(LCD_INK, Color32::BLACK, 0.5));
    let inner = r.shrink(OUTLINE_W);
    p.rect_filled(inner, cr((rad - OUTLINE_W) as u8), LCD);
    for (d, a) in [(0.0, 0.36), (1.0, 0.22), (2.0, 0.11), (3.0, 0.04)] {
        let line = Rect::from_min_size(inner.min + egui::vec2(rad * 0.5, d), egui::vec2(inner.width() - rad, 1.0));
        p.rect_filled(line, cr(0), mix(LCD, Color32::BLACK, a));
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

// ── Clips on the arrangement ──
// Tuned by hand (Rhodexa): big areas of the pastel lights shout, so bodies
// are a faint tint and title strips a darkened light, with light ink.

/// A clip body's tint: the clip's light at this opacity.
pub const CLIP_BODY: f32 = 0.03;
/// The current clip (the one the next click places a copy of).
pub const CLIP_BODY_CURRENT: f32 = 0.10;
/// Added while the playhead is in the clip.
pub const CLIP_BODY_PLAYING: f32 = 0.08;
/// The outline's opacity (0 = no outline).
pub const CLIP_EDGE: f32 = 0.0;

/// A clip's title strip.
pub fn clip_strip(light: Light, playing: bool) -> Color32 {
    shade(light.mid, if playing { 0.6 } else { 0.5 })
}

/// The name on a clip's title strip.
pub fn clip_ink(light: Light) -> Color32 {
    light.hi
}
