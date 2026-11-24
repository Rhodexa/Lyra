//! The piano roll: edit one channel's notes in the current pattern.
//!
//! Mouse grammar ("it knows what you mean", FL-style):
//! * **click empty** — place a note (or a whole chord, see *Stamp*) with the
//!   length you used last; keep holding to drag it into place
//! * **drag a note** — move · **drag its right end** (a generous zone that reaches a
//!   little past the note) — resize · **Shift+drag** — duplicate
//! * clicking a note also makes its length the new default length
//! * **right-drag** — erase · **Ctrl+drag** — box select (Shift adds)
//! * **Alt** — no snap. Once a drag has gone unsnapped it *stays* unsnapped,
//!   so letting go of Alt a moment early never re-snaps your note.
//! * **wheel** — scroll · **Shift+wheel** — zoom time · **Ctrl+wheel** — sideways ·
//!   **Alt+wheel** — zoom pitch · **middle-drag** — pan · **wheel over a note** —
//!   its velocity (the whole selection's, if the note is selected)
//! * while playing, placing/moving notes stays silent (no preview over the music)
//! * **double right-click a ghost note** — switch to that channel
//! * overview bar (very top) — drag the thumb to pan, drag its ends to zoom
//! * ruler — click/drag to move the playhead · velocity lane — drag to paint ·
//!   keyboard (left) — click to listen
//!
//! Keys (when this window was clicked last): Delete, Ctrl+A, Ctrl+C/X/V (paste
//! lands under the mouse, or at the playhead),
//! Ctrl+D or Ctrl+B (duplicate after), Ctrl+Q (quantize), ↑↓ semitone,
//! Ctrl+↑↓ octave, ←→ nudge by snap, Ctrl+←→ by a beat, Esc.

use crate::widgets;
use crate::keys::Action;
use crate::theme::{self, cr, stroke};
use crate::timeline::{self, Input, Overview, OverviewDrag, RowView, TimeView, VBar, VDrag, Wheel};
use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, StrokeKind, Ui};
use lyra_model::{theory, ChannelId, Chord, History, Note, Project, Scale, TICKS_PER_BAR, TICKS_PER_STEP};

const VEL_H: f32 = 56.0;
const BEAT: u32 = TICKS_PER_STEP * 4;

pub const SNAPS: [(&str, u32); 7] =
    [("Free", 1), ("¼ step", 6), ("½ step", 12), ("Step", 24), ("½ beat", 48), ("Beat", 96), ("Bar", 384)];

enum Drag {
    None,
    /// Moving `originals`; `lead` = the note under the mouse (it's the one that snaps).
    Move {
        anchor_tick: f32,
        anchor_key: i32,
        originals: Vec<(usize, Note)>,
        lead: usize,
        free: bool,
    },
    Resize {
        anchor_tick: f32,
        originals: Vec<(usize, Note)>,
        lead: usize,
        free: bool,
    },
    Select {
        from: Pos2,
        base: Vec<usize>,
    },
    Erase,
    Velocity,
    Seek,
    /// Alt+drag on the ruler: drawing a loop region from `anchor` (ticks).
    /// `moved` false at release = it was an Alt+click, which clears the loop.
    Loop {
        anchor: f32,
        moved: bool,
    },
    Audition,
    /// The overview bar (pan or zoom), shared with the playlist.
    Overview(OverviewDrag),
    /// The vertical bar beside the grid (scroll or zoom the keys).
    VBar(VDrag),
    /// Middle mouse: grab the canvas and move it.
    Hand {
        last: Pos2,
    },
}

pub struct PianoRoll {
    /// Horizontal view: tick at the left edge, pixels per tick.
    time: TimeView,
    /// Key value at the top edge (fractional = smooth scrolling).
    view_top: f32,
    row_h: f32,
    pub snap: u32,
    pub chord: Chord,
    pub show_scale: bool,
    pub show_ghosts: bool,
    selection: Vec<usize>,
    /// The loop region (pattern ticks), if one is drawn; see `timeline::loop_span`.
    pub looped: Option<(u32, u32)>,
    /// Default lengths for new notes: [drum channels, melodic channels].
    last_len: [u32; 2],
    last_vel: f32,
    clipboard: Vec<Note>,
    drag: Drag,
    audition: Option<u8>,
    /// (pattern, channel) being edited — selection resets when it changes.
    editing: Option<(usize, ChannelId)>,
    melodic: bool,
    /// Transport is running: note previews stay quiet.
    playing: bool,
    /// Where the mouse is over the grid (tick), for paste-at-mouse.
    hover_tick: Option<f32>,
    /// A MIDI file dropped on the grid, and the tick: the app reads it into
    /// the instrument being edited (and reports trouble).
    pub dropped_midi: Option<(std::path::PathBuf, u32)>,
    playhead: f64,
}

impl Default for PianoRoll {
    fn default() -> Self {
        Self {
            time: TimeView { scroll: 0.0, ppt: 0.9 },
            view_top: 76.0,
            row_h: theme::Metrics::DEFAULT.row_h,
            snap: TICKS_PER_STEP,
            chord: Chord::Single,
            show_scale: true,
            show_ghosts: true,
            selection: Vec::new(),
            looped: None,
            last_len: [TICKS_PER_STEP, TICKS_PER_STEP * 2],
            last_vel: 0.8,
            clipboard: Vec::new(),
            drag: Drag::None,
            audition: None,
            editing: None,
            melodic: true,
            playing: false,
            hover_tick: None,
            dropped_midi: None,
            playhead: 0.0,
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

/// A note whose right end is under the mouse: inside the note's last
/// `edge_grab` points, or up to `edge_reach` points past it (same row).
fn edge_hit(view: &View, notes: &[Note], pos: Pos2) -> Option<usize> {
    let m = theme::m();
    (0..notes.len()).rev().find(|&i| {
        let r = view.note_rect(&notes[i]);
        let grab = m.edge_grab.min(r.width() * 0.45); // short notes stay movable
        (r.top()..r.bottom()).contains(&pos.y) && pos.x >= r.right() - grab && pos.x <= r.right() + m.edge_reach
    })
}

impl PianoRoll {
    /// Scroll so the channel's notes are in view (called when the roll opens).
    pub fn frame_notes(&mut self, notes: &[Note]) {
        if let Some(top) = notes.iter().map(|n| n.key).max() {
            self.view_top = top as f32 + 8.0;
        }
        self.time.scroll = 0.0;
    }

    fn sanitize(&mut self, pat: usize, ch: ChannelId, len: usize, melodic: bool) {
        if self.editing != Some((pat, ch)) {
            if self.editing.is_some_and(|(was, _)| was != pat) {
                self.looped = None; // a loop belongs to its pattern
            }
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

    /// The selected notes of (pattern, channel), if the roll is showing them.
    fn selected<'a>(&self, p: &'a Project, pat: usize, ch: usize) -> Vec<&'a Note> {
        let id = p.channels[ch].id;
        if self.editing != Some((pat, id)) {
            return Vec::new();
        }
        let notes = p.patterns[pat].notes(id);
        self.selection.iter().filter_map(|&i| notes.get(i)).collect()
    }

    /// The bars an export of "these bars" covers: the loop region, else the
    /// whole bars the selected notes are in.
    fn bars(&self, p: &Project, pat: usize, ch: usize) -> Option<(u32, u32)> {
        if let Some(range) = self.looped {
            return Some(range);
        }
        let sel = self.selected(p, pat, ch);
        let from = sel.iter().map(|n| n.start).min()?;
        let to = sel.iter().map(|n| n.start + n.len).max()?;
        Some((from / TICKS_PER_BAR * TICKS_PER_BAR, to.div_ceil(TICKS_PER_BAR).max(1) * TICKS_PER_BAR))
    }

    /// What the context bar (and F3) offers here right now.
    pub fn context(&self, p: &Project, pat: usize, ch: usize) -> Vec<(Action, String)> {
        let mut out = Vec::new();
        if !self.selected(p, pat, ch).is_empty() {
            out.push((Action::ExportNotesMidi, "Export notes as MIDI".to_string()));
        }
        if let Some((a, b)) = self.bars(p, pat, ch) {
            out.push((Action::ExportBarsMidi, format!("Export {} as MIDI", crate::midi::bars_label(a, b))));
        }
        out.push((Action::ExportPatternMidi, "Export pattern as MIDI".to_string()));
        if self.looped.is_some() {
            out.push((Action::ClearLoop, "Clear loop".to_string()));
        }
        out
    }

    /// The MIDI an export action asks for (None: nothing to export).
    pub fn midi_export(&self, a: Action, p: &Project, pat: usize, ch: usize) -> Option<crate::midi::Export> {
        use crate::midi::{bars_label, section, Export};
        let pattern = &p.patterns[pat];
        let every = |from: u32, to: u32| {
            p.channels.iter().map(move |c| (c.name.clone(), section(pattern.notes(c.id), from, to))).collect::<Vec<_>>()
        };
        match a {
            Action::ExportNotesMidi => {
                // From the bar the first note is in, so the notes keep their place in the beat.
                let sel: Vec<Note> = self.selected(p, pat, ch).into_iter().cloned().collect();
                let from = sel.iter().map(|n| n.start).min()? / TICKS_PER_BAR * TICKS_PER_BAR;
                let to = sel.iter().map(|n| n.start + n.len).max()?;
                let name = format!("{} {}", pattern.name, p.channels[ch].name);
                Export::new(name, to - from, [(p.channels[ch].name.clone(), section(&sel, from, to))])
            }
            Action::ExportBarsMidi => {
                let (from, to) = self.bars(p, pat, ch)?;
                Export::new(format!("{} {}", pattern.name, bars_label(from, to)), to - from, every(from, to))
            }
            Action::ExportPatternMidi => {
                let len = pattern.play_length() * TICKS_PER_STEP;
                Export::new(pattern.name.clone(), len, every(0, len))
            }
            _ => None,
        }
    }

    /// Keyboard commands routed here by the app. Returns true if handled.
    pub fn action(&mut self, a: Action, p: &mut Project, h: &mut History, pat: usize, ch: usize) -> bool {
        let id = p.channels[ch].id;
        let len = p.patterns[pat].notes(id).len();
        let melodic = lyra_catalog::info(&p.channels[ch].instrument).is_some_and(|i| i.melodic);
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
                // Lands under the mouse if it's over the grid, else at the playhead;
                // keeps the copied rhythm and pitches. Comes in selected, ready to drag.
                h.checkpoint(p);
                let at = self.hover_tick.unwrap_or(self.playhead as f32).max(0.0) as u32;
                let at = (at / snap) * snap;
                let first = self.clipboard.iter().map(|n| n.start).min().unwrap_or(0);
                let notes = p.patterns[pat].notes_mut(id);
                let base = notes.len();
                notes.extend(self.clipboard.iter().map(|n| Note { start: n.start - first + at, ..n.clone() }));
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
                    notes[i].start = timeline::snap(notes[i].start as f32, snap) as u32;
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
        let melodic = lyra_catalog::info(&p.channels[ch].instrument).is_some_and(|i| i.melodic);
        self.sanitize(pat, ch_id, p.patterns[pat].notes(ch_id).len(), melodic);
        self.playing = playing;
        self.playhead = playhead;
        let m = theme::m();

        // ── Layout ──
        let size = ui.available_size().max(vec2(320.0, 220.0));
        // Hover only: the editor reads the mouse itself, and a sensing canvas would
        // win egui's hit test over the docked panels' resize edges.
        let (rect, _resp) = ui.allocate_exact_size(size, Sense::hover());
        // The vertical bar runs down the right of the grid, thinner than the overview.
        let vbar_w = VBar::width();
        let right = rect.right() - vbar_w - 2.0;
        let overview_r =
            Rect::from_min_max(pos2(rect.left() + m.keys_w, rect.top()), pos2(right, rect.top() + m.overview_h));
        let ruler = Rect::from_min_max(
            pos2(overview_r.left(), overview_r.bottom() + 2.0),
            pos2(right, overview_r.bottom() + 2.0 + m.ruler_h),
        );
        let grid = Rect::from_min_max(pos2(overview_r.left(), ruler.bottom()), pos2(right, rect.bottom() - VEL_H));
        let keys_r = Rect::from_min_max(pos2(rect.left(), grid.top()), pos2(grid.left(), grid.bottom()));
        let vel = Rect::from_min_max(pos2(grid.left(), grid.bottom() + 3.0), pos2(right, rect.bottom()));
        let vbar = VBar { rect: Rect::from_min_max(pos2(right + 2.0, grid.top()), pos2(rect.right(), grid.bottom())) };

        let input = timeline::Input::read(ui, rect);
        let Input {
            pos,
            pressed: pressed_p,
            pressed_right: pressed_s,
            pressed_middle: pressed_m,
            down,
            mods,
            hovering,
            ..
        } = input;
        let double_r = ui.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Secondary));
        let pattern_ticks = (p.patterns[pat].play_length() * TICKS_PER_STEP) as f32;
        let overview = Overview { rect: overview_r, area: grid, content: pattern_ticks };
        let over_bars = pos.is_some_and(|p| overview_r.contains(p) || vbar.rect.contains(p));

        // ── Wheel ──
        if hovering && over_bars {
            // Wheel over a scrollbar zooms its axis, anchored at the top-left edge.
            if pos.is_some_and(|p| overview_r.contains(p)) {
                overview.wheel_zoom(&mut self.time, input.wheel.y, 0.08, 8.0);
            } else {
                let mut rv = self.rows(grid);
                vbar.wheel_zoom(&mut rv, input.wheel.y, grid.height() / 36.0, grid.height() / 6.0);
                self.set_rows(rv, grid);
            }
        } else if hovering {
            let view = self.view(grid);
            let hovered_note =
                pos.filter(|p| grid.contains(*p)).and_then(|pos| hit(&view, p.patterns[pat].notes(ch_id), pos));
            match timeline::wheel(&input, ruler) {
                Wheel::ZoomTime { factor, at } => self.zoom_time(factor, at, grid),
                Wheel::ZoomRows { factor, at } => {
                    // Pitch zoom, keeping the key under the mouse in place.
                    let anchor = self.view_top - (at - grid.top()) / self.row_h;
                    self.row_h = (self.row_h * factor).clamp(6.0, 36.0);
                    self.view_top = anchor + (at - grid.top()) / self.row_h;
                }
                Wheel::Scroll { y, .. } if y != 0.0 && hovered_note.is_some() && !mods.command => {
                    // Wheel over a note: velocity (of the selection, if that note is in it).
                    let i = hovered_note.unwrap_or(0);
                    h.checkpoint(p);
                    let targets = if self.selection.contains(&i) { self.selection.clone() } else { vec![i] };
                    let notes = p.patterns[pat].notes_mut(ch_id);
                    for t in targets {
                        notes[t].velocity = (notes[t].velocity - y.signum() * 0.05).clamp(0.05, 1.0);
                    }
                }
                Wheel::Scroll { x, y } => {
                    self.time.scroll += x / self.time.ppt;
                    self.view_top -= y / self.row_h; // rows count down from the top key
                }
                Wheel::None => {}
            }
        }
        let rows = grid.height() / self.row_h;
        self.view_top = self.view_top.clamp(rows.min(128.0), 128.0);
        self.time.scroll = self.time.scroll.max(0.0);
        let view = self.view(grid);
        self.hover_tick = pos.filter(|p| hovering && grid.contains(*p)).map(|p| view.tick(p.x));
        // A MIDI file from the browser: its notes land where it's dropped (on the grid).
        if let Some(tick) = self.hover_tick {
            if egui::DragAndDrop::has_payload_of_type::<crate::midi::MidiDrag>(ui.ctx()) {
                let at = timeline::snap(tick.max(0.0), self.snap).max(0.0) as u32;
                let x = view.x(at as f32);
                drop_line(ui, x, grid);
                if ui.input(|i| i.pointer.any_released()) {
                    if let Some(d) = egui::DragAndDrop::take_payload::<crate::midi::MidiDrag>(ui.ctx()) {
                        self.dropped_midi = Some((d.0.clone(), at));
                    }
                }
            }
        }

        // Double right-click on a ghost note: jump to the channel it belongs to.
        if let (true, true, Some(pos)) = (hovering && double_r, self.show_ghosts, pos) {
            if grid.contains(pos) && hit(&view, p.patterns[pat].notes(ch_id), pos).is_none() {
                let ghost_owner = p.channels.iter().enumerate().find(|(i, c)| {
                    *i != ch
                        && lyra_catalog::info(&c.instrument).is_some_and(|inst| inst.melodic)
                        && hit(&view, p.patterns[pat].notes(c.id), pos).is_some()
                });
                if let Some((i, _)) = ghost_owner {
                    out.push(Action::SelectChannel(i));
                }
            }
        }

        // ── Mouse ──
        let (scale, root) = (p.scale, p.key_root);
        if hovering && pressed_m {
            if let Some(pos) = pos {
                self.drag = Drag::Hand { last: pos };
            }
        } else if hovering && (pressed_p || pressed_s) {
            if let Some(pos) = pos {
                self.press(
                    pos,
                    pressed_p,
                    mods,
                    &view,
                    (&overview, &vbar),
                    ruler,
                    keys_r,
                    vel,
                    pattern_ticks,
                    p,
                    h,
                    pat,
                    ch_id,
                    scale,
                    root,
                    out,
                );
            }
        }
        if down {
            if let Some(pos) = pos {
                // Held at or past an edge while moving, resizing or selecting:
                // the view scrolls, and the drag follows (a note goes up an
                // octave by holding it at the top edge).
                let horizontal = matches!(self.drag, Drag::Move { .. } | Drag::Resize { .. } | Drag::Select { .. });
                let vertical = matches!(self.drag, Drag::Move { .. } | Drag::Select { .. });
                if horizontal {
                    let egui::Vec2 { x: dx, y: dy } = timeline::edge_pan_2d(pos, grid, input.dt, vertical);
                    if dx != 0.0 || dy != 0.0 {
                        let (scroll0, top0) = (self.time.scroll, self.view_top);
                        self.time.scroll = (self.time.scroll + dx / self.time.ppt).max(0.0);
                        self.view_top = (self.view_top - dy / self.row_h).clamp(rows.min(128.0), 128.0);
                        // A rubber band keeps its anchor on the music, not on the screen.
                        if let Drag::Select { from, .. } = &mut self.drag {
                            from.x -= (self.time.scroll - scroll0) * self.time.ppt;
                            from.y += (self.view_top - top0) * self.row_h;
                        }
                        ui.ctx().request_repaint();
                    }
                }
                let view = self.view(grid);
                self.drag_to(pos, mods, &view, vel, (&overview, &vbar), p, pat, ch_id, out);
            }
        } else if !matches!(self.drag, Drag::None) {
            if let Some(k) = self.audition.take() {
                out.push(Action::NoteOff(k));
            }
            if matches!(self.drag, Drag::Loop { moved: false, .. }) {
                self.looped = None; // Alt+click: no loop
            }
            self.drag = Drag::None;
        }

        // Cursor feedback
        if let (true, Some(pos), Drag::None) = (hovering && grid.contains(pos.unwrap_or_default()), pos, &self.drag) {
            let notes = p.patterns[pat].notes(ch_id);
            if edge_hit(&view, notes, pos).is_some() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            } else if hit(&view, notes, pos).is_some() {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
            }
        } else if matches!(
            self.drag,
            Drag::Move { .. }
                | Drag::Hand { .. }
                | Drag::Overview(OverviewDrag::Pan { .. })
                | Drag::VBar(VDrag::Pan { .. })
        ) {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if matches!(self.drag, Drag::VBar(VDrag::Edge { .. })) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
        } else if matches!(self.drag, Drag::Resize { .. } | Drag::Overview(OverviewDrag::Edge { .. })) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }

        // ── Paint ──
        let view = self.view(grid); // the drag may have scrolled or zoomed
        let painter = ui.painter_at(rect);
        let play = playhead as f32;
        self.paint_grid(&painter, &view, pattern_ticks, scale, root);
        self.paint_notes(&painter, &view, p, pat, ch, color, playing.then_some(playhead));
        if let (Drag::Select { from, .. }, Some(pos)) = (&self.drag, pos) {
            timeline::paint_rubber_band(&painter, *from, pos);
        }
        overview.paint(&painter, &self.time, matches!(self.drag, Drag::Overview(_)));
        // A tiny silhouette of the channel's notes, so you can see where things are.
        for n in p.patterns[pat].notes(ch_id) {
            let x0 = overview.x_of(&self.time, n.start as f32);
            let x1 = overview.x_of(&self.time, (n.start + n.len) as f32);
            let y = overview_r.center().y;
            painter.rect_filled(
                Rect::from_min_max(pos2(x0, y - 1.0), pos2(x1.max(x0 + 1.0), y + 1.0)),
                cr(0),
                color.gamma_multiply(0.5),
            );
        }
        vbar.paint(&painter, &self.rows(grid), matches!(self.drag, Drag::VBar(_)));
        // Playhead: a bright line while playing, a dim marker (where Play starts) when stopped.
        timeline::paint_ruler(&painter, &self.time, ruler, pattern_ticks, Some((play, playing)), grid);
        if let Some(range) = self.looped {
            timeline::paint_loop(&painter, &self.time, ruler, grid, range);
        }
        if let Some(pos) = pos.filter(|p| hovering && (grid.contains(*p) || ruler.contains(*p))) {
            timeline::paint_cursor(&painter, pos.x, ruler, grid);
        }
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
        View { grid, scroll_x: self.time.scroll, ppt: self.time.ppt, view_top: self.view_top, row_h: self.row_h }
    }

    /// Zoom the time axis by `factor`, keeping the tick under screen-x `anchor_x` still.
    fn zoom_time(&mut self, factor: f32, anchor_x: f32, grid: Rect) {
        self.time.zoom_at(grid, factor, anchor_x, 0.08, 8.0);
    }

    /// The keys as rows for the vertical bar: row 0 is key 127 at the top.
    fn rows(&self, grid: Rect) -> RowView {
        RowView { start: 128.0 - self.view_top, visible: grid.height() / self.row_h, total: 128.0 }
    }

    /// Apply a vertical-bar change (scroll and/or row height).
    fn set_rows(&mut self, rv: RowView, grid: Rect) {
        self.row_h = (grid.height() / rv.visible.max(1.0)).clamp(6.0, 36.0);
        self.view_top = 128.0 - rv.start;
    }

    /// The piano roll's controls; the area draws them on its header row.
    pub fn toolbar(&mut self, ui: &mut Ui, p: &mut Project, h: &mut History, color: Color32, ch: usize) {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 4.0, color);
            ui.label(egui::RichText::new(&p.channels[ch].name).strong());
            ui.add_space(10.0);

            ui.label(egui::RichText::new("snap").small().color(theme::TEXT_DIM));
            let snap_name = SNAPS.iter().find(|s| s.1 == self.snap).map_or("?", |s| s.0);
            let _ = snap_name;
            let rows: Vec<_> = SNAPS.iter().map(|(n, _)| widgets::DropRow::text(*n)).collect();
            let at = SNAPS.iter().position(|s| s.1 == self.snap).unwrap_or(0);
            if let Some(i) = widgets::dropdown(ui, "pr-snap", 96.0, &rows, at, false, "") {
                self.snap = SNAPS[i].1;
            }

            ui.label(egui::RichText::new("stamp").small().color(theme::TEXT_DIM));
            let rows: Vec<_> = Chord::ALL.iter().map(|c| widgets::DropRow::text(c.name())).collect();
            let at = Chord::ALL.iter().position(|c| *c == self.chord).unwrap_or(0);
            let hint = "What one click places. 'In key' chords always fit the song's key.";
            if let Some(i) = widgets::dropdown(ui, "pr-chord", 130.0, &rows, at, false, hint) {
                self.chord = Chord::ALL[i];
            }

            // Key + view options live in one menu, out of the way.
            let key_label = format!("{} {}", theory::NOTE_NAMES[p.key_root as usize], p.scale.name().to_lowercase());
            ui.menu_button(format!("View · {key_label}"), |ui| {
                ui.label(egui::RichText::new("SONG KEY").small().color(theme::TEXT_DIM));
                let (mut root, mut scale) = (p.key_root, p.scale);
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("pr-root")
                        .width(44.0)
                        .selected_text(theory::NOTE_NAMES[root as usize])
                        .show_ui(ui, |ui| {
                            for (i, name) in theory::NOTE_NAMES.iter().enumerate() {
                                ui.selectable_value(&mut root, i as u8, *name);
                            }
                        });
                    egui::ComboBox::from_id_salt("pr-scale").width(120.0).selected_text(scale.name()).show_ui(
                        ui,
                        |ui| {
                            for s in Scale::ALL {
                                ui.selectable_value(&mut scale, s, s.name());
                            }
                        },
                    );
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
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn press(
        &mut self,
        pos: Pos2,
        primary: bool,
        mods: egui::Modifiers,
        view: &View,
        (overview, vbar): (&Overview, &VBar),
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
            let edge = edge_hit(view, notes, pos);
            match edge.or_else(|| hit(view, notes, pos)) {
                Some(i) => {
                    if !self.selection.contains(&i) {
                        self.selection = vec![i];
                    }
                    let (len, velocity) = (notes[i].len, notes[i].velocity);
                    let near_edge = edge.is_some();
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
                        self.preview(key, out);
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
                    self.preview(first, out);
                    self.drag = Drag::Move { anchor_tick, anchor_key, originals, lead: 0, free };
                }
            }
        } else if overview.rect.contains(pos) && primary {
            self.drag = Drag::Overview(overview.press(&mut self.time, pos));
        } else if vbar.rect.contains(pos) && primary {
            let mut rv = self.rows(view.grid);
            self.drag = Drag::VBar(vbar.press(&mut rv, pos));
            self.set_rows(rv, view.grid);
        } else if ruler.contains(pos) && primary && mods.alt {
            self.drag = Drag::Loop { anchor: view.tick(pos.x), moved: false };
        } else if ruler.contains(pos) && primary {
            // The piano roll's ruler is about *this pattern*: hear it from here.
            // (In song mode the app finds where this pattern plays in the song.)
            out.push(Action::SeekPattern(view.tick(pos.x).clamp(0.0, pattern_ticks - 1.0) as f64));
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

    /// A note preview while editing — silent while the song is playing.
    fn preview(&mut self, key: u8, out: &mut Vec<Action>) {
        if !self.playing {
            self.audition(key, out);
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
        (overview, vbar): (&Overview, &VBar),
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
                let start = lead_note.start as f32;
                let to = timeline::drag_to_grid(start, *anchor_tick, view.tick(pos.x), view.ppt, snap);
                let snapped = to.unwrap_or(start).max(0.0);
                let min_start = originals.iter().map(|(_, n)| n.start).min().unwrap_or(0) as i64;
                let dt = (snapped as i64 - lead_note.start as i64).max(-min_start);
                let (lo, hi) = originals
                    .iter()
                    .fold((127i32, 0i32), |(lo, hi), (_, n)| (lo.min(n.key as i32), hi.max(n.key as i32)));
                let dk = (view.key(pos.y) - *anchor_key).clamp(-lo, 127 - hi);
                for (i, n) in originals.iter() {
                    if let Some(note) = notes.get_mut(*i) {
                        note.start = (n.start as i64 + dt) as u32;
                        note.key = (n.key as i32 + dk) as u8;
                    }
                }
                let key = (lead_note.key as i32 + dk) as u8;
                self.preview(key, out);
            }
            Drag::Resize { anchor_tick, originals, lead, free } => {
                *free |= mods.alt;
                let snap = if *free { 1 } else { self.snap };
                let lead_note = &originals[*lead].1;
                let to = timeline::drag_to_grid(lead_note.len as f32, *anchor_tick, view.tick(pos.x), view.ppt, snap);
                let new_len = to.map_or(lead_note.len as i64, |t| t.max(snap as f32) as i64);
                let dl = new_len - lead_note.len as i64;
                for (i, n) in originals.iter() {
                    if let Some(note) = notes.get_mut(*i) {
                        note.len = (n.len as i64 + dl).max(snap.min(n.len) as i64) as u32;
                    }
                }
                if to.is_some() {
                    self.set_default_len(new_len as u32);
                }
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
            Drag::Seek => out.push(Action::SeekPattern(view.tick(pos.x).clamp(0.0, pattern_ticks - 1.0) as f64)),
            Drag::Loop { anchor, moved } => {
                let tick = view.tick(pos.x).clamp(0.0, pattern_ticks);
                *moved |= (tick - *anchor).abs() * view.ppt >= 3.0;
                if *moved {
                    self.looped = Some(timeline::loop_span(*anchor, tick, self.snap));
                }
            }
            Drag::Audition => {
                let key = view.key(pos.y).clamp(0, 127) as u8;
                self.audition(key, out);
            }
            Drag::Overview(od) => overview.drag(&mut self.time, *od, pos, TICKS_PER_STEP as f32 * 2.0, 8.0),
            Drag::VBar(vd) => {
                let mut rv = self.rows(view.grid);
                vbar.drag(&mut rv, *vd, pos, view.grid.height() / 36.0, view.grid.height() / 6.0);
                self.set_rows(rv, view.grid);
            }
            Drag::Hand { last } => {
                let d = pos - *last;
                self.time.scroll = (self.time.scroll - d.x / self.time.ppt).max(0.0);
                self.view_top += d.y / self.row_h;
                *last = pos;
            }
            Drag::None => {}
        }
        self.drag = drag;
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
            painter.rect_filled(
                Rect::from_min_max(pos2(end.max(g.left()), g.top()), g.right_bottom()),
                cr(0),
                Color32::from_black_alpha(90),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_notes(
        &self,
        painter: &egui::Painter,
        v: &View,
        p: &Project,
        pat: usize,
        ch: usize,
        color: Color32,
        playhead: Option<f64>,
    ) {
        let pattern = &p.patterns[pat];
        if self.show_ghosts {
            // Ghosts: other melodic channels' notes, so you can write around them.
            for (other_i, other) in p.channels.iter().enumerate() {
                let melodic = lyra_catalog::info(&other.instrument).is_some_and(|i| i.melodic);
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
            // Flat keys: a dark recess outline, a flat fill in the channel's
            // light (softer notes dimmer), brighter while sounding. Selected
            // notes get a white ring outside the dark one, which reads on any
            // note colour and on the grid.
            let mut fill = theme::shade(color, 0.6 + 0.4 * n.velocity);
            if playing {
                fill = theme::shade(fill, 1.12);
            }
            // Slightly see-through, so notes that overlap (or hide each other) show.
            painter.rect_filled(r, cr(3), theme::KEY_RECESS.gamma_multiply(0.9));
            painter.rect_filled(r.shrink(1.0), cr(2), fill.gamma_multiply(0.78));
            if selected {
                painter.rect_stroke(r, cr(3), stroke(1.5, theme::TEXT), StrokeKind::Outside);
            }
            if r.width() > 30.0 && v.row_h >= 10.0 {
                painter.text(
                    r.left_center() + vec2(4.0, 0.0),
                    Align2::LEFT_CENTER,
                    theory::note_name(n.key),
                    FontId::proportional(9.0),
                    theme::key_ink(true),
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_keys(
        &self,
        painter: &egui::Painter,
        v: &View,
        r: Rect,
        color: Color32,
        live: &[u8],
        sounding: &[u8],
        scale: Scale,
        root: u8,
    ) {
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
                    painter.hline(r.x_range(), y + v.row_h, stroke(1.0, theme::KEY_EDGE));
                    // B|C and E|F seams
                }
            }
            if self.show_scale && !scale.contains(root, key) && !theory::is_black(key) && !lit {
                painter.rect_filled(row, cr(0), Color32::from_black_alpha(40));
            }
            if key.is_multiple_of(12) {
                painter.text(
                    pos2(r.right() - 4.0, y + v.row_h * 0.5),
                    Align2::RIGHT_CENTER,
                    theory::note_name(key),
                    FontId::proportional(8.5),
                    theme::KEY_BLACK,
                );
            }
        }
        painter.vline(r.right() - 0.5, r.y_range(), stroke(1.0, theme::LINE));
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_velocity(
        &self,
        painter: &egui::Painter,
        v: &View,
        r: Rect,
        p: &Project,
        pat: usize,
        ch_id: ChannelId,
        color: Color32,
    ) {
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

/// Where a dropped file will land: a bright line down the grid.
fn drop_line(ui: &Ui, x: f32, grid: Rect) {
    ui.painter().vline(x, grid.y_range(), stroke(2.0, theme::MINT));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_take_the_selection_its_bars_or_the_pattern() {
        let mut p = Project::default();
        let lead = p.add_channel("Lead", "lyra.synth", lyra_catalog::default_params("lyra.synth"));
        let bass = p.add_channel("Bass", "lyra.synth", lyra_catalog::default_params("lyra.synth"));
        let bar = TICKS_PER_BAR;
        let n = |start, key| Note { start, len: 96, key, velocity: 0.8 };
        p.patterns[0].notes_mut(lead).extend([n(0, 60), n(bar + 96, 64), n(2 * bar, 67)]);
        p.patterns[0].notes_mut(bass).extend([n(bar, 36), n(2 * bar, 38)]);
        let mut roll = PianoRoll { editing: Some((0, lead)), selection: vec![1], ..Default::default() };

        // The selected note, from the start of its bar (so it keeps its beat).
        let notes = roll.midi_export(Action::ExportNotesMidi, &p, 0, 0).unwrap();
        assert_eq!(notes.tracks.len(), 1);
        assert_eq!(notes.tracks[0].notes[0].start, 96);
        // Its bar, every instrument.
        let bars = roll.midi_export(Action::ExportBarsMidi, &p, 0, 0).unwrap();
        assert_eq!(bars.name, "Pattern 1 bar 2");
        let keys: Vec<Vec<u8>> = bars.tracks.iter().map(|t| t.notes.iter().map(|n| n.key).collect()).collect();
        assert_eq!(keys, vec![vec![64], vec![36]], "only bar 2, lead and bass");
        assert_eq!(bars.tracks[1].notes[0].start, 0, "moved to the start");
        // A loop region wins over the selection.
        roll.looped = Some((2 * bar, 3 * bar));
        let looped = roll.midi_export(Action::ExportBarsMidi, &p, 0, 0).unwrap();
        assert_eq!(looped.tracks.iter().map(|t| t.notes[0].key).collect::<Vec<_>>(), vec![67, 38]);
        // The whole pattern.
        let all = roll.midi_export(Action::ExportPatternMidi, &p, 0, 0).unwrap();
        assert_eq!(all.tracks.iter().map(|t| t.notes.len()).sum::<usize>(), 5);
    }
}
