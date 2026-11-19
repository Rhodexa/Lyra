//! The hand-made editor for **Chrysalis** (`lyra.chrysalis`), the wavetable synth.
//!
//! ```text
//!  ┌ ◀ [ preset ▾ ] ▶ ─────────────────────────────┐ ┌ voice ────────────┐
//!  ├OSC 1┼OSC 2┼SUB┼FILTER┼ENV┼LFO┼MOD─────────────┤ │                   │
//!  ┌ the open page ─────────────────────────────────┐ ├ macros ───────────┤
//!  │ switches · a big LCD · knobs                   │ │ 4 big knobs       │
//!  └────────────────────────────────────────────────┘ └ (wordmark) ───────┘
//! ```
//!
//! Same plate language as Fluttersine's editor (and its drawing helpers):
//! fixed plates a seam apart, colour only as light, dark ink on LCDs.
//!
//! * OSC: the table as stacked frames; the bold one is what plays (position
//!   and warp applied). Drag it up/down or wheel over it to move through it.
//! * FILTER: both filters' combined response; drag a dot for cutoff (sideways)
//!   and resonance (up/down).
//! * ENV: dots for times and sustain, rings for the curves.
//! * LFO: draw the shape. Drag points (they snap to the grid; Alt = free),
//!   drag the rings between them to bend, double-click empty space to add a
//!   point, double-click or right-click a point to remove it.
//! * MOD: 16 slots, 8 at a time: source → destination × amount, "via" a
//!   second source, and Bi to centre the source.
//!
//! Like every view, this only edits `channel.params` (and `channel.sample`
//! for an imported table). The pictures call `lyra_chrysalis` itself, so
//! they show exactly what plays.

use crate::fm_editor::{dot, fill_under, lcd_fill, lcd_grid, plate, switch, wheel_step, PAD};
use crate::rack::tracked;
use crate::theme::{self, cr, stroke, Light};
use crate::widgets::{self, Icon, Legend};
use egui::{pos2, vec2, Align, Align2, Color32, FontId, Id, Layout, Pos2, Rect, Response, RichText, Sense, Shape, Ui};
use lyra_chrysalis::curve::{self, Point, MAX_POINTS, SHAPES};
use lyra_chrysalis::params::{ft, warp};
use lyra_chrysalis::table::{self, Table, IMPORTED};
use lyra_chrysalis::{p, PARAMS, PRESETS};
use lyra_model::{History, Project};
use std::sync::Arc;

const OSC_LIGHTS: [Light; 2] = [theme::L_PINK, theme::L_BLUE];
const SUB_LIGHT: Light = theme::L_PEACH;
const FILTER_LIGHT: Light = theme::L_CYAN;
const ENV_LIGHT: Light = theme::L_AMBER;
const LFO_LIGHT: Light = theme::L_LIME;
const MOD_LIGHT: Light = theme::L_LAVENDER;
const VOICE_LIGHT: Light = theme::L_MINT;

/// The level the pictures draw from: 128 harmonics, smooth but still sharp.
const PICTURE_LEVEL: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Osc(usize),
    Sub,
    Filter,
    Env,
    Lfo,
    Mod,
}

impl Page {
    const COUNT: usize = 7;
    fn index(self) -> usize {
        match self {
            Page::Osc(i) => i,
            Page::Sub => 2,
            Page::Filter => 3,
            Page::Env => 4,
            Page::Lfo => 5,
            Page::Mod => 6,
        }
    }
    fn from_index(i: usize) -> Page {
        match i {
            0 | 1 => Page::Osc(i),
            2 => Page::Sub,
            3 => Page::Filter,
            4 => Page::Env,
            5 => Page::Lfo,
            _ => Page::Mod,
        }
    }
    fn light(self) -> Light {
        match self {
            Page::Osc(i) => OSC_LIGHTS[i],
            Page::Sub => SUB_LIGHT,
            Page::Filter => FILTER_LIGHT,
            Page::Env => ENV_LIGHT,
            Page::Lfo => LFO_LIGHT,
            Page::Mod => MOD_LIGHT,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Page::Osc(0) => "OSC 1",
            Page::Osc(_) => "OSC 2",
            Page::Sub => "SUB",
            Page::Filter => "FILTER",
            Page::Env => "ENV",
            Page::Lfo => "LFO",
            Page::Mod => "MOD",
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

/// Every size, from the theme's metrics (F12 still drives them). Plates have
/// fixed sizes, so switching pages never resizes the window.
struct Sizes {
    cell_w: f32,
    cell_h: f32,
    key_h: f32,
    left_w: f32,
    right_w: f32,
    preset_h: f32,
    tab_h: f32,
    page_h: f32,
    display_h: f32,
    small_cell_w: f32,
    small_cell_h: f32,
    slot_h: f32,
    macros_h: f32,
}

fn sizes() -> Sizes {
    let m = theme::m();
    let cell_w = m.knob_large + 22.0;
    let cell_h = m.knob_large + 2.0 * m.text_small + 14.0;
    let key_h = m.control_h;
    let gap = 6.0;
    let display_h = 176.0;
    let small_cell_h = m.knob_small + 2.0 * m.text_small + 12.0;
    Sizes {
        cell_w,
        cell_h,
        key_h,
        left_w: 9.0 * cell_w + 8.0 * 5.0,
        right_w: 4.0 * cell_w + 3.0 * 6.0 + 24.0,
        preset_h: m.control_h + 14.0,
        tab_h: m.control_h + 8.0,
        page_h: key_h + gap + display_h + gap + 2.0 * cell_h + gap,
        display_h,
        small_cell_w: m.knob_small + 24.0,
        small_cell_h,
        slot_h: key_h + 6.0,
        macros_h: 16.0 + cell_h + 8.0,
    }
}

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, ch: usize) {
    let mut ed = Ed { p, h, ch };
    let s = sizes();
    let seam = theme::SEAM;
    let page_id = Id::new(("chrys-page", ch));
    let stored = ui.ctx().data(|d| d.get_temp::<usize>(page_id)).unwrap_or(0);
    let mut page = Page::from_index(stored);

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
    let voice_r = Rect::from_min_max(pos2(x1, y0), pos2(x1 + rw, tabs_r.bottom()));
    let macros_r = Rect::from_min_size(pos2(x1, page_r.top()), vec2(rw, outer(s.macros_h)));
    let frame_left = Rect::from_min_max(rack.min, pos2(x1, rack.bottom()));
    let frame_right = Rect::from_min_max(pos2(x1 - seam, rack.top()), pos2(rack.right(), macros_r.bottom() + seam));
    ui.painter().rect_filled(frame_left, cr(3), theme::BG);
    ui.painter().rect_filled(frame_right, cr(3), theme::BG);
    let logo_r = Rect::from_min_max(pos2(x1, macros_r.bottom() + seam), pos2(rack.right(), rack.bottom()));

    plate(ui, preset_r, "chrys-preset-plate", |ui| preset_row(ui, &mut ed));
    tabs(ui, tabs_r, &mut page);
    plate(ui, page_r, ("chrys-page-plate", page.index()), |ui| match page {
        Page::Osc(o) => osc_page(ui, &mut ed, o),
        Page::Sub => sub_page(ui, &mut ed),
        Page::Filter => filter_page(ui, &mut ed),
        Page::Env => env_page(ui, &mut ed),
        Page::Lfo => lfo_page(ui, &mut ed),
        Page::Mod => mod_page(ui, &mut ed),
    });
    plate(ui, voice_r, "chrys-voice-plate", |ui| voice(ui, &mut ed));
    plate(ui, macros_r, "chrys-macros-plate", |ui| macros(ui, &mut ed));
    wordmark(ui, logo_r);

    ui.ctx().data_mut(|d| d.insert_temp(page_id, page.index()));
}

/// The user's emblem, straight on the window's surface (no plate, no frame).
fn wordmark(ui: &mut Ui, r: Rect) {
    let (tex, [w, h]) = theme::chrysalis_emblem(ui.ctx());
    let room = r.shrink(2.0 * PAD);
    let scale = (room.width() / w as f32).min(room.height() / h as f32);
    let img = Rect::from_center_size(room.center(), vec2(w as f32, h as f32) * scale);
    ui.painter().image(tex, img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    ui.interact(r, Id::new("chrys-emblem"), Sense::hover()).on_hover_text("Chrysalis · wavetable synthesizer");
}

// ───────────────────────────────────────────────────────────── presets

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

/// `◀ [ preset ▾ ] ▶`: click for the list, wheel over it to step.
fn preset_row(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
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
        chevron(painter, r.right_center() - vec2(13.0, 1.5), 3.5);
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
        if key(ui, Icon::Next, "Next preset").clicked() {
            pick = Some((at + 1) % n);
        }
    });
    if let Some(k) = pick {
        ed.h.checkpoint(ed.p);
        ed.p.channels[ed.ch].params = preset_values(k);
        ui.ctx().data_mut(|d| d.insert_temp(last_id, k));
    }
}

fn chevron(painter: &egui::Painter, c: Pos2, w: f32) {
    let pts = vec![c + vec2(-w, -w * 0.43), c + vec2(0.0, w * 0.57), c + vec2(w, -w * 0.43)];
    painter.add(Shape::line(pts, stroke(1.5, theme::TEXT_DIM)));
}

// ───────────────────────────────────────────────────────────── tabs

fn tabs(ui: &mut Ui, r: Rect, page: &mut Page) {
    let seam = theme::SEAM;
    let w = (r.width() - seam * (Page::COUNT - 1) as f32) / Page::COUNT as f32;
    let font = FontId::proportional(theme::m().text_small + 1.0);
    for k in 0..Page::COUNT {
        let pg = Page::from_index(k);
        let b = Rect::from_min_size(pos2(r.left() + k as f32 * (w + seam), r.top()), vec2(w, r.height()));
        let resp = ui.interact(b, Id::new(("chrys-tab", k)), Sense::click());
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

// ───────────────────────────────────────────────────────────── shared pieces

/// A page's top line of switches and wells.
fn switch_row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let s = sizes();
    ui.allocate_ui_with_layout(vec2(s.left_w, s.key_h), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        add(ui);
    });
}

/// A lit-when-on key with a word on it, for on/off parameters.
fn toggle_key(ui: &mut Ui, ed: &mut Ed, id: usize, word: &str, light: Light, hint: &str) {
    let s = sizes();
    let w = word.len() as f32 * 7.0 + 22.0;
    if widgets::key(ui, vec2(w, s.key_h), Legend::Text(word), ed.on(id), light, hint).clicked() {
        ed.toggle(id);
    }
}

/// A knob with its name and value under it. Disabled = faded and inert.
fn knob_cell(ui: &mut Ui, ed: &mut Ed, id: usize, label: &str, color: Color32, enabled: bool) {
    let s = sizes();
    let m = theme::m();
    let info = &PARAMS[id];
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
                r.on_hover_text(info.name);
                ui.add_space(3.0);
                ui.label(RichText::new(label).small().color(theme::TEXT_DIM));
                ui.label(RichText::new(format_value(id, ed.v(id))).small().color(theme::TEXT));
            });
        });
    });
}

/// The smaller cell (voice plate).
fn small_cell(ui: &mut Ui, ed: &mut Ed, id: usize, label: &str, color: Color32, enabled: bool) {
    let s = sizes();
    let m = theme::m();
    let info = &PARAMS[id];
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
                ui.label(RichText::new(format_value(id, ed.v(id))).small().color(theme::TEXT));
            });
        });
    });
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

/// A dropdown well for a choice parameter: click for the list, wheel to step.
fn choice_well(ui: &mut Ui, ed: &mut Ed, id: usize, width: f32, hint: &str) {
    choice_well_order(ui, ed, id, width, hint, None);
}

/// …listing the choices in `order` (indices), when given.
fn choice_well_order(ui: &mut Ui, ed: &mut Ed, id: usize, width: f32, hint: &str, order: Option<&[usize]>) {
    let s = sizes();
    let m = theme::m();
    let info = &PARAMS[id];
    let n = info.labels.len();
    let at = ed.choice(id).min(n - 1);
    let all: Vec<usize> = (0..n).collect();
    let order = order.unwrap_or(&all);
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
    chevron(painter, r.right_center() - vec2(10.0, 1.0), 3.0);
    if let Some(step) = wheel_step(ui, r) {
        let pos = order.iter().position(|&k| k == at).unwrap_or(0) as i32;
        let next = order[(pos + step).clamp(0, order.len() as i32 - 1) as usize];
        ed.set(id, next as f32);
    }
    let popup = ui.make_persistent_id(("chrys-choice", ed.ch, id));
    if resp.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup));
    }
    let resp = resp.on_hover_text(hint);
    let mut pick = None;
    egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(width);
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for &k in order {
                if ui.selectable_label(k == at, info.labels[k]).clicked() {
                    pick = Some(k);
                }
            }
        });
    });
    if let Some(k) = pick {
        ed.set(id, k as f32);
    }
}

/// A double-click on `r`, timed by Lyra itself (egui only knows on the frame
/// the button comes back up, so quick clicks slip through). `what` names the target.
fn dbl(ui: &Ui, r: &Response, what: impl std::hash::Hash) -> bool {
    r.clicked() && crate::browser::double_click(ui, "chrys-double-click", Id::new(what))
}

/// A thin seam between groups of knobs.
fn divider(ui: &mut Ui, h: f32) {
    crate::fm_editor::divider(ui, h);
}

/// A drag on a picture moves `id` like a knob would (up = more); wheel nudges,
/// double-click resets. `span` = points of travel for the whole range.
fn drag_param(ui: &Ui, ed: &mut Ed, r: &Response, id: usize, span: f32, wheel_rect: Rect) {
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
    if let Some(step) = wheel_step(ui, wheel_rect) {
        ed.h.checkpoint(ed.p);
        let n = info.to_normalized(ed.v(id)) - step as f32 * 0.02;
        ed.put(id, info.from_normalized(n));
    }
}

// ───────────────────────────────────────────────────────────── OSC page

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

fn osc_page(ui: &mut Ui, ed: &mut Ed, o: usize) {
    let s = sizes();
    let lt = OSC_LIGHTS[o];
    let c = lt.mid;
    let at = |k: usize| p::osc(o, k);
    let on = ed.on(at(p::OSC_ON));
    let is_import = ed.choice(at(p::TABLE)) == IMPORTED;
    switch_row(ui, |ui| {
        toggle_key(ui, ed, at(p::OSC_ON), "ON", lt, "Oscillator on / off");
        choice_well(ui, ed, at(p::TABLE), 120.0, "Wavetable: scroll to flip through");
        let has = ed.p.channels[ed.ch].sample.is_some();
        let load = widgets::key(
            ui,
            vec2(58.0, s.key_h),
            Legend::Text("LOAD"),
            is_import && has,
            lt,
            "Import a WAV as a wavetable (cut into frames of 2048 samples). Both oscillators can play it.",
        );
        if load.clicked() {
            if let Some(path) = rfd::FileDialog::new().add_filter("Audio", &["wav", "flac"]).pick_file() {
                ed.h.checkpoint(ed.p);
                ed.p.channels[ed.ch].sample = Some(crate::samples::name_for(&path));
                ed.put(at(p::TABLE), IMPORTED as f32);
            }
        }
        if is_import {
            let name =
                ed.p.channels[ed.ch].sample.as_deref().map_or("no WAV loaded".into(), crate::samples::display_name);
            ui.label(RichText::new(name).small().color(theme::TEXT_DIM));
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            choice_well(ui, ed, at(p::WARP_MODE), 104.0, "Warp: how the phase is bent before the table is read");
        });
    });
    ui.add_enabled_ui(on, |ui| table_view(ui, ed, o, vec2(s.left_w, s.display_h)));
    let warps = ed.choice(at(p::WARP_MODE)) != 0;
    let voices = ed.choice(at(p::UNISON)) > 0;
    ui.add_enabled_ui(on, |ui| {
        ui.horizontal(|ui| {
            knob_cell(ui, ed, at(p::POS), "Position", c, true);
            knob_cell(ui, ed, at(p::WARP), "Warp", c, warps);
            divider(ui, s.cell_h);
            knob_cell(ui, ed, at(p::OCTAVE), "Octave", c, true);
            knob_cell(ui, ed, at(p::SEMI), "Semi", c, true);
            knob_cell(ui, ed, at(p::FINE), "Fine", c, true);
            divider(ui, s.cell_h);
            knob_cell(ui, ed, at(p::LEVEL), "Level", c, true);
            knob_cell(ui, ed, at(p::PAN), "Pan", c, true);
        });
        ui.horizontal(|ui| {
            knob_cell(ui, ed, at(p::UNISON), "Unison", c, true);
            knob_cell(ui, ed, at(p::DETUNE), "Detune", c, voices);
            knob_cell(ui, ed, at(p::BLEND), "Blend", c, voices);
            knob_cell(ui, ed, at(p::SPREAD), "Spread", c, voices);
            divider(ui, s.cell_h);
            knob_cell(ui, ed, at(p::PHASE), "Phase", c, true);
            knob_cell(ui, ed, at(p::RAND), "Random", c, true);
        });
    });
}

/// The table as stacked frames, back to front; the bold one is what plays.
fn table_view(ui: &mut Ui, ed: &mut Ed, o: usize, size: egui::Vec2) {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let enabled = ui.is_enabled();
    if enabled {
        drag_param(ui, ed, &resp, p::osc(o, p::POS), 220.0, rect);
        resp.clone()
            .on_hover_text("Drag up/down or scroll to move through the table (double-click: back to the start)");
    }
    let t = table_of(ui, ed, o);
    let frames = t.frames();
    let painter = ui.painter_at(rect);
    theme::lcd_panel(&painter, rect);
    let inner = rect.shrink2(vec2(16.0, 14.0));
    let depth = vec2(inner.width() * 0.2, -inner.height() * 0.36);
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
    // Back to front, at most 28 lines.
    let shown = frames.min(28);
    for r in (0..shown).rev() {
        let f = if shown > 1 { r * (frames - 1) / (shown - 1) } else { 0 };
        let d = if frames > 1 { f as f32 / (frames - 1) as f32 } else { 0.0 };
        let ink = theme::mix(theme::LCD_INK, theme::LCD, 0.62 + 0.22 * d);
        painter.add(Shape::line(curve(f as f32, d, 0, 0.0), stroke(1.0, ink)));
    }
    // What plays: the position between frames, with the warp.
    let pos = ed.v(p::osc(o, p::POS));
    let fpos = pos * (frames - 1) as f32;
    let d = if frames > 1 { pos } else { 0.0 };
    let mode = ed.choice(p::osc(o, p::WARP_MODE)) as u8;
    let w = ed.v(p::osc(o, p::WARP));
    let pts = curve(fpos, d, if mode == warp::FM { 0 } else { mode }, w);
    let (_, base) = place(d);
    fill_under(&painter, &pts, base, lcd_fill());
    let ink = if enabled { theme::LCD_INK } else { theme::LCD_INK_DIM };
    painter.add(Shape::line(pts, stroke(2.0, ink)));
    let small = FontId::proportional(theme::m().text_small - 1.0);
    let caption =
        if frames > 1 { format!("frame {:.0} of {frames}", fpos.round() + 1.0) } else { "1 frame".to_string() };
    painter.text(rect.right_bottom() - vec2(9.0, 4.0), Align2::RIGHT_BOTTOM, caption, small, theme::LCD_INK_DIM);
}

// ───────────────────────────────────────────────────────────── SUB page

fn sub_page(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let c = SUB_LIGHT.mid;
    let mut set = None;
    switch_row(ui, |ui| {
        toggle_key(ui, ed, p::SUB_ON, "SUB", SUB_LIGHT, "Sub oscillator on / off");
        if let Some(k) = switch(ui, &["SINE", "SQUARE"], ed.choice(p::SUB_SHAPE), SUB_LIGHT, "Sub shape") {
            set = Some((p::SUB_SHAPE, k));
        }
        if let Some(k) = switch(ui, &["−1 OCT", "−2 OCT"], ed.choice(p::SUB_OCT), SUB_LIGHT, "Below the note by") {
            set = Some((p::SUB_OCT, k));
        }
        ui.add_space(12.0);
        toggle_key(ui, ed, p::NOISE_ON, "NOISE", SUB_LIGHT, "Noise on / off (a little different on each side)");
    });
    if let Some((id, k)) = set.take() {
        ed.set(id, k as f32);
    }
    // The drive's curve: input (across) to output (up), as the voices shape it.
    drive_view(ui, ed, vec2(s.left_w, s.display_h));
    let (sub, noise, drive) = (ed.on(p::SUB_ON), ed.on(p::NOISE_ON), ed.choice(p::DIST_MODE) != 0);
    ui.horizontal(|ui| {
        knob_cell(ui, ed, p::SUB_LEVEL, "Sub", c, sub);
        divider(ui, s.cell_h);
        knob_cell(ui, ed, p::NOISE_COLOR, "Colour", c, noise);
        knob_cell(ui, ed, p::NOISE_LEVEL, "Noise", c, noise);
    });
    ui.horizontal(|ui| {
        const MODES: [&str; 5] = ["OFF", "TANH", "HARD", "FOLD", "CRUSH"];
        ui.vertical(|ui| {
            ui.add_space(8.0);
            if let Some(k) =
                switch(ui, &MODES, ed.choice(p::DIST_MODE), SUB_LIGHT, "Drive, per voice, before the filters")
            {
                set = Some((p::DIST_MODE, k));
            }
        });
        ui.add_space(6.0);
        knob_cell(ui, ed, p::DIST_DRIVE, "Drive", c, drive);
        knob_cell(ui, ed, p::DIST_MIX, "Mix", c, drive);
    });
    if let Some((id, k)) = set {
        ed.set(id, k as f32);
    }
}

fn drive_view(ui: &mut Ui, ed: &mut Ed, size: egui::Vec2) {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let mode = ed.choice(p::DIST_MODE) as u8;
    if mode != 0 {
        drag_param(ui, ed, &resp, p::DIST_DRIVE, 200.0, rect);
        resp.on_hover_text("Drag up/down or scroll: drive");
    }
    let painter = ui.painter_at(rect);
    theme::lcd_panel(&painter, rect);
    let inner = rect.shrink2(vec2(16.0, 14.0));
    // Square-ish plot in the middle; the wave it does to a sine on the right.
    let side = inner.height();
    let plot = Rect::from_min_size(inner.left_top(), vec2(side, side));
    let wave = Rect::from_min_max(pos2(plot.right() + 24.0, inner.top()), inner.right_bottom());
    for r in [plot, wave] {
        painter.line_segment([r.left_center(), r.right_center()], stroke(1.0, lcd_grid()));
    }
    painter.line_segment([plot.center_top(), plot.center_bottom()], stroke(1.0, lcd_grid()));
    let (gain, steps, _) = lyra_chrysalis::drive_consts(ed.v(p::DIST_DRIVE));
    let mix = if mode == 0 { 0.0 } else { ed.v(p::DIST_MIX) };
    let shape = |x: f32| x + (lyra_chrysalis::drive_shape(mode, gain, steps, x) - x) * mix;
    let ink = if mode == 0 { theme::LCD_INK_DIM } else { theme::LCD_INK };
    let n = 160;
    let pts: Vec<Pos2> = (0..=n)
        .map(|k| {
            let x = -1.0 + 2.0 * k as f32 / n as f32;
            pos2(plot.center().x + x * side * 0.5, plot.center().y - shape(x).clamp(-1.0, 1.0) * side * 0.5)
        })
        .collect();
    painter.add(Shape::line(pts, stroke(1.8, ink)));
    let pts: Vec<Pos2> = (0..=240)
        .map(|k| {
            let t = k as f32 / 240.0;
            let x = 0.9 * (std::f32::consts::TAU * 2.0 * t).sin();
            pos2(wave.left() + t * wave.width(), wave.center().y - shape(x).clamp(-1.0, 1.0) * wave.height() * 0.48)
        })
        .collect();
    fill_under(&painter, &pts, wave.center().y, lcd_fill());
    painter.add(Shape::line(pts, stroke(1.8, ink)));
}

// ───────────────────────────────────────────────────────────── FILTER page

fn filter_page(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let c = FILTER_LIGHT.mid;
    let mut set = None;
    switch_row(ui, |ui| {
        let hint = "Serial: filter 1 into filter 2. Parallel: both hear the voice, side by side.";
        if let Some(k) = switch(ui, &["SERIAL", "PARALLEL"], ed.choice(p::ROUTING), FILTER_LIGHT, hint) {
            set = Some(k);
        }
    });
    if let Some(k) = set {
        ed.set(p::ROUTING, k as f32);
    }
    filter_graph(ui, ed, vec2(s.left_w, s.display_h));
    for f in 0..2 {
        let at = |k: usize| p::filter(f, k);
        let on = ed.on(at(p::F_ON));
        let kind = ed.choice(at(p::F_TYPE)) as u8;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    toggle_key(ui, ed, at(p::F_ON), if f == 0 { "1" } else { "2" }, FILTER_LIGHT, "Filter on / off");
                    choice_well(ui, ed, at(p::F_TYPE), 92.0, "Filter type");
                });
            });
            let comb = kind >= ft::COMB_PLUS;
            let label = match kind {
                ft::FORMANT => "Shift",
                _ if comb => "Pitch",
                _ => "Cutoff",
            };
            knob_cell(ui, ed, at(p::F_CUTOFF), label, c, on);
            knob_cell(ui, ed, at(p::F_RESO), if comb { "Feedback" } else { "Reso" }, c, on);
            knob_cell(ui, ed, at(p::F_DRIVE), "Drive", c, on);
            knob_cell(ui, ed, at(p::F_KEY), "Key", c, on);
            knob_cell(ui, ed, at(p::F_VOWEL), "Vowel", c, on && kind == ft::FORMANT);
            knob_cell(ui, ed, at(p::F_MIX), "Mix", c, on);
        });
    }
}

/// One filter's response at `hz` (complex), as the voice computes it.
fn response(kind: u8, fc: f32, reso: f32, vowel: f32, mix: f32, hz: f32, sr: f32) -> (f32, f32) {
    use std::f32::consts::PI;
    let pre = |f: f32| (PI * f.min(sr * 0.49) / sr).tan();
    let fc = fc.clamp(20.0, sr * 0.45);
    // H(w) of an SVF as (re, im) with w = tan(πf/sr) / tan(πfc/sr).
    let div = |nr: f32, ni: f32, dr: f32, di: f32| {
        let m = dr * dr + di * di;
        ((nr * dr + ni * di) / m, (ni * dr - nr * di) / m)
    };
    let mul = |a: (f32, f32), b: (f32, f32)| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
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
                let b = svf(pre(hz) / pre(f), kf, (0.0, kf * pre(hz) / pre(f)));
                sum.0 += 2.0 * gain * b.0;
                sum.1 += 2.0 * gain * b.1;
            }
            sum
        }
        _ => {
            // 1 / (1 ∓ fb·e^(−jωD)), scaled by √(1 − fb²).
            let fb = 0.97 * reso;
            let sign = if kind == ft::COMB_PLUS { 1.0 } else { -1.0 };
            let ph = std::f32::consts::TAU * hz / fc;
            let (dr, di) = (1.0 - sign * fb * ph.cos(), sign * fb * ph.sin());
            let n = (1.0 - fb * fb).sqrt();
            div(n, 0.0, dr, di)
        }
    };
    (1.0 + (y.0 - 1.0) * mix, y.1 * mix)
}

#[allow(clippy::needless_range_loop)] // two filters, indexed in params and in `hot`
fn filter_graph(ui: &mut Ui, ed: &mut Ed, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let inner = rect.shrink2(vec2(14.0, 12.0));
    let (lo, hi) = (20f32.ln(), 20_000f32.ln());
    let fx = |hz: f32| inner.left() + (hz.ln() - lo) / (hi - lo) * inner.width();
    let hz_at = |x: f32| (lo + (x - inner.left()) / inner.width() * (hi - lo)).exp();
    let (db_top, db_bot) = (24.0, -36.0);
    let fy = |db: f32| inner.top() + (db_top - db.clamp(db_bot, db_top)) / (db_top - db_bot) * inner.height();
    let sr = 48_000.0;
    let parallel = ed.on(p::ROUTING);
    let one = |ed: &Ed, f: usize, hz: f32| {
        let at = |k: usize| ed.v(p::filter(f, k));
        response(
            ed.choice(p::filter(f, p::F_TYPE)) as u8,
            at(p::F_CUTOFF),
            at(p::F_RESO),
            at(p::F_VOWEL),
            at(p::F_MIX),
            hz,
            sr,
        )
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
        20.0 * (h.0 * h.0 + h.1 * h.1).sqrt().max(1e-6).log10()
    };
    // Dots: drag sideways for cutoff, up/down for resonance.
    let mut hot = [false; 2];
    for f in 0..2 {
        if !ed.on(p::filter(f, p::F_ON)) {
            continue;
        }
        let cut_id = p::filter(f, p::F_CUTOFF);
        let at = pos2(fx(ed.v(cut_id)), fy(20.0 * response_mag(one(ed, f, ed.v(cut_id))).log10()));
        let r = ui.interact(
            Rect::from_center_size(at, vec2(22.0, 22.0)),
            Id::new(("chrys-fdot", ed.ch, f)),
            Sense::click_and_drag(),
        );
        let reset = dbl(ui, &r, ("fdot", f));
        if r.drag_started() || reset {
            ed.h.checkpoint(ed.p);
        }
        if reset {
            ed.put(cut_id, PARAMS[cut_id].default);
            ed.put(p::filter(f, p::F_RESO), PARAMS[p::filter(f, p::F_RESO)].default);
        } else if r.dragged() {
            if let Some(pos) = r.interact_pointer_pos() {
                ed.put(cut_id, hz_at(pos.x));
                let reso = p::filter(f, p::F_RESO);
                ed.put(reso, ed.v(reso) - r.drag_delta().y / 150.0);
            }
        }
        hot[f] = r.hovered() || r.dragged();
        r.on_hover_text(format!(
            "Filter {}: {} · reso {}",
            f + 1,
            PARAMS[cut_id].format(ed.v(cut_id)),
            PARAMS[p::filter(f, p::F_RESO)].format(ed.v(p::filter(f, p::F_RESO)))
        ));
    }
    let painter = ui.painter_at(rect);
    theme::lcd_panel(&painter, rect);
    for hz in [100.0, 1000.0, 10_000.0] {
        painter.line_segment([pos2(fx(hz), inner.top()), pos2(fx(hz), inner.bottom())], stroke(1.0, lcd_grid()));
    }
    painter.line_segment([pos2(inner.left(), fy(0.0)), pos2(inner.right(), fy(0.0))], stroke(1.0, lcd_grid()));
    let any = ed.on(p::filter(0, p::F_ON)) || ed.on(p::filter(1, p::F_ON));
    let n = (inner.width() * 1.2) as usize;
    let pts: Vec<Pos2> = (0..=n)
        .map(|k| {
            let x = inner.left() + k as f32 / n as f32 * inner.width();
            pos2(x, fy(total(ed, hz_at(x))))
        })
        .collect();
    fill_under(&painter, &pts, inner.bottom(), lcd_fill());
    painter.add(Shape::line(pts, stroke(1.8, if any { theme::LCD_INK } else { theme::LCD_INK_DIM })));
    for f in 0..2 {
        if ed.on(p::filter(f, p::F_ON)) {
            let cut = ed.v(p::filter(f, p::F_CUTOFF));
            let at = pos2(fx(cut), fy(20.0 * response_mag(one(ed, f, cut)).log10()));
            dot(&painter, at, f == 0, hot[f]);
        }
    }
    let small = FontId::proportional(theme::m().text_small - 1.0);
    for (hz, label) in [(100.0, "100"), (1000.0, "1k"), (10_000.0, "10k")] {
        painter.text(pos2(fx(hz) + 3.0, inner.bottom()), Align2::LEFT_BOTTOM, label, small.clone(), theme::LCD_INK_DIM);
    }
}

fn response_mag(h: (f32, f32)) -> f32 {
    (h.0 * h.0 + h.1 * h.1).sqrt().max(1e-6)
}

// ───────────────────────────────────────────────────────────── ENV page

fn env_page(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let c = ENV_LIGHT.mid;
    let which_id = Id::new(("chrys-env", ed.ch));
    let mut e = ui.ctx().data(|d| d.get_temp::<usize>(which_id)).unwrap_or(0);
    switch_row(ui, |ui| {
        let hint = "The amp envelope shapes every note's loudness; envelopes 2–4 go wherever the MOD page sends them.";
        if let Some(k) = switch(ui, &["AMP", "ENV 2", "ENV 3", "ENV 4"], e, ENV_LIGHT, hint) {
            e = k;
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(which_id, e));
    envelope(ui, ed, e, vec2(s.left_w, s.display_h));
    let at = |k: usize| p::env(e, k);
    ui.horizontal(|ui| {
        knob_cell(ui, ed, at(p::E_ATTACK), "Attack", c, true);
        knob_cell(ui, ed, at(p::E_DECAY), "Decay", c, true);
        knob_cell(ui, ed, at(p::E_SUSTAIN), "Sustain", c, true);
        knob_cell(ui, ed, at(p::E_RELEASE), "Release", c, true);
    });
    ui.horizontal(|ui| {
        knob_cell(ui, ed, at(p::E_A_CURVE), "A curve", c, true);
        knob_cell(ui, ed, at(p::E_D_CURVE), "D curve", c, true);
        knob_cell(ui, ed, at(p::E_R_CURVE), "R curve", c, true);
    });
}

/// An ADSR you can drag: dots for the times and the sustain, rings in the
/// middle of each stage for its curve. Each stage has its own lane, so a time
/// sits where its knob points. Double-click resets what a handle moves.
fn envelope(ui: &mut Ui, ed: &mut Ed, e: usize, size: egui::Vec2) {
    let at = |k: usize| p::env(e, k);
    let (a_id, d_id, s_id, r_id) = (at(p::E_ATTACK), at(p::E_DECAY), at(p::E_SUSTAIN), at(p::E_RELEASE));
    let curves = [at(p::E_A_CURVE), at(p::E_D_CURVE), at(p::E_R_CURVE)];
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
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
    // The three stages as (x0, y0, x1, y1, curve id).
    let stages = |ed: &Ed| {
        let (xa, xd, xs, xr, s) = geom(ed);
        [(inner.left(), 0.0, xa, 1.0, curves[0]), (xa, 1.0, xd, s, curves[1]), (xs, s, xr, 0.0, curves[2])]
    };
    let base = Id::new(("chrys-envdots", ed.ch, e));
    let mut hot = None;
    let (xa, xd, _, xr, s) = geom(ed);
    let dots = [(0, pos2(xa, y(1.0))), (1, pos2(xd, y(s))), (2, pos2(xr, y(0.0)))];
    for (k, at) in dots {
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
    // Curve rings, in the middle of each stage.
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
    theme::lcd_panel(&painter, rect);
    let (_, _, xs, _, s) = geom(ed);
    for x in [inner.left() + lane, inner.left() + 2.0 * lane, xs] {
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], stroke(1.0, lcd_grid()));
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
    fill_under(&painter, &pts, inner.bottom(), lcd_fill());
    painter.add(Shape::line(pts, stroke(1.8, theme::LCD_INK)));
    let (xa, xd, _, xr, s) = geom(ed);
    for (k, at) in [pos2(xa, y(1.0)), pos2(xd, y(s)), pos2(xr, y(0.0))].into_iter().enumerate() {
        dot(&painter, at, true, hot == Some(k));
    }
    for (k, (x0, y0, x1, y1, cid)) in stages(ed).into_iter().enumerate() {
        if (y1 - y0).abs() < 0.02 || x1 - x0 < 6.0 {
            continue;
        }
        let mid = pos2(0.5 * (x0 + x1), y(y0 + (y1 - y0) * curve::bend(0.5, ed.v(cid))));
        ring(&painter, mid, hot == Some(10 + k));
    }
}

/// A small hollow handle (bends), lighter than a point.
fn ring(painter: &egui::Painter, at: Pos2, hot: bool) {
    if hot {
        painter.circle_stroke(at, 7.5, stroke(1.2, theme::LCD_INK_DIM));
    }
    painter.circle_filled(at, 3.5, theme::LCD);
    painter.circle_stroke(at, 3.2, stroke(1.4, theme::LCD_INK_DIM));
}

// ───────────────────────────────────────────────────────────── LFO page

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
        let pt = pts.get(k).copied();
        let (xi, yi, bi) = (p::point_x(l, k), p::point_y(l, k), p::point_bend(l, k));
        match pt {
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

fn lfo_page(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let lt = LFO_LIGHT;
    let c = lt.mid;
    let which_id = Id::new(("chrys-lfo", ed.ch));
    let mut l = ui.ctx().data(|d| d.get_temp::<usize>(which_id)).unwrap_or(0);
    let mut set = None;
    switch_row(ui, |ui| {
        if let Some(k) = switch(
            ui,
            &["LFO 1", "LFO 2", "LFO 3", "LFO 4"],
            l,
            lt,
            "Draw a shape, then send it somewhere on the MOD page",
        ) {
            l = k;
        }
        let hint = "Free: one shared LFO (locked to the song when synced). Retrig: restarts on every note. \
                    One-shot: runs once per note and holds its end, like an envelope.";
        if let Some(k) = switch(ui, &["FREE", "RETRIG", "1-SHOT"], ed.choice(p::lfo(l, p::L_MODE)), lt, hint) {
            set = Some(k);
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(which_id, l));
    if let Some(k) = set {
        ed.set(p::lfo(l, p::L_MODE), k as f32);
    }
    // Shape presets: a key each, with its picture.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let now = read_points(ed, l);
        for (name, pts) in SHAPES.iter() {
            let shape: Vec<Point> = pts.iter().map(|&(x, y, bend)| Point { x, y, bend }).collect();
            let lit = now == shape;
            if shape_key(ui, vec2(60.0, 32.0), &shape, lit.then_some(lt)).on_hover_text(*name).clicked() {
                ed.h.checkpoint(ed.p);
                write_points(ed, l, &shape);
            }
        }
    });
    // Taller than the other pages' LCDs: it has only one row of knobs under it.
    lfo_editor(ui, ed, l, vec2(s.left_w, s.display_h + s.cell_h - 32.0));
    let synced = ed.choice(p::lfo(l, p::L_SYNC)) != 0;
    ui.horizontal(|ui| {
        knob_cell(ui, ed, p::lfo(l, p::L_SYNC), "Sync", c, true);
        knob_cell(ui, ed, p::lfo(l, p::L_RATE), "Rate", c, !synced);
    });
}

/// A key with an LFO shape printed on it.
fn shape_key(ui: &mut Ui, size: egui::Vec2, shape: &[Point], light: Option<Light>) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let pressed = resp.is_pointer_button_down_on();
    theme::key(painter, rect, light, resp.hovered() && !pressed, if pressed { 0.9 } else { 1.0 });
    let face = theme::key_face(rect).shrink2(vec2(6.0, 7.0));
    let s = curve::Shape::new(shape);
    let pts: Vec<Pos2> = (0..=48)
        .map(|k| {
            let t = k as f32 / 48.0;
            pos2(face.left() + t * face.width(), face.bottom() - s.eval(t.min(0.9999)) * face.height())
        })
        .collect();
    painter.add(Shape::line(pts, stroke(1.5, theme::key_ink(light.is_some()))));
    resp
}

/// Draw the LFO: points, bends, adding and removing.
fn lfo_editor(ui: &mut Ui, ed: &mut Ed, l: usize, size: egui::Vec2) {
    let (rect, bg) = ui.allocate_exact_size(size, Sense::click());
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
    // Points.
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
    // Bend rings, between points.
    let seg = |pts: &[Point], k: usize| {
        let (a, b) = (pts[k], if k + 1 < pts.len() { pts[k + 1] } else { Point { x: 1.0, y: pts[0].y, bend: 0.0 } });
        (a, b)
    };
    for k in 0..n {
        let (a, b) = seg(&pts, k);
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
    // Double-click on empty space: a new point there.
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
    theme::lcd_panel(&painter, rect);
    for k in 1..16 {
        let x = sx(k as f32 / 16.0);
        let strong = k % 4 == 0;
        let c = if strong { lcd_grid() } else { theme::mix(theme::LCD, lcd_grid(), 0.45) };
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], stroke(1.0, c));
    }
    for k in 1..4 {
        let y = sy(k as f32 / 4.0);
        painter.line_segment(
            [pos2(inner.left(), y), pos2(inner.right(), y)],
            stroke(1.0, theme::mix(theme::LCD, lcd_grid(), 0.45)),
        );
    }
    let shape = curve::Shape::new(&pts);
    let m = (inner.width() * 1.5) as usize;
    let mut line: Vec<Pos2> = Vec::with_capacity(m + 8);
    // Sample densely, and exactly at every point's x so jumps stay vertical.
    let mut xs: Vec<f32> = (0..=m).map(|k| k as f32 / m as f32).collect();
    for q in &pts {
        xs.push(q.x);
        xs.push((q.x - 1e-5).max(0.0));
    }
    xs.sort_by(f32::total_cmp);
    for x in xs {
        line.push(pos2(sx(x), sy(shape.eval(x.min(0.999_999)))));
    }
    fill_under(&painter, &line, inner.bottom(), lcd_fill());
    painter.add(Shape::line(line, stroke(1.8, theme::LCD_INK)));
    for k in 0..pts.len() {
        let (a, b) = seg(&pts, k);
        if (b.y - a.y).abs() >= 0.02 && sx(b.x) - sx(a.x) >= 10.0 {
            let mid = pos2(sx(0.5 * (a.x + b.x)), sy(a.y + (b.y - a.y) * curve::bend(0.5, a.bend)));
            ring(&painter, mid, hot == Some(100 + k));
        }
    }
    for (k, q) in pts.iter().enumerate() {
        dot(&painter, pos2(sx(q.x), sy(q.y)), k > 0, hot == Some(k));
    }
}

// ───────────────────────────────────────────────────────────── MOD page

/// 16 slots, 8 at a time: # · source → destination · amount · via · Bi.
fn mod_page(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let c = MOD_LIGHT.mid;
    let bank_id = Id::new(("chrys-mod-bank", ed.ch));
    let mut bank = ui.ctx().data(|d| d.get_temp::<usize>(bank_id)).unwrap_or(0);
    switch_row(ui, |ui| {
        if let Some(k) = switch(ui, &["1–8", "9–16"], bank, MOD_LIGHT, "Slots 1–8 or 9–16") {
            bank = k;
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(bank_id, bank));
    // Column heads.
    let small = FontId::proportional(theme::m().text_small);
    let (head, _) = ui.allocate_exact_size(vec2(s.left_w, 14.0), Sense::hover());
    let cols = [(22.0, "source"), (160.0, "destination"), (330.0, "amount"), (420.0, "via")];
    for (x, t) in cols {
        ui.painter().text(head.left_center() + vec2(x, 0.0), Align2::LEFT_CENTER, t, small.clone(), theme::TEXT_DIM);
    }
    for k in bank * 8..bank * 8 + 8 {
        let at = |q: usize| p::slot(k, q);
        let live = ed.choice(at(p::S_SRC)) != 0 && ed.choice(at(p::S_DEST)) != 0;
        ui.allocate_ui_with_layout(vec2(s.left_w, s.slot_h), Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let ink = if live { theme::TEXT } else { theme::TEXT_DIM };
            ui.add_sized(vec2(14.0, s.key_h), egui::Label::new(RichText::new(format!("{}", k + 1)).small().color(ink)));
            choice_well(ui, ed, at(p::S_SRC), 112.0, "Source: what moves this slot");
            arrow(ui, s.key_h);
            choice_well(ui, ed, at(p::S_DEST), 150.0, "Destination: what it moves");
            let info = &PARAMS[at(p::S_AMT)];
            let mut v = ed.v(at(p::S_AMT));
            let r = widgets::knob(ui, &mut v, info, theme::m().knob_small, if live { c } else { theme::TEXT_DIM });
            ed.track(&r, at(p::S_AMT), v);
            r.on_hover_text("Amount: how far, and which way (double-click: 0)");
            ui.add_sized(
                vec2(40.0, s.key_h),
                egui::Label::new(RichText::new(info.format(ed.v(at(p::S_AMT)))).small().color(ink)),
            );
            choice_well(ui, ed, at(p::S_VIA), 96.0, "Via: a second source that scales the amount (e.g. velocity)");
            let bi = ed.on(at(p::S_BIPOLAR));
            let hint = "Bi: centre the source (−1..1), so it swings both ways around the knob";
            if widgets::key(ui, vec2(36.0, s.key_h), Legend::Text("BI"), bi, MOD_LIGHT, hint).clicked() {
                ed.toggle(at(p::S_BIPOLAR));
            }
        });
    }
}

fn arrow(ui: &mut Ui, h: f32) {
    let (ar, _) = ui.allocate_exact_size(vec2(14.0, h), Sense::hover());
    let (y, x0, x1) = (ar.center().y, ar.left() + 1.0, ar.right() - 1.0);
    let st = stroke(1.4, theme::TEXT_DIM);
    ui.painter().line_segment([pos2(x0, y), pos2(x1, y)], st);
    ui.painter().add(Shape::line(vec![pos2(x1 - 4.0, y - 3.5), pos2(x1, y), pos2(x1 - 4.0, y + 3.5)], st));
}

// ───────────────────────────────────────────────────────────── voice + macros

fn voice(ui: &mut Ui, ed: &mut Ed) {
    let c = VOICE_LIGHT.mid;
    let (mode, glide_mode) = (ed.choice(p::MODE), ed.choice(p::GLIDE_MODE));
    let glides = ed.v(p::GLIDE) > 0.0;
    let mut set = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        small_cell(ui, ed, p::VOLUME, "Volume", c, true);
        small_cell(ui, ed, p::AMP_VEL, "Velocity", c, true);
        small_cell(ui, ed, p::GLIDE, "Glide", c, true);
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.add_space(6.0);
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

/// Four big knobs; a macro's light is on when a slot listens to it.
fn macros(ui: &mut Ui, ed: &mut Ed) {
    let s = sizes();
    let m = theme::m();
    let small = FontId::proportional(m.text_small);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for k in 0..4 {
            let src = (lyra_chrysalis::params::src::MACRO + k) as f32;
            let routes = (0..p::SLOT_COUNT)
                .filter(|&sl| {
                    ed.v(p::slot(sl, p::S_DEST)) >= 0.5
                        && (ed.v(p::slot(sl, p::S_SRC)) == src || ed.v(p::slot(sl, p::S_VIA)) == src)
                })
                .count();
            let id = p::mac(k);
            ui.allocate_ui(vec2(s.cell_w, s.macros_h), |ui| {
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let (head, _) = ui.allocate_exact_size(vec2(s.cell_w, 14.0), Sense::hover());
                    theme::led(
                        ui.painter(),
                        head.center() - vec2(10.0, 0.0),
                        9.0,
                        (routes > 0).then_some(MOD_LIGHT),
                        false,
                    );
                    ui.painter().text(
                        head.center() + vec2(2.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("{}", k + 1),
                        small.clone(),
                        theme::TEXT_DIM,
                    );
                    let mut v = ed.v(id);
                    let r = widgets::knob(ui, &mut v, &PARAMS[id], m.knob_large + 8.0, MOD_LIGHT.mid);
                    ed.track(&r, id, v);
                    let tip = if routes == 0 {
                        format!("Macro {}: send it somewhere on the MOD page (source \"Macro {}\")", k + 1, k + 1)
                    } else {
                        format!("Macro {}: drives {routes} slot{}", k + 1, if routes == 1 { "" } else { "s" })
                    };
                    r.on_hover_text(tip);
                    ui.label(RichText::new(PARAMS[id].format(ed.v(id))).small().color(theme::TEXT));
                });
            });
        }
    });
}
