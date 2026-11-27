//! # octavia-editor
//!
//! Octavia's faceplate in Lyra, the port of `docs/design/faceplate.html`
//! (approved 2026-11-25; `docs/FACEPLATE.md` says what's on it and why).
//! It draws only: it reads and changes parameters through Lyra's plugin kit
//! ([`lyra_kit::Host`], one undo step per gesture) and reads the engine's
//! [`octavia_lyra::Telemetry`]; it never touches the engine.
//!
//! ```text
//!  ┌──────────────────────────────────────────────────────────┐
//!  │ [logo]                                  [CELLO|VIOLIN]   │
//!  │ ┌─bezel──────────────────────┐  STYLE · decoration ───── │
//!  │ │ the LCD (lcd.rs)            │  (expr) (vib) (breath)    │
//!  │ │                             │  [      PERFORMER      ]  │
//!  │ │                             │  SCREEN ──────────────── │
//!  │ └─────────────────────────────┘  [PERF|LOG]  take [−][+] │
//!  │ PLAYER · ability ──────────────  VOICE ───────────────── │
//!  │ [CONCERT ]  ┃   ┃   ┃   ┃        (character) [DOUBLE]    │
//!  │ [STUDENT ]  ┃   ┃   ┃   ┃         natural bright [STOPS]  │
//!  │ [VIRTUOSO]  ┃   ┃   ┃   ┃        [ARCO | PIZZ | TREM]     │
//!  │ [SESSION ]  intonation dexterity bowing composure        │
//!  │ rhodune! lab                                             │
//!  └──────────────────────────────────────────────────────────┘
//! ```
//!
//! The plate has a fixed size (about 790×610 at Lyra's zoom 1), laid out in
//! the HTML's own CSS arithmetic so the two can be checked side by side.

mod lcd;
mod look;

use egui::{pos2, vec2, CursorIcon, Id, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};
use look::{Align, ButtonState};
use lyra_kit::assets::Artwork;
use lyra_kit::{knob_turn, tracked, wheel_step, Host};
use lyra_plugin::ParamInfo;
use octavia_core::telemetry::StringSound;
use octavia_lyra::PARAMS;
use std::sync::Arc;

/// Octavia's editor, for Lyra's list (`plugin_host::EDITORS`).
pub const EDITOR: lyra_kit::Editor = lyra_kit::Editor { plugin: octavia_lyra::ID, show, fonts: look::add_fonts };

/// The logo, Rhodexa's art on the palette (`scripts/recolor.py` makes it).
/// Re-export it while Lyra runs from the source tree and the plate shows it
/// within a second.
const LOGO: Artwork = Artwork {
    name: "octavia-logo",
    built_in: include_bytes!("../../docs/design/logo.palette.png"),
    file: concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/design/logo.palette.png"),
    // About twice its size on the plate: scaled down further by the GPU it
    // shimmered and looked jagged.
    max_h: 84,
};

// ───────────────────────────────────────────────────────────── layout
// The plate's CSS, in points: 2 px outline, padding 24 28 22, two columns
// (the bezel; style, screen, voice) 30 apart, rows 6 apart.

const DROP: f32 = 6.0;
const X1: f32 = 30.0;
const COL1: f32 = 459.0;
const X2: f32 = X1 + COL1 + 30.0;
const COL2: f32 = 234.0;
const RIGHT: f32 = X2 + COL2;
const PLATE_W: f32 = RIGHT + 30.0;
const HEAD_Y: f32 = 26.0;
const HEAD_H: f32 = 54.0;
const BODY_Y: f32 = HEAD_Y + HEAD_H + 14.0;
const ROW2_Y: f32 = BODY_Y + 252.0 + 6.0;
const FOOT_Y: f32 = ROW2_Y + 16.0 + 20.0 + 166.0 + 18.0;
const PLATE_H: f32 = FOOT_Y + 8.0 + 24.0;
const KNOB: f32 = 58.0;
const FADER_TRAVEL: f32 = 128.0;
const BTN_H: f32 = 30.0;
/// A switch's track around its buttons: outline + padding.
const TRACK: f32 = look::OL + 3.0;

/// Four letters for the LCD when a control is touched.
fn abbr(id: u32) -> &'static str {
    use octavia_lyra::*;
    match id {
        INTONATION => "INTN",
        DEXTERITY => "DEXT",
        BOWING => "BOWG",
        COMPOSURE => "CALM",
        EXPRESSION => "EXPR",
        VIBRATO => "VIBR",
        BREATH => "BRTH",
        CHARACTER => "CHAR",
        _ => "",
    }
}

/// One frame of the plate: where it is and who it talks to.
struct Plate<'a> {
    ui: &'a mut Ui,
    host: &'a mut dyn Host,
    painter: Painter,
    origin: Pos2,
    now: f64,
}

impl Plate<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, y)
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(self.at(x, y), vec2(w, h))
    }

    fn id(&self, what: impl std::hash::Hash) -> Id {
        Id::new(("octavia", self.host.instance(), what))
    }

    fn param(&self, id: u32) -> f32 {
        self.host.param(id as usize)
    }

    fn on(&self, id: u32) -> bool {
        self.param(id) >= 0.5
    }

    /// One click, one undo step.
    fn set(&mut self, id: u32, v: f32) {
        self.host.begin_edit();
        self.host.set_param(id as usize, v);
    }

    /// Shows `text` on the LCD's top line for 1.6 s.
    fn say(&mut self, text: String) {
        let until = self.now + 1.6;
        let id = self.id("message");
        self.ui.ctx().data_mut(|d| d.insert_temp(id, (text, until)));
    }

    fn message(&self) -> Option<String> {
        let id = self.id("message");
        let (text, until): (String, f64) = self.ui.ctx().data(|d| d.get_temp(id))?;
        (self.now < until).then_some(text)
    }

    fn touched(&mut self, id: u32, v: f32) {
        self.say(format!("{} {v:.2}", abbr(id)));
    }

    /// A lone button: casts a hard shadow, drops into it when pressed.
    fn solo(&mut self, r: Rect, what: &str, lines: &[&str], on: bool) -> bool {
        let resp = self.ui.interact(r, self.id(what), Sense::click());
        let s = ButtonState { on, hovered: resp.hovered(), pressed: resp.is_pointer_button_down_on(), solo: true };
        look::button(&self.painter, r, lines, s, 10.0);
        resp.clicked()
    }

    /// Buttons in a sunken track; returns the one clicked.
    fn switch(&mut self, what: &str, rects: &[Rect], labels: &[&str], chosen: Option<usize>) -> Option<usize> {
        let all = rects.iter().skip(1).fold(rects[0], |a, b| a.union(*b));
        look::track(&self.painter, all.expand(TRACK));
        let mut clicked = None;
        for (i, (r, text)) in rects.iter().zip(labels).enumerate() {
            let resp = self.ui.interact(*r, self.id((what, i)), Sense::click());
            let s = ButtonState { on: chosen == Some(i), hovered: resp.hovered(), pressed: false, solo: false };
            look::button(&self.painter, *r, &[text], s, 10.0);
            if resp.clicked() {
                clicked = Some(i);
            }
        }
        clicked
    }

    /// A row of buttons `widths` wide from (x, y), 3 apart.
    fn row(&self, x: f32, y: f32, widths: &[f32]) -> Vec<Rect> {
        let mut x = x;
        widths
            .iter()
            .map(|&w| {
                let r = self.rect(x, y, w, BTN_H);
                x += w + 3.0;
                r
            })
            .collect()
    }

    /// A knob with its caption, `w` wide from `x`, its top at `top`.
    fn knob(&mut self, id: u32, x: f32, w: f32, top: f32) {
        let info: &ParamInfo = &PARAMS[id as usize];
        let r = self.rect(x + (w - KNOB) / 2.0, top, KNOB, KNOB);
        let resp = self.ui.interact(r, self.id(("knob", id)), Sense::click_and_drag());
        let mut v = self.param(id);
        let (resp, hot) = knob_turn(self.ui, resp, &mut v, info);
        if tracked(&resp, self.host) {
            self.host.set_param(id as usize, v);
        }
        if hot {
            self.touched(id, v);
        }
        resp.on_hover_cursor(CursorIcon::ResizeVertical);
        look::knob(&self.painter, r, info.to_normalized(v), hot);
        let name = info.name;
        look::caption(
            &self.painter,
            name,
            self.at(x + w / 2.0, 0.0).x,
            self.at(0.0, top + KNOB + 7.0).y,
            Align::Center,
            false,
            0.12,
        );
    }

    /// A fader with its caption, 66 wide from `x`, its top at `top`.
    /// The cap follows the mouse (Shift = fine), the wheel nudges 5 % (Shift 1 %),
    /// double-click resets.
    fn fader(&mut self, id: u32, x: f32, top: f32) {
        let info: &ParamInfo = &PARAMS[id as usize];
        let at = self.at(x + (66.0 - look::FADER_W) / 2.0, top);
        let r = Rect::from_min_size(at, vec2(look::FADER_W, look::fader_h(FADER_TRAVEL)));
        let mut resp = self.ui.interact(r, self.id(("fader", id)), Sense::click_and_drag());
        let before = self.param(id);
        let n0 = info.to_normalized(before);
        let mut n = n0;
        let (fine, raw_id) = (self.ui.input(|i| i.modifiers.shift), resp.id.with("raw"));
        if resp.dragged() {
            // Its own raw position while dragging, so slow drags aren't lost.
            let raw: f32 = if resp.drag_started() { n0 } else { self.ui.data(|d| d.get_temp(raw_id)).unwrap_or(n0) };
            n = (raw - resp.drag_delta().y / FADER_TRAVEL * if fine { 0.1 } else { 1.0 }).clamp(0.0, 1.0);
            self.ui.data_mut(|d| d.insert_temp(raw_id, n));
        }
        if let Some(step) = wheel_step(self.ui, r) {
            n = (n - step as f32 * if fine { 0.01 } else { 0.05 }).clamp(0.0, 1.0);
        }
        let mut v = if n != n0 { info.from_normalized(n) } else { before };
        if resp.double_clicked() {
            v = info.default;
        }
        if v != before {
            resp.mark_changed();
        }
        if tracked(&resp, self.host) {
            self.host.set_param(id as usize, v);
        }
        let hot = resp.hovered() || resp.dragged();
        if hot {
            self.touched(id, v);
        }
        resp.on_hover_cursor(CursorIcon::ResizeVertical);
        look::fader(&self.painter, at, FADER_TRAVEL, info.to_normalized(v), hot);
        let cap_top = top + look::fader_h(FADER_TRAVEL) + 7.0;
        look::caption(
            &self.painter,
            info.name,
            self.at(x + 33.0, 0.0).x,
            self.at(0.0, cap_top).y,
            Align::Center,
            false,
            0.06,
        );
    }

    fn section(&self, x: f32, top: f32, right: f32, name: &str, sub: Option<&str>) {
        look::section(&self.painter, self.at(x, 0.0).x, self.at(0.0, top).y, self.at(right, 0.0).x, name, sub);
    }

    fn caption(&self, text: &str, x: f32, top: f32, align: Align, dim: bool) {
        let em = if dim { 0.08 } else { 0.12 };
        look::caption(&self.painter, text, self.at(x, 0.0).x, self.at(0.0, top).y, align, dim, em);
    }
}

/// The player profile the skill faders are exactly, if any.
fn profile(host: &dyn Host) -> Option<&'static str> {
    let presets = octavia_lyra::PLUGINS[0].presets;
    presets.iter().find(|(_, vals)| vals.iter().all(|&(i, v)| (host.param(i) - v).abs() < 1e-4)).map(|(n, _)| *n)
}

fn show(ui: &mut Ui, host: &mut dyn Host) {
    let (rect, _) = ui.allocate_exact_size(vec2(PLATE_W + DROP, PLATE_H + DROP), Sense::hover());
    let painter = ui.painter_at(rect);
    let now = ui.input(|i| i.time);
    let mut p = Plate { ui, host, painter, origin: rect.min, now };
    use octavia_lyra::*;

    // ── The plate: a hard shadow, the coat, the purfling in her violin's woods ──
    let plate = p.rect(0.0, 0.0, PLATE_W, PLATE_H);
    p.painter.rect_filled(plate.translate(vec2(DROP, DROP)), 0.0, look::MANE_OUTLINE);
    look::block(&p.painter, plate, look::CHASSIS);
    let purf = plate.shrink(look::OL + 7.0);
    p.painter.rect_stroke(purf, egui::CornerRadius::ZERO, Stroke::new(3.0_f32, look::VIOLIN_WOOD), StrokeKind::Inside);
    p.painter.rect_stroke(
        purf.shrink(3.0 + 2.0),
        egui::CornerRadius::ZERO,
        Stroke::new(1.8_f32, look::VIOLIN_BODY),
        StrokeKind::Inside,
    );

    // ── Head: the logo, a sticker; the instrument ──
    let (tex, size) = LOGO.texture(p.ui.ctx());
    let h = 41.0;
    let logo = Rect::from_min_size(
        p.at(29.0, HEAD_Y + HEAD_H / 2.0 - h / 2.0 - 1.0),
        vec2(h * size[0] as f32 / size[1].max(1) as f32, h),
    );
    p.painter.image(tex, logo, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
    let names = ["cello", "violin"];
    let widths: Vec<f32> = names.iter().map(|n| look::button_w(&p.painter, n, 58.0, 12.0)).collect();
    let x = RIGHT - TRACK - widths.iter().sum::<f32>() - 3.0;
    let rects = p.row(x, HEAD_Y + HEAD_H / 2.0 - BTN_H / 2.0, &widths);
    let violin_param = p.param(INSTRUMENT) >= 0.5;
    if let Some(i) = p.switch("instrument", &rects, &names, Some(violin_param as usize)) {
        p.set(INSTRUMENT, i as f32);
        p.say(names[i].to_uppercase());
    }

    // ── The screen ──
    let bezel = p.rect(X1, BODY_Y, COL1, 252.0);
    look::block(&p.painter, bezel, look::LCD_BEZEL);
    let glass = Rect::from_min_size(bezel.min + vec2(12.0, 12.0), lcd::size());
    p.painter.rect_stroke(
        glass,
        egui::CornerRadius::ZERO,
        Stroke::new(look::OL, look::CHASSIS_INSET),
        StrokeKind::Outside,
    );
    p.painter.rect_filled(glass, 0.0, look::LCD[0]);
    let view = gather(&mut p);
    let glass_painter = p.painter.with_clip_rect(glass);
    let screen = lcd::Lcd::new(&glass_painter, glass.min, glass.width() / lcd::W);
    lcd::draw(&screen, &view);

    // ── Style · decoration ──
    p.section(X2, BODY_Y, RIGHT, "style", Some("decoration"));
    let mut x = X2;
    for id in [EXPRESSION, VIBRATO, BREATH] {
        let w = look::label_w(&p.painter, PARAMS[id as usize].name, 9.0, 0.12, false).max(KNOB);
        p.knob(id, x, w, BODY_Y + 20.0);
        x += w + 6.0;
    }
    let perf_on = p.on(PERFORMER);
    if p.solo(p.rect(X2, BODY_Y + 108.0, COL2, 34.0), "performer", &["performer"], perf_on) {
        p.set(PERFORMER, if perf_on { 0.0 } else { 1.0 });
        p.say(if perf_on { "PERF OFF" } else { "PERF ON" }.into());
    }

    // ── Screen: the page, the take ──
    let y = BODY_Y + 158.0;
    p.section(X2, y, RIGHT, "screen", None);
    let page_id = p.id("page");
    let log = p.ui.ctx().data(|d| d.get_temp::<bool>(page_id)).unwrap_or(false);
    let pages = ["perf", "log"];
    let widths: Vec<f32> = pages.iter().map(|n| look::button_w(&p.painter, n, 58.0, 12.0)).collect();
    let rects = p.row(X2 + TRACK, y + 20.0 + TRACK, &widths);
    if let Some(i) = p.switch("pages", &rects, &pages, Some(log as usize)) {
        p.ui.ctx().data_mut(|d| d.insert_temp(page_id, i == 1));
    }
    let mid = y + 20.0 + 20.0;
    let take = p.param(TAKE).round();
    let mut step = 0.0;
    if p.solo(p.rect(RIGHT - 30.0, mid - 12.0, 30.0, 24.0), "take+", &["+"], false) {
        step = 1.0;
    }
    if p.solo(p.rect(RIGHT - 68.0, mid - 12.0, 30.0, 24.0), "take-", &["−"], false) {
        step = -1.0;
    }
    if step != 0.0 {
        let t = (take + step).clamp(1.0, 99.0);
        p.set(TAKE, t);
        p.say(format!("TAKE {:02}", t as u32));
    }
    p.caption("take", RIGHT - 76.0, mid - 4.5, Align::Right, true);

    // ── Player · ability: the profiles (presets) and the four skills ──
    p.section(X1, ROW2_Y + 16.0, X1 + COL1, "player", Some("ability"));
    let top = ROW2_Y + 36.0;
    let players = ["concert", "student", "virtuoso", "session"];
    let rects: Vec<Rect> =
        (0..4).map(|i| p.rect(X1 + TRACK, top + TRACK + i as f32 * (BTN_H + 3.0), 118.0, BTN_H)).collect();
    let chosen = profile(p.host).and_then(|n| players.iter().position(|q| q.eq_ignore_ascii_case(n)));
    if let Some(i) = p.switch("player", &rects, &players, chosen) {
        let presets = octavia_lyra::PLUGINS[0].presets;
        if let Some((_, vals)) = presets.iter().find(|(n, _)| n.eq_ignore_ascii_case(players[i])) {
            p.host.begin_edit();
            for &(id, v) in vals.iter() {
                p.host.set_param(id, v);
            }
        }
        p.say(players[i].to_uppercase());
    }
    let fx = X1 + 118.0 + 2.0 * TRACK + 22.0;
    for (i, id) in [INTONATION, DEXTERITY, BOWING, COMPOSURE].into_iter().enumerate() {
        p.fader(id, fx + i as f32 * (66.0 + 6.0), top);
    }

    // ── Voice ──
    p.section(X2, ROW2_Y + 16.0, RIGHT, "voice", None);
    let ends = look::label_w(&p.painter, "natural", 9.0, 0.08, true)
        + 8.0
        + look::label_w(&p.painter, "bright", 9.0, 0.08, true);
    let kw = ends.max(look::label_w(&p.painter, "character", 9.0, 0.12, false)).max(KNOB);
    p.knob(CHARACTER, X2, kw, top);
    let ends_top = top + KNOB + 7.0 + 9.0 + 4.0;
    p.caption("natural", X2, ends_top, Align::Left, true);
    p.caption("bright", X2 + kw, ends_top, Align::Right, true);
    let row_h = KNOB + 7.0 + 9.0 + 4.0 + 9.0;
    let bw = look::button_w(&p.painter, "double", 58.0, 12.0);
    let cw = bw.max(look::label_w(&p.painter, "two strings", 9.0, 0.08, true));
    let dx = X2 + kw + 16.0;
    let dtop = top + (row_h - (34.0 + 7.0 + 9.0)) / 2.0;
    let doubles = p.on(DOUBLE_STOPS);
    if p.solo(p.rect(dx + (cw - bw) / 2.0, dtop, bw, 34.0), "doubles", &["double", "stops"], doubles) {
        p.set(DOUBLE_STOPS, if doubles { 0.0 } else { 1.0 });
        p.say(if doubles { "DBL OFF" } else { "DBL ON" }.into());
    }
    p.caption("two strings", dx + cw / 2.0, dtop + 34.0 + 7.0, Align::Center, true);
    let ptop = top + row_h + 12.0;
    let patches = ["arco", "pizz", "trem"];
    let pw = (COL2 - 2.0 * TRACK - 6.0) / 3.0;
    let rects = p.row(X2 + TRACK, ptop + TRACK, &[pw; 3]);
    let patch = (p.param(PATCH).round() as usize).min(2);
    if let Some(i) = p.switch("patch", &rects, &patches, Some(patch)) {
        p.set(PATCH, i as f32);
        p.say(patches[i].to_uppercase());
    }
    p.caption("patch", X2 + COL2 / 2.0, ptop + 40.0 + 7.0, Align::Center, true);

    // ── Foot ──
    let foot = p.at(X1, look::caps_baseline(FOOT_Y, 8.0));
    look::label(&p.painter, "rhodune! lab", foot, Align::Left, 8.0, 0.2, false, look::TEXT_SECONDARY);

    // Moving while something plays or a message shows; otherwise look in
    // now and then for the song starting.
    let busy = view.active || p.message().is_some() || (view.hand - view.snap.hand).abs() > 0.01;
    if busy {
        p.ui.ctx().request_repaint();
    } else {
        p.ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// What the screen shows this frame: telemetry, parameters, and the little
/// the plate remembers itself (the hand's glide, a slide's trail, the page).
fn gather(p: &mut Plate) -> lcd::View {
    use octavia_lyra::*;
    let tele = p.host.telemetry().and_then(|t| t.downcast::<Telemetry>().ok());
    let snap = tele.as_ref().map(|t| t.read()).unwrap_or_default();
    let violin = tele.as_ref().map_or(p.param(INSTRUMENT) >= 0.5, |t: &Arc<Telemetry>| t.instrument() == 1);
    let active = snap.frame.loudness > 0.02
        || snap.strings.iter().any(|s| s.sound != StringSound::Silent)
        || snap.upcoming[0].is_some();

    // The hand glides to where it is.
    let hand_id = p.id("hand");
    let dt = p.ui.input(|i| i.stable_dt).min(0.1);
    let hand = match p.ui.ctx().data(|d| d.get_temp::<f32>(hand_id)) {
        Some(h) => h + (snap.hand - h) * (1.0 - (-dt * 14.0).exp()),
        None => snap.hand,
    };
    p.ui.ctx().data_mut(|d| d.insert_temp(hand_id, hand));

    // A slide leaves a trail that fades over half a second after it lands.
    let trail_id = p.id("trail");
    let sliding = snap.strings.iter().enumerate().find_map(|(s, st)| Some((s, st.sliding_from?, st.finger?)));
    if let Some((s, from, to)) = sliding {
        p.ui.ctx().data_mut(|d| d.insert_temp(trail_id, (s, from, to, p.now)));
    }
    let trail = p.ui.ctx().data(|d| d.get_temp::<(usize, f32, f32, f64)>(trail_id)).and_then(|(s, from, to, seen)| {
        let age = ((p.now - seen) / 0.5) as f32;
        (age < 1.0).then_some((s, from, to, age))
    });

    let performer = p.on(PERFORMER);
    let idle = if !performer { "PERF OFF".to_string() } else { profile(p.host).unwrap_or("custom").to_string() };
    let log = p.ui.ctx().data(|d| d.get_temp::<bool>(p.id("page"))).unwrap_or(false);
    lcd::View {
        snap,
        violin,
        message: p.message().unwrap_or(idle),
        take: p.param(TAKE).round().clamp(1.0, 99.0) as u32,
        patch: (p.param(PATCH).round() as usize).min(2),
        doubles: p.on(DOUBLE_STOPS),
        performer,
        log,
        active,
        hand,
        trail,
    }
}
