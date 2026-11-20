//! The automation editor: one automation's curve, one loop of it.
//!
//! * **click** empty space — a new point (snapped; **Alt** — free) ·
//!   **drag** a point — move it · **right-click** a point — remove it
//! * **the ring** between two points — bend that line (drag up and down)
//! * **the line at the end** — the curve's length (clips longer than it
//!   repeat it); drag it
//! * **outputs** (the rack at the left) — what the curve moves, one plate
//!   each: the control, its mapping (the curve's 0..1 to the control's
//!   **from**..**to**, drag them up and down, double-click to reset; ⇅ turns
//!   it around), × to stop moving it. **Link** at the bottom: the curve moves
//!   the control you changed last too
//! * header: the name, snap
//! * **Shift+wheel** (or the wheel over the ruler) — zoom · **Ctrl+wheel** —
//!   sideways · **middle-drag** — pan
//!
//! The curve is knob travel, 0 at the bottom, 1 at the top; each control it
//! moves turns its own knob that far (the readout shows the first one's value).

use crate::app::UiState;
use crate::flex::{self, Grow, Px};
use crate::keys::Action;
use crate::playlist::SNAPS;
use crate::theme::{self, cr, stroke};
use crate::timeline::{self, TimeView, Wheel};
use crate::waveform;
use crate::widgets;
use egui::{pos2, vec2, Align2, CursorIcon, FontId, Pos2, Rect, RichText, Sense, Ui};
use lyra_model::{AutomationId, History, Point, Project, Source, TICKS_PER_BAR, TICKS_PER_STEP};

const HANDLE_R: f32 = 10.0;
/// The outputs rack at the left.
const RACK_W: f32 = 236.0;
const PLATE_H: f32 = 58.0;
/// Lines narrower than this show no ring.
const RING_MIN_W: f32 = 24.0;

enum Drag {
    None,
    Point { index: usize },
    Tension { index: usize, orig: f32, anchor_y: f32 },
    Length { orig: u32, anchor_tick: f32 },
    Hand { last: Pos2 },
}

pub struct AutomationEditor {
    time: TimeView,
    framed: Option<AutomationId>,
    drag: Drag,
    pub snap: u32,
}

impl Default for AutomationEditor {
    fn default() -> Self {
        Self { time: TimeView { scroll: 0.0, ppt: 0.5 }, framed: None, drag: Drag::None, snap: TICKS_PER_STEP }
    }
}

/// The automation the editor shows: the current one, or the first.
fn current(p: &Project, st: &mut UiState) -> Option<usize> {
    let i = st.automation.and_then(|id| p.automation_index(id)).or((!p.automations.is_empty()).then_some(0))?;
    st.automation = Some(p.automations[i].id);
    Some(i)
}

/// Screen ↔ curve for one frame.
#[derive(Clone, Copy)]
struct View {
    grid: Rect,
    time: TimeView,
}

impl View {
    fn x(&self, tick: f32) -> f32 {
        self.time.x(self.grid, tick)
    }
    fn tick(&self, x: f32) -> f32 {
        self.time.tick(self.grid, x)
    }
    /// The curve's 0..1 fills the grid, less a margin so the ends can be grabbed.
    fn area(&self) -> Rect {
        self.grid.shrink2(vec2(0.0, 10.0))
    }
    fn y(&self, v: f32) -> f32 {
        let a = self.area();
        a.bottom() - v.clamp(0.0, 1.0) * a.height()
    }
    fn value(&self, y: f32) -> f32 {
        let a = self.area();
        ((a.bottom() - y) / a.height()).clamp(0.0, 1.0)
    }
    fn at(&self, p: &Point) -> Pos2 {
        pos2(self.x(p.tick as f32), self.y(p.value))
    }
}

/// Where the ring of the line from point `i` to `i + 1` sits (None: too
/// short, or flat, so nothing to bend).
fn ring(v: &View, pts: &[Point], i: usize) -> Option<Pos2> {
    let (a, b) = (pts.get(i)?, pts.get(i + 1)?);
    let (xa, xb) = (v.x(a.tick as f32), v.x(b.tick as f32));
    (xb - xa >= RING_MIN_W && (a.value - b.value).abs() > 1e-3).then(|| {
        let mid = a.value + (b.value - a.value) * lyra_engine::fade_curve(0.5, a.tension);
        pos2((xa + xb) * 0.5, v.y(mid))
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Hover {
    Point(usize),
    Ring(usize),
    Length,
}

fn hover(v: &View, a: &lyra_model::Automation, pos: Pos2) -> Option<Hover> {
    if let Some(i) = (0..a.points.len()).rev().find(|&i| v.at(&a.points[i]).distance(pos) <= HANDLE_R) {
        return Some(Hover::Point(i));
    }
    if let Some(i) = (0..a.points.len()).find(|&i| ring(v, &a.points, i).is_some_and(|r| r.distance(pos) <= HANDLE_R)) {
        return Some(Hover::Ring(i));
    }
    ((pos.x - v.x(a.length as f32)).abs() <= 5.0).then_some(Hover::Length)
}

/// The outputs rack: one plate per control the curve moves, then Link.
fn outputs_rack(
    ui: &mut Ui,
    r: Rect,
    p: &mut Project,
    h: &mut History,
    st: &UiState,
    ai: usize,
    out: &mut Vec<Action>,
) {
    let m = theme::m();
    ui.painter().rect_filled(r, cr(0), theme::BG);
    let light = theme::channel_light(p.automations[ai].color);
    let n = p.automations[ai].links.len();
    let mut remove = None;
    let mut y = r.top();
    for k in 0..n {
        let plate = Rect::from_min_size(pos2(r.left(), y), vec2(r.width(), PLATE_H));
        y += PLATE_H + theme::SEAM;
        if plate.bottom() > r.bottom() {
            break;
        }
        ui.painter().rect_filled(plate, cr(0), theme::PANEL);
        let inner = plate.shrink2(vec2(8.0, 6.0));
        let [title, map] = flex::col(inner, [Px(16.0), Grow(1.0)], 4.0);
        let link = p.automations[ai].links[k];
        let control = crate::automation::control(p, link.target);
        // The control's name, and × (when there's more than one).
        let [name_r, x_r] = flex::row(title, [Grow(1.0), Px(if n > 1 { 16.0 } else { 0.0 })], 4.0);
        let name = control.as_ref().map_or("(gone)".to_string(), |c| c.2.clone());
        ui.painter().with_clip_rect(name_r).text(
            name_r.left_center(),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(m.text_small),
            theme::TEXT,
        );
        if n > 1 {
            let resp = ui.interact(x_r, ui.id().with(("auto-out-x", k)), Sense::click());
            let ink = if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
            ui.painter().text(x_r.center(), Align2::CENTER_CENTER, "×", FontId::proportional(m.text), ink);
            if resp.on_hover_text("Stop moving this one").clicked() {
                remove = Some(k);
            }
        }
        // The mapping: a picture of curve → control, from, to, and turn around.
        let [graph, from_r, to_r, flip_r] =
            flex::row(map, [Px(map.height() * 1.4), Grow(1.0), Grow(1.0), Px(map.height())], 4.0);
        theme::well(ui.painter(), graph);
        let g = graph.shrink(4.0);
        let gy = |t: f32| g.bottom() - t * g.height();
        let (a, b) = (pos2(g.left(), gy(link.from)), pos2(g.right(), gy(link.to)));
        ui.painter().line_segment([a, b], stroke(1.5, light.hi));
        ui.painter().circle_filled(a, 2.0, light.hi);
        ui.painter().circle_filled(b, 2.0, light.hi);
        let fmt =
            |t: f32| control.as_ref().map_or(format!("{:.0}%", t * 100.0), |c| c.0.format(c.0.from_normalized(t)));
        let mut changed = None;
        for (end, cell, value, reset) in [(0, from_r, link.from, 0.0), (1, to_r, link.to, 1.0)] {
            let resp = ui.interact(cell, ui.id().with(("auto-out", k, end)), Sense::click_and_drag());
            if resp.drag_started() || resp.double_clicked() {
                h.checkpoint(p);
            }
            let mut v = value;
            if resp.dragged() {
                let fine = ui.input(|i| i.modifiers.shift);
                v = (v - resp.drag_delta().y * if fine { 0.001 } else { 0.004 }).clamp(0.0, 1.0);
            }
            if resp.double_clicked() {
                v = reset;
            }
            if v != value {
                changed = Some((end, v));
            }
            theme::well(ui.painter(), cell);
            let hot = resp.hovered() || resp.dragged();
            ui.painter().with_clip_rect(cell).text(
                cell.center(),
                Align2::CENTER_CENTER,
                fmt(v),
                FontId::monospace(m.text_small),
                if hot { theme::TEXT } else { theme::TEXT_DIM },
            );
            let hint =
                if end == 0 { "Where the curve's bottom puts the control" } else { "Where the curve's top puts it" };
            resp.on_hover_text(format!("{hint} · drag up/down (Shift: fine) · double-click: reset"));
        }
        if let Some((end, v)) = changed {
            let l = &mut p.automations[ai].links[k];
            if end == 0 {
                l.from = v;
            } else {
                l.to = v;
            }
        }
        let flip = ui.interact(flip_r, ui.id().with(("auto-out-flip", k)), Sense::click());
        theme::well(ui.painter(), flip_r);
        let ink = if flip.hovered() { theme::TEXT } else { theme::TEXT_DIM };
        // Two arrows, up and down: "turn it around".
        let c = flip_r.center();
        let s = flip_r.height() * 0.07;
        for (dx, dir) in [(-3.0f32, -1.0f32), (3.0, 1.0)] {
            let x = c.x + dx * s;
            ui.painter().line_segment([pos2(x, c.y - 5.0 * s), pos2(x, c.y + 5.0 * s)], stroke(1.3, ink));
            let tip = pos2(x, c.y + 6.0 * s * dir);
            let tri = vec![tip, pos2(x - 2.4 * s, c.y + 2.5 * s * dir), pos2(x + 2.4 * s, c.y + 2.5 * s * dir)];
            ui.painter().add(egui::Shape::convex_polygon(tri, ink, egui::Stroke::NONE));
        }
        if flip.on_hover_text("Turn it around (up on the curve = down on the control)").clicked() {
            h.checkpoint(p);
            let l = &mut p.automations[ai].links[k];
            std::mem::swap(&mut l.from, &mut l.to);
        }
    }
    if let Some(k) = remove {
        h.checkpoint(p);
        p.automations[ai].links.remove(k);
    }
    // Link: the curve moves the control changed last too.
    let key = Rect::from_min_size(pos2(r.left() + 8.0, y + 4.0), vec2(r.width() - 16.0, m.control_h));
    if key.bottom() <= r.bottom() {
        let hint = match st.last_tweaked.and_then(|t| crate::automation::control(p, t)) {
            Some((_, _, name)) => format!("This curve moves {name} too (the control you changed last)"),
            None => "Change a control, then Link: this curve moves it too".to_string(),
        };
        let resp = ui.put(key, |ui: &mut Ui| {
            widgets::key(ui, key.size(), widgets::Legend::Text("+ Link"), false, theme::L_MINT, &hint)
        });
        if resp.clicked() {
            out.push(Action::LinkTweaked);
        }
    }
}

impl AutomationEditor {
    /// The header: name, snap.
    pub fn toolbar(&mut self, ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
        let Some(i) = current(p, st) else { return };
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 4.0, theme::channel_color(p.automations[i].color));
        if let Some(r) = widgets::name_field(ui, &p.automations[i].name, 140.0) {
            if r.first {
                h.checkpoint(p);
            }
            p.automations[i].name = r.text;
        }
        ui.add_space(10.0);
        ui.label(RichText::new("snap").small().color(theme::TEXT_DIM));
        let name = SNAPS.iter().find(|s| s.1 == self.snap).map_or("?", |s| s.0);
        egui::ComboBox::from_id_salt("auto-snap").width(64.0).selected_text(name).show_ui(ui, |ui| {
            for (n, t) in SNAPS {
                ui.selectable_value(&mut self.snap, t, n);
            }
        });
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        p: &mut Project,
        h: &mut History,
        st: &mut UiState,
        playhead: Option<f64>,
        out: &mut Vec<Action>,
    ) {
        let m = theme::m();
        let Some(ai) = current(p, st) else {
            ui.label(
                RichText::new("Change a control, then press Automate in the arrangement's header.")
                    .color(theme::TEXT_DIM),
            );
            return;
        };
        let size = ui.available_size().max(vec2(320.0, 140.0));
        let (all, _) = ui.allocate_exact_size(size, Sense::hover());
        let [outputs, rect] = flex::row(all, [Px(RACK_W), Grow(1.0)], theme::SEAM);
        outputs_rack(ui, outputs, p, h, st, ai, out);
        let ruler = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + m.ruler_h));
        let grid = Rect::from_min_max(pos2(rect.left(), ruler.bottom()), rect.max);
        let id = p.automations[ai].id;
        let length = p.automations[ai].length as f32;
        if self.framed != Some(id) {
            self.time = TimeView { scroll: 0.0, ppt: grid.width() / (length * 1.08).max(1.0) };
            self.framed = Some(id);
        }

        let input = timeline::Input::read(ui, rect);
        let timeline::Input { pos, pressed, pressed_right, pressed_middle, down, mods, hovering, .. } = input;
        if hovering {
            match timeline::wheel(&input, ruler) {
                Wheel::ZoomTime { factor, at } => self.time.zoom_at(grid, factor, at, 0.02, 8.0),
                Wheel::Scroll { x, .. } => self.time.scroll = (self.time.scroll + x / self.time.ppt).max(0.0),
                _ => {}
            }
        }
        let v = View { grid, time: self.time };
        let snap = if mods.alt { 1 } else { self.snap };
        let to_tick = |x: f32| timeline::snap(v.tick(x), snap).max(0.0) as u32;

        // ── Mouse ──
        if hovering && pressed_middle {
            if let Some(pos) = pos {
                self.drag = Drag::Hand { last: pos };
            }
        } else if hovering && (pressed || pressed_right) && pos.is_some_and(|p| grid.contains(p)) {
            let pos = pos.unwrap_or_default();
            let a = &p.automations[ai];
            match (hover(&v, a, pos), pressed) {
                (Some(Hover::Point(i)), false) => {
                    if a.points.len() > 1 {
                        h.checkpoint(p);
                        p.automations[ai].points.remove(i);
                    }
                }
                (Some(Hover::Point(index)), true) => {
                    h.checkpoint(p);
                    self.drag = Drag::Point { index };
                }
                (Some(Hover::Ring(index)), true) => {
                    h.checkpoint(p);
                    self.drag = Drag::Tension { index, orig: a.points[index].tension, anchor_y: pos.y };
                }
                (Some(Hover::Length), true) => {
                    h.checkpoint(p);
                    self.drag = Drag::Length { orig: a.length, anchor_tick: v.tick(pos.x) };
                }
                (None, true) => {
                    // A new point, kept in order, and dragged right away.
                    h.checkpoint(p);
                    let tick = to_tick(pos.x).min(a.length);
                    let points = &mut p.automations[ai].points;
                    let index = points.partition_point(|q| q.tick <= tick);
                    points.insert(index, Point { tick, value: v.value(pos.y), tension: 0.0 });
                    self.drag = Drag::Point { index };
                }
                _ => {}
            }
        }
        if down {
            if let Some(pos) = pos {
                let a = &mut p.automations[ai];
                match &mut self.drag {
                    Drag::Point { index } => {
                        let i = *index;
                        // Between its neighbours (on the same tick = a jump).
                        let lo = if i > 0 { a.points[i - 1].tick } else { 0 };
                        let hi = a.points.get(i + 1).map_or(a.length, |q| q.tick);
                        let tick = to_tick(pos.x).clamp(lo, hi);
                        a.points[i].tick = tick;
                        a.points[i].value = v.value(pos.y);
                    }
                    Drag::Tension { index, orig, anchor_y } => {
                        let i = *index;
                        if let (Some(pa), Some(pb)) = (a.points.get(i).copied(), a.points.get(i + 1).copied()) {
                            // The ring follows the mouse: solve for the bend through it.
                            let ring_y = v.y(pa.value + (pb.value - pa.value) * lyra_engine::fade_curve(0.5, *orig));
                            let want = v.value(ring_y + pos.y - *anchor_y);
                            let g = (want - pa.value) / (pb.value - pa.value);
                            a.points[i].tension = lyra_engine::tension_through(g);
                        }
                    }
                    Drag::Length { orig, anchor_tick } => {
                        let raw = *orig as f32 + v.tick(pos.x) - *anchor_tick;
                        let last = a.points.iter().map(|q| q.tick).max().unwrap_or(0);
                        a.length = (timeline::snap(raw, snap) as u32).max(last).max(TICKS_PER_STEP);
                    }
                    Drag::Hand { last } => {
                        self.time.scroll = (self.time.scroll - (pos.x - last.x) / self.time.ppt).max(0.0);
                        *last = pos;
                    }
                    Drag::None => {}
                }
            }
        } else {
            self.drag = Drag::None;
        }

        let hv = match self.drag {
            Drag::None => pos.filter(|_| hovering).and_then(|pos| hover(&v, &p.automations[ai], pos)),
            Drag::Point { index } => Some(Hover::Point(index)),
            Drag::Tension { index, .. } => Some(Hover::Ring(index)),
            Drag::Length { .. } => Some(Hover::Length),
            Drag::Hand { .. } => None,
        };
        match hv {
            Some(Hover::Point(_)) => ui.ctx().set_cursor_icon(CursorIcon::Grab),
            Some(Hover::Ring(_)) => ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical),
            Some(Hover::Length) => ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal),
            None => {}
        }

        // ── Paint ──
        let painter = ui.painter_at(rect);
        let a = &p.automations[ai];
        let light = theme::channel_light(a.color);
        painter.rect_filled(grid, cr(0), theme::ROLL_BLACK_ROW);
        let end_x = v.x(a.length as f32);
        if end_x < grid.right() {
            // Past the end the curve starts over: dimmed.
            painter.rect_filled(
                Rect::from_min_max(pos2(end_x, grid.top()), grid.max),
                cr(0),
                egui::Color32::from_black_alpha(70),
            );
        }
        // Beat and bar lines; the 0, ½ and 1 levels.
        let beat = TICKS_PER_STEP * 4;
        let first = (v.tick(grid.left()) / beat as f32).floor().max(0.0) as u32;
        let last = (v.tick(grid.right()) / beat as f32).ceil() as u32;
        for b in first..=last {
            let t = b * beat;
            let alpha = if t.is_multiple_of(TICKS_PER_BAR) { 0.8 } else { 0.3 };
            painter.vline(v.x(t as f32), grid.y_range(), stroke(1.0, theme::LINE.gamma_multiply(alpha)));
        }
        for level in [0.0, 0.5, 1.0] {
            painter.hline(grid.x_range(), v.y(level), stroke(1.0, theme::LINE.gamma_multiply(0.7)));
        }
        // The curve (and its next repeat, faint).
        let pts = crate::automation::points(a);
        let mut line = Vec::new();
        let mut x = grid.left();
        while x <= grid.right() {
            let u = v.tick(x) as f64;
            if u >= 0.0 {
                line.push(pos2(x, v.y(crate::automation::value_at(a, &pts, u))));
            }
            x += 2.0;
        }
        let (inside, beyond): (Vec<Pos2>, Vec<Pos2>) = line.iter().partition(|q| q.x <= end_x);
        for w in inside.windows(2) {
            let r = Rect::from_min_max(pos2(w[0].x, w[0].y.min(w[1].y)), pos2(w[1].x, v.area().bottom()));
            painter.rect_filled(r, cr(0), light.mid.gamma_multiply(0.14));
        }
        painter.add(egui::Shape::line(inside, stroke(2.0, light.hi)));
        painter.add(egui::Shape::line(beyond, stroke(1.5, light.hi.gamma_multiply(0.35))));
        // Rings, then points on top.
        for i in 0..a.points.len() {
            if let Some(r) = ring(&v, &a.points, i) {
                waveform::paint_tension(&painter, r, light.hi, hv == Some(Hover::Ring(i)), waveform::LARGE);
            }
        }
        for (i, q) in a.points.iter().enumerate() {
            waveform::paint_handle(&painter, v.at(q), light.hi, hv == Some(Hover::Point(i)), waveform::LARGE);
        }
        // The length line.
        let hot = hv == Some(Hover::Length);
        painter.vline(
            end_x,
            grid.y_range(),
            stroke(if hot { 2.0 } else { 1.5 }, theme::TEXT.gamma_multiply(if hot { 1.0 } else { 0.6 })),
        );
        // What the point under the mouse means, in the first control's own units.
        if let (Some(Hover::Point(i)), Some(link)) = (hv, a.links.first()) {
            if let (Some(q), Some((info, _, _))) = (a.points.get(i), crate::automation::control(p, link.target)) {
                let text = info.format(info.from_normalized(link.travel(q.value)));
                painter.text(
                    v.at(q) + vec2(12.0, -12.0),
                    Align2::LEFT_BOTTOM,
                    text,
                    FontId::monospace(m.text_small),
                    theme::TEXT,
                );
            }
        }
        timeline::paint_ruler(&painter, &self.time, ruler, a.length as f32, None, grid);
        // Where the song is in this curve, while a clip of it plays.
        if let Some(ph) = playhead {
            let c = p
                .clips
                .iter()
                .find(|c| c.source == Source::Automation(id) && (c.start as f64..c.end() as f64).contains(&ph));
            if let Some(c) = c {
                let local = (c.offset as f64 + ph - c.start as f64).rem_euclid(a.length.max(1) as f64);
                timeline::paint_playhead(&painter, v.x(local as f32), ruler, grid.y_range(), true);
            }
        }
        if hovering || !matches!(self.drag, Drag::None) {
            ui.ctx().request_repaint();
        }
    }
}
