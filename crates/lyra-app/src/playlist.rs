//! The playlist: arrange patterns into a song.
//!
//! Mouse grammar (the same as the piano roll, so hands only learn it once):
//! * **click empty** — place a copy of the clip last clicked (or the top
//!   bar's pattern); keep holding to drag it into place
//! * **drag a clip** — move (also to another track) · **drag either end** —
//!   resize; the left end trims the start (a clip is a window onto its
//!   pattern or audio; longer than the pattern = it repeats) · **Shift+drag** — duplicate
//! * **click a clip** — makes its pattern the current one · **double-click** — open it in the piano roll
//!   (an audio clip: in the audio-clip editor)
//! * **audio clips** — drop a sample from the browser onto a track · drag
//!   either end to trim it (a clip is a window onto its audio). Hover a clip
//!   for its handles: the dots on the title strip are its fades (drag
//!   sideways), the ring on a fade's curve bends it (drag up and down), the ↕
//!   in the middle is its gain (drag up and down). A double-click anywhere on
//!   a clip opens it in the audio-clip editor.
//!   These are each clip's own. Double-click: the audio-clip editor, where
//!   its window onto the audio slides, and what all copies share is edited.
//! * a click never moves anything: moves and resizes start once the mouse
//!   has really moved (so an off-grid clip isn't pulled onto the grid)
//! * **right-drag** — delete · **Ctrl+drag** — box select (Shift adds) · **Alt** — no snap
//! * **wheel** — scroll tracks · **Shift+wheel** — zoom time · **Ctrl+wheel** — sideways ·
//!   **Alt+wheel** — track height · **middle-drag** — pan
//! * overview bar — drag the thumb to pan, its ends to zoom · ruler — click to
//!   play the song from there · track lights — mute a track
//!
//! Tools (the keys in the header): **Edit** (all of the above) · **Cut** —
//! click a clip to split it there (the selected clips too, if it's one of
//! them) · **Slide** — drag a clip to move what plays inside it (a pattern
//! wraps around; audio stops at its ends); the clip stays put.
//!
//! Keys (when this window was clicked last): Delete, Ctrl+A, Ctrl+D/B (duplicate
//! after), Ctrl+U (the selected audio clips get their own copy, to change apart), ←→ move by snap (Ctrl = 4 bars), ↑↓ move to the track above/below, Esc.

use crate::app::UiState;
use crate::widgets::Hint;
use crate::browser::SampleDrag;
use crate::keys::Action;
use crate::pool::{AudioDrag, AutomationDrag, PatternDrag};
use crate::samples::{self, Samples};
use crate::theme::{self, cr, stroke};
use crate::timeline::{self, Input, Overview, OverviewDrag, RowView, TimeView, VBar, VDrag, Wheel};
use crate::waveform::{self, End};
use crate::widgets;
use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, StrokeKind, Ui};
use lyra_model::{Audio, Clip, History, Project, Source, TICKS_PER_BAR, TICKS_PER_STEP};

const HEADER_W: f32 = 150.0;
const BEAT: u32 = TICKS_PER_STEP * 4;

/// What a left press on a clip does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Edit,
    Cut,
    Slide,
}

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
    /// An audio clip's left end: trims its start (start and offset move together).
    ResizeLeft {
        anchor_tick: f32,
        originals: Vec<(usize, Clip)>,
        lead: usize,
        free: bool,
    },
    /// A clip's fade length.
    Fade {
        clip: usize,
        end: End,
        orig: f32,
        anchor_x: f32,
        seconds_per_px: f32,
        max: f32,
    },
    /// A clip's fade curve.
    Tension {
        clip: usize,
        end: End,
        orig: f32,
        anchor_y: f32,
    },
    /// A clip's gain.
    Gain {
        clip: usize,
        orig: f32,
        anchor_y: f32,
    },
    /// The Slide tool: what plays inside the clips moves, the clips don't.
    Slide {
        anchor_tick: f32,
        originals: Vec<(usize, Clip)>,
        free: bool,
    },
    Select {
        from: Pos2,
        base: Vec<usize>,
    },
    Erase,
    Seek,
    /// Alt+drag on the ruler: drawing a loop region from `anchor` (ticks).
    /// `moved` false at release = it was an Alt+click, which clears the loop.
    Loop {
        anchor: f32,
        moved: bool,
    },
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
    /// The loop region (song ticks), if one is drawn; see `timeline::loop_span`.
    pub looped: Option<(u32, u32)>,
    /// A MIDI file dropped on the arrangement (file, tick, track): the app
    /// reads it in there (and reports trouble).
    pub dropped_midi: Option<(std::path::PathBuf, u32, usize)>,
    drag: Drag,
    clicks: timeline::Clicks,
    pub tool: Tool,
}

impl Default for Playlist {
    fn default() -> Self {
        Self {
            time: TimeView { scroll: 0.0, ppt: 64.0 / TICKS_PER_BAR as f32 },
            scroll_y: 0.0,
            track_h: 52.0,
            snap: TICKS_PER_BAR,
            selection: Vec::new(),
            looped: None,
            dropped_midi: None,
            drag: Drag::None,
            clicks: timeline::Clicks::default(),
            tool: Tool::default(),
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

/// A clip's parts on screen: the inner face, the title strip, and the body below it.
fn parts(r: Rect) -> (Rect, Rect, Rect) {
    let m = theme::m();
    let inner = r.shrink(1.0);
    let strip_h = (m.text_small + 6.0).min(inner.height() * 0.45);
    let strip = Rect::from_min_max(inner.min, pos2(inner.right(), inner.top() + strip_h));
    let body = Rect::from_min_max(pos2(inner.left(), strip.bottom()), inner.right_bottom() - vec2(0.0, 1.0));
    (inner, strip, body)
}

/// Ticks of audio an audio clip has left from its offset on (None for pattern clips).
pub fn audio_room(p: &Project, c: &Clip) -> Option<u32> {
    let a = p.audio(c.audio()?)?;
    let ticks = (a.length() / p.seconds_per_tick()).ceil() as u32;
    Some(ticks.saturating_sub(c.offset).max(1))
}

/// Where an audio clip's fades are on screen.
struct FadeGeo {
    body: Rect,
    /// The title strip's bottom edge, where the fade handles sit.
    handles_y: f32,
    /// Clip start, end of the fade-in, start of the fade-out, end of the sound.
    x0: f32,
    x_in: f32,
    x_out: f32,
    x_end: f32,
    /// The arrangement's visible left and right edges.
    view: (f32, f32),
}

fn fade_geo(v: &View, p: &Project, c: &Clip) -> FadeGeo {
    let (_, strip, body) = parts(v.clip_rect(c));
    let spt = p.seconds_per_tick() as f32;
    let room = audio_room(p, c).unwrap_or(c.len);
    let x0 = body.left();
    let x_end = v.x((c.start + c.len.min(room)) as f32).min(body.right());
    let px = |secs: f32| secs / spt * v.time.ppt;
    let x_in = (x0 + px(c.fade_in.len)).min(x_end);
    let x_out = (x_end - px(c.fade_out.len)).max(x0);
    FadeGeo { body, handles_y: strip.bottom(), x0, x_in, x_out, x_end, view: (v.grid.left(), v.grid.right()) }
}

/// The part of an audio clip's handles a press lands on.
#[derive(Clone, Copy, PartialEq)]
enum Grip {
    /// A fade's square (its length).
    Fade(End),
    /// The dot on a fade's curve (its tension).
    Curve(End),
    /// The gain pill.
    Gain,
}

#[derive(Clone, Copy, PartialEq)]
struct Handle {
    clip: usize,
    grip: Grip,
}

const HANDLE_R: f32 = 6.0;
/// Fades narrower than this show no curve ring (too fiddly).
const CURVE_MIN_W: f32 = 16.0;

/// Clips smaller than this on screen show no handles at all (just the
/// waveform), so a short or narrow clip is never all handles: it can always
/// be grabbed and moved.
fn shows_handles(r: Rect) -> bool {
    r.height() >= 40.0 && r.width() >= 64.0
}

fn curve_mid(g: &FadeGeo, end: End, tension: f32) -> Option<Pos2> {
    let (f0, f1) = if end == End::In { (g.x0, g.x_in) } else { (g.x_out, g.x_end) };
    (f1 - f0 >= CURVE_MIN_W)
        .then(|| pos2((f0 + f1) * 0.5, g.body.bottom() - lyra_engine::fade_curve(0.5, tension) * g.body.height()))
}

/// The gain grip sits in the middle of the sound's visible part (so a clip
/// wider than the view still has it in reach).
fn gain_pos(g: &FadeGeo) -> Pos2 {
    let (l, r) = (g.x0.max(g.view.0), g.x_end.min(g.view.1));
    pos2((l + r) * 0.5, g.body.center().y)
}

/// The handle under `pos`, on the clip it hovers (only that clip shows them).
fn handle_hit(v: &View, p: &Project, pos: Pos2) -> Option<Handle> {
    let i = hit(v, &p.clips, pos)?;
    let c = &p.clips[i];
    c.audio()?;
    if !shows_handles(v.clip_rect(c)) {
        return None;
    }
    let g = fade_geo(v, p, c);
    let near = |at: Pos2| pos.distance(at) <= HANDLE_R;
    let grip = if near(pos2(g.x_in, g.handles_y)) {
        Grip::Fade(End::In)
    } else if near(pos2(g.x_out, g.handles_y)) {
        Grip::Fade(End::Out)
    } else if near(gain_pos(&g)) {
        Grip::Gain
    } else if curve_mid(&g, End::In, c.fade_in.tension).is_some_and(near) {
        Grip::Curve(End::In)
    } else if curve_mid(&g, End::Out, c.fade_out.tension).is_some_and(near) {
        Grip::Curve(End::Out)
    } else {
        return None;
    };
    Some(Handle { clip: i, grip })
}

fn left_edge_hit(v: &View, clips: &[Clip], pos: Pos2) -> Option<usize> {
    let m = theme::m();
    (0..clips.len()).rev().find(|&i| {
        let r = v.clip_rect(&clips[i]);
        let grab = m.edge_grab.min(r.width() * 0.3);
        r.y_range().contains(pos.y) && pos.x >= r.left() - m.edge_reach && pos.x <= r.left() + grab
    })
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

    /// What the context bar (and F3) offers here right now.
    pub fn context(&self) -> Vec<(Action, String)> {
        let Some((a, b)) = self.looped else { return Vec::new() };
        vec![
            (Action::ExportBarsMidi, format!("Export {} as MIDI", crate::midi::bars_label(a, b))),
            (Action::ClearLoop, "Clear loop".to_string()),
        ]
    }

    /// The MIDI an export action asks for: the song's notes inside the loop region.
    pub fn midi_export(&self, a: Action, p: &Project, title: &str) -> Option<crate::midi::Export> {
        let (Action::ExportBarsMidi, Some((from, to))) = (a, self.looped) else { return None };
        let song = crate::bridge::compile_song(p);
        let parts = p.channels.iter().enumerate().map(|(i, c)| {
            let notes: Vec<lyra_model::Note> = song
                .notes
                .iter()
                .filter(|n| n.channel as usize == i)
                .map(|n| lyra_model::Note { start: n.start, len: n.len, key: n.key, velocity: n.velocity, link: None })
                .collect();
            (c.name.clone(), crate::midi::section(&notes, from, to))
        });
        crate::midi::Export::new(format!("{title} {}", crate::midi::bars_label(from, to)), to - from, parts)
    }

    /// Keyboard commands routed here by the app. Returns true if handled.
    pub fn action(&mut self, a: Action, p: &mut Project, h: &mut History) -> bool {
        self.selection.retain(|&i| i < p.clips.len());
        match a {
            Action::SelectAll => self.selection = (0..p.clips.len()).collect(),
            Action::Deselect => self.selection.clear(),
            Action::Delete | Action::Backspace if !self.selection.is_empty() => {
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
                    p.add_clip(c);
                }
                self.selection = (base..p.clips.len()).collect();
            }
            Action::MakeUnique if !self.selection.is_empty() => {
                // One new audio clip per audio clip the selection plays, shared by
                // the selected copies; only when copies outside the selection
                // play it too (otherwise it's unique already).
                let mut sources: Vec<lyra_model::AudioId> =
                    self.selection.iter().filter_map(|&i| p.clips[i].audio()).collect();
                sources.sort_unstable();
                sources.dedup();
                let shared = |p: &Project, id| {
                    (0..p.clips.len()).any(|i| p.clips[i].audio() == Some(id) && !self.selection.contains(&i))
                };
                let todo: Vec<_> = sources.into_iter().filter(|&id| shared(p, id)).collect();
                if todo.is_empty() {
                    return true;
                }
                h.checkpoint(p);
                for id in todo {
                    let Some(index) = p.audio_index(id) else { continue };
                    let copy = p.clone_audio(index);
                    let new_id = p.audio[copy].id;
                    for &i in &self.selection {
                        if p.clips[i].audio() == Some(id) {
                            p.clips[i].source = Source::Audio(new_id);
                        }
                    }
                }
            }
            Action::Arrow { dx, dy, big, .. } if !self.selection.is_empty() => {
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
        samples: &mut Samples,
        out: &mut Vec<Action>,
    ) {
        let m = theme::m();
        self.selection.retain(|&i| i < p.clips.len());

        let size = ui.available_size().max(vec2(420.0, 200.0));
        // Hover only: the editor reads the mouse itself, and a sensing canvas would
        // win egui's hit test over the docked panels' resize edges.
        let (rect, _resp) = ui.allocate_exact_size(size, Sense::hover());
        // The vertical bar runs down the right of the grid, thinner than the overview.
        let vbar_w = VBar::width();
        let right = rect.right() - vbar_w - 2.0;
        let overview_r =
            Rect::from_min_max(pos2(rect.left() + HEADER_W, rect.top()), pos2(right, rect.top() + m.overview_h));
        let ruler = Rect::from_min_max(
            pos2(overview_r.left(), overview_r.bottom() + 2.0),
            pos2(right, overview_r.bottom() + 2.0 + m.ruler_h),
        );
        let grid = Rect::from_min_max(pos2(ruler.left(), ruler.bottom()), pos2(right, rect.bottom()));
        let vbar_lock = match self.drag {
            Drag::VBar(d) => Some(d.span()),
            _ => None,
        };
        let vbar = VBar {
            rect: Rect::from_min_max(pos2(right + 2.0, grid.top()), pos2(rect.right(), grid.bottom())),
            lock: vbar_lock,
        };
        let headers = Rect::from_min_max(pos2(rect.left(), grid.top()), pos2(grid.left(), grid.bottom()));
        let song_len = p.song_length() as f32;
        let overview_lock = match self.drag {
            Drag::Overview(d) => Some(d.span()),
            _ => None,
        };
        let overview = Overview { rect: overview_r, area: grid, lock: overview_lock, content: song_len };

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
        let double = self.clicks.double(&input);

        // ── Wheel ──
        let over_bars = pos.is_some_and(|p| overview_r.contains(p) || vbar.rect.contains(p));
        if hovering && over_bars {
            // Wheel over a scrollbar zooms its axis, anchored at the top-left edge.
            if pos.is_some_and(|p| overview_r.contains(p)) {
                overview.wheel_zoom(&mut self.time, input.wheel.y, 0.01, 2.0);
            } else {
                let mut rv = self.rows(grid, p.tracks.len());
                vbar.wheel_zoom(&mut rv, input.wheel.y, grid.height() / 140.0, grid.height() / 24.0);
                self.set_rows(rv, grid);
            }
        } else if hovering {
            match timeline::wheel(&input, ruler) {
                Wheel::ZoomTime { factor, at } => self.time.zoom_at(grid, factor, at, 0.01, 2.0),
                Wheel::ZoomRows { factor, at } => {
                    // Track height, keeping the track under the mouse in place.
                    let y = at - grid.top();
                    let anchor = self.scroll_y + y / self.track_h;
                    self.track_h = (self.track_h * factor).clamp(24.0, 140.0);
                    self.scroll_y = anchor - y / self.track_h;
                }
                Wheel::Scroll { x, y } => {
                    self.time.scroll = (self.time.scroll + x / self.time.ppt).max(0.0);
                    self.scroll_y += y / self.track_h;
                }
                Wheel::None => {}
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

        // ── Drops from the browser: a sample becomes a new audio clip, an audio
        //    clip a new copy of it. ──
        let drop_at = pos.filter(|p| hovering && grid.contains(*p));
        let dropping = egui::DragAndDrop::has_payload_of_type::<SampleDrag>(ui.ctx())
            || egui::DragAndDrop::has_payload_of_type::<AudioDrag>(ui.ctx())
            || egui::DragAndDrop::has_payload_of_type::<AutomationDrag>(ui.ctx())
            || egui::DragAndDrop::has_payload_of_type::<PatternDrag>(ui.ctx());
        let mut ghost = None;
        // A MIDI file: every track of it, from here down.
        if let Some(at) = drop_at.filter(|_| egui::DragAndDrop::has_payload_of_type::<crate::midi::MidiDrag>(ui.ctx()))
        {
            let snap = if mods.alt { 1 } else { self.snap };
            let start = (view.tick(at.x).max(0.0) as u32 / snap) * snap;
            let track = view.track(at.y).max(0) as usize;
            ghost = Some(Clip::of_pattern(0, track.min(p.tracks.len().saturating_sub(1)), start, TICKS_PER_BAR));
            if ui.input(|i| i.pointer.any_released()) {
                if let Some(d) = egui::DragAndDrop::take_payload::<crate::midi::MidiDrag>(ui.ctx()) {
                    self.dropped_midi = Some((d.0.clone(), start, track));
                }
            }
        }
        if let (Some(at), true) = (drop_at, dropping) {
            let snap = if mods.alt { 1 } else { self.snap };
            let start = (view.tick(at.x).max(0.0) as u32 / snap) * snap;
            let track = view.track(at.y);
            let spt = p.seconds_per_tick();
            let automation = egui::DragAndDrop::payload::<AutomationDrag>(ui.ctx()).and_then(|d| p.automation(d.0));
            let secs = if let Some(d) = egui::DragAndDrop::payload::<SampleDrag>(ui.ctx()) {
                samples.get(&d.0).map(|s| s.seconds() as f64).unwrap_or(0.0)
            } else {
                egui::DragAndDrop::payload::<AudioDrag>(ui.ctx()).and_then(|d| p.audio(d.0)).map_or(0.0, |a| a.length())
            };
            // An automation is one loop of its curve long; a pattern, its length.
            let pattern = egui::DragAndDrop::payload::<PatternDrag>(ui.ctx()).and_then(|d| p.pattern(d.0));
            let len = match (automation, pattern) {
                (Some(a), _) => a.length.max(1),
                (None, Some(pat)) => pat.play_length() * TICKS_PER_STEP,
                (None, None) => ((secs / spt).ceil() as u32).max(1),
            };
            if let Ok(track) = usize::try_from(track).map(|t| t.min(p.tracks.len().saturating_sub(1))) {
                ghost = Some(Clip::of_audio(0, track, start, len));
                if ui.input(|i| i.pointer.any_released()) {
                    let id = if let Some(d) = egui::DragAndDrop::take_payload::<SampleDrag>(ui.ctx()) {
                        h.checkpoint(p);
                        let i = p.add_audio(&samples::display_name(&d.0), &d.0, secs);
                        Some(p.audio[i].id)
                    } else if let Some(d) = egui::DragAndDrop::take_payload::<AudioDrag>(ui.ctx()) {
                        h.checkpoint(p);
                        Some(d.0)
                    } else {
                        if let Some(d) = egui::DragAndDrop::take_payload::<PatternDrag>(ui.ctx()) {
                            h.checkpoint(p);
                            let i = p.add_clip(Clip::of_pattern(d.0, track, start, len));
                            self.selection = vec![i];
                            st.clip = Some(p.clips[i].id);
                            ghost = None;
                        }
                        if let Some(d) = egui::DragAndDrop::take_payload::<AutomationDrag>(ui.ctx()) {
                            h.checkpoint(p);
                            let i = p.add_clip(Clip::of_automation(d.0, track, start, len));
                            self.selection = vec![i];
                            st.automation = Some(d.0);
                            st.clip = Some(p.clips[i].id);
                            ghost = None;
                        }
                        None
                    };
                    if let Some(id) = id {
                        let i = p.add_clip(Clip::of_audio(id, track, start, len));
                        self.selection = vec![i];
                        st.audio = Some(id);
                        st.clip = Some(p.clips[i].id);
                        st.place_audio = true;
                        ghost = None;
                    }
                }
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
                } else if ruler.contains(pos) && pressed_p && mods.alt {
                    self.drag = Drag::Loop { anchor: view.tick(pos.x), moved: false };
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
                // Held at or past an edge while moving, resizing or selecting:
                // the view scrolls and the drag follows.
                let horizontal = matches!(
                    self.drag,
                    Drag::Move { .. } | Drag::Resize { .. } | Drag::ResizeLeft { .. } | Drag::Select { .. }
                );
                let vertical = matches!(self.drag, Drag::Move { .. } | Drag::Select { .. });
                let mut view = view;
                if horizontal {
                    let egui::Vec2 { x: dx, y: dy } = timeline::edge_pan_2d(pos, grid, input.dt, vertical);
                    if dx != 0.0 || dy != 0.0 {
                        let (scroll0, y0) = (self.time.scroll, self.scroll_y);
                        let rows = grid.height() / self.track_h;
                        self.time.scroll = (self.time.scroll + dx / self.time.ppt).max(0.0);
                        self.scroll_y =
                            (self.scroll_y + dy / self.track_h).clamp(0.0, (p.tracks.len() as f32 - rows).max(0.0));
                        // A rubber band keeps its anchor on the music, not on the screen.
                        if let Drag::Select { from, .. } = &mut self.drag {
                            from.x -= (self.time.scroll - scroll0) * self.time.ppt;
                            from.y -= (self.scroll_y - y0) * self.track_h;
                        }
                        view = self.view(grid);
                        ui.ctx().request_repaint();
                    }
                }
                self.drag_to(pos, mods, &view, (&overview, &vbar), p, song_len, out);
            }
        } else {
            if matches!(self.drag, Drag::Loop { moved: false, .. }) {
                self.looped = None; // Alt+click: no loop
            }
            self.drag = Drag::None;
        }

        // Cursor feedback
        let on_clip = pos.filter(|p| hovering && grid.contains(*p)).and_then(|pos| hit(&view, &p.clips, pos));
        if let (Tool::Cut | Tool::Slide, Some(_), Drag::None | Drag::Slide { .. }) = (self.tool, on_clip, &self.drag) {
            ui.ctx().set_cursor_icon(if self.tool == Tool::Cut {
                CursorIcon::Crosshair
            } else {
                CursorIcon::ResizeHorizontal
            });
        } else if let (true, Some(pos), Drag::None) = (hovering, pos, &self.drag) {
            if grid.contains(pos) {
                if let Some(hd) = handle_hit(&view, p, pos).filter(|_| self.tool == Tool::Edit) {
                    ui.ctx().set_cursor_icon(if matches!(hd.grip, Grip::Fade(_)) {
                        CursorIcon::ResizeHorizontal
                    } else {
                        CursorIcon::ResizeVertical
                    });
                } else if edge_hit(&view, &p.clips, pos).is_some() || left_edge_hit(&view, &p.clips, pos).is_some() {
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
        } else if matches!(self.drag, Drag::VBar(VDrag::Edge { .. }) | Drag::Tension { .. } | Drag::Gain { .. }) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
        } else if matches!(
            self.drag,
            Drag::Resize { .. }
                | Drag::ResizeLeft { .. }
                | Drag::Fade { .. }
                | Drag::Overview(OverviewDrag::Edge { .. })
        ) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }

        // ── Paint ──
        let view = self.view(grid);
        let painter = ui.painter_at(rect);
        self.paint_grid(&painter, &view, p, song_len);
        // Handles show on the clip under the mouse (or the one being changed).
        let hot = match self.drag {
            Drag::Fade { clip, end, .. } => Some((clip, Some(Grip::Fade(end)))),
            Drag::Tension { clip, end, .. } => Some((clip, Some(Grip::Curve(end)))),
            Drag::Gain { clip, .. } => Some((clip, Some(Grip::Gain))),
            Drag::None if self.tool == Tool::Edit => pos
                .filter(|p| hovering && grid.contains(*p))
                .and_then(|pos| hit(&view, &p.clips, pos).map(|i| (i, handle_hit(&view, p, pos).map(|hd| hd.grip)))),
            _ => None,
        };
        self.paint_clips(&painter, &view, p, st, playing.then_some(playhead), samples, hot);
        // The Cut tool shows where it would cut.
        if let (Tool::Cut, Some(i), Some(pos), Drag::None) = (self.tool, on_clip, pos, &self.drag) {
            let snap = if mods.alt { 1 } else { self.snap };
            let at = timeline::snap(view.tick(pos.x), snap);
            let c = &p.clips[i];
            if at > c.start as f32 && at < c.end() as f32 {
                let r = view.clip_rect(c);
                painter.vline(view.x(at), r.y_range(), stroke(1.5, theme::TEXT));
            }
        }
        if let Some(g) = &ghost {
            // The settled drop preview: what will be there, as a ghost clip (a
            // soft tint of the light and a title strip; no outline).
            let r = view.clip_rect(g);
            let light = theme::L_MINT;
            painter.rect_filled(r, cr(2), light.mid.gamma_multiply(0.22));
            let strip = Rect::from_min_max(r.min, pos2(r.right(), (r.top() + 14.0).min(r.bottom())));
            painter.rect_filled(strip, egui::CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, light.mid.gamma_multiply(0.45));
        }
        if let (Drag::Select { from, .. }, Some(pos)) = (&self.drag, pos) {
            timeline::paint_rubber_band(&painter, *from, pos);
        }
        self.paint_headers(&painter, &view, headers, p);
        overview.paint(&painter, &self.time, matches!(self.drag, Drag::Overview(_)), || {
            for c in &p.clips {
                let x0 = overview.x_of(&self.time, c.start as f32);
                let x1 = overview.x_of(&self.time, c.end() as f32);
                let y = overview_r.top()
                    + 3.0
                    + (c.track as f32 / p.tracks.len().max(1) as f32) * (overview_r.height() - 6.0);
                painter.rect_filled(
                    Rect::from_min_max(pos2(x0, y), pos2(x1.max(x0 + 1.0), y + 2.0)),
                    cr(0),
                    clip_light(p, c).mid.gamma_multiply(0.8),
                );
            }
        });
        vbar.paint(&painter, &self.rows(grid, p.tracks.len()), matches!(self.drag, Drag::VBar(_)));
        timeline::paint_ruler(&painter, &self.time, ruler, song_len, Some((playhead as f32, playing)), grid);
        if let Some(range) = self.looped {
            timeline::paint_loop(&painter, &self.time, ruler, grid, range);
        }
        if let Some(pos) = pos.filter(|p| hovering && (grid.contains(*p) || ruler.contains(*p))) {
            timeline::paint_cursor(&painter, pos.x, ruler, grid);
        }
    }

    /// The arrangement's controls; the area draws them on its header row.
    pub fn toolbar(&mut self, ui: &mut Ui, p: &Project, st: &UiState, out: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            let h = theme::m().control_h;
            let tools = [Tool::Edit, Tool::Cut, Tool::Slide];
            let options = [
                (widgets::Legend::Icon(widgets::Icon::Pointer), theme::L_MINT),
                (widgets::Legend::Icon(widgets::Icon::Cut), theme::L_MINT),
                (widgets::Legend::Icon(widgets::Icon::Slide), theme::L_MINT),
            ];
            let chosen = tools.iter().position(|&t| t == self.tool).unwrap_or(0);
            let hint =
                "Edit: place, move, resize · Cut: click a clip to split it · Slide: drag what plays inside a clip";
            if let Some(i) = widgets::choice(ui, vec2(h, h), &options, chosen, hint) {
                self.tool = tools[i];
            }
            ui.add_space(6.0);
            let hint = match st.last_tweaked.and_then(|t| crate::automation::control(p, t)) {
                Some((_, _, name)) => format!("Automate {name} (the control you changed last)"),
                None => "Automate the control you change next: move a knob first".to_string(),
            };
            let key = widgets::Legend::Icon(widgets::Icon::Curve);
            if widgets::button_sized(ui, vec2(h, h), key, None, &hint).clicked() {
                out.push(Action::Automate);
            }
            ui.add_space(10.0);
            ui.label(egui::RichText::new("snap").small().color(theme::TEXT_DIM));
            let name = SNAPS.iter().find(|s| s.1 == self.snap).map_or("?", |s| s.0);
            let _ = name;
            let rows: Vec<_> = SNAPS.iter().map(|(n, _)| widgets::DropRow::text(*n)).collect();
            let at = SNAPS.iter().position(|s| s.1 == self.snap).unwrap_or(0);
            if let Some(i) = widgets::dropdown(ui, "pl-snap", 96.0, &rows, at, false, "") {
                self.snap = SNAPS[i].1;
            }
            ui.add_space(10.0);
            let placing = if st.place_audio { st.audio.and_then(|id| p.audio(id)) } else { None };
            let automation = st.automation.filter(|_| st.place_automation).and_then(|id| p.automation(id));
            if let Some(a) = automation {
                let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, theme::channel_color(a.color));
                ui.label(egui::RichText::new(format!("click places: {}", a.name)).color(theme::TEXT_DIM));
            } else if let Some(a) = placing {
                let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, theme::channel_color(a.color));
                ui.label(egui::RichText::new(format!("click places: {}", a.name)).color(theme::TEXT_DIM))
                    .hint("Click a pattern clip (or pick a pattern in the top bar) to place patterns again.");
            } else if let Some(pat) = p.patterns.get(st.pattern) {
                let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, theme::channel_color(st.pattern));
                ui.label(egui::RichText::new(format!("click places: {}", pat.name)).color(theme::TEXT_DIM))
                    .hint("Pick another pattern in the top bar (or click a clip) to place that one.");
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
        // The Cut and Slide tools take over presses on clips (and do nothing
        // on empty space).
        if self.tool != Tool::Edit {
            let Some(i) = hit(v, &p.clips, pos) else { return };
            // The selected clips too, if this is one of them.
            let targets = if self.selection.contains(&i) { self.selection.clone() } else { vec![i] };
            h.checkpoint(p);
            if self.tool == Tool::Cut {
                let at = timeline::snap(anchor_tick, snap).max(0.0) as u32;
                for j in targets {
                    cut(p, j, at);
                }
                self.drag = Drag::None;
            } else {
                let originals = targets.iter().map(|&j| (j, p.clips[j].clone())).collect();
                self.drag = Drag::Slide { anchor_tick, originals, free };
            }
            return;
        }
        // An audio clip's handles: its own fades and gain.
        // (A double-click always opens the clip, handles or not.)
        if let Some(hd) = handle_hit(v, p, pos).filter(|_| !double) {
            h.checkpoint(p);
            let c = &mut p.clips[hd.clip];
            let fade = |end| if end == End::In { c.fade_in } else { c.fade_out };
            self.drag = match hd.grip {
                Grip::Gain => Drag::Gain { clip: hd.clip, orig: c.gain, anchor_y: pos.y },
                Grip::Curve(end) => Drag::Tension { clip: hd.clip, end, orig: fade(end).tension, anchor_y: pos.y },
                Grip::Fade(end) => {
                    let orig = fade(end).len;
                    let spt = p.seconds_per_tick() as f32;
                    let c = &p.clips[hd.clip];
                    let room = audio_room(p, c).unwrap_or(c.len).min(c.len);
                    Drag::Fade {
                        clip: hd.clip,
                        end,
                        orig,
                        anchor_x: pos.x,
                        seconds_per_px: spt / v.time.ppt,
                        max: room as f32 * spt,
                    }
                }
            };
            return;
        }
        let edge = edge_hit(v, &p.clips, pos);
        let left = if edge.is_none() { left_edge_hit(v, &p.clips, pos) } else { None };
        if let Some(i) = edge.or(left).or_else(|| hit(v, &p.clips, pos)) {
            // Clicking a clip makes it the current one (FL-style): the next click
            // on empty space places a copy of it, settings and all.
            st.clip = Some(p.clips[i].id);
            match p.clips[i].source {
                Source::Pattern(id) => {
                    if let Some(idx) = p.pattern_index(id) {
                        st.pattern = idx;
                    }
                    st.place_audio = false;
                }
                Source::Audio(id) => {
                    st.audio = Some(id);
                    st.place_audio = true;
                }
                Source::Automation(id) => st.automation = Some(id),
            }
            if double {
                // Double-click a clip: edit its pattern in the piano roll, its
                // audio in the audio-clip editor, its curve in the automation editor.
                match p.clips[i].source {
                    Source::Pattern(id) => st.open_pattern = p.pattern_index(id),
                    Source::Audio(id) => st.open_audio = Some(id),
                    Source::Automation(id) => st.open_automation = Some(id),
                }
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
                    p.add_clip(c);
                }
                self.selection = (base..p.clips.len()).collect();
                lead_i = self.selection[at];
            }
            let originals: Vec<(usize, Clip)> = self.selection.iter().map(|&j| (j, p.clips[j].clone())).collect();
            let lead = originals.iter().position(|(j, _)| *j == lead_i).unwrap_or(0);
            self.drag = if edge.is_some() {
                Drag::Resize { anchor_tick, originals, lead, free }
            } else if left.is_some() {
                Drag::ResizeLeft { anchor_tick, originals, lead, free }
            } else {
                Drag::Move { anchor_tick, anchor_track, originals, lead, free }
            };
        } else if let Ok(track) = usize::try_from(anchor_track) {
            if track >= p.tracks.len() {
                return;
            }
            // Place a copy of the current clip here (its length, window, fades
            // and gain too), or else the current pattern / audio clip at full
            // length, and keep dragging it.
            let start = (anchor_tick.max(0.0) as u32 / snap) * snap;
            let placing = st.audio.filter(|_| st.place_audio).filter(|&id| p.audio(id).is_some());
            let automation = st.automation.filter(|_| st.place_automation).and_then(|id| p.automation(id));
            let template = st.clip.and_then(|id| p.clip_index(id)).map(|i| p.clips[i].clone());
            let clip = if let Some(t) = template {
                Clip { track, start, ..t }
            } else if let Some(a) = automation {
                Clip::of_automation(a.id, track, start, a.length.max(1))
            } else if let Some(id) = placing {
                let probe = Clip::of_audio(id, track, start, 1);
                Clip::of_audio(id, track, start, audio_room(p, &probe).unwrap_or(TICKS_PER_BAR))
            } else if let Some(pat) = p.patterns.get(st.pattern) {
                Clip::of_pattern(pat.id, track, start, pat.play_length() * TICKS_PER_STEP)
            } else {
                return;
            };
            h.checkpoint(p);
            let i = p.add_clip(clip);
            st.clip = Some(p.clips[i].id);
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
                let start = lead_clip.start as f32;
                let to = timeline::drag_to_grid(start, *anchor_tick, v.tick(pos.x), v.time.ppt, snap).unwrap_or(start);
                let min_start = originals.iter().map(|(_, c)| c.start).min().unwrap_or(0) as i64;
                let dt = (to.max(0.0) as i64 - lead_clip.start as i64).max(-min_start);
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
                let to = timeline::drag_to_grid(lead_clip.len as f32, *anchor_tick, v.tick(pos.x), v.time.ppt, snap);
                let dl = to.map_or(0, |t| t.max(snap.max(TICKS_PER_STEP) as f32) as i64 - lead_clip.len as i64);
                for (i, c) in originals.iter() {
                    // An audio clip can't run past the end of its audio.
                    let room = audio_room(p, c).map_or(i64::MAX, |r| r as i64);
                    if let Some(clip) = p.clips.get_mut(*i) {
                        clip.len = (c.len as i64 + dl).max(TICKS_PER_STEP as i64).min(room.max(1)) as u32;
                    }
                }
            }
            Drag::ResizeLeft { anchor_tick, originals, lead, free } => {
                *free |= mods.alt;
                let snap = if *free { 1 } else { self.snap };
                let lead_clip = &originals[*lead].1;
                let start = lead_clip.start as f32;
                let to = timeline::drag_to_grid(start, *anchor_tick, v.tick(pos.x), v.time.ppt, snap);
                let mut d = to.map_or(0, |t| t as i64 - lead_clip.start as i64);
                // Every clip in the selection moves its start the same amount:
                // not before the start of its pattern / audio, not past its own end.
                for (_, c) in originals.iter() {
                    d = d.max(-(c.offset as i64)).max(-(c.start as i64)).min(c.len as i64 - 1);
                }
                for (i, c) in originals.iter() {
                    if let Some(clip) = p.clips.get_mut(*i) {
                        clip.start = (c.start as i64 + d) as u32;
                        clip.offset = (c.offset as i64 + d) as u32;
                        clip.len = (c.len as i64 - d) as u32;
                    }
                }
            }
            Drag::Fade { clip, end, orig, anchor_x, seconds_per_px, max } => {
                let dx = pos.x - *anchor_x;
                let d = if *end == End::In { dx } else { -dx };
                let len = (*orig + d * *seconds_per_px).clamp(0.0, max.max(0.0));
                if let Some(c) = p.clips.get_mut(*clip) {
                    match end {
                        End::In => c.fade_in.len = len,
                        End::Out => c.fade_out.len = len,
                    }
                }
            }
            Drag::Tension { clip, end, orig, anchor_y } => {
                let body = fade_geo(v, p, &p.clips[*clip]).body;
                let t = waveform::drag_tension(*orig, *anchor_y, pos.y, body);
                if let Some(c) = p.clips.get_mut(*clip) {
                    match end {
                        End::In => c.fade_in.tension = t,
                        End::Out => c.fade_out.tension = t,
                    }
                }
            }
            Drag::Slide { anchor_tick, originals, free } => {
                *free |= mods.alt;
                let snap = if *free { 1 } else { self.snap };
                // Content follows the mouse: dragging right shows earlier material.
                let d =
                    timeline::drag_to_grid(0.0, *anchor_tick, v.tick(pos.x), v.time.ppt, snap).unwrap_or(0.0) as i64;
                let spt = p.seconds_per_tick();
                for (i, c) in originals.iter() {
                    let wanted = c.offset as i64 - d;
                    let offset = match c.source {
                        Source::Audio(id) => {
                            // Audio stops at its ends.
                            let total = p.audio(id).map_or(0, |a| (a.length() / spt).floor() as i64);
                            wanted.clamp(0, (total - c.len as i64).max(0))
                        }
                        Source::Pattern(id) => {
                            // A pattern repeats forever: wrap around it.
                            let loop_len = p.pattern(id).map_or(1, |pat| (pat.play_length() * TICKS_PER_STEP).max(1));
                            wanted.rem_euclid(loop_len as i64)
                        }
                        // So does an automation's curve.
                        Source::Automation(id) => {
                            wanted.rem_euclid(p.automation(id).map_or(1, |a| a.length.max(1)) as i64)
                        }
                    };
                    if let Some(clip) = p.clips.get_mut(*i) {
                        clip.offset = offset as u32;
                    }
                }
            }
            Drag::Gain { clip, orig, anchor_y } => {
                // Up = louder, 0.1 dB per point (Shift: finer).
                let per = if mods.shift { 0.02 } else { 0.1 };
                if let Some(c) = p.clips.get_mut(*clip) {
                    c.gain = (*orig - (pos.y - *anchor_y) * per).clamp(-60.0, 24.0);
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
            Drag::Loop { anchor, moved } => {
                let tick = v.tick(pos.x).max(0.0);
                *moved |= (tick - *anchor).abs() * v.time.ppt >= 3.0;
                if *moved {
                    self.looped = Some(timeline::loop_span(*anchor, tick, self.snap));
                }
            }
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
    /// clip's light, and a flat title strip in that light with dark ink.
    #[allow(clippy::too_many_arguments)]
    fn paint_clips(
        &self,
        painter: &egui::Painter,
        v: &View,
        p: &Project,
        st: &UiState,
        playhead: Option<f64>,
        samples: &mut Samples,
        hot: Option<(usize, Option<Grip>)>,
    ) {
        let m = theme::m();
        for (i, c) in p.clips.iter().enumerate() {
            let r = v.clip_rect(c);
            if !r.intersects(v.grid) {
                continue;
            }
            // Current: the clip the next click copies; with none, every copy of
            // what the next click places (the current pattern or audio clip).
            let template = st.clip.filter(|&id| p.clip_index(id).is_some());
            let (name, current) = match c.source {
                Source::Pattern(id) => match p.pattern(id) {
                    Some(pat) => (pat.name.as_str(), !st.place_audio && p.pattern_index(id) == Some(st.pattern)),
                    None => continue,
                },
                Source::Audio(id) => match p.audio(id) {
                    Some(a) => (a.name.as_str(), st.place_audio && st.audio == Some(id)),
                    None => continue,
                },
                Source::Automation(id) => match p.automation(id) {
                    Some(a) => (a.name.as_str(), false),
                    None => continue,
                },
            };
            let current = template.map_or(current, |t| t == c.id);
            let light = clip_light(p, c);
            let playing = playhead.is_some_and(|t| (c.start as f64..c.end() as f64).contains(&t));
            let selected = self.selection.contains(&i);
            // The body is a see-through tint of the clip's light, so what's in it
            // (notes, waveform) stands out against the grid.
            let (_, strip, body) = parts(r);
            let alpha = if current { theme::CLIP_BODY_CURRENT } else { theme::CLIP_BODY }
                + if playing { theme::CLIP_BODY_PLAYING } else { 0.0 };
            painter.rect_filled(r, cr(4), light.mid.gamma_multiply(alpha));
            if theme::CLIP_EDGE > 0.0 {
                painter.rect_stroke(
                    r,
                    cr(4),
                    stroke(1.0, light.mid.gamma_multiply(theme::CLIP_EDGE)),
                    StrokeKind::Inside,
                );
            }
            let strip_fill = theme::clip_strip(light, playing);
            painter.rect_filled(strip, egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, strip_fill);
            if r.width() > 24.0 {
                painter.with_clip_rect(strip.intersect(v.grid)).text(
                    strip.left_center() + vec2(5.0, 0.0),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(m.text_small),
                    theme::clip_ink(light),
                );
            }
            match c.source {
                Source::Pattern(id) => {
                    if let Some(pat) = p.pattern(id) {
                        paint_notes(painter, v, p, c, pat, body);
                    }
                }
                Source::Audio(id) => {
                    if let Some(a) = p.audio(id) {
                        let hot = hot.filter(|h| h.0 == i).map(|h| h.1);
                        paint_audio(painter, v, p, c, a, light, samples, hot);
                    }
                }
                Source::Automation(id) => {
                    if let Some(a) = p.automation(id) {
                        paint_curve(painter, v, c, a, light, body);
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

/// Split clip `i` at song tick `at` (if `at` is inside it): it keeps the part
/// before, a new clip plays the rest, from the same point in its pattern or
/// audio. Audio halves get a short fade where they were cut (no click).
fn cut(p: &mut Project, i: usize, at: u32) {
    let c = p.clips[i].clone();
    if at <= c.start || at >= c.end() {
        return;
    }
    let before = at - c.start;
    let audio = c.audio().is_some();
    let rest = Clip {
        start: at,
        len: c.end() - at,
        offset: c.offset + before,
        fade_in: if audio { lyra_model::EDGE_FADE } else { c.fade_in },
        ..c
    };
    let first = &mut p.clips[i];
    first.len = before;
    if audio {
        first.fade_out = lyra_model::EDGE_FADE;
    }
    p.add_clip(rest);
}

/// Clips are lit by what they play, so the same pattern (or audio clip)
/// looks the same everywhere.
pub fn clip_light(p: &Project, c: &Clip) -> theme::Light {
    match c.source {
        Source::Pattern(id) => theme::channel_light(p.pattern_index(id).unwrap_or(0)),
        Source::Audio(id) => theme::channel_light(p.audio(id).map_or(0, |a| a.color)),
        Source::Automation(id) => theme::channel_light(p.automation(id).map_or(0, |a| a.color)),
    }
}

/// An automation clip's body: its curve over the clip's window, a line with
/// a faint fill under it.
fn paint_curve(
    painter: &egui::Painter,
    v: &View,
    c: &Clip,
    a: &lyra_model::Automation,
    light: theme::Light,
    body: Rect,
) {
    let area = body.shrink2(vec2(1.0, 3.0));
    let clipped = painter.with_clip_rect(body.intersect(v.grid));
    let pts = crate::automation::window(a, c.offset, c.len);
    let (x0, x1) = (v.x(c.start as f32).max(v.grid.left() - 2.0), v.x(c.end() as f32).min(v.grid.right() + 2.0));
    if x1 <= x0 {
        return;
    }
    let y = |val: f32| area.bottom() - val.clamp(0.0, 1.0) * area.height();
    let mut line = Vec::new();
    let mut x = x0;
    while x <= x1 {
        let t = (v.tick(x) - c.start as f32) as f64;
        if let Some(val) = lyra_engine::curve_at(&pts, t) {
            line.push(pos2(x, y(val)));
        }
        x += 2.0;
    }
    if line.len() < 2 {
        return;
    }
    let fill = light.mid.gamma_multiply(0.18);
    for w in line.windows(2) {
        let r = Rect::from_min_max(pos2(w[0].x, w[0].y.min(w[1].y)), pos2(w[1].x, area.bottom()));
        clipped.rect_filled(r, cr(0), fill);
    }
    clipped.add(egui::Shape::line(line, stroke(1.5, light.hi)));
}

/// A pattern clip's body: note silhouettes of every channel, repeated across the clip.
fn paint_notes(painter: &egui::Painter, v: &View, p: &Project, c: &Clip, pat: &lyra_model::Pattern, body: Rect) {
    let body_r = body.shrink2(vec2(2.0, 2.0));
    let rows = crate::pool::NoteRows::new(p, pat);
    let clipped = painter.with_clip_rect(body_r.intersect(v.grid));
    // Where the pattern starts over.
    for t in c.pattern_restarts(pat) {
        let x = v.x((c.start + t) as f32);
        clipped.vline(x, body_r.y_range(), stroke(1.0, Color32::from_black_alpha(90)));
    }
    for ch in &p.channels {
        let ink = theme::channel_light(ch.color).mid;
        for (at, len, n) in c.pattern_notes(pat, ch.id) {
            let x0 = v.x((c.start + at) as f32);
            let x1 = v.x((c.start + at + len) as f32);
            let y = rows.y(body_r, ch.id, n.key) + 1.5;
            clipped.rect_filled(Rect::from_min_max(pos2(x0, y - 1.5), pos2(x1.max(x0 + 1.5), y)), cr(0), ink);
        }
    }
}

/// An audio clip's body: its waveform, shaped by the gain and fades it
/// plays with, and a thin line for each of its fades. Hovered (`hot` is
/// Some: the handle under the mouse, if any), it shows its handles too.
#[allow(clippy::too_many_arguments)]
fn paint_audio(
    painter: &egui::Painter,
    v: &View,
    p: &Project,
    c: &Clip,
    a: &Audio,
    light: theme::Light,
    samples: &mut Samples,
    hot: Option<Option<Grip>>,
) {
    let m = theme::m();
    let g = fade_geo(v, p, c);
    let clipped = painter.with_clip_rect(g.body.intersect(v.grid));
    if g.x_end < g.body.right() {
        // The clip runs on past the end of its sound: nothing plays here.
        let rest = Rect::from_min_max(pos2(g.x_end, g.body.top()), g.body.right_bottom());
        clipped.rect_filled(rest, cr(0), Color32::from_black_alpha(90));
    }
    let Some(peaks) = samples.peaks(&a.sample) else {
        clipped.text(
            g.body.left_center() + vec2(5.0, 0.0),
            Align2::LEFT_CENTER,
            "file missing",
            FontId::proportional(m.text_small),
            theme::TEXT_DIM,
        );
        return;
    };
    let spt = p.seconds_per_tick();
    let sr = peaks.sample_rate() as f64;
    let gain = a.volume * lyra_plugin::db_to_gain(c.gain);
    let wave = g.body.shrink2(vec2(0.0, 2.0));
    waveform::paint(&clipped, wave, &peaks, light.hi, 1.0, |x| {
        if x < g.x0 || x >= g.x_end {
            return None;
        }
        // Seconds into the file at screen-x: through the window forwards, or
        // backwards from its end when the clip is reversed.
        let from = a.start + c.offset as f64 * spt;
        let span = (g.x_end - g.x0) as f64 / v.time.ppt as f64 * spt;
        let secs = |x: f32| {
            let el = (v.tick(x) - c.start as f32) as f64 * spt;
            if c.reverse {
                from + span - el
            } else {
                from + el
            }
        };
        let mut gain = gain;
        if x < g.x_in {
            gain *= waveform::fade_gain(End::In, x, g.x0, g.x_in, c.fade_in.tension);
        }
        if x > g.x_out {
            gain *= waveform::fade_gain(End::Out, x, g.x_out, g.x_end, c.fade_out.tension);
        }
        let (s0, s1) = (secs(x), secs(x + 1.0));
        Some((s0.min(s1) * sr, s0.max(s1) * sr, gain))
    });
    // The fades' curves: thin, in the clip's light, so they never hide the waveform.
    let ink = light.hi.gamma_multiply(if hot.is_some() { 0.9 } else { 0.5 });
    let mut mids = [None, None];
    for (k, (end, f0, f1, fade)) in
        [(End::In, g.x0, g.x_in, c.fade_in), (End::Out, g.x_out, g.x_end, c.fade_out)].into_iter().enumerate()
    {
        if f1 - f0 >= 3.0 {
            waveform::paint_fade(&clipped, g.body, end, f0, f1, fade.tension, ink, false);
            mids[k] = curve_mid(&g, end, fade.tension);
        }
    }
    let Some(hot) = hot else { return };
    if !shows_handles(v.clip_rect(c)) {
        return; // too small to grab anything: just the waveform
    }
    let on_grid = painter.with_clip_rect(v.grid);
    for (k, end) in [End::In, End::Out].into_iter().enumerate() {
        if let Some(mid) = mids[k] {
            waveform::paint_tension(&clipped, mid, light.hi, hot == Some(Grip::Curve(end)), waveform::SMALL);
        }
        let x = if end == End::In { g.x_in } else { g.x_out };
        waveform::paint_handle(&on_grid, pos2(x, g.handles_y), light.hi, hot == Some(Grip::Fade(end)), waveform::SMALL);
    }
    // The gain grip, with the clip's gain beside it while it's under the mouse.
    let at = gain_pos(&g);
    let gain_hot = hot == Some(Grip::Gain);
    waveform::paint_gain(&clipped, at, light.hi, gain_hot, waveform::SMALL);
    if gain_hot {
        let text = format!("{:+.1} dB", c.gain);
        clipped.text(at + vec2(9.0, 0.0), Align2::LEFT_CENTER, text, FontId::monospace(m.text_small), light.hi);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gain_grip_stays_on_screen() {
        let body = Rect::from_min_max(pos2(-5000.0, 0.0), pos2(9000.0, 40.0));
        let g = FadeGeo {
            body,
            handles_y: 0.0,
            x0: -5000.0,
            x_in: -4990.0,
            x_out: 8990.0,
            x_end: 9000.0,
            view: (200.0, 1400.0),
        };
        assert_eq!(gain_pos(&g).x, 800.0, "the middle of what you see");
        let short = FadeGeo { x0: 300.0, x_end: 500.0, ..g };
        assert_eq!(gain_pos(&short).x, 400.0, "a clip in view keeps it in its own middle");
    }
}
