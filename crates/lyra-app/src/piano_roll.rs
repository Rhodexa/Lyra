//! The piano roll: edit one channel's notes in the current pattern.
//!
//! Mouse grammar ("it knows what you mean", FL-style):
//! * **click empty** — place a note (or a whole chord, see *Stamp*) with the
//!   length you used last; keep holding to drag it into place
//! * **drag a note** — move · **drag its right edge** — resize · **Shift+drag** — duplicate
//! * clicking a note also makes its length the new default length
//! * **right-drag** — erase · **Ctrl+drag** — box select (Shift adds)
//! * **Alt** — no snap. Once a drag has gone unsnapped it *stays* unsnapped,
//!   so letting go of Alt a moment early never re-snaps your note.
//! * **wheel** — scroll · **Shift+wheel** — sideways · **Alt+wheel** — zoom time ·
//!   **Ctrl+wheel** — zoom pitch · **wheel over a note** — its velocity
//!   (the whole selection's, if the note is selected)
//! * overview bar (very top) — drag the thumb to pan, drag its ends to zoom
//! * ruler — click/drag to move the playhead · velocity lane — drag to paint ·
//!   keyboard (left) — click to listen
//!
//! Keys (when this window was clicked last): Delete, Ctrl+A, Ctrl+C/X/V,
//! Ctrl+D or Ctrl+B (duplicate after), Ctrl+Q (quantize), ↑↓ semitone,
//! Ctrl+↑↓ octave, ←→ nudge by snap, Ctrl+←→ by a beat, Esc.

use crate::keys::Action;
use crate::theme::{self, cr, stroke};
use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, StrokeKind, Ui};
use lyra_model::{theory, ChannelId, Chord, History, Note, Project, Scale, TICKS_PER_BAR, TICKS_PER_STEP};

const KEYS_W: f32 = 56.0;
const OVERVIEW_H: f32 = 11.0;
const RULER_H: f32 = 18.0;
const VEL_H: f32 = 52.0;
/// How close to a note's right end counts as "grab the edge" (generous on purpose).
const EDGE_GRAB: f32 = 8.0;
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
    Move { anchor_tick: f32, anchor_key: i32, originals: Vec<(usize, Note)>, lead: usize, free: bool },
    Resize { anchor_tick: f32, originals: Vec<(usize, Note)>, lead: usize, free: bool },
    Select { from: Pos2, base: Vec<usize> },
    Erase,
    Velocity,
    Seek,
    Audition,
    /// Overview bar: dragging the thumb's middle (grab offset in ticks from its left end).
    Pan { grab: f32 },
    /// Overview bar: dragging one end of the thumb; the other end stays put.
    ZoomEdge { left: bool, fixed: f32 },
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
    pub show_ghosts: bool,
    /// Fill the whole workspace (toggled from the toolbar).
    pub maximized: bool,
    selection: Vec<usize>,
    /// Default lengths for new notes: [drum channels, melodic channels].
    last_len: [u32; 2],
    last_vel: f32,
    clipboard: Vec<Note>,
    drag: Drag,
    audition: Option<u8>,
    /// (pattern, channel) being edited — selection resets when it changes.
    editing: Option<(usize, ChannelId)>,
    melodic: bool,
}

impl Default for PianoRoll {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            ppt: 0.9,
            view_top: 76.0,
            row_h: 13.0,
            snap: TICKS_PER_STEP,
            chord: Chord::Single,
            show_scale: true,
            show_ghosts: true,
            maximized: false,
            selection: Vec::new(),
            last_len: [TICKS_PER_STEP, TICKS_PER_STEP * 2],
            last_vel: 0.8,
            clipboard: Vec::new(),
            drag: Drag::None,
            audition: None,
            editing: None,
            melodic: true,
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
    fn visible_ticks(&self) -> f32 {
        self.grid.width() / self.ppt
    }
}

fn hit(view: &View, notes: &[Note], pos: Pos2) -> Option<usize> {
    (0..notes.len()).rev().find(|&i| view.note_rect(&notes[i]).contains(pos))
}

fn snap_ticks(raw: f32, snap: u32) -> f32 {
    (raw / snap as f32).round() * snap as f32
}

impl PianoRoll {
    /// Scroll so the channel's notes are in view (called when the roll opens).
    pub fn frame_notes(&mut self, notes: &[Note]) {
        if let Some(top) = notes.iter().map(|n| n.key).max() {
            self.view_top = top as f32 + 8.0;
        }
        self.scroll_x = 0.0;
    }

    fn sanitize(&mut self, pat: usize, ch: ChannelId, len: usize, melodic: bool) {
        if self.editing != Some((pat, ch)) {
            self.editing = Some((pat, ch));
            self.selection.clear();
            self.drag = Drag::None;
        }
        self.melodic = melodic;
        self.selection.retain(|&i| i < len);
    }

    fn default_len(&self) -> u32 {
        self.last_len[self.melodic as usize]
    }

    fn set_default_len(&mut self, len: u32) {
        self.last_len[self.melodic as usize] = len;
    }

    /// Keyboard commands routed here by the app. Returns true if handled.
    pub fn action(&mut self, a: Action, p: &mut Project, h: &mut History, pat: usize, ch: usize) -> bool {
        let id = p.channels[ch].id;
        let len = p.patterns[pat].notes(id).len();
        let melodic = lyra_dsp::info(&p.channels[ch].instrument).is_some_and(|i| i.melodic);
        self.sanitize(pat, id, len, melodic);
        let snap = self.snap;
        match a {
            Action::SelectAll => self.selection = (0..len).collect(),
            Action::Deselect => self.selection.clear(),
            Action::Copy | Action::Cut if !self.selection.is_empty() => {
                let notes = p.patterns[pat].notes(id);
                self.clipboard = self.selection.iter().map(|&i| notes[i].clone()).collect();
                if a == Action::Cut {
                    return self.action(Action::Delete, p, h, pat, ch);
                }
            }
            Action::Paste if !self.clipboard.is_empty() => {
                // In place (like FL): the pasted notes come in selected, ready to drag.
                h.checkpoint(p);
                let notes = p.patterns[pat].notes_mut(id);
                let base = notes.len();
                notes.extend(self.clipboard.iter().cloned());
                self.selection = (base..notes.len()).collect();
            }
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
            }
            Action::Quantize => {
                h.checkpoint(p);
                let notes = p.patterns[pat].notes_mut(id);
                let all: Vec<usize> = (0..notes.len()).collect();
                let targets = if self.selection.is_empty() { &all } else { &self.selection };
                for &i in targets {
                    notes[i].start = snap_ticks(notes[i].start as f32, snap) as u32;
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
        playhead: f64,
        playing: bool,
        live_keys: &[u8],
        out: &mut Vec<Action>,
    ) {
        let ch_id = p.channels[ch].id;
        let color = theme::channel_color(p.channels[ch].color);
        let melodic = lyra_dsp::info(&p.channels[ch].instrument).is_some_and(|i| i.melodic);
        self.sanitize(pat, ch_id, p.patterns[pat].notes(ch_id).len(), melodic);

        self.toolbar(ui, p, h, color, ch);
        ui.add_space(3.0);

        // ── Layout ──
        let size = ui.available_size().max(vec2(320.0, 220.0));
        let (rect, _resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let overview = Rect::from_min_max(pos2(rect.left() + KEYS_W, rect.top()), pos2(rect.right(), rect.top() + OVERVIEW_H));
        let ruler = Rect::from_min_max(pos2(overview.left(), overview.bottom() + 1.0), pos2(rect.right(), overview.bottom() + 1.0 + RULER_H));
        let grid = Rect::from_min_max(pos2(overview.left(), ruler.bottom()), pos2(rect.right(), rect.bottom() - VEL_H));
        let keys_r = Rect::from_min_max(pos2(rect.left(), grid.top()), pos2(grid.left(), grid.bottom()));
        let vel = Rect::from_min_max(pos2(grid.left(), grid.bottom() + 3.0), rect.right_bottom());

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
        let pattern_ticks = (p.patterns[pat].play_length() * TICKS_PER_STEP) as f32;

        // ── Wheel ──
        if hovering {
            let view = self.view(grid);
            let hovered_note = pos.filter(|p| grid.contains(*p)).and_then(|pos| hit(&view, p.patterns[pat].notes(ch_id), pos));
            let vertical = wheel.y;
            if zoom != 1.0 {
                // Ctrl+wheel: pitch zoom, keeping the key under the mouse in place.
                if let Some(pos) = pos {
                    let anchor = self.view_top - (pos.y - grid.top()) / self.row_h;
                    self.row_h = (self.row_h * zoom).clamp(6.0, 30.0);
                    self.view_top = anchor + (pos.y - grid.top()) / self.row_h;
                }
            } else if vertical != 0.0 && mods.alt {
                // Alt+wheel: time zoom around the mouse.
                self.zoom_time((vertical * 0.004).exp(), pos.map_or(grid.left(), |p| p.x), grid);
            } else if let (Some(i), true) = (hovered_note, vertical != 0.0 && !mods.shift) {
                // Wheel over a note: velocity (of the selection, if that note is in it).
                h.checkpoint(p);
                let targets = if self.selection.contains(&i) { self.selection.clone() } else { vec![i] };
                let notes = p.patterns[pat].notes_mut(ch_id);
                for t in targets {
                    notes[t].velocity = (notes[t].velocity + vertical.signum() * 0.05).clamp(0.05, 1.0);
                }
            } else {
                let sideways = if wheel.x != 0.0 { wheel.x } else if mods.shift { vertical } else { 0.0 };
                if sideways != 0.0 {
                    self.scroll_x -= sideways / self.ppt;
                } else if vertical != 0.0 {
                    self.view_top += vertical / self.row_h * 0.5;
                }
            }
        }
        let rows = grid.height() / self.row_h;
        self.view_top = self.view_top.clamp(rows.min(128.0), 128.0);
        self.scroll_x = self.scroll_x.max(0.0);
        let view = self.view(grid);

        // ── Mouse ──
        let (scale, root) = (p.scale, p.key_root);
        if hovering && (pressed_p || pressed_s) {
            if let Some(pos) = pos {
                self.press(pos, pressed_p, mods, &view, overview, ruler, keys_r, vel, pattern_ticks, p, h, pat, ch_id, scale, root, out);
            }
        }
        if down {
            if let Some(pos) = pos {
                self.drag_to(pos, mods, &view, vel, p, pat, ch_id, out);
            }
        } else if !matches!(self.drag, Drag::None) {
            if let Some(k) = self.audition.take() {
                out.push(Action::NoteOff(k));
            }
            self.drag = Drag::None;
        }

        // Cursor feedback
        if let (true, Some(pos), Drag::None) = (hovering && grid.contains(pos.unwrap_or_default()), pos, &self.drag) {
            let notes = p.patterns[pat].notes(ch_id);
            if let Some(i) = hit(&view, notes, pos) {
                let r = view.note_rect(&notes[i]);
                ui.ctx().set_cursor_icon(if pos.x > r.right() - EDGE_GRAB { CursorIcon::ResizeHorizontal } else { CursorIcon::Grab });
            }
        } else if matches!(self.drag, Drag::Move { .. } | Drag::Pan { .. }) {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if matches!(self.drag, Drag::Resize { .. } | Drag::ZoomEdge { .. }) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }

        // ── Paint ──
        let view = self.view(grid); // the drag may have scrolled or zoomed
        let painter = ui.painter_at(rect);
        let play = playhead as f32;
        self.paint_grid(&painter, &view, pattern_ticks, scale, root);
        self.paint_notes(&painter, &view, p, pat, ch, color, playing.then_some(playhead));
        if let (Drag::Select { from, .. }, Some(pos)) = (&self.drag, pos) {
            let r = Rect::from_two_pos(*from, pos);
            painter.rect_filled(r, cr(0), theme::MINT.gamma_multiply(0.07));
            painter.rect_stroke(r, cr(0), stroke(1.0, theme::MINT.gamma_multiply(0.7)), StrokeKind::Inside);
        }
        self.paint_overview(&painter, &view, overview, pattern_ticks, p, pat, ch_id, color);
        self.paint_ruler(&painter, &view, ruler, pattern_ticks, play, playing);
        let sounding: Vec<u8> = if playing {
            p.patterns[pat]
                .notes(ch_id)
                .iter()
                .filter(|n| (n.start as f64..(n.start + n.len) as f64).contains(&playhead))
                .map(|n| n.key)
                .collect()
        } else {
            Vec::new()
        };
        self.paint_keys(&painter, &view, keys_r, color, live_keys, &sounding, scale, root);
        self.paint_velocity(&painter, &view, vel, p, pat, ch_id, color);
    }

    fn view(&self, grid: Rect) -> View {
        View { grid, scroll_x: self.scroll_x, ppt: self.ppt, view_top: self.view_top, row_h: self.row_h }
    }

    /// Zoom the time axis by `factor`, keeping the tick under screen-x `anchor_x` still.
    fn zoom_time(&mut self, factor: f32, anchor_x: f32, grid: Rect) {
        let anchor = (anchor_x - grid.left()) / self.ppt + self.scroll_x;
        self.ppt = (self.ppt * factor).clamp(0.08, 8.0);
        self.scroll_x = (anchor - (anchor_x - grid.left()) / self.ppt).max(0.0);
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
            egui::ComboBox::from_id_salt("pr-chord")
                .width(104.0)
                .selected_text(self.chord.name())
                .show_ui(ui, |ui| {
                    for c in Chord::ALL {
                        ui.selectable_value(&mut self.chord, c, c.name());
                    }
                })
                .response
                .on_hover_text("What one click places. 'In key' chords always fit the song's key.");

            // Key + view options live in one menu, out of the way.
            let key_label = format!("{} {}", theory::NOTE_NAMES[p.key_root as usize], p.scale.name().to_lowercase());
            ui.menu_button(format!("View · {key_label}"), |ui| {
                ui.label(egui::RichText::new("SONG KEY").small().color(theme::TEXT_DIM));
                let (mut root, mut scale) = (p.key_root, p.scale);
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("pr-root").width(44.0).selected_text(theory::NOTE_NAMES[root as usize]).show_ui(ui, |ui| {
                        for (i, name) in theory::NOTE_NAMES.iter().enumerate() {
                            ui.selectable_value(&mut root, i as u8, *name);
                        }
                    });
                    egui::ComboBox::from_id_salt("pr-scale").width(120.0).selected_text(scale.name()).show_ui(ui, |ui| {
                        for s in Scale::ALL {
                            ui.selectable_value(&mut scale, s, s.name());
                        }
                    });
                });
                if (root, scale) != (p.key_root, p.scale) {
                    h.checkpoint(p);
                    p.key_root = root;
                    p.scale = scale;
                }
                ui.separator();
                ui.checkbox(&mut self.show_scale, "Shade notes outside the key");
                ui.checkbox(&mut self.show_ghosts, "Show other channels (ghost notes)");
            });

            // (Inline on purpose: a right-aligned layout here would claim all
            // available width and make the resizable window balloon.)
            ui.add_space(8.0);
            let label = if self.maximized { "restore" } else { "full screen" };
            if ui.small_button(label).on_hover_text("Fill the workspace with the piano roll").clicked() {
                self.maximized ^= true;
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn press(
        &mut self,
        pos: Pos2,
        primary: bool,
        mods: egui::Modifiers,
        view: &View,
        overview: Rect,
        ruler: Rect,
        keys_r: Rect,
        vel: Rect,
        pattern_ticks: f32,
        p: &mut Project,
        h: &mut History,
        pat: usize,
        ch_id: ChannelId,
        scale: Scale,
        root: u8,
        out: &mut Vec<Action>,
    ) {
        let free = mods.alt;
        let snap = if free { 1 } else { self.snap };
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
                    let (len, velocity) = (notes[i].len, notes[i].velocity);
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
                    let key = notes[lead_i].key;
                    self.set_default_len(len); // "use this length from now on"
                    self.last_vel = velocity;
                    if near_edge {
                        self.drag = Drag::Resize { anchor_tick, originals, lead, free };
                    } else {
                        self.audition(key, out);
                        self.drag = Drag::Move { anchor_tick, anchor_key, originals, lead, free };
                    }
                }
                None => {
                    // Place a note — or a whole chord — and keep dragging it.
                    let key = anchor_key.clamp(0, 127) as u8;
                    let start = (anchor_tick.max(0.0) as u32 / snap) * snap;
                    let (len, velocity) = (self.default_len(), self.last_vel);
                    let base = notes.len();
                    for k in self.chord.keys(key, scale, root) {
                        notes.push(Note { start, len, key: k, velocity });
                    }
                    self.selection = (base..notes.len()).collect();
                    let originals = self.selection.iter().map(|&j| (j, notes[j].clone())).collect();
                    let first = notes[base].key;
                    self.audition(first, out);
                    self.drag = Drag::Move { anchor_tick, anchor_key, originals, lead: 0, free };
                }
            }
        } else if overview.contains(pos) && primary {
            let (thumb_l, thumb_r) = self.thumb(view, overview, pattern_ticks);
            let span = self.overview_span(view, pattern_ticks);
            let at = (pos.x - overview.left()) / overview.width() * span;
            if (pos.x - thumb_l).abs() < 6.0 {
                self.drag = Drag::ZoomEdge { left: true, fixed: self.scroll_x + view.visible_ticks() };
            } else if (pos.x - thumb_r).abs() < 6.0 {
                self.drag = Drag::ZoomEdge { left: false, fixed: self.scroll_x };
            } else if pos.x > thumb_l && pos.x < thumb_r {
                self.drag = Drag::Pan { grab: at - self.scroll_x };
            } else {
                // Click on the track: centre the view there, then keep panning.
                self.scroll_x = (at - view.visible_ticks() * 0.5).max(0.0);
                self.drag = Drag::Pan { grab: view.visible_ticks() * 0.5 };
            }
        } else if ruler.contains(pos) && primary {
            out.push(Action::Seek(view.tick(pos.x).clamp(0.0, pattern_ticks - 1.0) as f64));
            self.drag = Drag::Seek;
        } else if keys_r.contains(pos) && primary {
            let key = view.key(pos.y).clamp(0, 127) as u8;
            self.audition(key, out);
            self.drag = Drag::Audition;
        } else if vel.contains(pos) && primary {
            h.checkpoint(p);
            self.drag = Drag::Velocity;
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
        mods: egui::Modifiers,
        view: &View,
        vel: Rect,
        p: &mut Project,
        pat: usize,
        ch_id: ChannelId,
        out: &mut Vec<Action>,
    ) {
        let mut drag = std::mem::replace(&mut self.drag, Drag::None);
        let pattern_ticks = (p.patterns[pat].play_length() * TICKS_PER_STEP) as f32;
        let notes = p.patterns[pat].notes_mut(ch_id);
        match &mut drag {
            Drag::Move { anchor_tick, anchor_key, originals, lead, free } => {
                *free |= mods.alt; // latch: once free, free until the mouse is released
                let snap = if *free { 1 } else { self.snap };
                let lead_note = &originals[*lead].1;
                let raw = lead_note.start as f32 + (view.tick(pos.x) - *anchor_tick);
                let snapped = snap_ticks(raw, snap).max(0.0);
                let min_start = originals.iter().map(|(_, n)| n.start).min().unwrap_or(0) as i64;
                let dt = (snapped as i64 - lead_note.start as i64).max(-min_start);
                let (lo, hi) = originals.iter().fold((127i32, 0i32), |(lo, hi), (_, n)| (lo.min(n.key as i32), hi.max(n.key as i32)));
                let dk = (view.key(pos.y) - *anchor_key).clamp(-lo, 127 - hi);
                for (i, n) in originals.iter() {
                    if let Some(note) = notes.get_mut(*i) {
                        note.start = (n.start as i64 + dt) as u32;
                        note.key = (n.key as i32 + dk) as u8;
                    }
                }
                let key = (lead_note.key as i32 + dk) as u8;
                self.audition(key, out);
            }
            Drag::Resize { anchor_tick, originals, lead, free } => {
                *free |= mods.alt;
                let snap = if *free { 1 } else { self.snap };
                let lead_note = &originals[*lead].1;
                let raw = lead_note.len as f32 + (view.tick(pos.x) - *anchor_tick);
                let new_len = snap_ticks(raw, snap).max(snap as f32) as i64;
                let dl = new_len - lead_note.len as i64;
                for (i, n) in originals.iter() {
                    if let Some(note) = notes.get_mut(*i) {
                        note.len = (n.len as i64 + dl).max(snap.min(n.len) as i64) as u32;
                    }
                }
                self.set_default_len(new_len as u32);
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
                    let near = (view.x(n.start as f32) - pos.x).abs() < 6.0;
                    if near && (self.selection.is_empty() || self.selection.contains(&i)) {
                        n.velocity = v;
                    }
                }
                self.last_vel = v;
            }
            Drag::Seek => out.push(Action::Seek(view.tick(pos.x).clamp(0.0, pattern_ticks - 1.0) as f64)),
            Drag::Audition => {
                let key = view.key(pos.y).clamp(0, 127) as u8;
                self.audition(key, out);
            }
            Drag::Pan { grab } => {
                let span = self.overview_span(view, pattern_ticks);
                let at = (pos.x - view.grid.left()) / view.grid.width() * span;
                self.scroll_x = (at - *grab).max(0.0);
            }
            Drag::ZoomEdge { left, fixed } => {
                let span = self.overview_span(view, pattern_ticks);
                let at = ((pos.x - view.grid.left()) / view.grid.width() * span).max(0.0);
                let min_ticks = TICKS_PER_STEP as f32 * 2.0;
                let (a, b) = if *left { (at.min(*fixed - min_ticks), *fixed) } else { (*fixed, at.max(*fixed + min_ticks)) };
                self.ppt = (view.grid.width() / (b - a)).clamp(0.08, 8.0);
                self.scroll_x = a.max(0.0);
            }
            Drag::None => {}
        }
        self.drag = drag;
    }

    /// Total ticks the overview bar represents: the pattern plus some room to grow.
    fn overview_span(&self, v: &View, pattern_ticks: f32) -> f32 {
        (pattern_ticks + TICKS_PER_BAR as f32).max(self.scroll_x + v.visible_ticks())
    }

    /// Screen x of the overview thumb's two ends.
    fn thumb(&self, v: &View, overview: Rect, pattern_ticks: f32) -> (f32, f32) {
        let span = self.overview_span(v, pattern_ticks);
        let l = overview.left() + self.scroll_x / span * overview.width();
        let r = overview.left() + (self.scroll_x + v.visible_ticks()) / span * overview.width();
        (l, r.max(l + 12.0))
    }

    fn paint_grid(&self, painter: &egui::Painter, v: &View, pattern_ticks: f32, scale: Scale, root: u8) {
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
        let end = v.x(pattern_ticks);
        if end < g.right() {
            painter.rect_filled(Rect::from_min_max(pos2(end.max(g.left()), g.top()), g.right_bottom()), cr(0), Color32::from_black_alpha(90));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_notes(&self, painter: &egui::Painter, v: &View, p: &Project, pat: usize, ch: usize, color: Color32, playhead: Option<f64>) {
        let pattern = &p.patterns[pat];
        if self.show_ghosts {
            // Ghosts: other melodic channels' notes, so you can write around them.
            for (other_i, other) in p.channels.iter().enumerate() {
                let melodic = lyra_dsp::info(&other.instrument).is_some_and(|i| i.melodic);
                if other_i == ch || !melodic {
                    continue;
                }
                let c = theme::channel_color(other.color);
                for n in pattern.notes(other.id) {
                    let r = v.note_rect(n).shrink2(vec2(0.5, 1.5));
                    painter.rect_filled(r, cr(1), c.gamma_multiply(0.13));
                }
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
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_overview(&self, painter: &egui::Painter, v: &View, r: Rect, pattern_ticks: f32, p: &Project, pat: usize, ch_id: ChannelId, color: Color32) {
        painter.rect_filled(r, cr(3), theme::INSET);
        let span = self.overview_span(v, pattern_ticks);
        // A tiny silhouette of the channel's notes, so you can see where things are.
        for n in p.patterns[pat].notes(ch_id) {
            let x0 = r.left() + n.start as f32 / span * r.width();
            let x1 = r.left() + (n.start + n.len) as f32 / span * r.width();
            painter.rect_filled(Rect::from_min_max(pos2(x0, r.center().y - 1.0), pos2(x1.max(x0 + 1.0), r.center().y + 1.0)), cr(0), color.gamma_multiply(0.5));
        }
        let (l, rr) = self.thumb(v, r, pattern_ticks);
        let thumb = Rect::from_min_max(pos2(l, r.top() + 1.0), pos2(rr, r.bottom() - 1.0));
        painter.rect_filled(thumb, cr(3), theme::RAISED.gamma_multiply(0.85));
        painter.rect_stroke(thumb, cr(3), stroke(1.0, theme::LINE), StrokeKind::Inside);
        // Grip ticks on both ends: "drag me to zoom".
        for x in [thumb.left() + 3.0, thumb.right() - 3.0] {
            painter.vline(x, thumb.shrink(2.0).y_range(), stroke(1.0, theme::TEXT_DIM));
        }
    }

    fn paint_ruler(&self, painter: &egui::Painter, v: &View, ruler: Rect, pattern_ticks: f32, playhead: f32, playing: bool) {
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
        // Pattern end (where the loop wraps).
        let end = v.x(pattern_ticks);
        if end >= ruler.left() {
            painter.vline(end, ruler.y_range(), stroke(2.0, theme::GOLD.gamma_multiply(0.8)));
        }
        // Playhead: a bright line while playing, a marker (where Play starts) when stopped.
        let x = v.x(playhead);
        if x >= ruler.left() {
            let tri = vec![pos2(x - 5.0, ruler.top() + 2.0), pos2(x + 5.0, ruler.top() + 2.0), pos2(x, ruler.bottom() - 2.0)];
            painter.add(egui::Shape::convex_polygon(tri, theme::MINT, egui::Stroke::NONE));
            let line = if playing { theme::MINT } else { theme::MINT.gamma_multiply(0.35) };
            painter.vline(x, v.grid.y_range(), stroke(1.0, line));
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
