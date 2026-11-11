//! The piano roll: edit one channel's notes in the current pattern.
//!
//! Mouse grammar ("it knows what you mean", FL-style):
//! * **click empty** — place a note (or a whole chord, see *Stamp*) with the
//!   length you used last; keep holding to drag it into place
//! * **drag a note** — move · **drag its right edge** — resize · **Shift+drag** — duplicate
//! * clicking a note also makes its length the new default length
//! * **right-drag** — erase · **Ctrl+drag** — box select (Shift adds) · **Alt** — no snap
//! * **wheel** — scroll · **Shift+wheel** — sideways · **Ctrl+wheel** — zoom
//! * velocity lane (bottom) — drag to paint velocities · ruler — drag the gold
//!   marker to set the pattern length · keyboard (left) — click to listen
//!
//! Keys (when this window was clicked last): Delete, Ctrl+A, Ctrl+D (duplicate
//! after), Ctrl+Q (quantize), ↑↓ transpose (Shift = octave), ←→ nudge by snap, Esc.

use crate::keys::Action;
use crate::theme::{self, cr, stroke};
use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, StrokeKind, Ui};
use lyra_model::{theory, ChannelId, Chord, History, Note, Project, Scale, TICKS_PER_BAR, TICKS_PER_STEP};

const KEYS_W: f32 = 54.0;
const RULER_H: f32 = 18.0;
const VEL_H: f32 = 52.0;
const EDGE_GRAB: f32 = 5.0;
const BEAT: u32 = TICKS_PER_STEP * 4;

pub const SNAPS: [(&str, u32); 7] = [
    ("Free", 1),
    ("¼ step", 6),
    ("½ step", 12),
    ("Step", 24),
    ("½ beat", 48),
    ("Beat", 96),
    ("Bar", 384),
];

enum Drag {
    None,
    /// Moving `originals`; `lead` = the note under the mouse (it's the one that snaps).
    Move { anchor_tick: f32, anchor_key: i32, originals: Vec<(usize, Note)>, lead: usize },
    Resize { anchor_tick: f32, originals: Vec<(usize, Note)>, lead: usize },
    Select { from: Pos2, base: Vec<usize> },
    Erase,
    Velocity,
    Length,
    Audition,
}

pub struct PianoRoll {
    /// Tick at the left edge of the grid.
    scroll_x: f32,
    /// Pixels per tick.
    ppt: f32,
    /// Key value at the top edge (fractional = smooth scrolling).
    view_top: f32,
    row_h: f32,
    pub snap: u32,
    pub chord: Chord,
    pub show_scale: bool,
    selection: Vec<usize>,
    last_len: u32,
    last_vel: f32,
    drag: Drag,
    audition: Option<u8>,
    /// (pattern, channel) being edited — selection resets when it changes.
    editing: Option<(usize, ChannelId)>,
}

impl Default for PianoRoll {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            ppt: 0.9,
            view_top: 76.0,
            row_h: 12.0,
            snap: TICKS_PER_STEP,
            chord: Chord::Single,
            show_scale: true,
            selection: Vec::new(),
            last_len: TICKS_PER_STEP * 2,
            last_vel: 0.8,
            drag: Drag::None,
            audition: None,
            editing: None,
        }
    }
}

/// Screen ↔ music coordinates for one frame.
#[derive(Clone, Copy)]
struct View {
    grid: Rect,
    scroll_x: f32,
    ppt: f32,
    view_top: f32,
    row_h: f32,
}

impl View {
    fn x(&self, tick: f32) -> f32 {
        self.grid.left() + (tick - self.scroll_x) * self.ppt
    }
    fn tick(&self, x: f32) -> f32 {
        (x - self.grid.left()) / self.ppt + self.scroll_x
    }
    /// Top edge of a key's row.
    fn y(&self, key: u8) -> f32 {
        self.grid.top() + (self.view_top - (key as f32 + 1.0)) * self.row_h
    }
    fn key(&self, y: f32) -> i32 {
        (self.view_top - (y - self.grid.top()) / self.row_h).floor() as i32
    }
    fn note_rect(&self, n: &Note) -> Rect {
        let y = self.y(n.key);
        Rect::from_min_max(pos2(self.x(n.start as f32), y), pos2(self.x((n.start + n.len) as f32), y + self.row_h))
    }
}

fn hit(view: &View, notes: &[Note], pos: Pos2) -> Option<usize> {
    (0..notes.len()).rev().find(|&i| view.note_rect(&notes[i]).contains(pos))
}

impl PianoRoll {
    /// Scroll so the channel's notes are in view (called when the roll opens).
    pub fn frame_notes(&mut self, notes: &[Note]) {
        if let Some(top) = notes.iter().map(|n| n.key).max() {
            self.view_top = top as f32 + 8.0;
        }
        self.scroll_x = 0.0;
    }

    fn sanitize(&mut self, pat: usize, ch: ChannelId, len: usize) {
        if self.editing != Some((pat, ch)) {
            self.editing = Some((pat, ch));
            self.selection.clear();
            self.drag = Drag::None;
        }
        self.selection.retain(|&i| i < len);
    }

    /// Keyboard commands routed here by the app. Returns true if handled.
    pub fn action(&mut self, a: Action, p: &mut Project, h: &mut History, pat: usize, ch: usize) -> bool {
        let id = p.channels[ch].id;
        let len = p.patterns[pat].notes(id).len();
        self.sanitize(pat, id, len);
        let snap = self.snap;
        match a {
            Action::SelectAll => self.selection = (0..len).collect(),
            Action::Deselect => self.selection.clear(),
            Action::Delete if !self.selection.is_empty() => {
                h.checkpoint(p);
                let notes = p.patterns[pat].notes_mut(id);
                let mut sel = std::mem::take(&mut self.selection);
                sel.sort_unstable_by(|a, b| b.cmp(a));
                for i in sel {
                    notes.remove(i);
                }
            }
            Action::Duplicate if !self.selection.is_empty() => {
                h.checkpoint(p);
                let notes = p.patterns[pat].notes_mut(id);
                let first = self.selection.iter().map(|&i| notes[i].start).min().unwrap_or(0);
                let last = self.selection.iter().map(|&i| notes[i].start + notes[i].len).max().unwrap_or(0);
                let shift = (last - first).div_ceil(BEAT).max(1) * BEAT; // keep the groove: whole beats
                let base = notes.len();
                for &i in &self.selection {
                    let mut n = notes[i].clone();
                    n.start += shift;
                    notes.push(n);
                }
                self.selection = (base..notes.len()).collect();
                p.patterns[pat].fit_to_notes();
            }
            Action::Quantize => {
                h.checkpoint(p);
                let notes = p.patterns[pat].notes_mut(id);
                let all: Vec<usize> = (0..notes.len()).collect();
                let targets = if self.selection.is_empty() { &all } else { &self.selection };
                for &i in targets {
                    notes[i].start = ((notes[i].start as f32 / snap as f32).round() as u32) * snap;
                }
            }
            Action::Arrow { dx, dy, big } if !self.selection.is_empty() => {
                h.checkpoint(p);
                let notes = p.patterns[pat].notes_mut(id);
                let dk = dy * if big { 12 } else { 1 };
                let dt = dx as i64 * if big { BEAT as i64 } else { snap as i64 };
                let ok = self.selection.iter().all(|&i| {
                    let k = notes[i].key as i32 + dk;
                    (0..=127).contains(&k) && notes[i].start as i64 + dt >= 0
                });
                if ok {
                    for &i in &self.selection {
                        notes[i].key = (notes[i].key as i32 + dk) as u8;
                        notes[i].start = (notes[i].start as i64 + dt) as u32;
                    }
                }
                p.patterns[pat].fit_to_notes();
            }
            _ => return false,
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        p: &mut Project,
        h: &mut History,
        pat: usize,
        ch: usize,
        playhead: Option<f64>,
        live_keys: &[u8],
        out: &mut Vec<Action>,
    ) {
        let ch_id = p.channels[ch].id;
        let color = theme::channel_color(p.channels[ch].color);
        self.sanitize(pat, ch_id, p.patterns[pat].notes(ch_id).len());

        self.toolbar(ui, p, h, color, ch);
        ui.add_space(3.0);

        // ── Layout ──
        let size = ui.available_size().max(vec2(320.0, 220.0));
        let (rect, _resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let grid = Rect::from_min_max(
            pos2(rect.left() + KEYS_W, rect.top() + RULER_H),
            pos2(rect.right(), rect.bottom() - VEL_H),
        );
        let ruler = Rect::from_min_max(pos2(grid.left(), rect.top()), pos2(rect.right(), grid.top()));
        let keys_r = Rect::from_min_max(pos2(rect.left(), grid.top()), pos2(grid.left(), grid.bottom()));
        let vel = Rect::from_min_max(pos2(grid.left(), grid.bottom() + 3.0), rect.right_bottom());

        // ── Scroll & zoom ──
        let (pos, pressed_p, pressed_s, down, mods, wheel, zoom) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.primary_pressed(),
                i.pointer.secondary_pressed(),
                i.pointer.any_down(),
                i.modifiers,
                i.raw_scroll_delta,
                i.zoom_delta(),
            )
        });
        let hovering = ui.rect_contains_pointer(rect);
        if hovering {
            if zoom != 1.0 {
                if let Some(pos) = pos {
                    let anchor = (pos.x - grid.left()) / self.ppt + self.scroll_x;
                    self.ppt = (self.ppt * zoom).clamp(0.12, 6.0);
                    self.scroll_x = anchor - (pos.x - grid.left()) / self.ppt;
                }
            }
            let sideways = if wheel.x != 0.0 { wheel.x } else if mods.shift { wheel.y } else { 0.0 };
            if sideways != 0.0 {
                self.scroll_x -= sideways / self.ppt;
            } else if wheel.y != 0.0 {
                self.view_top += wheel.y / self.row_h * 0.5;
            }
        }
        let rows = grid.height() / self.row_h;
        self.view_top = self.view_top.clamp(rows.min(128.0), 128.0);
        self.scroll_x = self.scroll_x.max(0.0);
        let view = View { grid, scroll_x: self.scroll_x, ppt: self.ppt, view_top: self.view_top, row_h: self.row_h };

        // ── Mouse ──
        let snap = if mods.alt { 1 } else { self.snap };
        let (scale, root) = (p.scale, p.key_root);
        if hovering && (pressed_p || pressed_s) {
            if let Some(pos) = pos {
                self.press(pos, pressed_p, mods, &view, ruler, keys_r, vel, p, h, pat, ch_id, scale, root, snap, out);
            }
        }
        if down {
            if let Some(pos) = pos {
                self.drag_to(pos, &view, vel, p, pat, ch_id, snap, out);
            }
        } else if !matches!(self.drag, Drag::None) {
            if let Some(k) = self.audition.take() {
                out.push(Action::NoteOff(k));
            }
            p.patterns[pat].fit_to_notes();
            self.drag = Drag::None;
        }

        // Cursor feedback
        if let (true, Some(pos), Drag::None) = (hovering && grid.contains(pos.unwrap_or_default()), pos, &self.drag) {
            let notes = p.patterns[pat].notes(ch_id);
            if let Some(i) = hit(&view, notes, pos) {
                let r = view.note_rect(&notes[i]);
                ui.ctx().set_cursor_icon(if pos.x > r.right() - EDGE_GRAB { CursorIcon::ResizeHorizontal } else { CursorIcon::Grab });
            }
        } else if matches!(self.drag, Drag::Move { .. }) {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if matches!(self.drag, Drag::Resize { .. }) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }

        // ── Paint ──
        let painter = ui.painter_at(rect);
        self.paint_grid(&painter, &view, p, pat, scale, root);
        self.paint_notes(&painter, &view, p, pat, ch, color, playhead);
        if let (Drag::Select { from, .. }, Some(pos)) = (&self.drag, pos) {
            let r = Rect::from_two_pos(*from, pos);
            painter.rect_filled(r, cr(0), theme::MINT.gamma_multiply(0.07));
            painter.rect_stroke(r, cr(0), stroke(1.0, theme::MINT.gamma_multiply(0.7)), StrokeKind::Inside);
        }
        self.paint_ruler(&painter, &view, ruler, p.patterns[pat].length, playhead);
        let sounding: Vec<u8> = playhead
            .map(|t| {
                p.patterns[pat]
                    .notes(ch_id)
                    .iter()
                    .filter(|n| (n.start as f64..(n.start + n.len) as f64).contains(&t))
                    .map(|n| n.key)
                    .collect()
            })
            .unwrap_or_default();
        self.paint_keys(&painter, &view, keys_r, color, live_keys, &sounding, scale, root);
        self.paint_velocity(&painter, &view, vel, p, pat, ch_id, color);
    }

    fn toolbar(&mut self, ui: &mut Ui, p: &mut Project, h: &mut History, color: Color32, ch: usize) {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 4.0, color);
            ui.label(egui::RichText::new(&p.channels[ch].name).strong());
            ui.add_space(10.0);

            ui.label(egui::RichText::new("snap").small().color(theme::TEXT_DIM));
            let snap_name = SNAPS.iter().find(|s| s.1 == self.snap).map_or("?", |s| s.0);
            egui::ComboBox::from_id_salt("pr-snap").width(64.0).selected_text(snap_name).show_ui(ui, |ui| {
                for (name, ticks) in SNAPS {
                    ui.selectable_value(&mut self.snap, ticks, name);
                }
            });

            ui.label(egui::RichText::new("stamp").small().color(theme::TEXT_DIM));
            egui::ComboBox::from_id_salt("pr-chord").width(104.0).selected_text(self.chord.name()).show_ui(ui, |ui| {
                for c in Chord::ALL {
                    ui.selectable_value(&mut self.chord, c, c.name());
                }
            })
            .response
            .on_hover_text("What one click places. 'In key' chords always fit the song's scale.");

            ui.label(egui::RichText::new("key").small().color(theme::TEXT_DIM));
            let (mut root, mut scale) = (p.key_root, p.scale);
            egui::ComboBox::from_id_salt("pr-root").width(40.0).selected_text(theory::NOTE_NAMES[root as usize]).show_ui(ui, |ui| {
                for (i, name) in theory::NOTE_NAMES.iter().enumerate() {
                    ui.selectable_value(&mut root, i as u8, *name);
                }
            });
            egui::ComboBox::from_id_salt("pr-scale").width(110.0).selected_text(scale.name()).show_ui(ui, |ui| {
                for s in Scale::ALL {
                    ui.selectable_value(&mut scale, s, s.name());
                }
            });
            if (root, scale) != (p.key_root, p.scale) {
                h.checkpoint(p);
                p.key_root = root;
                p.scale = scale;
            }
            ui.checkbox(&mut self.show_scale, "highlight");
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn press(
        &mut self,
        pos: Pos2,
        primary: bool,
        mods: egui::Modifiers,
        view: &View,
        ruler: Rect,
        keys_r: Rect,
        vel: Rect,
        p: &mut Project,
        h: &mut History,
        pat: usize,
        ch_id: ChannelId,
        scale: Scale,
        root: u8,
        snap: u32,
        out: &mut Vec<Action>,
    ) {
        if view.grid.contains(pos) {
            h.checkpoint(p);
            let notes = p.patterns[pat].notes_mut(ch_id);
            if !primary {
                self.selection.clear();
                if let Some(i) = hit(view, notes, pos) {
                    notes.remove(i);
                }
                self.drag = Drag::Erase;
                return;
            }
            if mods.command {
                let base = if mods.shift { self.selection.clone() } else { Vec::new() };
                self.drag = Drag::Select { from: pos, base };
                return;
            }
            let anchor_tick = view.tick(pos.x);
            let anchor_key = view.key(pos.y);
            match hit(view, notes, pos) {
                Some(i) => {
                    if !self.selection.contains(&i) {
                        self.selection = vec![i];
                    }
                    self.last_len = notes[i].len; // "use this length from now on"
                    self.last_vel = notes[i].velocity;
                    let near_edge = pos.x > view.note_rect(&notes[i]).right() - EDGE_GRAB;
                    let mut lead_i = i;
                    if mods.shift && !near_edge {
                        // Duplicate the selection; drag the copies.
                        let base = notes.len();
                        let at = self.selection.iter().position(|&j| j == i).unwrap_or(0);
                        for &j in &self.selection {
                            let n = notes[j].clone();
                            notes.push(n);
                        }
                        self.selection = (base..notes.len()).collect();
                        lead_i = self.selection[at];
                    }
                    let originals: Vec<(usize, Note)> = self.selection.iter().map(|&j| (j, notes[j].clone())).collect();
                    let lead = originals.iter().position(|(j, _)| *j == lead_i).unwrap_or(0);
                    if near_edge {
                        self.drag = Drag::Resize { anchor_tick, originals, lead };
                    } else {
                        self.audition(notes[lead_i].key, out);
                        self.drag = Drag::Move { anchor_tick, anchor_key, originals, lead };
                    }
                }
                None => {
                    // Place a note — or a whole chord — and keep dragging it.
                    let key = anchor_key.clamp(0, 127) as u8;
                    let start = (anchor_tick.max(0.0) as u32 / snap) * snap;
                    let base = notes.len();
                    for k in self.chord.keys(key, scale, root) {
                        notes.push(Note { start, len: self.last_len, key: k, velocity: self.last_vel });
                    }
                    self.selection = (base..notes.len()).collect();
                    let originals = self.selection.iter().map(|&j| (j, notes[j].clone())).collect();
                    self.audition(notes[base].key, out);
                    self.drag = Drag::Move { anchor_tick, anchor_key, originals, lead: 0 };
                }
            }
        } else if keys_r.contains(pos) && primary {
            let key = view.key(pos.y).clamp(0, 127) as u8;
            self.audition(key, out);
            self.drag = Drag::Audition;
        } else if vel.contains(pos) && primary {
            h.checkpoint(p);
            self.drag = Drag::Velocity;
        } else if ruler.contains(pos) && primary {
            h.checkpoint(p);
            self.drag = Drag::Length;
        }
    }

    fn audition(&mut self, key: u8, out: &mut Vec<Action>) {
        if self.audition == Some(key) {
            return;
        }
        if let Some(prev) = self.audition.take() {
            out.push(Action::NoteOff(prev));
        }
        out.push(Action::NoteOn(key));
        self.audition = Some(key);
    }

    #[allow(clippy::too_many_arguments)]
    fn drag_to(
        &mut self,
        pos: Pos2,
        view: &View,
        vel: Rect,
        p: &mut Project,
        pat: usize,
        ch_id: ChannelId,
        snap: u32,
        out: &mut Vec<Action>,
    ) {
        let drag = std::mem::replace(&mut self.drag, Drag::None);
        let notes = p.patterns[pat].notes_mut(ch_id);
        match &drag {
            Drag::Move { anchor_tick, anchor_key, originals, lead } => {
                let lead_note = &originals[*lead].1;
                let raw = lead_note.start as f32 + (view.tick(pos.x) - anchor_tick);
                let snapped = ((raw / snap as f32).round() * snap as f32).max(0.0);
                let min_start = originals.iter().map(|(_, n)| n.start).min().unwrap_or(0) as i64;
                let dt = (snapped as i64 - lead_note.start as i64).max(-min_start);
                let (lo, hi) = originals.iter().fold((127i32, 0i32), |(lo, hi), (_, n)| (lo.min(n.key as i32), hi.max(n.key as i32)));
                let dk = (view.key(pos.y) - anchor_key).clamp(-lo, 127 - hi);
                for (i, n) in originals {
                    if let Some(note) = notes.get_mut(*i) {
                        note.start = (n.start as i64 + dt) as u32;
                        note.key = (n.key as i32 + dk) as u8;
                    }
                }
                self.audition((lead_note.key as i32 + dk) as u8, out);
            }
            Drag::Resize { anchor_tick, originals, lead } => {
                let lead_note = &originals[*lead].1;
                let raw = lead_note.len as f32 + (view.tick(pos.x) - anchor_tick);
                let new_len = ((raw / snap as f32).round() * snap as f32).max(snap as f32) as i64;
                let dl = new_len - lead_note.len as i64;
                for (i, n) in originals {
                    if let Some(note) = notes.get_mut(*i) {
                        note.len = (n.len as i64 + dl).max(snap.min(n.len) as i64) as u32;
                    }
                }
                self.last_len = new_len as u32;
            }
            Drag::Select { from, base } => {
                let r = Rect::from_two_pos(*from, pos);
                let mut sel = base.clone();
                sel.extend((0..notes.len()).filter(|&i| view.note_rect(&notes[i]).intersects(r) && !base.contains(&i)));
                self.selection = sel;
            }
            Drag::Erase => {
                if let Some(i) = hit(view, notes, pos) {
                    notes.remove(i);
                }
            }
            Drag::Velocity => {
                let v = ((vel.bottom() - pos.y) / vel.height()).clamp(0.05, 1.0);
                for (i, n) in notes.iter_mut().enumerate() {
                    let near = (view.x(n.start as f32) - pos.x).abs() < 5.0;
                    if near && (self.selection.is_empty() || self.selection.contains(&i)) {
                        n.velocity = v;
                    }
                }
                self.last_vel = v;
            }
            Drag::Length => {
                let bars = (view.tick(pos.x) / TICKS_PER_BAR as f32).round().max(1.0) as u32;
                p.patterns[pat].length = bars * lyra_model::STEPS_PER_BAR;
            }
            Drag::Audition => {
                let key = view.key(pos.y).clamp(0, 127) as u8;
                self.audition(key, out);
            }
            Drag::None => {}
        }
        self.drag = drag;
    }

    fn paint_grid(&self, painter: &egui::Painter, v: &View, p: &Project, pat: usize, scale: Scale, root: u8) {
        let g = v.grid;
        painter.rect_filled(g, cr(0), theme::ROLL_WHITE_ROW);
        let lo = v.key(g.bottom()).max(0);
        let hi = v.key(g.top()).min(127);
        for key in lo..=hi {
            let key = key as u8;
            let y = v.y(key);
            let row = Rect::from_min_max(pos2(g.left(), y), pos2(g.right(), y + v.row_h));
            let in_scale = !self.show_scale || scale.contains(root, key);
            let fill = match (theory::is_black(key), in_scale) {
                (_, false) => Some(theme::ROLL_OUT_ROW),
                (true, true) => Some(theme::ROLL_BLACK_ROW),
                (false, true) => None,
            };
            if let Some(f) = fill {
                painter.rect_filled(row, cr(0), f);
            }
            if self.show_scale && key % 12 == root % 12 {
                painter.rect_filled(row, cr(0), theme::MINT.gamma_multiply(0.05));
            }
            let line = if key.is_multiple_of(12) { theme::LINE } else { theme::LINE.gamma_multiply(0.45) };
            painter.hline(g.x_range(), y + v.row_h, stroke(1.0, line));
        }

        // Vertical lines: steps faint, beats stronger, bars strongest — all dark.
        let first = (v.scroll_x / TICKS_PER_STEP as f32).floor().max(0.0) as u32;
        let last = (v.tick(g.right()) / TICKS_PER_STEP as f32).ceil() as u32;
        let step_px = TICKS_PER_STEP as f32 * v.ppt;
        for s in first..=last {
            let t = s * TICKS_PER_STEP;
            let x = v.x(t as f32);
            let (alpha, show) = if t.is_multiple_of(TICKS_PER_BAR) {
                (1.0, true)
            } else if t.is_multiple_of(BEAT) {
                (0.6, step_px * 4.0 >= 8.0)
            } else {
                (0.28, step_px >= 7.0)
            };
            if show {
                painter.vline(x, g.y_range(), stroke(1.0, theme::LINE.gamma_multiply(alpha)));
            }
        }
        // Beyond the pattern's end: dimmed.
        let end = v.x((p.patterns[pat].length * TICKS_PER_STEP) as f32);
        if end < g.right() {
            painter.rect_filled(Rect::from_min_max(pos2(end.max(g.left()), g.top()), g.right_bottom()), cr(0), Color32::from_black_alpha(90));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_notes(&self, painter: &egui::Painter, v: &View, p: &Project, pat: usize, ch: usize, color: Color32, playhead: Option<f64>) {
        let pattern = &p.patterns[pat];
        // Ghosts: other channels' notes, so you can write around them.
        for (other_i, other) in p.channels.iter().enumerate() {
            let melodic = lyra_dsp::info(&other.instrument).is_some_and(|i| i.melodic);
            if other_i == ch || !melodic {
                continue; // drums as ghosts are just noise in a melodic editor
            }
            let c = theme::channel_color(other.color);
            for n in pattern.notes(other.id) {
                let r = v.note_rect(n).shrink2(vec2(0.5, 1.5));
                painter.rect_filled(r, cr(1), c.gamma_multiply(0.13));
            }
        }
        for (i, n) in pattern.notes(p.channels[ch].id).iter().enumerate() {
            let r = v.note_rect(n).shrink2(vec2(0.5, 0.5));
            let selected = self.selection.contains(&i);
            let playing = playhead.is_some_and(|t| (n.start as f64..(n.start + n.len) as f64).contains(&t));
            let mut fill = theme::mix(theme::PANEL_HI, color, 0.45 + 0.55 * n.velocity);
            if playing {
                fill = theme::mix(fill, Color32::WHITE, 0.25);
            }
            painter.rect_filled(r, cr(2), fill);
            let edge = if selected { stroke(1.5, theme::TEXT) } else { stroke(1.0, theme::LINE) };
            painter.rect_stroke(r, cr(2), edge, StrokeKind::Inside);
            if r.width() > 30.0 && v.row_h >= 10.0 {
                painter.text(
                    r.left_center() + vec2(4.0, 0.0),
                    Align2::LEFT_CENTER,
                    theory::note_name(n.key),
                    FontId::proportional(9.0),
                    theme::INSET,
                );
            }
        }
        if let Some(t) = playhead {
            let x = v.x(t as f32);
            if x >= v.grid.left() {
                painter.vline(x, v.grid.y_range(), stroke(1.0, theme::MINT));
            }
        }
    }

    fn paint_ruler(&self, painter: &egui::Painter, v: &View, ruler: Rect, length_steps: u32, playhead: Option<f64>) {
        painter.rect_filled(ruler, cr(0), theme::TOOLBAR);
        painter.hline(ruler.x_range(), ruler.bottom() - 0.5, stroke(1.0, theme::LINE));
        let bar_px = TICKS_PER_BAR as f32 * v.ppt;
        let every = if bar_px < 30.0 { 4 } else if bar_px < 60.0 { 2 } else { 1 };
        let first = (v.scroll_x / TICKS_PER_BAR as f32).floor() as u32;
        let last = (v.tick(ruler.right()) / TICKS_PER_BAR as f32).ceil() as u32;
        for bar in (first..=last).filter(|b| b % every == 0) {
            let x = v.x((bar * TICKS_PER_BAR) as f32);
            painter.vline(x, ruler.y_range(), stroke(1.0, theme::LINE));
            painter.text(pos2(x + 3.0, ruler.center().y), Align2::LEFT_CENTER, format!("{}", bar + 1), FontId::proportional(9.5), theme::TEXT_DIM);
        }
        // Pattern end marker (draggable).
        let end = v.x((length_steps * TICKS_PER_STEP) as f32);
        if end >= ruler.left() {
            let tri = vec![pos2(end - 5.0, ruler.top() + 2.0), pos2(end + 5.0, ruler.top() + 2.0), pos2(end, ruler.bottom() - 3.0)];
            painter.add(egui::Shape::convex_polygon(tri, theme::GOLD, egui::Stroke::NONE));
        }
        if let Some(t) = playhead {
            let x = v.x(t as f32);
            if x >= ruler.left() {
                painter.vline(x, ruler.y_range(), stroke(1.0, theme::MINT));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_keys(&self, painter: &egui::Painter, v: &View, r: Rect, color: Color32, live: &[u8], sounding: &[u8], scale: Scale, root: u8) {
        painter.rect_filled(r, cr(0), theme::KEY_WHITE);
        let lo = v.key(r.bottom()).max(0);
        let hi = v.key(r.top()).min(127);
        for key in lo..=hi {
            let key = key as u8;
            let y = v.y(key);
            let row = Rect::from_min_max(pos2(r.left(), y), pos2(r.right(), y + v.row_h));
            let lit = live.contains(&key) || sounding.contains(&key) || self.audition == Some(key);
            if theory::is_black(key) {
                let black = Rect::from_min_max(row.min, pos2(r.left() + r.width() * 0.62, row.max.y));
                painter.rect_filled(black, cr(0), if lit { color } else { theme::KEY_BLACK });
            } else {
                if lit {
                    painter.rect_filled(row, cr(0), color);
                }
                if matches!(key % 12, 0 | 5) {
                    painter.hline(r.x_range(), y + v.row_h, stroke(1.0, theme::KEY_EDGE)); // B|C and E|F seams
                }
            }
            if self.show_scale && !scale.contains(root, key) && !theory::is_black(key) && !lit {
                painter.rect_filled(row, cr(0), Color32::from_black_alpha(40));
            }
            if key.is_multiple_of(12) {
                painter.text(pos2(r.right() - 4.0, y + v.row_h * 0.5), Align2::RIGHT_CENTER, theory::note_name(key), FontId::proportional(8.5), theme::KEY_BLACK);
            }
        }
        painter.vline(r.right() - 0.5, r.y_range(), stroke(1.0, theme::LINE));
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_velocity(&self, painter: &egui::Painter, v: &View, r: Rect, p: &Project, pat: usize, ch_id: ChannelId, color: Color32) {
        painter.rect_filled(r, cr(2), theme::INSET);
        for (i, n) in p.patterns[pat].notes(ch_id).iter().enumerate() {
            let x = v.x(n.start as f32);
            if x < r.left() || x > r.right() {
                continue;
            }
            let top = r.bottom() - n.velocity * (r.height() - 4.0);
            let c = if self.selection.contains(&i) { theme::TEXT } else { color };
            painter.vline(x + 0.5, top..=r.bottom(), stroke(1.5, c.gamma_multiply(0.8)));
            painter.circle_filled(pos2(x + 0.5, top), 2.5, c);
        }
    }
}
