//! The hand-made editor for Lyra FM, a.k.a. **Fluttersine** (Sytrus-style).
//!
//! ```text
//!  ┌ ◀ [ preset ▾ ] ▶ ───────────────────────┐   Fluttersine (logo,
//!  ├OP1┼OP2┼OP3┼OP4┼OP5┼OP6┼FILTER┼LFO┼MOD───┤   no frame)
//!  ┌ the open page ──────────────────────────┐┌ FM matrix ──────┐
//!  │ wave keys · LCD wave · LCD envelope     ││ 6×7 knobs       │
//!  │ knobs                                   │├ voice ──────────┤
//!  └─────────────────────────────────────────┘└─────────────────┘
//! ```
//!
//! Lyra's visual language: fixed plates on a dark rack, a seam apart, so
//! nothing moves when you switch pages. The open tab is just a brighter plate
//! with a thin line of its light on top (like the selected mixer strip).
//! Colour is only light: LEDs, lit keys and knob arcs, one light per operator. Text stays grey, and the curves are dark ink on LCDs.
//!
//! * The matrix is always visible; clicking a row's name opens that operator.
//! * Envelope dots and the filter dots can be dragged; the knobs below are the
//!   same parameters.
//! * Wheel over the tabs, the wave keys or the preset dropdown steps through them.
//! * LFO holds both LFOs (one at a time) with their tempo sync; MOD holds the
//!   two mod envelopes and the six mod slots (source → destination × amount).
//!
//! Like every view, this only edits `channel.params`; the bridge does the rest.
//! The waveform pictures are display-only copies of `lyra_fluttersine`'s shapes.

use crate::rack::tracked;
use crate::theme::{self, cr, stroke, Light};
use crate::widgets::{self, Icon, Legend};
use egui::{pos2, vec2, Align, Align2, Color32, FontId, Id, Layout, Rect, Response, RichText, Sense, Shape, Ui};
use egui::{UiBuilder, Vec2};
use lyra_fluttersine::{p, FM_PARAMS, FM_PRESETS};
use lyra_model::{History, Project};
use p::{op, OPS};
use std::f32::consts::TAU;

/// One light per operator: a rainbow, so an operator is easy to follow from
/// its tab to its matrix row and its knobs.
const OP_LIGHTS: [Light; OPS] =
    [theme::L_PINK, theme::L_PEACH, theme::L_AMBER, theme::L_MINT, theme::L_BLUE, theme::L_LAVENDER];
const FILTER_LIGHT: Light = theme::L_CYAN;
const LFO_LIGHT: Light = theme::L_LIME;
const VOICE_LIGHT: Light = theme::L_MINT;
/// The MOD page shares the voice's light: both are about the whole patch.
const MOD_LIGHT: Light = VOICE_LIGHT;

/// Waveform indices that behave differently (see `lyra_fluttersine`).
const W_PULSE: usize = 8;
const W_SSAW: usize = 9;
/// Supersaw side-voice detunes, for the picture only.
const SS_DETUNE: [f32; 6] = [-0.110, -0.063, -0.020, 0.020, 0.062, 0.107];

/// Padding inside a plate.
const PAD: f32 = 8.0;
/// Space between the matrix and its OUT column.
const OUT_GAP: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Op(usize),
    Filter,
    Lfo,
    Mod,
}

impl Page {
    const COUNT: usize = OPS + 3;
    fn index(self) -> usize {
        match self {
            Page::Op(i) => i,
            Page::Filter => OPS,
            Page::Lfo => OPS + 1,
            Page::Mod => OPS + 2,
        }
    }
    fn from_index(i: usize) -> Page {
        match i {
            i if i < OPS => Page::Op(i),
            i if i == OPS => Page::Filter,
            i if i == OPS + 1 => Page::Lfo,
            _ => Page::Mod,
        }
    }
    fn light(self) -> Light {
        match self {
            Page::Op(i) => OP_LIGHTS[i],
            Page::Filter => FILTER_LIGHT,
            Page::Lfo => LFO_LIGHT,
            Page::Mod => MOD_LIGHT,
        }
    }
    fn name(self) -> String {
        match self {
            Page::Op(i) => format!("OP{}", i + 1),
            Page::Filter => "FILTER".into(),
            Page::Lfo => "LFO".into(),
            Page::Mod => "MOD".into(),
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

/// Every size, derived from the theme's metrics so F12 still drives them. The
/// plates have fixed sizes: switching pages never resizes the window.
struct Sizes {
    cell_w: f32,
    cell_h: f32,
    /// Height of keys and radio keys.
    key_h: f32,
    /// Inner width of the left column (preset row, tabs, page).
    left_w: f32,
    /// Inner width of the right column (voice, matrix, logo).
    right_w: f32,
    preset_h: f32,
    tab_h: f32,
    page_h: f32,
    matrix_h: f32,
    mcell: Vec2,
    mlabel_w: f32,
    /// Height of the LCD displays on the operator page.
    display_h: f32,
    /// The MOD page: small knob cells (envelopes) and one row per slot.
    small_cell_w: f32,
    small_cell_h: f32,
    slot_h: f32,
}

fn sizes() -> Sizes {
    let m = theme::m();
    let cell_w = m.knob_large + 22.0;
    let cell_h = m.knob_large + 2.0 * m.text_small + 14.0;
    let key_h = m.control_h;
    let gap = 6.0;
    let display_h = 104.0;
    // Matrix cells: square, side by side with no gaps, a small knob in each.
    let mcell = Vec2::splat(m.knob_small + 6.0);
    let mlabel_w = 30.0;
    let header_h = 16.0;
    Sizes {
        cell_w,
        cell_h,
        key_h,
        left_w: 7.0 * cell_w + 6.0 * 5.0,
        right_w: mlabel_w + 6.0 * mcell.x + OUT_GAP + 2.0 * mcell.x,
        // Tall enough that the voice plate beside it (preset row + tabs) fits a row of knobs.
        preset_h: m.control_h + 14.0,
        tab_h: m.control_h + 8.0,
        page_h: key_h + gap + 32.0 + gap + display_h + gap + 2.0 * cell_h + gap,
        matrix_h: header_h + 2.0 + 6.0 * mcell.y,
        mcell,
        mlabel_w,
        display_h,
        small_cell_w: m.knob_small + 24.0,
        small_cell_h: m.knob_small + 2.0 * m.text_small + 12.0,
        slot_h: key_h + 6.0,
    }
}

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, ch: usize) {
    let mut ed = Ed { p, h, ch };
    let s = sizes();
    let seam = theme::SEAM;
    let page_id = Id::new(("fm-page", ch));
    let stored = ui.ctx().data(|d| d.get_temp::<usize>(page_id)).unwrap_or(0);
    let mut page = Page::from_index(stored);
    let used = used_ops(&ed);

    // The rack: one dark frame, the plates carved out of it a seam apart.
    let outer = |inner: f32| inner + 2.0 * PAD;
    let (lw, rw) = (outer(s.left_w), outer(s.right_w));
    let (preset_h, page_h) = (outer(s.preset_h), outer(s.page_h));
    let left_h = preset_h + seam + s.tab_h + seam + page_h;
    let (rack, _) = ui.allocate_exact_size(vec2(seam + lw + seam + rw + seam, seam + left_h + seam), Sense::hover());
    let x0 = rack.left() + seam;
    let x1 = x0 + lw + seam;
    let y0 = rack.top() + seam;
    let preset_r = Rect::from_min_size(pos2(x0, y0), vec2(lw, preset_h));
    let tabs_r = Rect::from_min_size(pos2(x0, preset_r.bottom() + seam), vec2(lw, s.tab_h));
    let page_r = Rect::from_min_size(pos2(x0, tabs_r.bottom() + seam), vec2(lw, page_h));
    // Right column: voice beside the preset row and tabs, the matrix beside the
    // page, and below it open window surface where the logo sits, unframed.
    let voice_r = Rect::from_min_max(pos2(x1, y0), pos2(x1 + rw, tabs_r.bottom()));
    let matrix_r = Rect::from_min_size(pos2(x1, page_r.top()), vec2(rw, outer(s.matrix_h)));
    let frame_left = Rect::from_min_max(rack.min, pos2(x1, rack.bottom()));
    let frame_right = Rect::from_min_max(pos2(x1 - seam, rack.top()), pos2(rack.right(), matrix_r.bottom() + seam));
    ui.painter().rect_filled(frame_left, cr(3), theme::BG);
    ui.painter().rect_filled(frame_right, cr(3), theme::BG);
    let logo_r = Rect::from_min_max(pos2(x1, matrix_r.bottom() + seam), pos2(rack.right(), rack.bottom()));

    plate(ui, preset_r, "fm-preset-plate", |ui| preset_row(ui, &mut ed));
    tabs(ui, tabs_r, &mut page);
    plate(ui, page_r, ("fm-page-plate", page.index()), |ui| match page {
        Page::Op(i) => op_page(ui, &mut ed, i),
        Page::Filter => filter_page(ui, &mut ed),
        Page::Lfo => lfo_page(ui, &mut ed),
        Page::Mod => mod_page(ui, &mut ed),
    });
    logo(ui, logo_r);
    plate(ui, matrix_r, "fm-matrix-plate", |ui| matrix(ui, &mut ed, &mut page, &used));
    plate(ui, voice_r, "fm-voice-plate", |ui| voice(ui, &mut ed));

    ui.ctx().data_mut(|d| d.insert_temp(page_id, page.index()));
}

/// A flat plate (`theme::PANEL`) filling `r`, with its contents laid out inside the padding.
fn plate<R>(ui: &mut Ui, r: Rect, salt: impl std::hash::Hash, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.painter().rect_filled(r, cr(0), theme::PANEL);
    let mut child =
        ui.new_child(UiBuilder::new().id_salt(salt).max_rect(r.shrink(PAD)).layout(Layout::top_down(Align::Min)));
    child.spacing_mut().item_spacing = vec2(5.0, 6.0);
    add(&mut child)
}

/// The Fluttersine wordmark, straight on the window's surface (no plate, no frame).
fn logo(ui: &mut Ui, r: Rect) {
    let (tex, [w, h]) = theme::fluttersine_logo(ui.ctx());
    let room = r.shrink(2.0 * PAD);
    let scale = (room.width() / w as f32).min(room.height() / h as f32);
    let img = Rect::from_center_size(room.center(), vec2(w as f32, h as f32) * scale);
    ui.painter().image(tex, img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    ui.interact(r, Id::new("fm-logo"), Sense::hover()).on_hover_text("Fluttersine · FM synthesizer");
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

/// `◀ [ preset ▾ ] ▶`, a dropdown like the pattern picker. Click for the list,
/// scroll over it to step.
fn preset_row(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let n = FM_PRESETS.len();
    let current = current_preset(&ed.p.channels[ed.ch].params);
    // The last preset loaded here, so ◀ ▶ still make sense after you tweak it.
    let last_id = Id::new(("fm-last-preset", ed.ch));
    let last = ui.ctx().data(|d| d.get_temp::<usize>(last_id));
    let at = current.or(last).unwrap_or(0);
    let (name, edited) = match (current, last) {
        (Some(k), _) => (FM_PRESETS[k].0, false),
        (None, Some(k)) => (FM_PRESETS[k].0, true),
        (None, None) => ("Custom", false),
    };
    let mut pick = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let key = |ui: &mut Ui, icon, hint| {
            widgets::key(ui, vec2(s.preset_h, s.preset_h), Legend::Icon(icon), false, VOICE_LIGHT, hint)
        };
        if key(ui, Icon::Prev, "Previous preset").clicked() {
            pick = Some((at + n - 1) % n);
        }
        let well_w = ui.available_width() - s.preset_h - 4.0;
        let (r, resp) = ui.allocate_exact_size(vec2(well_w, s.preset_h), Sense::click());
        let painter = ui.painter();
        theme::well(painter, r);
        let m = theme::m();
        let text = painter.text(
            r.left_center() + vec2(10.0, 0.0),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(m.text),
            theme::TEXT,
        );
        if edited {
            painter.text(
                text.right_center() + vec2(8.0, 1.0),
                Align2::LEFT_CENTER,
                "edited",
                FontId::proportional(m.text_small),
                theme::TEXT_DIM,
            );
        }
        let c = r.right_center() - vec2(13.0, 1.5);
        let chevron = vec![c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)];
        painter.add(Shape::line(chevron, stroke(1.6, theme::TEXT_DIM)));
        if let Some(step) = wheel_step(ui, r) {
            pick = Some((at as i32 + step).rem_euclid(n as i32) as usize);
        }
        let popup = ui.make_persistent_id(("fm-preset-list", ed.ch));
        if resp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup));
        }
        let resp = resp.on_hover_text("Preset: scroll to flip through, click for the list");
        egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(r.width());
            for (k, (name, _)) in FM_PRESETS.iter().enumerate() {
                if ui.selectable_label(current == Some(k), *name).clicked() {
                    pick = Some(k);
                }
            }
        });
        if key(ui, Icon::Next, "Next preset").clicked() {
            pick = Some((at + 1) % n);
        }
    });
    if let Some(k) = pick {
        ed.h.checkpoint(ed.p);
        ed.p.channels[ed.ch].params = preset_values(k).to_vec();
        ui.ctx().data_mut(|d| d.insert_temp(last_id, k));
    }
}

/// -1 / +1 when the wheel turns over `rect` this frame (wheel down = next).
pub(crate) fn wheel_step(ui: &Ui, rect: Rect) -> Option<i32> {
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

// ───────────────────────────────────────────────────────────── tabs

/// The tabs: square blocks side by side, a seam apart. The open one is a
/// brighter plate with a thin line of its light on top. (Whether an operator is
/// in use shows once, on the matrix's LEDs.)
fn tabs(ui: &mut Ui, r: Rect, page: &mut Page) {
    let seam = theme::SEAM;
    let w = (r.width() - seam * (Page::COUNT - 1) as f32) / Page::COUNT as f32;
    let font = FontId::proportional(theme::m().text_small + 1.0);
    for k in 0..Page::COUNT {
        let pg = Page::from_index(k);
        let b = Rect::from_min_size(pos2(r.left() + k as f32 * (w + seam), r.top()), vec2(w, r.height()));
        let resp = ui.interact(b, Id::new(("fm-tab", k)), Sense::click());
        let open = *page == pg;
        let painter = ui.painter();
        let fill = if open {
            theme::PANEL_HI
        } else if resp.hovered() {
            theme::mix(theme::PANEL, theme::PANEL_HI, 0.5)
        } else {
            theme::PANEL
        };
        painter.rect_filled(b, cr(0), fill);
        if open {
            painter.rect_filled(Rect::from_min_size(b.min, vec2(b.width(), 2.0)), cr(0), pg.light().mid);
        }
        let ink = if open || resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
        painter.text(b.center() + vec2(0.0, 1.0), Align2::CENTER_CENTER, pg.name(), font.clone(), ink);
        if resp.clicked() {
            *page = pg;
        }
    }
    if let Some(step) = wheel_step(ui, r) {
        *page = Page::from_index((page.index() as i32 + step).rem_euclid(Page::COUNT as i32) as usize);
    }
}

#[derive(Clone, Copy)]
enum Glyph {
    Wave(usize),
    Lfo(usize),
}

fn draw_glyph(painter: &egui::Painter, rect: Rect, glyph: Glyph, color: Color32, width: f32) {
    match glyph {
        Glyph::Wave(w) => draw_curve(painter, rect, &|t| wave_at(w, t, 0.5, 0.35, 0.6), 1.0, color, width, None),
        // S&H holds one value per cycle, so its picture shows several cycles.
        Glyph::Lfo(s) => {
            let cycles = if s == 4 { 5.0 } else { 1.0 };
            draw_curve(painter, rect, &|t| lfo_at(s, t), cycles, color, width, None)
        }
    }
}

// ───────────────────────────────────────────────────────────── operator page

/// A page's top line: just its switches. (No title: the tab already says
/// which page this is, and the matrix shows what an operator does.)
fn switch_row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let s = sizes();
    ui.allocate_ui_with_layout(vec2(s.left_w, s.key_h), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        add(ui);
    });
}

/// Merged radio keys with text legends; returns the clicked option. Every
/// switch in the editor has the same key size, whatever its labels say.
fn switch(ui: &mut Ui, labels: &[&str], chosen: usize, light: Light, hint: &str) -> Option<usize> {
    let s = sizes();
    let w = 6.0 * 6.2 + 20.0; // room for six letters
    let options: Vec<(Legend, Light)> = labels.iter().map(|l| (Legend::Text(l), light)).collect();
    widgets::radio_keys(ui, vec2(w, s.key_h), &options, chosen, hint)
}

fn op_page(ui: &mut Ui, ed: &mut Ed, i: usize) {
    let s = sizes();
    let lt = OP_LIGHTS[i];
    let c = lt.mid;
    let at = |k: usize| op(i, k);
    let wave = ed.choice(at(p::OP_WAVE));

    let (phase, pitch) = (ed.choice(at(p::OP_RESET)), ed.choice(at(p::OP_FIXED)));
    let mut set = None;
    switch_row(ui, |ui| {
        let hint = "Ratio: follows the key you play (×1 = the note).\nFixed: always the same frequency, for bells, drums and metal.";
        if let Some(k) = switch(ui, &["RATIO", "FIXED"], pitch, lt, hint) {
            set = Some((at(p::OP_FIXED), k));
        }
        let hint = "Free: the phase keeps running between notes.\nReset: every note starts the wave from zero (punchier, repeatable).";
        if let Some(k) = switch(ui, &["FREE", "RESET"], phase, lt, hint) {
            set = Some((at(p::OP_RESET), k));
        }
    });
    if let Some((id, k)) = set {
        ed.set(id, k as f32);
    }

    // Wave keys
    let labels = FM_PARAMS[at(p::OP_WAVE)].labels;
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let n = labels.len() as f32;
        let w = (s.left_w - 3.0 * (n - 1.0)) / n;
        for (k, name) in labels.iter().enumerate() {
            if glyph_key(ui, vec2(w, 32.0), Glyph::Wave(k), (k == wave).then_some(lt)).on_hover_text(*name).clicked() {
                ed.set(at(p::OP_WAVE), k as f32);
            }
        }
    });
    if let Some(step) = wheel_step(ui, row.response.rect) {
        let k = (wave as i32 + step).clamp(0, labels.len() as i32 - 1);
        ed.set(at(p::OP_WAVE), k as f32);
    }

    // Wave on one LCD, envelope on another.
    let warps = wave <= W_PULSE;
    let half = (s.left_w - 6.0) * 0.5;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (pw, spread, sides) = (ed.v(at(p::OP_PW)), ed.v(at(p::OP_SPREAD)), ed.v(at(p::OP_SS_MIX)));
        let (rect, _) = ui.allocate_exact_size(vec2(half, s.display_h), Sense::hover());
        let painter = ui.painter_at(rect);
        theme::lcd_panel(&painter, rect);
        let inner = rect.shrink2(vec2(10.0, 14.0));
        painter.line_segment([inner.left_center(), inner.right_center()], stroke(1.0, lcd_grid()));
        draw_curve(
            &painter,
            inner,
            &|t| wave_at(wave, t, pw, spread, sides),
            2.0,
            theme::LCD_INK,
            1.8,
            Some(inner.center().y),
        );
        envelope(
            ui,
            ed,
            [at(p::OP_ATTACK), at(p::OP_DECAY), at(p::OP_SUSTAIN), at(p::OP_RELEASE)],
            vec2(half, s.display_h),
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
        knob_cell(ui, ed, p::matrix(i, i), "Feedback", c, true);
    });
}

// ───────────────────────────────────────────────────────────── filter page

fn filter_page(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let lt = FILTER_LIGHT;
    let c = lt.mid;
    let (lp, hp) = (ed.on(p::LP_ON), ed.on(p::HP_ON));
    let slope = ed.choice(p::LP_SLOPE);
    let mut set = None;
    switch_row(ui, |ui| {
        if widgets::key(ui, vec2(78.0, s.key_h), Legend::Text("LOW-PASS"), lp, lt, "Low-pass on / off").clicked() {
            set = Some((p::LP_ON, if lp { 0 } else { 1 }));
        }
        ui.add_enabled_ui(lp, |ui| {
            if let Some(k) = switch(ui, &["12 dB", "24 dB"], slope, lt, "Low-pass slope") {
                set = Some((p::LP_SLOPE, k));
            }
        });
        if widgets::key(ui, vec2(78.0, s.key_h), Legend::Text("HIGH-PASS"), hp, lt, "High-pass on / off").clicked() {
            set = Some((p::HP_ON, if hp { 0 } else { 1 }));
        }
    });
    if let Some((id, k)) = set {
        ed.set(id, k as f32);
    }
    filter_graph(ui, ed, vec2(s.left_w, 32.0 + s.display_h - s.cell_h + s.cell_h - 6.0));
    ui.horizontal(|ui| {
        knob_cell(ui, ed, p::CUTOFF, "Cutoff", c, lp);
        knob_cell(ui, ed, p::RESO, "Reso", c, lp);
        knob_cell(ui, ed, p::F_ENV, "Env", c, lp);
        knob_cell(ui, ed, p::KEYTRACK, "Key", c, lp);
        knob_cell(ui, ed, p::F_LFO, "LFO", LFO_LIGHT.mid, lp);
        knob_cell(ui, ed, p::HP_CUTOFF, "HP cut", c, hp);
        knob_cell(ui, ed, p::HP_RESO, "HP reso", c, hp);
    });
    ui.horizontal(|ui| {
        let env_w = s.left_w - 4.0 * (s.cell_w + 5.0);
        ui.add_enabled_ui(lp, |ui| {
            envelope(ui, ed, [p::F_ATTACK, p::F_DECAY, p::F_SUSTAIN, p::F_RELEASE], vec2(env_w, s.cell_h), "fm-f-env");
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

/// The filters' combined response on an LCD, with a draggable dot for each one
/// that is on: left/right = cutoff, up/down = resonance.
fn filter_graph(ui: &mut Ui, ed: &mut Ed, size: Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let inner = rect.shrink2(vec2(12.0, 12.0));
    let decades = 1000f32.ln();
    let fx = |f: f32| inner.left() + (f / 20.0).max(1e-3).ln() / decades * inner.width();
    let f_of = |x: f32| 20.0 * ((x - inner.left()) / inner.width() * decades).exp();
    let (top_db, bot_db) = (24.0, -48.0);
    let fy = |db: f32| inner.top() + (top_db - db.clamp(bot_db, top_db)) / (top_db - bot_db) * inner.height();
    let db_of = |y: f32| top_db - (y - inner.top()) / inner.height() * (top_db - bot_db);
    let lp24 = ed.on(p::LP_SLOPE);
    let peak_gain =
        |reso: f32, is_lp: bool| 1.0 / reso_k(reso) * if is_lp && lp24 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };

    // Dots first, so the curve drawn below already shows this frame's drag.
    let mut hot = [false; 2];
    for (k, (on, cut_id, reso_id, is_lp)) in
        [(ed.on(p::LP_ON), p::CUTOFF, p::RESO, true), (ed.on(p::HP_ON), p::HP_CUTOFF, p::HP_RESO, false)]
            .into_iter()
            .enumerate()
    {
        if !on {
            continue;
        }
        let at = pos2(fx(ed.v(cut_id)), fy(20.0 * peak_gain(ed.v(reso_id), is_lp).log10()));
        let r = ui.interact(
            Rect::from_center_size(at, vec2(22.0, 22.0)),
            Id::new(("fm-filter-dot", k, ed.ch)),
            Sense::click_and_drag(),
        );
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
                if is_lp && lp24 {
                    g *= std::f32::consts::SQRT_2;
                }
                ed.put(reso_id, (2.0 - 1.0 / g) / 1.98);
            }
        }
        hot[k] = r.hovered() || r.dragged();
        let tip = if is_lp { "Low-pass" } else { "High-pass" };
        r.on_hover_text(format!("{tip}: drag sideways for cutoff, up/down for resonance. Double-click resets."));
    }

    let painter = ui.painter_at(rect);
    theme::lcd_panel(&painter, rect);
    let small = FontId::proportional(theme::m().text_small - 1.0);
    for (f, label) in [(100.0, "100"), (1000.0, "1k"), (10_000.0, "10k")] {
        let x = fx(f);
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], stroke(1.0, lcd_grid()));
        painter.text(pos2(x + 3.0, inner.bottom()), Align2::LEFT_BOTTOM, label, small.clone(), theme::LCD_INK_DIM);
    }
    painter.line_segment([pos2(inner.left(), fy(0.0)), pos2(inner.right(), fy(0.0))], stroke(1.0, lcd_grid()));

    let (lp, hp) = (ed.on(p::LP_ON), ed.on(p::HP_ON));
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
    let ink = if lp || hp { theme::LCD_INK } else { theme::LCD_INK_DIM };
    let n = inner.width() as usize;
    let pts: Vec<_> = (0..=n)
        .map(|k| {
            let x = inner.left() + k as f32;
            pos2(x, fy(20.0 * mag(f_of(x)).max(1e-6).log10()))
        })
        .collect();
    fill_under(&painter, &pts, inner.bottom(), lcd_fill());
    painter.add(Shape::line(pts, stroke(1.8, ink)));
    if !lp && !hp {
        painter.text(inner.center(), Align2::CENTER_CENTER, "filters off", small, theme::LCD_INK_DIM);
    }
    for (k, (on, cut_id, reso_id, is_lp)) in
        [(lp, p::CUTOFF, p::RESO, true), (hp, p::HP_CUTOFF, p::HP_RESO, false)].into_iter().enumerate()
    {
        if on {
            let at = pos2(fx(ed.v(cut_id)), fy(20.0 * peak_gain(ed.v(reso_id), is_lp).log10()));
            dot(&painter, at, is_lp, hot[k]);
        }
    }
}

// ───────────────────────────────────────────────────────────── LFO page

fn lfo_page(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let lt = LFO_LIGHT;
    let c = lt.mid;
    // Which LFO the page shows (remembered per channel, like the page itself).
    let which_id = Id::new(("fm-lfo", ed.ch));
    let mut which = ui.ctx().data(|d| d.get_temp::<usize>(which_id)).unwrap_or(0);
    let [shape_id, rate_id, sync_id, retrig_id] = if which == 0 {
        [p::LFO_SHAPE, p::LFO_RATE, p::LFO_SYNC, p::LFO_RETRIG]
    } else {
        [p::LFO2_SHAPE, p::LFO2_RATE, p::LFO2_SYNC, p::LFO2_RETRIG]
    };
    let shape = ed.choice(shape_id);
    let retrig = ed.choice(retrig_id);
    let mut set = None;
    switch_row(ui, |ui| {
        if let Some(k) = switch(
            ui,
            &["LFO 1", "LFO 2"],
            which,
            lt,
            "LFO 1 also has fixed routes (vibrato, cutoff, PWM); both feed the MOD slots.",
        ) {
            which = k;
        }
        if let Some(k) = switch(ui, &["FREE", "RETRIG"], retrig, lt, "Retrigger: the LFO restarts on every note.") {
            set = Some((retrig_id, k));
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(which_id, which));
    let labels = FM_PARAMS[shape_id].labels;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for (k, name) in labels.iter().enumerate() {
            if glyph_key(ui, vec2(58.0, 32.0), Glyph::Lfo(k), (k == shape).then_some(lt)).on_hover_text(*name).clicked()
            {
                set = Some((shape_id, k));
            }
        }
    });
    if let Some((id, k)) = set {
        ed.set(id, k as f32);
    }
    // One second of the LFO, so the rate is visible (a synced one at the song's tempo).
    let beats = lyra_fluttersine::SYNC_BEATS[ed.choice(sync_id).min(lyra_fluttersine::SYNC_BEATS.len() - 1)];
    let synced = beats > 0.0;
    let rate = if synced { ed.p.bpm as f32 / 60.0 / beats } else { ed.v(rate_id) };
    let (rect, _) = ui.allocate_exact_size(vec2(s.left_w, s.display_h - 12.0), Sense::hover());
    let painter = ui.painter_at(rect);
    theme::lcd_panel(&painter, rect);
    let inner = rect.shrink2(vec2(10.0, 14.0));
    painter.line_segment([inner.left_center(), inner.right_center()], stroke(1.0, lcd_grid()));
    draw_curve(&painter, inner, &|t| lfo_at(shape, t), rate, theme::LCD_INK, 1.8, Some(inner.center().y));
    let caption = if synced { format!("1 second at {:.0} BPM", ed.p.bpm) } else { "1 second".into() };
    painter.text(
        rect.right_bottom() - vec2(9.0, 3.0),
        Align2::RIGHT_BOTTOM,
        caption,
        FontId::proportional(theme::m().text_small - 1.0),
        theme::LCD_INK_DIM,
    );

    ui.horizontal(|ui| {
        knob_cell(ui, ed, rate_id, "Rate", c, !synced);
        knob_cell(ui, ed, sync_id, "Sync", c, true);
        if which == 0 {
            divider(ui, s.cell_h);
            knob_cell(ui, ed, p::VIBRATO, "Vibrato", c, true);
            knob_cell(ui, ed, p::F_LFO, "Cutoff", FILTER_LIGHT.mid, ed.on(p::LP_ON));
        }
    });
    if which == 0 {
        ui.horizontal(|ui| {
            for (i, lt) in OP_LIGHTS.iter().enumerate() {
                let warps = ed.choice(op(i, p::OP_WAVE)) <= W_PULSE;
                knob_cell(ui, ed, op(i, p::OP_PWM), &format!("PWM {}", i + 1), lt.mid, warps)
            }
        });
    }
}

// ───────────────────────────────────────────────────────────── MOD page

/// Two mod envelopes on top; below them the six slots, each a source, a
/// destination and a bipolar amount. A slot only sounds when all three are set.
fn mod_page(ui: &mut Ui, ed: &mut Ed) {
    use p::{env, mod_amt, mod_dest, mod_src, ENV_ATTACK, ENV_DECAY, ENV_RELEASE, ENV_SUSTAIN};
    let s = sizes();
    let c = MOD_LIGHT.mid;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for e in 0..2 {
            if e == 1 {
                divider(ui, s.small_cell_h);
            }
            for (k, label) in
                [(ENV_ATTACK, "Attack"), (ENV_DECAY, "Decay"), (ENV_SUSTAIN, "Sustain"), (ENV_RELEASE, "Release")]
            {
                let name = if k == ENV_ATTACK { format!("Env {} A", e + 1) } else { label[..1].to_string() };
                small_cell(ui, ed, env(e, k), &name, c, true);
            }
        }
    });
    ui.add_space(4.0);
    for k in 0..p::MOD_SLOTS {
        let live = ed.choice(mod_src(k)) != 0 && ed.choice(mod_dest(k)) != 0;
        ui.allocate_ui_with_layout(vec2(s.left_w, s.slot_h), Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let ink = if live { theme::TEXT } else { theme::TEXT_DIM };
            ui.add_sized(vec2(14.0, s.key_h), egui::Label::new(RichText::new(format!("{}", k + 1)).small().color(ink)));
            choice_well(ui, ed, mod_src(k), 116.0, "Source: what moves this slot");
            // An arrow, drawn (the UI font has no "→").
            let (ar, _) = ui.allocate_exact_size(vec2(14.0, s.key_h), Sense::hover());
            let (y, x0, x1) = (ar.center().y, ar.left() + 1.0, ar.right() - 1.0);
            let arrow = stroke(1.4, theme::TEXT_DIM);
            ui.painter().line_segment([pos2(x0, y), pos2(x1, y)], arrow);
            ui.painter().add(Shape::line(vec![pos2(x1 - 4.0, y - 3.5), pos2(x1, y), pos2(x1 - 4.0, y + 3.5)], arrow));
            choice_well(ui, ed, mod_dest(k), 150.0, "Destination: what it moves");
            let info = &FM_PARAMS[mod_amt(k)];
            let mut v = ed.v(mod_amt(k));
            let r = widgets::knob(ui, &mut v, info, theme::m().knob_small, if live { c } else { theme::TEXT_DIM });
            ed.track(&r, mod_amt(k), v);
            r.on_hover_text("Amount: how far, and which way (double-click: 0)");
            ui.label(RichText::new(info.format(ed.v(mod_amt(k)))).small().color(ink));
        });
    }
}

/// A dropdown well for a choice parameter: click for the list, wheel to step.
fn choice_well(ui: &mut Ui, ed: &mut Ed, id: usize, width: f32, hint: &str) {
    let s = sizes();
    let m = theme::m();
    let info = &FM_PARAMS[id];
    let n = info.labels.len();
    let at = ed.choice(id).min(n - 1);
    let (r, resp) = ui.allocate_exact_size(vec2(width, s.key_h), Sense::click());
    let painter = ui.painter();
    theme::well(painter, r);
    let ink = if at == 0 { theme::TEXT_DIM } else { theme::TEXT };
    painter.text(
        r.left_center() + vec2(8.0, 0.0),
        Align2::LEFT_CENTER,
        info.labels[at],
        FontId::proportional(m.text_small + 1.0),
        ink,
    );
    let c = r.right_center() - vec2(10.0, 1.0);
    let chevron = vec![c + vec2(-3.0, -1.2), c + vec2(0.0, 1.8), c + vec2(3.0, -1.2)];
    painter.add(Shape::line(chevron, stroke(1.4, theme::TEXT_DIM)));
    if let Some(step) = wheel_step(ui, r) {
        ed.set(id, (at as i32 + step).clamp(0, n as i32 - 1) as f32);
    }
    let popup = ui.make_persistent_id(("fm-choice", ed.ch, id));
    if resp.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup));
    }
    let resp = resp.on_hover_text(hint);
    let mut pick = None;
    egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(width);
        for (k, label) in info.labels.iter().enumerate() {
            if ui.selectable_label(k == at, *label).clicked() {
                pick = Some(k);
            }
        }
    });
    if let Some(k) = pick {
        ed.set(id, k as f32);
    }
}

/// A smaller knob cell, for rows that need more than seven knobs.
fn small_cell(ui: &mut Ui, ed: &mut Ed, id: usize, label: &str, color: Color32, enabled: bool) {
    let s = sizes();
    let m = theme::m();
    let info = &FM_PARAMS[id];
    ui.allocate_ui(vec2(s.small_cell_w, s.small_cell_h), |ui| {
        ui.add_enabled_ui(enabled, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.add_space(2.0);
                let mut v = ed.v(id);
                let r = widgets::knob(ui, &mut v, info, m.knob_small, color);
                if enabled {
                    ed.track(&r, id, v);
                }
                r.on_hover_text(info.name);
                ui.add_space(2.0);
                ui.label(RichText::new(label).small().color(theme::TEXT_DIM));
                ui.label(RichText::new(info.format(ed.v(id))).small().color(theme::TEXT));
            });
        });
    });
}

// ───────────────────────────────────────────────────────────── matrix + voice

fn matrix(ui: &mut Ui, ed: &mut Ed, page: &mut Page, used: &[bool; OPS]) {
    let s = sizes();
    let small = FontId::proportional(theme::m().text_small);
    let origin = ui.cursor().min;
    // Square cells, packed edge to edge.
    // Columns: the 6×6 matrix, a gap, then OUT and PAN side by side.
    let col_x = |col: usize| origin.x + s.mlabel_w + col as f32 * s.mcell.x + if col >= OPS { OUT_GAP } else { 0.0 };
    let row_y = |row: usize| origin.y + 18.0 + row as f32 * s.mcell.y;
    let (area, _) = ui.allocate_exact_size(vec2(s.right_w, s.matrix_h), Sense::hover());
    ui.interact(Rect::from_min_size(area.min, vec2(s.mlabel_w - 4.0, 16.0)), Id::new("fm-matrix-help"), Sense::hover())
        .on_hover_text(
            "FM matrix. Each row is an operator sending into each column: turn a knob up to modulate.\n\
             The diagonal is feedback (an operator modulating itself). OUT is how loud it is,\n\
             PAN where it sits, left to right.",
        );
    // Column heads
    for col in 0..=OPS + 1 {
        let text = match col {
            c if c == OPS => "OUT".to_string(),
            c if c == OPS + 1 => "PAN".to_string(),
            c => (c + 1).to_string(),
        };
        let at = pos2(col_x(col) + s.mcell.x * 0.5, origin.y + 8.0);
        ui.painter().text(at, Align2::CENTER_CENTER, text, small.clone(), theme::TEXT_DIM);
    }
    for row in 0..OPS {
        let lt = OP_LIGHTS[row];
        // Row name: the operator's LED (lit = it can be heard) and its number;
        // the open operator's row name sits on a brighter plate.
        let name_r = Rect::from_min_size(pos2(origin.x, row_y(row)), vec2(s.mlabel_w - 3.0, s.mcell.y));
        let resp = ui.interact(name_r, Id::new(("fm-matrix-row", row)), Sense::click());
        let open = *page == Page::Op(row);
        let painter = ui.painter();
        if open || resp.hovered() {
            painter.rect_filled(
                name_r,
                cr(2),
                if open { theme::PANEL_HI } else { theme::mix(theme::PANEL, theme::PANEL_HI, 0.5) },
            );
        }
        if open {
            painter.rect_filled(Rect::from_min_size(name_r.min, vec2(2.0, name_r.height())), cr(0), lt.mid);
        }
        let led_c = pos2(name_r.left() + 9.0, name_r.center().y);
        theme::led(painter, led_c, 10.0, used[row].then_some(lt), false);
        let ink = if open || resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
        painter.text(led_c + vec2(8.0, 0.0), Align2::LEFT_CENTER, (row + 1).to_string(), small.clone(), ink);
        if resp.on_hover_text(format!("Operator {}: click to open it", row + 1)).clicked() {
            *page = Page::Op(row);
        }
        for col in 0..=OPS + 1 {
            let id = match col {
                c if c == OPS => p::out(row),
                c if c == OPS + 1 => p::pan(row),
                c => p::matrix(row, c),
            };
            let cell = Rect::from_min_size(pos2(col_x(col), row_y(row)), s.mcell);
            if col == row {
                // Feedback: the diagonal sits in a faint, square recess.
                ui.painter().rect_filled(cell, cr(0), theme::mix(theme::PANEL, theme::BG, 0.3));
            }
            let knob = Rect::from_center_size(cell.center(), Vec2::splat(theme::m().knob_small));
            let mut v = ed.v(id);
            let r = widgets::knob_at(ui, Id::new(("fm-matrix-knob", id)), knob, &mut v, &FM_PARAMS[id], lt.mid);
            ed.track(&r, id, v);
        }
    }
}

fn voice(ui: &mut Ui, ed: &mut Ed) {
    let c = VOICE_LIGHT.mid;
    let (mode, glide_mode) = (ed.choice(p::MODE), ed.choice(p::GLIDE_MODE));
    let glides = ed.v(p::GLIDE) > 0.0;
    let mut set = None;
    let saws = (0..OPS).any(|i| ed.choice(op(i, p::OP_WAVE)) == W_SSAW);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        small_cell(ui, ed, p::VOLUME, "Volume", c, true);
        small_cell(ui, ed, p::AMP_VEL, "Velocity", c, true);
        small_cell(ui, ed, p::WIDTH, "Width", c, saws);
        small_cell(ui, ed, p::GLIDE, "Glide", c, true);
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            if let Some(k) =
                switch(ui, &["POLY", "MONO"], mode, VOICE_LIGHT, "Mono: one note at a time, with legato and glide.")
            {
                set = Some((p::MODE, k));
            }
            ui.add_enabled_ui(glides, |ui| {
                let hint = "Glide only between overlapping notes (legato), or on every note.";
                if let Some(k) = switch(ui, &["LEGATO", "ALWAYS"], glide_mode, VOICE_LIGHT, hint) {
                    set = Some((p::GLIDE_MODE, k));
                }
            });
        });
    });
    if let Some((id, k)) = set {
        ed.set(id, k as f32);
    }
}

// ───────────────────────────────────────────────────────────── shared pieces

/// A knob with its name and value under it; the arc in `color`. Disabled = faded and inert.
fn knob_cell(ui: &mut Ui, ed: &mut Ed, id: usize, label: &str, color: Color32, enabled: bool) {
    let s = sizes();
    let m = theme::m();
    let info = &FM_PARAMS[id];
    ui.allocate_ui(vec2(s.cell_w, s.cell_h), |ui| {
        ui.add_enabled_ui(enabled, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.add_space(2.0);
                let mut v = ed.v(id);
                let r = widgets::knob(ui, &mut v, info, m.knob_large, color);
                if enabled {
                    ed.track(&r, id, v);
                }
                ui.add_space(3.0);
                ui.label(RichText::new(label).small().color(theme::TEXT_DIM));
                ui.label(RichText::new(format_value(id, ed.v(id))).small().color(theme::TEXT));
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

/// A silicone key with a little curve printed on it; lit when `light` is `Some`.
fn glyph_key(ui: &mut Ui, size: Vec2, glyph: Glyph, light: Option<Light>) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let pressed = resp.is_pointer_button_down_on();
    theme::key(painter, rect, light, resp.hovered() && !pressed, if pressed { 0.9 } else { 1.0 });
    let face = theme::key_face(rect);
    draw_glyph(painter, face.shrink2(vec2(5.0, 6.0)), glyph, theme::key_ink(light.is_some()), 1.5);
    resp
}

/// A thin seam between groups of knobs.
fn divider(ui: &mut Ui, h: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(6.0, h), Sense::hover());
    let x = r.center().x;
    ui.painter().rect_filled(
        Rect::from_min_max(pos2(x - 1.0, r.top() + 8.0), pos2(x + 1.0, r.bottom() - 8.0)),
        cr(0),
        theme::BG,
    );
}

/// Faint LCD grid lines.
fn lcd_grid() -> Color32 {
    theme::mix(theme::LCD, theme::LCD_INK, 0.13)
}
/// The shading under a curve on an LCD.
fn lcd_fill() -> Color32 {
    theme::LCD_INK.gamma_multiply(0.09)
}

/// A draggable point on an LCD: dark ink; `solid` = filled, else a ring.
fn dot(painter: &egui::Painter, at: egui::Pos2, solid: bool, hot: bool) {
    if hot {
        painter.circle_stroke(at, 8.5, stroke(1.2, theme::LCD_INK_DIM));
    }
    if solid {
        painter.circle_filled(at, 4.5, theme::LCD_INK);
    } else {
        painter.circle_filled(at, 4.5, theme::LCD);
        painter.circle_stroke(at, 4.0, stroke(1.8, theme::LCD_INK));
    }
}

/// Draw `f(t)` (t in cycles, output -1..1) over `cycles` cycles; with `fill_to`,
/// shade between the curve and that height.
fn draw_curve(
    painter: &egui::Painter,
    rect: Rect,
    f: &dyn Fn(f32) -> f32,
    cycles: f32,
    color: Color32,
    width: f32,
    fill_to: Option<f32>,
) {
    let n = (rect.width() * 1.5).max(16.0) as usize;
    let mid = rect.center().y;
    let pts: Vec<_> = (0..=n)
        .map(|k| {
            let x = k as f32 / n as f32;
            pos2(rect.left() + x * rect.width(), mid - f(x * cycles).clamp(-1.0, 1.0) * rect.height() * 0.5)
        })
        .collect();
    if let Some(base) = fill_to {
        fill_under(painter, &pts, base, lcd_fill());
    }
    painter.add(Shape::line(pts, stroke(width, color)));
}

/// Shade between a polyline (left to right) and a baseline, as one mesh.
fn fill_under(painter: &egui::Painter, pts: &[egui::Pos2], base: f32, color: Color32) {
    let mut mesh = egui::Mesh::default();
    for q in pts {
        mesh.colored_vertex(*q, color);
        mesh.colored_vertex(pos2(q.x, base), color);
    }
    for k in 0..pts.len().saturating_sub(1) as u32 {
        let (a, b) = (2 * k, 2 * k + 2);
        mesh.add_triangle(a, a + 1, b + 1);
        mesh.add_triangle(a, b + 1, b);
    }
    painter.add(Shape::mesh(mesh));
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

/// An ADSR on an LCD that you can drag: the attack peak (time), the
/// decay/sustain corner (time and level) and the release end (time). Each stage
/// gets its own lane, so times sit on the same curve as their knobs.
/// Double-click a dot to reset what it moves.
fn envelope(ui: &mut Ui, ed: &mut Ed, ids: [usize; 4], size: Vec2, salt: impl std::hash::Hash) {
    let [a_id, d_id, s_id, r_id] = ids;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let enabled = ui.is_enabled();
    let inner = rect.shrink2(vec2(12.0, 12.0));
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

    let (xa, xd, xs, xr, s) = points(ed);
    let dots = [(0, pos2(xa, y(1.0))), (1, pos2(xd, y(s))), (2, pos2(xr, y(0.0)))];
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
    theme::lcd_panel(&painter, rect);
    let ink = if enabled { theme::LCD_INK } else { theme::LCD_INK_DIM };
    for x in [inner.left() + lane, inner.left() + 2.0 * lane, xs] {
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], stroke(1.0, lcd_grid()));
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
    fill_under(&painter, &pts, inner.bottom(), lcd_fill());
    painter.add(Shape::line(pts, stroke(1.8, ink)));
    if enabled {
        for (k, at) in [(0, pos2(xa, y(1.0))), (1, pos2(xd, y(s))), (2, pos2(xr, y(0.0)))] {
            dot(&painter, at, true, hot == Some(k));
        }
    }
}
