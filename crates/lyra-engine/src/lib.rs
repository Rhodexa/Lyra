//! # lyra-engine
//!
//! The real-time core. Mental model, for anyone coming from embedded:
//!
//! ```text
//!   UI thread ("main loop")                     audio thread ("ISR")
//!   ───────────────────────                     ────────────────────
//!   EngineHandle ── Command FIFO ─────────────▶ Engine::process()
//!        ▲                                          │
//!        ├──────── Trash FIFO ◀────────────────────┤  old data to free
//!        └──────── Shared atomics ◀─────────────────┘  playhead, meters
//! ```
//!
//! * The audio device calls [`Engine::process`] every few milliseconds and
//!   wants a buffer back *on time, every time*. So it never allocates, never
//!   locks, never waits.
//! * The UI never touches the engine directly. It **sends commands** through a
//!   lock-free ring buffer. Anything big (a new sequence, a new channel) is
//!   allocated on the UI side and sent as a `Box`.
//! * When the engine replaces something, the old `Box` goes back through the
//!   **trash** FIFO, so the *UI thread* frees it — freeing memory is not
//!   real-time safe either.
//! * Small read-only facts (playhead, meter levels, "this channel just
//!   triggered") come back as plain atomics.
//!
//! That's the whole threading model: two threads, three one-way pipes.

use lyra_plugin::{Event, Plugin, ProcessContext, SampleData, TimedEvent, Transport};
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

/// Ticks per quarter note. All musical time is in ticks.
pub const PPQ: u32 = 96;
/// A step = one 16th note.
pub const TICKS_PER_STEP: u32 = PPQ / 4;
pub const MAX_CHANNELS: usize = 128;
/// Mixer tracks: 0 = master, 1..=16 = inserts that channels route into.
pub const MIXER_TRACKS: usize = 17;
/// Effect slots per mixer track.
pub const MAX_FX: usize = 8;
/// The engine renders in slices of at most this many frames.
pub const BLOCK: usize = 256;
const MAX_EVENTS_PER_BLOCK: usize = 256;
const MAX_PENDING_LIVE: usize = 64;
/// Sequenced notes that can sound at once on one channel (chords + tails).
const MAX_HELD: usize = 64;
/// Gain smoothing per sample (avoids zipper noise when a fader moves).
const SMOOTH: f32 = 0.002;

// ─────────────────────────── data sent to the engine ───────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeqNote {
    pub start: u32,
    pub len: u32,
    /// Index into the engine's channel list.
    pub channel: u16,
    pub key: u8,
    pub velocity: f32,
}

/// A compiled, read-only loop of notes the engine plays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sequence {
    /// Loop length in ticks (0 = nothing to play).
    pub length: u32,
    pub notes: Vec<SeqNote>,
}

/// A stretch of audio on the song timeline (an audio clip on the playlist).
///
/// Played straight from the song position, not triggered: start the song
/// halfway through a clip (or seek into it) and you hear it from there.
#[derive(Clone, Debug)]
pub struct AudioRegion {
    /// Where it sits in the song, in ticks.
    pub start: f64,
    pub end: f64,
    pub data: Arc<SampleData>,
    /// Seconds into the file at `start`, and the point in the file where it
    /// must stop (the end of the audio clip's trim).
    pub from: f64,
    pub until: f64,
    /// Linear gain.
    pub gain: f32,
    /// (seconds, tension) at each end of the region; see [`fade_curve`].
    pub fade_in: (f32, f32),
    pub fade_out: (f32, f32),
    /// Mixer track it plays into.
    pub track: usize,
}

impl PartialEq for AudioRegion {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.data, &o.data)
            && (self.start, self.end, self.from, self.until) == (o.start, o.end, o.from, o.until)
            && (self.gain, self.fade_in, self.fade_out, self.track) == (o.gain, o.fade_in, o.fade_out, o.track)
    }
}

/// Every audio region in the song (empty when a pattern loops on its own).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioLane {
    pub regions: Vec<AudioRegion>,
}

/// A fade's shape: `x` (0..1, how far towards full volume) to gain (0..1).
/// `tension` 0 is a straight line; above 0 the curve bows down (quieter
/// through the fade), below 0 it bows up (louder). A fade-out uses the same
/// curve read backwards, so the same tension looks the same at both ends.
pub fn fade_curve(x: f32, tension: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let c = tension.clamp(-1.0, 1.0) * 6.0;
    if c.abs() < 1e-3 {
        x
    } else {
        (c * x).exp_m1() / c.exp_m1()
    }
}

impl AudioRegion {
    /// Mix this region's part of a block into `out_l`/`out_r`. `pos` is the
    /// song tick at the first sample, the song loops every `len` ticks.
    /// REAL-TIME: no allocation.
    #[allow(clippy::too_many_arguments)]
    fn render(
        &self,
        pos: f64,
        ticks_per_sample: f64,
        len: f64,
        seconds_per_tick: f64,
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) {
        let n = out_l.len();
        let lo = pos;
        let hi = pos + ticks_per_sample * n as f64;
        let overlaps = |a: f64, b: f64| self.start < b && self.end > a;
        if !(overlaps(lo, hi.min(len)) || (hi > len && overlaps(0.0, hi - len))) {
            return;
        }
        let data = &*self.data;
        let frames = data.left.len().min(data.right.len());
        // How long this region really sounds: the clip's length, or less if
        // the audio runs out first.
        let dur = ((self.end - self.start) * seconds_per_tick).min(self.until - self.from);
        let (fi, fo) = (self.fade_in.0 as f64, self.fade_out.0 as f64);
        for s in 0..n {
            let mut t = pos + s as f64 * ticks_per_sample;
            if t >= len {
                t -= len;
            }
            if t < self.start || t >= self.end {
                continue;
            }
            let el = (t - self.start) * seconds_per_tick;
            if el >= dur {
                continue;
            }
            let mut g = self.gain;
            if el < fi {
                g *= fade_curve((el / fi) as f32, self.fade_in.1);
            }
            let left = dur - el;
            if left < fo {
                g *= fade_curve((left / fo) as f32, self.fade_out.1);
            }
            let at = (self.from + el) * data.sample_rate as f64;
            if at < 0.0 {
                continue;
            }
            let i = at as usize;
            if i + 1 >= frames {
                continue;
            }
            let f = (at - i as f64) as f32;
            out_l[s] += (data.left[i] + (data.left[i + 1] - data.left[i]) * f) * g;
            out_r[s] += (data.right[i] + (data.right[i + 1] - data.right[i]) * f) * g;
        }
    }
}

/// One instrument + its mixer settings, as the engine owns it.
/// Built on the UI thread with [`ChannelSlot::new`], then sent over.
pub struct ChannelSlot {
    plugin: Box<dyn Plugin>,
    events: Vec<TimedEvent>,
    pending: Vec<Event>,
    /// Sequenced notes currently sounding, with ticks left until their note-off.
    /// The engine releases these itself, so deleting a note mid-play (or
    /// swapping the whole sequence) can never leave it hanging.
    held: Vec<Held>,
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    volume: f32,
    pan: f32,
    mute: bool,
    choke: u8,
    /// Mixer track this channel plays into.
    route: usize,
    gain_l: f32,
    gain_r: f32,
}

impl ChannelSlot {
    /// Allocates everything this channel will ever need. UI thread only.
    pub fn new(mut plugin: Box<dyn Plugin>, sample_rate: f32) -> Box<Self> {
        plugin.activate(sample_rate);
        Box::new(Self {
            plugin,
            events: Vec::with_capacity(MAX_EVENTS_PER_BLOCK),
            pending: Vec::with_capacity(MAX_PENDING_LIVE),
            held: Vec::with_capacity(MAX_HELD),
            buf_l: vec![0.0; BLOCK],
            buf_r: vec![0.0; BLOCK],
            volume: 0.8,
            pan: 0.0,
            mute: false,
            choke: 0,
            route: 0,
            gain_l: 0.0,
            gain_r: 0.0,
        })
    }
}

/// One mixer track's effects, in order. Built and activated on the UI thread,
/// then swapped into the engine whole (the old chain comes back as trash).
pub struct FxChain {
    plugins: Vec<Box<dyn Plugin>>,
    bypass: Vec<bool>,
}

impl FxChain {
    /// `plugins` with their bypass flags. Activates each one (UI thread).
    pub fn new(slots: Vec<(Box<dyn Plugin>, bool)>, sample_rate: f32) -> Box<Self> {
        let mut plugins = Vec::with_capacity(MAX_FX);
        let mut bypass = Vec::with_capacity(MAX_FX);
        for (mut p, b) in slots.into_iter().take(MAX_FX) {
            p.activate(sample_rate);
            plugins.push(p);
            bypass.push(b);
        }
        Box::new(Self { plugins, bypass })
    }
}

struct MixerTrack {
    fx: Box<FxChain>,
    volume: f32,
    pan: f32,
    mute: bool,
    gain_l: f32,
    gain_r: f32,
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
}

#[derive(Clone, Copy)]
struct Held {
    key: u8,
    /// Ticks from the start of the *next* block until note-off.
    remaining: f64,
}

pub enum Command {
    /// Start playing from the start marker.
    Play,
    /// Stop and return to the start marker.
    Stop,
    /// Move the playhead (and the start marker) to this tick. Works playing or stopped.
    Seek(f64),
    SetTempo(f64),
    /// 0 = straight, 1 = full triplet shuffle.
    SetSwing(f32),
    SetMasterVolume(f32),
    SetSequence(Box<Sequence>),
    AddChannel(Box<ChannelSlot>),
    ClearChannels,
    SetParam {
        channel: usize,
        id: u32,
        value: f32,
    },
    /// `choke`: channels sharing a non-zero group cut each other off (closed hat kills open hat).
    /// Which mixer track a channel plays into.
    SetRoute {
        channel: usize,
        track: usize,
    },
    SetTrackMix {
        track: usize,
        volume: f32,
        pan: f32,
        mute: bool,
    },
    SetFxChain {
        track: usize,
        chain: Box<FxChain>,
    },
    SetFxParam {
        track: usize,
        slot: usize,
        id: u32,
        value: f32,
    },
    SetFxBypass {
        track: usize,
        slot: usize,
        bypass: bool,
    },
    SetMix {
        channel: usize,
        volume: f32,
        pan: f32,
        mute: bool,
        choke: u8,
    },
    /// Notes played live (typing keyboard, MIDI, clicking a key).
    Live {
        channel: usize,
        event: Event,
    },
    /// Audition something outside the song (a sample clicked in the browser):
    /// this slot plays `key` once, straight into the master, replacing any
    /// preview still sounding.
    Preview {
        slot: Box<ChannelSlot>,
        key: u8,
    },
    /// Fade the preview out.
    StopPreview,
    /// Replace the audio clips the song plays.
    SetAudio(Box<AudioLane>),
}

enum Trash {
    Audio(#[allow(dead_code)] Box<AudioLane>),
    Sequence(#[allow(dead_code)] Box<Sequence>),
    Channel(#[allow(dead_code)] Box<ChannelSlot>),
    Chain(#[allow(dead_code)] Box<FxChain>),
}

// ─────────────────────────── data coming back ──────────────────────────────

/// Facts the audio thread publishes for the UI. Lock-free, read any time.
pub struct Shared {
    playhead: AtomicU64,
    playing: AtomicBool,
    peaks: Vec<AtomicU32>,
    triggers: Vec<AtomicU32>,
    master_peak: [AtomicU32; 2],
    /// Per mixer track: [left, right] peaks, flattened.
    track_peaks: Vec<AtomicU32>,
}

impl Shared {
    fn new() -> Self {
        Self {
            playhead: AtomicU64::new(0f64.to_bits()),
            playing: AtomicBool::new(false),
            peaks: (0..MAX_CHANNELS).map(|_| AtomicU32::new(0)).collect(),
            triggers: (0..MAX_CHANNELS).map(|_| AtomicU32::new(0)).collect(),
            master_peak: [AtomicU32::new(0), AtomicU32::new(0)],
            track_peaks: (0..MIXER_TRACKS * 2).map(|_| AtomicU32::new(0)).collect(),
        }
    }

    /// Current position inside the loop, in ticks.
    pub fn playhead(&self) -> f64 {
        f64::from_bits(self.playhead.load(Relaxed))
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Relaxed)
    }

    /// Loudest sample on a channel since the last call (resets it).
    pub fn take_peak(&self, channel: usize) -> f32 {
        self.peaks.get(channel).map_or(0.0, |a| f32::from_bits(a.swap(0, Relaxed)))
    }

    /// Loudest [left, right] on a mixer track since the last call (resets it).
    pub fn take_track_peak(&self, track: usize) -> [f32; 2] {
        [0, 1].map(|c| self.track_peaks.get(track * 2 + c).map_or(0.0, |a| f32::from_bits(a.swap(0, Relaxed))))
    }

    pub fn take_master_peak(&self) -> [f32; 2] {
        [0, 1].map(|i| f32::from_bits(self.master_peak[i].swap(0, Relaxed)))
    }

    /// Increments every time the channel gets a note-on. Changes = "flash the LED".
    pub fn trigger_count(&self, channel: usize) -> u32 {
        self.triggers.get(channel).map_or(0, |a| a.load(Relaxed))
    }
}

/// Raise an atomic peak meter. Works because positive f32 bit patterns sort like the floats.
fn raise_peak(a: &AtomicU32, v: f32) {
    a.fetch_max(v.to_bits(), Relaxed);
}

// ─────────────────────────── UI-side handle ────────────────────────────────

pub struct EngineHandle {
    tx: Producer<Command>,
    trash: Consumer<Trash>,
    shared: Arc<Shared>,
    sample_rate: f32,
}

impl EngineHandle {
    /// Queue a command. If the FIFO is full (it shouldn't be), the command is dropped.
    pub fn send(&mut self, cmd: Command) {
        let _ = self.tx.push(cmd);
    }

    /// Free whatever the audio thread handed back. Call once per UI frame.
    pub fn collect_garbage(&mut self) {
        while self.trash.pop().is_ok() {}
    }

    pub fn shared(&self) -> &Shared {
        &self.shared
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

// ─────────────────────────── the audio-thread side ─────────────────────────

pub struct Engine {
    sr: f32,
    rx: Consumer<Command>,
    trash: Producer<Trash>,
    shared: Arc<Shared>,

    // Boxed on purpose: channels arrive/leave as one pointer through the FIFOs,
    // so the audio thread only ever moves 8 bytes, never a whole slot.
    #[allow(clippy::vec_box)]
    channels: Vec<Box<ChannelSlot>>,
    seq: Box<Sequence>,
    audio: Box<AudioLane>,

    playing: bool,
    /// Position inside the loop, in ticks.
    pos: f64,
    /// Where Play starts and Stop returns to (set by Seek).
    start_marker: f64,
    tempo: f64,
    swing: f32,
    master: f32,
    master_gain: f32,

    mixer: Vec<MixerTrack>,
    mix_l: Vec<f32>,
    mix_r: Vec<f32>,
    /// What the browser is auditioning (see `Command::Preview`).
    preview: Option<Box<ChannelSlot>>,
    /// (sample offset, choke group, source channel) collected while scheduling.
    chokes: Vec<(u32, u8, usize)>,
}

/// Creates the two halves: the [`Engine`] (move it into the audio callback)
/// and the [`EngineHandle`] (keep it in the UI).
pub fn create(sample_rate: f32) -> (Engine, EngineHandle) {
    let (tx, rx) = RingBuffer::new(1024);
    let (trash_tx, trash_rx) = RingBuffer::new(512);
    let shared = Arc::new(Shared::new());
    let engine = Engine {
        sr: sample_rate,
        rx,
        trash: trash_tx,
        shared: shared.clone(),
        channels: Vec::with_capacity(MAX_CHANNELS),
        seq: Box::default(),
        audio: Box::default(),
        playing: false,
        pos: 0.0,
        start_marker: 0.0,
        tempo: 124.0,
        swing: 0.0,
        master: 0.8,
        master_gain: 0.8,
        mixer: (0..MIXER_TRACKS)
            .map(|_| MixerTrack {
                fx: FxChain::new(Vec::new(), sample_rate),
                volume: 1.0,
                pan: 0.0,
                mute: false,
                gain_l: 0.0,
                gain_r: 0.0,
                buf_l: vec![0.0; BLOCK],
                buf_r: vec![0.0; BLOCK],
            })
            .collect(),
        mix_l: vec![0.0; BLOCK],
        mix_r: vec![0.0; BLOCK],
        preview: None,
        chokes: Vec::with_capacity(64),
    };
    let handle = EngineHandle { tx, trash: trash_rx, shared, sample_rate };
    (engine, handle)
}

impl Engine {
    /// Fill an interleaved output buffer (`channels` = 2 for stereo). REAL-TIME.
    pub fn process(&mut self, out: &mut [f32], channels: usize) {
        self.drain_commands();
        let frames = out.len() / channels.max(1);
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(BLOCK);
            self.render_block(n);
            for i in 0..n {
                let o = (done + i) * channels;
                out[o] = self.mix_l[i];
                if channels > 1 {
                    out[o + 1] = self.mix_r[i];
                    out[o + 2..o + channels].fill(0.0);
                }
            }
            done += n;
        }
        self.shared.playhead.store(self.pos.to_bits(), Relaxed);
        self.shared.playing.store(self.playing, Relaxed);
    }

    /// Throw something away without freeing it here.
    fn discard(&mut self, t: Trash) {
        // If even the trash FIFO is full the value is dropped right here —
        // a rare, tiny real-time hiccup, but never a crash.
        let _ = self.trash.push(t);
    }

    fn drain_commands(&mut self) {
        while let Ok(cmd) = self.rx.pop() {
            match cmd {
                Command::Play => {
                    self.playing = true;
                    self.pos = self.start_marker;
                }
                Command::Seek(tick) => {
                    let len = self.seq.length.max(1) as f64;
                    self.start_marker = tick.clamp(0.0, len - 1.0);
                    self.pos = self.start_marker;
                }
                Command::Stop => {
                    self.playing = false;
                    self.pos = self.start_marker;
                    for ch in &mut self.channels {
                        ch.held.clear();
                        if ch.pending.len() < ch.pending.capacity() {
                            ch.pending.push(Event::AllNotesOff);
                        }
                    }
                }
                Command::SetTempo(t) => self.tempo = t.clamp(20.0, 999.0),
                Command::SetSwing(s) => self.swing = s.clamp(0.0, 1.0),
                Command::SetMasterVolume(v) => self.master = v.max(0.0),
                Command::SetSequence(seq) => {
                    let old = std::mem::replace(&mut self.seq, seq);
                    if self.seq.length > 0 {
                        self.pos %= self.seq.length as f64;
                        self.start_marker %= self.seq.length as f64;
                    }
                    self.discard(Trash::Sequence(old));
                }
                Command::AddChannel(slot) => {
                    if self.channels.len() < MAX_CHANNELS {
                        self.channels.push(slot); // capacity reserved: no allocation
                    } else {
                        self.discard(Trash::Channel(slot));
                    }
                }
                Command::ClearChannels => {
                    while let Some(ch) = self.channels.pop() {
                        self.discard(Trash::Channel(ch));
                    }
                }
                Command::SetParam { channel, id, value } => {
                    if let Some(ch) = self.channels.get_mut(channel) {
                        ch.plugin.set_param(id, value);
                    }
                }
                Command::SetRoute { channel, track } => {
                    if let Some(ch) = self.channels.get_mut(channel) {
                        ch.route = track.min(MIXER_TRACKS - 1);
                    }
                }
                Command::SetTrackMix { track, volume, pan, mute } => {
                    if let Some(t) = self.mixer.get_mut(track) {
                        (t.volume, t.pan, t.mute) = (volume, pan, mute);
                    }
                }
                Command::SetFxChain { track, chain } => match self.mixer.get_mut(track) {
                    Some(t) => {
                        let old = std::mem::replace(&mut t.fx, chain);
                        self.discard(Trash::Chain(old));
                    }
                    None => self.discard(Trash::Chain(chain)),
                },
                Command::SetFxParam { track, slot, id, value } => {
                    if let Some(p) = self.mixer.get_mut(track).and_then(|t| t.fx.plugins.get_mut(slot)) {
                        p.set_param(id, value);
                    }
                }
                Command::SetFxBypass { track, slot, bypass } => {
                    if let Some(b) = self.mixer.get_mut(track).and_then(|t| t.fx.bypass.get_mut(slot)) {
                        *b = bypass;
                    }
                }
                Command::SetMix { channel, volume, pan, mute, choke } => {
                    if let Some(ch) = self.channels.get_mut(channel) {
                        ch.volume = volume;
                        ch.pan = pan;
                        ch.mute = mute;
                        ch.choke = choke;
                    }
                }
                Command::Preview { mut slot, key } => {
                    slot.pending.push(Event::NoteOn { key, velocity: 0.9 }); // capacity reserved
                    if let Some(old) = self.preview.replace(slot) {
                        self.discard(Trash::Channel(old));
                    }
                }
                Command::SetAudio(lane) => {
                    let old = std::mem::replace(&mut self.audio, lane);
                    self.discard(Trash::Audio(old));
                }
                Command::StopPreview => {
                    if let Some(p) = &mut self.preview {
                        if p.pending.len() < p.pending.capacity() {
                            p.pending.push(Event::AllNotesOff);
                        }
                    }
                }
                Command::Live { channel, event } => {
                    if let Some(ch) = self.channels.get_mut(channel) {
                        if ch.pending.len() < ch.pending.capacity() {
                            ch.pending.push(event);
                        }
                        if matches!(event, Event::NoteOn { .. }) {
                            self.shared.triggers[channel].fetch_add(1, Relaxed);
                        }
                    }
                }
            }
        }
    }

    fn render_block(&mut self, n: usize) {
        let transport = Transport { tempo: self.tempo, playing: self.playing, position: self.pos };
        // 1. Gather this block's events: live notes first, then the sequencer.
        for ch in &mut self.channels {
            let ChannelSlot { events, pending, .. } = &mut **ch;
            events.clear();
            for e in pending.drain(..) {
                events.push(TimedEvent { offset: 0, event: e });
            }
        }
        let block_pos = self.pos;
        if self.playing && self.seq.length > 0 {
            self.schedule(n);
        }
        self.apply_chokes();

        // 2. Run every instrument, apply volume/pan, add it into its mixer track.
        for t in &mut self.mixer {
            t.buf_l[..n].fill(0.0);
            t.buf_r[..n].fill(0.0);
        }
        for (i, ch) in self.channels.iter_mut().enumerate() {
            let ChannelSlot { plugin, events, buf_l, buf_r, .. } = &mut **ch;
            // AllNotesOff sorts before a NoteOn at the same sample, so a choke
            // never kills the note that's starting right then. (In-place sort: no allocation.)
            events.sort_unstable_by_key(|e| (e.offset, !matches!(e.event, Event::AllNotesOff)));
            plugin.process(ProcessContext {
                sample_rate: self.sr,
                events,
                out_l: &mut buf_l[..n],
                out_r: &mut buf_r[..n],
            });

            // Constant-power pan law.
            let (target_l, target_r) = if ch.mute {
                (0.0, 0.0)
            } else {
                let angle = (ch.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
                (angle.cos() * ch.volume, angle.sin() * ch.volume)
            };
            let mut peak = 0.0f32;
            for s in 0..n {
                ch.gain_l += (target_l - ch.gain_l) * SMOOTH;
                ch.gain_r += (target_r - ch.gain_r) * SMOOTH;
                let l = ch.buf_l[s] * ch.gain_l;
                let r = ch.buf_r[s] * ch.gain_r;
                peak = peak.max(l.abs()).max(r.abs());
                let track = &mut self.mixer[ch.route];
                track.buf_l[s] += l;
                track.buf_r[s] += r;
            }
            raise_peak(&self.shared.peaks[i], peak);
        }

        // 2b. Audio clips play straight into their mixer tracks.
        if self.playing && self.seq.length > 0 {
            let ticks_per_sample = self.tempo * PPQ as f64 / 60.0 / self.sr as f64;
            let seconds_per_tick = 60.0 / (self.tempo * PPQ as f64);
            let len = self.seq.length as f64;
            for r in &self.audio.regions {
                let track = &mut self.mixer[r.track.min(MIXER_TRACKS - 1)];
                r.render(
                    block_pos,
                    ticks_per_sample,
                    len,
                    seconds_per_tick,
                    &mut track.buf_l[..n],
                    &mut track.buf_r[..n],
                );
            }
        }

        // 3. Mixer: inserts 1..N run their effects, then volume/pan/mute, into
        //    the master (track 0), which runs its own effects last.
        for index in (1..MIXER_TRACKS).chain(std::iter::once(0)) {
            let MixerTrack { fx, volume, pan, mute, gain_l, gain_r, buf_l, buf_r } = &mut self.mixer[index];
            for (plugin, bypass) in fx.plugins.iter_mut().zip(&fx.bypass) {
                if !bypass {
                    plugin.transport(transport);
                    plugin.process(ProcessContext {
                        sample_rate: self.sr,
                        events: &[],
                        out_l: &mut buf_l[..n],
                        out_r: &mut buf_r[..n],
                    });
                }
            }
            let (target_l, target_r) = if *mute {
                (0.0, 0.0)
            } else {
                let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
                (angle.cos() * *volume * std::f32::consts::SQRT_2, angle.sin() * *volume * std::f32::consts::SQRT_2)
            };
            let (mut pl, mut pr) = (0.0f32, 0.0f32);
            for s in 0..n {
                *gain_l += (target_l - *gain_l) * SMOOTH;
                *gain_r += (target_r - *gain_r) * SMOOTH;
                buf_l[s] *= *gain_l;
                buf_r[s] *= *gain_r;
                pl = pl.max(buf_l[s].abs());
                pr = pr.max(buf_r[s].abs());
            }
            raise_peak(&self.shared.track_peaks[index * 2], pl);
            raise_peak(&self.shared.track_peaks[index * 2 + 1], pr);
            if index != 0 {
                let (head, tail) = self.mixer.split_at_mut(index);
                let (master, track) = (&mut head[0], &tail[0]);
                for s in 0..n {
                    master.buf_l[s] += track.buf_l[s];
                    master.buf_r[s] += track.buf_r[s];
                }
            }
        }
        self.mix_l[..n].copy_from_slice(&self.mixer[0].buf_l[..n]);
        self.mix_r[..n].copy_from_slice(&self.mixer[0].buf_r[..n]);

        // The browser's preview goes straight into the master (before master volume).
        if let Some(pv) = &mut self.preview {
            let ChannelSlot { plugin, events, pending, buf_l, buf_r, .. } = &mut **pv;
            events.clear();
            for e in pending.drain(..) {
                events.push(TimedEvent { offset: 0, event: e });
            }
            plugin.process(ProcessContext {
                sample_rate: self.sr,
                events,
                out_l: &mut buf_l[..n],
                out_r: &mut buf_r[..n],
            });
            for s in 0..n {
                self.mix_l[s] += buf_l[s] * 0.8;
                self.mix_r[s] += buf_r[s] * 0.8;
            }
        }

        // 4. Master volume + a hard safety clamp (protects ears and speakers).
        let (mut pl, mut pr) = (0.0f32, 0.0f32);
        for s in 0..n {
            self.master_gain += (self.master - self.master_gain) * SMOOTH;
            let l = (self.mix_l[s] * self.master_gain).clamp(-1.0, 1.0);
            let r = (self.mix_r[s] * self.master_gain).clamp(-1.0, 1.0);
            pl = pl.max(l.abs());
            pr = pr.max(r.abs());
            self.mix_l[s] = l;
            self.mix_r[s] = r;
        }
        raise_peak(&self.shared.master_peak[0], pl);
        raise_peak(&self.shared.master_peak[1], pr);
    }

    /// Turn the notes that fall inside this block into sample-accurate events.
    fn schedule(&mut self, n: usize) {
        let ticks_per_sample = self.tempo * PPQ as f64 / 60.0 / self.sr as f64;
        let len = self.seq.length as f64;
        let start = self.pos;
        let end = start + ticks_per_sample * n as f64;
        let block_ticks = end - start;
        // Swing pushes every odd 16th late, up to a third of a step (triplet feel).
        let swing_ticks = self.swing as f64 * TICKS_PER_STEP as f64 / 3.0;

        // 1. Release held notes that end inside this block (before new notes
        //    start, so a re-struck key gets its off first, then its new on).
        for ch in &mut self.channels {
            let mut i = 0;
            while i < ch.held.len() {
                let h = ch.held[i];
                if h.remaining < block_ticks {
                    let off = ((h.remaining / ticks_per_sample) as u32).min(n as u32 - 1);
                    if ch.events.len() < ch.events.capacity() {
                        ch.events.push(TimedEvent { offset: off, event: Event::NoteOff { key: h.key } });
                    }
                    ch.held.swap_remove(i); // no allocation
                } else {
                    ch.held[i].remaining -= block_ticks;
                    i += 1;
                }
            }
        }

        // A tick `t` in the loop sounds in this block if `t` or `t + len`
        // (after wrapping around the loop end) lies in [start, end).
        let offset_of = |t: f64| -> Option<u32> {
            [t, t + len]
                .into_iter()
                .find(|&c| c >= start && c < end)
                .map(|c| (((c - start) / ticks_per_sample) as u32).min(n as u32 - 1))
        };

        for note in &self.seq.notes {
            let Some(ch) = self.channels.get_mut(note.channel as usize) else { continue };
            let on = note.start as f64 + if (note.start / TICKS_PER_STEP) % 2 == 1 { swing_ticks } else { 0.0 };

            if let Some(o) = offset_of(on) {
                // Ticks from block start to this note's end (exact, before rounding to a sample).
                let since_start = if on >= start { on - start } else { on + len - start };
                let end_tick = since_start + note.len as f64;
                if end_tick < block_ticks {
                    // Ends inside this very block.
                    let off = ((end_tick / ticks_per_sample) as u32).min(n as u32 - 1).max(o);
                    if ch.events.len() < ch.events.capacity() {
                        ch.events.push(TimedEvent { offset: off, event: Event::NoteOff { key: note.key } });
                    }
                } else if ch.held.len() < ch.held.capacity() {
                    ch.held.push(Held { key: note.key, remaining: end_tick - block_ticks });
                }
                if ch.events.len() < ch.events.capacity() {
                    ch.events.push(TimedEvent {
                        offset: o,
                        event: Event::NoteOn { key: note.key, velocity: note.velocity },
                    });
                }
                self.shared.triggers[note.channel as usize].fetch_add(1, Relaxed);
                if ch.choke > 0 && self.chokes.len() < self.chokes.capacity() {
                    self.chokes.push((o, ch.choke, note.channel as usize));
                }
            }
        }

        self.pos = end % len;
    }

    fn apply_chokes(&mut self) {
        for &(offset, group, source) in &self.chokes {
            for (i, ch) in self.channels.iter_mut().enumerate() {
                if i != source && ch.choke == group && ch.events.len() < ch.events.capacity() {
                    ch.events.push(TimedEvent { offset, event: Event::AllNotesOff });
                }
            }
        }
        self.chokes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyra_plugin::ParamInfo;
    use std::sync::Mutex;

    /// Records every event it receives, with the absolute sample it landed on.
    struct Recorder {
        log: Arc<Mutex<Vec<(u64, Event)>>>,
        clock: u64,
    }

    impl Plugin for Recorder {
        fn name(&self) -> &'static str {
            "recorder"
        }
        fn params(&self) -> &'static [ParamInfo] {
            &[]
        }
        fn set_param(&mut self, _: u32, _: f32) {}
        fn activate(&mut self, _: f32) {}
        fn process(&mut self, ctx: ProcessContext<'_>) {
            let mut log = self.log.lock().unwrap(); // a Mutex is fine in a *test* plugin
            for e in ctx.events {
                log.push((self.clock + e.offset as u64, e.event));
            }
            self.clock += ctx.out_l.len() as u64;
            ctx.out_l.fill(0.0);
            ctx.out_r.fill(0.0);
        }
    }

    type Log = Arc<Mutex<Vec<(u64, Event)>>>;

    /// 48 kHz at 120 bpm: one tick = 250 samples (96 ticks per 0.5 s beat).
    fn setup() -> (Engine, EngineHandle, Log) {
        let (mut eng, mut h) = create(48_000.0);
        let log = Arc::new(Mutex::new(Vec::new()));
        h.send(Command::AddChannel(ChannelSlot::new(Box::new(Recorder { log: log.clone(), clock: 0 }), 48_000.0)));
        h.send(Command::SetTempo(120.0));
        let mut out = vec![0.0; 2 * 64];
        eng.process(&mut out, 2); // apply setup commands
        log.lock().unwrap().clear();
        (eng, h, log)
    }

    fn note(start: u32, len: u32) -> SeqNote {
        SeqNote { start, len, channel: 0, key: 60, velocity: 1.0 }
    }

    fn run(eng: &mut Engine, samples: usize) {
        let mut out = vec![0.0; 2 * samples];
        eng.process(&mut out, 2);
    }

    #[test]
    fn note_off_lands_at_note_end() {
        let (mut eng, mut h, log) = setup();
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![note(0, 48)] })));
        h.send(Command::Play);
        run(&mut eng, 48 * 250 + 1000);
        let log = log.lock().unwrap();
        let on = log.iter().find(|(_, e)| matches!(e, Event::NoteOn { .. })).expect("note on").0;
        let off = log.iter().find(|(_, e)| matches!(e, Event::NoteOff { .. })).expect("note off").0;
        let len = off - on;
        assert!((11_990..=12_010).contains(&len), "48 ticks ≈ 12000 samples, got {len}");
    }

    #[test]
    fn deleting_a_sounding_note_still_releases_it() {
        let (mut eng, mut h, log) = setup();
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![note(0, 96)] })));
        h.send(Command::Play);
        run(&mut eng, 5_000); // note is now sounding
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![] }))); // user deletes it
        run(&mut eng, 30_000);
        let log = log.lock().unwrap();
        assert!(log.iter().any(|(_, e)| matches!(e, Event::NoteOff { key: 60 })), "the note must not hang");
    }

    /// A second of a steady 0.5 tone at 48 kHz.
    fn tone() -> Arc<SampleData> {
        Arc::new(SampleData { sample_rate: 48_000.0, left: vec![0.5; 48_000], right: vec![0.5; 48_000] })
    }

    fn region(start: f64, end: f64) -> AudioRegion {
        AudioRegion {
            start,
            end,
            data: tone(),
            from: 0.0,
            until: 1.0,
            gain: 1.0,
            fade_in: (0.0, 0.0),
            fade_out: (0.0, 0.0),
            track: 0,
        }
    }

    /// Run `samples` frames and return the left channel.
    fn capture(eng: &mut Engine, samples: usize) -> Vec<f32> {
        let mut out = vec![0.0; 2 * samples];
        eng.process(&mut out, 2);
        out.iter().step_by(2).copied().collect()
    }

    #[test]
    fn audio_plays_where_it_sits_and_from_the_middle_after_a_seek() {
        let (mut eng, mut h, _) = setup();
        // At 120 bpm a tick is 250 samples: the region covers samples 24_000..36_000.
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![] })));
        h.send(Command::SetAudio(Box::new(AudioLane { regions: vec![region(96.0, 144.0)] })));
        h.send(Command::SetMasterVolume(1.0));
        h.send(Command::Play);
        let out = capture(&mut eng, 40_000);
        assert!(out[23_900].abs() < 1e-4, "silent before");
        assert!(out[30_000].abs() > 0.2, "sounds inside");
        assert!(out[36_100].abs() < 1e-4, "silent after");
        // Seek into the middle: plays immediately, no trigger needed.
        h.send(Command::Seek(120.0));
        let out = capture(&mut eng, 1_000);
        assert!(out[500].abs() > 0.2);
    }

    #[test]
    fn audio_fades_follow_their_curve() {
        let (mut eng, mut h, _) = setup();
        let mut r = region(0.0, 192.0); // one second
        r.fade_in = (0.5, 0.0);
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![] })));
        h.send(Command::SetAudio(Box::new(AudioLane { regions: vec![r] })));
        h.send(Command::SetMasterVolume(1.0));
        h.send(Command::Play);
        let out = capture(&mut eng, 30_000);
        // Linear fade over 0.5 s: a quarter of the way in, ~a quarter of the level.
        // (The master fader smooths in from 0.8 to 1.0 over the first few ms.)
        let quarter = out[6_000] / out[29_000];
        assert!((quarter - 0.25).abs() < 0.02, "got {quarter}");
        assert!(out[0].abs() < 1e-3, "starts from silence");
        assert!((fade_curve(0.5, 0.5) - 0.5).abs() > 0.1 && fade_curve(0.5, 0.5) < 0.5, "tension bends it");
        assert!(fade_curve(0.5, -0.5) > 0.5);
        assert_eq!(fade_curve(1.0, 0.7), 1.0);
    }

    #[test]
    fn note_crossing_the_loop_end_is_released_after_wrapping() {
        let (mut eng, mut h, log) = setup();
        // Starts 24 ticks before the loop end, lasts 48: ends 24 ticks into the next loop.
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![note(360, 48)] })));
        h.send(Command::Play);
        run(&mut eng, 384 * 250 + 24 * 250 + 2_000);
        let log = log.lock().unwrap();
        let on = log.iter().find(|(_, e)| matches!(e, Event::NoteOn { .. })).unwrap().0;
        let off = log.iter().find(|(_, e)| matches!(e, Event::NoteOff { .. })).unwrap().0;
        assert!((11_990..=12_010).contains(&(off - on)), "got {}", off - on);
    }
}
