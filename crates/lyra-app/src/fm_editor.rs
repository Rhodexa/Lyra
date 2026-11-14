//! The hand-made editor for Lyra FM, a.k.a. **Fluttersine** (Sytrus-style).
//!
//! ```text
//!  ◀ [ preset ▾ ] ▶                                        Fluttersine
//!  [OP1][OP2][OP3][OP4][OP5][OP6][FILTER][LFO]       FM MATRIX
//!  ┌ one page at a time ──────────────────────┐   ┌ 6×6 + OUT ──────┐
//!  │ wave tiles                                │   │ row → column     │
//!  │ wave preview        │ envelope (drag me)  │   └──────────────────┘
//!  │ knobs                                     │   ┌ voice ──────────┐
//!  └───────────────────────────────────────────┘   └──────────────────┘
//! ```
//!
//! * Every operator has its own pastel (rose → lavender, a rainbow) for its tab, its
//!   matrix row, its knobs and its curves, so one operator can be followed across the
//!   whole window. The matrix diagonal (feedback) is gold, as in Fluttersine.
//! * The matrix is always visible; clicking a row's name opens that operator.
//! * Envelopes and the filter curve have handles you can drag; the knobs below them
//!   are the same parameters.
//! * Wheel over the tabs, the wave tiles or the preset name steps through them.
//!
//! Like every view, this only edits `channel.params`; the bridge does the rest.
//! The waveform pictures are display-only copies of `lyra_dsp::fm`'s shapes.

use crate::rack::tracked;
use crate::theme::{self, cr, stroke};
use crate::widgets::{self, Icon};
use egui::{pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Rect, Response, RichText, Sense};
use egui::{Shape, StrokeKind, Ui, Vec2};
use lyra_dsp::fm::{p, FM_PARAMS, FM_PRESETS};
use lyra_model::{History, Project};
use p::{op, OPS};
use std::f32::consts::TAU;

const OP_COLORS: [Color32; OPS] = [theme::ROSE, theme::PEACH, theme::BUTTER, theme::MINT, theme::SKY, theme::LAVENDER];
/// Waveform indices that behave differently (see `lyra_dsp::fm`).
const W_PULSE: usize = 8;
const W_SSAW: usize = 9;
/// Supersaw side-voice detunes, for the picture only.
const SS_DETUNE: [f32; 6] = [-0.110, -0.063, -0.020, 0.020, 0.062, 0.107];

fn filter_color() -> Color32 {
    theme::mix(theme::MINT, theme::SKY, 0.5)
}
fn lfo_color() -> Color32 {
    theme::mix(theme::ROSE, theme::LAVENDER, 0.55)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Op(usize),
    Filter,
    Lfo,
}

impl Page {
    const COUNT: usize = OPS + 2;
    fn index(self) -> usize {
        match self {
            Page::Op(i) => i,
            Page::Filter => OPS,
            Page::Lfo => OPS + 1,
        }
    }
    fn from_index(i: usize) -> Page {
        match i {
            i if i < OPS => Page::Op(i),
            i if i == OPS => Page::Filter,
            _ => Page::Lfo,
        }
    }
    fn color(self) -> Color32 {
        match self {
            Page::Op(i) => OP_COLORS[i],
            Page::Filter => filter_color(),
            Page::Lfo => lfo_color(),
        }
    }
}

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
        let info = &FM_PARAMS[id];
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
}

/// Sizes, derived from the theme's metrics so F12 still drives everything.
struct Sizes {
    cell_w: f32,
    cell_h: f32,
    /// Inner width of the page (everything left of the matrix).
    page_w: f32,
    page_h: f32,
    tab_h: f32,
    mcell: Vec2,
    mlabel_w: f32,
}

fn sizes() -> Sizes {
    let m = theme::m();
    let cell_w = m.knob_large + 22.0;
    let mcell = vec2((m.control_h * 1.45).round(), m.control_h + 2.0);
    Sizes {
        cell_w,
        cell_h: m.knob_large + 2.0 * m.text_small + 12.0,
        page_w: 7.0 * cell_w + 6.0 * 5.0,
        page_h: 2.0 * m.control_h + 116.0 + 2.0 * (m.knob_large + 2.0 * m.text_small + 12.0) + 34.0,
        tab_h: m.control_h + 6.0,
        mcell,
        mlabel_w: 42.0,
    }
}

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, ch: usize) {
    let mut ed = Ed { p, h, ch };
    let s = sizes();
    let page_id = Id::new(("fm-page", ch));
    let stored = ui.ctx().data(|d| d.get_temp::<usize>(page_id)).unwrap_or(0);
    let mut page = Page::from_index(stored);
    let used = used_ops(&ed);

    preset_strip(ui, &mut ed);
    ui.add_space(6.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            tabs(ui, &ed, &mut page, &used);
            let c = page.color();
            egui::Frame::default()
                .fill(theme::mix(theme::PANEL_HI, c, 0.035))
                .stroke(stroke(1.5, c.gamma_multiply(0.55)))
                .corner_radius(CornerRadius { nw: 0, ne: 8, sw: 8, se: 8 })
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.set_width(s.page_w);
                    ui.set_min_height(s.page_h);
                    match page {
                        Page::Op(i) => op_page(ui, &mut ed, i, &used),
                        Page::Filter => filter_page(ui, &mut ed),
                        Page::Lfo => lfo_page(ui, &mut ed),
                    }
                });
        });
        ui.add_space(6.0);
        ui.vertical(|ui| {
            matrix(ui, &mut ed, &mut page, &used);
            ui.add_space(8.0);
            voice(ui, &mut ed);
        });
    });
    ui.ctx().data_mut(|d| d.insert_temp(page_id, page.index()));
}

// ───────────────────────────────────────────────────────────── presets

/// A preset's full parameter list (defaults + its changes), without allocating.
fn preset_values(k: usize) -> [f32; p::COUNT] {
    let mut v = [0.0; p::COUNT];
    for (slot, info) in v.iter_mut().zip(FM_PARAMS.iter()) {
        *slot = info.default;
    }
    for &(id, x) in FM_PRESETS[k].1 {
        v[id] = x;
    }
    v
}

/// The preset the current sound is exactly equal to, if any.
fn current_preset(params: &[f32]) -> Option<usize> {
    (0..FM_PRESETS.len())
        .find(|&k| preset_values(k).iter().zip(params).all(|(a, b)| (a - b).abs() <= 1e-5 * a.abs().max(1.0)))
}

fn preset_strip(ui: &mut Ui, ed: &mut Ed) {
    let n = FM_PRESETS.len();
    let current = current_preset(&ed.p.channels[ed.ch].params);
    // The last preset loaded here, so ◀ ▶ still make sense after you tweak it.
    let last_id = Id::new(("fm-last-preset", ed.ch));
    let last = ui.ctx().data(|d| d.get_temp::<usize>(last_id));
    let at = current.or(last).unwrap_or(0);
    let text = match (current, last) {
        (Some(k), _) => FM_PRESETS[k].0.to_string(),
        (None, Some(k)) => format!("{} (edited)", FM_PRESETS[k].0),
        (None, None) => "Custom".to_string(),
    };
    let mut pick = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        if widgets::icon_button(ui, Icon::Prev, false, "Previous preset").clicked() {
            pick = Some((at + n - 1) % n);
        }
        let combo =
            egui::ComboBox::from_id_salt(("fm-preset", ed.ch)).width(240.0).selected_text(text).show_ui(ui, |ui| {
                for (k, (name, _)) in FM_PRESETS.iter().enumerate() {
                    if ui.selectable_label(current == Some(k), *name).clicked() {
                        pick = Some(k);
                    }
                }
            });
        if let Some(step) = wheel_step(ui, combo.response.rect) {
            pick = Some((at as i32 + step).rem_euclid(n as i32) as usize);
        }
        if widgets::icon_button(ui, Icon::Next, false, "Next preset").clicked() {
            pick = Some((at + 1) % n);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new("FM").small().color(theme::TEXT_DIM));
            ui.label(RichText::new("Fluttersine").size(theme::m().text + 5.0).strong().color(theme::ROSE));
        });
    });
    if let Some(k) = pick {
        ed.h.checkpoint(ed.p);
        ed.p.channels[ed.ch].params = preset_values(k).to_vec();
        ui.ctx().data_mut(|d| d.insert_temp(last_id, k));
    }
}

/// -1 / +1 when the wheel turns over `rect` this frame (wheel down = next).
fn wheel_step(ui: &Ui, rect: Rect) -> Option<i32> {
    if !ui.rect_contains_pointer(rect) {
        return None;
    }
    let d = ui.input(|i| i.raw_scroll_delta);
    let dy = if d.y != 0.0 { d.y } else { d.x };
    (dy != 0.0).then_some(if dy > 0.0 { -1 } else { 1 })
}

// ───────────────────────────────────────────────────────────── routing

/// Operators that can reach the output, directly or through the matrix (as the engine decides).
fn used_ops(ed: &Ed) -> [bool; OPS] {
    let mut used: [bool; OPS] = std::array::from_fn(|i| ed.v(p::out(i)) > 0.0);
    for _ in 0..OPS {
        for i in 0..OPS {
            if !used[i] {
                used[i] = (0..OPS).any(|j| j != i && used[j] && ed.v(p::matrix(i, j)) > 0.0);
            }
        }
    }
    used
}

/// "carrier · modulates OP2" etc.
fn op_status(ed: &Ed, i: usize, used: &[bool; OPS]) -> String {
    if !used[i] {
        return "silent: raise OUT or send it into an operator".into();
    }
    let mut parts = Vec::new();
    if ed.v(p::out(i)) > 0.0 {
        parts.push("carrier".to_string());
    }
    let targets: Vec<String> =
        (0..OPS).filter(|&j| j != i && ed.v(p::matrix(i, j)) > 0.0).map(|j| format!("OP{}", j + 1)).collect();
    if !targets.is_empty() {
        parts.push(format!("modulates {}", targets.join(", ")));
    }
    let fb = ed.v(p::matrix(i, i));
    if fb > 0.0 {
        parts.push(format!("feedback {:.0}%", fb * 100.0));
    }
    parts.join(" · ")
}

// ───────────────────────────────────────────────────────────── tabs

#[derive(Clone, Copy)]
enum Glyph {
    Wave(usize),
    Filter,
    Lfo(usize),
}

fn tabs(ui: &mut Ui, ed: &Ed, page: &mut Page, used: &[bool; OPS]) {
    let s = sizes();
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for i in 0..OPS {
            let g = Glyph::Wave(ed.choice(op(i, p::OP_WAVE)));
            if tab(ui, &format!("OP{}", i + 1), g, OP_COLORS[i], *page == Page::Op(i), used[i], s.tab_h).clicked() {
                *page = Page::Op(i);
            }
        }
        if tab(
            ui,
            "FILTER",
            Glyph::Filter,
            filter_color(),
            *page == Page::Filter,
            ed.on(p::LP_ON) || ed.on(p::HP_ON),
            s.tab_h,
        )
        .clicked()
        {
            *page = Page::Filter;
        }
        let lfo_used = ed.v(p::VIBRATO) > 0.0
            || (ed.on(p::LP_ON) && ed.v(p::F_LFO) != 0.0)
            || (0..OPS).any(|i| ed.v(op(i, p::OP_PWM)) > 0.0);
        let g = Glyph::Lfo(ed.choice(p::LFO_SHAPE));
        if tab(ui, "LFO", g, lfo_color(), *page == Page::Lfo, lfo_used, s.tab_h).clicked() {
            *page = Page::Lfo;
        }
    });
    if let Some(step) = wheel_step(ui, row.response.rect) {
        *page = Page::from_index((page.index() as i32 + step).rem_euclid(Page::COUNT as i32) as usize);
    }
}

/// A rounded-top tab: a little glyph and a name. Lit = the thing is doing something.
fn tab(ui: &mut Ui, label: &str, glyph: Glyph, color: Color32, active: bool, lit: bool, h: f32) -> Response {
    let font = FontId::proportional(theme::m().text_small + 1.0);
    let text_w = ui.painter().layout_no_wrap(label.to_string(), font.clone(), theme::TEXT).size().x;
    let w = text_w + 36.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let painter = ui.painter();
    let shape = CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 };
    let fill = if active {
        color
    } else if resp.hovered() {
        theme::mix(theme::RAISED, color, 0.22)
    } else {
        theme::mix(theme::RAISED, color, 0.06)
    };
    painter.rect_filled(rect, shape, fill);
    painter.rect_stroke(rect, shape, stroke(1.0, theme::LINE), StrokeKind::Inside);
    let ink = if active {
        theme::INSET
    } else if lit {
        color
    } else {
        color.gamma_multiply(0.38)
    };
    let g = Rect::from_min_size(rect.left_top() + vec2(7.0, h * 0.5 - 6.0), vec2(16.0, 12.0));
    draw_glyph(painter, g, glyph, ink, 1.4);
    painter.text(pos2(g.right() + 5.0, rect.center().y + 1.0), Align2::LEFT_CENTER, label, font, ink);
    resp
}

fn draw_glyph(painter: &egui::Painter, rect: Rect, glyph: Glyph, color: Color32, width: f32) {
    match glyph {
        Glyph::Wave(w) => draw_curve(painter, rect, &|t| wave_at(w, t, 0.5, 0.35, 0.6), 1.0, color, width, false),
        // S&H holds one value per cycle, so its picture shows several cycles.
        Glyph::Lfo(s) => {
            let cycles = if s == 4 { 5.0 } else { 1.0 };
            draw_curve(painter, rect, &|t| lfo_at(s, t), cycles, color, width, false)
        }
        Glyph::Filter => {
            // A little resonant low-pass.
            let pts: Vec<_> = (0..=24)
                .map(|k| {
                    let x = k as f32 / 24.0;
                    let w = x * 2.2;
                    let mag = svf_lp(w, 0.7).min(2.2) / 2.2;
                    pos2(rect.left() + x * rect.width(), rect.bottom() - mag * rect.height())
                })
                .collect();
            painter.add(Shape::line(pts, stroke(width, color)));
        }
    }
}

// ───────────────────────────────────────────────────────────── operator page

fn op_page(ui: &mut Ui, ed: &mut Ed, i: usize, used: &[bool; OPS]) {
    let s = sizes();
    let c = OP_COLORS[i];
    let at = |k: usize| op(i, k);
    let wave = ed.choice(at(p::OP_WAVE));

    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("OPERATOR {}", i + 1)).color(c).strong());
        ui.label(RichText::new(op_status(ed, i, used)).small().color(theme::TEXT_DIM));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let phase = ed.choice(at(p::OP_RESET));
            let r = seg(ui, ("fm-phase", i), &["Free", "Reset"], phase, c, true);
            if let Some(k) = r.0 {
                ed.set(at(p::OP_RESET), k as f32);
            }
            r.1.on_hover_text("Free: the phase keeps running between notes.\nReset: every note starts the wave from zero (punchier, repeatable).");
            let r = seg(ui, ("fm-pitch", i), &["Ratio", "Fixed"], ed.choice(at(p::OP_FIXED)), c, true);
            if let Some(k) = r.0 {
                ed.set(at(p::OP_FIXED), k as f32);
            }
            r.1.on_hover_text("Ratio: follows the key you play (×1 = the note).\nFixed: always the same frequency, for bells, drums and metal.");
        });
    });

    // Wave tiles
    let labels = FM_PARAMS[at(p::OP_WAVE)].labels;
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let n = labels.len() as f32;
        let w = (s.page_w - 4.0 * (n - 1.0)) / n;
        for (k, name) in labels.iter().enumerate() {
            if tile(ui, vec2(w, 30.0), Glyph::Wave(k), k == wave, c).on_hover_text(*name).clicked() {
                ed.set(at(p::OP_WAVE), k as f32);
            }
        }
    });
    if let Some(step) = wheel_step(ui, row.response.rect) {
        let k = (wave as i32 + step).clamp(0, labels.len() as i32 - 1);
        ed.set(at(p::OP_WAVE), k as f32);
    }

    // Wave preview + envelope
    let warps = wave <= W_PULSE;
    let half = (s.page_w - 6.0) * 0.5;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (pw, spread, sides) = (ed.v(at(p::OP_PW)), ed.v(at(p::OP_SPREAD)), ed.v(at(p::OP_SS_MIX)));
        let (rect, _) = ui.allocate_exact_size(vec2(half, 104.0), Sense::hover());
        let painter = ui.painter_at(rect);
        well(&painter, rect);
        let inner = rect.shrink2(vec2(8.0, 12.0));
        painter.line_segment([inner.left_center(), inner.right_center()], stroke(1.0, theme::RAISED));
        draw_curve(&painter, inner, &|t| wave_at(wave, t, pw, spread, sides), 2.0, c, 2.0, true);
        painter.text(
            rect.left_bottom() + vec2(7.0, -4.0),
            Align2::LEFT_BOTTOM,
            labels[wave.min(labels.len() - 1)],
            FontId::proportional(theme::m().text_small - 1.0),
            theme::TEXT_DIM,
        );
        envelope(
            ui,
            ed,
            [at(p::OP_ATTACK), at(p::OP_DECAY), at(p::OP_SUSTAIN), at(p::OP_RELEASE)],
            c,
            vec2(half, 104.0),
            ("fm-op-env", i),
        );
    });

    // Tone knobs
    ui.horizontal(|ui| {
        if ed.on(at(p::OP_FIXED)) {
            knob_cell(ui, ed, at(p::OP_HZ), "Hz", c, true);
        } else {
            knob_cell(ui, ed, at(p::OP_RATIO), "Ratio", c, true);
        }
        knob_cell(ui, ed, at(p::OP_FINE), "Fine", c, true);
        knob_cell(ui, ed, at(p::OP_PW), if wave == W_PULSE { "Width" } else { "Warp" }, c, warps);
        knob_cell(ui, ed, at(p::OP_PWM), "PWM", c, warps);
        knob_cell(ui, ed, at(p::OP_VEL), "Velocity", c, true);
        knob_cell(ui, ed, at(p::OP_SPREAD), "Spread", c, wave == W_SSAW);
        knob_cell(ui, ed, at(p::OP_SS_MIX), "Sides", c, wave == W_SSAW);
    });
    // Envelope + levels
    ui.horizontal(|ui| {
        knob_cell(ui, ed, at(p::OP_ATTACK), "Attack", c, true);
        knob_cell(ui, ed, at(p::OP_DECAY), "Decay", c, true);
        knob_cell(ui, ed, at(p::OP_SUSTAIN), "Sustain", c, true);
        knob_cell(ui, ed, at(p::OP_RELEASE), "Release", c, true);
        divider(ui, s.cell_h);
        knob_cell(ui, ed, p::out(i), "Out", c, true);
        knob_cell(ui, ed, p::matrix(i, i), "Feedback", theme::GOLD, true);
    });
}

// ───────────────────────────────────────────────────────────── filter page

fn filter_page(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let c = filter_color();
    let (lp, hp) = (ed.on(p::LP_ON), ed.on(p::HP_ON));
    ui.horizontal(|ui| {
        ui.label(RichText::new("FILTER").color(c).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if pill(ui, "High-pass", hp, c).clicked() {
                ed.set(p::HP_ON, if hp { 0.0 } else { 1.0 });
            }
            ui.add_space(6.0);
            if let (Some(k), _) = seg(ui, "fm-slope", &["12 dB", "24 dB"], ed.choice(p::LP_SLOPE), c, lp) {
                ed.set(p::LP_SLOPE, k as f32);
            }
            if pill(ui, "Low-pass", lp, c).clicked() {
                ed.set(p::LP_ON, if lp { 0.0 } else { 1.0 });
            }
        });
    });
    filter_graph(ui, ed, vec2(s.page_w, 120.0), c);
    ui.horizontal(|ui| {
        knob_cell(ui, ed, p::CUTOFF, "Cutoff", c, lp);
        knob_cell(ui, ed, p::RESO, "Reso", c, lp);
        knob_cell(ui, ed, p::F_ENV, "Env", c, lp);
        knob_cell(ui, ed, p::KEYTRACK, "Key", c, lp);
        knob_cell(ui, ed, p::F_LFO, "LFO", lfo_color(), lp);
        knob_cell(ui, ed, p::HP_CUTOFF, "HP cut", c, hp);
        knob_cell(ui, ed, p::HP_RESO, "HP reso", c, hp);
    });
    ui.horizontal(|ui| {
        let env_w = s.page_w - 4.0 * (s.cell_w + 5.0);
        ui.add_enabled_ui(lp, |ui| {
            envelope(
                ui,
                ed,
                [p::F_ATTACK, p::F_DECAY, p::F_SUSTAIN, p::F_RELEASE],
                c,
                vec2(env_w, s.cell_h),
                "fm-f-env",
            );
        });
        knob_cell(ui, ed, p::F_ATTACK, "Attack", c, lp);
        knob_cell(ui, ed, p::F_DECAY, "Decay", c, lp);
        knob_cell(ui, ed, p::F_SUSTAIN, "Sustain", c, lp);
        knob_cell(ui, ed, p::F_RELEASE, "Release", c, lp);
    });
}

/// SVF low-pass magnitude at `w` = f / cutoff, damping `k`.
fn svf_lp(w: f32, k: f32) -> f32 {
    1.0 / ((1.0 - w * w).powi(2) + (k * w).powi(2)).sqrt()
}
fn reso_k(reso: f32) -> f32 {
    2.0 - 1.98 * reso.clamp(0.0, 1.0)
}

/// The filters' combined response, with a draggable dot for each one that is on:
/// left/right = cutoff, up/down = resonance.
fn filter_graph(ui: &mut Ui, ed: &mut Ed, size: Vec2, c: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let inner = rect.shrink2(vec2(8.0, 8.0));
    let decades = 1000f32.ln();
    let fx = |f: f32| inner.left() + (f / 20.0).max(1e-3).ln() / decades * inner.width();
    let f_of = |x: f32| 20.0 * ((x - inner.left()) / inner.width() * decades).exp();
    let (top_db, bot_db) = (24.0, -48.0);
    let fy = |db: f32| inner.top() + (top_db - db.clamp(bot_db, top_db)) / (top_db - bot_db) * inner.height();
    let db_of = |y: f32| top_db - (y - inner.top()) / inner.height() * (top_db - bot_db);

    // Handles first, so the curve drawn below already shows this frame's drag.
    for (on, cut_id, reso_id, lp, name) in [
        (ed.on(p::LP_ON), p::CUTOFF, p::RESO, true, "fm-lp-dot"),
        (ed.on(p::HP_ON), p::HP_CUTOFF, p::HP_RESO, false, "fm-hp-dot"),
    ] {
        if !on {
            continue;
        }
        let peak = |reso: f32| {
            let g = 1.0 / reso_k(reso) * if lp && ed.on(p::LP_SLOPE) { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
            20.0 * g.log10()
        };
        let at = pos2(fx(ed.v(cut_id)), fy(peak(ed.v(reso_id))));
        let r =
            ui.interact(Rect::from_center_size(at, vec2(22.0, 22.0)), Id::new((name, ed.ch)), Sense::click_and_drag());
        if r.drag_started() || r.double_clicked() {
            ed.h.checkpoint(ed.p);
        }
        if r.double_clicked() {
            ed.put(cut_id, FM_PARAMS[cut_id].default);
            ed.put(reso_id, FM_PARAMS[reso_id].default);
        } else if r.dragged() {
            if let Some(pos) = r.interact_pointer_pos() {
                ed.put(cut_id, f_of(pos.x));
                let mut g = 10f32.powf(db_of(pos.y) / 20.0).max(0.5);
                if lp && ed.on(p::LP_SLOPE) {
                    g *= std::f32::consts::SQRT_2;
                }
                ed.put(reso_id, (2.0 - 1.0 / g) / 1.98);
            }
        }
        let tip = if lp { "Low-pass" } else { "High-pass" };
        r.on_hover_text(format!("{tip}: drag sideways for cutoff, up/down for resonance. Double-click resets."));
    }

    let painter = ui.painter_at(rect);
    well(&painter, rect);
    let small = FontId::proportional(theme::m().text_small - 2.0);
    for (f, label) in [(100.0, "100"), (1000.0, "1k"), (10_000.0, "10k")] {
        let x = fx(f);
        painter.line_segment(
            [pos2(x, inner.top()), pos2(x, inner.bottom())],
            stroke(1.0, theme::RAISED.gamma_multiply(0.6)),
        );
        painter.text(pos2(x + 3.0, inner.bottom()), Align2::LEFT_BOTTOM, label, small.clone(), theme::TEXT_DIM);
    }
    painter.line_segment([pos2(inner.left(), fy(0.0)), pos2(inner.right(), fy(0.0))], stroke(1.0, theme::RAISED));

    let (lp, lp24, hp) = (ed.on(p::LP_ON), ed.on(p::LP_SLOPE), ed.on(p::HP_ON));
    let (cut, lk, hcut, hk) = (ed.v(p::CUTOFF), reso_k(ed.v(p::RESO)), ed.v(p::HP_CUTOFF), reso_k(ed.v(p::HP_RESO)));
    let mag = |f: f32| {
        let mut g = 1.0;
        if lp {
            let w = f / cut;
            g *= svf_lp(w, lk);
            if lp24 {
                g *= svf_lp(w, std::f32::consts::SQRT_2);
            }
        }
        if hp {
            let w = f / hcut;
            g *= w * w * svf_lp(w, hk);
        }
        g
    };
    let ink = if lp || hp { c } else { c.gamma_multiply(0.35) };
    let n = inner.width() as usize;
    let pts: Vec<_> = (0..=n)
        .map(|k| {
            let x = inner.left() + k as f32;
            pos2(x, fy(20.0 * mag(f_of(x)).max(1e-6).log10()))
        })
        .collect();
    fill_under(&painter, &pts, inner.bottom(), ink.gamma_multiply(0.14));
    painter.add(Shape::line(pts, stroke(2.0, ink)));
    if !lp && !hp {
        painter.text(inner.center(), Align2::CENTER_CENTER, "filters off", small, theme::TEXT_DIM);
    }
    for (on, cut_id, reso_id, is_lp) in [(lp, p::CUTOFF, p::RESO, true), (hp, p::HP_CUTOFF, p::HP_RESO, false)] {
        if on {
            let g = 1.0 / reso_k(ed.v(reso_id)) * if is_lp && lp24 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
            let at = pos2(fx(ed.v(cut_id)), fy(20.0 * g.log10()));
            handle(&painter, at, c, is_lp);
        }
    }
}

// ───────────────────────────────────────────────────────────── LFO page

fn lfo_page(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let c = lfo_color();
    let shape = ed.choice(p::LFO_SHAPE);
    ui.horizontal(|ui| {
        ui.label(RichText::new("LFO").color(c).strong());
        ui.label(RichText::new("one for the whole instrument").small().color(theme::TEXT_DIM));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let r = seg(ui, "fm-retrig", &["Free", "Retrigger"], ed.choice(p::LFO_RETRIG), c, true);
            if let Some(k) = r.0 {
                ed.set(p::LFO_RETRIG, k as f32);
            }
            r.1.on_hover_text("Retrigger: the LFO restarts on every note.");
        });
    });
    let labels = FM_PARAMS[p::LFO_SHAPE].labels;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (k, name) in labels.iter().enumerate() {
            if tile(ui, vec2(52.0, 30.0), Glyph::Lfo(k), k == shape, c).on_hover_text(*name).clicked() {
                ed.set(p::LFO_SHAPE, k as f32);
            }
        }
    });
    // One second of the LFO, so the rate is visible.
    let (rect, _) = ui.allocate_exact_size(vec2(s.page_w, 84.0), Sense::hover());
    let painter = ui.painter_at(rect);
    well(&painter, rect);
    let inner = rect.shrink2(vec2(8.0, 12.0));
    painter.line_segment([inner.left_center(), inner.right_center()], stroke(1.0, theme::RAISED));
    let rate = ed.v(p::LFO_RATE);
    draw_curve(&painter, inner, &|t| lfo_at(shape, t), rate, c, 2.0, true);
    painter.text(
        rect.right_bottom() - vec2(7.0, 4.0),
        Align2::RIGHT_BOTTOM,
        "1 second",
        FontId::proportional(theme::m().text_small - 2.0),
        theme::TEXT_DIM,
    );

    ui.horizontal(|ui| {
        knob_cell(ui, ed, p::LFO_RATE, "Rate", c, true);
        divider(ui, s.cell_h);
        knob_cell(ui, ed, p::VIBRATO, "Vibrato", c, true);
        knob_cell(ui, ed, p::F_LFO, "Cutoff", filter_color(), ed.on(p::LP_ON));
    });
    ui.label(RichText::new("WARP / PULSE WIDTH").small().color(theme::TEXT_DIM));
    ui.horizontal(|ui| {
        for (i, &c) in OP_COLORS.iter().enumerate() {
            let warps = ed.choice(op(i, p::OP_WAVE)) <= W_PULSE;
            knob_cell(ui, ed, op(i, p::OP_PWM), &format!("OP{}", i + 1), c, warps);
        }
    });
}

// ───────────────────────────────────────────────────────────── matrix + voice

fn matrix(ui: &mut Ui, ed: &mut Ed, page: &mut Page, used: &[bool; OPS]) {
    let s = sizes();
    let small = FontId::proportional(theme::m().text_small);
    let w = s.mlabel_w + 7.0 * (s.mcell.x + 3.0);
    ui.allocate_ui_with_layout(vec2(w, s.tab_h), Layout::left_to_right(Align::Center), |ui| {
        ui.label(RichText::new("FM MATRIX").small().color(theme::TEXT_DIM)).on_hover_text(
            "Each row is an operator sending into each column: drag a cell up to modulate.\n\
             The gold diagonal is feedback (an operator modulating itself). OUT is how loud it is.",
        );
    });
    panel(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
        // Column heads
        ui.horizontal(|ui| {
            ui.add_space(s.mlabel_w);
            for col in 0..=OPS {
                let (r, _) = ui.allocate_exact_size(vec2(s.mcell.x, 16.0), Sense::hover());
                let (text, color) = if col == OPS {
                    ("OUT".to_string(), theme::MINT)
                } else {
                    let c = OP_COLORS[col];
                    ((col + 1).to_string(), if used[col] { c } else { c.gamma_multiply(0.4) })
                };
                ui.painter().text(r.center(), Align2::CENTER_CENTER, text, small.clone(), color);
            }
        });
        for row in 0..OPS {
            ui.horizontal(|ui| {
                let c = OP_COLORS[row];
                let (r, resp) = ui.allocate_exact_size(vec2(s.mlabel_w - 3.0, s.mcell.y), Sense::click());
                let active = *page == Page::Op(row);
                let painter = ui.painter();
                if active {
                    painter.rect_filled(r, cr(5), c);
                } else if resp.hovered() {
                    painter.rect_filled(r, cr(5), theme::mix(theme::RAISED, c, 0.2));
                }
                let ink = if active {
                    theme::INSET
                } else if used[row] {
                    c
                } else {
                    c.gamma_multiply(0.4)
                };
                painter.text(r.center(), Align2::CENTER_CENTER, format!("OP{}", row + 1), small.clone(), ink);
                if resp.on_hover_text("Open this operator").clicked() {
                    *page = Page::Op(row);
                }
                for col in 0..=OPS {
                    let (id, kind) = if col == OPS {
                        (p::out(row), Cell::Out)
                    } else if col == row {
                        (p::matrix(row, col), Cell::Feedback)
                    } else {
                        (p::matrix(row, col), Cell::Mod)
                    };
                    let mut v = ed.v(id);
                    let r = matrix_cell(ui, &mut v, id, s.mcell, c, kind);
                    let tip = match kind {
                        Cell::Out => format!("OP{} to the output: {:.0}%", row + 1, v * 100.0),
                        Cell::Feedback => format!("OP{} feedback: {:.0}%", row + 1, v * 100.0),
                        Cell::Mod => format!("OP{} into OP{}: {:.0}%", row + 1, col + 1, v * 100.0),
                    };
                    let r = r.on_hover_text(tip);
                    ed.track(&r, id, v);
                }
            });
        }
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Cell {
    Mod,
    Feedback,
    Out,
}

/// One matrix cell: drag up/down (Shift = fine), wheel = 1% steps, double-click = reset.
/// It fills from the bottom in its row's colour.
fn matrix_cell(ui: &mut Ui, v: &mut f32, id: usize, size: Vec2, color: Color32, kind: Cell) -> Response {
    let info = &FM_PARAMS[id];
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let before = *v;
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        *v -= resp.drag_delta().y * if fine { 0.001 } else { 0.005 };
    }
    if resp.hovered() {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            *v = ((*v * 100.0).round() + wheel.signum()) / 100.0;
        }
    }
    if resp.double_clicked() {
        *v = info.default;
    }
    *v = v.clamp(info.min, info.max);
    if *v != before {
        resp.mark_changed();
    }

    let painter = ui.painter();
    let hot = resp.hovered() || resp.dragged();
    let fill_color = if kind == Cell::Feedback { theme::GOLD } else { color };
    painter.rect_filled(rect, cr(4), if hot { theme::mix(theme::INSET, fill_color, 0.1) } else { theme::INSET });
    if *v > 0.0 {
        let h = (rect.height() * *v).max(2.0);
        let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - h), rect.right_bottom());
        painter.rect_filled(bar, cr(4), fill_color.gamma_multiply(0.55));
    }
    let edge = match kind {
        Cell::Feedback => theme::GOLD.gamma_multiply(0.75),
        Cell::Out => theme::MINT.gamma_multiply(0.6),
        Cell::Mod if hot => fill_color.gamma_multiply(0.7),
        Cell::Mod => theme::LINE,
    };
    painter.rect_stroke(rect, cr(4), stroke(1.0, edge), StrokeKind::Inside);
    if *v > 0.0 {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            format!("{:.0}", *v * 100.0),
            FontId::monospace(theme::m().text_small),
            theme::TEXT,
        );
    }
    resp
}

fn voice(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let c = theme::MINT;
    ui.label(RichText::new("VOICE").small().color(theme::TEXT_DIM));
    panel(ui, |ui| {
        ui.set_width(s.mlabel_w + 7.0 * (s.mcell.x + 3.0) - 3.0);
        ui.horizontal_top(|ui| {
            knob_cell(ui, ed, p::VOLUME, "Volume", c, true);
            knob_cell(ui, ed, p::GLIDE, "Glide", c, true);
            ui.vertical(|ui| {
                ui.add_space(4.0);
                let r = seg(ui, "fm-mode", &["Poly", "Mono"], ed.choice(p::MODE), c, true);
                if let Some(k) = r.0 {
                    ed.set(p::MODE, k as f32);
                }
                r.1.on_hover_text("Mono: one note at a time, with legato and glide.");
                ui.add_space(4.0);
                let glides = ed.v(p::GLIDE) > 0.0;
                let r = seg(ui, "fm-glide-mode", &["Legato", "Always"], ed.choice(p::GLIDE_MODE), c, glides);
                if let Some(k) = r.0 {
                    ed.set(p::GLIDE_MODE, k as f32);
                }
                r.1.on_hover_text("Glide only between overlapping notes, or on every note.");
            });
        });
    });
}

// ───────────────────────────────────────────────────────────── shared pieces

/// A knob with its name and value under it, in the parameter's colour. Disabled = dimmed and inert.
fn knob_cell(ui: &mut Ui, ed: &mut Ed, id: usize, label: &str, color: Color32, enabled: bool) {
    let s = sizes();
    let m = theme::m();
    let info = &FM_PARAMS[id];
    ui.allocate_ui(vec2(s.cell_w, s.cell_h), |ui| {
        ui.add_enabled_ui(enabled, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                let c = if enabled { color } else { color.gamma_multiply(0.3) };
                let mut v = ed.v(id);
                let r = widgets::knob(ui, &mut v, info, m.knob_large, c);
                if enabled {
                    ed.track(&r, id, v);
                }
                ui.add_space(2.0);
                ui.label(RichText::new(label).small());
                ui.label(RichText::new(format_value(id, ed.v(id))).small().color(theme::TEXT_DIM));
            });
        });
    });
}

fn format_value(id: usize, v: f32) -> String {
    if id < p::MATRIX && id % p::OP_STRIDE == p::OP_RATIO {
        format!("×{v:.2}")
    } else {
        FM_PARAMS[id].format(v)
    }
}

/// A rounded segmented switch ("Poly | Mono"). Returns the clicked segment (if it changed) and a
/// response covering the whole switch (for a tooltip).
fn seg(
    ui: &mut Ui,
    salt: impl std::hash::Hash,
    labels: &[&str],
    current: usize,
    color: Color32,
    enabled: bool,
) -> (Option<usize>, Response) {
    let m = theme::m();
    let h = m.control_h - 2.0;
    let font = FontId::proportional(m.text_small + 1.0);
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), theme::TEXT).size().x + 18.0)
        .collect();
    let (rect, whole) = ui.allocate_exact_size(vec2(widths.iter().sum::<f32>() + 4.0, h), Sense::hover());
    let id = Id::new(salt);
    let round = CornerRadius::same((h * 0.5) as u8);
    let mut picked = None;
    let painter = ui.painter();
    painter.rect_filled(rect, round, theme::INSET);
    painter.rect_stroke(rect, round, stroke(1.0, theme::LINE), StrokeKind::Inside);
    let mut x = rect.left() + 2.0;
    for (k, (label, w)) in labels.iter().zip(&widths).enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(*w, h - 4.0));
        x += w;
        let resp = ui.interact(r, id.with(k), if enabled { Sense::click() } else { Sense::hover() });
        let selected = k == current;
        let inner_round = CornerRadius::same((h * 0.5 - 2.0) as u8);
        if selected {
            painter.rect_filled(r, inner_round, if enabled { color } else { color.gamma_multiply(0.3) });
        } else if enabled && resp.hovered() {
            painter.rect_filled(r, inner_round, theme::mix(theme::INSET, color, 0.18));
        }
        let ink = if selected {
            theme::INSET
        } else if enabled && resp.hovered() {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        };
        painter.text(r.center(), Align2::CENTER_CENTER, *label, font.clone(), ink);
        if enabled && resp.clicked() && !selected {
            picked = Some(k);
        }
    }
    (picked, whole)
}

/// An on/off pill with a little light.
fn pill(ui: &mut Ui, label: &str, on: bool, color: Color32) -> Response {
    let m = theme::m();
    let h = m.control_h - 2.0;
    let font = FontId::proportional(m.text_small + 1.0);
    let text_w = ui.painter().layout_no_wrap(label.to_string(), font.clone(), theme::TEXT).size().x;
    let (rect, resp) = ui.allocate_exact_size(vec2(text_w + 34.0, h), Sense::click());
    let painter = ui.painter();
    let round = CornerRadius::same((h * 0.5) as u8);
    let fill = match (on, resp.hovered()) {
        (true, _) => theme::mix(theme::RAISED, color, 0.28),
        (false, true) => theme::mix(theme::RAISED, color, 0.12),
        (false, false) => theme::RAISED,
    };
    painter.rect_filled(rect, round, fill);
    painter.rect_stroke(
        rect,
        round,
        stroke(1.0, if on { color.gamma_multiply(0.6) } else { theme::LINE }),
        StrokeKind::Inside,
    );
    let dot = pos2(rect.left() + 13.0, rect.center().y);
    if on {
        painter.circle_filled(dot, 7.0, color.gamma_multiply(0.2));
        painter.circle_filled(dot, 4.0, color);
    } else {
        painter.circle_filled(dot, 4.0, theme::INSET);
        painter.circle_stroke(dot, 4.0, stroke(1.0, theme::LINE));
    }
    painter.text(
        pos2(dot.x + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        if on { theme::TEXT } else { theme::TEXT_DIM },
    );
    resp
}

/// A wave/shape tile: selected = filled in the colour, the shape drawn dark on it.
fn tile(ui: &mut Ui, size: Vec2, glyph: Glyph, selected: bool, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let (fill, ink) = if selected {
        (color, theme::INSET)
    } else if resp.hovered() {
        (theme::mix(theme::INSET, color, 0.16), color)
    } else {
        (theme::INSET, color.gamma_multiply(0.7))
    };
    painter.rect_filled(rect, cr(6), fill);
    painter.rect_stroke(rect, cr(6), stroke(1.0, theme::LINE), StrokeKind::Inside);
    draw_glyph(painter, rect.shrink2(vec2(6.0, 7.0)), glyph, ink, 1.5);
    resp
}

/// A rounded panel on the window background (matrix, voice).
fn panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::default()
        .fill(theme::PANEL_HI)
        .stroke(stroke(1.0, theme::LINE))
        .corner_radius(cr(8))
        .inner_margin(egui::Margin::same(8))
        .show(ui, add)
        .inner
}

/// A dark, sunken display area.
fn well(painter: &egui::Painter, rect: Rect) {
    painter.rect_filled(rect, cr(6), theme::INSET);
    painter.rect_stroke(rect, cr(6), stroke(1.0, theme::LINE), StrokeKind::Inside);
}

fn divider(ui: &mut Ui, h: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(6.0, h), Sense::hover());
    ui.painter()
        .line_segment([r.center_top() + vec2(0.0, 6.0), r.center_bottom() - vec2(0.0, 6.0)], stroke(1.0, theme::LINE));
}

/// A draggable dot. `solid` = filled, else a ring.
fn handle(painter: &egui::Painter, at: egui::Pos2, color: Color32, solid: bool) {
    painter.circle_filled(at, 9.0, color.gamma_multiply(0.15));
    if solid {
        painter.circle_filled(at, 5.0, color);
        painter.circle_stroke(at, 5.0, stroke(1.5, theme::LINE));
    } else {
        painter.circle_filled(at, 5.0, theme::INSET);
        painter.circle_stroke(at, 4.5, stroke(2.0, color));
    }
}

/// Draw `f(t)` (t in cycles, output -1..1) over `cycles` cycles, filling towards the middle.
#[allow(clippy::too_many_arguments)]
fn draw_curve(
    painter: &egui::Painter,
    rect: Rect,
    f: &dyn Fn(f32) -> f32,
    cycles: f32,
    color: Color32,
    width: f32,
    fill: bool,
) {
    let n = (rect.width() * 1.5).max(16.0) as usize;
    let mid = rect.center().y;
    let pts: Vec<_> = (0..=n)
        .map(|k| {
            let x = k as f32 / n as f32;
            pos2(rect.left() + x * rect.width(), mid - f(x * cycles).clamp(-1.0, 1.0) * rect.height() * 0.5)
        })
        .collect();
    if fill {
        fill_under(painter, &pts, mid, color.gamma_multiply(0.13));
    }
    painter.add(Shape::line(pts, stroke(width, color)));
}

/// Thin vertical slivers between a polyline and a baseline: a cheap filled area.
fn fill_under(painter: &egui::Painter, pts: &[egui::Pos2], base: f32, color: Color32) {
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let y = (a.y + b.y) * 0.5;
        let r = Rect::from_min_max(pos2(a.x, y.min(base)), pos2(b.x.max(a.x + 0.5), y.max(base)));
        painter.rect_filled(r, cr(0), color);
    }
}

/// Fluttersine's waveforms, for pictures. `t` in cycles; `pw` warps the phase like the engine.
fn wave_at(w: usize, t: f32, pw: f32, spread: f32, sides: f32) -> f32 {
    let ph = t.rem_euclid(1.0);
    let sin = |x: f32| (x * TAU).sin();
    let saw = |x: f32| 2.0 * x.rem_euclid(1.0) - 1.0;
    if w == W_SSAW {
        let side = sides.clamp(0.0, 1.0);
        let mut y = saw(t);
        for (k, d) in SS_DETUNE.iter().enumerate() {
            y += side * saw(t * (1.0 + d * (0.3 + 3.0 * spread)) + k as f32 * 0.17);
        }
        return y / (1.0 + 2.45 * side);
    }
    if w > W_SSAW {
        // Noise: a fixed scribble.
        let k = (t * 37.0).floor() as i32 as u32;
        let mut x = k.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
        x ^= x >> 15;
        x = x.wrapping_mul(0x2c1b_3c6d);
        x ^= x >> 12;
        return (x & 0xffff) as f32 / 32767.5 - 1.0;
    }
    let pw = pw.clamp(0.02, 0.98);
    let p = if ph < pw { ph * 0.5 / pw } else { 0.5 + (ph - pw) * 0.5 / (1.0 - pw) };
    match w {
        0 => sin(p),
        1 => {
            if p < 0.5 {
                sin(p)
            } else {
                0.0
            }
        }
        2 => sin(p).abs(),
        3 => {
            if p.rem_euclid(0.5) < 0.25 {
                sin(p).abs()
            } else {
                0.0
            }
        }
        4 => {
            if p < 0.5 {
                sin(2.0 * p)
            } else {
                0.0
            }
        }
        5 => {
            if p < 0.5 {
                sin(2.0 * p).abs()
            } else {
                0.0
            }
        }
        6 => {
            if p < 0.25 {
                4.0 * p
            } else if p < 0.75 {
                2.0 - 4.0 * p
            } else {
                4.0 * p - 4.0
            }
        }
        7 => 2.0 * p - 1.0,
        _ => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
    }
}

/// The LFO's shapes, as the engine runs them.
fn lfo_at(shape: usize, t: f32) -> f32 {
    let ph = t.rem_euclid(1.0);
    match shape {
        1 => {
            if ph < 0.5 {
                4.0 * ph - 1.0
            } else {
                3.0 - 4.0 * ph
            }
        }
        2 => 1.0 - 2.0 * ph,
        3 => {
            if ph < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        4 => {
            // Sample & hold: a fixed sequence of steps.
            let k = t.floor() as i32 as u32;
            let mut x = k.wrapping_mul(0x9e37_79b9).wrapping_add(0x7f4a_7c15);
            x ^= x >> 16;
            x = x.wrapping_mul(0x85eb_ca6b);
            x ^= x >> 13;
            ((x & 0xffff) as f32 / 32767.5 - 1.0) * 0.9
        }
        _ => (ph * TAU).sin(),
    }
}

// ───────────────────────────────────────────────────────────── envelope editor

/// An ADSR you can drag: the attack peak (time), the decay/sustain corner (time and level)
/// and the release end (time). Each stage gets its own lane, so times are on the same
/// curve as their knobs. Double-click a dot to reset what it moves.
fn envelope(ui: &mut Ui, ed: &mut Ed, ids: [usize; 4], color: Color32, size: Vec2, salt: impl std::hash::Hash) {
    let [a_id, d_id, s_id, r_id] = ids;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let enabled = ui.is_enabled();
    let inner = rect.shrink2(vec2(10.0, 12.0));
    let lane = inner.width() / 3.5;
    let hold = lane * 0.5;
    let n = |id: usize, ed: &Ed| FM_PARAMS[id].to_normalized(ed.v(id));
    let points = |ed: &Ed| {
        let xa = inner.left() + n(a_id, ed) * lane;
        let xd = xa + n(d_id, ed) * lane;
        let xs = inner.left() + 2.0 * lane + hold;
        let xr = xs + n(r_id, ed) * lane;
        (xa, xd, xs, xr, ed.v(s_id))
    };
    let y = |level: f32| inner.bottom() - level * inner.height();
    let base = Id::new(salt).with(ed.ch);

    let (xa, xd, xs, _, s) = points(ed);
    let dots = [(0, pos2(xa, y(1.0))), (1, pos2(xd, y(s))), (2, pos2(points(ed).3, y(0.0)))];
    let mut hot = None;
    if enabled {
        for (k, at) in dots {
            let r = ui.interact(Rect::from_center_size(at, vec2(20.0, 20.0)), base.with(k), Sense::click_and_drag());
            if r.drag_started() || r.double_clicked() {
                ed.h.checkpoint(ed.p);
            }
            let resets: &[usize] = match k {
                0 => &[a_id],
                1 => &[d_id, s_id],
                _ => &[r_id],
            };
            if r.double_clicked() {
                for &id in resets {
                    ed.put(id, FM_PARAMS[id].default);
                }
            } else if r.dragged() {
                if let Some(pos) = r.interact_pointer_pos() {
                    let from_lane = |x0: f32, id: usize| FM_PARAMS[id].from_normalized((pos.x - x0) / lane);
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
            let tip = resets.iter().map(|&id| format!("{} {}", FM_PARAMS[id].name, format_value(id, ed.v(id))));
            r.on_hover_text(tip.collect::<Vec<_>>().join(" · "));
        }
    }

    // Paint with this frame's values.
    let (xa, xd, xs, xr, s) = points(ed);
    let painter = ui.painter_at(rect);
    well(&painter, rect);
    let ink = if enabled { color } else { color.gamma_multiply(0.3) };
    for x in [inner.left() + lane, inner.left() + 2.0 * lane, xs] {
        painter.line_segment(
            [pos2(x, inner.top()), pos2(x, inner.bottom())],
            stroke(1.0, theme::RAISED.gamma_multiply(0.5)),
        );
    }
    let mut pts = vec![pos2(inner.left(), y(0.0)), pos2(xa, y(1.0))];
    for k in 1..=20 {
        let f = k as f32 / 20.0;
        pts.push(pos2(xa + (xd - xa) * f, y(s + (1.0 - s) * (-5.0 * f).exp() * (1.0 - f))));
    }
    pts.push(pos2(xs, y(s)));
    for k in 1..=20 {
        let f = k as f32 / 20.0;
        pts.push(pos2(xs + (xr - xs) * f, y(s * (-5.0 * f).exp() * (1.0 - f))));
    }
    fill_under(&painter, &pts, inner.bottom(), ink.gamma_multiply(0.14));
    painter.add(Shape::line(pts, stroke(2.0, ink)));
    for (k, at) in [(0, pos2(xa, y(1.0))), (1, pos2(xd, y(s))), (2, pos2(xr, y(0.0)))] {
        if hot == Some(k) {
            painter.circle_filled(at, 10.0, ink.gamma_multiply(0.25));
        }
        handle(&painter, at, ink, true);
    }
}
