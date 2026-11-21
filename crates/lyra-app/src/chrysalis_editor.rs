//! The hand-made editor for **Chrysalis** (`lyra.chrysalis`), the wavetable synth.
//!
//! Chrysalis has its own look, taken from the user's banner art rather than
//! Lyra's charcoal rack: teal-black panels, dark "scope" screens with glowing
//! curves, neon knobs. Green and cyan are the sound (oscillator A, B, the
//! filters), violet is everything that moves it (envelopes, LFOs, the
//! matrix, macros), as the banner's violet glows in from its edges.
//!
//! ```text
//!  ┌ ‹ preset › ──────────── (emblem) ─────────── volume · velocity · glide ┐
//!  ├ OSC A ──────────────────────────┬ OSC B ───────────────────────────────┤
//!  │ table as stacked frames, knobs  │ the same                             │
//!  ├ SUB · NOISE ┬ DRIVE ────────┬ FILTERS ─────────────────────────────────┤
//!  ├ AMP ENV2 ENV3 ENV4 · LFO1-4 · MATRIX ────────────────────┬ MACROS ─────┤
//!  └──────────────────────────────────────────────────────────┴─────────────┘
//! ```
//!
//! * The table view: drag up/down or scroll to move through the table.
//! * Filters: drag a dot for cutoff (sideways) and resonance (up/down).
//! * Envelopes: dots for times and sustain, rings for the curves.
//! * LFOs: draw the shape. Drag points (they snap; Alt = free), drag the rings
//!   to bend, double-click empty space to add a point, double-click or
//!   right-click a point to remove it.
//! * Matrix: 16 slots, source → destination × amount, "via" a second source,
//!   Bi to centre the source.
//!
//! Like every view, this only edits `channel.params` (and `channel.sample`
//! for an imported table). The pictures call `lyra_chrysalis` itself, so
//! they show exactly what plays.

use crate::flex::{self, Grow, Px};
use crate::fm_editor::wheel_step;
use crate::rack::tracked;
use crate::theme::{self, cr, stroke};
use egui::text::LayoutJob;
use egui::{
    pos2, vec2, Align, Align2, Color32, FontId, Id, Layout, Pos2, Rect, Response, Sense, Shape, Ui, UiBuilder, Vec2,
};
use lyra_chrysalis::curve::{self, Point, MAX_POINTS, SHAPES};
use lyra_chrysalis::params::{ft, warp};
use lyra_chrysalis::table::{self, Table, IMPORTED};
use lyra_chrysalis::telemetry::{Snapshot, Telemetry};
use lyra_chrysalis::{p, PARAMS, PRESETS};
use lyra_model::{History, Project};
use std::f32::consts::PI;
use std::sync::Arc;

// ───────────────────────────────────────────────────────────── the look

const fn hex(v: u32) -> Color32 {
    theme::hex(v)
}

/// Behind everything: teal-black, with violet glowing in at the corners.
// Her mane is turquoise: so is the hero colour, and the ground leans teal,
// a little lighter and softer than black.
const VOID: Color32 = hex(0x0b1b1c);
const VIOLET_GLOW: Color32 = Color32::from_rgba_premultiplied(0x26, 0x1a, 0x4c, 0x5a);
const PANEL: Color32 = hex(0x12262a);
/// Edges are darker than what they edge, never lighter.
const PANEL_EDGE: Color32 = hex(0x0a1618);
/// Screens and recesses: darker than the panels.
const SCREEN: Color32 = hex(0x091619);
/// A control or segment under the mouse.
const HOVER: Color32 = hex(0x1b3539);
/// Lettering on a solid, lit colour.
const INK_ON: Color32 = hex(0x06191b);
const GRID: Color32 = Color32::from_rgba_premultiplied(0x15, 0x38, 0x3a, 0x80);
const TEXT: Color32 = hex(0xd2efe8);
const TEXT_DIM: Color32 = hex(0x82a7a1);
const TEXT_FAINT: Color32 = hex(0x4f706c);

/// The hero: oscillator A, sub and noise, the voice, the nameplate.
const TURQ: Color32 = hex(0x5ce8cf);
/// The banner's green, softened: oscillator B.
const MINT: Color32 = hex(0x9cf0a2);
const LIME: Color32 = hex(0xcfe87c);
const AQUA: Color32 = hex(0x79c8f2);
/// Everything that moves the sound.
const VIOLET: Color32 = hex(0xa58fff);
const OSC_COLORS: [Color32; 2] = [TURQ, MINT];

/// The level the pictures draw from: 128 harmonics, smooth but still sharp.
const PICTURE_LEVEL: usize = 6;

fn alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a)
}

// ───────────────────────────────────────────────────────────── editing

/// The channel being edited, plus undo.
struct Ed<'a> {
    p: &'a mut Project,
    h: &'a mut History,
    ch: usize,
    /// What the sound is doing right now (None before the engine has the plugin).
    live: Option<Snapshot>,
    /// The output level, smoothed for the screens' backlight (0..1).
    glow: f32,
}

impl Ed<'_> {
    /// A parameter as it is right now: where automation has it while the
    /// song plays, else its own value.
    fn v(&self, id: usize) -> f32 {
        crate::automation::shown_param(self.ch, id).unwrap_or(self.p.channels[self.ch].params[id])
    }
    fn on(&self, id: usize) -> bool {
        self.v(id) >= 0.5
    }
    fn choice(&self, id: usize) -> usize {
        self.v(id).round().max(0.0) as usize
    }
    /// Set without a checkpoint (the middle of a drag).
    fn put(&mut self, id: usize, x: f32) {
        let info = &PARAMS[id];
        self.p.channels[self.ch].params[id] = x.clamp(info.min, info.max);
    }
    /// A one-shot change (a click): checkpoint, then set.
    fn set(&mut self, id: usize, x: f32) {
        if self.v(id) != x {
            self.h.checkpoint(self.p);
            self.put(id, x);
        }
    }
    /// A change from a knob-like widget: one checkpoint per drag.
    fn track(&mut self, r: &Response, id: usize, x: f32) {
        if tracked(r, self.h, self.p) {
            self.put(id, x);
        }
    }
    fn toggle(&mut self, id: usize) {
        let x = if self.on(id) { 0.0 } else { 1.0 };
        self.set(id, x);
    }
}

// ───────────────────────────────────────────────────────────── layout
//
// Everything is placed with `flex` (fixed parts and growing parts in rows
// and columns), so pieces line up and fill their panels instead of being
// positioned by hand.

const W: f32 = 1100.0;
const HEADER_H: f32 = 76.0;
const OSC_H: f32 = 316.0;
const FX_H: f32 = 206.0;
const MOD_H: f32 = 256.0;
const GAP: f32 = 6.0;
const PAD: f32 = 10.0;
const TITLE_H: f32 = 20.0;
const KEY_H: f32 = 24.0;
/// The one line of text under a knob.
const LABEL_H: f32 = 15.0;

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, ch: usize, live: Option<crate::bridge::Live>) {
    let live = live.and_then(|l| l.downcast::<Telemetry>().ok()).map(|t| t.snapshot());
    // The backlight follows the output: up at once, down gently.
    let glow_id = Id::new(("chrys-glow", ch));
    let before: f32 = ui.ctx().data(|d| d.get_temp(glow_id)).unwrap_or(0.0);
    let glow = live.map_or(0.0, |l| l.level.min(1.0)).max(before * 0.88);
    ui.ctx().data_mut(|d| d.insert_temp(glow_id, glow));
    // Animate only while something moves: notes sounding, a free LFO, a fading glow.
    let free_lfo = (0..4).any(|l| p.channels[ch].params[p::lfo(l, p::L_MODE)] < 0.5);
    if live.is_some_and(|l| l.voices > 0) || glow > 0.01 || (live.is_some() && free_lfo) {
        ui.ctx().request_repaint();
    }
    let mut ed = Ed { p, h, ch, live, glow };
    let total = vec2(W, HEADER_H + OSC_H + FX_H + MOD_H + 3.0 * GAP);
    let (all, _) = ui.allocate_exact_size(total, Sense::hover());
    backdrop(ui, all);
    let [header, oscs, fx, mods] = flex::col(all, [Px(HEADER_H), Px(OSC_H), Px(FX_H), Px(MOD_H)], GAP);
    header_bar(ui, &mut ed, header);
    let [a, b] = flex::row(oscs, [Grow(1.0), Grow(1.0)], GAP);
    osc_panel(ui, &mut ed, 0, a);
    osc_panel(ui, &mut ed, 1, b);
    let [sub, drive, filters] = flex::row(fx, [Px(246.0), Px(222.0), Grow(1.0)], GAP);
    sub_panel(ui, &mut ed, sub);
    drive_panel(ui, &mut ed, drive);
    filter_panel(ui, &mut ed, filters);
    let [modulation, macros] = flex::row(mods, [Grow(1.0), Px(180.0)], GAP);
    mod_panel(ui, &mut ed, modulation);
    macros_panel(ui, &mut ed, macros);
}

/// A child Ui filling `r` exactly (top-down, centred), greyed out unless `enabled`.
fn place<R>(ui: &mut Ui, r: Rect, salt: impl std::hash::Hash, enabled: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(UiBuilder::new().id_salt(salt).max_rect(r).layout(Layout::top_down(Align::Center)));
    child.spacing_mut().item_spacing = vec2(0.0, 0.0);
    child.add_enabled_ui(enabled, add).inner
}

// ───────────────────────────────────────────────────────────── live

/// The snapshot, while at least one voice sounds.
fn sounding(ed: &Ed) -> Option<Snapshot> {
    ed.live.filter(|l| l.voices > 0)
}

/// Where the matrix is pushing parameter `id` right now (its value with the
/// newest voice's modulation, the way the engine applies it), if anywhere.
fn modulated(ed: &Ed, id: usize) -> Option<f32> {
    use lyra_chrysalis::params::dest;
    let live = sounding(ed)?;
    let info = &PARAMS[id];
    let base = ed.v(id);
    let m = |d: usize| live.mods[d];
    let add = |d: usize| base + m(d);
    let times = |d: usize| base * (1.0 + m(d)).clamp(0.0, 2.0);
    let v = if id < p::SUB_ON {
        let (o, k) = (id / p::OSC_STRIDE, id % p::OSC_STRIDE);
        let d = |q: usize| dest::osc(o, q);
        match k {
            p::POS => add(d(dest::O_POS)),
            p::LEVEL => times(d(dest::O_LEVEL)),
            p::PAN => add(d(dest::O_PAN)),
            p::WARP => add(d(dest::O_WARP)),
            p::DETUNE => add(d(dest::O_DETUNE)),
            p::BLEND => add(d(dest::O_BLEND)),
            _ => return None,
        }
    } else if (p::FILTERS..p::ROUTING).contains(&id) {
        let (f, k) = ((id - p::FILTERS) / p::F_STRIDE, (id - p::FILTERS) % p::F_STRIDE);
        let d = |q: usize| dest::filter(f, q);
        match k {
            p::F_CUTOFF => base * (6.0 * m(d(dest::F_CUTOFF))).exp2(),
            p::F_RESO => add(d(dest::F_RESO)),
            p::F_DRIVE => add(d(dest::F_DRIVE)),
            p::F_VOWEL => add(d(dest::F_VOWEL)),
            p::F_MIX => add(d(dest::F_MIX)),
            _ => return None,
        }
    } else if let Some(l) = (0..4).find(|&l| id == p::lfo(l, p::L_RATE)) {
        base * (4.0 * m(dest::LFO_RATE + l)).exp2()
    } else {
        match id {
            p::SUB_LEVEL => times(dest::SUB_LEVEL),
            p::NOISE_LEVEL => times(dest::NOISE_LEVEL),
            p::NOISE_COLOR => base + 2.0 * m(dest::NOISE_COLOR),
            p::DIST_DRIVE => add(dest::DRIVE),
            p::DIST_MIX => add(dest::DRIVE_MIX),
            p::VOLUME => base * (1.0 + m(dest::VOLUME)).max(0.0),
            _ => return None,
        }
    };
    let v = v.clamp(info.min, info.max);
    ((info.to_normalized(v) - info.to_normalized(base)).abs() > 0.002).then_some(v)
}

/// A screen's backlight, rising with the output level.
fn breathe(p: &egui::Painter, r: Rect, color: Color32, glow: f32) {
    if glow > 0.01 {
        let at = r.center_bottom();
        let stops = [(0.0, alpha(color, 0.16 * glow)), (1.0, Color32::TRANSPARENT)];
        theme::radial_circle(p, at, r.width().max(r.height()) * 0.7, at, &stops);
    }
}

// ───────────────────────────────────────────────────────────── the readout

/// What the mouse is on: a control's full name and value, shown in the
/// header (so the knobs themselves carry only their names).
#[derive(Clone)]
struct Readout {
    name: &'static str,
    value: String,
    color: Color32,
}

fn readout_id() -> Id {
    Id::new("chrys-readout")
}

fn readout(ui: &Ui, name: &'static str, value: String, color: Color32) {
    ui.ctx().data_mut(|d| d.insert_temp(readout_id(), Readout { name, value, color }));
}

fn readout_display(ui: &Ui, r: Rect) {
    // Clipped to its box: a long name never runs into what's beside it.
    let p = &ui.painter_at(r.shrink(2.0));
    screen(p, r);
    let shown = ui.ctx().data(|d| d.get_temp::<Readout>(readout_id()));
    let inner = r.shrink2(vec2(10.0, 6.0));
    if shown.is_none() {
        // Nothing touched yet: three holes of her wings, big to small.
        let c = r.center();
        for (dx, dy, rx, ry) in [(-22.0, 2.0, 15.0, 8.5), (8.0, -6.0, 10.0, 5.6), (30.0, 5.0, 6.0, 3.4)] {
            wing_hole(p, c + vec2(dx, dy), rx, ry, -0.45, 0.8);
        }
    }
    if let Some(ro) = shown {
        // The editor calls the oscillators A and B; the saved names say 1 and 2.
        let name = ro.name.replace("Osc 1", "Osc A").replace("Osc 2", "Osc B");
        caps(p, inner.left_top() + vec2(0.0, 6.0), &name, TEXT_DIM, Align2::LEFT_CENTER);
        p.text(inner.left_bottom(), Align2::LEFT_BOTTOM, ro.value, FontId::proportional(16.0), ro.color);
    }
}

// ───────────────────────────────────────────────────────────── controls

/// A knob filling the cell `r`: as big as fits (up to `max_d`), its name
/// under it. Hovered or turned, the name becomes the value and the header's
/// readout shows both.
#[allow(clippy::too_many_arguments)]
fn knob_in(ui: &mut Ui, ed: &mut Ed, r: Rect, id: usize, label: &str, color: Color32, enabled: bool, max_d: f32) {
    let info = &PARAMS[id];
    let d = (r.height() - LABEL_H - 2.0).min(r.width() - 4.0).min(max_d).max(16.0);
    let top = r.top() + (r.height() - d - LABEL_H - 2.0) * 0.5;
    let kr = Rect::from_min_size(pos2(r.center().x - d * 0.5, top), vec2(d, d));
    let lr = Rect::from_min_size(pos2(r.left(), kr.bottom() + 2.0), vec2(r.width(), LABEL_H));
    place(ui, kr.union(lr), ("chrys-knob", id), enabled, |ui| {
        let mut v = ed.v(id);
        let live = if enabled { modulated(ed, id) } else { None };
        let resp = crate::widgets::knob_painted(ui, &mut v, info, d, |p, rect, v, hot| {
            paint_knob_live(p, rect, info, v, color, hot, live)
        });
        if enabled {
            ed.track(&resp, id, v);
        }
        let hot = resp.hovered() || resp.dragged();
        let value = format_value(id, ed.v(id));
        if hot {
            readout(ui, info.name, value.clone(), color);
        }
        let (text, ink) = if hot { (value, color) } else { (label.to_string(), TEXT_DIM) };
        ui.painter().text(lr.center(), Align2::CENTER_CENTER, text, FontId::proportional(10.5), ink);
    });
}

/// Segmented switch filling `size`: one dark recess; the chosen segment is
/// solid colour with dark lettering. Returns the click.
fn seg(ui: &mut Ui, labels: &[&str], chosen: usize, color: Color32, size: Vec2, hint: &str) -> Option<usize> {
    let seg_w = size.x / labels.len() as f32;
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    p.rect_filled(r, cr(3), SCREEN);
    let hover = resp.hover_pos().map(|pos| (((pos.x - r.left()) / seg_w) as usize).min(labels.len() - 1));
    for (i, l) in labels.iter().enumerate() {
        let s = Rect::from_min_size(pos2(r.left() + i as f32 * seg_w, r.top()), vec2(seg_w, size.y)).shrink(2.0);
        if i == chosen {
            p.rect_filled(s, cr(2), color);
            caps_style(p, s.center(), l, INK_ON, Align2::CENTER_CENTER, 0.8, true);
        } else {
            if hover == Some(i) {
                p.rect_filled(s, cr(2), HOVER);
            }
            let ink = if hover == Some(i) { TEXT } else { TEXT_DIM };
            caps_spaced(p, s.center(), l, ink, Align2::CENTER_CENTER, 0.8);
        }
    }
    let clicked = resp.clicked().then_some(()).and(hover);
    resp.on_hover_text(hint);
    clicked
}

/// A key with a word on it, filling `size`: solid colour with dark lettering
/// when lit, a dark recess otherwise.
fn key(ui: &mut Ui, text: &str, size: Vec2, lit: bool, color: Color32, hint: &str) -> Response {
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    paint_key(ui.painter(), r, text, lit, resp.hovered(), color, TEXT_DIM);
    resp.on_hover_text(hint)
}

fn paint_key(p: &egui::Painter, r: Rect, text: &str, lit: bool, hot: bool, color: Color32, ink: Color32) {
    if lit {
        p.rect_filled(r, cr(3), if hot { theme::shade(color, 1.08) } else { color });
        caps_style(p, r.center(), text, INK_ON, Align2::CENTER_CENTER, 1.0, true);
    } else {
        p.rect_filled(r, cr(3), if hot { HOVER } else { SCREEN });
        caps_spaced(p, r.center(), text, if hot { TEXT } else { ink }, Align2::CENTER_CENTER, 1.0);
    }
}

/// An on/off parameter as a key with its name on it.
fn toggle(ui: &mut Ui, ed: &mut Ed, id: usize, label: &str, color: Color32, size: Vec2, hint: &str) {
    let on = ed.on(id);
    if key(ui, label, size, on, color, hint).clicked() {
        ed.toggle(id);
    }
}

/// A dropdown for a choice parameter filling `size`: click for the list,
/// scroll to step. A dark field; no border.
fn well(ui: &mut Ui, ed: &mut Ed, id: usize, size: Vec2, color: Color32, hint: &str) {
    let info = &PARAMS[id];
    let n = info.labels.len();
    let at = ed.choice(id).min(n - 1);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    p.rect_filled(r, cr(3), if resp.hovered() { HOVER } else { SCREEN });
    let ink = if at == 0 { TEXT_DIM } else { TEXT };
    p.text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, info.labels[at], FontId::proportional(12.0), ink);
    chevron(p, r.right_center() - vec2(10.0, 1.0), 3.0, alpha(color, 0.7));
    if resp.hovered() {
        readout(ui, info.name, info.labels[at].to_string(), color);
    }
    if let Some(step) = wheel_step(ui, r) {
        ed.set(id, (at as i32 + step).clamp(0, n as i32 - 1) as f32);
    }
    let popup = ui.make_persistent_id(("chrys-well", ed.ch, id));
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup));
    }
    let resp = resp.on_hover_text(hint);
    let mut pick = None;
    egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(size.x);
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (k, label) in info.labels.iter().enumerate() {
                if ui.selectable_label(k == at, *label).clicked() {
                    pick = Some(k);
                }
            }
        });
    });
    if let Some(k) = pick {
        ed.set(id, k as f32);
    }
}

// ───────────────────────────────────────────────────────────── header

fn header_bar(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    panel(ui, r, "", TURQ);
    let p = ui.painter();
    // The nameplate: the emblem lights the bar around it, and its glow runs
    // out along the bar like the wings' trails.
    let c = r.center();
    let glow = [(0.0, alpha(TURQ, 0.18)), (0.5, alpha(TURQ, 0.05)), (1.0, Color32::TRANSPARENT)];
    theme::radial_circle(p, c, 190.0, c, &glow);
    for side in [-1.0f32, 1.0] {
        let n = 24;
        for k in 0..n {
            let (t0, t1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
            let x = |t: f32| c.x + side * (60.0 + t * 230.0);
            let y = |t: f32| c.y + 14.0 - 10.0 * (t * PI).sin() * (1.0 - t);
            let a = (1.0 - t0).powi(2);
            p.line_segment([pos2(x(t0), y(t0)), pos2(x(t1), y(t1))], stroke(1.2, alpha(TURQ, 0.5 * a)));
        }
    }
    let (tex, [w, h]) = theme::chrysalis_emblem(ui.ctx());
    // A badge: a little taller than the bar, so it breaks out of it.
    let eh = r.height() + 18.0;
    let img = Rect::from_center_size(c, vec2(eh * w as f32 / h as f32, eh));
    p.image(tex, img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    ui.interact(img, Id::new("chrys-emblem"), Sense::hover()).on_hover_text("Chrysalis · wavetable synthesizer");

    let [left, _, right] = flex::row(r.shrink2(vec2(PAD + 4.0, 0.0)), [Grow(1.0), Px(190.0), Grow(1.0)], 0.0);
    // Left: the preset, and the readout of whatever the mouse is on.
    let [read, preset] = flex::row(left, [Px(150.0), Grow(1.0)], 10.0);
    readout_display(ui, flex::center(read, vec2(read.width(), 46.0)));
    preset_row(ui, ed, flex::center(preset, vec2(preset.width(), 34.0)));
    // Right: the voice.
    let [_, knobs, switches] = flex::row(right, [Grow(1.0), Px(3.0 * 62.0), Px(140.0)], 10.0);
    let cells = flex::even_row(flex::center(knobs, vec2(knobs.width(), 60.0)), 3, 0.0);
    knob_in(ui, ed, cells[0], p::VOLUME, "Volume", TURQ, true, 40.0);
    knob_in(ui, ed, cells[1], p::AMP_VEL, "Velocity", TURQ, true, 40.0);
    knob_in(ui, ed, cells[2], p::GLIDE, "Glide", TURQ, true, 40.0);
    let [m1, m2] =
        flex::col(flex::center(switches, vec2(switches.width(), 2.0 * KEY_H + 6.0)), [Px(KEY_H), Px(KEY_H)], 6.0);
    let mut set = None;
    place(ui, m1, "chrys-mode", true, |ui| {
        let hint = "Mono: one note at a time, with legato and glide";
        if let Some(k) = seg(ui, &["poly", "mono"], ed.choice(p::MODE), TURQ, m1.size(), hint) {
            set = Some((p::MODE, k));
        }
    });
    let glides = ed.v(p::GLIDE) > 0.0;
    place(ui, m2, "chrys-glide-mode", glides, |ui| {
        let hint = "Glide only between overlapping notes (legato), or on every note";
        if let Some(k) = seg(ui, &["legato", "always"], ed.choice(p::GLIDE_MODE), TURQ, m2.size(), hint) {
            set = Some((p::GLIDE_MODE, k));
        }
    });
    if let Some((id, k)) = set {
        ed.set(id, k as f32);
    }
}

/// `‹ name ›` in `r`: click the name for the list, scroll over it to step.
fn preset_row(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let n = PRESETS.len();
    let current = current_preset(&ed.p.channels[ed.ch].params);
    let last_id = Id::new(("chrys-last-preset", ed.ch));
    let last = ui.ctx().data(|d| d.get_temp::<usize>(last_id));
    let at = current.or(last).unwrap_or(0);
    let (name, edited) = match (current, last) {
        (Some(k), _) => (PRESETS[k].0, false),
        (None, Some(k)) => (PRESETS[k].0, true),
        (None, None) => ("Custom", false),
    };
    let [prev, field, next] = flex::row(r, [Px(r.height()), Grow(1.0), Px(r.height())], 4.0);
    let mut pick = None;
    for (rect, left) in [(prev, true), (next, false)] {
        let resp = ui.interact(rect, Id::new(("chrys-preset-step", left)), Sense::click());
        let p = ui.painter();
        p.rect_filled(rect, cr(3), if resp.hovered() { HOVER } else { SCREEN });
        let s = if left { -1.0 } else { 1.0 };
        let c = rect.center();
        let pts = vec![c + vec2(-2.5 * s, -5.0), c + vec2(2.5 * s, 0.0), c + vec2(-2.5 * s, 5.0)];
        p.add(Shape::line(pts, stroke(1.6, TURQ)));
        if resp.on_hover_text(if left { "Previous preset" } else { "Next preset" }).clicked() {
            pick = Some(if left { (at + n - 1) % n } else { (at + 1) % n });
        }
    }
    let resp = ui.interact(field, Id::new(("chrys-preset-field", ed.ch)), Sense::click());
    let p = ui.painter();
    p.rect_filled(field, cr(3), if resp.hovered() { HOVER } else { SCREEN });
    let cap = caps(p, field.left_center() + vec2(10.0, 0.0), "preset", TEXT_FAINT, Align2::LEFT_CENTER);
    // The name stays clear of the chevron (clipped, never overlapping it).
    let text_area =
        Rect::from_min_max(pos2(cap.right() + 10.0, field.top()), pos2(field.right() - 24.0, field.bottom()));
    let tp = ui.painter_at(text_area);
    let t = tp.text(text_area.left_center(), Align2::LEFT_CENTER, name, FontId::proportional(15.0), TEXT);
    if edited {
        tp.text(t.right_center() + vec2(8.0, 1.0), Align2::LEFT_CENTER, "edited", FontId::proportional(10.5), TEXT_DIM);
    }
    chevron(p, field.right_center() - vec2(11.0, 1.0), 3.5, TURQ);
    if let Some(step) = wheel_step(ui, field) {
        pick = Some((at as i32 + step).rem_euclid(n as i32) as usize);
    }
    let popup = ui.make_persistent_id(("chrys-preset-list", ed.ch));
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup));
    }
    let resp = resp.on_hover_text("Preset: scroll to flip through, click for the list");
    egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(field.width());
        for (k, (name, _)) in PRESETS.iter().enumerate() {
            if ui.selectable_label(current == Some(k), *name).clicked() {
                pick = Some(k);
            }
        }
    });
    if let Some(k) = pick {
        ed.h.checkpoint(ed.p);
        ed.p.channels[ed.ch].params = preset_values(k);
        ui.ctx().data_mut(|d| d.insert_temp(last_id, k));
    }
}

// ───────────────────────────────────────────────────────────── oscillators

fn osc_panel(ui: &mut Ui, ed: &mut Ed, o: usize, r: Rect) {
    let color = OSC_COLORS[o];
    let inner = panel(ui, r, "", color);
    let at = |k: usize| p::osc(o, k);
    let on = ed.on(at(p::OSC_ON));
    let is_import = ed.choice(at(p::TABLE)) == IMPORTED;
    let [bar, scope, k1, k2] = flex::col(inner, [Px(KEY_H), Grow(1.0), Px(52.0), Px(52.0)], 6.0);
    let [tog, table, load, name, warp_cap, warp_r] =
        flex::row(bar, [Px(72.0), Px(124.0), Px(54.0), Grow(1.0), Px(40.0), Px(108.0)], 6.0);
    let label = if o == 0 { "osc a" } else { "osc b" };
    place(ui, tog, ("chrys-osc-on", o), true, |ui| {
        toggle(ui, ed, at(p::OSC_ON), label, color, tog.size(), "Oscillator on / off")
    });
    place(ui, table, ("chrys-osc-table", o), true, |ui| {
        well(ui, ed, at(p::TABLE), table.size(), color, "Wavetable: scroll to flip through")
    });
    let has = ed.p.channels[ed.ch].sample.is_some();
    let hint = "Import a WAV as a wavetable (cut into frames of 2048 samples). Both oscillators can play it.";
    let clicked = place(ui, load, ("chrys-osc-load", o), true, |ui| {
        key(ui, "load", load.size(), is_import && has, color, hint).clicked()
    });
    if clicked {
        if let Some(path) = rfd::FileDialog::new().add_filter("Audio", &["wav", "flac"]).pick_file() {
            ed.h.checkpoint(ed.p);
            ed.p.channels[ed.ch].sample = Some(crate::samples::name_for(&path));
            ed.put(at(p::TABLE), IMPORTED as f32);
        }
    }
    if is_import {
        let text = ed.p.channels[ed.ch].sample.as_deref().map_or("no WAV loaded".into(), crate::samples::display_name);
        ui.painter().text(name.left_center(), Align2::LEFT_CENTER, text, FontId::proportional(11.0), TEXT_DIM);
    }
    caps(ui.painter(), warp_cap.right_center(), "warp", TEXT_FAINT, Align2::RIGHT_CENTER);
    place(ui, warp_r, ("chrys-osc-warp", o), true, |ui| {
        well(ui, ed, at(p::WARP_MODE), warp_r.size(), color, "Warp: how the phase is bent before the table is read")
    });
    place(ui, scope, ("chrys-scope", o), on, |ui| table_view(ui, ed, o, scope, color));
    // Seven columns in both rows, so the knobs line up.
    let warps = ed.choice(at(p::WARP_MODE)) != 0;
    let voices = ed.choice(at(p::UNISON)) > 0;
    let c1 = flex::even_row(k1, 7, 2.0);
    let c2 = flex::even_row(k2, 7, 2.0);
    let row1 =
        [(p::POS, "Position", true), (p::WARP, "Warp", warps), (p::OCTAVE, "Octave", true), (p::SEMI, "Semi", true)];
    let row1b = [(p::FINE, "Fine", true), (p::LEVEL, "Level", true), (p::PAN, "Pan", true)];
    for (cell, (k, name, en)) in c1.iter().zip(row1.into_iter().chain(row1b)) {
        knob_in(ui, ed, *cell, at(k), name, color, on && en, 36.0);
    }
    let row2 = [
        (p::UNISON, "Unison", true),
        (p::DETUNE, "Detune", voices),
        (p::BLEND, "Blend", voices),
        (p::SPREAD, "Spread", voices),
        (p::PHASE, "Phase", true),
        (p::RAND, "Random", true),
    ];
    for (cell, (k, name, en)) in c2.iter().zip(row2) {
        knob_in(ui, ed, *cell, at(k), name, color, on && en, 36.0);
    }
}

// ───────────────────────────────────────────────────────────── sub, noise, drive

fn sub_panel(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let inner = panel(ui, r, "", TURQ);
    let [a, b] = flex::row(inner, [Grow(1.0), Grow(1.0)], 8.0);
    let [t, s1, s2, k] = flex::col(a, [Px(KEY_H), Px(KEY_H), Px(KEY_H), Grow(1.0)], 6.0);
    let sub = ed.on(p::SUB_ON);
    place(ui, t, "chrys-sub-on", true, |ui| {
        toggle(ui, ed, p::SUB_ON, "sub", TURQ, t.size(), "Sub oscillator on / off")
    });
    let mut set = None;
    place(ui, s1, "chrys-sub-shape", sub, |ui| {
        if let Some(k) = seg(ui, &["sine", "square"], ed.choice(p::SUB_SHAPE), TURQ, s1.size(), "Sub shape") {
            set = Some((p::SUB_SHAPE, k));
        }
    });
    place(ui, s2, "chrys-sub-oct", sub, |ui| {
        if let Some(k) = seg(ui, &["−1 oct", "−2 oct"], ed.choice(p::SUB_OCT), TURQ, s2.size(), "Below the note") {
            set = Some((p::SUB_OCT, k));
        }
    });
    knob_in(ui, ed, k, p::SUB_LEVEL, "Level", TURQ, sub, 40.0);
    let [t2, ka, kb] = flex::col(b, [Px(KEY_H), Grow(1.0), Grow(1.0)], 6.0);
    let noise = ed.on(p::NOISE_ON);
    place(ui, t2, "chrys-noise-on", true, |ui| {
        toggle(ui, ed, p::NOISE_ON, "noise", TURQ, t2.size(), "Noise on / off (a little different on each side)")
    });
    knob_in(ui, ed, ka, p::NOISE_COLOR, "Colour", TURQ, noise, 40.0);
    knob_in(ui, ed, kb, p::NOISE_LEVEL, "Level", TURQ, noise, 40.0);
    if let Some((id, k)) = set {
        ed.set(id, k as f32);
    }
}

fn drive_panel(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let inner = panel(ui, r, "drive", LIME);
    let [s, scr, ks] = flex::col(inner, [Px(KEY_H), Grow(1.0), Px(52.0)], 6.0);
    let mode = ed.choice(p::DIST_MODE);
    let labels = ["off", "tanh", "hard", "fold", "crush"];
    let pick = place(ui, s, "chrys-drive-mode", true, |ui| {
        seg(ui, &labels, mode, LIME, s.size(), "Drive, per voice, before the filters")
    });
    if let Some(k) = pick {
        ed.set(p::DIST_MODE, k as f32);
    }
    let resp = ui.interact(scr, Id::new(("chrys-drive-screen", ed.ch)), Sense::click_and_drag());
    if mode != 0 {
        drag_param(ui, ed, &resp, p::DIST_DRIVE, 200.0);
        resp.on_hover_text("Drag up/down or scroll: drive");
    }
    drive_view(ui, ed, scr);
    let cells = flex::even_row(ks, 2, 8.0);
    knob_in(ui, ed, cells[0], p::DIST_DRIVE, "Drive", LIME, mode != 0, 36.0);
    knob_in(ui, ed, cells[1], p::DIST_MIX, "Mix", LIME, mode != 0, 36.0);
}

// ───────────────────────────────────────────────────────────── filters

fn filter_panel(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let inner = panel(ui, r, "", AQUA);
    let [graph, ctl] = flex::row(inner, [Grow(1.0), Px(6.0 * 54.0)], 10.0);
    place(ui, graph, "chrys-fgraph", true, |ui| filter_graph(ui, ed, graph));
    let [route, h1, k1, h2, k2] = flex::col(ctl, [Px(KEY_H), Px(KEY_H), Grow(1.0), Px(KEY_H), Grow(1.0)], 4.0);
    let hint = "Serial: filter 1 into filter 2. Parallel: both hear the voice, side by side.";
    let pick = place(ui, route, "chrys-route", true, |ui| {
        seg(ui, &["serial", "parallel"], ed.choice(p::ROUTING), AQUA, route.size(), hint)
    });
    if let Some(k) = pick {
        ed.set(p::ROUTING, k as f32);
    }
    for (f, head, knobs) in [(0, h1, k1), (1, h2, k2)] {
        let at = |k: usize| p::filter(f, k);
        let on = ed.on(at(p::F_ON));
        let kind = ed.choice(at(p::F_TYPE)) as u8;
        let [t, w] = flex::row(head, [Px(88.0), Grow(1.0)], 6.0);
        let label = if f == 0 { "filter 1" } else { "filter 2" };
        place(ui, t, ("chrys-f-on", f), true, |ui| {
            toggle(ui, ed, at(p::F_ON), label, AQUA, t.size(), "Filter on / off")
        });
        place(ui, w, ("chrys-f-type", f), true, |ui| well(ui, ed, at(p::F_TYPE), w.size(), AQUA, "Filter type"));
        let comb = kind >= ft::COMB_PLUS;
        let cut = match kind {
            ft::FORMANT => "Shift",
            _ if comb => "Pitch",
            _ => "Cutoff",
        };
        let cells = flex::even_row(knobs, 6, 0.0);
        let list = [
            (p::F_CUTOFF, cut, on),
            (p::F_RESO, if comb { "Feedback" } else { "Reso" }, on),
            (p::F_DRIVE, "Drive", on),
            (p::F_KEY, "Key", on),
            (p::F_VOWEL, "Vowel", on && kind == ft::FORMANT),
            (p::F_MIX, "Mix", on),
        ];
        for (cell, (k, name, en)) in cells.iter().zip(list) {
            knob_in(ui, ed, *cell, at(k), name, AQUA, en, 30.0);
        }
    }
}

// ───────────────────────────────────────────────────────────── modulation

fn mod_panel(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let inner = panel(ui, r, "", VIOLET);
    let [tabs, content] = flex::col(inner, [Px(KEY_H), Grow(1.0)], 8.0);
    // Tabs are keys: the open one solid violet, the rest dark recesses whose
    // names are fainter when nothing uses that source. Gaps group them.
    let view_id = Id::new(("chrys-mod-view", ed.ch));
    let mut view = ModView::from_index(ui.ctx().data(|d| d.get_temp::<usize>(view_id)).unwrap_or(4));
    let t = Px(66.0);
    let slots = flex::row(tabs, [t, t, t, t, Px(12.0), t, t, t, t, Px(12.0), Px(84.0)], 0.0);
    for (a, b) in [(0, 3), (5, 8), (10, 10)] {
        ui.painter().rect_filled(slots[a].union(slots[b]), cr(3), SCREEN);
    }
    for (i, name) in MOD_TABS.iter().enumerate() {
        let rect = slots[[0, 1, 2, 3, 5, 6, 7, 8, 10][i]];
        let resp = ui.interact(rect, Id::new(("chrys-mod-tab", i)), Sense::click());
        let open = view.index() == i;
        let face = rect.shrink(2.0);
        let p = ui.painter();
        if open {
            p.rect_filled(face, cr(2), VIOLET);
            caps_style(p, face.center(), name, INK_ON, Align2::CENTER_CENTER, 1.0, true);
            let c = rect.center_bottom();
            let notch = vec![c + vec2(-6.0, 2.0), c + vec2(6.0, 2.0), c + vec2(0.0, 7.0)];
            p.add(Shape::convex_polygon(notch, VIOLET, egui::Stroke::NONE));
        } else {
            if resp.hovered() {
                p.rect_filled(face, cr(2), HOVER);
            }
            let ink = if resp.hovered() {
                TEXT
            } else if mod_used(ed, i) {
                TEXT_DIM
            } else {
                TEXT_FAINT
            };
            caps_spaced(p, face.center(), name, ink, Align2::CENTER_CENTER, 1.0);
        }
        if resp.clicked() {
            view = ModView::from_index(i);
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(view_id, view.index()));
    match view {
        ModView::Env(e) => env_view(ui, ed, e, content),
        ModView::Lfo(l) => lfo_view(ui, ed, l, content),
        ModView::Matrix => matrix_view(ui, ed, content),
    }
}

fn env_view(ui: &mut Ui, ed: &mut Ed, e: usize, inner: Rect) {
    let [graph, knobs] = flex::row(inner, [Grow(1.0), Px(4.0 * 62.0)], 12.0);
    place(ui, graph, ("chrys-env-g", e), true, |ui| envelope(ui, ed, e, graph));
    let [r1, r2] = flex::col(knobs, [Grow(1.0), Grow(1.0)], 4.0);
    let at = |k: usize| p::env(e, k);
    let c1 = flex::even_row(r1, 4, 2.0);
    let c2 = flex::even_row(r2, 4, 2.0);
    for (cell, (k, name)) in c1.iter().zip([
        (p::E_ATTACK, "Attack"),
        (p::E_DECAY, "Decay"),
        (p::E_SUSTAIN, "Sustain"),
        (p::E_RELEASE, "Release"),
    ]) {
        knob_in(ui, ed, *cell, at(k), name, VIOLET, true, 40.0);
    }
    for (cell, (k, name)) in
        c2.iter().zip([(p::E_A_CURVE, "A curve"), (p::E_D_CURVE, "D curve"), (p::E_R_CURVE, "R curve")])
    {
        knob_in(ui, ed, *cell, at(k), name, VIOLET, true, 40.0);
    }
}

fn lfo_view(ui: &mut Ui, ed: &mut Ed, l: usize, inner: Rect) {
    use lyra_chrysalis::params::lfo_mode;
    let [graph, side] = flex::row(inner, [Grow(1.0), Px(272.0)], 12.0);
    let style = ed.choice(p::lfo_style(l));
    if style == 0 {
        place(ui, graph, ("chrys-lfo-g", l), true, |ui| lfo_editor(ui, ed, l, graph));
    } else {
        lfo_trace(ui, ed, l, graph);
    }
    let [mode_r, style_r, sh1, sh2, knobs] =
        flex::col(side, [Px(KEY_H), Px(KEY_H), Px(32.0), Px(32.0), Grow(1.0)], 6.0);
    // Modes in the order you think of them; saved by their index.
    const ORDER: [u8; 4] = [lfo_mode::FREE, lfo_mode::PHRASE, lfo_mode::RETRIG, lfo_mode::ONE_SHOT];
    let hint = "Free: one LFO for all notes, always running (locked to the song when synced). \
                Phrase: one for all notes, running only while notes sound, restarting after silence. \
                Retrig: each note has its own, from the start. One-shot: each note's runs once and holds its end.";
    let mode = ed.choice(p::lfo(l, p::L_MODE)) as u8;
    let shown = ORDER.iter().position(|&m| m == mode).unwrap_or(0);
    let pick = place(ui, mode_r, ("chrys-lfo-mode", l), true, |ui| {
        seg(ui, &["free", "phrase", "retrig", "1-shot"], shown, VIOLET, mode_r.size(), hint)
    });
    if let Some(k) = pick {
        ed.set(p::lfo(l, p::L_MODE), ORDER[k] as f32);
    }
    let hint =
        "Drawn: the shape you draw. Steps: a new random value every cycle. Smooth: random values, glided between.";
    let pick = place(ui, style_r, ("chrys-lfo-style", l), true, |ui| {
        seg(ui, &["drawn", "steps", "smooth"], style, VIOLET, style_r.size(), hint)
    });
    if let Some(k) = pick {
        ed.set(p::lfo_style(l), k as f32);
    }
    // Shape presets: a picture each, four to a row (for drawn LFOs).
    let now = read_points(ed, l);
    for (row, cells) in SHAPES.chunks(4).zip([flex::even_row(sh1, 4, 4.0), flex::even_row(sh2, 4, 4.0)]) {
        for ((name, pts), cell) in row.iter().zip(cells) {
            let shape: Vec<Point> = pts.iter().map(|&(x, y, bend)| Point { x, y, bend }).collect();
            let picked = place(ui, cell, ("chrys-shape", l, *name), style == 0, |ui| {
                let (r, resp) = ui.allocate_exact_size(cell.size(), Sense::click());
                paint_shape_key(ui.painter(), r, &shape, now == shape, resp.hovered());
                resp.on_hover_text(*name).clicked()
            });
            if picked {
                ed.h.checkpoint(ed.p);
                write_points(ed, l, &shape);
            }
        }
    }
    let synced = ed.choice(p::lfo(l, p::L_SYNC)) != 0;
    let cells = flex::even_row(knobs, 2, 8.0);
    knob_in(ui, ed, cells[0], p::lfo(l, p::L_SYNC), "Sync", VIOLET, true, 40.0);
    knob_in(ui, ed, cells[1], p::lfo(l, p::L_RATE), "Rate", VIOLET, !synced, 40.0);
}

/// A random LFO has no shape to draw: its screen shows what it has been
/// doing, the newest value at the right edge.
fn lfo_trace(ui: &mut Ui, ed: &Ed, l: usize, rect: Rect) {
    const N: usize = 240;
    let id = Id::new(("chrys-lfo-trace", ed.ch, l));
    let mut trace: Vec<f32> = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    if let Some(live) = ed.live {
        trace.push(live.lfo_value[l].clamp(0.0, 1.0));
        if trace.len() > N {
            trace.drain(..trace.len() - N);
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, trace.clone()));
    let painter = ui.painter_at(rect);
    screen(&painter, rect);
    let inner = rect.shrink2(vec2(14.0, 12.0));
    for k in 1..4 {
        let y = inner.bottom() - k as f32 / 4.0 * inner.height();
        painter.line_segment([pos2(inner.left(), y), pos2(inner.right(), y)], stroke(1.0, alpha(GRID, 0.45)));
    }
    let step = inner.width() / (N - 1) as f32;
    let x0 = inner.right() - (trace.len().saturating_sub(1)) as f32 * step;
    let pts: Vec<Pos2> = trace
        .iter()
        .enumerate()
        .map(|(i, v)| pos2(x0 + i as f32 * step, inner.bottom() - v * inner.height()))
        .collect();
    if let Some(&last) = pts.last() {
        glow_fill(&painter, &pts, inner.bottom(), VIOLET);
        glow_line(&painter, pts, VIOLET, 1.6);
        let bright = theme::mix(VIOLET, Color32::WHITE, 0.6);
        painter.circle_filled(last, 9.0, alpha(VIOLET, 0.25));
        painter.circle_filled(last, 4.0, bright);
    } else {
        caps(&painter, inner.center(), "random: play to see it move", TEXT_FAINT, Align2::CENTER_CENTER);
    }
    breathe(&painter, rect, VIOLET, ed.glow);
}

/// The routes in use, one per row (the rest of the 16 slots stay out of the
/// way), and "+ route" to start another. A long list scrolls.
fn matrix_view(ui: &mut Ui, ed: &mut Ed, inner: Rect) {
    const ROW: f32 = 28.0;
    const SPACE: f32 = 3.0;
    let used: Vec<usize> = (0..p::SLOT_COUNT)
        .filter(|&k| ed.choice(p::slot(k, p::S_SRC)) != 0 || ed.choice(p::slot(k, p::S_DEST)) != 0)
        .collect();
    let free = (0..p::SLOT_COUNT).find(|k| !used.contains(k));
    let rows = used.len() + free.is_some() as usize;
    let fits = ((inner.height() + SPACE) / (ROW + SPACE)).floor() as usize;
    let draw = |ui: &mut Ui, ed: &mut Ed, width: f32| {
        for &k in &used {
            let (r, _) = ui.allocate_exact_size(vec2(width, ROW), Sense::hover());
            slot_row(ui, ed, k, r);
        }
        if let Some(free) = free {
            let (r, _) = ui.allocate_exact_size(vec2(width, ROW), Sense::hover());
            let [_, add, note] = flex::row(r, [Px(18.0), Px(124.0), Grow(1.0)], 4.0);
            let hint = "A new route: pick what moves (an LFO, an envelope, velocity, a macro…) and where it goes";
            if place(ui, add, "chrys-add-route", true, |ui| {
                key(ui, "+ route", add.size(), false, VIOLET, hint).clicked()
            }) {
                ed.set(p::slot(free, p::S_SRC), 1.0);
            }
            if used.is_empty() {
                let t = "Send the LFOs, envelopes and macros anywhere.";
                ui.painter().text(
                    note.left_center() + vec2(8.0, 0.0),
                    Align2::LEFT_CENTER,
                    t,
                    FontId::proportional(11.5),
                    TEXT_FAINT,
                );
            }
        }
    };
    let mut child = ui.new_child(
        UiBuilder::new().id_salt(("chrys-matrix", ed.ch)).max_rect(inner).layout(Layout::top_down(Align::Min)),
    );
    child.spacing_mut().item_spacing.y = SPACE;
    if rows <= fits {
        draw(&mut child, ed, inner.width());
    } else {
        egui::ScrollArea::vertical()
            .id_salt(("chrys-matrix-scroll", ed.ch))
            .max_height(inner.height())
            .show(&mut child, |ui| draw(ui, ed, inner.width() - 18.0));
    }
}

fn slot_row(ui: &mut Ui, ed: &mut Ed, k: usize, r: Rect) {
    let at = |q: usize| p::slot(k, q);
    let live = ed.choice(at(p::S_SRC)) != 0 && ed.choice(at(p::S_DEST)) != 0;
    let [num, src, arrow, dest, bar, value, via_cap, via, bi, x] = flex::row(
        r,
        [Px(18.0), Px(124.0), Px(14.0), Grow(1.0), Grow(1.3), Px(44.0), Px(26.0), Px(104.0), Px(36.0), Px(r.height())],
        4.0,
    );
    let p = ui.painter();
    p.text(
        num.center(),
        Align2::CENTER_CENTER,
        format!("{}", k + 1),
        FontId::proportional(11.0),
        if live { VIOLET } else { TEXT_FAINT },
    );
    let (y, x0, x1) = (arrow.center().y, arrow.left(), arrow.right());
    let st = stroke(1.5, if live { VIOLET } else { TEXT_FAINT });
    p.line_segment([pos2(x0, y), pos2(x1, y)], st);
    p.add(Shape::line(vec![pos2(x1 - 4.0, y - 3.5), pos2(x1, y), pos2(x1 - 4.0, y + 3.5)], st));
    let amount = PARAMS[at(p::S_AMT)].format(ed.v(at(p::S_AMT)));
    p.text(
        value.right_center(),
        Align2::RIGHT_CENTER,
        amount,
        FontId::proportional(12.0),
        if live { TEXT } else { TEXT_FAINT },
    );
    caps(p, via_cap.right_center(), "via", TEXT_FAINT, Align2::RIGHT_CENTER);
    place(ui, src, ("chrys-src", k), true, |ui| {
        well(ui, ed, at(p::S_SRC), src.size(), VIOLET, "Source: what moves this route")
    });
    place(ui, dest, ("chrys-dest", k), true, |ui| {
        well(ui, ed, at(p::S_DEST), dest.size(), VIOLET, "Destination: what it moves")
    });
    amount_bar(ui, ed, at(p::S_AMT), bar, if live { VIOLET } else { TEXT_FAINT });
    let hint = "Via: a second source that scales the amount (e.g. velocity)";
    place(ui, via, ("chrys-via", k), true, |ui| well(ui, ed, at(p::S_VIA), via.size(), VIOLET, hint));
    let hint = "Bi: centre the source (−1..1), so it swings both ways around the knob";
    if place(ui, bi, ("chrys-bi", k), true, |ui| {
        key(ui, "bi", bi.size(), ed.on(at(p::S_BIPOLAR)), VIOLET, hint).clicked()
    }) {
        ed.toggle(at(p::S_BIPOLAR));
    }
    // Remove: the route goes back to an empty slot.
    let xresp = ui.interact(x, Id::new(("chrys-route-x", k)), Sense::click());
    let hot = xresp.hovered();
    let p = ui.painter();
    p.rect_filled(x, cr(3), if hot { HOVER } else { SCREEN });
    let c = x.center();
    let xs = stroke(1.5, if hot { TEXT } else { TEXT_FAINT });
    p.line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], xs);
    p.line_segment([c + vec2(-4.0, 4.0), c + vec2(4.0, -4.0)], xs);
    if xresp.on_hover_text("Remove this route").clicked() {
        ed.h.checkpoint(ed.p);
        for q in [p::S_SRC, p::S_DEST, p::S_AMT, p::S_VIA, p::S_BIPOLAR] {
            ed.put(at(q), PARAMS[at(q)].default);
        }
    }
}

/// A route's amount as a wide bar from the middle (−100 % … +100 %): drag
/// sideways (Shift = fine), scroll to nudge, double-click for 0.
fn amount_bar(ui: &mut Ui, ed: &mut Ed, id: usize, r: Rect, color: Color32) {
    let resp = ui.interact(r, Id::new(("chrys-amount", id)), Sense::click_and_drag());
    let info = &PARAMS[id];
    let reset = dbl(ui, &resp, ("amount", id));
    if resp.drag_started() || reset {
        ed.h.checkpoint(ed.p);
    }
    if reset {
        ed.put(id, 0.0);
    } else if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
        let d = resp.drag_delta();
        ed.put(id, ed.v(id) + (d.x - d.y) / r.width() * (info.max - info.min) * fine);
    }
    if let Some(step) = wheel_step(ui, r) {
        ed.h.checkpoint(ed.p);
        ed.put(id, ed.v(id) - step as f32 * 0.01);
    }
    let hot = resp.hovered() || resp.dragged();
    if hot {
        readout(ui, info.name, info.format(ed.v(id)), color);
    }
    let p = ui.painter();
    p.rect_filled(r, cr(3), if hot { HOVER } else { SCREEN });
    let track = r.shrink2(vec2(6.0, 7.0));
    let mid = track.center().x;
    let x = mid + ed.v(id) / info.max * track.width() * 0.5;
    let fill = Rect::from_x_y_ranges(mid.min(x)..=mid.max(x), track.y_range());
    if fill.width() > 0.5 {
        p.rect_filled(fill.expand(3.0), cr(4), alpha(color, 0.14));
        p.rect_filled(fill, cr(2), color);
    }
    p.line_segment([pos2(mid, r.top() + 4.0), pos2(mid, r.bottom() - 4.0)], stroke(1.0, TEXT_FAINT));
    resp.on_hover_text("Drag up/down or sideways (Shift: fine); double-click: 0");
}

// ───────────────────────────────────────────────────────────── macros

/// Four knobs; a macro is dim until a route listens to it.
fn macros_panel(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let inner = panel(ui, r, "macros", VIOLET);
    let rows = flex::even_col(inner, 2, 4.0);
    for k in 0..4 {
        let cell = flex::even_row(rows[k / 2], 2, 4.0)[k % 2];
        let src = (lyra_chrysalis::params::src::MACRO + k) as f32;
        let routed = (0..p::SLOT_COUNT).any(|s| {
            ed.v(p::slot(s, p::S_DEST)) >= 0.5
                && (ed.v(p::slot(s, p::S_SRC)) == src || ed.v(p::slot(s, p::S_VIA)) == src)
        });
        let color = if routed { VIOLET } else { alpha(VIOLET, 0.45) };
        let name = ["Macro 1", "Macro 2", "Macro 3", "Macro 4"][k];
        knob_in(ui, ed, cell, p::mac(k), name, color, true, 44.0);
    }
}

// ───────────────────────────────────────────────────────────── drawing and editing pieces

/// The whole editor's ground: teal-black, violet glowing in at two corners.
fn backdrop(ui: &Ui, r: Rect) {
    let p = ui.painter();
    p.rect_filled(r.expand(4.0), cr(4), VOID);
    for (at, radius) in [(r.left_bottom() + vec2(60.0, -40.0), 380.0), (r.right_top() + vec2(-80.0, 60.0), 300.0)] {
        theme::radial_circle(p, at, radius, at, &[(0.0, VIOLET_GLOW), (1.0, Color32::TRANSPARENT)]);
    }
}

/// A panel: dark teal with a dark edge, and its name (if any) in spaced
/// capitals. Returns the inside, below the name.
fn panel(ui: &Ui, r: Rect, title: &str, _color: Color32) -> Rect {
    let p = ui.painter();
    p.rect_filled(r, cr(5), PANEL);
    p.rect_stroke(r, cr(5), stroke(1.0, PANEL_EDGE), egui::StrokeKind::Outside);
    if title.is_empty() {
        return r.shrink(PAD);
    }
    caps(p, r.left_top() + vec2(PAD + 2.0, PAD + 6.0), title, TEXT_DIM, Align2::LEFT_CENTER);
    Rect::from_min_max(r.left_top() + vec2(PAD, PAD + TITLE_H), r.right_bottom() - vec2(PAD, PAD))
}

/// One hole of her wings: an oval cut into the plate (darker than it), with a
/// faint turquoise rim; `fade` 0..1 dims it.
fn wing_hole(p: &egui::Painter, c: Pos2, rx: f32, ry: f32, tilt: f32, fade: f32) {
    let (s, co) = tilt.sin_cos();
    let pts: Vec<Pos2> = (0..28)
        .map(|k| {
            let a = k as f32 / 28.0 * std::f32::consts::TAU;
            let (x, y) = (rx * a.cos(), ry * a.sin());
            c + vec2(x * co - y * s, x * s + y * co)
        })
        .collect();
    p.add(Shape::convex_polygon(pts.clone(), SCREEN, egui::Stroke::NONE));
    p.add(Shape::closed_line(pts, stroke(1.0, alpha(TURQ, 0.55 * fade))));
}

/// Small spaced capitals.
fn caps(p: &egui::Painter, at: Pos2, text: &str, color: Color32, align: Align2) -> Rect {
    caps_spaced(p, at, text, color, align, 1.6)
}

/// Small capitals, `spacing` points apart (switch segments use less).
fn caps_spaced(p: &egui::Painter, at: Pos2, text: &str, color: Color32, align: Align2, spacing: f32) -> Rect {
    caps_style(p, at, text, color, align, spacing, false)
}

/// Small capitals; `bold` thickens them (the UI font has no bold: the text is
/// drawn twice, half a point apart).
fn caps_style(
    p: &egui::Painter,
    at: Pos2,
    text: &str,
    color: Color32,
    align: Align2,
    spacing: f32,
    bold: bool,
) -> Rect {
    let mut job = LayoutJob::default();
    let font_id = FontId::proportional(if bold { 10.5 } else { 10.0 });
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat { font_id, color, extra_letter_spacing: spacing, ..Default::default() },
    );
    let galley = p.layout_job(job);
    let rect = align.anchor_size(at, galley.size());
    if bold {
        p.galley(rect.min + vec2(0.5, 0.0), galley.clone(), color);
    }
    p.galley(rect.min, galley, color);
    rect
}

/// A neon knob: a dark cap, a dim track, the value as a glowing arc.
/// While it's modulated, a lighter arc runs from the setting to where the
/// modulation has it right now, with a dot there. The pointer stays put.
fn paint_knob_live(
    p: &egui::Painter,
    r: Rect,
    info: &lyra_plugin::ParamInfo,
    v: f32,
    color: Color32,
    hot: bool,
    live: Option<f32>,
) {
    let c = r.center();
    let rad = r.width() * 0.5;
    let n = info.to_normalized(v);
    let (start, sweep) = (0.75 * PI, 1.5 * PI);
    let arc = |from: f32, to: f32, radius: f32| -> Vec<Pos2> {
        let k = ((to - from).abs() / 0.12).ceil().max(2.0) as usize;
        (0..=k).map(|i| c + Vec2::angled(from + (to - from) * i as f32 / k as f32) * radius).collect()
    };
    let ring = rad - 2.5;
    p.add(Shape::line(arc(start, start + sweep, ring), stroke(3.0, hex(0x14282a))));
    let angle = start + sweep * n;
    let (a0, a1) = if info.is_bipolar() {
        let mid = start + sweep * info.to_normalized(0.0);
        (mid.min(angle), mid.max(angle))
    } else {
        (start, angle)
    };
    if info.is_choice() {
        let k = info.labels.len();
        if k <= 16 {
            for i in 0..k {
                let t = start + sweep * i as f32 / (k - 1).max(1) as f32;
                let on = (t - angle).abs() < 1e-3;
                p.circle_filled(
                    c + Vec2::angled(t) * ring,
                    if on { 2.4 } else { 1.3 },
                    if on { color } else { hex(0x234240) },
                );
            }
        }
    } else if a1 - a0 > 0.01 {
        p.add(Shape::line(arc(a0, a1, ring), stroke(7.0, alpha(color, 0.16))));
        p.add(Shape::line(arc(a0, a1, ring), stroke(2.6, color)));
    }
    let cap = rad - 7.0;
    p.circle_filled(c, cap, if hot { hex(0x173033) } else { hex(0x10221f) });
    p.circle_stroke(c, cap, stroke(1.0, hex(0x24423f)));
    let dir = Vec2::angled(angle);
    p.line_segment(
        [c + dir * cap * 0.35, c + dir * cap * 0.9],
        stroke(2.0, if hot { color } else { alpha(color, 0.85) }),
    );
    if let Some(m) = live {
        let to = start + sweep * info.to_normalized(m);
        let bright = theme::mix(color, Color32::WHITE, 0.55);
        p.add(Shape::line(arc(angle.min(to), angle.max(to), ring), stroke(2.0, bright)));
        let at = c + Vec2::angled(to) * ring;
        p.circle_filled(at, 4.5, alpha(bright, 0.25));
        p.circle_filled(at, 2.6, bright);
    }
}

fn chevron(p: &egui::Painter, c: Pos2, w: f32, color: Color32) {
    let pts = vec![c + vec2(-w, -w * 0.43), c + vec2(0.0, w * 0.57), c + vec2(w, -w * 0.43)];
    p.add(Shape::line(pts, stroke(1.4, color)));
}

/// A dark screen (darker than its panel, no edge); what's drawn on it glows.
fn screen(p: &egui::Painter, r: Rect) {
    p.rect_filled(r, cr(4), SCREEN);
}

/// A neon line: a wide faint halo under a bright core.
fn glow_line(p: &egui::Painter, pts: Vec<Pos2>, color: Color32, width: f32) {
    p.add(Shape::line(pts.clone(), stroke(width * 4.5, alpha(color, 0.07))));
    p.add(Shape::line(pts.clone(), stroke(width * 2.2, alpha(color, 0.18))));
    p.add(Shape::line(pts, stroke(width, color)));
}

/// Light under a curve, fading toward the base.
fn glow_fill(p: &egui::Painter, pts: &[Pos2], base: f32, color: Color32) {
    let mut mesh = egui::Mesh::default();
    for q in pts {
        mesh.colored_vertex(*q, alpha(color, 0.16));
        mesh.colored_vertex(pos2(q.x, base), alpha(color, 0.0));
    }
    for k in 0..pts.len().saturating_sub(1) as u32 {
        let (a, b) = (2 * k, 2 * k + 2);
        mesh.add_triangle(a, a + 1, b + 1);
        mesh.add_triangle(a, b + 1, b);
    }
    p.add(Shape::mesh(mesh));
}

/// A draggable point: filled = a point, hollow = a handle.
fn handle(p: &egui::Painter, at: Pos2, color: Color32, solid: bool, hot: bool) {
    if hot {
        p.circle_filled(at, 10.0, alpha(color, 0.15));
    }
    if solid {
        p.circle_filled(at, 4.5, color);
        p.circle_stroke(at, 4.5, stroke(1.0, SCREEN));
    } else {
        p.circle_filled(at, 3.6, SCREEN);
        p.circle_stroke(at, 3.6, stroke(1.4, alpha(color, 0.8)));
    }
}

/// A double-click on `r`, timed by Lyra itself (egui only knows on the frame
/// the button comes back up, so quick clicks slip through).
fn dbl(ui: &Ui, r: &Response, what: impl std::hash::Hash) -> bool {
    r.clicked() && crate::browser::double_click(ui, "chrys-double-click", Id::new(what))
}

/// A drag on a picture moves `id` like a knob (up = more); wheel nudges,
/// double-click resets. `span` = points of travel for the whole range.
fn drag_param(ui: &Ui, ed: &mut Ed, r: &Response, id: usize, span: f32) {
    let info = &PARAMS[id];
    let reset = dbl(ui, r, ("param", id));
    if r.drag_started() || reset {
        ed.h.checkpoint(ed.p);
    }
    if reset {
        ed.put(id, info.default);
    } else if r.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
        let n = info.to_normalized(ed.v(id)) - r.drag_delta().y / span * fine;
        ed.put(id, info.from_normalized(n));
    }
    if let Some(step) = wheel_step(ui, r.rect) {
        ed.h.checkpoint(ed.p);
        let n = info.to_normalized(ed.v(id)) - step as f32 * 0.02;
        ed.put(id, info.from_normalized(n));
    }
}

/// Values as a musician reads them where the generic format isn't enough.
fn format_value(id: usize, v: f32) -> String {
    let info = &PARAMS[id];
    let osc_k = (id < p::SUB_ON).then_some(id % p::OSC_STRIDE);
    match osc_k {
        Some(p::POS) => format!("{:.0}%", v * 100.0),
        Some(p::SEMI) => format!("{:+.0} st", v.round()),
        Some(p::UNISON) => {
            let n = v.round() as usize + 1;
            if n == 1 {
                "1 voice".into()
            } else {
                format!("{n} voices")
            }
        }
        _ if (p::ENVS..p::LFOS).contains(&id) && (id - p::ENVS) % p::E_STRIDE >= p::E_A_CURVE => format!("{v:+.2}"),
        _ if id == p::NOISE_COLOR => {
            if v.abs() < 0.02 {
                "white".into()
            } else if v < 0.0 {
                format!("dark {:.0}", -v * 100.0)
            } else {
                format!("bright {:.0}", v * 100.0)
            }
        }
        _ => info.format(v),
    }
}

fn preset_values(k: usize) -> Vec<f32> {
    let mut v = lyra_plugin::default_params(&PARAMS);
    for &(id, x) in PRESETS[k].1 {
        v[id] = x;
    }
    v
}

fn current_preset(params: &[f32]) -> Option<usize> {
    (0..PRESETS.len())
        .find(|&k| preset_values(k).iter().zip(params).all(|(a, b)| (a - b).abs() <= 1e-5 * a.abs().max(1.0)))
}

/// The table an oscillator plays, for its picture: a built-in, or the
/// channel's import (decoded once and kept, like the engine's own copy).
enum Tab {
    Builtin(&'static Table),
    Imported(Arc<Table>),
}

impl std::ops::Deref for Tab {
    type Target = Table;
    fn deref(&self) -> &Table {
        match self {
            Tab::Builtin(t) => t,
            Tab::Imported(t) => t,
        }
    }
}

fn table_of(ui: &Ui, ed: &Ed, o: usize) -> Tab {
    let idx = ed.choice(p::osc(o, p::TABLE));
    if idx == IMPORTED {
        if let Some(t) = imported_table(ui, ed) {
            return Tab::Imported(t);
        }
    }
    let all = table::builtin();
    Tab::Builtin(&all[idx.min(all.len() - 1)])
}

fn imported_table(ui: &Ui, ed: &Ed) -> Option<Arc<Table>> {
    let name = ed.p.channels[ed.ch].sample.clone()?;
    let id = Id::new(("chrys-import", name.as_str()));
    if let Some(t) = ui.ctx().data(|d| d.get_temp::<Option<Arc<Table>>>(id)) {
        return t;
    }
    let t = crate::samples::decode(&crate::samples::path_for(&name))
        .ok()
        .and_then(|s| Table::from_sample(&s))
        .map(Arc::new);
    ui.ctx().data_mut(|d| d.insert_temp(id, t.clone()));
    t
}

/// The table as stacked frames, back to front, dim; the one that plays glows.
fn table_view(ui: &mut Ui, ed: &mut Ed, o: usize, rect: Rect, color: Color32) {
    let resp = ui.interact(rect, Id::new(("chrys-table", ed.ch, o)), Sense::click_and_drag());
    let enabled = ui.is_enabled();
    if enabled {
        drag_param(ui, ed, &resp, p::osc(o, p::POS), 220.0);
        resp.on_hover_text("Drag up/down or scroll to move through the table (double-click: back to the start)");
    }
    let t = table_of(ui, ed, o);
    let frames = t.frames();
    let painter = ui.painter_at(rect);
    screen(&painter, rect);
    let inner = rect.shrink2(vec2(18.0, 14.0));
    let depth = vec2(inner.width() * 0.22, -inner.height() * 0.4);
    let wave_w = inner.width() - depth.x;
    let amp = (inner.height() + depth.y) * 0.42;
    let base0 = inner.bottom() - (inner.height() + depth.y) * 0.5;
    let level = PICTURE_LEVEL.min(table::LEVELS - 1);
    let place = |d: f32| (inner.left() + d * depth.x, base0 + d * depth.y);
    let mode = ed.choice(p::osc(o, p::WARP_MODE)) as u8;
    // The warp as the matrix has it right now (it moves when modulated).
    let w = modulated(ed, p::osc(o, p::WARP)).unwrap_or_else(|| ed.v(p::osc(o, p::WARP)));
    let warp_shown = if mode == warp::FM { 0 } else { mode };
    // FM: the other oscillator's wave (at its pitch) bends the phase, as in the voice.
    let other = 1 - o;
    let fm = (mode == warp::FM && ed.on(p::osc(other, p::OSC_ON))).then(|| {
        let semis = |q: usize| {
            (ed.choice(p::osc(q, p::OCTAVE)) as f32 - 3.0) * 12.0
                + ed.v(p::osc(q, p::SEMI)).round()
                + ed.v(p::osc(q, p::FINE)) / 100.0
        };
        let ratio = ((semis(other) - semis(o)) / 12.0).exp2();
        let mod_pos = sounding(ed).map_or(ed.v(p::osc(other, p::POS)), |l| l.pos[other]);
        (table_of(ui, ed, other), ratio, mod_pos)
    });
    let n_pts = 120;
    let curve = |fpos: f32, d: f32, warp_mode: u8, w: f32| -> Vec<Pos2> {
        let (x0, y) = place(d);
        (0..=n_pts)
            .map(|k| {
                let ph = k as f32 / n_pts as f32;
                let read = match &fm {
                    Some((t2, ratio, mpos)) => {
                        let m = t2.sample(level, mpos * (t2.frames() - 1) as f32, (ph * ratio).fract());
                        (ph + 1.5 * w * m).rem_euclid(1.0)
                    }
                    None => lyra_chrysalis::warp_preview(warp_mode, w, ph.min(0.999_99)),
                };
                let v = t.sample(level, fpos, read).clamp(-1.2, 1.2);
                pos2(x0 + ph * wave_w, y - v * amp)
            })
            .collect()
    };
    let pos = ed.v(p::osc(o, p::POS));
    let shown = frames.min(30);
    for r in (0..shown).rev() {
        let f = if shown > 1 { r * (frames - 1) / (shown - 1) } else { 0 };
        let d = if frames > 1 { f as f32 / (frames - 1) as f32 } else { 0.0 };
        // Frames near the playing one are lit a little more.
        let near = (1.0 - (d - pos).abs() * 4.0).max(0.0);
        let ink = alpha(color, 0.07 + 0.1 * (1.0 - d) + 0.25 * near);
        painter.add(Shape::line(curve(f as f32, d, 0, 0.0), stroke(1.0, ink)));
    }
    // While notes sound, the bold curve is the frame that plays (modulation
    // included) and the setting stays as a faint line.
    let playing = sounding(ed).filter(|_| enabled && ed.on(p::osc(o, p::OSC_ON))).map(|l| l.pos[o]);
    if let Some(now) = playing {
        if (now - pos).abs() > 0.002 {
            let set = pos * (frames - 1) as f32;
            painter.add(Shape::line(
                curve(set, if frames > 1 { pos } else { 0.0 }, warp_shown, w),
                stroke(1.2, alpha(color, 0.45)),
            ));
        }
    }
    let shown_pos = playing.unwrap_or(pos);
    let fpos = shown_pos * (frames - 1) as f32;
    let d = if frames > 1 { shown_pos } else { 0.0 };
    let pts = curve(fpos, d, warp_shown, w);
    let (_, base) = place(d);
    let ink = if enabled { color } else { TEXT_FAINT };
    breathe(&painter, rect, color, ed.glow);
    glow_fill(&painter, &pts, base, ink);
    glow_line(&painter, pts, ink, 1.8);
    let caption = if frames > 1 { format!("{:.0} / {frames}", fpos.round() + 1.0) } else { "1 frame".to_string() };
    caps(&painter, rect.right_bottom() - vec2(10.0, 9.0), &caption, TEXT_DIM, Align2::RIGHT_CENTER);
}

/// What the drive does to a sine, as the voices shape it.
fn drive_view(ui: &Ui, ed: &Ed, r: Rect) {
    let painter = ui.painter_at(r);
    screen(&painter, r);
    breathe(&painter, r, LIME, ed.glow);
    let inner = r.shrink2(vec2(10.0, 8.0));
    painter.line_segment([inner.left_center(), inner.right_center()], stroke(1.0, GRID));
    let mode = ed.choice(p::DIST_MODE) as u8;
    let (gain, steps, _) = lyra_chrysalis::drive_consts(ed.v(p::DIST_DRIVE));
    let mix = if mode == 0 { 0.0 } else { ed.v(p::DIST_MIX) };
    let shape = |x: f32| x + (lyra_chrysalis::drive_shape(mode, gain, steps, x) - x) * mix;
    let pts: Vec<Pos2> = (0..=200)
        .map(|k| {
            let t = k as f32 / 200.0;
            let x = 0.9 * (2.0 * PI * 2.0 * t).sin();
            pos2(inner.left() + t * inner.width(), inner.center().y - shape(x).clamp(-1.0, 1.0) * inner.height() * 0.5)
        })
        .collect();
    glow_line(&painter, pts, if mode == 0 { TEXT_FAINT } else { LIME }, 1.5);
}

/// One filter's response at `hz` (complex), as the voice computes it.
fn response(kind: u8, fc: f32, reso: f32, vowel: f32, mix: f32, hz: f32, sr: f32) -> (f32, f32) {
    let pre = |f: f32| (PI * f.min(sr * 0.49) / sr).tan();
    let fc = fc.clamp(20.0, sr * 0.45);
    let div = |nr: f32, ni: f32, dr: f32, di: f32| {
        let m = dr * dr + di * di;
        ((nr * dr + ni * di) / m, (ni * dr - nr * di) / m)
    };
    let mul = |a: (f32, f32), b: (f32, f32)| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
    // An SVF's H(w) with w = tan(πf/sr) / tan(πfc/sr).
    let svf = |w: f32, k: f32, num: (f32, f32)| div(num.0, num.1, 1.0 - w * w, k * w);
    let k = 2.0 * (1.0 - 0.985 * reso);
    let w = pre(hz) / pre(fc);
    let y = match kind {
        ft::LOW12 => svf(w, k, (1.0, 0.0)),
        ft::LOW24 => mul(svf(w, k, (1.0, 0.0)), svf(w, std::f32::consts::SQRT_2, (1.0, 0.0))),
        ft::HIGH12 => svf(w, k, (-w * w, 0.0)),
        ft::BAND => svf(w, k, (0.0, k * w)),
        ft::NOTCH => svf(w, k, (1.0 - w * w, 0.0)),
        ft::FORMANT => {
            let shift = (fc / 1000.0).sqrt().clamp(0.5, 2.0);
            let narrow = 2f32.powf(-2.0 * reso);
            let mut sum = (0.0, 0.0);
            for &(f0, gain, bw) in table::vowel_at(vowel).iter() {
                let f = (f0 * shift).min(sr * 0.45);
                let kf = (bw * narrow * shift / f).clamp(0.01, 2.0);
                let wf = pre(hz) / pre(f);
                let b = svf(wf, kf, (0.0, kf * wf));
                sum.0 += 2.0 * gain * b.0;
                sum.1 += 2.0 * gain * b.1;
            }
            sum
        }
        _ => {
            // 1 / (1 ∓ fb·e^(−jωD)), scaled by √(1 − fb²).
            let fb = 0.97 * reso;
            let sign = if kind == ft::COMB_PLUS { 1.0 } else { -1.0 };
            let ph = 2.0 * PI * hz / fc;
            div((1.0 - fb * fb).sqrt(), 0.0, 1.0 - sign * fb * ph.cos(), sign * fb * ph.sin())
        }
    };
    (1.0 + (y.0 - 1.0) * mix, y.1 * mix)
}

fn mag(h: (f32, f32)) -> f32 {
    (h.0 * h.0 + h.1 * h.1).sqrt().max(1e-6)
}

#[allow(clippy::needless_range_loop)] // two filters, indexed in params and in `hot`
fn filter_graph(ui: &mut Ui, ed: &mut Ed, rect: Rect) {
    let inner = rect.shrink2(vec2(6.0, 6.0));
    let (lo, hi) = (20f32.ln(), 20_000f32.ln());
    let fx = |hz: f32| inner.left() + (hz.ln() - lo) / (hi - lo) * inner.width();
    let hz_at = |x: f32| (lo + (x - inner.left()) / inner.width() * (hi - lo)).exp();
    let (db_top, db_bot) = (24.0, -36.0);
    let fy = |db: f32| inner.top() + (db_top - db.clamp(db_bot, db_top)) / (db_top - db_bot) * inner.height();
    let sr = 48_000.0;
    let parallel = ed.on(p::ROUTING);
    let one = |ed: &Ed, f: usize, hz: f32| {
        let at = |k: usize| ed.v(p::filter(f, k));
        let kind = ed.choice(p::filter(f, p::F_TYPE)) as u8;
        response(kind, at(p::F_CUTOFF), at(p::F_RESO), at(p::F_VOWEL), at(p::F_MIX), hz, sr)
    };
    let total = |ed: &Ed, hz: f32| {
        let (a, b) = (ed.on(p::filter(0, p::F_ON)), ed.on(p::filter(1, p::F_ON)));
        let (h1, h2) = (one(ed, 0, hz), one(ed, 1, hz));
        let h = match (a, b) {
            (true, true) if parallel => (h1.0 + h2.0, h1.1 + h2.1),
            (true, true) => (h1.0 * h2.0 - h1.1 * h2.1, h1.0 * h2.1 + h1.1 * h2.0),
            (true, false) => h1,
            (false, true) => h2,
            _ => (1.0, 0.0),
        };
        20.0 * mag(h).log10()
    };
    let mut hot = [false; 2];
    for f in 0..2 {
        if !ed.on(p::filter(f, p::F_ON)) {
            continue;
        }
        let cut_id = p::filter(f, p::F_CUTOFF);
        let reso_id = p::filter(f, p::F_RESO);
        let at = pos2(fx(ed.v(cut_id)), fy(20.0 * mag(one(ed, f, ed.v(cut_id))).log10()));
        let id = Id::new(("chrys-fdot", ed.ch, f));
        let r = ui.interact(Rect::from_center_size(at, vec2(22.0, 22.0)), id, Sense::click_and_drag());
        let reset = dbl(ui, &r, ("fdot", f));
        if r.drag_started() || reset {
            ed.h.checkpoint(ed.p);
        }
        if reset {
            ed.put(cut_id, PARAMS[cut_id].default);
            ed.put(reso_id, PARAMS[reso_id].default);
        } else if r.dragged() {
            if let Some(pos) = r.interact_pointer_pos() {
                ed.put(cut_id, hz_at(pos.x));
                ed.put(reso_id, ed.v(reso_id) - r.drag_delta().y / 150.0);
            }
        }
        hot[f] = r.hovered() || r.dragged();
        r.on_hover_text(format!(
            "Filter {}: {} · reso {}",
            f + 1,
            PARAMS[cut_id].format(ed.v(cut_id)),
            PARAMS[reso_id].format(ed.v(reso_id))
        ));
    }
    let painter = ui.painter_at(rect);
    screen(&painter, rect);
    breathe(&painter, rect, AQUA, ed.glow);
    for hz in [100.0, 1000.0, 10_000.0] {
        painter.line_segment([pos2(fx(hz), inner.top()), pos2(fx(hz), inner.bottom())], stroke(1.0, GRID));
    }
    painter.line_segment([pos2(inner.left(), fy(0.0)), pos2(inner.right(), fy(0.0))], stroke(1.0, GRID));
    let any = ed.on(p::filter(0, p::F_ON)) || ed.on(p::filter(1, p::F_ON));
    let n = (inner.width() * 1.2) as usize;
    let pts: Vec<Pos2> = (0..=n)
        .map(|k| {
            let x = inner.left() + k as f32 / n as f32 * inner.width();
            pos2(x, fy(total(ed, hz_at(x))))
        })
        .collect();
    let ink = if any { AQUA } else { TEXT_FAINT };
    glow_fill(&painter, &pts, inner.bottom(), ink);
    glow_line(&painter, pts, ink, 1.6);
    for f in 0..2 {
        if ed.on(p::filter(f, p::F_ON)) {
            let cut = ed.v(p::filter(f, p::F_CUTOFF));
            let at = pos2(fx(cut), fy(20.0 * mag(one(ed, f, cut)).log10()));
            handle(&painter, at, AQUA, f == 0, hot[f]);
        }
    }
    for (hz, label) in [(100.0, "100"), (1000.0, "1k"), (10_000.0, "10k")] {
        painter.text(
            pos2(fx(hz) + 3.0, inner.bottom()),
            Align2::LEFT_BOTTOM,
            label,
            FontId::proportional(9.5),
            TEXT_FAINT,
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModView {
    Env(usize),
    Lfo(usize),
    Matrix,
}

impl ModView {
    fn index(self) -> usize {
        match self {
            ModView::Env(e) => e,
            ModView::Lfo(l) => 4 + l,
            ModView::Matrix => 8,
        }
    }
    fn from_index(i: usize) -> Self {
        match i {
            0..=3 => ModView::Env(i),
            4..=7 => ModView::Lfo(i - 4),
            _ => ModView::Matrix,
        }
    }
}

const MOD_TABS: [&str; 9] = ["amp", "env 2", "env 3", "env 4", "lfo 1", "lfo 2", "lfo 3", "lfo 4", "matrix"];

/// Is this mod tab's source used by any slot? (The amp envelope always is.)
fn mod_used(ed: &Ed, tab: usize) -> bool {
    use lyra_chrysalis::params::src;
    let live = |s: usize| ed.choice(p::slot(s, p::S_DEST)) != 0 && ed.choice(p::slot(s, p::S_SRC)) != 0;
    let source = match tab {
        0 => return true,
        1..=3 => src::ENV + tab,
        4..=7 => src::LFO + tab - 4,
        _ => return (0..p::SLOT_COUNT).any(live),
    } as f32;
    (0..p::SLOT_COUNT)
        .any(|s| live(s) && (ed.v(p::slot(s, p::S_SRC)) == source || ed.v(p::slot(s, p::S_VIA)) == source))
}

/// An ADSR you can drag: dots for the times and the sustain, rings in the
/// middle of each stage for its curve. Each stage has its own lane, so a time
/// sits where its knob points. Double-click resets what a handle moves.
fn envelope(ui: &mut Ui, ed: &mut Ed, e: usize, rect: Rect) {
    let at = |k: usize| p::env(e, k);
    let (a_id, d_id, s_id, r_id) = (at(p::E_ATTACK), at(p::E_DECAY), at(p::E_SUSTAIN), at(p::E_RELEASE));
    let curves = [at(p::E_A_CURVE), at(p::E_D_CURVE), at(p::E_R_CURVE)];
    let inner = rect.shrink2(vec2(14.0, 14.0));
    let lane = inner.width() / 3.5;
    let hold = lane * 0.5;
    let n = |id: usize, ed: &Ed| PARAMS[id].to_normalized(ed.v(id));
    let geom = |ed: &Ed| {
        let xa = inner.left() + n(a_id, ed) * lane;
        let xd = xa + n(d_id, ed) * lane;
        let xs = inner.left() + 2.0 * lane + hold;
        let xr = xs + n(r_id, ed) * lane;
        (xa, xd, xs, xr, ed.v(s_id))
    };
    let y = |level: f32| inner.bottom() - level * inner.height();
    let stages = |ed: &Ed| {
        let (xa, xd, xs, xr, s) = geom(ed);
        [(inner.left(), 0.0, xa, 1.0, curves[0]), (xa, 1.0, xd, s, curves[1]), (xs, s, xr, 0.0, curves[2])]
    };
    let base = Id::new(("chrys-envdots", ed.ch, e));
    let mut hot = None;
    let (xa, xd, _, xr, s) = geom(ed);
    for (k, at) in [pos2(xa, y(1.0)), pos2(xd, y(s)), pos2(xr, y(0.0))].into_iter().enumerate() {
        let r = ui.interact(Rect::from_center_size(at, vec2(20.0, 20.0)), base.with(k), Sense::click_and_drag());
        let reset = dbl(ui, &r, ("envdot", e, k));
        if r.drag_started() || reset {
            ed.h.checkpoint(ed.p);
        }
        let resets: &[usize] = match k {
            0 => &[a_id],
            1 => &[d_id, s_id],
            _ => &[r_id],
        };
        if reset {
            for &id in resets {
                ed.put(id, PARAMS[id].default);
            }
        } else if r.dragged() {
            if let Some(pos) = r.interact_pointer_pos() {
                let (xa, _, xs, _, _) = geom(ed);
                let from_lane = |x0: f32, id: usize| PARAMS[id].from_normalized((pos.x - x0) / lane);
                match k {
                    0 => ed.put(a_id, from_lane(inner.left(), a_id)),
                    1 => {
                        ed.put(d_id, from_lane(xa, d_id));
                        ed.put(s_id, (inner.bottom() - pos.y) / inner.height());
                    }
                    _ => ed.put(r_id, from_lane(xs, r_id)),
                }
            }
        }
        if r.hovered() || r.dragged() {
            hot = Some(k);
        }
        let tip = resets.iter().map(|&id| format!("{} {}", PARAMS[id].name, PARAMS[id].format(ed.v(id))));
        r.on_hover_text(tip.collect::<Vec<_>>().join(" · "));
    }
    for (k, (x0, y0, x1, y1, cid)) in stages(ed).into_iter().enumerate() {
        if (y1 - y0).abs() < 0.02 || x1 - x0 < 6.0 {
            continue;
        }
        let mid = pos2(0.5 * (x0 + x1), y(y0 + (y1 - y0) * curve::bend(0.5, ed.v(cid))));
        let r = ui.interact(Rect::from_center_size(mid, vec2(18.0, 18.0)), base.with(10 + k), Sense::click_and_drag());
        let reset = dbl(ui, &r, ("envring", e, k));
        if r.drag_started() || reset {
            ed.h.checkpoint(ed.p);
        }
        if reset {
            ed.put(cid, PARAMS[cid].default);
        } else if r.dragged() {
            if let Some(pos) = r.interact_pointer_pos() {
                let level = (inner.bottom() - pos.y) / inner.height();
                ed.put(cid, curve::bend_through((level - y0) / (y1 - y0)));
            }
        }
        if r.hovered() || r.dragged() {
            hot = Some(10 + k);
        }
        r.on_hover_text(format!("{}: drag up/down to bend", PARAMS[cid].name));
    }

    let painter = ui.painter_at(rect);
    screen(&painter, rect);
    let (xa, xd, xs, xr, s) = geom(ed);
    for x in [inner.left() + lane, inner.left() + 2.0 * lane, xs] {
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], stroke(1.0, GRID));
    }
    let mut pts = vec![pos2(inner.left(), y(0.0))];
    for (k, (x0, y0, x1, y1, cid)) in stages(ed).into_iter().enumerate() {
        if k == 2 {
            pts.push(pos2(xs, y(s)));
        }
        let b = ed.v(cid);
        for i in 1..=32 {
            let t = i as f32 / 32.0;
            pts.push(pos2(x0 + (x1 - x0) * t, y(y0 + (y1 - y0) * curve::bend(t, b))));
        }
    }
    glow_fill(&painter, &pts, inner.bottom(), VIOLET);
    glow_line(&painter, pts, VIOLET, 1.6);
    for (k, (x0, y0, x1, y1, cid)) in stages(ed).into_iter().enumerate() {
        if (y1 - y0).abs() >= 0.02 && x1 - x0 >= 6.0 {
            let mid = pos2(0.5 * (x0 + x1), y(y0 + (y1 - y0) * curve::bend(0.5, ed.v(cid))));
            handle(&painter, mid, VIOLET, false, hot == Some(10 + k));
        }
    }
    for (k, at) in [pos2(xa, y(1.0)), pos2(xd, y(s)), pos2(xr, y(0.0))].into_iter().enumerate() {
        handle(&painter, at, VIOLET, true, hot == Some(k));
    }
    // The newest note's place on it: through each stage's lane by its
    // progress, at the level the envelope really has.
    if let Some(live) = sounding(ed) {
        let (stage, t, level) = (live.env_stage[e], live.env_t[e].clamp(0.0, 1.0), live.env_level[e]);
        let x = match stage {
            1 => inner.left() + (xa - inner.left()) * t,
            2 => xa + (xd - xa) * t,
            3 => xd + (xs - xd) * 0.5,
            4 => xs + (xr - xs) * t,
            _ => f32::NAN,
        };
        if x.is_finite() {
            let at = pos2(x, y(level.clamp(0.0, 1.0)));
            let bright = theme::mix(VIOLET, Color32::WHITE, 0.6);
            painter.circle_filled(at, 9.0, alpha(VIOLET, 0.25));
            painter.circle_filled(at, 4.0, bright);
        }
    }
    breathe(&painter, rect, VIOLET, ed.glow);
}

fn read_points(ed: &Ed, l: usize) -> Vec<Point> {
    let n = (ed.v(p::lfo(l, p::L_POINTS)).round() as usize).clamp(1, MAX_POINTS);
    (0..n)
        .map(|k| Point { x: ed.v(p::point_x(l, k)), y: ed.v(p::point_y(l, k)), bend: ed.v(p::point_bend(l, k)) })
        .collect()
}

/// Write a whole shape (no checkpoint). Unused points go back to their defaults.
fn write_points(ed: &mut Ed, l: usize, pts: &[Point]) {
    ed.put(p::lfo(l, p::L_POINTS), pts.len() as f32);
    for k in 0..MAX_POINTS {
        let (xi, yi, bi) = (p::point_x(l, k), p::point_y(l, k), p::point_bend(l, k));
        match pts.get(k) {
            Some(pt) => {
                ed.put(xi, if k == 0 { 0.0 } else { pt.x });
                ed.put(yi, pt.y);
                ed.put(bi, pt.bend);
            }
            None => {
                ed.put(xi, PARAMS[xi].default);
                ed.put(yi, PARAMS[yi].default);
                ed.put(bi, PARAMS[bi].default);
            }
        }
    }
}

/// Draw the LFO: points, bends, adding and removing.
fn lfo_editor(ui: &mut Ui, ed: &mut Ed, l: usize, rect: Rect) {
    let bg = ui.interact(rect, Id::new(("chrys-lfo-bg", ed.ch, l)), Sense::click());
    let inner = rect.shrink2(vec2(14.0, 12.0));
    let sx = |x: f32| inner.left() + x * inner.width();
    let sy = |y: f32| inner.bottom() - y * inner.height();
    let fx = |px: f32| ((px - inner.left()) / inner.width()).clamp(0.0, 1.0);
    let fy = |py: f32| ((inner.bottom() - py) / inner.height()).clamp(0.0, 1.0);
    let free = ui.input(|i| i.modifiers.alt);
    let snap = |v: f32, steps: f32| if free { v } else { (v * steps).round() / steps };
    let base = Id::new(("chrys-lfo-ed", ed.ch, l));
    let mut pts = read_points(ed, l);
    let n = pts.len();
    let mut hot = None;
    let mut changed = false;
    let mut remove = None;
    for k in 0..n {
        let at = pos2(sx(pts[k].x), sy(pts[k].y));
        let r = ui.interact(Rect::from_center_size(at, vec2(18.0, 18.0)), base.with(k), Sense::click_and_drag());
        if r.drag_started() {
            ed.h.checkpoint(ed.p);
        }
        if (dbl(ui, &r, ("lfopt", l, k)) || r.secondary_clicked()) && k > 0 {
            remove = Some(k);
        } else if r.dragged() {
            if let Some(pos) = r.interact_pointer_pos() {
                let lo = if k == 0 { 0.0 } else { pts[k - 1].x };
                let hi = if k + 1 < n { pts[k + 1].x } else { 1.0 };
                pts[k].x = if k == 0 { 0.0 } else { snap(fx(pos.x), 32.0).clamp(lo, hi) };
                pts[k].y = snap(fy(pos.y), 16.0);
                changed = true;
            }
        }
        if r.hovered() || r.dragged() {
            hot = Some(k);
        }
        r.on_hover_text(format!(
            "{:.0}% · {:.0}%  (drag; Alt: off the grid; double-click or right-click: remove)",
            pts[k].x * 100.0,
            pts[k].y * 100.0
        ));
    }
    let seg_of = |pts: &[Point], k: usize| {
        (pts[k], if k + 1 < pts.len() { pts[k + 1] } else { Point { x: 1.0, y: pts[0].y, bend: 0.0 } })
    };
    for k in 0..n {
        let (a, b) = seg_of(&pts, k);
        if (b.y - a.y).abs() < 0.02 || sx(b.x) - sx(a.x) < 10.0 {
            continue;
        }
        let mid = pos2(sx(0.5 * (a.x + b.x)), sy(a.y + (b.y - a.y) * curve::bend(0.5, a.bend)));
        let r = ui.interact(Rect::from_center_size(mid, vec2(16.0, 16.0)), base.with(100 + k), Sense::click_and_drag());
        let reset = dbl(ui, &r, ("lfobend", l, k));
        if r.drag_started() || reset {
            ed.h.checkpoint(ed.p);
        }
        if reset {
            pts[k].bend = 0.0;
            changed = true;
        } else if r.dragged() {
            if let Some(pos) = r.interact_pointer_pos() {
                pts[k].bend = curve::bend_through((fy(pos.y) - a.y) / (b.y - a.y));
                changed = true;
            }
        }
        if r.hovered() || r.dragged() {
            hot = Some(100 + k);
        }
        r.on_hover_text("Drag up/down to bend this segment (double-click: straight)");
    }
    if dbl(ui, &bg, ("lfoadd", l)) && hot.is_none() && n < MAX_POINTS {
        if let Some(pos) = bg.interact_pointer_pos() {
            let (x, y) = (snap(fx(pos.x), 32.0).max(1e-4), snap(fy(pos.y), 16.0));
            let at = pts.iter().position(|q| q.x > x).unwrap_or(n);
            ed.h.checkpoint(ed.p);
            pts.insert(at.max(1), Point { x, y, bend: 0.0 });
            changed = true;
        }
    }
    if let Some(k) = remove {
        ed.h.checkpoint(ed.p);
        pts.remove(k);
        changed = true;
    }
    if changed {
        write_points(ed, l, &pts);
    }
    bg.on_hover_text("Double-click to add a point");

    let pts = read_points(ed, l);
    let painter = ui.painter_at(rect);
    screen(&painter, rect);
    for k in 1..16 {
        let x = sx(k as f32 / 16.0);
        let c = if k % 4 == 0 { GRID } else { alpha(GRID, 0.45) };
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], stroke(1.0, c));
    }
    for k in 1..4 {
        let yy = sy(k as f32 / 4.0);
        painter.line_segment([pos2(inner.left(), yy), pos2(inner.right(), yy)], stroke(1.0, alpha(GRID, 0.45)));
    }
    let shape = curve::Shape::new(&pts);
    let m = (inner.width() * 1.5) as usize;
    // Sample densely, and right at every point's x so jumps stay vertical.
    let mut xs: Vec<f32> = (0..=m).map(|k| k as f32 / m as f32).collect();
    for q in &pts {
        xs.push(q.x);
        xs.push((q.x - 1e-5).max(0.0));
    }
    xs.sort_by(f32::total_cmp);
    let line: Vec<Pos2> = xs.into_iter().map(|x| pos2(sx(x), sy(shape.eval(x.min(0.999_999))))).collect();
    glow_fill(&painter, &line, inner.bottom(), VIOLET);
    glow_line(&painter, line, VIOLET, 1.6);
    for k in 0..pts.len() {
        let (a, b) = seg_of(&pts, k);
        if (b.y - a.y).abs() >= 0.02 && sx(b.x) - sx(a.x) >= 10.0 {
            let mid = pos2(sx(0.5 * (a.x + b.x)), sy(a.y + (b.y - a.y) * curve::bend(0.5, a.bend)));
            handle(&painter, mid, VIOLET, false, hot == Some(100 + k));
        }
    }
    for (k, q) in pts.iter().enumerate() {
        handle(&painter, pos2(sx(q.x), sy(q.y)), VIOLET, k > 0, hot == Some(k));
    }
    // The playhead: a free LFO always runs (it's shared); a per-note one
    // shows the newest note's.
    let free = ed.choice(p::lfo(l, p::L_MODE)) == 0; // (a phrase LFO only moves with notes)
    if let Some(live) = ed.live.filter(|lv| free || lv.voices > 0) {
        let (ph, val) = (live.lfo_phase[l].clamp(0.0, 1.0), live.lfo_value[l].clamp(0.0, 1.0));
        let x = sx(ph);
        let bright = theme::mix(VIOLET, Color32::WHITE, 0.6);
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], stroke(5.0, alpha(VIOLET, 0.12)));
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], stroke(1.2, alpha(bright, 0.7)));
        let at = pos2(x, sy(val));
        painter.circle_filled(at, 9.0, alpha(VIOLET, 0.25));
        painter.circle_filled(at, 4.0, bright);
    }
    breathe(&painter, rect, VIOLET, ed.glow);
}

/// A key with an LFO shape drawn on it: solid violet when it's the shape you have.
fn paint_shape_key(p: &egui::Painter, rect: Rect, shape: &[Point], lit: bool, hot: bool) {
    let fill = if lit {
        VIOLET
    } else if hot {
        HOVER
    } else {
        SCREEN
    };
    p.rect_filled(rect, cr(3), fill);
    let face = rect.shrink2(vec2(9.0, 8.0));
    let s = curve::Shape::new(shape);
    let pts: Vec<Pos2> = (0..=40)
        .map(|k| {
            let t = k as f32 / 40.0;
            pos2(face.left() + t * face.width(), face.bottom() - s.eval(t.min(0.9999)) * face.height())
        })
        .collect();
    p.add(Shape::line(pts, stroke(if lit { 2.0 } else { 1.5 }, if lit { INK_ON } else { TEXT_DIM })));
}
