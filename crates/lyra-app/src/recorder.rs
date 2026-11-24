//! The Recorder: a scratch tape.
//!
//! One tape for the whole app, in memory. **Record** runs it, **Pause**
//! stops it (Record again carries on at the end), **Clear** throws it away.
//! Nothing is a file until you keep it: *Send to arranger* writes it as a
//! WAV and places a clip, *Append to clip* adds it to the end of the audio
//! clip that's open (as a new version of its file). Either one *moves* the
//! take: the tape is empty again afterwards, ready for the next. The tape and the song
//! know nothing about each other: play the song or don't, either way.
//!
//! **Wait for sound**: Record arms the tape, and it starts by itself when
//! the input gets louder than the threshold (the line on the meter). The
//! last [`PREROLL`] seconds before that come along, so the first soft
//! consonant isn't cut off.
//!
//! **Dry.** The tape keeps the raw input, untouched.
//!
//! **How the samples travel.** The input device's callback only pushes
//! samples into a lock-free FIFO. A small worker thread empties it every
//! few milliseconds onto the tape (behind a mutex the UI locks briefly to
//! draw), so nothing is lost while the window is hidden and the UI isn't
//! drawing. The input is open only while the Recorder tab is on screen or
//! the tape is running.
//!
//! **Safety copy.** Once a minute of recording has piled up, the worker
//! appends that minute to `$XDG_DATA_HOME/lyra/tape.wav` (one small write a
//! minute, never per press). Clear deletes it. If Lyra stops without
//! clearing, the next start puts that tape back.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

/// Seconds kept from before "wait for sound" fires.
pub const PREROLL: f32 = 0.3;
/// Frames summarised by one waveform column.
pub const PEAK_FRAMES: usize = 256;
/// Frames of new tape before the worker appends them to the safety file.
const SAFETY_SECONDS: usize = 60;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// Not taking anything (the tape may hold a paused recording).
    Stopped,
    /// Armed: starts by itself at the first sound over the threshold.
    Waiting,
    Recording,
}

/// The recording: stereo (a mono input fills both sides the same).
#[derive(Default)]
pub struct Tape {
    pub rate: u32,
    /// Whether the input had more than one channel (a mono input is written mono).
    pub stereo: bool,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    /// (lowest, highest) of every [`PEAK_FRAMES`] frames, both sides together.
    pub peaks: Vec<(f32, f32)>,
}

impl Tape {
    pub fn frames(&self) -> usize {
        self.left.len()
    }

    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / self.rate.max(1) as f64
    }

    fn push(&mut self, l: f32, r: f32) {
        if self.left.len().is_multiple_of(PEAK_FRAMES) {
            self.peaks.push((f32::MAX, f32::MIN));
        }
        self.left.push(l);
        self.right.push(r);
        let p = self.peaks.last_mut().expect("pushed above");
        *p = (p.0.min(l.min(r)), p.1.max(l.max(r)));
    }

    fn clear(&mut self) {
        self.left.clear();
        self.right.clear();
        self.peaks.clear();
    }

    /// The tape as a sample, to play or to write.
    pub fn sample(&self) -> lyra_plugin::SampleData {
        lyra_plugin::SampleData { sample_rate: self.rate as f32, left: self.left.clone(), right: self.right.clone() }
    }
}

/// What the worker and the UI share.
pub struct Shared {
    pub tape: Tape,
    pub state: State,
    pub wait_for_sound: bool,
    /// The trigger level for "wait for sound", as a linear peak (0–1).
    pub threshold: f32,
    /// The input's peak level lately (decays), for the meter.
    pub level: f32,
    /// The last [`PREROLL`] seconds of input while waiting.
    preroll: VecDeque<(f32, f32)>,
    /// Frames of the tape already in the safety file.
    safe: usize,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            tape: Tape::default(),
            state: State::Stopped,
            wait_for_sound: false,
            threshold: 0.06, // about -24 dBFS: a voice, not the room
            level: 0.0,
            preroll: VecDeque::new(),
            safe: 0,
        }
    }
}

impl Shared {
    /// Take a chunk of input (interleaved, `channels` wide, at `rate`).
    /// This is the whole state machine; it runs on the worker thread.
    pub fn take(&mut self, chunk: &[f32], channels: usize, rate: u32) {
        let ch = channels.max(1);
        if self.tape.frames() == 0 {
            (self.tape.rate, self.tape.stereo) = (rate, ch > 1);
        }
        let mut peak = 0.0f32;
        let keep = (PREROLL * rate as f32) as usize;
        for frame in chunk.chunks_exact(ch) {
            let (l, r) = (frame[0], if ch > 1 { frame[1] } else { frame[0] });
            peak = peak.max(l.abs()).max(r.abs());
            match self.state {
                State::Stopped => {}
                State::Waiting => {
                    if l.abs().max(r.abs()) >= self.threshold {
                        // Here it starts: the pre-roll first, then this frame on.
                        self.state = State::Recording;
                        for (pl, pr) in self.preroll.drain(..) {
                            self.tape.push(pl, pr);
                        }
                        self.tape.push(l, r);
                    } else {
                        self.preroll.push_back((l, r));
                        if self.preroll.len() > keep {
                            self.preroll.pop_front();
                        }
                    }
                }
                State::Recording => self.tape.push(l, r),
            }
        }
        // The meter falls back slowly, so short peaks stay readable.
        self.level = peak.max(self.level * 0.9);
    }

    pub fn record(&mut self) {
        self.preroll.clear();
        self.state = if self.wait_for_sound { State::Waiting } else { State::Recording };
    }

    pub fn pause(&mut self) {
        self.state = State::Stopped;
    }

    pub fn clear(&mut self) {
        self.tape.clear();
        self.preroll.clear();
        self.state = State::Stopped;
        self.safe = 0;
        let _ = std::fs::remove_file(safety_path());
    }
}

/// The open input device and the worker emptying it.
struct Input {
    _stream: cpal::Stream,
    name: String,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Input {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

pub struct Recorder {
    pub shared: Arc<Mutex<Shared>>,
    /// The input device picked (None = the system's default).
    pub device: Option<String>,
    input: Option<Input>,
    /// Why the input couldn't open, if it couldn't.
    pub error: Option<String>,
    /// The start of a kept recording's file name ("Recording 2026-11-23 21.04").
    pub prefix: String,
    /// What the user asked for this frame (the app does the file work).
    pub ask: Option<Ask>,
    /// The input devices, listed once (asking the system is slow).
    device_list: Option<Vec<String>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ask {
    /// Write the tape and place it on the arrangement.
    SendToArranger,
    /// Add the tape to the end of the open audio clip.
    Append,
}

impl Default for Recorder {
    fn default() -> Self {
        Self {
            shared: Arc::default(),
            device: None,
            input: None,
            error: None,
            prefix: "Recording".to_string(),
            ask: None,
            device_list: None,
        }
    }
}

impl Recorder {
    /// Once per frame: open the input while the tab is shown or the tape is
    /// running, close it otherwise (the microphone is never open for nothing).
    pub fn poll(&mut self, tab_shown: bool) {
        let running = self.shared.lock().is_ok_and(|s| s.state != State::Stopped);
        let want = tab_shown || running;
        let name = self.device.clone().unwrap_or_default();
        if !want || self.input.as_ref().is_some_and(|i| i.name != name) {
            self.input = None; // closes the stream and stops the worker
        }
        if want && self.input.is_none() && self.error.is_none() {
            match open(self.device.as_deref(), self.shared.clone()) {
                Ok(input) => self.input = Some(input),
                Err(e) => {
                    self.error = Some(e);
                    self.shared.lock().unwrap_or_else(|e| e.into_inner()).pause();
                }
            }
        }
    }

    /// Pick another input device (only while the tape is empty: one tape, one rate).
    pub fn set_device(&mut self, name: Option<String>) {
        self.device = name;
        self.error = None;
    }

    /// The input devices there are, by name.
    pub fn devices() -> Vec<String> {
        cpal::default_host().input_devices().map(|d| d.filter_map(|d| d.name().ok()).collect()).unwrap_or_default()
    }

    /// At start: a tape left over from last time (Lyra stopped without Clear).
    pub fn recover(&mut self) -> Option<f64> {
        let data = crate::samples::decode(&safety_path()).ok()?;
        let mut s = self.shared.lock().ok()?;
        s.tape.rate = data.sample_rate as u32;
        s.tape.stereo = data.left != data.right;
        for (l, r) in data.left.iter().zip(&data.right) {
            s.tape.push(*l, *r);
        }
        s.safe = s.tape.frames();
        Some(s.tape.seconds())
    }
}

/// Where the safety copy lives.
fn safety_path() -> PathBuf {
    crate::files::data_dir().join("tape.wav")
}

/// Appends the tape's newest whole minute(s) to the safety file. Worker thread.
fn keep_safe(s: &mut Shared) {
    let rate = s.tape.rate.max(1) as usize;
    if s.tape.frames() - s.safe < SAFETY_SECONDS * rate {
        return;
    }
    let path = safety_path();
    let spec = spec(&s.tape);
    let writer = if s.safe > 0 && path.exists() {
        hound::WavWriter::append(&path)
    } else {
        let _ = std::fs::create_dir_all(crate::files::data_dir());
        hound::WavWriter::create(&path, spec)
    };
    let Ok(mut w) = writer else { return };
    let (from, to) = (s.safe, s.tape.frames());
    let ok = write_frames(&mut w, &s.tape, from, to).is_ok() && w.finalize().is_ok();
    if ok {
        s.safe = to;
    }
}

fn spec(t: &Tape) -> hound::WavSpec {
    hound::WavSpec {
        channels: if t.stereo { 2 } else { 1 },
        sample_rate: t.rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    }
}

fn write_frames<W: std::io::Write + std::io::Seek>(
    w: &mut hound::WavWriter<W>,
    t: &Tape,
    from: usize,
    to: usize,
) -> Result<(), hound::Error> {
    for i in from..to {
        w.write_sample(t.left[i])?;
        if t.stereo {
            w.write_sample(t.right[i])?;
        }
    }
    Ok(())
}

/// `old` with `tape` after it, at `old`'s sample rate (the tape is
/// resampled, linearly, if the two differ).
pub fn append(old: &lyra_plugin::SampleData, tape: &lyra_plugin::SampleData) -> lyra_plugin::SampleData {
    let ratio = tape.sample_rate as f64 / old.sample_rate.max(1.0) as f64;
    let frames = (tape.left.len() as f64 / ratio).floor() as usize;
    let at = |side: &[f32], i: usize| {
        let x = i as f64 * ratio;
        let k = x as usize;
        let f = (x - k as f64) as f32;
        let a = side.get(k).copied().unwrap_or(0.0);
        a + (side.get(k + 1).copied().unwrap_or(a) - a) * f
    };
    let mut out =
        lyra_plugin::SampleData { sample_rate: old.sample_rate, left: old.left.clone(), right: old.right.clone() };
    out.left.extend((0..frames).map(|i| at(&tape.left, i)));
    out.right.extend((0..frames).map(|i| at(&tape.right, i)));
    out
}

/// Writes `data` as a 32-bit float WAV (mono when both sides are the same).
pub fn write_wav(path: &std::path::Path, data: &lyra_plugin::SampleData) -> Result<(), String> {
    let tape = Tape {
        rate: data.sample_rate as u32,
        stereo: data.left != data.right,
        left: data.left.clone(),
        right: data.right.clone(),
        peaks: Vec::new(),
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    }
    let mut w =
        hound::WavWriter::create(path, spec(&tape)).map_err(|e| format!("can't write {}: {e}", path.display()))?;
    write_frames(&mut w, &tape, 0, tape.frames()).and_then(|_| w.finalize()).map_err(|e| e.to_string())
}

/// Opens an input device and starts the worker that fills the tape.
fn open(name: Option<&str>, shared: Arc<Mutex<Shared>>) -> Result<Input, String> {
    let host = cpal::default_host();
    let device = match name {
        Some(n) => host
            .input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().ok().as_deref() == Some(n))
            .ok_or_else(|| format!("the input \"{n}\" isn't there"))?,
        None => host.default_input_device().ok_or("no input device (microphone) found")?,
    };
    let config = device.default_input_config().map_err(|e| format!("input: {e}"))?;
    let (channels, rate) = (config.channels() as usize, config.sample_rate().0);
    // Ten seconds of room: the worker empties it every few milliseconds.
    let (tx, mut rx) = rtrb::RingBuffer::<f32>::new(rate as usize * channels * 10);
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config.into(), tx),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config.into(), tx),
        cpal::SampleFormat::I32 => build::<i32>(&device, &config.into(), tx),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config.into(), tx),
        other => Err(format!("input sample format {other:?} isn't supported")),
    }?;
    stream.play().map_err(|e| format!("input: {e}"))?;
    let stop = Arc::new(AtomicBool::new(false));
    let quit = stop.clone();
    let worker = std::thread::Builder::new()
        .name("lyra-recorder".into())
        .spawn(move || {
            let mut chunk = Vec::with_capacity(rate as usize * channels);
            while !quit.load(Relaxed) {
                chunk.clear();
                while let Ok(s) = rx.pop() {
                    chunk.push(s);
                }
                if let Ok(mut s) = shared.lock() {
                    s.take(&chunk, channels, rate);
                    keep_safe(&mut s);
                }
                std::thread::sleep(std::time::Duration::from_millis(8));
            }
        })
        .map_err(|e| e.to_string())?;
    let name = name.unwrap_or_default().to_string();
    Ok(Input { _stream: stream, name, stop, worker: Some(worker) })
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut tx: rtrb::Producer<f32>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    device
        .build_input_stream(
            config,
            // The input callback: only pushes (a full FIFO drops, never blocks).
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                for &s in data {
                    let _ = tx.push(<f32 as cpal::FromSample<T>>::from_sample_(s));
                }
            },
            |e| eprintln!("lyra: input: {e}"),
            None,
        )
        .map_err(|e| format!("input: {e}"))
}

// ── The Recorder tab ──

use crate::flex::{self, Grow, Px};
use crate::theme;
use crate::widgets::{self, Icon, Legend};
use egui::{vec2, Align, Align2, FontId, Layout, Rect, Sense, Ui, UiBuilder};

/// A child ui laid out left to right inside `r`.
fn inside<R>(ui: &mut Ui, r: Rect, salt: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child =
        ui.new_child(UiBuilder::new().id_salt(salt).max_rect(r).layout(Layout::left_to_right(Align::Center)));
    add(&mut child)
}

/// "1:04.3"
fn clock(secs: f64) -> String {
    format!("{}:{:04.1}", (secs / 60.0) as u32, secs % 60.0)
}

/// The Recorder tab. `can_append`: an audio clip is open to append to.
pub fn show(ui: &mut Ui, rec: &mut Recorder, can_append: bool) {
    let m = theme::m();
    let size = ui.available_size().max(vec2(420.0, 160.0));
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let key = (m.step_h * 1.6).round();
    let [top, meter_r, wave_r, bottom] = flex::col(rect, [Px(key), Px(12.0), Grow(1.0), Px(key)], 6.0);
    let shared = rec.shared.clone();
    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
    let running = s.state != State::Stopped;
    let has_tape = s.tape.frames() > 0;
    if running {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
    }

    // ── Top: record / pause, clear, where we are · wait for sound, the input ──
    let [rec_r, clear_r, status_r, wait_r, dev_r] =
        flex::row(top, [Px(key * 1.6), Px(key * 2.0), Grow(1.0), Px(key * 3.6), Px(240.0)], 6.0);
    inside(ui, rec_r, "rec", |ui| {
        let (icon, hint) = if running {
            (Icon::Pause, "Pause (Record again carries on at the end)")
        } else {
            (Icon::Record, "Record (carries on at the end of the tape)")
        };
        if widgets::key(ui, rec_r.size(), Legend::Icon(icon), running, theme::L_PINK, hint).clicked() {
            if running {
                s.pause();
            } else {
                s.record();
            }
        }
    });
    inside(ui, clear_r, "clear", |ui| {
        if widgets::key(ui, clear_r.size(), Legend::Text("CLEAR"), false, theme::L_MINT, "Throw the tape away")
            .clicked()
        {
            s.clear();
        }
    });
    let status = match s.state {
        State::Recording => "recording".to_string(),
        State::Waiting => "waiting for a sound…".to_string(),
        State::Stopped if has_tape => "paused".to_string(),
        State::Stopped => "empty".to_string(),
    };
    let ink = if running { theme::ROSE } else { theme::TEXT_DIM };
    ui.painter().text(
        status_r.left_center() + vec2(6.0, 0.0),
        Align2::LEFT_CENTER,
        format!("{}   {status}", clock(s.tape.seconds())),
        FontId::monospace(m.text_small * 1.3),
        ink,
    );
    inside(ui, wait_r, "wait", |ui| {
        let hint = "Record waits until the input is louder than the line on the meter, then starts \
                    (with the moment just before, so the first sound isn't cut off)";
        if widgets::key(ui, wait_r.size(), Legend::Text("WAIT FOR SOUND"), s.wait_for_sound, theme::L_AMBER, hint)
            .clicked()
        {
            s.wait_for_sound ^= true;
            if s.state == State::Waiting && !s.wait_for_sound {
                s.state = State::Recording;
            }
        }
    });
    inside(ui, dev_r, "device", |ui| {
        let shown = rec.device.clone().unwrap_or_else(|| "Default input".to_string());
        ui.add_enabled_ui(!has_tape && !running, |ui| {
            egui::ComboBox::from_id_salt("recorder-device").width(dev_r.width() - 8.0).selected_text(shown).show_ui(
                ui,
                |ui| {
                    if ui.selectable_label(rec.device.is_none(), "Default input").clicked() {
                        rec.set_device(None);
                    }
                    for name in rec.device_list.get_or_insert_with(Recorder::devices).clone() {
                        if ui.selectable_label(rec.device.as_deref() == Some(name.as_str()), &name).clicked() {
                            rec.set_device(Some(name));
                        }
                    }
                },
            );
        })
        .response
        .on_hover_text("Where the tape listens (clear the tape to pick another)");
    });

    // ── The input meter, and the "wait for sound" line on it (drag it) ──
    widgets::meter_bar(ui.painter(), meter_r, widgets::meter_fraction(s.level), false);
    if s.wait_for_sound {
        let x = meter_r.left() + widgets::meter_fraction(s.threshold) * meter_r.width();
        let resp = ui.interact(meter_r.expand(4.0), ui.id().with("threshold"), Sense::drag());
        if let Some(pos) = resp.interact_pointer_pos().filter(|_| resp.dragged()) {
            let frac = ((pos.x - meter_r.left()) / meter_r.width()).clamp(0.02, 1.0);
            s.threshold = lyra_plugin::db_to_gain(frac * 60.0 - 60.0);
        }
        ui.painter().vline(x, meter_r.expand(3.0).y_range(), theme::stroke(2.0, theme::GOLD));
        resp.on_hover_text(format!("Starts above {:.0} dB: drag", lyra_plugin::gain_to_db(s.threshold)));
    }
    if let Some(e) = &rec.error {
        ui.painter().text(meter_r.center(), Align2::CENTER_CENTER, e, FontId::proportional(m.text_small), theme::ROSE);
    }

    // ── The tape, growing from the left (at least ten seconds wide) ──
    theme::well(ui.painter(), wave_r);
    let inner = wave_r.shrink(4.0);
    if !has_tape {
        ui.painter().text(
            inner.center(),
            Align2::CENTER_CENTER,
            "A scratch tape: record, then send it to the arranger or append it to a clip. Clear throws it away.",
            FontId::proportional(m.text_small * 1.1),
            theme::TEXT_DIM,
        );
    } else {
        let peaks = &s.tape.peaks;
        let span = (s.tape.frames() as f64).max(10.0 * s.tape.rate as f64) / PEAK_FRAMES as f64;
        let per_px = span / inner.width() as f64;
        let mid = inner.center().y;
        let half = inner.height() * 0.5;
        // The length of the tape as a line, so even a quiet take shows how long it is.
        let end = inner.left() + ((s.tape.frames() as f64 / PEAK_FRAMES as f64) / per_px) as f32;
        ui.painter().hline(inner.left()..=end.min(inner.right()), mid, theme::stroke(1.0, theme::MINT_DEEP));
        let mut x = 0.0f32;
        while x < inner.width() {
            let a = (x as f64 * per_px) as usize;
            let b = (((x + 1.0) as f64 * per_px) as usize).max(a + 1).min(peaks.len());
            if a >= peaks.len() {
                break;
            }
            let (lo, hi) = peaks[a..b].iter().fold((0.0f32, 0.0f32), |(lo, hi), p| (lo.min(p.0), hi.max(p.1)));
            let px = inner.left() + x;
            ui.painter().vline(
                px,
                egui::Rangef::new(mid - hi.min(1.0) * half, mid - lo.max(-1.0) * half + 1.0),
                theme::stroke(1.0, theme::MINT),
            );
            x += 1.0;
        }
    }

    // ── Bottom: the name a kept take gets · append · send ──
    let [name_label, name_r, _, append_r, send_r] =
        flex::row(bottom, [Px(46.0), Px(220.0), Grow(1.0), Px(key * 4.2), Px(key * 4.8)], 6.0);
    ui.painter().text(
        name_label.right_center(),
        Align2::RIGHT_CENTER,
        "name",
        FontId::proportional(m.text_small),
        theme::TEXT_DIM,
    );
    inside(ui, name_r, "prefix", |ui| {
        ui.add(egui::TextEdit::singleline(&mut rec.prefix).desired_width(name_r.width() - 8.0))
            .on_hover_text("A kept take is called this, plus the date and time");
    });
    drop(s);
    let usable = has_tape && !running;
    inside(ui, append_r, "append", |ui| {
        let hint = if can_append {
            "Add the tape to the end of the audio clip that's open (as a new version of its file; undo goes back)"
        } else {
            "Open an audio clip first (Editor tab) to append the tape to it"
        };
        if widgets::key(ui, append_r.size(), Legend::Text("APPEND TO CLIP"), false, theme::L_MINT, hint).clicked()
            && usable
            && can_append
        {
            rec.ask = Some(Ask::Append);
        }
    });
    inside(ui, send_r, "send", |ui| {
        let hint = "Write the tape as a file and place it on the arrangement, at the playhead";
        if widgets::key(ui, send_r.size(), Legend::Text("SEND TO ARRANGER"), false, theme::L_MINT, hint).clicked()
            && usable
        {
            rec.ask = Some(Ask::SendToArranger);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_pause_record_carries_on() {
        let mut s = Shared::default();
        s.record();
        s.take(&[0.1, 0.2, 0.3], 1, 1000);
        s.pause();
        s.take(&[0.9, 0.9], 1, 1000); // not taken while paused
        s.record();
        s.take(&[0.4], 1, 1000);
        assert_eq!(s.tape.left, vec![0.1, 0.2, 0.3, 0.4]);
        assert!(!s.tape.stereo);
    }

    #[test]
    fn waiting_starts_at_the_first_sound_with_the_preroll() {
        let mut s = Shared { wait_for_sound: true, threshold: 0.5, ..Default::default() };
        s.record();
        assert_eq!(s.state, State::Waiting);
        // 1000 frames a second: the pre-roll keeps 300 of the quiet ones.
        let quiet: Vec<f32> = (0..1000).map(|i| i as f32 * 1e-5).collect();
        s.take(&quiet, 1, 1000);
        assert_eq!(s.tape.frames(), 0, "nothing yet");
        s.take(&[0.8, 0.2], 1, 1000);
        assert_eq!(s.state, State::Recording);
        assert_eq!(s.tape.frames(), 300 + 2, "the pre-roll, then the sound");
        assert_eq!(s.tape.left[300], 0.8);
        assert!((s.tape.left[0] - 700.0 * 1e-5).abs() < 1e-7, "the pre-roll is the latest 0.3 s");
    }

    #[test]
    fn appending_matches_the_clip_rate() {
        let data = |rate: f32, n: usize, v: f32| lyra_plugin::SampleData {
            sample_rate: rate,
            left: vec![v; n],
            right: vec![v; n],
        };
        let joined = append(&data(48_000.0, 100, 0.1), &data(24_000.0, 50, 0.5));
        assert_eq!(joined.sample_rate, 48_000.0);
        assert_eq!(joined.left.len(), 200, "half the rate: twice the frames");
        assert_eq!((joined.left[99], joined.left[150]), (0.1, 0.5));
    }

    #[test]
    fn stereo_input_and_peaks() {
        let mut s = Shared::default();
        s.record();
        let frames: Vec<f32> = (0..600).flat_map(|i| [i as f32 / 600.0, -0.5]).collect();
        s.take(&frames, 2, 48_000);
        assert!(s.tape.stereo);
        assert_eq!(s.tape.frames(), 600);
        assert_eq!(s.tape.peaks.len(), 3, "a column per 256 frames");
        assert_eq!(s.tape.peaks[0].0, -0.5);
    }
}
