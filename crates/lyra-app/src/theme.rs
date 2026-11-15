//! Lyra's look, in one place.
//!
//! Flat & minimal, but never plastic: everything sits on a faint grain, the
//! workspace carries a barely-there seigaiha wave pattern, and colour comes
//! from pastels with mint as the hero. No wood, no chrome.
//!
//! The textures here are generated in code as *placeholders*. Swapping them
//! for hand-made art later means replacing `grain_image` / `pattern_image`
//! with loaded PNGs — nothing else changes.

use egui::{Color32, ColorImage, Context, FontId, Painter, Rect, Stroke, TextStyle, TextureHandle};

// ── Surfaces: charcoal, the faintest teal ──
// From the control-library studies; the background is nudged from the
// studies' blue-slate towards charcoal, as the user asked.
pub const BG: Color32 = Color32::from_rgb(0x1b, 0x1f, 0x21);
pub const PANEL: Color32 = Color32::from_rgb(0x25, 0x2a, 0x2d);
pub const PANEL_HI: Color32 = Color32::from_rgb(0x2b, 0x31, 0x34);
/// The rack frame things are mounted in: toolbar background, gaps between
/// mixer strips.
pub const CHASSIS: Color32 = Color32::from_rgb(0x11, 0x16, 0x18);
/// Toolbars and status bar.
pub const TOOLBAR: Color32 = CHASSIS;
/// Small flat buttons (M / S, menus).
pub const RAISED: Color32 = Color32::from_rgb(0x34, 0x3e, 0x40);
pub const INSET: Color32 = Color32::from_rgb(0x15, 0x1b, 0x1e);
/// Recessed wells: dropdowns, displays.
pub const WELL: Color32 = Color32::from_rgb(0x15, 0x1b, 0x20);
pub const WELL_EDGE: Color32 = Color32::from_rgb(0x10, 0x15, 0x19);
/// Borders and details are *darker* than the surfaces they sit on (engraved,
/// not glowing). Only selected/active things get bright edges.
pub const LINE: Color32 = Color32::from_rgb(0x0e, 0x12, 0x14);

// ── Text ──
pub const TEXT: Color32 = Color32::from_rgb(0xc9, 0xd2, 0xce);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8d, 0x9b, 0x96);

// ── Accents (the lights' mid tones; see `Light` below) ──
pub const MINT: Color32 = Color32::from_rgb(0xa6, 0xe7, 0xc9);
pub const MINT_DEEP: Color32 = Color32::from_rgb(0x55, 0x79, 0x6b);
/// Lyra's eyes. Use sparingly.
pub const GOLD: Color32 = Color32::from_rgb(0xf5, 0xdb, 0xa5);
pub const ROSE: Color32 = Color32::from_rgb(0xff, 0xc5, 0xdf);
pub const SKY: Color32 = Color32::from_rgb(0xbf, 0xce, 0xfb);
pub const LAVENDER: Color32 = Color32::from_rgb(0xdc, 0xc6, 0xfa);
pub const PEACH: Color32 = Color32::from_rgb(0xff, 0xd0, 0xb4);
pub const BUTTER: Color32 = Color32::from_rgb(0xf5, 0xdb, 0xa5);

/// A channel's colour (its light's mid tone), cycled.
pub fn channel_color(i: usize) -> Color32 {
    channel_light(i).mid
}

// ── Piano roll ──
pub const ROLL_WHITE_ROW: Color32 = Color32::from_rgb(0x24, 0x29, 0x2c);
pub const ROLL_BLACK_ROW: Color32 = Color32::from_rgb(0x1e, 0x22, 0x25);
/// Rows outside the song's scale (when highlighting is on).
pub const ROLL_OUT_ROW: Color32 = Color32::from_rgb(0x19, 0x1c, 0x1e);
pub const KEY_WHITE: Color32 = Color32::from_rgb(0xc3, 0xcb, 0xcc);
pub const KEY_BLACK: Color32 = Color32::from_rgb(0x1a, 0x1e, 0x20);
pub const KEY_EDGE: Color32 = Color32::from_rgb(0x8d, 0x97, 0x99);

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
        control_h: 26.0,
        toolbar_h: 60.0,
        module_pad: 6.0,
        module_gap: 8.0,
        corner: 5.0,
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
    let edge = stroke(1.0, Color32::from_rgb(0x16, 0x1d, 0x1f));
    let corner = cr(4);
    for (w, fill) in [
        (&mut v.widgets.inactive, RAISED),
        (&mut v.widgets.hovered, Color32::from_rgb(0x3b, 0x48, 0x49)),
        (&mut v.widgets.active, MINT_DEEP),
        (&mut v.widgets.open, Color32::from_rgb(0x3b, 0x48, 0x49)),
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

/// Icon fonts: Phosphor (MIT), bold for line icons and fill for solid ones.
/// Both weights share codepoints, so `egui_phosphor::bold::PLAY` works with either.
pub fn install_fonts(ctx: &Context) {
    use egui::FontFamily;
    let mut fonts = egui::FontDefinitions::default();
    for (name, variant) in [("ph-bold", egui_phosphor::Variant::Bold), ("ph-fill", egui_phosphor::Variant::Fill)] {
        fonts.font_data.insert(name.into(), std::sync::Arc::new(variant.font_data()));
        fonts.families.insert(FontFamily::Name(name.into()), vec![name.into()]);
    }
    ctx.set_fonts(fonts);
}

/// A Phosphor glyph at `size` points; `solid` picks the filled weight.
pub fn icon_font(size: f32, solid: bool) -> FontId {
    FontId::new(size, egui::FontFamily::Name(if solid { "ph-fill" } else { "ph-bold" }.into()))
}

/// The textures every surface is printed on, plus the logo.
pub struct Textures {
    grain: TextureHandle,
    pattern: TextureHandle,
    logo: TextureHandle,
}

/// Raw RGBA pixels, pre-converted from `assets/icon.png` so the app needs no
/// image-decoding library. Regenerate with Pillow if the icon changes:
/// `Image.open("assets/icon.png").convert("RGBA").resize((48, 48), Image.LANCZOS).tobytes()`
pub const LOGO_48: &[u8] = include_bytes!("../assets/logo_48.rgba");
pub const ICON_128: &[u8] = include_bytes!("../assets/icon_128.rgba");

const GRAIN: usize = 128;
/// Seigaiha scale radius in pixels.
const WAVE_R: usize = 22;

impl Textures {
    pub fn new(ctx: &Context) -> Self {
        let opts = egui::TextureOptions::NEAREST_REPEAT;
        Self {
            grain: ctx.load_texture("lyra-grain", grain_image(), opts),
            pattern: ctx.load_texture("lyra-seigaiha", pattern_image(), egui::TextureOptions::LINEAR_REPEAT),
            logo: ctx.load_texture(
                "lyra-logo",
                ColorImage::from_rgba_unmultiplied([48, 48], LOGO_48),
                egui::TextureOptions::LINEAR,
            ),
        }
    }

    pub fn logo(&self) -> egui::TextureId {
        self.logo.id()
    }

    /// Fine paper grain. Paint it *before* a surface's content.
    pub fn grain(&self, painter: &Painter, rect: Rect) {
        tile(painter, &self.grain, rect, GRAIN as f32, GRAIN as f32);
    }

    /// Faint wave pattern for big empty areas (the workspace).
    pub fn pattern(&self, painter: &Painter, rect: Rect) {
        tile(painter, &self.pattern, rect, (WAVE_R * 2) as f32, WAVE_R as f32);
    }
}

/// Tiles a texture over `rect`, anchored to the rect's own corner — so each
/// surface has its own grain. (Anchoring to the screen made a window's grain line
/// up with the background's, which read as "the window is see-through".)
fn tile(painter: &Painter, tex: &TextureHandle, rect: Rect, w: f32, h: f32) {
    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(rect.width() / w, rect.height() / h));
    painter.image(tex.id(), rect, uv, Color32::WHITE);
}

/// Placeholder: mostly dark specks, a few faint light ones.
fn grain_image() -> ColorImage {
    let mut rng = 0x9e3779b9u32;
    let mut px = Vec::with_capacity(GRAIN * GRAIN * 4);
    for _ in 0..GRAIN * GRAIN {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        let v = (rng >> 24) as u8;
        let (c, a) = if v.is_multiple_of(4) { (255, v / 64) } else { (0, v / 26) };
        px.extend_from_slice(&[c, c, c, a]);
    }
    ColorImage::from_rgba_unmultiplied([GRAIN, GRAIN], &px)
}

/// Placeholder: seigaiha (overlapping wave scales), computed per pixel.
/// Rows of circles every R/2, alternate rows shifted by R; lower rows cover upper ones.
fn pattern_image() -> ColorImage {
    let r = WAVE_R as f32;
    let (w, h) = (WAVE_R * 2, WAVE_R);
    let mut px = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut alpha = 0u8;
            // Candidate rows whose circle top can reach this pixel, nearest (topmost-drawn) first.
            let k_max = ((fy + r) / (r / 2.0)).floor() as i32;
            let k_min = (fy / (r / 2.0)).ceil() as i32;
            for k in (k_min..=k_max).rev() {
                let cy = k as f32 * r / 2.0;
                let cx_off = if k.rem_euclid(2) == 1 { r } else { 0.0 };
                let mut dx = (fx - cx_off).rem_euclid(2.0 * r);
                if dx > r {
                    dx -= 2.0 * r;
                }
                let d = (dx * dx + (fy - cy) * (fy - cy)).sqrt();
                if d <= r {
                    // Three concentric rings per scale.
                    let ring = (d / (r / 3.0)).fract();
                    alpha = if ring > 0.9 || d > r - 1.0 { 26 } else { 0 };
                    break;
                }
            }
            // Dark engraving, not light lines: present when you look, gone when you don't.
            px.extend_from_slice(&[0, 0, 0, alpha]);
        }
    }
    ColorImage::from_rgba_unmultiplied([w, h], &px)
}

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

const fn hex(v: u32) -> Color32 {
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

/// Left/right extent of a rounded rect at height `y`.
fn rounded_span(r: Rect, rad: f32, y: f32) -> (f32, f32) {
    let dy = if y < r.top() + rad {
        r.top() + rad - y
    } else if y > r.bottom() - rad {
        y - (r.bottom() - rad)
    } else {
        0.0
    };
    let inset = rad - (rad * rad - dy.min(rad).powi(2)).max(0.0).sqrt();
    (r.left() + inset, r.right() - inset)
}

/// A rounded rect filled with a vertical gradient (exact at every stop).
pub fn vgradient(p: &Painter, r: Rect, radius: f32, stops: &[(f32, Color32)]) {
    let rad = radius.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0);
    // Anti-aliased base in the middle colour hides the mesh's hard edge.
    p.rect_filled(r, cr(rad.round() as u8), ramp(stops, 0.5));
    let r = r.shrink(0.4);
    let rad = (rad - 0.4).max(0.0);
    let mut ys: Vec<f32> = Vec::with_capacity(24);
    const ARC: usize = 5;
    for i in 0..=ARC {
        let f = i as f32 / ARC as f32;
        ys.push(r.top() + rad * (1.0 - (f * std::f32::consts::FRAC_PI_2).cos()));
        ys.push(r.bottom() - rad * (1.0 - (f * std::f32::consts::FRAC_PI_2).cos()));
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
fn rounded_loop(r: Rect, rad: f32, arc: usize) -> Vec<egui::Pos2> {
    use std::f32::consts::{FRAC_PI_2, PI};
    let mut out = Vec::with_capacity(4 * (arc + 1) + 4);
    let corners = [
        (egui::pos2(r.left() + rad, r.top() + rad), PI),
        (egui::pos2(r.right() - rad, r.top() + rad), 1.5 * PI),
        (egui::pos2(r.right() - rad, r.bottom() - rad), 0.0),
        (egui::pos2(r.left() + rad, r.bottom() - rad), FRAC_PI_2),
    ];
    for (c, from) in corners {
        for i in 0..=arc {
            let a = from + FRAC_PI_2 * i as f32 / arc as f32;
            out.push(c + egui::vec2(a.cos(), a.sin()) * rad);
        }
    }
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
pub fn radial_rect(p: &Painter, r: Rect, radius: f32, at: egui::Vec2, radii: egui::Vec2, stops: &[(f32, Color32)]) {
    let rad = radius.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0);
    let focus = r.min + r.size() * at;
    let (rx, ry) = ((r.width() * radii.x).max(1.0), (r.height() * radii.y).max(1.0));
    let color = |q: egui::Pos2| ramp(stops, (((q.x - focus.x) / rx).powi(2) + ((q.y - focus.y) / ry).powi(2)).sqrt());
    let outline = rounded_loop(r, rad, 5);
    p.rect_filled(r, cr(rad.round() as u8), color(outline[0]));
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
    let face = pad.shrink(g.pad);
    match light {
        Some(l) => {
            let l = if hot { l.scaled(1.06) } else { l };
            vgradient(p, pad, g.r_pad, &[(0.0, l.rim), (0.12, l.edge), (0.88, l.edge), (1.0, l.rim)]);
            radial_rect(
                p,
                face,
                g.r_face,
                egui::vec2(0.5, 0.49),
                egui::vec2(0.85, 0.82),
                &[(0.0, l.hi), (0.38, l.mid), (0.76, l.low), (1.0, l.edge)],
            );
        }
        None => {
            let k = tone * if hot { 1.14 } else { 1.0 };
            let (top, mid, bottom) = (shade(PAD_TOP, k), shade(PAD_MID, k), shade(PAD_BOTTOM, k));
            vgradient(p, pad, g.r_pad, &[(0.0, top), (0.12, mid), (0.88, mid), (1.0, bottom)]);
            p.rect_filled(face, cr(g.r_face.round() as u8), shade(KEY_FACE, k));
        }
    }
}

/// Ink for a legend or icon printed on a key.
pub fn key_ink(lit: bool) -> Color32 {
    if lit {
        hex(0x25302c)
    } else {
        hex(0xa3a69a)
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
            p.circle_filled(c, r, hex(0x11181b));
            p.circle_stroke(c, r - ring * 0.5, stroke(ring, hex(0x171f21)));
            let bead_c = if hot { hex(0x4a5755) } else { hex(0x404c4a) };
            p.circle_filled(c, bead, bead_c);
            // A faint top highlight on the bead (inset 0 1px 1px).
            let arc: Vec<egui::Pos2> = (0..=8)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.15 + 0.7 * i as f32 / 8.0);
                    c + egui::vec2(a.cos(), a.sin()) * (bead - 0.8)
                })
                .collect();
            p.add(egui::Shape::line(arc, stroke(0.8, mix(bead_c, hex(0x63706a), 0.35))));
        }
    }
}

/// LCD colours: pale backlight, dark ink.
pub const LCD: Color32 = hex(0xd5ddda);
pub const LCD_INK: Color32 = hex(0x27332f);
pub const LCD_INK_DIM: Color32 = hex(0x53635e);

/// The LCD panel itself (text goes on top).
pub fn lcd_panel(p: &Painter, r: Rect) {
    let rad = (r.height() * 0.11).clamp(3.0, 7.0);
    p.rect_filled(r, cr(rad as u8), LCD);
    // inset 0 1px 2px: a faint shadow under the top edge.
    let top = Rect::from_min_size(r.min + egui::vec2(rad * 0.6, 0.0), egui::vec2(r.width() - rad * 1.2, 1.0));
    p.rect_filled(top, cr(0), mix(LCD, hex(0x19231e), 0.13));
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
