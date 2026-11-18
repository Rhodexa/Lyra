//! The audio-clip editor: one audio clip, big.
//!
//! Opened from a clip on the playlist (double-click it), it knows *that*
//! clip: its window onto the audio is the framed part of the waveform, and
//! its own gain and fades are the "this clip" knobs. The rest is the clip's
//! *source*, shared by every copy (like a pattern's notes). None of it
//! touches the file on disk:
//!
//! * **knobs** — gain, the fades' lengths and their curves: this clip's, and
//!   every copy's
//! * **the frame** — this clip's window: drag it sideways to hear another
//!   part of the audio (the clip stays where it is on the playlist)
//! * **the white lines** — trim, for every copy: where in the file the audio
//!   starts and ends (drag them; outside them the file is drawn dim)
//! * **the dots** — the source's fade lengths (drag sideways) · **the rings
//!   on the curves** — bend them (drag up and down: up = louder sooner)
//! * **ruler** — click to play the song from there (in the clip that's
//!   playing, or the first one)
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
use lyra_model::{AudioId, Fade, History, Project, Source};
use lyra_plugin::{Curve, ParamInfo};

const GAIN: ParamInfo = ParamInfo::new(0, "Gain", -36.0, 12.0, 0.0, "dB", Curve::Linear);
const FADE_IN: ParamInfo = ParamInfo::new(1, "Fade in", 0.001, 10.0, 0.005, "s", Curve::Exponential);
const FADE_OUT: ParamInfo = ParamInfo::new(3, "Fade out", 0.001, 10.0, 0.005, "s", Curve::Exponential);
const CURVE_IN: ParamInfo = ParamInfo::new(2, "Curve", -1.0, 1.0, 0.0, "", Curve::Linear);
const CURVE_OUT: ParamInfo = ParamInfo::new(4, "Curve", -1.0, 1.0, 0.0, "", Curve::Linear);

/// One plate of knobs (this clip's, or every copy's).
const PLATE_W: f32 = 150.0;
const HANDLE_R: f32 = 6.0;
/// The trim lines can be grabbed this far to either side.
const TRIM_GRAB: f32 = 5.0;

enum Drag {
    None,
    /// A trim line: `end` = the right one.
    Trim {
        end: bool,
        orig: f64,
        anchor_x: f32,
    },
    Fade {
        end: End,
        orig: f32,
        anchor_x: f32,
    },
    Tension {
        end: End,
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
    /// The clip the view was last fitted to.
    framed: Option<AudioId>,
    drag: Drag,
}

impl Default for AudioEditor {
    fn default() -> Self {
        Self { scroll: 0.0, pps: 100.0, vzoom: 1.0, framed: None, drag: Drag::None }
    }
}

/// Which audio clip the editor shows: the current one, or the first.
fn current(p: &Project, st: &mut UiState) -> Option<usize> {
    let i = st.audio.and_then(|id| p.audio_index(id)).or(if p.audio.is_empty() { None } else { Some(0) })?;
    st.audio = Some(p.audio[i].id);
    Some(i)
}

/// The playlist clip the editor was opened from, if it still plays this audio.
fn context_clip(p: &Project, st: &UiState, id: AudioId) -> Option<usize> {
    st.audio_clip.filter(|&i| p.clips.get(i).is_some_and(|c| c.audio() == Some(id)))
}

/// Gain and fades as the knobs see them: [gain, in, in curve, out, out curve].
fn levels(gain: f32, fade_in: Fade, fade_out: Fade) -> [f32; 5] {
    [gain, fade_in.len, fade_in.tension, fade_out.len, fade_out.tension]
}

fn set_level(i: usize, v: f32, gain: &mut f32, fade_in: &mut Fade, fade_out: &mut Fade) {
    match i {
        0 => *gain = v,
        1 => fade_in.len = v,
        2 => fade_in.tension = v,
        3 => fade_out.len = v,
        _ => fade_out.tension = v,
    }
}

/// What a plate of knobs did this frame.
struct PlateEdit {
    /// Take an undo step now (a knob was grabbed, or changed by wheel/click).
    checkpoint: bool,
    /// (which level, new value).
    change: Option<(usize, f32)>,
}

/// A plate of five knobs: gain, then each fade's length and curve.
fn levels_plate(ui: &mut Ui, area: Rect, title: &str, salt: &str, vals: [f32; 5], color: Color32) -> PlateEdit {
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
    let cell = vec2(area.width() * 0.5, m.knob_small + 2.0 * m.text_small + 12.0);
    let infos = [&GAIN, &FADE_IN, &CURVE_IN, &FADE_OUT, &CURVE_OUT];
    let places = [(0, 0), (0, 1), (1, 1), (0, 2), (1, 2)];
    let mut edit = PlateEdit { checkpoint: false, change: None };
    for (k, (info, (col, row))) in infos.into_iter().zip(places).enumerate() {
        let r = Rect::from_min_size(pos2(area.left() + col as f32 * cell.x, top + row as f32 * cell.y), cell);
        if r.bottom() > area.bottom() {
            continue;
        }
        let mut v = vals[k];
        let knob =
            Rect::from_center_size(pos2(r.center().x, r.top() + m.knob_small * 0.5), vec2(m.knob_small, m.knob_small));
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

/// Where the fades and trim lines are on screen.
struct Geo {
    x0: f32,
    x_in: f32,
    x_out: f32,
    x_end: f32,
    handles_y: f32,
}

fn geo(v: &View, a: &lyra_model::Audio) -> Geo {
    let (x0, x_end) = (v.x(a.start), v.x(a.end));
    Geo {
        x0,
        x_end,
        x_in: v.x(a.start + a.fade_in.len as f64).min(x_end),
        x_out: v.x(a.end - a.fade_out.len as f64).max(x0),
        handles_y: v.wave.top() + 8.0,
    }
}

/// What the mouse is over.
#[derive(Clone, Copy, PartialEq)]
enum Hover {
    Fade(End),
    Curve(End),
    Trim(bool),
    /// Inside this clip's window.
    Window,
}

/// `window`: this clip's window in file seconds, when opened from a clip.
fn hover(v: &View, a: &lyra_model::Audio, window: Option<(f64, f64)>, pos: Pos2) -> Option<Hover> {
    if !v.wave.expand(HANDLE_R).contains(pos) {
        return None;
    }
    let g = geo(v, a);
    for (end, x, f0, f1, tension) in
        [(End::In, g.x_in, g.x0, g.x_in, a.fade_in.tension), (End::Out, g.x_out, g.x_out, g.x_end, a.fade_out.tension)]
    {
        if pos.distance(pos2(x, g.handles_y)) <= HANDLE_R {
            return Some(Hover::Fade(end));
        }
        if f1 - f0 >= 16.0 {
            let mid = pos2((f0 + f1) * 0.5, v.wave.bottom() - lyra_engine::fade_curve(0.5, tension) * v.wave.height());
            if pos.distance(mid) <= HANDLE_R {
                return Some(Hover::Curve(end));
            }
        }
    }
    if (pos.x - g.x0).abs() <= TRIM_GRAB {
        Some(Hover::Trim(false))
    } else if (pos.x - g.x_end).abs() <= TRIM_GRAB {
        Some(Hover::Trim(true))
    } else if window.is_some_and(|(s, e)| v.wave.contains(pos) && (v.x(s)..=v.x(e)).contains(&pos.x)) {
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

        // ── Knobs at the left: the clip's own (when opened from a clip), then the source's ──
        let id = p.audio[ai].id;
        let clip = context_clip(p, st, id);
        let color = theme::channel_color(p.audio[ai].color);
        let mut x = rect.left();
        if let Some(ci) = clip {
            let c = &p.clips[ci];
            let plate = Rect::from_min_max(pos2(x, rect.top()), pos2(x + PLATE_W, rect.bottom()));
            let k = levels_plate(ui, plate, "THIS CLIP", "clip-knob", levels(c.gain, c.fade_in, c.fade_out), color);
            if k.checkpoint {
                h.checkpoint(p);
            }
            if let Some((i, v)) = k.change {
                let c = &mut p.clips[ci];
                set_level(i, v, &mut c.gain, &mut c.fade_in, &mut c.fade_out);
            }
            x += PLATE_W + theme::SEAM;
        }
        let a = &p.audio[ai];
        let plate = Rect::from_min_max(pos2(x, rect.top()), pos2(x + PLATE_W, rect.bottom()));
        let k = levels_plate(ui, plate, "EVERY COPY", "source-knob", levels(a.gain, a.fade_in, a.fade_out), color);
        if k.checkpoint {
            h.checkpoint(p);
        }
        if let Some((i, v)) = k.change {
            let a = &mut p.audio[ai];
            set_level(i, v, &mut a.gain, &mut a.fade_in, &mut a.fade_out);
        }
        let knobs = Rect::from_min_max(rect.min, pos2(plate.right(), rect.bottom()));

        // ── The waveform ──
        let canvas = Rect::from_min_max(pos2(knobs.right() + theme::SEAM, rect.top()), rect.max);
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
        // This clip's window onto the audio, in file seconds.
        let window = clip.map(|ci| {
            let c = &p.clips[ci];
            let a = &p.audio[ai];
            let start = a.start + c.offset as f64 * spt;
            let room = crate::playlist::audio_room(p, c).unwrap_or(c.len);
            (start, start + c.len.min(room) as f64 * spt)
        });
        if self.framed != Some(id) {
            // First look at this clip: the whole file, with a little room after it.
            self.pps = wave.width() as f64 / (file_secs * 1.04).max(0.01);
            self.scroll = 0.0;
            self.framed = Some(id);
        }
        let min_pps = wave.width() as f64 / (file_secs * 4.0).max(0.01);
        let max_pps = peaks.sample_rate() as f64 * 4.0; // four points per sample

        let input = timeline::Input::read(ui, canvas);
        let timeline::Input { pos, pressed: pressed_p, pressed_middle: pressed_m, down, hovering, .. } = input;

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
                } else if let Some(hv) = hover(&v, a, window, pos) {
                    h.checkpoint(p);
                    self.drag = match hv {
                        Hover::Window => match clip {
                            Some(ci) => Drag::Slide { clip: ci, orig: p.clips[ci].offset, anchor_x: pos.x },
                            None => Drag::None,
                        },
                        Hover::Trim(end) => {
                            Drag::Trim { end, orig: if end { a.end } else { a.start }, anchor_x: pos.x }
                        }
                        Hover::Fade(end) => {
                            let orig = if end == End::In { a.fade_in.len } else { a.fade_out.len };
                            Drag::Fade { end, orig, anchor_x: pos.x }
                        }
                        Hover::Curve(end) => {
                            let orig = if end == End::In { a.fade_in.tension } else { a.fade_out.tension };
                            Drag::Tension { end, orig, anchor_y: pos.y }
                        }
                    };
                }
            }
        }
        if down {
            if let Some(pos) = pos {
                if let Drag::Slide { clip, orig, anchor_x } = self.drag {
                    // Slide the window: the clip stays put on the playlist and
                    // plays another part of the audio (within the trim).
                    let c_len = p.clips[clip].len as i64;
                    let max = ((p.audio[ai].length() / spt).floor() as i64 - c_len).max(0);
                    let d = ((pos.x - anchor_x) as f64 / self.pps / spt).round() as i64;
                    p.clips[clip].offset = (orig as i64 + d).clamp(0, max) as u32;
                }
                let a = &mut p.audio[ai];
                match &mut self.drag {
                    Drag::Trim { end, orig, anchor_x } => {
                        let t = *orig + (pos.x - *anchor_x) as f64 / self.pps;
                        // At least 10 ms of sound; never outside the file.
                        if *end {
                            a.end = t.clamp(a.start + 0.01, file_secs);
                        } else {
                            a.start = t.clamp(0.0, a.end - 0.01);
                        }
                    }
                    Drag::Fade { end, orig, anchor_x } => {
                        let dx = (pos.x - *anchor_x) as f64 / self.pps;
                        let d = if *end == End::In { dx } else { -dx };
                        let len = (*orig as f64 + d).clamp(0.0, a.length()) as f32;
                        match end {
                            End::In => a.fade_in.len = len,
                            End::Out => a.fade_out.len = len,
                        }
                    }
                    Drag::Tension { end, orig, anchor_y } => {
                        let t = waveform::drag_tension(*orig, pos.y - *anchor_y);
                        match end {
                            End::In => a.fade_in.tension = t,
                            End::Out => a.fade_out.tension = t,
                        }
                    }
                    Drag::Seek => {
                        if let Some(tick) = song_tick(p, id, v.secs(pos.x), playhead, spt) {
                            out.push(Action::SongMode(true));
                            out.push(Action::Seek(tick));
                        }
                    }
                    Drag::Hand { last } => {
                        self.scroll -= (pos.x - last.x) as f64 / self.pps;
                        *last = pos;
                    }
                    Drag::Slide { .. } | Drag::None => {}
                }
            }
        } else {
            self.drag = Drag::None;
        }

        // Cursor feedback
        let hv = match self.drag {
            Drag::None => pos.filter(|_| hovering).and_then(|pos| hover(&v, &p.audio[ai], window, pos)),
            Drag::Slide { .. } => Some(Hover::Window),
            Drag::Trim { end, .. } => Some(Hover::Trim(end)),
            Drag::Fade { end, .. } => Some(Hover::Fade(end)),
            Drag::Tension { end, .. } => Some(Hover::Curve(end)),
            _ => None,
        };
        match (hv, &self.drag) {
            (Some(Hover::Window), Drag::Slide { .. }) => ui.ctx().set_cursor_icon(CursorIcon::Grabbing),
            (Some(Hover::Window), _) => ui.ctx().set_cursor_icon(CursorIcon::Grab),
            (Some(Hover::Curve(_)), _) => ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical),
            (Some(_), _) => ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal),
            (None, Drag::Hand { .. }) => ui.ctx().set_cursor_icon(CursorIcon::Grabbing),
            _ => {}
        }

        // ── Paint ──
        let painter = ui.painter_at(canvas);
        let a = &p.audio[ai];
        let light = theme::channel_light(a.color);
        painter.rect_filled(wave, cr(0), theme::ROLL_BLACK_ROW);
        let g = geo(&v, a);
        let trim = Rect::from_min_max(pos2(g.x0, wave.top()), pos2(g.x_end, wave.bottom()));
        painter.rect_filled(trim.intersect(wave), cr(0), theme::mix(theme::PANEL, light.low, 0.24));
        painter.hline(wave.x_range(), wave.center().y, stroke(1.0, Color32::from_black_alpha(80)));
        let sr = peaks.sample_rate() as f64;
        let gain = lyra_plugin::db_to_gain(a.gain);
        let column = |x: f32| (v.secs(x) * sr, v.secs(x + 1.0) * sr);
        // The whole file dim, then the part that plays, shaped by gain and fades.
        paint_wave(&painter, wave, &peaks, theme::TEXT_DIM.gamma_multiply(0.45), self.vzoom, |x| {
            let (f0, f1) = column(x);
            Some((f0, f1, gain))
        });
        let inside = painter.with_clip_rect(trim.intersect(wave));
        paint_wave(&inside, wave, &peaks, light.hi, self.vzoom, |x| {
            let (f0, f1) = column(x);
            let mut g2 = gain;
            if x < g.x_in {
                g2 *= waveform::fade_gain(End::In, x, g.x0, g.x_in, a.fade_in.tension);
            }
            if x > g.x_out {
                g2 *= waveform::fade_gain(End::Out, x, g.x_out, g.x_end, a.fade_out.tension);
            }
            Some((f0, f1, g2))
        });
        for (end, f0, f1, fade, handle) in
            [(End::In, g.x0, g.x_in, a.fade_in, g.x_in), (End::Out, g.x_out, g.x_end, a.fade_out, g.x_out)]
        {
            if f1 - f0 >= 2.0 {
                let mid = waveform::paint_fade(&inside, wave, end, f0, f1, fade.tension, theme::TEXT, true);
                if f1 - f0 >= 16.0 {
                    waveform::paint_tension(&painter, mid, light.hi, hv == Some(Hover::Curve(end)));
                }
            }
            waveform::paint_handle(&painter, pos2(handle, g.handles_y), light.hi, hv == Some(Hover::Fade(end)));
        }
        for (x, end) in [(g.x0, false), (g.x_end, true)] {
            let hot = hv == Some(Hover::Trim(end));
            painter.vline(
                x,
                wave.y_range(),
                stroke(if hot { 2.0 } else { 1.5 }, if hot { theme::TEXT } else { theme::TEXT.gamma_multiply(0.7) }),
            );
        }
        // This clip's window: a frame over the part of the audio it plays,
        // with its own fades drawn inside.
        if let (Some((ws, we)), Some(ci)) = (window, clip) {
            let c = &p.clips[ci];
            let (x0, x1) = (v.x(ws), v.x(we));
            let frame = Rect::from_min_max(pos2(x0, wave.top() + 1.0), pos2(x1, wave.bottom() - 1.0));
            let hot = hv == Some(Hover::Window);
            painter.rect_filled(frame, cr(4), light.mid.gamma_multiply(if hot { 0.12 } else { 0.07 }));
            painter.rect_stroke(
                frame,
                cr(4),
                stroke(1.5, light.hi.gamma_multiply(if hot { 1.0 } else { 0.7 })),
                StrokeKind::Inside,
            );
            let px = |secs: f32| secs as f64 * self.pps;
            let fi = (x0 + px(c.fade_in.len) as f32).min(x1);
            let fo = (x1 - px(c.fade_out.len) as f32).max(x0);
            let ink = light.hi.gamma_multiply(0.6);
            if fi - x0 >= 3.0 {
                waveform::paint_fade(&painter, frame, End::In, x0, fi, c.fade_in.tension, ink, false);
            }
            if x1 - fo >= 3.0 {
                waveform::paint_fade(&painter, frame, End::Out, fo, x1, c.fade_out.tension, ink, false);
            }
        }
        paint_seconds(&painter, &v, ruler, file_secs);
        // Where the song is in this clip, while a copy of it plays.
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

/// The song position that plays file second `secs` (in the copy playing now,
/// or the first), if that copy reaches it.
fn song_tick(p: &Project, id: AudioId, secs: f64, playhead: Option<f64>, spt: f64) -> Option<f64> {
    let c = clip_for(p, id, playhead)?;
    let a = p.audio(id)?;
    let tick = c.start as f64 + (secs - a.start) / spt - c.offset as f64;
    (c.start as f64..c.end() as f64).contains(&tick).then_some(tick)
}
