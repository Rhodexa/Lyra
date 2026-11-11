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

// ── Surfaces (cool slate, slightly teal) ──
pub const BG: Color32 = Color32::from_rgb(0x16, 0x1a, 0x1f);
pub const PANEL: Color32 = Color32::from_rgb(0x1e, 0x23, 0x2a);
pub const PANEL_HI: Color32 = Color32::from_rgb(0x26, 0x2c, 0x34);
/// Toolbars and status bar: solid, a touch lighter than panels so they read clearly.
pub const TOOLBAR: Color32 = Color32::from_rgb(0x24, 0x2a, 0x32);
pub const RAISED: Color32 = Color32::from_rgb(0x2f, 0x36, 0x40);
pub const INSET: Color32 = Color32::from_rgb(0x12, 0x15, 0x19);
/// Borders and details are *darker* than the surfaces they sit on (engraved,
/// not glowing). Only selected/active things get bright edges.
pub const LINE: Color32 = Color32::from_rgb(0x0e, 0x11, 0x14);

// ── Text ──
pub const TEXT: Color32 = Color32::from_rgb(0xdc, 0xe4, 0xe2);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x84, 0x93, 0x95);

// ── Accents ──
pub const MINT: Color32 = Color32::from_rgb(0x9d, 0xe9, 0xd2);
pub const MINT_DEEP: Color32 = Color32::from_rgb(0x4f, 0xb8, 0x9a);
/// Lyra's eyes. Use sparingly.
pub const GOLD: Color32 = Color32::from_rgb(0xe8, 0xc8, 0x7a);
pub const ROSE: Color32 = Color32::from_rgb(0xf2, 0xa9, 0xbe);
pub const SKY: Color32 = Color32::from_rgb(0xa9, 0xd3, 0xf3);
pub const LAVENDER: Color32 = Color32::from_rgb(0xc4, 0xb7, 0xf2);
pub const PEACH: Color32 = Color32::from_rgb(0xf5, 0xc6, 0xa4);
pub const BUTTER: Color32 = Color32::from_rgb(0xf1, 0xe4, 0xa6);

/// Channel colours, cycled.
pub const CHANNEL_COLORS: [Color32; 6] = [MINT, ROSE, SKY, LAVENDER, PEACH, BUTTER];

pub fn channel_color(i: usize) -> Color32 {
    CHANNEL_COLORS[i % CHANNEL_COLORS.len()]
}

// ── Step buttons: two alternating shades mark each beat (4 steps) ──
pub const STEP_OFF_A: Color32 = Color32::from_rgb(0x2b, 0x31, 0x3a);
pub const STEP_OFF_B: Color32 = Color32::from_rgb(0x34, 0x3b, 0x46);

// ── Piano roll ──
pub const ROLL_WHITE_ROW: Color32 = Color32::from_rgb(0x22, 0x28, 0x2f);
pub const ROLL_BLACK_ROW: Color32 = Color32::from_rgb(0x1c, 0x21, 0x27);
/// Rows outside the song's scale (when highlighting is on).
pub const ROLL_OUT_ROW: Color32 = Color32::from_rgb(0x17, 0x1b, 0x20);
pub const KEY_WHITE: Color32 = Color32::from_rgb(0xc3, 0xcb, 0xcc);
pub const KEY_BLACK: Color32 = Color32::from_rgb(0x1a, 0x1e, 0x23);
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

/// Dense, small, quiet: a DAW is a cockpit, not a landing page.
pub fn apply(ctx: &Context) {
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(9.5)),
        (TextStyle::Body, FontId::proportional(11.5)),
        (TextStyle::Button, FontId::proportional(11.5)),
        (TextStyle::Heading, FontId::proportional(14.0)),
        (TextStyle::Monospace, FontId::monospace(11.0)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(4.0, 3.0);
    style.spacing.button_padding = egui::vec2(6.0, 2.0);
    style.spacing.interact_size = egui::vec2(20.0, 18.0);

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
    v.selection.bg_fill = MINT_DEEP.gamma_multiply(0.5);
    v.selection.stroke = stroke(1.0, MINT);
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.bg_stroke = stroke(1.0, LINE);
    v.widgets.inactive.bg_fill = RAISED;
    v.widgets.inactive.weak_bg_fill = RAISED;
    v.widgets.hovered.bg_fill = mix(RAISED, MINT, 0.15);
    v.widgets.hovered.weak_bg_fill = mix(RAISED, MINT, 0.15);
    v.widgets.hovered.bg_stroke = stroke(1.0, MINT_DEEP);
    v.widgets.active.bg_fill = mix(RAISED, MINT, 0.3);
    v.widgets.active.weak_bg_fill = mix(RAISED, MINT, 0.3);
    ctx.set_style(style);
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
