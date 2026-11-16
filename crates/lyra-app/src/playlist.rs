//! The playlist: arrange patterns into a song.
//!
//! Mouse grammar (the same as the piano roll, so hands only learn it once):
//! * **click empty** — place the current pattern (the one in the top bar);
//!   keep holding to drag it into place
//! * **drag a clip** — move (also to another track) · **drag its right end** —
//!   resize (longer than the pattern = the pattern repeats) · **Shift+drag** — duplicate
//! * **click a clip** — makes its pattern the current one · **double-click** — open it in the rack
//! * **right-drag** — delete · **Ctrl+drag** — box select (Shift adds) · **Alt** — no snap
//! * **wheel** — scroll tracks · **Shift+wheel** — sideways · **Ctrl+wheel** — zoom time ·
//!   **Alt+wheel** — track height · **middle-drag** — pan
//! * overview bar — drag the thumb to pan, its ends to zoom · ruler — click to
//!   play the song from there · track lights — mute a track
//!
//! Keys (when this window was clicked last): Delete, Ctrl+A, Ctrl+D/B (duplicate
//! after), ←→ move by snap (Ctrl = 4 bars), ↑↓ move to the track above/below, Esc.

use crate::app::{Focus, UiState};
use crate::keys::Action;
use crate::theme::{self, cr, stroke};
use crate::timeline::{self, Overview, OverviewDrag, RowView, TimeView, VBar, VDrag};
use crate::widgets;
use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, StrokeKind, Ui};
use lyra_model::{Clip, History, Project, TICKS_PER_BAR, TICKS_PER_STEP};

const HEADER_W: f32 = 150.0;
const BEAT: u32 = TICKS_PER_STEP * 4;

pub const SNAPS: [(&str, u32); 4] = [("Free", 1), ("Step", TICKS_PER_STEP), ("Beat", BEAT), ("Bar", TICKS_PER_BAR)];

enum Drag {
    None,
    Move {
        anchor_tick: f32,
        anchor_track: i32,
        originals: Vec<(usize, Clip)>,
        lead: usize,
        free: bool,
    },
    Resize {
        anchor_tick: f32,
        originals: Vec<(usize, Clip)>,
        lead: usize,
        free: bool,
    },
    Select {
        from: Pos2,
        base: Vec<usize>,
    },
    Erase,
    Seek,
    Hand {
        last: Pos2,
    },
    Overview(OverviewDrag),
    /// The vertical bar beside the grid (scroll or zoom the tracks).
    VBar(VDrag),
}

pub struct Playlist {
    time: TimeView,
    /// First visible track (fractional = smooth scrolling).
    scroll_y: f32,
    track_h: f32,
    pub snap: u32,
    selection: Vec<usize>,
    drag: Drag,
}

impl Default for Playlist {
    fn default() -> Self {
        Self {
            time: TimeView { scroll: 0.0, ppt: 64.0 / TICKS_PER_BAR as f32 },
            scroll_y: 0.0,
            track_h: 52.0,
            snap: TICKS_PER_BAR,
            selection: Vec::new(),
            drag: Drag::None,
        }
    }
}

/// Screen ↔ song coordinates for one frame.
#[derive(Clone, Copy)]
struct View {
    grid: Rect,
    time: TimeView,
    scroll_y: f32,
    track_h: f32,
}

impl View {
    fn x(&self, tick: f32) -> f32 {
        self.time.x(self.grid, tick)
    }
    fn tick(&self, x: f32) -> f32 {
        self.time.tick(self.grid, x)
    }
    fn y(&self, track: usize) -> f32 {
        self.grid.top() + (track as f32 - self.scroll_y) * self.track_h
    }
    fn track(&self, y: f32) -> i32 {
        ((y - self.grid.top()) / self.track_h + self.scroll_y).floor() as i32
    }
    fn clip_rect(&self, c: &Clip) -> Rect {
        let y = self.y(c.track);
        Rect::from_min_max(pos2(self.x(c.start as f32), y + 1.0), pos2(self.x(c.end() as f32), y + self.track_h - 1.0))
    }
}

fn hit(v: &View, clips: &[Clip], pos: Pos2) -> Option<usize> {
    (0..clips.len()).rev().find(|&i| v.clip_rect(&clips[i]).contains(pos))
}

fn edge_hit(v: &View, clips: &[Clip], pos: Pos2) -> Option<usize> {
    let m = theme::m();
    (0..clips.len()).rev().find(|&i| {
        let r = v.clip_rect(&clips[i]);
        let grab = m.edge_grab.min(r.width() * 0.4);
        r.y_range().contains(pos.y) && pos.x >= r.right() - grab && pos.x <= r.right() + m.edge_reach
    })
}

fn snapped(raw: f32, snap: u32) -> f32 {
    (raw / snap as f32).round() * snap as f32
}

impl Playlist {
    /// The tracks as rows for the vertical bar.
    fn rows(&self, grid: Rect, tracks: usize) -> RowView {
        RowView { start: self.scroll_y, visible: grid.height() / self.track_h, total: tracks as f32 }
    }

    /// Apply a vertical-bar change (scroll and/or track height).
    fn set_rows(&mut self, rv: RowView, grid: Rect) {
        self.track_h = (grid.height() / rv.visible.max(0.1)).clamp(24.0, 140.0);
        self.scroll_y = rv.start;
    }

    fn view(&self, grid: Rect) -> View {
        View { grid, time: self.time, scroll_y: self.scroll_y, track_h: self.track_h }
    }

    /// Keyboard commands routed here by the app. Returns true if handled.
    pub fn action(&mut self, a: Action, p: &mut Project, h: &mut History) -> bool {
        self.selection.retain(|&i| i < p.clips.len());
        match a {
            Action::SelectAll => self.selection = (0..p.clips.len()).collect(),
            Action::Deselect => self.selection.clear(),
            Action::Delete if !self.selection.is_empty() => {
                h.checkpoint(p);
                let mut sel = std::mem::take(&mut self.selection);
                sel.sort_unstable_by(|a, b| b.cmp(a));
                for i in sel {
                    p.clips.remove(i);
                }
            }
            Action::Duplicate if !self.selection.is_empty() => {
                h.checkpoint(p);
                let first = self.selection.iter().map(|&i| p.clips[i].start).min().unwrap_or(0);
                let last = self.selection.iter().map(|&i| p.clips[i].end()).max().unwrap_or(0);
                let shift = (last - first).div_ceil(TICKS_PER_BAR).max(1) * TICKS_PER_BAR;
                let base = p.clips.len();
                for &i in &self.selection.clone() {
                    let mut c = p.clips[i].clone();
                    c.start += shift;
                    p.clips.push(c);
                }
                self.selection = (base..p.clips.len()).collect();
            }
            Action::Arrow { dx, dy, big } if !self.selection.is_empty() => {
                let dt = dx as i64 * if big { 4 * TICKS_PER_BAR as i64 } else { self.snap as i64 };
                let tracks = p.tracks.len() as i64;
                let ok = self.selection.iter().all(|&i| {
                    let c = &p.clips[i];
                    c.start as i64 + dt >= 0 && (0..tracks).contains(&(c.track as i64 - dy as i64))
                });
                if ok {
                    h.checkpoint(p);
                    for &i in &self.selection {
                        let c = &mut p.clips[i];
                        c.start = (c.start as i64 + dt) as u32;
                        c.track = (c.track as i64 - dy as i64) as usize; // ↑ = the track above
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
        st: &mut UiState,
        playhead: f64,
        playing: bool,
        out: &mut Vec<Action>,
    ) {
        let m = theme::m();
        self.selection.retain(|&i| i < p.clips.len());
        self.toolbar(ui, p, st);
        ui.add_space(3.0);

        let size = ui.available_size().max(vec2(420.0, 200.0));
        // Hover only: the editor reads the mouse itself, and a sensing canvas would
        // win egui's hit test over the docked panels' resize edges.
        let (rect, _resp) = ui.allocate_exact_size(size, Sense::hover());
        // The vertical bar runs down the right of the grid, thinner than the overview.
        let vbar_w = (m.overview_h * 0.6).round();
        let right = rect.right() - vbar_w - 2.0;
        let overview_r =
            Rect::from_min_max(pos2(rect.left() + HEADER_W, rect.top()), pos2(right, rect.top() + m.overview_h));
        let ruler = Rect::from_min_max(
            pos2(overview_r.left(), overview_r.bottom() + 2.0),
            pos2(right, overview_r.bottom() + 2.0 + m.ruler_h),
        );
        let grid = Rect::from_min_max(pos2(ruler.left(), ruler.bottom()), pos2(right, rect.bottom()));
        let vbar = VBar { rect: Rect::from_min_max(pos2(right + 2.0, grid.top()), pos2(rect.right(), grid.bottom())) };
        let headers = Rect::from_min_max(pos2(rect.left(), grid.top()), pos2(grid.left(), grid.bottom()));
        let song_len = p.song_length() as f32;
        let overview = Overview { rect: overview_r, area: grid, content: song_len };

        let (pos, pressed_p, pressed_s, pressed_m, double, down, mods, wheel, zoom) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.primary_pressed(),
                i.pointer.secondary_pressed(),
                i.pointer.button_pressed(egui::PointerButton::Middle),
                i.pointer.button_double_clicked(egui::PointerButton::Primary),
                i.pointer.any_down(),
                i.modifiers,
                i.raw_scroll_delta,
                i.zoom_delta(),
            )
        });
        let hovering = ui.rect_contains_pointer(rect) && !crate::app::on_panel_edge(ui.ctx());

        // ── Wheel ──
        let over_bars = pos.is_some_and(|p| overview_r.contains(p) || vbar.rect.contains(p));
        if hovering && over_bars {
            // Wheel over a scrollbar zooms its axis, anchored at the top-left edge.
            if pos.is_some_and(|p| overview_r.contains(p)) {
                overview.wheel_zoom(&mut self.time, wheel.y, 0.01, 2.0);
            } else {
                let mut rv = self.rows(grid, p.tracks.len());
                vbar.wheel_zoom(&mut rv, wheel.y, grid.height() / 140.0, grid.height() / 24.0);
                self.set_rows(rv, grid);
            }
        } else if hovering {
            if zoom != 1.0 {
                self.time.zoom_at(grid, zoom, pos.map_or(grid.left(), |p| p.x), 0.01, 2.0);
            } else if wheel.y != 0.0 && mods.alt {
                self.track_h = (self.track_h * (wheel.y * 0.004).exp()).clamp(24.0, 140.0);
            } else {
                let sideways = if wheel.x != 0.0 {
                    wheel.x
                } else if mods.shift {
                    wheel.y
                } else {
                    0.0
                };
                if sideways != 0.0 {
                    self.time.scroll = (self.time.scroll - sideways / self.time.ppt).max(0.0);
                } else if wheel.y != 0.0 {
                    self.scroll_y -= wheel.y / self.track_h * 0.5;
                }
            }
        }
        let rows = grid.height() / self.track_h;
        self.scroll_y = self.scroll_y.clamp(0.0, (p.tracks.len() as f32 - rows).max(0.0));
        let view = self.view(grid);

        // Track headers mute like every row of lights: press one, sweep over the others.
        for t in 0..p.tracks.len() {
            let y = view.y(t);
            let row = Rect::from_min_max(pos2(headers.left(), y), pos2(headers.right(), y + self.track_h));
            if !row.intersects(headers) || !hovering {
                continue;
            }
            match widgets::sweep(ui, row.intersect(headers), "track-mute", !p.tracks[t].mute) {
                Some(widgets::Sweep::Begin(on)) => {
                    h.checkpoint(p);
                    p.tracks[t].mute = !on;
                }
                Some(widgets::Sweep::Continue(on)) => p.tracks[t].mute = !on,
                None => {}
            }
        }

        // ── Mouse ──
        if hovering && pressed_m {
            if let Some(pos) = pos {
                self.drag = Drag::Hand { last: pos };
            }
        } else if hovering && (pressed_p || pressed_s) {
            if let Some(pos) = pos {
                if overview_r.contains(pos) && pressed_p {
                    self.drag = Drag::Overview(overview.press(&mut self.time, pos));
                } else if vbar.rect.contains(pos) && pressed_p {
                    let mut rv = self.rows(grid, p.tracks.len());
                    self.drag = Drag::VBar(vbar.press(&mut rv, pos));
                    self.set_rows(rv, grid);
                } else if ruler.contains(pos) && pressed_p {
                    out.push(Action::SongMode(true));
                    out.push(Action::Seek(view.tick(pos.x).clamp(0.0, song_len - 1.0) as f64));
                    self.drag = Drag::Seek;
                } else if headers.contains(pos) {
                    // Track mutes are handled by the sweep below.
                } else if grid.contains(pos) {
                    self.press_grid(pos, pressed_p, double, mods, &view, p, h, st);
                }
            }
        }
        if down {
            if let Some(pos) = pos {
                self.drag_to(pos, mods, &view, (&overview, &vbar), p, song_len, out);
            }
        } else {
            self.drag = Drag::None;
        }

        // Cursor feedback
        if let (true, Some(pos), Drag::None) = (hovering, pos, &self.drag) {
            if grid.contains(pos) {
                if edge_hit(&view, &p.clips, pos).is_some() {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                } else if hit(&view, &p.clips, pos).is_some() {
                    ui.ctx().set_cursor_icon(CursorIcon::Grab);
                }
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
        let view = self.view(grid);
        let painter = ui.painter_at(rect);
        self.paint_grid(&painter, &view, p, song_len);
        self.paint_clips(&painter, &view, p, st, playing.then_some(playhead));
        if let (Drag::Select { from, .. }, Some(pos)) = (&self.drag, pos) {
            let r = Rect::from_two_pos(*from, pos);
            painter.rect_filled(r, cr(0), theme::MINT.gamma_multiply(0.07));
            painter.rect_stroke(r, cr(0), stroke(1.0, theme::MINT.gamma_multiply(0.7)), StrokeKind::Inside);
        }
        self.paint_headers(&painter, &view, headers, p);
        overview.paint(&painter, &self.time, matches!(self.drag, Drag::Overview(_)));
        vbar.paint(&painter, &self.rows(grid, p.tracks.len()), matches!(self.drag, Drag::VBar(_)));
        for c in &p.clips {
            let x0 = overview.x_of(&self.time, c.start as f32);
            let x1 = overview.x_of(&self.time, c.end() as f32);
            let y =
                overview_r.top() + 3.0 + (c.track as f32 / p.tracks.len().max(1) as f32) * (overview_r.height() - 6.0);
            painter.rect_filled(
                Rect::from_min_max(pos2(x0, y), pos2(x1.max(x0 + 1.0), y + 2.0)),
                cr(0),
                clip_light(p, c).mid.gamma_multiply(0.8),
            );
        }
        timeline::paint_ruler(&painter, &self.time, ruler, song_len, Some((playhead as f32, playing)), grid);
    }

    fn toolbar(&mut self, ui: &mut Ui, p: &Project, st: &UiState) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("snap").small().color(theme::TEXT_DIM));
            let name = SNAPS.iter().find(|s| s.1 == self.snap).map_or("?", |s| s.0);
            egui::ComboBox::from_id_salt("pl-snap").width(64.0).selected_text(name).show_ui(ui, |ui| {
                for (n, t) in SNAPS {
                    ui.selectable_value(&mut self.snap, t, n);
                }
            });
            ui.add_space(10.0);
            if let Some(pat) = p.patterns.get(st.pattern) {
                let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, theme::channel_color(st.pattern));
                ui.label(egui::RichText::new(format!("click places: {}", pat.name)).color(theme::TEXT_DIM))
                    .on_hover_text("Pick another pattern in the top bar (or click a clip) to place that one.");
            }
            ui.add_space(10.0);
            let bars = p.song_length() / TICKS_PER_BAR;
            ui.label(egui::RichText::new(format!("song: {bars} bars")).small().color(theme::TEXT_DIM));
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn press_grid(
        &mut self,
        pos: Pos2,
        primary: bool,
        double: bool,
        mods: egui::Modifiers,
        v: &View,
        p: &mut Project,
        h: &mut History,
        st: &mut UiState,
    ) {
        let free = mods.alt;
        let snap = if free { 1 } else { self.snap };
        let anchor_tick = v.tick(pos.x);
        let anchor_track = v.track(pos.y);
        if !primary {
            h.checkpoint(p);
            self.selection.clear();
            if let Some(i) = hit(v, &p.clips, pos) {
                p.clips.remove(i);
            }
            self.drag = Drag::Erase;
            return;
        }
        if mods.command {
            let base = if mods.shift { self.selection.clone() } else { Vec::new() };
            self.drag = Drag::Select { from: pos, base };
            return;
        }
        let edge = edge_hit(v, &p.clips, pos);
        if let Some(i) = edge.or_else(|| hit(v, &p.clips, pos)) {
            // Clicking a clip makes its pattern the current one (FL-style).
            if let Some(idx) = p.pattern_index(p.clips[i].pattern) {
                st.pattern = idx;
            }
            if double {
                st.show_rack = true;
                st.raise = Some(Focus::Rack);
                self.drag = Drag::None;
                return;
            }
            h.checkpoint(p);
            if !self.selection.contains(&i) {
                self.selection = vec![i];
            }
            let mut lead_i = i;
            if mods.shift && edge.is_none() {
                let base = p.clips.len();
                let at = self.selection.iter().position(|&j| j == i).unwrap_or(0);
                for &j in &self.selection.clone() {
                    let c = p.clips[j].clone();
                    p.clips.push(c);
                }
                self.selection = (base..p.clips.len()).collect();
                lead_i = self.selection[at];
            }
            let originals: Vec<(usize, Clip)> = self.selection.iter().map(|&j| (j, p.clips[j].clone())).collect();
            let lead = originals.iter().position(|(j, _)| *j == lead_i).unwrap_or(0);
            self.drag = if edge.is_some() {
                Drag::Resize { anchor_tick, originals, lead, free }
            } else {
                Drag::Move { anchor_tick, anchor_track, originals, lead, free }
            };
        } else if let (Some(pat), Ok(track)) = (p.patterns.get(st.pattern), usize::try_from(anchor_track)) {
            if track >= p.tracks.len() {
                return;
            }
            // Place the current pattern here, full length, and keep dragging it.
            h.checkpoint(p);
            let start = (anchor_tick.max(0.0) as u32 / snap) * snap;
            let len = pat.play_length() * TICKS_PER_STEP;
            p.clips.push(Clip { pattern: pat.id, track, start, len });
            let i = p.clips.len() - 1;
            self.selection = vec![i];
            self.drag =
                Drag::Move { anchor_tick, anchor_track, originals: vec![(i, p.clips[i].clone())], lead: 0, free };
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn drag_to(
        &mut self,
        pos: Pos2,
        mods: egui::Modifiers,
        v: &View,
        (overview, vbar): (&Overview, &VBar),
        p: &mut Project,
        song_len: f32,
        out: &mut Vec<Action>,
    ) {
        let mut drag = std::mem::replace(&mut self.drag, Drag::None);
        match &mut drag {
            Drag::Move { anchor_tick, anchor_track, originals, lead, free } => {
                *free |= mods.alt; // once free, free until release
                let snap = if *free { 1 } else { self.snap };
                let lead_clip = &originals[*lead].1;
                let raw = lead_clip.start as f32 + (v.tick(pos.x) - *anchor_tick);
                let min_start = originals.iter().map(|(_, c)| c.start).min().unwrap_or(0) as i64;
                let dt = (snapped(raw, snap).max(0.0) as i64 - lead_clip.start as i64).max(-min_start);
                let (lo, hi) =
                    originals.iter().fold((usize::MAX, 0), |(lo, hi), (_, c)| (lo.min(c.track), hi.max(c.track)));
                let max_down = p.tracks.len().saturating_sub(1).saturating_sub(hi) as i32;
                let dtrack = (v.track(pos.y) - *anchor_track).clamp(-(lo as i32), max_down);
                for (i, c) in originals.iter() {
                    if let Some(clip) = p.clips.get_mut(*i) {
                        clip.start = (c.start as i64 + dt) as u32;
                        clip.track = (c.track as i32 + dtrack) as usize;
                    }
                }
            }
            Drag::Resize { anchor_tick, originals, lead, free } => {
                *free |= mods.alt;
                let snap = if *free { 1 } else { self.snap };
                let lead_clip = &originals[*lead].1;
                let raw = lead_clip.len as f32 + (v.tick(pos.x) - *anchor_tick);
                let new_len = snapped(raw, snap).max(snap.max(TICKS_PER_STEP) as f32) as i64;
                let dl = new_len - lead_clip.len as i64;
                for (i, c) in originals.iter() {
                    if let Some(clip) = p.clips.get_mut(*i) {
                        clip.len = (c.len as i64 + dl).max(TICKS_PER_STEP as i64) as u32;
                    }
                }
            }
            Drag::Select { from, base } => {
                let r = Rect::from_two_pos(*from, pos);
                let mut sel = base.clone();
                sel.extend(
                    (0..p.clips.len()).filter(|&i| v.clip_rect(&p.clips[i]).intersects(r) && !base.contains(&i)),
                );
                self.selection = sel;
            }
            Drag::Erase => {
                if let Some(i) = hit(v, &p.clips, pos) {
                    p.clips.remove(i);
                }
            }
            Drag::Seek => out.push(Action::Seek(v.tick(pos.x).clamp(0.0, song_len - 1.0) as f64)),
            Drag::Hand { last } => {
                let d = pos - *last;
                self.time.scroll = (self.time.scroll - d.x / self.time.ppt).max(0.0);
                self.scroll_y = (self.scroll_y - d.y / self.track_h).max(0.0);
                *last = pos;
            }
            Drag::Overview(od) => overview.drag(&mut self.time, *od, pos, TICKS_PER_BAR as f32, 2.0),
            Drag::VBar(vd) => {
                let mut rv = self.rows(v.grid, p.tracks.len());
                vbar.drag(&mut rv, *vd, pos, v.grid.height() / 140.0, v.grid.height() / 24.0);
                self.set_rows(rv, v.grid);
            }
            Drag::None => {}
        }
        self.drag = drag;
    }

    fn paint_grid(&self, painter: &egui::Painter, v: &View, p: &Project, song_len: f32) {
        let g = v.grid;
        painter.rect_filled(g, cr(0), theme::ROLL_BLACK_ROW);
        for t in 0..p.tracks.len() {
            let y = v.y(t);
            if y > g.bottom() || y + v.track_h < g.top() {
                continue;
            }
            let row = Rect::from_min_max(pos2(g.left(), y), pos2(g.right(), y + v.track_h));
            if t % 2 == 0 {
                painter.rect_filled(row, cr(0), theme::ROLL_WHITE_ROW);
            }
            if p.tracks[t].mute {
                painter.rect_filled(row, cr(0), Color32::from_black_alpha(60));
            }
            painter.hline(g.x_range(), y + v.track_h, stroke(1.0, theme::LINE.gamma_multiply(0.6)));
        }
        // Beats faint (when there's room), bars stronger, every 4 bars strongest — all dark.
        let first = (v.time.scroll / BEAT as f32).floor().max(0.0) as u32;
        let last = (v.tick(g.right()) / BEAT as f32).ceil() as u32;
        let beat_px = BEAT as f32 * v.time.ppt;
        for b in first..=last {
            let t = b * BEAT;
            let (alpha, show) = if t.is_multiple_of(4 * TICKS_PER_BAR) {
                (1.0, true)
            } else if t.is_multiple_of(TICKS_PER_BAR) {
                (0.55, beat_px * 4.0 >= 10.0)
            } else {
                (0.22, beat_px >= 10.0)
            };
            if show {
                painter.vline(v.x(t as f32), g.y_range(), stroke(1.0, theme::LINE.gamma_multiply(alpha)));
            }
        }
        let end = v.x(song_len);
        if end < g.right() {
            painter.rect_filled(
                Rect::from_min_max(pos2(end.max(g.left()), g.top()), g.right_bottom()),
                cr(0),
                Color32::from_black_alpha(70),
            );
        }
    }

    /// Clips are flat keys: a dark recess outline, a body tinted with the
    /// pattern's light, and a flat title strip in that light with dark ink.
    fn paint_clips(&self, painter: &egui::Painter, v: &View, p: &Project, st: &UiState, playhead: Option<f64>) {
        let m = theme::m();
        for (i, c) in p.clips.iter().enumerate() {
            let r = v.clip_rect(c);
            if !r.intersects(v.grid) {
                continue;
            }
            let Some(pat) = p.pattern(c.pattern) else { continue };
            let light = clip_light(p, c);
            let current = p.pattern_index(c.pattern) == Some(st.pattern);
            let playing = playhead.is_some_and(|t| (c.start as f64..c.end() as f64).contains(&t));
            let selected = self.selection.contains(&i);
            painter.rect_filled(r, cr(4), theme::KEY_RECESS);
            let inner = r.shrink(1.0);
            let mut body = theme::mix(theme::PANEL, light.low, if current { 0.34 } else { 0.24 });
            if playing {
                body = theme::shade(body, 1.12);
            }
            painter.rect_filled(inner, cr(3), body);
            // Title strip.
            let strip_h = (m.text_small + 6.0).min(inner.height() * 0.45);
            let strip = Rect::from_min_max(inner.min, pos2(inner.right(), inner.top() + strip_h));
            let strip_fill = if playing { theme::shade(light.mid, 1.04) } else { light.mid };
            painter.rect_filled(strip, egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, strip_fill);
            if r.width() > 24.0 {
                painter.with_clip_rect(strip.intersect(v.grid)).text(
                    strip.left_center() + vec2(5.0, 0.0),
                    Align2::LEFT_CENTER,
                    &pat.name,
                    FontId::proportional(m.text_small),
                    theme::key_ink(true),
                );
            }
            // Note silhouettes of every channel, repeated across the clip.
            let body_r = Rect::from_min_max(
                pos2(inner.left() + 2.0, strip.bottom() + 2.0),
                inner.right_bottom() - vec2(2.0, 2.0),
            );
            let loop_len = (pat.play_length() * TICKS_PER_STEP).max(1);
            let (lo, hi) =
                pat.notes.values().flatten().fold((127u8, 0u8), |(lo, hi), n| (lo.min(n.key), hi.max(n.key)));
            let span = (hi.saturating_sub(lo) as f32).max(12.0);
            let clipped = painter.with_clip_rect(body_r.intersect(v.grid));
            for (ch_i, ch) in p.channels.iter().enumerate() {
                let ink = theme::channel_light(ch.color).mid;
                for rep in (0..c.len).step_by(loop_len as usize) {
                    if rep > 0 && ch_i == 0 {
                        // Where the pattern repeats.
                        let x = v.x((c.start + rep) as f32);
                        clipped.vline(x, body_r.y_range(), stroke(1.0, Color32::from_black_alpha(90)));
                    }
                    for n in pat.notes(ch.id).iter().filter(|n| rep + n.start < c.len) {
                        let x0 = v.x((c.start + rep + n.start) as f32);
                        let x1 = v.x((c.start + rep + (n.start + n.len).min(c.len - rep)) as f32);
                        let y = body_r.bottom() - (n.key - lo) as f32 / span * (body_r.height() - 2.0);
                        clipped.rect_filled(
                            Rect::from_min_max(pos2(x0, y - 1.5), pos2(x1.max(x0 + 1.5), y)),
                            cr(0),
                            ink,
                        );
                    }
                }
            }
            if selected {
                painter.rect_stroke(r, cr(4), stroke(1.5, theme::TEXT), StrokeKind::Inside);
            }
        }
    }

    /// Track headers: plates in a rack, a seam apart, each with its round light.
    fn paint_headers(&self, painter: &egui::Painter, v: &View, r: Rect, p: &Project) {
        let m = theme::m();
        painter.rect_filled(r, cr(0), theme::BG);
        let clipped = painter.with_clip_rect(r);
        for (t, track) in p.tracks.iter().enumerate() {
            let y = v.y(t);
            if y > r.bottom() || y + v.track_h < r.top() {
                continue;
            }
            let plate =
                Rect::from_min_max(pos2(r.left(), y), pos2(r.right() - theme::SEAM, y + v.track_h - theme::SEAM));
            clipped.rect_filled(plate, cr(0), theme::PANEL);
            let lit = !track.mute;
            // The track's light takes the colour of what plays on it first.
            let light = p.clips.iter().find(|c| c.track == t).map_or(theme::L_MINT, |c| clip_light(p, c));
            let led = plate.left_center() + vec2(m.led * 0.5 + 8.0, 0.0);
            theme::led(&clipped, led, m.led, lit.then_some(light), false);
            clipped.text(
                led + vec2(m.led * 0.5 + 8.0, 0.0),
                Align2::LEFT_CENTER,
                &track.name,
                FontId::proportional(m.text),
                if lit { theme::TEXT } else { theme::TEXT_DIM },
            );
        }
    }
}

/// Clips are lit by pattern, so the same pattern looks the same everywhere.
pub fn clip_light(p: &Project, c: &Clip) -> theme::Light {
    theme::channel_light(p.pattern_index(c.pattern).unwrap_or(0))
}
