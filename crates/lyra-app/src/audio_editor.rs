//! The audio-clip editor: one audio clip, big.
//!
//! It shows the clip last clicked on the playlist (double-click one to open
//! it here): the whole file, with that clip's *window* onto it framed, and
//! that clip's own fades and gain on the frame. What every copy shares (the
//! source) is just the trim and a gain. None of it touches the file on disk:
//!
//! * **the frame** — this clip's window: drag it sideways to hear another
//!   part of the audio (the clip stays where it is on the playlist)
//! * **on the frame** — the dots are this clip's fades (drag sideways), the
//!   rings on their curves bend them (drag up and down: up = louder sooner),
//!   the ↕ in the middle is its gain (up/down, Shift = fine, double-click = 0 dB)
//! * **the white lines** — trim, for every copy: where in the file the audio
//!   starts and ends (drag them; outside them the file is drawn dim)
//! * **knobs** — the same, as numbers: this clip's, and every copy's gain ·
//!   **Reverse** — this clip plays its window backwards (its fade-in then
//!   sits at the frame's right end, where it starts)
//! * **ruler** — click to play the song from there (in this clip)
//! * **Shift+wheel** (or the wheel over the ruler) — zoom at the mouse ·
//!   **Ctrl+wheel** — sideways · **Alt+wheel** — waveform height ·
//!   **middle-drag** — pan · double-click a knob — back to its default

use crate::app::UiState;
use crate::keys::Action;
use crate::samples::{Peaks, Samples};
use crate::theme::{self, cr, stroke};
use crate::timeline::{self, Wheel};
use crate::waveform::{self, End};
use crate::widgets;
use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Pos2, Rect, RichText, Sense, StrokeKind, Ui};
use lyra_model::{AudioId, Clip, Fade, History, Project, Source};
use lyra_plugin::{Curve, ParamInfo};

// Ranges sized for voice work: every knob in Lyra turns 0.6% of its range per
// point dragged (Shift: finer), so a wide range feels fast.
const GAIN: ParamInfo = ParamInfo::new(0, "Gain", -24.0, 12.0, 0.0, "dB", Curve::Linear);
const FADE_IN: ParamInfo = ParamInfo::new(1, "Fade in", 0.001, 5.0, 0.005, "s", Curve::Exponential);
const FADE_OUT: ParamInfo = ParamInfo::new(3, "Fade out", 0.001, 5.0, 0.005, "s", Curve::Exponential);
const CURVE_IN: ParamInfo = ParamInfo::new(2, "Curve", -1.0, 1.0, 0.0, "", Curve::Linear);
const CURVE_OUT: ParamInfo = ParamInfo::new(4, "Curve", -1.0, 1.0, 0.0, "", Curve::Linear);

/// This clip's knobs: gain, then each fade's length and curve.
const CLIP_KNOBS: [(&ParamInfo, (usize, usize)); 5] =
    [(&GAIN, (0, 0)), (&FADE_IN, (0, 1)), (&CURVE_IN, (1, 1)), (&FADE_OUT, (0, 2)), (&CURVE_OUT, (1, 2))];
/// Every copy's volume: the same fader as its row in the Audio rack.
const SOURCE_KNOBS: [(&ParamInfo, (usize, usize)); 1] = [(&crate::rack::VOLUME, (0, 0))];

/// The plates of knobs (this clip's is two knobs wide, every copy's one).
const PLATE_W: f32 = 150.0;
const SOURCE_W: f32 = 84.0;
const HANDLE_R: f32 = 10.0;
/// The trim lines can be grabbed this far to either side.
const TRIM_GRAB: f32 = 5.0;
/// Fades narrower than this show no curve ring.
const CURVE_MIN_W: f32 = 28.0;

enum Drag {
    None,
    /// A trim line: `end` = the right one.
    Trim {
        end: bool,
        orig: f64,
        anchor_x: f32,
    },
    /// This clip's fade length.
    Fade {
        clip: usize,
        end: End,
        orig: f32,
        anchor_x: f32,
    },
    /// This clip's fade curve.
    Tension {
        clip: usize,
        end: End,
        orig: f32,
        anchor_y: f32,
    },
    /// This clip's gain.
    Gain {
        clip: usize,
        orig: f32,
        anchor_y: f32,
    },
    /// The clip's window: slides its offset.
    Slide {
        clip: usize,
        orig: u32,
        anchor_x: f32,
    },
    Seek,
    Hand {
        last: Pos2,
    },
}

pub struct AudioEditor {
    /// Seconds into the file at the left edge, and points per second.
    scroll: f64,
    pps: f64,
    /// Waveform height (Alt+wheel).
    vzoom: f32,
    /// The audio the view was last fitted to.
    framed: Option<AudioId>,
    drag: Drag,
    clicks: timeline::Clicks,
}

impl Default for AudioEditor {
    fn default() -> Self {
        Self {
            scroll: 0.0,
            pps: 100.0,
            vzoom: 1.0,
            framed: None,
            drag: Drag::None,
            clicks: timeline::Clicks::default(),
        }
    }
}

/// Which audio clip the editor shows: the current one, or the first.
fn current(p: &Project, st: &mut UiState) -> Option<usize> {
    let i = st.audio.and_then(|id| p.audio_index(id)).or(if p.audio.is_empty() { None } else { Some(0) })?;
    st.audio = Some(p.audio[i].id);
    Some(i)
}

/// The playlist clip last clicked, if it plays this audio.
fn context_clip(p: &Project, st: &UiState, id: AudioId) -> Option<usize> {
    st.clip.and_then(|c| p.clip_index(c)).filter(|&i| p.clips[i].audio() == Some(id))
}

/// Gain and fades as the knobs see them: [gain, in, in curve, out, out curve].
fn levels(c: &Clip) -> [f32; 5] {
    [c.gain, c.fade_in.len, c.fade_in.tension, c.fade_out.len, c.fade_out.tension]
}

fn set_level(c: &mut Clip, i: usize, v: f32) {
    let fade = |f: &mut Fade, len: bool| if len { f.len = v } else { f.tension = v };
    match i {
        0 => c.gain = v,
        1 => fade(&mut c.fade_in, true),
        2 => fade(&mut c.fade_in, false),
        3 => fade(&mut c.fade_out, true),
        _ => fade(&mut c.fade_out, false),
    }
}

/// What a plate of knobs did this frame.
struct PlateEdit {
    /// Take an undo step now (a knob was grabbed, or changed by wheel/click).
    checkpoint: bool,
    /// (which knob, new value).
    change: Option<(usize, f32)>,
}

/// Screen ↔ file time for one frame.
#[derive(Clone, Copy)]
struct View {
    wave: Rect,
    scroll: f64,
    pps: f64,
}

impl View {
    fn x(&self, secs: f64) -> f32 {
        self.wave.left() + ((secs - self.scroll) * self.pps) as f32
    }
    fn secs(&self, x: f32) -> f64 {
        self.scroll + (x - self.wave.left()) as f64 / self.pps
    }
}

/// This clip on screen: its window (in file seconds and as a frame) and
/// where its fades turn.
struct Frame {
    clip: usize,
    /// Window start and end, in file seconds.
    from: f64,
    to: f64,
    rect: Rect,
    /// Where the fade-in ends and the fade-out starts, on screen. A reversed
    /// clip plays from the frame's right end, so there they swap sides.
    x_in: f32,
    x_out: f32,
    handles_y: f32,
    /// The gain grip: the middle of the part of the frame on screen, so a
    /// clip wider than the view still has it in reach.
    gain_at: Pos2,
    reverse: bool,
}

impl Frame {
    fn new(v: &View, p: &Project, clip: usize, a: &lyra_model::Audio) -> Self {
        let c = &p.clips[clip];
        let spt = p.seconds_per_tick();
        let from = a.start + c.offset as f64 * spt;
        let room = crate::playlist::audio_room(p, c).unwrap_or(c.len);
        let to = from + c.len.min(room) as f64 * spt;
        let rect = Rect::from_min_max(pos2(v.x(from), v.wave.top() + 1.0), pos2(v.x(to), v.wave.bottom() - 1.0));
        let (fi, fo) = (c.fade_in.len as f64, c.fade_out.len as f64);
        let (x_in, x_out) = if c.reverse {
            (v.x(to - fi).max(rect.left()), v.x(from + fo).min(rect.right()))
        } else {
            (v.x(from + fi).min(rect.right()), v.x(to - fo).max(rect.left()))
        };
        let (l, r) = (rect.left().max(v.wave.left()), rect.right().min(v.wave.right()));
        let gain_at = pos2((l + r) * 0.5, rect.center().y);
        Frame { clip, from, to, rect, x_in, x_out, handles_y: rect.top() + 12.0, gain_at, reverse: c.reverse }
    }

    /// A fade's stretch on screen and the shape it has there: (from x, to x,
    /// drawn as a rise (In) or a fall (Out)). Mirrored for a reversed clip.
    fn span(&self, end: End) -> (f32, f32, End) {
        match (end, self.reverse) {
            (End::In, false) => (self.rect.left(), self.x_in, End::In),
            (End::Out, false) => (self.x_out, self.rect.right(), End::Out),
            (End::In, true) => (self.x_in, self.rect.right(), End::Out),
            (End::Out, true) => (self.rect.left(), self.x_out, End::In),
        }
    }

    /// Where a fade's curve ring sits (None when the fade is too narrow).
    fn curve_mid(&self, end: End, tension: f32) -> Option<Pos2> {
        let (f0, f1, _) = self.span(end);
        (f1 - f0 >= CURVE_MIN_W).then(|| {
            pos2((f0 + f1) * 0.5, self.rect.bottom() - lyra_engine::fade_curve(0.5, tension) * self.rect.height())
        })
    }
}

/// What the mouse is over.
#[derive(Clone, Copy, PartialEq)]
enum Hover {
    Fade(End),
    Curve(End),
    Gain,
    Trim(bool),
    Window,
}

fn hover(v: &View, p: &Project, a: &lyra_model::Audio, frame: Option<&Frame>, pos: Pos2) -> Option<Hover> {
    if !v.wave.expand(HANDLE_R).contains(pos) {
        return None;
    }
    if let Some(f) = frame {
        let c = &p.clips[f.clip];
        let near = |at: Pos2| pos.distance(at) <= HANDLE_R;
        if near(pos2(f.x_in, f.handles_y)) {
            return Some(Hover::Fade(End::In));
        }
        if near(pos2(f.x_out, f.handles_y)) {
            return Some(Hover::Fade(End::Out));
        }
        if pos.distance(f.gain_at) <= HANDLE_R + 4.0 {
            return Some(Hover::Gain);
        }
        if f.curve_mid(End::In, c.fade_in.tension).is_some_and(near) {
            return Some(Hover::Curve(End::In));
        }
        if f.curve_mid(End::Out, c.fade_out.tension).is_some_and(near) {
            return Some(Hover::Curve(End::Out));
        }
    }
    let (t0, t1) = (v.x(a.start), v.x(a.end));
    if (pos.x - t0).abs() <= TRIM_GRAB {
        Some(Hover::Trim(false))
    } else if (pos.x - t1).abs() <= TRIM_GRAB {
        Some(Hover::Trim(true))
    } else if frame.is_some_and(|f| f.rect.x_range().contains(pos.x)) {
        Some(Hover::Window)
    } else {
        None
    }
}

impl AudioEditor {
    /// The editor's header row: the clip's name and how many copies play it.
    pub fn toolbar(&mut self, ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
        let Some(i) = current(p, st) else { return };
        let a = &p.audio[i];
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 4.0, theme::channel_color(a.color));
        if let Some(r) = widgets::name_field(ui, &a.name, 160.0) {
            if r.first {
                h.checkpoint(p);
            }
            p.audio[i].name = r.text;
        }
        let id = p.audio[i].id;
        let copies = p.clips.iter().filter(|c| c.source == Source::Audio(id)).count();
        let file = crate::samples::path_for(&p.audio[i].sample);
        ui.label(RichText::new(format!("{copies}× on the playlist")).small().color(theme::TEXT_DIM))
            .on_hover_text(file.display().to_string());
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        p: &mut Project,
        h: &mut History,
        st: &mut UiState,
        samples: &mut Samples,
        playhead: Option<f64>,
        out: &mut Vec<Action>,
    ) {
        let m = theme::m();
        let Some(ai) = current(p, st) else {
            ui.label(
                RichText::new("Drop a sample from the browser onto an arrangement track to make an audio clip.")
                    .color(theme::TEXT_DIM),
            );
            return;
        };
        let size = ui.available_size().max(vec2(420.0, 160.0));
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());

        // ── Knobs at the left: this clip's (when one is picked), then every copy's ──
        let id = p.audio[ai].id;
        let clip = context_clip(p, st, id);
        let color = theme::channel_color(p.audio[ai].color);
        let mut x = rect.left();
        if let Some(ci) = clip {
            let plate = Rect::from_min_max(pos2(x, rect.top()), pos2(x + PLATE_W, rect.bottom()));
            let k = knob_plate(ui, plate, "THIS CLIP", "clip-knob", &CLIP_KNOBS, &levels(&p.clips[ci]), color);
            if k.checkpoint {
                h.checkpoint(p);
            }
            if let Some((i, v)) = k.change {
                set_level(&mut p.clips[ci], i, v);
            }
            // Under the knobs: play this clip's window backwards.
            let cell_h = m.knob_large + 2.0 * m.text_small + 12.0;
            let at = pos2(plate.left() + 8.0, plate.top() + 18.0 + m.text_small + 3.0 * cell_h);
            let key = Rect::from_min_size(at, vec2(PLATE_W - 16.0, m.control_h));
            if key.bottom() <= plate.bottom() {
                let on = p.clips[ci].reverse;
                let r = ui.put(key, |ui: &mut Ui| {
                    widgets::key(
                        ui,
                        key.size(),
                        widgets::Legend::Text("Reverse"),
                        on,
                        theme::L_MINT,
                        "Play this clip's window backwards",
                    )
                });
                if r.clicked() {
                    h.checkpoint(p);
                    p.clips[ci].reverse = !on;
                }
            }
            x += PLATE_W + theme::SEAM;
        }
        let plate = Rect::from_min_max(pos2(x, rect.top()), pos2(x + SOURCE_W, rect.bottom()));
        let k = knob_plate(ui, plate, "EVERY COPY", "source-knob", &SOURCE_KNOBS, &[p.audio[ai].volume], color);
        if k.checkpoint {
            h.checkpoint(p);
        }
        if let Some((_, v)) = k.change {
            p.audio[ai].volume = v;
        }

        // ── The waveform ──
        let canvas = Rect::from_min_max(pos2(plate.right() + theme::SEAM, rect.top()), rect.max);
        let ruler = Rect::from_min_max(canvas.min, pos2(canvas.right(), canvas.top() + m.ruler_h));
        let wave = Rect::from_min_max(pos2(canvas.left(), ruler.bottom()), canvas.max);
        let Some(peaks) = samples.peaks(&p.audio[ai].sample) else {
            ui.painter().text(
                wave.center(),
                Align2::CENTER_CENTER,
                format!("Can't read {}", crate::samples::path_for(&p.audio[ai].sample).display()),
                FontId::proportional(m.text),
                theme::TEXT_DIM,
            );
            return;
        };
        let file_secs = peaks.frames() as f64 / peaks.sample_rate() as f64;
        let spt = p.seconds_per_tick();
        if self.framed != Some(id) {
            // First look at this audio: the whole file, with a little room after it.
            self.pps = wave.width() as f64 / (file_secs * 1.04).max(0.01);
            self.scroll = 0.0;
            self.framed = Some(id);
        }
        let min_pps = wave.width() as f64 / (file_secs * 4.0).max(0.01);
        let max_pps = peaks.sample_rate() as f64 * 4.0; // four points per sample

        let input = timeline::Input::read(ui, canvas);
        let timeline::Input { pos, pressed: pressed_p, pressed_middle: pressed_m, down, hovering, mods, .. } = input;
        let double = self.clicks.double(&input);

        // ── Wheel ── (the shared scheme; Alt, the "rows" zoom, is the waveform's height)
        if hovering {
            match timeline::wheel(&input, ruler) {
                Wheel::ZoomTime { factor, at } => {
                    let v = View { wave, scroll: self.scroll, pps: self.pps };
                    let anchor = v.secs(at);
                    self.pps = (self.pps * factor as f64).clamp(min_pps, max_pps);
                    self.scroll = anchor - (at - wave.left()) as f64 / self.pps;
                }
                Wheel::ZoomRows { factor, .. } => self.vzoom = (self.vzoom * factor).clamp(0.25, 16.0),
                Wheel::Scroll { x, .. } => self.scroll += x as f64 / self.pps,
                Wheel::None => {}
            }
        }
        let visible = wave.width() as f64 / self.pps;
        self.scroll = self.scroll.clamp(-visible * 0.5, (file_secs - visible * 0.5).max(0.0));
        let v = View { wave, scroll: self.scroll, pps: self.pps };
        let frame = clip.map(|ci| Frame::new(&v, p, ci, &p.audio[ai]));

        // ── Mouse ──
        if hovering && pressed_m {
            if let Some(pos) = pos {
                self.drag = Drag::Hand { last: pos };
            }
        } else if hovering && pressed_p {
            if let Some(pos) = pos {
                let a = &p.audio[ai];
                if ruler.contains(pos) {
                    self.drag = Drag::Seek;
                } else if let Some(hv) = hover(&v, p, a, frame.as_ref(), pos) {
                    h.checkpoint(p);
                    let c = frame.as_ref().map(|f| f.clip);
                    let fade =
                        |end| c.map(|ci| if end == End::In { p.clips[ci].fade_in } else { p.clips[ci].fade_out });
                    self.drag = match (hv, c) {
                        (Hover::Trim(end), _) => {
                            Drag::Trim { end, orig: if end { a.end } else { a.start }, anchor_x: pos.x }
                        }
                        (Hover::Gain, Some(ci)) if double => {
                            p.clips[ci].gain = 0.0;
                            Drag::None
                        }
                        (Hover::Gain, Some(ci)) => Drag::Gain { clip: ci, orig: p.clips[ci].gain, anchor_y: pos.y },
                        (Hover::Fade(end), Some(ci)) => {
                            Drag::Fade { clip: ci, end, orig: fade(end).map_or(0.0, |f| f.len), anchor_x: pos.x }
                        }
                        (Hover::Curve(end), Some(ci)) => {
                            Drag::Tension { clip: ci, end, orig: fade(end).map_or(0.0, |f| f.tension), anchor_y: pos.y }
                        }
                        (Hover::Window, Some(ci)) => {
                            Drag::Slide { clip: ci, orig: p.clips[ci].offset, anchor_x: pos.x }
                        }
                        _ => Drag::None,
                    };
                }
            }
        }
        if down {
            if let Some(pos) = pos {
                match &mut self.drag {
                    Drag::Trim { end, orig, anchor_x } => {
                        let a = &mut p.audio[ai];
                        let t = *orig + (pos.x - *anchor_x) as f64 / self.pps;
                        // At least 10 ms of sound; never outside the file.
                        if *end {
                            a.end = t.clamp(a.start + 0.01, file_secs);
                        } else {
                            a.start = t.clamp(0.0, a.end - 0.01);
                        }
                    }
                    Drag::Fade { clip, end, orig, anchor_x } => {
                        let span = frame.as_ref().map_or(0.0, |f| f.to - f.from);
                        let dx = (pos.x - *anchor_x) as f64 / self.pps;
                        // A fade on the left grows rightwards, on the right leftwards.
                        let left_side = frame.as_ref().is_none_or(|f| f.span(*end).2 == End::In);
                        let d = if left_side { dx } else { -dx };
                        let len = (*orig as f64 + d).clamp(0.0, span) as f32;
                        let c = &mut p.clips[*clip];
                        match end {
                            End::In => c.fade_in.len = len,
                            End::Out => c.fade_out.len = len,
                        }
                    }
                    Drag::Tension { clip, end, orig, anchor_y } => {
                        let area = frame.as_ref().map_or(wave, |f| f.rect);
                        let t = waveform::drag_tension(*orig, *anchor_y, pos.y, area);
                        let c = &mut p.clips[*clip];
                        match end {
                            End::In => c.fade_in.tension = t,
                            End::Out => c.fade_out.tension = t,
                        }
                    }
                    Drag::Gain { clip, orig, anchor_y } => {
                        // Up = louder, 0.1 dB per point (Shift: finer).
                        let per = if mods.shift { 0.02 } else { 0.1 };
                        p.clips[*clip].gain = (*orig - (pos.y - *anchor_y) * per).clamp(-60.0, 24.0);
                    }
                    Drag::Slide { clip, orig, anchor_x } => {
                        // Slide the window: the clip stays put on the playlist and
                        // plays another part of the audio (within the trim).
                        let c_len = p.clips[*clip].len as i64;
                        let max = ((p.audio[ai].length() / spt).floor() as i64 - c_len).max(0);
                        let d = ((pos.x - *anchor_x) as f64 / self.pps / spt).round() as i64;
                        p.clips[*clip].offset = (*orig as i64 + d).clamp(0, max) as u32;
                    }
                    Drag::Seek => {
                        if let Some(tick) = song_tick(p, id, clip, v.secs(pos.x), playhead, spt) {
                            out.push(Action::SongMode(true));
                            out.push(Action::Seek(tick));
                        }
                    }
                    Drag::Hand { last } => {
                        self.scroll -= (pos.x - last.x) as f64 / self.pps;
                        *last = pos;
                    }
                    Drag::None => {}
                }
            }
        } else {
            self.drag = Drag::None;
        }

        // Cursor feedback
        let frame = clip.map(|ci| Frame::new(&v, p, ci, &p.audio[ai]));
        let hv = match self.drag {
            Drag::None => pos.filter(|_| hovering).and_then(|pos| hover(&v, p, &p.audio[ai], frame.as_ref(), pos)),
            Drag::Trim { end, .. } => Some(Hover::Trim(end)),
            Drag::Fade { end, .. } => Some(Hover::Fade(end)),
            Drag::Tension { end, .. } => Some(Hover::Curve(end)),
            Drag::Gain { .. } => Some(Hover::Gain),
            Drag::Slide { .. } => Some(Hover::Window),
            _ => None,
        };
        match (hv, &self.drag) {
            (Some(Hover::Window), Drag::Slide { .. }) => ui.ctx().set_cursor_icon(CursorIcon::Grabbing),
            (Some(Hover::Window), _) => ui.ctx().set_cursor_icon(CursorIcon::Grab),
            (Some(Hover::Curve(_) | Hover::Gain), _) => ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical),
            (Some(_), _) => ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal),
            (None, Drag::Hand { .. }) => ui.ctx().set_cursor_icon(CursorIcon::Grabbing),
            _ => {}
        }

        // ── Paint ──
        let painter = ui.painter_at(canvas);
        let a = &p.audio[ai];
        let light = theme::channel_light(a.color);
        painter.rect_filled(wave, cr(0), theme::ROLL_BLACK_ROW);
        let (t0, t1) = (v.x(a.start), v.x(a.end));
        let trim = Rect::from_min_max(pos2(t0, wave.top()), pos2(t1, wave.bottom())).intersect(wave);
        painter.rect_filled(trim, cr(0), theme::mix(theme::PANEL, light.low, 0.24));
        painter.hline(wave.x_range(), wave.center().y, stroke(1.0, Color32::from_black_alpha(80)));
        let sr = peaks.sample_rate() as f64;
        let source_gain = a.volume;
        let column = |x: f32| (v.secs(x) * sr, v.secs(x + 1.0) * sr);
        // The whole file dim; the trimmed part brighter; this clip's window
        // brightest, shaped by its own gain and fades.
        paint_wave(&painter, wave, &peaks, theme::TEXT_DIM.gamma_multiply(0.45), self.vzoom, |x| {
            let (f0, f1) = column(x);
            Some((f0, f1, source_gain))
        });
        let in_trim = painter.with_clip_rect(trim);
        let trim_ink = if frame.is_some() { light.hi.gamma_multiply(0.45) } else { light.hi };
        paint_wave(&in_trim, wave, &peaks, trim_ink, self.vzoom, |x| {
            let (f0, f1) = column(x);
            Some((f0, f1, source_gain))
        });
        if let Some(f) = &frame {
            let c = &p.clips[f.clip];
            let hot = hv.is_some();
            painter.rect_filled(f.rect, cr(4), light.mid.gamma_multiply(if hot { 0.12 } else { 0.07 }));
            let inside = painter.with_clip_rect(f.rect.intersect(trim));
            let gain = source_gain * lyra_plugin::db_to_gain(c.gain);
            paint_wave(&inside, wave, &peaks, light.hi, self.vzoom, |x| {
                let (f0, f1) = column(x);
                let mut g = gain;
                for (end, fade) in [(End::In, c.fade_in), (End::Out, c.fade_out)] {
                    let (s0, s1, shape) = f.span(end);
                    if x >= s0 && x <= s1 {
                        g *= waveform::fade_gain(shape, x, s0, s1, fade.tension);
                    }
                }
                Some((f0, f1, g))
            });
            painter.rect_stroke(
                f.rect,
                cr(4),
                stroke(1.5, light.hi.gamma_multiply(if hv == Some(Hover::Window) { 1.0 } else { 0.7 })),
                StrokeKind::Inside,
            );
            // This clip's fades and gain, on its frame.
            let ink = light.hi.gamma_multiply(0.8);
            for (end, fade) in [(End::In, c.fade_in), (End::Out, c.fade_out)] {
                let (f0, f1, shape) = f.span(end);
                if f1 - f0 >= 3.0 {
                    waveform::paint_fade(&painter, f.rect, shape, f0, f1, fade.tension, ink, false);
                }
                if let Some(mid) = f.curve_mid(end, fade.tension) {
                    waveform::paint_tension(&painter, mid, light.hi, hv == Some(Hover::Curve(end)), waveform::LARGE);
                }
                let x = if end == End::In { f.x_in } else { f.x_out };
                waveform::paint_handle(
                    &painter,
                    pos2(x, f.handles_y),
                    light.hi,
                    hv == Some(Hover::Fade(end)),
                    waveform::LARGE,
                );
            }
            let gain_hot = hv == Some(Hover::Gain);
            waveform::paint_gain(&painter, f.gain_at, light.hi, gain_hot, waveform::LARGE);
            if gain_hot || c.gain != 0.0 {
                let text = format!("{:+.1} dB", c.gain);
                painter.text(
                    f.gain_at + vec2(12.0, 0.0),
                    Align2::LEFT_CENTER,
                    text,
                    FontId::monospace(m.text_small),
                    light.hi,
                );
            }
        }
        for (x, end) in [(t0, false), (t1, true)] {
            let hot = hv == Some(Hover::Trim(end));
            painter.vline(
                x,
                wave.y_range(),
                stroke(if hot { 2.0 } else { 1.5 }, if hot { theme::TEXT } else { theme::TEXT.gamma_multiply(0.7) }),
            );
        }
        paint_seconds(&painter, &v, ruler, file_secs);
        // Where the song is in this audio, while a copy of it plays.
        if let Some(t) = playhead.and_then(|ph| file_position(p, id, ph, spt)) {
            let x = v.x(t);
            if wave.x_range().contains(x) {
                timeline::paint_playhead(&painter, x, ruler, wave.y_range(), true);
            }
        }
        if hovering || !matches!(self.drag, Drag::None) {
            ui.ctx().request_repaint();
        }
    }
}

/// A plate of knobs on a two-column grid: `knobs` = (parameter, (column, row)),
/// `vals` their values in the same order.
#[allow(clippy::too_many_arguments)]
fn knob_plate(
    ui: &mut Ui,
    area: Rect,
    title: &str,
    salt: &str,
    knobs: &[(&ParamInfo, (usize, usize))],
    vals: &[f32],
    color: Color32,
) -> PlateEdit {
    let m = theme::m();
    ui.painter().rect_filled(area, cr(0), theme::PANEL);
    ui.painter().text(
        area.left_top() + vec2(8.0, 6.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(m.text_small),
        theme::TEXT_DIM,
    );
    let top = area.top() + 12.0 + m.text_small;
    let cell = vec2(area.width() * 0.5, m.knob_large + 2.0 * m.text_small + 12.0);
    let mut edit = PlateEdit { checkpoint: false, change: None };
    for (k, &(info, (col, row))) in knobs.iter().enumerate() {
        let r = Rect::from_min_size(pos2(area.left() + col as f32 * cell.x, top + row as f32 * cell.y), cell);
        if r.bottom() > area.bottom() {
            continue;
        }
        let mut v = vals[k];
        let knob =
            Rect::from_center_size(pos2(r.center().x, r.top() + m.knob_large * 0.5), vec2(m.knob_large, m.knob_large));
        let resp = widgets::knob_at(ui, ui.id().with((salt, k)), knob, &mut v, info, color);
        // Same rule as `rack::tracked`: one undo step per grab, or per click/wheel change.
        edit.checkpoint |= resp.drag_started() || (resp.changed() && !resp.dragged());
        if resp.changed() {
            edit.change = Some((k, v));
        }
        let painter = ui.painter();
        let y = knob.bottom() + 4.0;
        painter.text(
            pos2(r.center().x, y),
            Align2::CENTER_TOP,
            info.name,
            FontId::proportional(m.text_small),
            theme::TEXT,
        );
        painter.text(
            pos2(r.center().x, y + m.text_small + 2.0),
            Align2::CENTER_TOP,
            info.format(v),
            FontId::proportional(m.text_small),
            theme::TEXT_DIM,
        );
    }
    edit
}

fn paint_wave(
    painter: &egui::Painter,
    wave: Rect,
    peaks: &Peaks,
    color: Color32,
    zoom: f32,
    at: impl FnMut(f32) -> Option<(f64, f64, f32)>,
) {
    waveform::paint(painter, wave.shrink2(vec2(0.0, 4.0)), peaks, color, zoom, at);
}

/// A ruler in seconds (minutes past a minute).
fn paint_seconds(painter: &egui::Painter, v: &View, ruler: Rect, file_secs: f64) {
    let m = theme::m();
    painter.rect_filled(ruler, cr(0), theme::TOOLBAR);
    painter.hline(ruler.x_range(), ruler.bottom() - 0.5, stroke(1.0, theme::LINE));
    let steps = [0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0];
    let step = steps.into_iter().find(|s| s * v.pps >= 64.0).unwrap_or(600.0);
    let first = (v.secs(ruler.left()) / step).floor().max(0.0) as i64;
    let last = (v.secs(ruler.right()) / step).ceil() as i64;
    for k in first..=last {
        let t = k as f64 * step;
        let x = v.x(t);
        painter.vline(x, ruler.y_range(), stroke(1.0, theme::LINE));
        let decimals = if step >= 1.0 {
            0
        } else if step >= 0.1 {
            1
        } else if step >= 0.01 {
            2
        } else {
            3
        };
        let label = if t >= 60.0 {
            format!(
                "{}:{:0w$.d$}",
                (t / 60.0).floor(),
                t % 60.0,
                w = if decimals > 0 { decimals + 3 } else { 2 },
                d = decimals
            )
        } else {
            format!("{t:.decimals$} s")
        };
        painter.text(
            pos2(x + 3.0, ruler.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(m.text_small),
            theme::TEXT_DIM,
        );
    }
    let end = v.x(file_secs);
    if end < ruler.right() {
        painter.vline(end, ruler.y_range(), stroke(2.0, theme::GOLD.gamma_multiply(0.8)));
    }
}

/// The copy of audio clip `id` the song is in now, or else the first one.
fn clip_for(p: &Project, id: AudioId, playhead: Option<f64>) -> Option<&lyra_model::Clip> {
    let mine = || p.clips.iter().filter(move |c| c.source == Source::Audio(id));
    playhead
        .and_then(|ph| mine().find(|c| (c.start as f64..c.end() as f64).contains(&ph)))
        .or_else(|| mine().min_by_key(|c| c.start))
}

/// Seconds into the file the song is at, while a copy of clip `id` plays.
fn file_position(p: &Project, id: AudioId, playhead: f64, spt: f64) -> Option<f64> {
    let c = clip_for(p, id, Some(playhead)).filter(|c| (c.start as f64..c.end() as f64).contains(&playhead))?;
    let a = p.audio(id)?;
    Some(a.start + (c.offset as f64 + playhead - c.start as f64) * spt)
}

/// The song position that plays file second `secs`: in this clip (when one
/// is picked), else in the copy playing now, or the first; if it reaches it.
fn song_tick(p: &Project, id: AudioId, clip: Option<usize>, secs: f64, playhead: Option<f64>, spt: f64) -> Option<f64> {
    let c = clip.map(|i| &p.clips[i]).or_else(|| clip_for(p, id, playhead))?;
    let a = p.audio(id)?;
    let tick = c.start as f64 + (secs - a.start) / spt - c.offset as f64;
    (c.start as f64..c.end() as f64).contains(&tick).then_some(tick)
}
