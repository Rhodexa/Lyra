//! Octavia's look: the palette (`docs/design/colors.css`, the only source of
//! colour), her fonts, and the plate's controls as painted shapes.
//!
//! The vibe (`docs/FACEPLATE.md`): a synth made to be cheap. Blocky and flat,
//! square corners, hard cartoon shadows (an offset copy, no blur), 2 px dark
//! outlines, few decorations. Nothing here comes from Lyra's theme.

use egui::text::{LayoutJob, TextFormat};
use egui::{
    pos2, vec2, Color32, FontData, FontDefinitions, FontFamily, FontId, Galley, Painter, Pos2, Rect, Shape, Stroke,
    StrokeKind,
};
use std::sync::Arc;

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

// ── The character palette (colors.css) ──
pub const COAT: Color32 = hex(0xBDB4AE);
pub const COAT_OUTLINE: Color32 = hex(0x988F89);
pub const MANE: Color32 = hex(0x423C3C);
pub const MANE_OUTLINE: Color32 = hex(0x231A20);
pub const BOW_TIE: Color32 = hex(0xE07ED4);
pub const COLLAR: Color32 = hex(0xDEDDE8);
pub const VIOLIN_BODY: Color32 = hex(0xBC7547);
pub const VIOLIN_WOOD: Color32 = hex(0x7C4A2D);

// ── What they mean on the plate ──
pub const CHASSIS: Color32 = COAT;
pub const CHASSIS_INSET: Color32 = COAT_OUTLINE;
pub const CONTROL: Color32 = MANE;
pub const CONTROL_TRACK: Color32 = MANE_OUTLINE;
pub const TEXT_PRIMARY: Color32 = MANE_OUTLINE;
pub const TEXT_SECONDARY: Color32 = MANE;
pub const TEXT_INVERSE: Color32 = COLLAR;
pub const ACCENT: Color32 = BOW_TIE;
pub const LCD_BEZEL: Color32 = MANE;

// ── The LCD (Game Boy inspired, the lavender set) ──
pub const LCD: [Color32; 4] = [hex(0xC4C3CF), hex(0xA4A2B2), hex(0x777584), hex(0x292830)];

/// Every outline, everywhere.
pub const OL: f32 = 2.0;
/// The hard shadow under lone buttons.
pub const BTN_DROP: f32 = 2.0;

/// `a` toward `b` by `t` (CSS's mix, per channel).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// CSS `filter: brightness(k)`.
fn brighten(c: Color32, k: f32) -> Color32 {
    let m = |x: u8| (x as f32 * k).min(255.0) as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

// ───────────────────────────────────────────────────────────── fonts

/// The plate's labels: IBM Plex Mono 700 (and 600 for the dim captions).
pub const LABEL: &str = "octavia-label";
pub const LABEL_SEMI: &str = "octavia-label-semi";
/// The LCD's text: Roboto Condensed 700 (600 for the log's plain cells).
pub const LCD_FONT: &str = "octavia-lcd";
pub const LCD_SEMI: &str = "octavia-lcd-semi";

/// Octavia's font families, added before Lyra's (so Lyra can't replace them).
pub fn add_fonts(defs: &mut FontDefinitions) {
    let fonts: [(&str, &str, &'static [u8]); 4] = [
        (LABEL, "octavia-IBMPlexMono-Bold", include_bytes!("../assets/fonts/IBMPlexMono-Bold.ttf")),
        (LABEL_SEMI, "octavia-IBMPlexMono-SemiBold", include_bytes!("../assets/fonts/IBMPlexMono-SemiBold.ttf")),
        (LCD_FONT, "octavia-RobotoCondensed-Bold", include_bytes!("../assets/fonts/RobotoCondensed-Bold.ttf")),
        (LCD_SEMI, "octavia-RobotoCondensed-SemiBold", include_bytes!("../assets/fonts/RobotoCondensed-SemiBold.ttf")),
    ];
    // egui's own fonts stay behind ours for anything ours lack.
    let fallback = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (family, data, bytes) in fonts {
        defs.font_data.insert(data.into(), Arc::new(FontData::from_static(bytes)));
        let list = std::iter::once(data.to_string()).chain(fallback.iter().cloned()).collect();
        defs.families.insert(FontFamily::Name(family.into()), list);
    }
}

/// Text laid out in `family` at `size`, with CSS-style tracking (`track` in
/// points, after every letter). Its colour is given when painting.
pub fn galley(painter: &Painter, text: &str, family: &str, size: f32, track: f32) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: FontId::new(size, FontFamily::Name(family.into())),
            extra_letter_spacing: track,
            color: Color32::PLACEHOLDER,
            ..Default::default()
        },
    );
    painter.layout_job(job)
}

/// Where the galley's baseline sits below its top.
pub fn baseline(g: &Galley) -> f32 {
    g.rows.first().and_then(|r| r.glyphs.first()).map_or(g.size().y * 0.8, |gl| gl.pos.y)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Paints `g` with its baseline at `at` (canvas-style), aligned on `at.x`.
pub fn put(painter: &Painter, g: &Arc<Galley>, at: Pos2, align: Align, color: Color32) {
    let w = g.size().x;
    let x = match align {
        Align::Left => at.x,
        Align::Center => at.x - w / 2.0,
        Align::Right => at.x - w,
    };
    painter.galley(pos2(x, at.y - baseline(g)), g.clone(), color);
}

/// The plate's lettering (always caps): `size` px, tracked `em`.
pub fn label(
    painter: &Painter,
    text: &str,
    at: Pos2,
    align: Align,
    size: f32,
    em: f32,
    semi: bool,
    color: Color32,
) -> f32 {
    let g = galley(painter, &text.to_uppercase(), if semi { LABEL_SEMI } else { LABEL }, size, size * em);
    let w = g.size().x;
    put(painter, &g, at, align, color);
    w
}

/// How wide [`label`] would be.
pub fn label_w(painter: &Painter, text: &str, size: f32, em: f32, semi: bool) -> f32 {
    galley(painter, &text.to_uppercase(), if semi { LABEL_SEMI } else { LABEL }, size, size * em).size().x
}

/// A line box of `size` px (CSS line-height 1) starting at `top`: where caps
/// sit so they look centred in it.
pub fn caps_baseline(top: f32, size: f32) -> f32 {
    top + size * 0.5 + size * 0.35
}

// ───────────────────────────────────────────────────────────── plate parts

/// A filled rectangle with the 2 px dark outline inside it.
pub fn block(painter: &Painter, r: Rect, fill: Color32) {
    painter.rect(r, 0.0, fill, Stroke::new(OL, MANE_OUTLINE), StrokeKind::Inside);
}

/// A section head: caps label, an optional quieter subtitle, a 2 px rule to
/// `right`. `top` is the top of its 10 px line box.
pub fn section(painter: &Painter, x: f32, top: f32, right: f32, name: &str, sub: Option<&str>) {
    let base = caps_baseline(top, 10.0);
    let mut cx = x + label(painter, name, pos2(x, base), Align::Left, 10.0, 0.18, false, TEXT_PRIMARY) + 10.0;
    if let Some(sub) = sub {
        cx += label(painter, &format!("· {sub}"), pos2(cx, base), Align::Left, 10.0, 0.1, false, TEXT_SECONDARY) + 10.0;
    }
    painter.rect_filled(Rect::from_min_max(pos2(cx, top + 4.0), pos2(right, top + 6.0)), 0.0, TEXT_PRIMARY);
}

/// A caption under a control: bold, or dim (semi-bold, the secondary ink).
/// `top` is the top of its 9 px line box.
pub fn caption(painter: &Painter, text: &str, x: f32, top: f32, align: Align, dim: bool, em: f32) {
    let color = if dim { TEXT_SECONDARY } else { TEXT_PRIMARY };
    label(painter, text, pos2(x, caps_baseline(top, 9.0)), align, 9.0, em, dim, color);
}

/// How a button looks this frame.
#[derive(Clone, Copy)]
pub struct ButtonState {
    pub on: bool,
    pub hovered: bool,
    pub pressed: bool,
    /// A lone button casts a hard shadow and drops into it when pressed.
    pub solo: bool,
}

/// A flat block button: dark body and pale caps, or pink and dark when on.
/// `lines` stack (line height 1.25); `size` is the lettering in px.
pub fn button(painter: &Painter, r: Rect, lines: &[&str], s: ButtonState, size: f32) {
    let mut r = r;
    if s.solo {
        if s.pressed {
            r = r.translate(vec2(BTN_DROP, BTN_DROP));
        } else {
            painter.rect_filled(r.translate(vec2(BTN_DROP, BTN_DROP)), 0.0, MANE_OUTLINE);
        }
    }
    let (body, ink) = if s.on { (ACCENT, MANE_OUTLINE) } else { (CONTROL, TEXT_INVERSE) };
    let body = if s.hovered { brighten(body, 1.15) } else { body };
    block(painter, r, body);
    let lh = size * 1.25;
    let top = r.center().y - lh * lines.len() as f32 / 2.0;
    for (i, line) in lines.iter().enumerate() {
        let base = top + lh * i as f32 + lh * 0.5 + size * 0.35;
        label(painter, line, pos2(r.center().x, base), Align::Center, size, 0.12, false, ink);
    }
}

/// How wide a button must be for `text`: CSS `min-width` and `padding 0 pad`.
pub fn button_w(painter: &Painter, text: &str, min: f32, pad: f32) -> f32 {
    (label_w(painter, text, 10.0, 0.12, false) + 2.0 * pad + 2.0 * OL).max(min)
}

/// A switch's track: sunk into the plate, 3 px around and between its buttons.
pub fn track(painter: &Painter, r: Rect) {
    block(painter, r, CHASSIS_INSET);
}

/// Points along a circle from `a0` to `a1` (radians, screen coordinates).
fn arc(c: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let n = ((a1 - a0).abs() * r / 2.0).ceil().max(2.0) as usize;
    (0..=n).map(|i| a0 + (a1 - a0) * i as f32 / n as f32).map(|a| c + vec2(a.cos(), a.sin()) * r).collect()
}

/// A knob `size` across in `r`: a wide dark groove with the pink value
/// inside it, a round dark body with a 2 px outline, a pink pointer.
/// `v` is 0..1.
pub fn knob(painter: &Painter, r: Rect, v: f32, hot: bool) {
    use std::f32::consts::PI;
    let s = r.width();
    let c = r.center();
    let (a0, a1) = (PI * 0.75, PI * 2.25);
    let av = a0 + (a1 - a0) * v.clamp(0.0, 1.0);
    let (rb, ra) = (s * 0.31, s * 0.43);
    painter.add(Shape::line(arc(c, ra, a0, a1), Stroke::new(s * 0.13, MANE_OUTLINE)));
    if v > 0.005 {
        painter.add(Shape::line(arc(c, ra, a0, av), Stroke::new(s * 0.07, ACCENT)));
    }
    let body = if hot { brighten(CONTROL, 1.15) } else { CONTROL };
    painter.circle(c, rb, body, Stroke::new(OL, MANE_OUTLINE));
    let d = vec2(av.cos(), av.sin());
    painter.line_segment([c + d * rb * 0.3, c + d * rb * 0.85], Stroke::new((s * 0.07).max(3.0), ACCENT));
}

/// The fader's picture: 44 px wide, `travel` px of travel, 150 px tall at
/// 128. `r.min` is its top-left; `v` is 0..1.
pub const FADER_W: f32 = 44.0;
const FADER_CAP: f32 = 16.0;
const FADER_TOP: f32 = FADER_CAP / 2.0 + 2.0;

pub fn fader_h(travel: f32) -> f32 {
    FADER_TOP + travel + FADER_CAP / 2.0 + 4.0
}

pub fn fader(painter: &Painter, at: Pos2, travel: f32, v: f32, hot: bool) {
    let (w, ch) = (FADER_W, FADER_CAP);
    let top = FADER_TOP;
    let bot = top + travel;
    let rect = |x: f32, y: f32, rw: f32, rh: f32| Rect::from_min_size(at + vec2(x, y), vec2(rw, rh));
    // The scale: a tick every tenth, longer at the ends and the middle.
    for i in 0..=10 {
        let y = (top + travel * i as f32 / 10.0).round();
        let (x, tw) = if i % 5 == 0 { (0.0, 10.0) } else { (4.0, 6.0) };
        painter.rect_filled(rect(x, y - 1.0, tw, 2.0), 0.0, TEXT_PRIMARY);
    }
    painter.rect_filled(rect(w / 2.0 - 1.0, top - 3.0, 8.0, travel + 6.0), 0.0, CONTROL_TRACK);
    let y = (bot - travel * v.clamp(0.0, 1.0)).round();
    painter.rect_filled(rect(w / 2.0 + 1.0, y, 4.0, bot - y), 0.0, ACCENT);
    // The cap: a hard shadow, the block, a pink line.
    painter.rect_filled(rect(16.0, y - ch / 2.0 + 2.0, w - 18.0, ch), 0.0, MANE_OUTLINE);
    let body = if hot { brighten(CONTROL, 1.15) } else { CONTROL };
    painter.rect(rect(14.0, y - ch / 2.0, w - 18.0, ch), 0.0, body, Stroke::new(OL, MANE_OUTLINE), StrokeKind::Middle);
    painter.rect_filled(rect(17.0, y - 1.5, w - 24.0, 3.0), 0.0, ACCENT);
}
