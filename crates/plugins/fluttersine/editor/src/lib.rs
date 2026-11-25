//! The hand-made editor for **Fluttersine**, Lyra's six-operator FM synth (Sytrus-style).
//!
//! ```text
//!  ┌ [readout] ◀ [ preset ▾ ] ▶      ~Fluttersine~      Tone Time Volume Glide  POLY|MONO ┐
//!  ├OP1┼OP2┼OP3┼OP4┼OP5┼OP6┼FILTER┼LFO┼MOD──────┤├MATRIX┼ALGORITHMS──────────────────────┤
//!  │ the open page: wave keys, LCDs, knobs     ││ 6 × 8 knobs, or a 3 × 3 grid of        │
//!  │                                           ││ classic routings                       │
//!  └───────────────────────────────────────────┘└────────────────────────────────────────┘
//! ```
//!
//! Lyra's visual language: fixed plates on a dark rack, a seam apart (laid out
//! with `flex`), so nothing moves when you switch pages. A tab is a flat block;
//! the open one is brighter with a thin line of its light on top. Colour is
//! only light: LEDs, lit keys and knob arcs, one light per operator. Keys are
//! flat (a dark recess, or solid light with dark lettering when lit), curves
//! are dark ink on LCDs, text stays grey.
//!
//! * Knobs carry only their names. Hover or turn one and its name becomes its
//!   value, while the readout in the header shows its full name and value.
//! * The right side shows the FM matrix (row modulates column; at-default
//!   knobs are faded so the routing stands out) or the algorithms: pressing one
//!   reshapes the matrix and keeps your amounts.
//! * Envelope dots and the filter dots can be dragged; the knobs are the same
//!   parameters. Wheel over tabs, wave keys or the preset steps through them.
//!
//! Like every view, this only edits `channel.params`; the bridge does the rest.
//! The waveform pictures are display-only copies of `lyra_fluttersine`'s shapes.

use lyra_kit::flex::{self, Grow, Px};
use lyra_kit::{tracked, wheel_step, Host};
use lyra_shared_look::{self as look, cr, stroke, Icon, Light};
use egui::{pos2, vec2, Align, Align2, Color32, Id, Layout, Rect, Response, RichText, Sense, Shape, Ui};
use egui::{UiBuilder, Vec2};
use lyra_fluttersine::{apply_algorithm, macro_factor, matching_algorithm, p, Algorithm, ALGORITHMS, FM_PARAMS};
use p::{op, OPS};
use std::f32::consts::TAU;

/// One light per operator: a rainbow, so an operator is easy to follow from
/// its tab to its matrix row and its knobs.
const OP_LIGHTS: [Light; OPS] =
    [look::L_PINK, look::L_PEACH, look::L_AMBER, look::L_MINT, look::L_BLUE, look::L_LAVENDER];
const FILTER_LIGHT: Light = look::L_CYAN;
const LFO_LIGHT: Light = look::L_LIME;
const VOICE_LIGHT: Light = look::L_MINT;
/// The MOD page and the matrix share the voice's light: they're about the whole patch.
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
/// Height of the matrix's column heads.
const HEADS_H: f32 = 18.0;

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
    host: &'a mut dyn Host,
    /// Tells this editor's egui ids apart from another Fluttersine's.
    ch: usize,
}

impl Ed<'_> {
    /// Where automation has it while the song plays, else its own value.
    fn v(&self, id: usize) -> f32 {
        self.host.param(id)
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
        self.host.set_param(id, x.clamp(info.min, info.max));
    }
    /// A one-shot change (a click): checkpoint, then set.
    fn set(&mut self, id: usize, x: f32) {
        if self.v(id) != x {
            self.host.begin_edit();
            self.put(id, x);
        }
    }
    /// A change from a knob-like widget: one checkpoint per drag.
    fn track(&mut self, r: &Response, id: usize, x: f32) {
        if tracked(r, self.host) {
            self.put(id, x);
        }
    }
}

/// Every size, derived from the synths' frozen sizes (`look::m()`). The
/// plates have fixed sizes: switching pages never resizes the window.
struct Sizes {
    cell_w: f32,
    cell_h: f32,
    /// A knob's name line under it.
    label_h: f32,
    /// Height of keys and switches.
    key_h: f32,
    /// Inside the page plate (left column).
    left_w: f32,
    page_h: f32,
    /// Inside the right plate (matrix / algorithms).
    right_w: f32,
    /// A matrix cell (square) and the row-name column beside the cells.
    mcell: f32,
    mlabel_w: f32,
    /// Inside the header plate.
    header_h: f32,
    tab_h: f32,
    /// Height of the LCD displays on the operator page.
    display_h: f32,
    /// The MOD page: small knob cells (envelopes) and one row per slot.
    small_cell_w: f32,
    small_cell_h: f32,
    slot_h: f32,
}

fn sizes() -> Sizes {
    let m = look::m();
    let label_h = m.text_small + 4.0;
    let cell_w = m.knob_large + 22.0;
    let cell_h = m.knob_large + label_h + 8.0;
    let key_h = m.control_h;
    let gap = 6.0;
    let display_h = 104.0;
    let page_h = key_h + gap + 32.0 + gap + display_h + gap + 2.0 * cell_h + gap;
    // The matrix gets the page's full height: cells as big as six rows allow.
    let mcell = ((page_h - HEADS_H) / OPS as f32).floor().min(46.0);
    let mlabel_w = 30.0;
    Sizes {
        cell_w,
        cell_h,
        label_h,
        key_h,
        left_w: 7.0 * cell_w + 6.0 * 5.0,
        page_h,
        right_w: mlabel_w + OPS as f32 * mcell + OUT_GAP + 2.0 * mcell,
        mcell,
        mlabel_w,
        header_h: m.knob_large - 2.0 + label_h + 6.0,
        tab_h: m.control_h + 8.0,
        display_h,
        small_cell_w: m.knob_small + 20.0,
        small_cell_h: m.knob_small + label_h + 6.0,
        slot_h: key_h + 6.0,
    }
}

/// Fluttersine's editor, for Lyra's list of editors.
pub const EDITOR: lyra_kit::Editor =
    lyra_kit::Editor { plugin: "lyra.fluttersine", show, fonts: look::add_font_families };

/// The Fluttersine wordmark, the user's artwork (`assets/logo.png`). Export over the file while
/// Lyra runs from the source tree and the editor shows it within a second.
const LOGO: lyra_kit::assets::Artwork = lyra_kit::assets::Artwork {
    name: "fluttersine-logo",
    built_in: include_bytes!("../assets/logo.png"),
    file: concat!(env!("CARGO_MANIFEST_DIR"), "/assets/logo.png"),
    max_h: 220,
};

/// The editor, in the synths' frozen style (see `lyra_shared_look`).
pub fn show(ui: &mut Ui, host: &mut dyn Host) {
    ui.scope(|ui| {
        look::freeze(ui);
        show_inner(ui, host);
    });
}

fn show_inner(ui: &mut Ui, host: &mut dyn Host) {
    let ch = host.instance();
    let mut ed = Ed { host, ch };
    let s = sizes();
    let seam = look::SEAM;
    let page_id = Id::new(("fm-page", ch));
    let side_id = Id::new(("fm-side", ch));
    let (stored, side) = ui.ctx().data(|d| (d.get_temp::<usize>(page_id), d.get_temp::<usize>(side_id)));
    let mut page = Page::from_index(stored.unwrap_or(0));
    let mut side = side.unwrap_or(0);
    let used = used_ops(&ed);

    // The rack: one dark frame, the plates carved out of it a seam apart.
    let outer = |inner: f32| inner + 2.0 * PAD;
    let (lw, rw) = (outer(s.left_w), outer(s.right_w));
    let size =
        vec2(seam + lw + seam + rw + seam, seam + outer(s.header_h) + seam + s.tab_h + seam + outer(s.page_h) + seam);
    let (rack, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rack, cr(3), look::BG);
    let [header_r, body] = flex::col(rack.shrink(seam), [Px(outer(s.header_h)), Grow(1.0)], seam);
    let [left, right] = flex::row(body, [Px(lw), Grow(1.0)], seam);
    let [left_tabs, page_r] = flex::col(left, [Px(s.tab_h), Grow(1.0)], seam);
    let [right_tabs, side_r] = flex::col(right, [Px(s.tab_h), Grow(1.0)], seam);

    header(ui, &mut ed, header_r);
    let names: Vec<String> = (0..Page::COUNT).map(|k| Page::from_index(k).name()).collect();
    let lights: Vec<Light> = (0..Page::COUNT).map(|k| Page::from_index(k).light()).collect();
    if let Some(k) = tab_strip(ui, left_tabs, "fm-tabs", &names, &lights, page.index()) {
        page = Page::from_index(k);
    }
    plate(ui, page_r, ("fm-page-plate", page.index()), |ui| match page {
        Page::Op(i) => op_page(ui, &mut ed, i),
        Page::Filter => filter_page(ui, &mut ed),
        Page::Lfo => lfo_page(ui, &mut ed),
        Page::Mod => mod_page(ui, &mut ed),
    });
    let side_names = ["MATRIX".to_string(), "ALGORITHMS".to_string()];
    if let Some(k) = tab_strip(ui, right_tabs, "fm-side-tabs", &side_names, &[MOD_LIGHT, MOD_LIGHT], side) {
        side = k;
    }
    ui.painter().rect_filled(side_r, cr(0), look::PANEL);
    let inner = side_r.shrink(PAD);
    if side == 0 {
        matrix(ui, &mut ed, inner, &mut page, &used);
    } else {
        algorithms(ui, &mut ed, inner);
    }

    ui.ctx().data_mut(|d| {
        d.insert_temp(page_id, page.index());
        d.insert_temp(side_id, side);
    });
}

/// A flat plate (`look::PANEL`) filling `r`, with its contents laid out inside the padding.
fn plate<R>(ui: &mut Ui, r: Rect, salt: impl std::hash::Hash, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.painter().rect_filled(r, cr(0), look::PANEL);
    let mut child =
        ui.new_child(UiBuilder::new().id_salt(salt).max_rect(r.shrink(PAD)).layout(Layout::top_down(Align::Min)));
    child.spacing_mut().item_spacing = vec2(5.0, 6.0);
    add(&mut child)
}

/// A child Ui exactly over `r` (for widgets placed by rectangle), disabled if asked.
fn place<R>(ui: &mut Ui, r: Rect, salt: impl std::hash::Hash, enabled: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(UiBuilder::new().id_salt(salt).max_rect(r).layout(Layout::top_down(Align::Min)));
    if !enabled {
        child.disable();
    }
    add(&mut child)
}

// ───────────────────────────────────────────────────────────── header

/// The header, one plate across the top: the readout, the preset, the logo,
/// the four knobs that shape any patch, and the voice switches.
fn header(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let s = sizes();
    ui.painter().rect_filled(r, cr(0), look::PANEL);
    let inner = r.shrink2(vec2(PAD + 2.0, PAD));
    let knob_w = 54.0;
    let seg_w = 2.0 * SEG_W;
    let [read, preset, logo_r, knobs, switches] =
        flex::row(inner, [Px(150.0), Grow(1.0), Px(140.0), Px(4.0 * knob_w), Px(seg_w)], 12.0);
    readout_display(ui, flex::center(read, vec2(read.width(), inner.height().min(50.0))));
    preset_row(ui, ed, flex::center(preset, vec2(preset.width(), s.key_h + 8.0)));
    logo(ui, logo_r);
    let c = VOICE_LIGHT.mid;
    let cells = flex::even_row(knobs, 4, 0.0);
    let max_d = look::m().knob_large - 2.0;
    knob_in(ui, ed, cells[0], p::TONE, "Tone", c, true, max_d);
    knob_in(ui, ed, cells[1], p::TIME, "Time", c, true, max_d);
    knob_in(ui, ed, cells[2], p::VOLUME, "Volume", c, true, max_d);
    knob_in(ui, ed, cells[3], p::GLIDE, "Glide", c, true, max_d);
    let pair = flex::center(switches, vec2(switches.width(), 2.0 * s.key_h + 6.0));
    let [mode_r, glide_r] = flex::col(pair, [Px(s.key_h), Px(s.key_h)], 6.0);
    let (mode, glide_mode) = (ed.choice(p::MODE), ed.choice(p::GLIDE_MODE));
    let glides = ed.v(p::GLIDE) > 0.0;
    let mut set = None;
    let hint = "Mono: one note at a time, with legato and glide.";
    if let Some(k) = place(ui, mode_r, "fm-mode", true, |ui| switch(ui, &["POLY", "MONO"], mode, VOICE_LIGHT, hint)) {
        set = Some((p::MODE, k));
    }
    let hint = "Glide only between overlapping notes (legato), or on every note.";
    if let Some(k) = place(ui, glide_r, "fm-glide-mode", glides, |ui| {
        switch(ui, &["LEGATO", "ALWAYS"], glide_mode, VOICE_LIGHT, hint)
    }) {
        set = Some((p::GLIDE_MODE, k));
    }
    if let Some((id, k)) = set {
        ed.set(id, k as f32);
    }
}

/// The Fluttersine wordmark, part of the header (no frame of its own).
fn logo(ui: &mut Ui, r: Rect) {
    let (tex, [w, h]) = LOGO.texture(ui.ctx());
    let scale = (r.width() / w as f32).min(r.height() / h as f32);
    let img = Rect::from_center_size(r.center(), vec2(w as f32, h as f32) * scale);
    ui.painter().image(tex, img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
}

// ───────────────────────────────────────────────────────────── the readout

/// What the mouse is on: a control's full name and its value, shown on the
/// header's LCD (so the knobs themselves only carry their names).
#[derive(Clone)]
struct Readout {
    name: String,
    value: String,
}

fn readout_id() -> Id {
    Id::new("fm-readout")
}

fn readout(ui: &Ui, name: String, value: String) {
    ui.ctx().data_mut(|d| d.insert_temp(readout_id(), Readout { name, value }));
}

fn readout_display(ui: &Ui, r: Rect) {
    let p = ui.painter_at(r);
    look::lcd_panel(&p, r);
    let shown = ui.ctx().data(|d| d.get_temp::<Readout>(readout_id()));
    let inner = r.shrink2(vec2(9.0, 6.0));
    if let Some(ro) = shown {
        let p = ui.painter_at(inner);
        p.text(
            inner.left_top(),
            Align2::LEFT_TOP,
            ro.name,
            look::font(look::m().text_small),
            look::LCD_INK_DIM,
        );
        p.text(inner.left_bottom(), Align2::LEFT_BOTTOM, ro.value, look::font(16.0), look::LCD_INK);
    }
}

/// A parameter's name as the readout shows it (operator knobs say which operator).
fn long_name(id: usize) -> String {
    let name = FM_PARAMS[id].name;
    if id < p::MATRIX {
        format!("OP{} {}", id / p::OP_STRIDE + 1, name.to_lowercase())
    } else {
        name.to_string()
    }
}

// ───────────────────────────────────────────────────────────── presets

/// `◀ [ preset ▾ ] ▶` filling `r`, a dropdown like the pattern picker. Click
/// for the list, scroll over it to step.
fn preset_row(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let all = ed.host.presets();
    let n = all.len();
    let current = lyra_kit::presets::current(&all, ed.host.params());
    // The last preset loaded here, so ◀ ▶ still make sense after you tweak it.
    let last_id = Id::new(("fm-last-preset", ed.ch));
    let last: Option<String> = ui.ctx().data(|d| d.get_temp(last_id));
    let last_k = last.as_ref().and_then(|name| all.iter().position(|e| &e.name == name));
    let at = current.or(last_k).unwrap_or(0);
    let (name, edited) = match (current, last_k) {
        (Some(k), _) => (all[k].name.clone(), false),
        (None, Some(k)) => (all[k].name.clone(), true),
        (None, None) => ("Custom".to_string(), false),
    };
    let mut pick: Option<lyra_kit::PresetEntry> = None;
    let [prev, field, next] = flex::row(r, [Px(r.height()), Grow(1.0), Px(r.height())], 4.0);
    if icon_key(ui, prev, "fm-prev-preset", Icon::Prev).on_hover_text("Previous preset").clicked() && n > 0 {
        pick = Some(all[(at + n - 1) % n].clone());
    }
    if icon_key(ui, next, "fm-next-preset", Icon::Next).on_hover_text("Next preset").clicked() && n > 0 {
        pick = Some(all[(at + 1) % n].clone());
    }
    let resp = ui.interact(field, Id::new(("fm-preset-field", ed.ch)), Sense::click());
    let painter = ui.painter_at(field);
    look::well(&painter, field);
    let m = look::m();
    let text = painter.text(
        field.left_center() + vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        &name,
        look::font(m.text),
        look::TEXT,
    );
    if edited {
        painter.text(
            text.right_center() + vec2(8.0, 1.0),
            Align2::LEFT_CENTER,
            "edited",
            look::font(m.text_small),
            look::TEXT_DIM,
        );
    }
    let c = field.right_center() - vec2(13.0, 1.5);
    let chevron = vec![c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)];
    painter.add(Shape::line(chevron, stroke(1.6, look::TEXT_DIM)));
    if let Some(step) = wheel_step(ui, field) {
        if n > 0 {
            pick = Some(all[(at as i32 + step).rem_euclid(n as i32) as usize].clone());
        }
    }
    let popup = ui.make_persistent_id(("fm-preset-list", ed.ch));
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup));
    }
    let resp = resp.on_hover_text("Preset: scroll to flip through, click for the list (and to save yours)");
    egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
        look::freeze(ui);
        let (picked, close) = lyra_kit::presets::menu(ui, ed.host, field.width().max(220.0), look::MENU_INK);
        if picked.is_some() {
            pick = picked;
        }
        if close {
            ui.memory_mut(|m| m.close_popup());
        }
    });
    if let Some(e) = pick {
        ed.host.begin_edit();
        ed.host.set_params(e.params);
        ui.ctx().data_mut(|d| d.insert_temp(last_id, e.name));
    }
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

/// Tabs as flat blocks side by side, a seam apart. The open one is a brighter
/// plate with a thin line of its light on top. The wheel steps through them.
/// Returns the tab picked this frame.
fn tab_strip(ui: &mut Ui, r: Rect, salt: &str, names: &[String], lights: &[Light], open: usize) -> Option<usize> {
    let font = look::font(look::m().text_small + 1.0);
    let mut picked = None;
    for (k, b) in flex::even_row(r, names.len(), look::SEAM).into_iter().enumerate() {
        let resp = ui.interact(b, Id::new((salt, k)), Sense::click());
        let is_open = k == open;
        let painter = ui.painter();
        let fill = if is_open {
            look::PANEL_HI
        } else if resp.hovered() {
            look::mix(look::PANEL, look::PANEL_HI, 0.5)
        } else {
            look::PANEL
        };
        painter.rect_filled(b, cr(0), fill);
        if is_open {
            painter.rect_filled(Rect::from_min_size(b.min, vec2(b.width(), 2.0)), cr(0), lights[k].mid);
        }
        let ink = if is_open || resp.hovered() { look::TEXT } else { look::TEXT_DIM };
        painter.text(b.center() + vec2(0.0, 1.0), Align2::CENTER_CENTER, &names[k], font.clone(), ink);
        if resp.clicked() {
            picked = Some(k);
        }
    }
    if let Some(step) = wheel_step(ui, r) {
        picked = Some((open as i32 + step).rem_euclid(names.len() as i32) as usize);
    }
    picked
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

/// Width of one segment of a switch: every switch in the editor has the same size.
const SEG_W: f32 = 52.0;

/// A flat segmented switch: one dark recess, the chosen segment solid light with
/// dark lettering. Returns the segment clicked this frame.
fn switch(ui: &mut Ui, labels: &[&str], chosen: usize, light: Light, hint: &str) -> Option<usize> {
    let s = sizes();
    let (r, resp) = ui.allocate_exact_size(vec2(SEG_W * labels.len() as f32, s.key_h), Sense::click());
    let p = ui.painter();
    p.rect_filled(r, cr(3), look::INSET);
    let hover = resp.hover_pos().map(|pos| (((pos.x - r.left()) / SEG_W) as usize).min(labels.len() - 1));
    let font = look::font(look::m().text_small);
    for (i, label) in labels.iter().enumerate() {
        let seg = Rect::from_min_size(pos2(r.left() + i as f32 * SEG_W, r.top()), vec2(SEG_W, r.height())).shrink(2.0);
        let ink = if i == chosen {
            p.rect_filled(seg, cr(2), if hover == Some(i) { look::shade(light.mid, 1.06) } else { light.mid });
            look::key_ink(true)
        } else if hover == Some(i) {
            p.rect_filled(seg, cr(2), look::mix(look::INSET, look::PANEL_HI, 0.55));
            look::TEXT
        } else {
            look::TEXT_DIM
        };
        p.text(seg.center(), Align2::CENTER_CENTER, *label, font.clone(), ink);
    }
    let clicked = resp.clicked().then_some(()).and(hover);
    resp.on_hover_text(hint);
    clicked
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
        look::lcd_panel(&painter, rect);
        let inner = rect.shrink2(vec2(10.0, 14.0));
        painter.line_segment([inner.left_center(), inner.right_center()], stroke(1.0, lcd_grid()));
        draw_curve(
            &painter,
            inner,
            &|t| wave_at(wave, t, pw, spread, sides),
            2.0,
            look::LCD_INK,
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
        if text_key(ui, vec2(78.0, s.key_h), "LOW-PASS", lp.then_some(lt), "Low-pass on / off").clicked() {
            set = Some((p::LP_ON, if lp { 0 } else { 1 }));
        }
        ui.add_enabled_ui(lp, |ui| {
            if let Some(k) = switch(ui, &["12 dB", "24 dB"], slope, lt, "Low-pass slope") {
                set = Some((p::LP_SLOPE, k));
            }
        });
        if text_key(ui, vec2(78.0, s.key_h), "HIGH-PASS", hp.then_some(lt), "High-pass on / off").clicked() {
            set = Some((p::HP_ON, if hp { 0 } else { 1 }));
        }
    });
    if let Some((id, k)) = set {
        ed.set(id, k as f32);
    }
    // The graph takes what the switches and two knob rows leave.
    filter_graph(ui, ed, vec2(s.left_w, s.page_h - s.key_h - 2.0 * s.cell_h - 4.0 * 6.0));
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
            ed.host.begin_edit();
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
    look::lcd_panel(&painter, rect);
    let small = look::font(look::m().text_small - 1.0);
    for (f, label) in [(100.0, "100"), (1000.0, "1k"), (10_000.0, "10k")] {
        let x = fx(f);
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], stroke(1.0, lcd_grid()));
        painter.text(pos2(x + 3.0, inner.bottom()), Align2::LEFT_BOTTOM, label, small.clone(), look::LCD_INK_DIM);
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
    let ink = if lp || hp { look::LCD_INK } else { look::LCD_INK_DIM };
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
        painter.text(inner.center(), Align2::CENTER_CENTER, "filters off", small, look::LCD_INK_DIM);
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
    let rate = if synced { ed.host.tempo() as f32 / 60.0 / beats } else { ed.v(rate_id) };
    let (rect, _) = ui.allocate_exact_size(vec2(s.left_w, s.display_h - 12.0), Sense::hover());
    let painter = ui.painter_at(rect);
    look::lcd_panel(&painter, rect);
    let inner = rect.shrink2(vec2(10.0, 14.0));
    painter.line_segment([inner.left_center(), inner.right_center()], stroke(1.0, lcd_grid()));
    draw_curve(&painter, inner, &|t| lfo_at(shape, t), rate, look::LCD_INK, 1.8, Some(inner.center().y));
    let caption = if synced { format!("1 second at {:.0} BPM", ed.host.tempo()) } else { "1 second".into() };
    painter.text(
        rect.right_bottom() - vec2(9.0, 3.0),
        Align2::RIGHT_BOTTOM,
        caption,
        look::font(look::m().text_small - 1.0),
        look::LCD_INK_DIM,
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
    let slots_w = s.left_w - s.small_cell_w - 12.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            for k in 0..p::MOD_SLOTS {
                let live = ed.choice(mod_src(k)) != 0 && ed.choice(mod_dest(k)) != 0;
                ui.allocate_ui_with_layout(vec2(slots_w, s.slot_h), Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let ink = if live { look::TEXT } else { look::TEXT_DIM };
                    ui.add_sized(
                        vec2(14.0, s.key_h),
                        egui::Label::new(RichText::new(format!("{}", k + 1)).small().color(ink)),
                    );
                    choice_well(ui, ed, mod_src(k), 116.0, "Source: what moves this slot");
                    // An arrow, drawn (the UI font has no "→").
                    let (ar, _) = ui.allocate_exact_size(vec2(14.0, s.key_h), Sense::hover());
                    let (y, x0, x1) = (ar.center().y, ar.left() + 1.0, ar.right() - 1.0);
                    let arrow = stroke(1.4, look::TEXT_DIM);
                    ui.painter().line_segment([pos2(x0, y), pos2(x1, y)], arrow);
                    ui.painter()
                        .add(Shape::line(vec![pos2(x1 - 4.0, y - 3.5), pos2(x1, y), pos2(x1 - 4.0, y + 3.5)], arrow));
                    choice_well(ui, ed, mod_dest(k), 150.0, "Destination: what it moves");
                    let info = &FM_PARAMS[mod_amt(k)];
                    bare_knob(ui, ed, mod_amt(k), look::m().knob_small, if live { c } else { look::TEXT_DIM }, true);
                    ui.label(RichText::new(info.format(ed.v(mod_amt(k)))).small().color(ink));
                });
            }
        });
        // Beside the slots: how velocity sets the loudness, and the supersaw's stereo width.
        ui.add_space(8.0);
        ui.vertical(|ui| {
            let saws = (0..OPS).any(|i| ed.choice(op(i, p::OP_WAVE)) == W_SSAW);
            small_cell(ui, ed, p::AMP_VEL, "Velocity", c, true);
            small_cell(ui, ed, p::WIDTH, "Width", c, saws);
        });
    });
}

/// A dropdown well for a choice parameter: click for the list, wheel to step.
fn choice_well(ui: &mut Ui, ed: &mut Ed, id: usize, width: f32, hint: &str) {
    let s = sizes();
    let m = look::m();
    let info = &FM_PARAMS[id];
    let n = info.labels.len();
    let at = ed.choice(id).min(n - 1);
    let (r, resp) = ui.allocate_exact_size(vec2(width, s.key_h), Sense::click());
    let painter = ui.painter();
    look::well(painter, r);
    let ink = if at == 0 { look::TEXT_DIM } else { look::TEXT };
    painter.text(
        r.left_center() + vec2(8.0, 0.0),
        Align2::LEFT_CENTER,
        info.labels[at],
        look::font(m.text_small + 1.0),
        ink,
    );
    let c = r.right_center() - vec2(10.0, 1.0);
    let chevron = vec![c + vec2(-3.0, -1.2), c + vec2(0.0, 1.8), c + vec2(3.0, -1.2)];
    painter.add(Shape::line(chevron, stroke(1.4, look::TEXT_DIM)));
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
        look::freeze(ui);
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

// ───────────────────────────────────────────────────────────── matrix and algorithms

/// The FM matrix filling `r`: row = operator, columns = what it modulates (the
/// diagonal is feedback), then OUT and PAN. Knobs at their default are faded,
/// so the routing that's actually there stands out.
fn matrix(ui: &mut Ui, ed: &mut Ed, r: Rect, page: &mut Page, used: &[bool; OPS]) {
    let s = sizes();
    let small = look::font(look::m().text_small);
    // Square cells, packed edge to edge, centred in the plate.
    let width = s.mlabel_w + (OPS + 2) as f32 * s.mcell + OUT_GAP;
    let origin = pos2(r.center().x - width * 0.5, r.top());
    let col_x = |col: usize| origin.x + s.mlabel_w + col as f32 * s.mcell + if col >= OPS { OUT_GAP } else { 0.0 };
    let row_y = |row: usize| origin.y + HEADS_H + row as f32 * s.mcell;
    for col in 0..OPS + 2 {
        let text = match col {
            c if c == OPS => "OUT".to_string(),
            c if c == OPS + 1 => "PAN".to_string(),
            c => (c + 1).to_string(),
        };
        let at = pos2(col_x(col) + s.mcell * 0.5, origin.y + HEADS_H * 0.45);
        ui.painter().text(at, Align2::CENTER_CENTER, text, small.clone(), look::TEXT_DIM);
    }
    let d = (s.mcell - 10.0).min(look::m().knob_large - 4.0);
    for row in 0..OPS {
        let lt = OP_LIGHTS[row];
        // Row name: the operator's LED (lit = it can be heard) and its number;
        // the open operator's row name sits on a brighter plate.
        let name_r = Rect::from_min_size(pos2(origin.x, row_y(row)), vec2(s.mlabel_w - 3.0, s.mcell));
        let resp = ui.interact(name_r, Id::new(("fm-matrix-row", row)), Sense::click());
        let open = *page == Page::Op(row);
        let painter = ui.painter();
        if open || resp.hovered() {
            let fill = if open { look::PANEL_HI } else { look::mix(look::PANEL, look::PANEL_HI, 0.5) };
            painter.rect_filled(name_r, cr(2), fill);
        }
        if open {
            painter.rect_filled(Rect::from_min_size(name_r.min, vec2(2.0, name_r.height())), cr(0), lt.mid);
        }
        let led_c = pos2(name_r.left() + 9.0, name_r.center().y);
        look::led(painter, led_c, 10.0, used[row].then_some(lt), false);
        let ink = if open || resp.hovered() { look::TEXT } else { look::TEXT_DIM };
        painter.text(led_c + vec2(8.0, 0.0), Align2::LEFT_CENTER, (row + 1).to_string(), small.clone(), ink);
        if resp.hovered() {
            readout(ui, format!("Operator {}", row + 1), "click to open".into());
        }
        if resp.clicked() {
            *page = Page::Op(row);
        }
        for col in 0..OPS + 2 {
            let id = match col {
                c if c == OPS => p::out(row),
                c if c == OPS + 1 => p::pan(row),
                c => p::matrix(row, c),
            };
            let cell = Rect::from_min_size(pos2(col_x(col), row_y(row)), Vec2::splat(s.mcell));
            if col == row {
                // Feedback: the diagonal sits in a faint, square recess.
                ui.painter().rect_filled(cell, cr(0), look::mix(look::PANEL, look::BG, 0.3));
            }
            let at_default = ed.v(id) == FM_PARAMS[id].default;
            let faded = at_default && !ui.rect_contains_pointer(cell);
            let kr = Rect::from_center_size(cell.center(), Vec2::splat(d));
            place(ui, kr, ("fm-matrix-knob", id), true, |ui| {
                if faded {
                    ui.set_opacity(0.4);
                }
                bare_knob(ui, ed, id, d, lt.mid, true);
            });
        }
    }
}

/// The classic routings as a 3 × 3 grid of big keys, each printed with its
/// diagram and name. A key lights while the matrix has its shape; pressing one
/// reshapes the matrix and keeps the amounts you set.
fn algorithms(ui: &mut Ui, ed: &mut Ed, r: Rect) {
    let current = matching_algorithm(ed.host.params());
    let mut pick = None;
    let font = look::font(look::m().text_small);
    let rows = flex::even_col(r, 3, 6.0);
    for (k, alg) in ALGORITHMS.iter().enumerate() {
        let key = flex::even_row(rows[k / 3], 3, 6.0)[k % 3];
        let resp = ui.interact(key, Id::new(("fm-algorithm", k)), Sense::click());
        let lit = (current == Some(k)).then_some(MOD_LIGHT);
        let ink = flat(ui.painter(), key, lit, resp.hovered());
        let [art, name] = flex::col(key.shrink2(vec2(14.0, 10.0)), [Grow(1.0), Px(14.0)], 6.0);
        draw_algorithm(ui.painter(), art, alg, ink);
        ui.painter().text(name.center(), Align2::CENTER_CENTER, alg.short, font.clone(), ink);
        if resp.hovered() {
            let use_for = alg.name.split_once(": ").map_or("", |(_, b)| b);
            readout(ui, alg.short.to_string(), use_for.to_string());
        }
        if resp.clicked() {
            pick = Some(k);
        }
    }
    if let Some(k) = pick {
        let changes = apply_algorithm(ed.host.params(), &ALGORITHMS[k]);
        ed.host.begin_edit();
        for (id, x) in changes {
            ed.put(id, x);
        }
    }
}

/// A routing as a diagram: carriers on a bus line at the bottom, each
/// modulator above what it modulates, lines for the connections. Scales to `r`.
fn draw_algorithm(painter: &egui::Painter, r: Rect, alg: &Algorithm, ink: Color32) {
    // Depth: carriers 0, a modulator one above the deepest thing it feeds.
    let mut depth = [0usize; OPS];
    for _ in 0..OPS {
        for &(from, to) in alg.edges {
            depth[from] = depth[from].max(depth[to] + 1);
        }
    }
    let levels = depth.iter().copied().max().unwrap_or(0);
    // Across: carriers evenly spaced; a modulator over the middle of its targets,
    // then neighbours on the same level that landed on one spot are fanned out.
    let mut x = [0.0f32; OPS];
    for (k, &c) in alg.carriers.iter().enumerate() {
        x[c] = k as f32;
    }
    for d in 1..=levels {
        for i in (0..OPS).filter(|&i| depth[i] == d) {
            let targets: Vec<f32> = alg.edges.iter().filter(|e| e.0 == i).map(|e| x[e.1]).collect();
            x[i] = targets.iter().sum::<f32>() / targets.len().max(1) as f32;
        }
        let mut row: Vec<usize> = (0..OPS).filter(|&i| depth[i] == d).collect();
        row.sort_by(|a, b| x[*a].total_cmp(&x[*b]));
        let mut k = 0;
        while k < row.len() {
            let same: Vec<usize> = row[k..].iter().copied().take_while(|&i| (x[i] - x[row[k]]).abs() < 0.01).collect();
            let mid = x[row[k]];
            for (j, &i) in same.iter().enumerate() {
                x[i] = mid + (j as f32 - (same.len() - 1) as f32 * 0.5);
            }
            k += same.len();
        }
    }
    let (lo, hi) = x.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let dot_r = (r.height().min(r.width()) / 16.0).clamp(1.9, 4.0);
    let bus = r.bottom() - 1.0;
    let step_y = ((r.height() - 3.0 * dot_r) / levels.max(1) as f32).min(r.height() * 0.32);
    let step_x = if hi > lo { ((r.width() - 2.0 * dot_r) / (hi - lo)).min(r.width() * 0.2) } else { 0.0 };
    let at =
        |i: usize| pos2(r.center().x + (x[i] - (lo + hi) * 0.5) * step_x, bus - 1.5 * dot_r - depth[i] as f32 * step_y);
    let line = stroke((dot_r * 0.5).max(1.0), ink);
    // The output bus under the carriers.
    let (cl, cr_) = alg.carriers.iter().fold((f32::MAX, f32::MIN), |(a, b), &c| (a.min(at(c).x), b.max(at(c).x)));
    painter.line_segment([pos2(cl - dot_r, bus), pos2(cr_ + dot_r, bus)], line);
    for &c in alg.carriers {
        painter.line_segment([at(c), pos2(at(c).x, bus)], line);
    }
    for &(from, to) in alg.edges {
        painter.line_segment([at(from), at(to)], line);
    }
    for i in 0..OPS {
        painter.circle_filled(at(i), dot_r, ink);
    }
}

// ───────────────────────────────────────────────────────────── shared pieces

/// A knob and nothing else: hovered or turned, it shows itself on the readout.
fn bare_knob(ui: &mut Ui, ed: &mut Ed, id: usize, d: f32, color: Color32, enabled: bool) -> bool {
    let info = &FM_PARAMS[id];
    let mut v = ed.v(id);
    let resp =
        look::knob_painted(ui, &mut v, info, d, |p, rect, v, hot| look::paint_knob(p, rect, info, v, color, hot));
    if enabled {
        ed.track(&resp, id, v);
    }
    let hot = resp.hovered() || resp.dragged();
    if hot && enabled {
        readout(ui, long_name(id), format_value(id, ed.v(id)));
    }
    hot
}

/// A knob filling the cell `r` (as big as fits, up to `max_d`) with its name
/// under it. Hovered or turned, the name becomes the value.
#[allow(clippy::too_many_arguments)]
fn knob_in(ui: &mut Ui, ed: &mut Ed, r: Rect, id: usize, label: &str, color: Color32, enabled: bool, max_d: f32) {
    let s = sizes();
    let d = (r.height() - s.label_h - 2.0).min(r.width() - 4.0).min(max_d).max(14.0);
    let top = r.top() + (r.height() - d - s.label_h - 2.0) * 0.5;
    let kr = Rect::from_min_size(pos2(r.center().x - d * 0.5, top), vec2(d, d));
    let lr = Rect::from_min_size(pos2(r.left(), kr.bottom() + 2.0), vec2(r.width(), s.label_h));
    place(ui, kr, ("fm-knob", id), enabled, |ui| {
        let hot = bare_knob(ui, ed, id, d, color, enabled);
        let (text, ink) =
            if hot { (format_value(id, ed.v(id)), look::TEXT) } else { (label.to_string(), look::TEXT_DIM) };
        // The child's painter, so a disabled knob's name fades with it.
        ui.painter().with_clip_rect(lr.expand(2.0)).text(
            lr.center(),
            Align2::CENTER_CENTER,
            text,
            look::font(look::m().text_small),
            ink,
        );
    });
}

/// A big knob in a page row (seven fit across a page).
fn knob_cell(ui: &mut Ui, ed: &mut Ed, id: usize, label: &str, color: Color32, enabled: bool) {
    let s = sizes();
    let (r, _) = ui.allocate_exact_size(vec2(s.cell_w, s.cell_h), Sense::hover());
    knob_in(ui, ed, r, id, label, color, enabled, look::m().knob_large);
}

/// A smaller knob cell, for rows that need more than seven knobs.
fn small_cell(ui: &mut Ui, ed: &mut Ed, id: usize, label: &str, color: Color32, enabled: bool) {
    let s = sizes();
    let (r, _) = ui.allocate_exact_size(vec2(s.small_cell_w, s.small_cell_h), Sense::hover());
    knob_in(ui, ed, r, id, label, color, enabled, look::m().knob_small);
}

/// Paint a flat key: a dark recess, or solid light when lit. Returns the ink for its legend.
fn flat(p: &egui::Painter, r: Rect, lit: Option<Light>, hot: bool) -> Color32 {
    match lit {
        Some(l) => {
            p.rect_filled(r, cr(3), if hot { look::shade(l.mid, 1.06) } else { l.mid });
            look::key_ink(true)
        }
        None => {
            p.rect_filled(r, cr(3), if hot { look::mix(look::INSET, look::PANEL_HI, 0.55) } else { look::INSET });
            if hot {
                look::TEXT
            } else {
                look::TEXT_DIM
            }
        }
    }
}

/// A flat key with a word on it.
fn text_key(ui: &mut Ui, size: Vec2, text: &str, lit: Option<Light>, hint: &str) -> Response {
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let ink = flat(ui.painter(), r, lit, resp.hovered());
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, look::font(look::m().text_small), ink);
    resp.on_hover_text(hint)
}

/// A flat key with an icon, at `r`.
fn icon_key(ui: &mut Ui, r: Rect, salt: &str, icon: Icon) -> Response {
    let resp = ui.interact(r, Id::new(salt), Sense::click());
    let ink = flat(ui.painter(), r, None, resp.hovered());
    look::paint_icon(ui.painter(), r.shrink(r.height() * 0.28), icon, ink, look::INSET);
    resp
}

/// A flat key with a little curve printed on it; lit when `light` is `Some`.
fn glyph_key(ui: &mut Ui, size: Vec2, glyph: Glyph, light: Option<Light>) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let ink = flat(ui.painter(), rect, light, resp.hovered());
    draw_glyph(ui.painter(), rect.shrink2(vec2(7.0, 8.0)), glyph, ink, 1.5);
    resp
}

fn format_value(id: usize, v: f32) -> String {
    if id == p::TONE || id == p::TIME {
        format!("×{:.2}", macro_factor(v))
    } else if id < p::MATRIX && id % p::OP_STRIDE == p::OP_RATIO {
        format!("×{v:.2}")
    } else {
        FM_PARAMS[id].format(v)
    }
}

/// A thin seam between groups of knobs.
fn divider(ui: &mut Ui, h: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(6.0, h), Sense::hover());
    let x = r.center().x;
    ui.painter().rect_filled(
        Rect::from_min_max(pos2(x - 1.0, r.top() + 8.0), pos2(x + 1.0, r.bottom() - 8.0)),
        cr(0),
        look::BG,
    );
}

/// Faint LCD grid lines.
fn lcd_grid() -> Color32 {
    look::mix(look::LCD, look::LCD_INK, 0.13)
}
/// The shading under a curve on an LCD.
fn lcd_fill() -> Color32 {
    look::LCD_INK.gamma_multiply(0.09)
}

/// A draggable point on an LCD: dark ink; `solid` = filled, else a ring.
fn dot(painter: &egui::Painter, at: egui::Pos2, solid: bool, hot: bool) {
    if hot {
        painter.circle_stroke(at, 8.5, stroke(1.2, look::LCD_INK_DIM));
    }
    if solid {
        painter.circle_filled(at, 4.5, look::LCD_INK);
    } else {
        painter.circle_filled(at, 4.5, look::LCD);
        painter.circle_stroke(at, 4.0, stroke(1.8, look::LCD_INK));
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
                ed.host.begin_edit();
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
    look::lcd_panel(&painter, rect);
    let ink = if enabled { look::LCD_INK } else { look::LCD_INK_DIM };
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
