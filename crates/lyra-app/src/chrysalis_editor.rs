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
use lyra_chrysalis::{p, PARAMS, PRESETS};
use lyra_model::{History, Project};
use std::f32::consts::PI;
use std::sync::Arc;

// ───────────────────────────────────────────────────────────── the look

const fn hex(v: u32) -> Color32 {
    theme::hex(v)
}

/// Behind everything: teal-black, with violet glowing in at the corners.
const VOID: Color32 = hex(0x050c0e);
const VIOLET_GLOW: Color32 = Color32::from_rgba_premultiplied(0x22, 0x14, 0x48, 0x60);
const PANEL: Color32 = hex(0x0a1618);
const PANEL_EDGE: Color32 = hex(0x17302f);
/// Screens and recesses: darker than the panels.
const SCREEN: Color32 = hex(0x030809);
const SCREEN_EDGE: Color32 = hex(0x112426);
const GRID: Color32 = Color32::from_rgba_premultiplied(0x10, 0x2a, 0x2a, 0x80);
const TEXT: Color32 = hex(0xcde8df);
const TEXT_DIM: Color32 = hex(0x6a8a84);
const TEXT_FAINT: Color32 = hex(0x3a5450);

const GREEN: Color32 = hex(0x8cff7a);
const CYAN: Color32 = hex(0x45e6da);
const LIME: Color32 = hex(0xc8ff5e);
const AQUA: Color32 = hex(0x5cc8ff);
const VIOLET: Color32 = hex(0x9b7bff);
const OSC_COLORS: [Color32; 2] = [GREEN, CYAN];

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
}

impl Ed<'_> {
    fn v(&self, id: usize) -> f32 {
        self.p.channels[self.ch].params[id]
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

const W: f32 = 1100.0;
const HEADER_H: f32 = 80.0;
const OSC_H: f32 = 316.0;
const FX_H: f32 = 206.0;
const MOD_H: f32 = 250.0;
const GAP: f32 = 6.0;
const PAD: f32 = 10.0;
const TITLE_H: f32 = 20.0;
const KEY_H: f32 = 22.0;
const KNOB: f32 = 34.0;
const CELL: Vec2 = vec2(64.0, 64.0);

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, ch: usize) {
    let mut ed = Ed { p, h, ch };
    let total = vec2(W, HEADER_H + OSC_H + FX_H + MOD_H + 3.0 * GAP);
    let (all, _) = ui.allocate_exact_size(total, Sense::hover());
    backdrop(ui, all);
    let mut y = all.top();
    let row = |y: &mut f32, h: f32| {
        let r = Rect::from_min_size(pos2(all.left(), *y), vec2(W, h));
        *y += h + GAP;
        r
    };
    let header = row(&mut y, HEADER_H);
    let oscs = row(&mut y, OSC_H);
    let fx = row(&mut y, FX_H);
    let mods = row(&mut y, MOD_H);

    header_bar(ui, &mut ed, header);
    let half = (W - GAP) / 2.0;
    for o in 0..2 {
        let r = Rect::from_min_size(pos2(oscs.left() + o as f32 * (half + GAP), oscs.top()), vec2(half, OSC_H));
        osc_panel(ui, &mut ed, o, r);
    }
    let (sub_w, drive_w) = (212.0, 236.0);
    let sub_r = Rect::from_min_size(fx.min, vec2(sub_w, FX_H));
    let drive_r = Rect::from_min_size(pos2(sub_r.right() + GAP, fx.top()), vec2(drive_w, FX_H));
    let filter_r = Rect::from_min_max(pos2(drive_r.right() + GAP, fx.top()), fx.max);
    sub_panel(ui, &mut ed, sub_r);
    drive_panel(ui, &mut ed, drive_r);
    filter_panel(ui, &mut ed, filter_r);
    let macros_w = 252.0;
    let mod_r = Rect::from_min_size(mods.min, vec2(W - macros_w - GAP, MOD_H));
    let mac_r = Rect::from_min_max(pos2(mod_r.right() + GAP, mods.top()), mods.max);
    mod_panel(ui, &mut ed, mod_r);
    macros_panel(ui, &mut ed, mac_r);
}

/// The whole editor's ground: teal-black, violet glowing in at two corners.
fn backdrop(ui: &Ui, r: Rect) {
    let p = ui.painter();
    p.rect_filled(r.expand(4.0), cr(4), VOID);
    for (at, radius) in [(r.left_bottom() + vec2(60.0, -40.0), 380.0), (r.right_top() + vec2(-80.0, 60.0), 300.0)] {
        theme::radial_circle(p, at, radius, at, &[(0.0, VIOLET_GLOW), (1.0, Color32::TRANSPARENT)]);
    }
}

/// A panel: dark teal, a thin edge, and its name in spaced capitals with a
/// short line of its colour. Returns the inside, below the title.
fn panel(ui: &Ui, r: Rect, title: &str, color: Color32) -> Rect {
    let p = ui.painter();
    p.rect_filled(r, cr(5), PANEL);
    p.rect_stroke(r, cr(5), stroke(1.0, PANEL_EDGE), egui::StrokeKind::Inside);
    if !title.is_empty() {
        let at = r.left_top() + vec2(PAD, PAD + 6.0);
        p.line_segment([at, at + vec2(10.0, 0.0)], stroke(2.0, color));
        caps(p, at + vec2(16.0, 0.0), title, TEXT_DIM, Align2::LEFT_CENTER);
    }
    Rect::from_min_max(r.left_top() + vec2(PAD, PAD + TITLE_H), r.right_bottom() - vec2(PAD, PAD))
}

/// Small spaced capitals.
fn caps(p: &egui::Painter, at: Pos2, text: &str, color: Color32, align: Align2) -> Rect {
    caps_spaced(p, at, text, color, align, 1.6)
}

/// Small capitals, `spacing` points apart (switch segments use less).
fn caps_spaced(p: &egui::Painter, at: Pos2, text: &str, color: Color32, align: Align2, spacing: f32) -> Rect {
    let mut job = LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: FontId::proportional(10.0),
            color,
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    );
    let galley = p.layout_job(job);
    let rect = align.anchor_size(at, galley.size());
    p.galley(rect.min, galley, color);
    rect
}

/// A child Ui laid out left to right inside `r`.
fn row_ui(ui: &mut Ui, r: Rect, salt: impl std::hash::Hash) -> Ui {
    let mut child = ui.new_child(UiBuilder::new().id_salt(salt).max_rect(r).layout(Layout::left_to_right(Align::Min)));
    child.spacing_mut().item_spacing = vec2(4.0, 4.0);
    child
}

// ───────────────────────────────────────────────────────────── controls

/// A neon knob: a dark cap, a dim track, the value as a glowing arc.
fn paint_knob(p: &egui::Painter, r: Rect, info: &lyra_plugin::ParamInfo, v: f32, color: Color32, hot: bool) {
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
}

/// A knob with its name and value under it; disabled = faded and inert.
fn knob(ui: &mut Ui, ed: &mut Ed, id: usize, label: &str, color: Color32, enabled: bool) {
    knob_sized(ui, ed, id, label, color, enabled, KNOB, CELL);
}

#[allow(clippy::too_many_arguments)]
fn knob_sized(ui: &mut Ui, ed: &mut Ed, id: usize, label: &str, color: Color32, enabled: bool, d: f32, cell: Vec2) {
    let info = &PARAMS[id];
    ui.allocate_ui_with_layout(cell, Layout::top_down(Align::Center), |ui| {
        ui.add_enabled_ui(enabled, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let mut v = ed.v(id);
            let r = crate::widgets::knob_painted(ui, &mut v, info, d, |p, rect, v, hot| {
                paint_knob(p, rect, info, v, color, hot)
            });
            if enabled {
                ed.track(&r, id, v);
            }
            let (lr, _) = ui.allocate_exact_size(vec2(cell.x, 24.0), Sense::hover());
            let painter = ui.painter();
            painter.text(lr.center_top(), Align2::CENTER_TOP, label, FontId::proportional(10.5), TEXT_DIM);
            painter.text(
                lr.center_top() + vec2(0.0, 12.0),
                Align2::CENTER_TOP,
                format_value(id, ed.v(id)),
                FontId::proportional(10.5),
                TEXT,
            );
        });
    });
}

/// Segmented switch: one recess, the chosen segment glows. Returns the click.
fn seg(ui: &mut Ui, labels: &[&str], chosen: usize, color: Color32, seg_w: f32, hint: &str) -> Option<usize> {
    let size = vec2(seg_w * labels.len() as f32, KEY_H);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    p.rect_filled(r, cr(3), SCREEN);
    p.rect_stroke(r, cr(3), stroke(1.0, SCREEN_EDGE), egui::StrokeKind::Inside);
    let hover = resp.hover_pos().map(|pos| (((pos.x - r.left()) / seg_w) as usize).min(labels.len() - 1));
    for (i, l) in labels.iter().enumerate() {
        let s = Rect::from_min_size(pos2(r.left() + i as f32 * seg_w, r.top()), vec2(seg_w, KEY_H)).shrink(2.0);
        let on = i == chosen;
        if on {
            p.rect_filled(s, cr(2), alpha(color, 0.2));
            p.rect_stroke(s, cr(2), stroke(1.0, alpha(color, 0.8)), egui::StrokeKind::Inside);
        } else if hover == Some(i) {
            p.rect_filled(s, cr(2), hex(0x0f2022));
        }
        let ink = if on {
            color
        } else if hover == Some(i) {
            TEXT
        } else {
            TEXT_DIM
        };
        caps_spaced(p, s.center(), l, ink, Align2::CENTER_CENTER, 0.6);
    }
    let clicked = resp.clicked().then_some(()).and(hover);
    resp.on_hover_text(hint);
    clicked
}

/// A round power button for on/off parameters.
fn power(ui: &mut Ui, ed: &mut Ed, id: usize, color: Color32, hint: &str) {
    let (r, resp) = ui.allocate_exact_size(vec2(KEY_H, KEY_H), Sense::click());
    let on = ed.on(id);
    let p = ui.painter();
    let c = r.center();
    if on {
        p.circle_filled(c, 9.0, alpha(color, 0.18));
    }
    p.circle_filled(c, 6.5, if on { color } else { SCREEN });
    let edge = if on {
        color
    } else if resp.hovered() {
        TEXT_DIM
    } else {
        hex(0x2a4a48)
    };
    p.circle_stroke(c, 6.5, stroke(1.0, edge));
    if resp.on_hover_text(hint).clicked() {
        ed.toggle(id);
    }
}

/// A dropdown for a choice parameter: click for the list, scroll to step.
fn well(ui: &mut Ui, ed: &mut Ed, id: usize, width: f32, color: Color32, hint: &str) {
    let info = &PARAMS[id];
    let n = info.labels.len();
    let at = ed.choice(id).min(n - 1);
    let (r, resp) = ui.allocate_exact_size(vec2(width, KEY_H), Sense::click());
    let p = ui.painter();
    p.rect_filled(r, cr(3), SCREEN);
    let edge = if resp.hovered() { alpha(color, 0.6) } else { SCREEN_EDGE };
    p.rect_stroke(r, cr(3), stroke(1.0, edge), egui::StrokeKind::Inside);
    let ink = if at == 0 { TEXT_DIM } else { TEXT };
    p.text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, info.labels[at], FontId::proportional(11.5), ink);
    chevron(p, r.right_center() - vec2(9.0, 1.0), 3.0, alpha(color, 0.9));
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
        ui.set_min_width(width);
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

fn chevron(p: &egui::Painter, c: Pos2, w: f32, color: Color32) {
    let pts = vec![c + vec2(-w, -w * 0.43), c + vec2(0.0, w * 0.57), c + vec2(w, -w * 0.43)];
    p.add(Shape::line(pts, stroke(1.4, color)));
}

/// A small text button.
fn button(ui: &mut Ui, text: &str, w: f32, lit: bool, color: Color32, hint: &str) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(w, KEY_H), Sense::click());
    let p = ui.painter();
    p.rect_filled(r, cr(3), if lit { alpha(color, 0.18) } else { SCREEN });
    let edge = if lit || resp.hovered() { alpha(color, 0.7) } else { SCREEN_EDGE };
    p.rect_stroke(r, cr(3), stroke(1.0, edge), egui::StrokeKind::Inside);
    caps_spaced(p, r.center(), text, if lit { color } else { TEXT_DIM }, Align2::CENTER_CENTER, 0.6);
    resp.on_hover_text(hint)
}

/// A dark screen with a faint edge; what's drawn on it glows.
fn screen(p: &egui::Painter, r: Rect) {
    p.rect_filled(r, cr(4), SCREEN);
    p.rect_stroke(r, cr(4), stroke(1.0, SCREEN_EDGE), egui::StrokeKind::Inside);
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

// ───────────────────────────────────────────────────────────── header

fn header_bar(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let p = ui.painter();
    p.rect_filled(r, cr(5), PANEL);
    p.rect_stroke(r, cr(5), stroke(1.0, PANEL_EDGE), egui::StrokeKind::Inside);
    // The nameplate: the emblem lights the bar around it, and its glow runs
    // out along the bar like the wings' trails.
    let c = r.center();
    let glow = [(0.0, alpha(GREEN, 0.16)), (0.5, alpha(CYAN, 0.05)), (1.0, Color32::TRANSPARENT)];
    theme::radial_circle(p, c, 190.0, c, &glow);
    for side in [-1.0f32, 1.0] {
        let n = 24;
        for k in 0..n {
            let (t0, t1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
            let x = |t: f32| c.x + side * (60.0 + t * 230.0);
            let y = |t: f32| c.y + 14.0 - 10.0 * (t * PI).sin() * (1.0 - t);
            let a = (1.0 - t0).powi(2);
            p.line_segment([pos2(x(t0), y(t0)), pos2(x(t1), y(t1))], stroke(1.2, alpha(CYAN, 0.5 * a)));
        }
    }
    let (tex, [w, h]) = theme::chrysalis_emblem(ui.ctx());
    // A badge: a little taller than the bar, so it breaks out of it.
    let eh = r.height() + 18.0;
    let ew = eh * w as f32 / h as f32;
    let img = Rect::from_center_size(c, vec2(ew, eh));
    p.image(tex, img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    ui.interact(img, Id::new("chrys-emblem"), Sense::hover()).on_hover_text("Chrysalis · wavetable synthesizer");

    // Preset on the left, the voice on the right.
    let left = Rect::from_min_max(r.left_top() + vec2(PAD + 4.0, 0.0), pos2(c.x - 170.0, r.bottom()));
    caps(ui.painter(), left.left_top() + vec2(2.0, 16.0), "preset", TEXT_FAINT, Align2::LEFT_CENTER);
    let mut lu = row_ui(ui, Rect::from_min_max(left.left_top() + vec2(0.0, 28.0), left.max), "chrys-preset");
    preset_row(&mut lu, ed, left.width());
    let right = Rect::from_min_max(pos2(c.x + 160.0, r.top() + 4.0), r.right_bottom() - vec2(PAD, 0.0));
    let mut ru = row_ui(ui, right, "chrys-voice");
    voice(&mut ru, ed);
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

/// `‹ name ›`: click the name for the list, scroll over it to step.
fn preset_row(ui: &mut Ui, ed: &mut Ed, width: f32) {
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
    let mut pick = None;
    let h = 28.0;
    let arrow = |ui: &mut Ui, left: bool| {
        let (r, resp) = ui.allocate_exact_size(vec2(h, h), Sense::click());
        let p = ui.painter();
        p.rect_filled(r, cr(3), SCREEN);
        let edge = if resp.hovered() { alpha(GREEN, 0.6) } else { SCREEN_EDGE };
        p.rect_stroke(r, cr(3), stroke(1.0, edge), egui::StrokeKind::Inside);
        let s = if left { -1.0 } else { 1.0 };
        let c = r.center();
        let pts = vec![c + vec2(-2.5 * s, -5.0), c + vec2(2.5 * s, 0.0), c + vec2(-2.5 * s, 5.0)];
        p.add(Shape::line(pts, stroke(1.6, GREEN)));
        resp.on_hover_text(if left { "Previous preset" } else { "Next preset" }).clicked()
    };
    if arrow(ui, true) {
        pick = Some((at + n - 1) % n);
    }
    let (r, resp) = ui.allocate_exact_size(vec2(width - 2.0 * h - 8.0, h), Sense::click());
    let p = ui.painter();
    p.rect_filled(r, cr(3), SCREEN);
    p.rect_stroke(r, cr(3), stroke(1.0, SCREEN_EDGE), egui::StrokeKind::Inside);
    let t = p.text(r.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(14.0), TEXT);
    if edited {
        p.text(t.right_center() + vec2(8.0, 1.0), Align2::LEFT_CENTER, "edited", FontId::proportional(10.5), TEXT_DIM);
    }
    chevron(p, r.right_center() - vec2(11.0, 1.0), 3.5, GREEN);
    if let Some(step) = wheel_step(ui, r) {
        pick = Some((at as i32 + step).rem_euclid(n as i32) as usize);
    }
    let popup = ui.make_persistent_id(("chrys-preset-list", ed.ch));
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup));
    }
    let resp = resp.on_hover_text("Preset: scroll to flip through, click for the list");
    egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(r.width());
        for (k, (name, _)) in PRESETS.iter().enumerate() {
            if ui.selectable_label(current == Some(k), *name).clicked() {
                pick = Some(k);
            }
        }
    });
    if arrow(ui, false) {
        pick = Some((at + 1) % n);
    }
    if let Some(k) = pick {
        ed.h.checkpoint(ed.p);
        ed.p.channels[ed.ch].params = preset_values(k);
        ui.ctx().data_mut(|d| d.insert_temp(last_id, k));
    }
}

fn voice(ui: &mut Ui, ed: &mut Ed) {
    let small = vec2(54.0, 66.0);
    knob_sized(ui, ed, p::VOLUME, "Volume", GREEN, true, 30.0, small);
    knob_sized(ui, ed, p::AMP_VEL, "Velocity", GREEN, true, 30.0, small);
    knob_sized(ui, ed, p::GLIDE, "Glide", GREEN, true, 30.0, small);
    ui.add_space(6.0);
    let mut set = None;
    ui.vertical(|ui| {
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.y = 6.0;
        let hint = "Mono: one note at a time, with legato and glide";
        if let Some(k) = seg(ui, &["poly", "mono"], ed.choice(p::MODE), GREEN, 54.0, hint) {
            set = Some((p::MODE, k));
        }
        ui.add_enabled_ui(ed.v(p::GLIDE) > 0.0, |ui| {
            let hint = "Glide only between overlapping notes (legato), or on every note";
            if let Some(k) = seg(ui, &["legato", "always"], ed.choice(p::GLIDE_MODE), GREEN, 54.0, hint) {
                set = Some((p::GLIDE_MODE, k));
            }
        });
    });
    if let Some((id, k)) = set {
        ed.set(id, k as f32);
    }
}

// ───────────────────────────────────────────────────────────── oscillators

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

fn osc_panel(ui: &mut Ui, ed: &mut Ed, o: usize, r: Rect) {
    let color = OSC_COLORS[o];
    let inner = panel(ui, r, if o == 0 { "osc a" } else { "osc b" }, color);
    let at = |k: usize| p::osc(o, k);
    let on = ed.on(at(p::OSC_ON));
    let is_import = ed.choice(at(p::TABLE)) == IMPORTED;
    // Controls on the title line, right of the name.
    let bar = Rect::from_min_size(pos2(r.left() + 76.0, r.top() + PAD - 5.0), vec2(r.width() - 76.0 - PAD, KEY_H));
    let mut b = row_ui(ui, bar, ("chrys-osc-bar", o));
    power(&mut b, ed, at(p::OSC_ON), color, "Oscillator on / off");
    well(&mut b, ed, at(p::TABLE), 112.0, color, "Wavetable: scroll to flip through");
    let has = ed.p.channels[ed.ch].sample.is_some();
    let hint = "Import a WAV as a wavetable (cut into frames of 2048 samples). Both oscillators can play it.";
    if button(&mut b, "load", 46.0, is_import && has, color, hint).clicked() {
        if let Some(path) = rfd::FileDialog::new().add_filter("Audio", &["wav", "flac"]).pick_file() {
            ed.h.checkpoint(ed.p);
            ed.p.channels[ed.ch].sample = Some(crate::samples::name_for(&path));
            ed.put(at(p::TABLE), IMPORTED as f32);
        }
    }
    if is_import {
        let name = ed.p.channels[ed.ch].sample.as_deref().map_or("no WAV loaded".into(), crate::samples::display_name);
        b.label(egui::RichText::new(name).size(10.5).color(TEXT_DIM));
    }
    b.with_layout(Layout::right_to_left(Align::Min), |ui| {
        well(ui, ed, at(p::WARP_MODE), 100.0, color, "Warp: how the phase is bent before the table is read");
        let (lr, _) = ui.allocate_exact_size(vec2(40.0, KEY_H), Sense::hover());
        caps(ui.painter(), lr.right_center() - vec2(2.0, 0.0), "warp", TEXT_FAINT, Align2::RIGHT_CENTER);
    });

    let scope_r = Rect::from_min_size(inner.min, vec2(inner.width(), 150.0));
    let mut su = ui.new_child(UiBuilder::new().id_salt(("chrys-scope", o)).max_rect(scope_r));
    su.add_enabled_ui(on, |ui| table_view(ui, ed, o, scope_r, color));
    let knobs_r = Rect::from_min_max(pos2(inner.left() + 4.0, scope_r.bottom() + 8.0), inner.max);
    let mut k = ui.new_child(
        UiBuilder::new().id_salt(("chrys-osc-knobs", o)).max_rect(knobs_r).layout(Layout::top_down(Align::Min)),
    );
    k.spacing_mut().item_spacing = vec2(0.0, 2.0);
    let warps = ed.choice(at(p::WARP_MODE)) != 0;
    let voices = ed.choice(at(p::UNISON)) > 0;
    k.add_enabled_ui(on, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            knob(ui, ed, at(p::POS), "Position", color, true);
            knob(ui, ed, at(p::WARP), "Warp", color, warps);
            knob(ui, ed, at(p::OCTAVE), "Octave", color, true);
            knob(ui, ed, at(p::SEMI), "Semi", color, true);
            knob(ui, ed, at(p::FINE), "Fine", color, true);
            knob(ui, ed, at(p::LEVEL), "Level", color, true);
            knob(ui, ed, at(p::PAN), "Pan", color, true);
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            knob(ui, ed, at(p::UNISON), "Unison", color, true);
            knob(ui, ed, at(p::DETUNE), "Detune", color, voices);
            knob(ui, ed, at(p::BLEND), "Blend", color, voices);
            knob(ui, ed, at(p::SPREAD), "Spread", color, voices);
            knob(ui, ed, at(p::PHASE), "Phase", color, true);
            knob(ui, ed, at(p::RAND), "Random", color, true);
        });
    });
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
    let n_pts = 120;
    let curve = |fpos: f32, d: f32, warp_mode: u8, w: f32| -> Vec<Pos2> {
        let (x0, y) = place(d);
        (0..=n_pts)
            .map(|k| {
                let ph = k as f32 / n_pts as f32;
                let read = lyra_chrysalis::warp_preview(warp_mode, w, ph.min(0.999_99));
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
    let fpos = pos * (frames - 1) as f32;
    let d = if frames > 1 { pos } else { 0.0 };
    let mode = ed.choice(p::osc(o, p::WARP_MODE)) as u8;
    let w = ed.v(p::osc(o, p::WARP));
    let pts = curve(fpos, d, if mode == warp::FM { 0 } else { mode }, w);
    let (_, base) = place(d);
    let ink = if enabled { color } else { TEXT_FAINT };
    glow_fill(&painter, &pts, base, ink);
    glow_line(&painter, pts, ink, 1.8);
    let caption = if frames > 1 { format!("{:.0} / {frames}", fpos.round() + 1.0) } else { "1 frame".to_string() };
    caps(&painter, rect.right_bottom() - vec2(10.0, 9.0), &caption, TEXT_DIM, Align2::RIGHT_CENTER);
}

// ───────────────────────────────────────────────────────────── sub, noise, drive

fn sub_panel(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let inner = panel(ui, r, "sub · noise", GREEN);
    let mut u =
        ui.new_child(UiBuilder::new().id_salt("chrys-sub").max_rect(inner).layout(Layout::top_down(Align::Min)));
    u.spacing_mut().item_spacing = vec2(4.0, 4.0);
    let mut set = None;
    u.horizontal(|ui| {
        power(ui, ed, p::SUB_ON, GREEN, "Sub oscillator on / off");
        if let Some(k) = seg(ui, &["sin", "sqr"], ed.choice(p::SUB_SHAPE), GREEN, 40.0, "Sub shape") {
            set = Some((p::SUB_SHAPE, k));
        }
        if let Some(k) = seg(ui, &["−1", "−2"], ed.choice(p::SUB_OCT), GREEN, 34.0, "Octaves below the note") {
            set = Some((p::SUB_OCT, k));
        }
    });
    let (sub, noise) = (ed.on(p::SUB_ON), ed.on(p::NOISE_ON));
    u.horizontal(|ui| {
        knob(ui, ed, p::SUB_LEVEL, "Sub", GREEN, sub);
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                power(ui, ed, p::NOISE_ON, GREEN, "Noise on / off (a little different on each side)");
                let (lr, _) = ui.allocate_exact_size(vec2(50.0, KEY_H), Sense::hover());
                caps(ui.painter(), lr.left_center(), "noise", TEXT_DIM, Align2::LEFT_CENTER);
            });
        });
    });
    u.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        knob(ui, ed, p::NOISE_COLOR, "Colour", GREEN, noise);
        knob(ui, ed, p::NOISE_LEVEL, "Noise", GREEN, noise);
    });
    if let Some((id, k)) = set {
        ed.set(id, k as f32);
    }
}

fn drive_panel(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let inner = panel(ui, r, "drive", LIME);
    let mut u =
        ui.new_child(UiBuilder::new().id_salt("chrys-drive").max_rect(inner).layout(Layout::top_down(Align::Min)));
    u.spacing_mut().item_spacing = vec2(4.0, 6.0);
    let mode = ed.choice(p::DIST_MODE);
    let labels = ["off", "tanh", "hard", "fold", "crush"];
    if let Some(k) = seg(&mut u, &labels, mode, LIME, inner.width() / 5.0, "Drive, per voice, before the filters") {
        ed.set(p::DIST_MODE, k as f32);
    }
    let (sr, resp) = u.allocate_exact_size(vec2(inner.width(), 62.0), Sense::click_and_drag());
    if mode != 0 {
        drag_param(&u, ed, &resp, p::DIST_DRIVE, 200.0);
        resp.on_hover_text("Drag up/down or scroll: drive");
    }
    drive_view(&u, ed, sr);
    let on = mode != 0;
    u.horizontal(|ui| {
        ui.add_space((inner.width() - 2.0 * CELL.x - 12.0) / 2.0);
        ui.spacing_mut().item_spacing.x = 12.0;
        knob(ui, ed, p::DIST_DRIVE, "Drive", LIME, on);
        knob(ui, ed, p::DIST_MIX, "Mix", LIME, on);
    });
}

/// What the drive does to a sine, as the voices shape it.
fn drive_view(ui: &Ui, ed: &Ed, r: Rect) {
    let painter = ui.painter_at(r);
    screen(&painter, r);
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

// ───────────────────────────────────────────────────────────── filters

fn filter_panel(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let inner = panel(ui, r, "filters", AQUA);
    let bar = Rect::from_min_size(pos2(r.right() - PAD - 152.0, r.top() + PAD - 5.0), vec2(152.0, KEY_H));
    let mut b = row_ui(ui, bar, "chrys-route");
    let hint = "Serial: filter 1 into filter 2. Parallel: both hear the voice, side by side.";
    if let Some(k) = seg(&mut b, &["serial", "parallel"], ed.choice(p::ROUTING), AQUA, 76.0, hint) {
        ed.set(p::ROUTING, k as f32);
    }
    let cell = vec2(54.0, 60.0);
    let knobs_w = 6.0 * cell.x + 5.0 * 2.0;
    let graph_r = Rect::from_min_max(inner.min, pos2(inner.right() - knobs_w - 12.0, inner.bottom()));
    let mut g = ui.new_child(UiBuilder::new().id_salt("chrys-fgraph").max_rect(graph_r));
    filter_graph(&mut g, ed, graph_r);
    let col = Rect::from_min_max(pos2(graph_r.right() + 12.0, inner.top() - 4.0), inner.max);
    let mut u =
        ui.new_child(UiBuilder::new().id_salt("chrys-filters").max_rect(col).layout(Layout::top_down(Align::Min)));
    u.spacing_mut().item_spacing = vec2(2.0, 0.0);
    for f in 0..2 {
        let at = |k: usize| p::filter(f, k);
        let on = ed.on(at(p::F_ON));
        let kind = ed.choice(at(p::F_TYPE)) as u8;
        u.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            power(ui, ed, at(p::F_ON), AQUA, "Filter on / off");
            let (lr, _) = ui.allocate_exact_size(vec2(12.0, KEY_H), Sense::hover());
            caps(ui.painter(), lr.left_center(), if f == 0 { "1" } else { "2" }, TEXT_DIM, Align2::LEFT_CENTER);
            well(ui, ed, at(p::F_TYPE), 96.0, AQUA, "Filter type");
        });
        let comb = kind >= ft::COMB_PLUS;
        let label = match kind {
            ft::FORMANT => "Shift",
            _ if comb => "Pitch",
            _ => "Cutoff",
        };
        u.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            knob_sized(ui, ed, at(p::F_CUTOFF), label, AQUA, on, 30.0, cell);
            knob_sized(ui, ed, at(p::F_RESO), if comb { "Feedback" } else { "Reso" }, AQUA, on, 30.0, cell);
            knob_sized(ui, ed, at(p::F_DRIVE), "Drive", AQUA, on, 30.0, cell);
            knob_sized(ui, ed, at(p::F_KEY), "Key", AQUA, on, 30.0, cell);
            knob_sized(ui, ed, at(p::F_VOWEL), "Vowel", AQUA, on && kind == ft::FORMANT, 30.0, cell);
            knob_sized(ui, ed, at(p::F_MIX), "Mix", AQUA, on, 30.0, cell);
        });
    }
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
    let inner = rect.shrink2(vec2(12.0, 10.0));
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

// ───────────────────────────────────────────────────────────── modulation

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

fn mod_panel(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let pnt = ui.painter();
    pnt.rect_filled(r, cr(5), PANEL);
    pnt.rect_stroke(r, cr(5), stroke(1.0, PANEL_EDGE), egui::StrokeKind::Inside);
    // Tabs along the top: text, the open one underlined in violet; a little
    // gap between envelopes, LFOs and the matrix. Unused sources are fainter.
    let view_id = Id::new(("chrys-mod-view", ed.ch));
    let mut view = ModView::from_index(ui.ctx().data(|d| d.get_temp::<usize>(view_id)).unwrap_or(4));
    let mut x = r.left() + PAD;
    let y = r.top() + PAD - 2.0;
    for (i, name) in MOD_TABS.iter().enumerate() {
        if i == 4 || i == 8 {
            x += 14.0;
        }
        let w = if i == 8 { 70.0 } else { 56.0 };
        let t = Rect::from_min_size(pos2(x, y), vec2(w, 24.0));
        let resp = ui.interact(t, Id::new(("chrys-mod-tab", i)), Sense::click());
        let open = view.index() == i;
        let p = ui.painter();
        let ink = if open {
            VIOLET
        } else if resp.hovered() {
            TEXT
        } else if mod_used(ed, i) {
            TEXT_DIM
        } else {
            TEXT_FAINT
        };
        caps(p, t.center() - vec2(0.0, 2.0), name, ink, Align2::CENTER_CENTER);
        if open {
            let line = [t.left_bottom() + vec2(10.0, -2.0), t.right_bottom() + vec2(-10.0, -2.0)];
            p.line_segment(line, stroke(6.0, alpha(VIOLET, 0.15)));
            p.line_segment(line, stroke(2.0, VIOLET));
        }
        if resp.clicked() {
            view = ModView::from_index(i);
        }
        x += w;
    }
    ui.ctx().data_mut(|d| d.insert_temp(view_id, view.index()));
    let inner = Rect::from_min_max(r.left_top() + vec2(PAD, PAD + 30.0), r.right_bottom() - vec2(PAD, PAD));
    match view {
        ModView::Env(e) => env_view(ui, ed, e, inner),
        ModView::Lfo(l) => lfo_view(ui, ed, l, inner),
        ModView::Matrix => matrix_view(ui, ed, inner),
    }
}

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

fn env_view(ui: &mut Ui, ed: &mut Ed, e: usize, inner: Rect) {
    let knobs_w = 4.0 * CELL.x + 3.0 * 8.0;
    let graph = Rect::from_min_max(inner.min, pos2(inner.right() - knobs_w - 14.0, inner.bottom()));
    let mut g = ui.new_child(UiBuilder::new().id_salt(("chrys-env-g", e)).max_rect(graph));
    envelope(&mut g, ed, e, graph);
    let col = Rect::from_min_max(pos2(graph.right() + 14.0, inner.top()), inner.max);
    let mut u = ui
        .new_child(UiBuilder::new().id_salt(("chrys-env-knobs", e)).max_rect(col).layout(Layout::top_down(Align::Min)));
    u.spacing_mut().item_spacing = vec2(8.0, 8.0);
    let at = |k: usize| p::env(e, k);
    u.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        knob(ui, ed, at(p::E_ATTACK), "Attack", VIOLET, true);
        knob(ui, ed, at(p::E_DECAY), "Decay", VIOLET, true);
        knob(ui, ed, at(p::E_SUSTAIN), "Sustain", VIOLET, true);
        knob(ui, ed, at(p::E_RELEASE), "Release", VIOLET, true);
    });
    u.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        knob(ui, ed, at(p::E_A_CURVE), "A curve", VIOLET, true);
        knob(ui, ed, at(p::E_D_CURVE), "D curve", VIOLET, true);
        knob(ui, ed, at(p::E_R_CURVE), "R curve", VIOLET, true);
    });
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

fn lfo_view(ui: &mut Ui, ed: &mut Ed, l: usize, inner: Rect) {
    let side_w = 4.0 * CELL.x + 3.0 * 8.0;
    let graph = Rect::from_min_max(inner.min, pos2(inner.right() - side_w - 14.0, inner.bottom()));
    let mut g = ui.new_child(UiBuilder::new().id_salt(("chrys-lfo-g", l)).max_rect(graph));
    lfo_editor(&mut g, ed, l, graph);
    let col = Rect::from_min_max(pos2(graph.right() + 14.0, inner.top()), inner.max);
    let mut u = ui
        .new_child(UiBuilder::new().id_salt(("chrys-lfo-side", l)).max_rect(col).layout(Layout::top_down(Align::Min)));
    u.spacing_mut().item_spacing = vec2(4.0, 6.0);
    let hint = "Free: one shared LFO (locked to the song when synced). Retrig: restarts on every note. \
                One-shot: runs once per note and holds its end, like an envelope.";
    let mode = ed.choice(p::lfo(l, p::L_MODE));
    if let Some(k) = seg(&mut u, &["free", "retrig", "1-shot"], mode, VIOLET, col.width() / 3.0, hint) {
        ed.set(p::lfo(l, p::L_MODE), k as f32);
    }
    // Shape presets: a little picture each, four to a row.
    let now = read_points(ed, l);
    for row in SHAPES.chunks(4) {
        u.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (name, pts) in row {
                let shape: Vec<Point> = pts.iter().map(|&(x, y, bend)| Point { x, y, bend }).collect();
                let w = (col.width() - 12.0) / 4.0;
                if shape_key(ui, vec2(w, 26.0), &shape, now == shape).on_hover_text(*name).clicked() {
                    ed.h.checkpoint(ed.p);
                    write_points(ed, l, &shape);
                }
            }
        });
    }
    let synced = ed.choice(p::lfo(l, p::L_SYNC)) != 0;
    u.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        knob(ui, ed, p::lfo(l, p::L_SYNC), "Sync", VIOLET, true);
        knob(ui, ed, p::lfo(l, p::L_RATE), "Rate", VIOLET, !synced);
    });
}

/// A small key with an LFO shape drawn on it.
fn shape_key(ui: &mut Ui, size: Vec2, shape: &[Point], lit: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, cr(3), if lit { alpha(VIOLET, 0.16) } else { SCREEN });
    let edge = if lit {
        alpha(VIOLET, 0.8)
    } else if resp.hovered() {
        alpha(VIOLET, 0.5)
    } else {
        SCREEN_EDGE
    };
    p.rect_stroke(rect, cr(3), stroke(1.0, edge), egui::StrokeKind::Inside);
    let face = rect.shrink2(vec2(7.0, 6.0));
    let s = curve::Shape::new(shape);
    let pts: Vec<Pos2> = (0..=40)
        .map(|k| {
            let t = k as f32 / 40.0;
            pos2(face.left() + t * face.width(), face.bottom() - s.eval(t.min(0.9999)) * face.height())
        })
        .collect();
    p.add(Shape::line(pts, stroke(1.4, if lit { VIOLET } else { TEXT_DIM })));
    resp
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
}

/// All 16 slots, two columns of 8: # · source → destination · amount · via · Bi.
fn matrix_view(ui: &mut Ui, ed: &mut Ed, inner: Rect) {
    let col_w = (inner.width() - 16.0) / 2.0;
    let row_h = (inner.height() / 8.0).floor();
    for k in 0..p::SLOT_COUNT {
        let (c, rr) = (k / 8, k % 8);
        let at = pos2(inner.left() + c as f32 * (col_w + 16.0), inner.top() + rr as f32 * row_h);
        let mut u = row_ui(ui, Rect::from_min_size(at, vec2(col_w, row_h)), ("chrys-slot", k));
        u.spacing_mut().item_spacing.x = 4.0;
        slot_row(&mut u, ed, k);
    }
}

fn slot_row(ui: &mut Ui, ed: &mut Ed, k: usize) {
    let at = |q: usize| p::slot(k, q);
    let live = ed.choice(at(p::S_SRC)) != 0 && ed.choice(at(p::S_DEST)) != 0;
    let (nr, _) = ui.allocate_exact_size(vec2(16.0, KEY_H), Sense::hover());
    let num_ink = if live { VIOLET } else { TEXT_FAINT };
    ui.painter().text(nr.center(), Align2::CENTER_CENTER, format!("{}", k + 1), FontId::proportional(10.5), num_ink);
    well(ui, ed, at(p::S_SRC), 86.0, VIOLET, "Source: what moves this slot");
    let (ar, _) = ui.allocate_exact_size(vec2(10.0, KEY_H), Sense::hover());
    let (y, x0, x1) = (ar.center().y, ar.left(), ar.right());
    let st = stroke(1.3, if live { VIOLET } else { TEXT_FAINT });
    ui.painter().line_segment([pos2(x0, y), pos2(x1, y)], st);
    ui.painter().add(Shape::line(vec![pos2(x1 - 3.5, y - 3.0), pos2(x1, y), pos2(x1 - 3.5, y + 3.0)], st));
    well(ui, ed, at(p::S_DEST), 112.0, VIOLET, "Destination: what it moves");
    let info = &PARAMS[at(p::S_AMT)];
    let mut v = ed.v(at(p::S_AMT));
    let color = if live { VIOLET } else { TEXT_FAINT };
    let r = crate::widgets::knob_painted(ui, &mut v, info, KEY_H, |p, rect, v, hot| {
        paint_knob(p, rect, info, v, color, hot)
    });
    ed.track(&r, at(p::S_AMT), v);
    r.on_hover_text("Amount: how far, and which way (double-click: 0)");
    let (vr, _) = ui.allocate_exact_size(vec2(34.0, KEY_H), Sense::hover());
    let ink = if live { TEXT } else { TEXT_FAINT };
    ui.painter().text(
        vr.left_center(),
        Align2::LEFT_CENTER,
        info.format(ed.v(at(p::S_AMT))),
        FontId::proportional(10.5),
        ink,
    );
    well(ui, ed, at(p::S_VIA), 66.0, VIOLET, "Via: a second source that scales the amount (e.g. velocity)");
    let bi = ed.on(at(p::S_BIPOLAR));
    let hint = "Bi: centre the source (−1..1), so it swings both ways around the knob";
    if button(ui, "bi", 26.0, bi, VIOLET, hint).clicked() {
        ed.toggle(at(p::S_BIPOLAR));
    }
}

// ───────────────────────────────────────────────────────────── macros

/// Four big knobs; a macro is dim until a slot listens to it.
fn macros_panel(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let inner = panel(ui, r, "macros", VIOLET);
    let cell = vec2(inner.width() / 2.0, inner.height() / 2.0);
    for k in 0..4 {
        let src = (lyra_chrysalis::params::src::MACRO + k) as f32;
        let routes = (0..p::SLOT_COUNT)
            .filter(|&s| {
                ed.v(p::slot(s, p::S_DEST)) >= 0.5
                    && (ed.v(p::slot(s, p::S_SRC)) == src || ed.v(p::slot(s, p::S_VIA)) == src)
            })
            .count();
        let c = Rect::from_min_size(inner.min + vec2((k % 2) as f32 * cell.x, (k / 2) as f32 * cell.y), cell);
        let mut u = ui.new_child(
            UiBuilder::new().id_salt(("chrys-macro", k)).max_rect(c).layout(Layout::top_down(Align::Center)),
        );
        u.spacing_mut().item_spacing.y = 1.0;
        let id = p::mac(k);
        let info = &PARAMS[id];
        let color = if routes > 0 { VIOLET } else { alpha(VIOLET, 0.45) };
        let mut v = ed.v(id);
        let resp = crate::widgets::knob_painted(&mut u, &mut v, info, 50.0, |p, rect, v, hot| {
            paint_knob(p, rect, info, v, color, hot)
        });
        ed.track(&resp, id, v);
        let tip = if routes == 0 {
            format!("Macro {}: send it somewhere in the MATRIX (source \"Macro {}\")", k + 1, k + 1)
        } else {
            format!("Macro {}: drives {routes} slot{}", k + 1, if routes == 1 { "" } else { "s" })
        };
        resp.on_hover_text(tip);
        let (lr, _) = u.allocate_exact_size(vec2(cell.x, 26.0), Sense::hover());
        let painter = u.painter();
        let ink = if routes > 0 { TEXT_DIM } else { TEXT_FAINT };
        caps(painter, lr.center_top() + vec2(0.0, 6.0), &format!("macro {}", k + 1), ink, Align2::CENTER_CENTER);
        let value = info.format(ed.v(id));
        painter.text(lr.center_top() + vec2(0.0, 13.0), Align2::CENTER_TOP, value, FontId::proportional(10.5), TEXT);
    }
}
