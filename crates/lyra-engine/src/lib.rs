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

use lyra_plugin::{Event, NoteId, ParamInfo, Plugin, ProcessContext, SampleData, TimedEvent, Transport, UpcomingNote};
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
/// Most notes one forecast holds (see `Plugin::upcoming`).
const MAX_UPCOMING: usize = 256;
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

/// A note's bend, drawn in the piano roll: from `start`, over `len` ticks,
/// its pitch moves `semitones` away along the automation curve (`tension`),
/// and stays there. Plugins hear it as the note's tuning expression.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeqBend {
    pub start: u32,
    pub len: u32,
    pub channel: u16,
    /// The note that bends (its start and key: what its id is made of).
    pub note_start: u32,
    pub note_key: u8,
    pub semitones: f32,
    pub tension: f32,
}

impl SeqBend {
    /// The tuning (semitones) at loop tick `t`.
    pub fn tuning_at(&self, t: f64) -> f32 {
        let u = if self.len == 0 { 1.0 } else { ((t - self.start as f64) / self.len as f64).clamp(0.0, 1.0) as f32 };
        self.semitones * fade_curve(u, self.tension)
    }
}

/// A compiled, read-only loop of notes the engine plays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sequence {
    /// Loop length in ticks (0 = nothing to play).
    pub length: u32,
    pub notes: Vec<SeqNote>,
    /// Notes' bends, in start order.
    pub bends: Vec<SeqBend>,
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
    /// Seconds into the file at `start`.
    pub from: f64,
    /// The source's trim, in seconds into the file: nothing outside it plays.
    pub trim: (f64, f64),
    /// Linear gain (the source's and the clip's together).
    pub gain: f32,
    /// -1 (left) .. 1 (right), constant power, unity in the middle.
    pub pan: f32,
    /// (seconds, tension) at each end of the region (the clip's own fades);
    /// see [`fade_curve`].
    pub fade_in: (f32, f32),
    pub fade_out: (f32, f32),
    /// Mixer track it plays into.
    pub track: usize,
    /// Read the window backwards: from its end in the file towards `from`.
    pub reverse: bool,
}

impl PartialEq for AudioRegion {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.data, &o.data)
            && (self.start, self.end, self.from, self.trim) == (o.start, o.end, o.from, o.trim)
            && (self.gain, self.fade_in, self.fade_out, self.track) == (o.gain, o.fade_in, o.fade_out, o.track)
            && self.pan == o.pan
            && self.reverse == o.reverse
    }
}

/// Every audio region in the song (empty when a pattern loops on its own).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioLane {
    pub regions: Vec<AudioRegion>,
}

/// Where an automated value goes, in the engine's own terms (channel and
/// slot *indexes*, which the bridge works out from the project's ids).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Addr {
    Param { channel: usize, id: u32 },
    Volume(usize),
    Pan(usize),
    MixerVolume(usize),
    MixerPan(usize),
    Effect { track: usize, slot: usize, id: u32 },
    Master,
}

/// One automated control: its curve along the song, as knob travel (0..1),
/// and how travel becomes a value (`info`, the control's own knob law).
#[derive(Clone, Debug, PartialEq)]
pub struct AutoLane {
    pub addr: Addr,
    pub info: ParamInfo,
    /// (song tick, travel, tension of the line to the next point), sorted.
    /// Two points on the same tick are a jump.
    pub points: Vec<(f64, f32, f32)>,
    /// Where the knob itself is: the value before the first point (and after Stop).
    pub rest: f32,
}

impl AutoLane {
    /// Travel at song tick `t`: the knob's own before the curve starts, the
    /// last value after it ends (automation holds).
    pub fn travel_at(&self, t: f64) -> f32 {
        curve_at(&self.points, t).unwrap_or(self.rest)
    }
}

/// A curve of (tick, value, tension-to-next) points at tick `t`: None before
/// the first point, the last value after the last. Lines bend like fades.
pub fn curve_at(points: &[(f64, f32, f32)], t: f64) -> Option<f32> {
    let first = points.first()?;
    if t < first.0 {
        return None;
    }
    // The last point at or before t (a jump: the later of two on one tick).
    let i = points.partition_point(|p| p.0 <= t) - 1;
    let a = points[i];
    let Some(&b) = points.get(i + 1) else { return Some(a.1) };
    let x = if b.0 > a.0 { ((t - a.0) / (b.0 - a.0)) as f32 } else { 1.0 };
    Some(a.1 + (b.1 - a.1) * fade_curve(x, a.2))
}

/// Every automated control in the song (empty when a pattern loops alone).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Automation {
    pub lanes: Vec<AutoLane>,
}

/// While automation runs, the engine works in slices this small, so values
/// move every ~1 ms instead of every block.
const AUTOMATION_SLICE: usize = 64;

/// How far tension ±1 bends a fade (the exponent at full tension).
pub const FADE_BEND: f32 = 6.0;

/// The tension whose curve passes through `gain` halfway through the fade:
/// the inverse of `fade_curve(0.5, t)`, which is 1 / (e^(BEND·t/2) + 1).
/// Dragging a curve's ring uses it, so the curve follows the mouse.
pub fn tension_through(gain: f32) -> f32 {
    let g = gain.clamp(0.005, 0.995);
    (2.0 * (1.0 / g - 1.0).ln() / FADE_BEND).clamp(-1.0, 1.0)
}

/// A fade's shape: `x` (0..1, how far towards full volume) to gain (0..1).
/// `tension` 0 is a straight line; above 0 the curve bows down (quieter
/// through the fade), below 0 it bows up (louder). A fade-out uses the same
/// curve read backwards, so the same tension looks the same at both ends.
pub fn fade_curve(x: f32, tension: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let c = tension.clamp(-1.0, 1.0) * FADE_BEND;
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
        let dur = ((self.end - self.start) * seconds_per_tick).min(self.trim.1 - self.from);
        let (fi, fo) = (self.fade_in.0 as f64, self.fade_out.0 as f64);
        // Constant-power pan, scaled so the middle is unity (like the mixer).
        let angle = (self.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        let (pl, pr) = (angle.cos() * std::f32::consts::SQRT_2, angle.sin() * std::f32::consts::SQRT_2);
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
            let file = if self.reverse { self.from + dur - el } else { self.from + el };
            if file < self.trim.0 {
                continue;
            }
            let at = file * data.sample_rate as f64;
            let i = at as usize;
            if i + 1 >= frames {
                continue;
            }
            let f = (at - i as f64) as f32;
            out_l[s] += (data.left[i] + (data.left[i + 1] - data.left[i]) * f) * g * pl;
            out_r[s] += (data.right[i] + (data.right[i + 1] - data.right[i]) * f) * g * pr;
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
    /// Seconds of upcoming notes the plugin asked for (0 = none), and the
    /// forecast handed to it each block.
    lookahead: f64,
    upcoming: Vec<UpcomingNote>,
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
        let lookahead = plugin.note_lookahead().clamp(0.0, 10.0);
        Box::new(Self {
            plugin,
            lookahead,
            upcoming: Vec::with_capacity(if lookahead > 0.0 { MAX_UPCOMING } else { 0 }),
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

/// The new channel list, built on the UI thread: for each position, either
/// the engine's channel now at index `keep` or a fresh slot. Channels no
/// position keeps are thrown away (as trash).
pub struct Arrangement {
    places: Vec<Place>,
}

pub enum Place {
    Keep(usize),
    Fresh(Option<Box<ChannelSlot>>),
}

impl Arrangement {
    pub fn new(places: Vec<Place>) -> Box<Self> {
        Box::new(Self { places })
    }
}

/// A sound played on its own (see `Command::Listen`): `data` from `from` to
/// `to` seconds, into mixer track `track`, at `gain`. Independent of the
/// song: it plays whether the song does or not.
pub struct Listen {
    pub data: Arc<SampleData>,
    pub from: f64,
    pub to: f64,
    pub track: usize,
    pub gain: f32,
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
    id: NoteId,
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
    /// Play only this part (ticks, start..end) over and over; None = all of it.
    SetLoop(Option<(f64, f64)>),
    AddChannel(Box<ChannelSlot>),
    /// The channel list changed (added, removed, reordered, swapped): the
    /// engine keeps the channels it still has, so their tails ring on, and
    /// throws away the rest. See [`Arrangement`].
    Arrange(Box<Arrangement>),
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
    /// Play a sound on its own, outside the song (the recorder's tape, an
    /// audio clip being edited), into a mixer track; None stops it.
    Listen(Option<Box<Listen>>),
    /// Replace the audio clips the song plays.
    SetAudio(Box<AudioLane>),
    /// Replace the automation the song plays.
    SetAutomation(Box<Automation>),
}

enum Trash {
    Listen(#[allow(dead_code)] Box<Listen>),
    Arrangement(#[allow(dead_code)] Box<Arrangement>),
    Audio(#[allow(dead_code)] Box<AudioLane>),
    Automation(#[allow(dead_code)] Box<Automation>),
    Sequence(#[allow(dead_code)] Box<Sequence>),
    Channel(#[allow(dead_code)] Box<ChannelSlot>),
    Chain(#[allow(dead_code)] Box<FxChain>),
}

// ─────────────────────────── data coming back ──────────────────────────────

/// Facts the audio thread publishes for the UI. Lock-free, read any time.
pub struct Shared {
    playhead: AtomicU64,
    playing: AtomicBool,
    /// Where the listened-to sound is, in seconds (negative: nothing playing).
    listen_at: AtomicU64,
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
            listen_at: AtomicU64::new((-1f64).to_bits()),
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

    /// Where the sound being listened to is (seconds into it), if one plays.
    pub fn listening(&self) -> Option<f64> {
        Some(f64::from_bits(self.listen_at.load(Relaxed))).filter(|t| *t >= 0.0)
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
/// A sequenced note's id on one pass of the loop: the same for its
/// forecast, its note-on and its note-off, and stable while other notes are
/// edited (it comes from the note's own key and place, not its index).
fn note_id(note: &SeqNote, lap: u32) -> NoteId {
    id_of(note.key, note.start, lap)
}

/// A note's id from its key and start (see `note_id`).
fn id_of(key: u8, start: u32, lap: u32) -> NoteId {
    let mut x = (key as u64) | (start as u64) << 8 | (lap as u64) << 40;
    // splitmix64's finaliser
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x as u32).min(lyra_plugin::NO_ID - 1)
}

fn raise_peak(a: &AtomicU32, v: f32) {
    a.fetch_max(v.to_bits(), Relaxed);
}

// ─────────────────────────── UI-side handle ────────────────────────────────

pub struct EngineHandle {
    tx: Producer<Command>,
    trash: Consumer<Trash>,
    shared: Arc<Shared>,
    sample_rate: f32,
    backlog: std::collections::VecDeque<Command>,
}

impl EngineHandle {
    /// Queue a command. If the FIFO is full (the audio thread stalled, or a
    /// huge edit), it waits in a backlog here and goes out, in order, as soon
    /// as there is room: a command is never lost.
    pub fn send(&mut self, cmd: Command) {
        self.flush();
        if !self.backlog.is_empty() {
            self.backlog.push_back(cmd);
        } else if let Err(rtrb::PushError::Full(cmd)) = self.tx.push(cmd) {
            self.backlog.push_back(cmd);
        }
    }

    /// Send what the backlog holds, as far as the FIFO has room.
    pub fn flush(&mut self) {
        while let Some(cmd) = self.backlog.pop_front() {
            if let Err(rtrb::PushError::Full(cmd)) = self.tx.push(cmd) {
                self.backlog.push_front(cmd);
                break;
            }
        }
    }

    /// Commands still waiting for room in the FIFO.
    pub fn backlog(&self) -> usize {
        self.backlog.len()
    }

    /// Free whatever the audio thread handed back, and send any backlog.
    /// Call once per UI frame.
    pub fn collect_garbage(&mut self) {
        while self.trash.pop().is_ok() {}
        self.flush();
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
    /// Scratch space for `Command::Arrange`.
    #[allow(clippy::vec_box)]
    spare: Vec<Option<Box<ChannelSlot>>>,
    seq: Box<Sequence>,
    audio: Box<AudioLane>,
    automation: Box<Automation>,

    playing: bool,
    /// A loop region inside the sequence (see `Command::SetLoop`).
    looped: Option<(f64, f64)>,
    /// Position inside the loop, in ticks.
    pos: f64,
    /// Where Play starts and Stop returns to (set by Seek).
    start_marker: f64,
    /// Times round the loop (and transport jumps), so each pass of a note
    /// gets its own id.
    laps: u32,
    tempo: f64,
    swing: f32,
    master: f32,
    master_gain: f32,

    mixer: Vec<MixerTrack>,
    mix_l: Vec<f32>,
    mix_r: Vec<f32>,
    /// What the browser is auditioning (see `Command::Preview`).
    preview: Option<Box<ChannelSlot>>,
    /// A sound played on its own, and where it is (seconds).
    listen: Option<Box<Listen>>,
    listen_pos: f64,
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
        spare: Vec::with_capacity(MAX_CHANNELS),
        seq: Box::default(),
        audio: Box::default(),
        automation: Box::default(),
        playing: false,
        looped: None,
        pos: 0.0,
        start_marker: 0.0,
        laps: 0,
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
        listen: None,
        listen_pos: 0.0,
        chokes: Vec::with_capacity(64),
    };
    let handle = EngineHandle { tx, trash: trash_rx, shared, sample_rate, backlog: Default::default() };
    (engine, handle)
}

impl Engine {
    /// Fill an interleaved output buffer (`channels` = 2 for stereo). REAL-TIME.
    pub fn process(&mut self, out: &mut [f32], channels: usize) {
        self.drain_commands();
        let frames = out.len() / channels.max(1);
        let mut done = 0;
        let slice = if self.playing && !self.automation.lanes.is_empty() { AUTOMATION_SLICE } else { BLOCK };
        while done < frames {
            let n = (frames - done).min(slice);
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
                    self.laps = self.laps.wrapping_add(1);
                    self.pos = self.start_marker;
                }
                Command::SetLoop(range) => self.looped = range,
                Command::Seek(tick) => {
                    let len = self.seq.length.max(1) as f64;
                    self.start_marker = tick.clamp(0.0, len - 1.0);
                    self.pos = self.start_marker;
                    self.laps = self.laps.wrapping_add(1);
                }
                Command::Stop => {
                    self.playing = false;
                    self.pos = self.start_marker;
                    // Stopped, every knob is where its owner set it.
                    for i in 0..self.automation.lanes.len() {
                        let lane = &self.automation.lanes[i];
                        let (addr, value) = (lane.addr, lane.info.from_normalized(lane.rest));
                        self.apply(addr, value);
                    }
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
                Command::Arrange(mut arr) => {
                    // Both lists have MAX_CHANNELS reserved: only pointers move.
                    self.spare.clear();
                    self.spare.extend(self.channels.drain(..).map(Some));
                    for place in arr.places.iter_mut().take(MAX_CHANNELS) {
                        let slot = match place {
                            Place::Keep(j) => self.spare.get_mut(*j).and_then(Option::take),
                            Place::Fresh(slot) => slot.take(),
                        };
                        // A bad index can't happen, but never leave a hole: indexes must line up.
                        let Some(slot) = slot else { break };
                        self.channels.push(slot);
                    }
                    for i in 0..self.spare.len() {
                        if let Some(gone) = self.spare[i].take() {
                            self.discard(Trash::Channel(gone));
                        }
                    }
                    self.discard(Trash::Arrangement(arr));
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
                    slot.pending.push(Event::note_on(key, 0.9)); // capacity reserved
                    if let Some(old) = self.preview.replace(slot) {
                        self.discard(Trash::Channel(old));
                    }
                }
                Command::SetAudio(lane) => {
                    let old = std::mem::replace(&mut self.audio, lane);
                    self.discard(Trash::Audio(old));
                }
                Command::SetAutomation(set) => {
                    let old = std::mem::replace(&mut self.automation, set);
                    // A control no longer automated goes back to its own value.
                    for lane in &old.lanes {
                        if !self.automation.lanes.iter().any(|l| l.addr == lane.addr) {
                            self.apply(lane.addr, lane.info.from_normalized(lane.rest));
                        }
                    }
                    self.discard(Trash::Automation(old));
                }
                Command::Listen(next) => {
                    self.listen_pos = next.as_ref().map_or(0.0, |l| l.from);
                    if let Some(old) = std::mem::replace(&mut self.listen, next) {
                        self.discard(Trash::Listen(old));
                    }
                    if self.listen.is_none() {
                        self.shared.listen_at.store((-1f64).to_bits(), Relaxed);
                    }
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
        for (i, ch) in self.channels.iter_mut().enumerate() {
            let ChannelSlot { events, pending, choke, .. } = &mut **ch;
            events.clear();
            for e in pending.drain(..) {
                // A live note chokes its group too (a closed hat played on the keyboard).
                if matches!(e, Event::NoteOn { .. }) && *choke > 0 && self.chokes.len() < self.chokes.capacity() {
                    self.chokes.push((0, *choke, i));
                }
                events.push(TimedEvent { offset: 0, event: e });
            }
        }
        let block_pos = self.pos;
        let block_laps = self.laps; // before scheduling wraps the loop
        if self.playing && self.seq.length > 0 {
            // Automation first: this slice plays with the values the song has here.
            for i in 0..self.automation.lanes.len() {
                let lane = &self.automation.lanes[i];
                let (addr, value) = (lane.addr, lane.info.from_normalized(lane.travel_at(block_pos)));
                self.apply(addr, value);
            }
            self.schedule(n);
        }
        self.forecast(block_pos, block_laps);
        self.apply_chokes();

        // 2. Run every instrument, apply volume/pan, add it into its mixer track.
        for t in &mut self.mixer {
            t.buf_l[..n].fill(0.0);
            t.buf_r[..n].fill(0.0);
        }
        for (i, ch) in self.channels.iter_mut().enumerate() {
            let ChannelSlot { plugin, events, buf_l, buf_r, lookahead, upcoming, .. } = &mut **ch;
            // AllNotesOff sorts before a NoteOn at the same sample, so a choke
            // never kills the note that's starting right then. (In-place sort: no allocation.)
            // At one sample: AllNotesOff first, note expressions last (after
            // the note-on of the note they change).
            events.sort_unstable_by_key(|e| {
                let rank = match e.event {
                    Event::AllNotesOff => 0,
                    Event::NoteExpression { .. } => 2,
                    _ => 1,
                };
                (e.offset, rank)
            });
            plugin.transport(transport);
            if *lookahead > 0.0 {
                plugin.upcoming(upcoming);
            }
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

        // 2c. A sound played on its own, into its mixer track.
        self.render_listen(n);

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
        // What plays round and round: the loop region, or the whole sequence.
        let (from, to) = self.span();
        let len = to - from;
        if !(from..to).contains(&self.pos) {
            self.pos = from; // started (or moved) outside the loop: go to its start
        }
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
                        ch.events.push(TimedEvent { offset: off, event: Event::NoteOff { key: h.key, id: h.id } });
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
        // Also says which pass it is: 0 this one, 1 the next (wrapped).
        let offset_of = |t: f64| -> Option<(u32, u32)> {
            if !(from..to).contains(&t) {
                return None;
            }
            [(t, 0), (t + len, 1)]
                .into_iter()
                .find(|&(c, _)| c >= start && c < end)
                .map(|(c, lap)| ((((c - start) / ticks_per_sample) as u32).min(n as u32 - 1), lap))
        };

        for note in &self.seq.notes {
            let Some(ch) = self.channels.get_mut(note.channel as usize) else { continue };
            let on = note.start as f64 + if (note.start / TICKS_PER_STEP) % 2 == 1 { swing_ticks } else { 0.0 };

            if let Some((o, lap)) = offset_of(on) {
                let id = note_id(note, self.laps.wrapping_add(lap));
                // Ticks from block start to this note's end (exact, before rounding to a sample).
                let since_start = if on >= start { on - start } else { on + len - start };
                let end_tick = since_start + note.len as f64;
                if end_tick < block_ticks {
                    // Ends inside this very block.
                    let off = ((end_tick / ticks_per_sample) as u32).min(n as u32 - 1).max(o);
                    if ch.events.len() < ch.events.capacity() {
                        ch.events.push(TimedEvent { offset: off, event: Event::NoteOff { key: note.key, id } });
                    }
                } else if ch.held.len() < ch.held.capacity() {
                    ch.held.push(Held { key: note.key, id, remaining: end_tick - block_ticks });
                }
                if ch.events.len() < ch.events.capacity() {
                    ch.events.push(TimedEvent {
                        offset: o,
                        event: Event::NoteOn { key: note.key, velocity: note.velocity, id },
                    });
                }
                self.shared.triggers[note.channel as usize].fetch_add(1, Relaxed);
                if ch.choke > 0 && self.chokes.len() < self.chokes.capacity() {
                    self.chokes.push((o, ch.choke, note.channel as usize));
                }
            }
        }

        // Bends: while a bent note's pitch moves (and once after, so it lands
        // exactly), its tuning where this block ends, as a note expression
        // where the bend's part of the block begins; the plugin glides there.
        // Unswung: a bend belongs to the line, not the grid.
        for b in &self.seq.bends {
            let Some(ch) = self.channels.get_mut(b.channel as usize) else { continue };
            let (a, z) = (b.start as f64, (b.start + b.len) as f64);
            // This pass, or (after the loop's end, wrapped) the next one.
            for (shift, lap) in [(0.0, 0), (len, 1)] {
                let (s, e) = (start - shift, end - shift);
                if e > a && s <= z && ch.events.len() < ch.events.capacity() {
                    let event = Event::NoteExpression {
                        id: id_of(b.note_key, b.note_start, self.laps.wrapping_add(lap)),
                        key: b.note_key,
                        expression: lyra_plugin::Expression::Tuning,
                        value: b.tuning_at(e.min(z)),
                    };
                    let o = (((a - s).max(0.0) / ticks_per_sample) as u32).min(n as u32 - 1);
                    ch.events.push(TimedEvent { offset: o, event });
                }
            }
        }

        if end >= to {
            self.laps = self.laps.wrapping_add(1);
        }
        self.pos = from + (end - from) % len;
    }

    /// The `lyra.note-lookahead` forecast for every channel that asked:
    /// each note starting in the next `lookahead` seconds from song tick
    /// `start` (in pass `laps`), round the loop as often as it fits. Ids match what
    /// `schedule` will give the notes. Empty while stopped. REAL-TIME.
    fn forecast(&mut self, start: f64, laps: u32) {
        let rolling = self.playing && self.seq.length > 0;
        let ticks_per_second = self.tempo * PPQ as f64 / 60.0;
        let (from, to) = self.span();
        let len = to - from;
        let swing_ticks = self.swing as f64 * TICKS_PER_STEP as f64 / 3.0;
        for (c, ch) in self.channels.iter_mut().enumerate() {
            if ch.lookahead <= 0.0 {
                continue;
            }
            ch.upcoming.clear();
            if !rolling {
                continue;
            }
            let horizon = start + ch.lookahead * ticks_per_second;
            for note in self.seq.notes.iter().filter(|n| n.channel as usize == c) {
                let on = note.start as f64 + if (note.start / TICKS_PER_STEP) % 2 == 1 { swing_ticks } else { 0.0 };
                if !(from..to).contains(&on) {
                    continue;
                }
                // Its first pass at or after `start`, then every loop after.
                let (mut at, mut lap) = if on >= start { (on, 0u32) } else { (on + len, 1) };
                while at < horizon && ch.upcoming.len() < ch.upcoming.capacity() {
                    ch.upcoming.push(UpcomingNote {
                        start: (at - start) / ticks_per_second,
                        len: note.len as f64 / ticks_per_second,
                        key: note.key,
                        velocity: note.velocity,
                        id: note_id(note, laps.wrapping_add(lap)),
                    });
                    at += len;
                    lap += 1;
                }
            }
            // In place, no allocation (insertion sort: nearly sorted already).
            let u = &mut ch.upcoming;
            for i in 1..u.len() {
                let mut j = i;
                while j > 0 && u[j - 1].start > u[j].start {
                    u.swap(j - 1, j);
                    j -= 1;
                }
            }
        }
    }

    /// Mix the listened-to sound's next `n` samples into its track. REAL-TIME.
    fn render_listen(&mut self, n: usize) {
        let Some(l) = &self.listen else { return };
        let track = &mut self.mixer[l.track.min(MIXER_TRACKS - 1)];
        let data = &*l.data;
        let frames = data.left.len().min(data.right.len());
        let step = 1.0 / self.sr as f64;
        for s in 0..n {
            let t = self.listen_pos + s as f64 * step;
            if t >= l.to {
                break;
            }
            let at = t * data.sample_rate as f64;
            let i = at as usize;
            if i + 1 >= frames {
                break;
            }
            let f = (at - i as f64) as f32;
            track.buf_l[s] += (data.left[i] + (data.left[i + 1] - data.left[i]) * f) * l.gain;
            track.buf_r[s] += (data.right[i] + (data.right[i + 1] - data.right[i]) * f) * l.gain;
        }
        self.listen_pos += n as f64 * step;
        let end = l.to.min(frames as f64 / data.sample_rate as f64);
        if self.listen_pos >= end {
            let done = self.listen.take().expect("checked above");
            self.discard(Trash::Listen(done));
            self.shared.listen_at.store((-1f64).to_bits(), Relaxed);
        } else {
            self.shared.listen_at.store(self.listen_pos.to_bits(), Relaxed);
        }
    }

    /// The part of the sequence that plays round and round.
    fn span(&self) -> (f64, f64) {
        let len = self.seq.length.max(1) as f64;
        match self.looped {
            Some((a, b)) if a < len && b > a => (a.max(0.0), b.min(len)),
            _ => (0.0, len),
        }
    }

    /// Set an automated control. REAL-TIME (plain stores and `set_param`).
    fn apply(&mut self, addr: Addr, value: f32) {
        match addr {
            Addr::Param { channel, id } => {
                if let Some(ch) = self.channels.get_mut(channel) {
                    ch.plugin.set_param(id, value);
                }
            }
            Addr::Volume(c) => {
                if let Some(ch) = self.channels.get_mut(c) {
                    ch.volume = value;
                }
            }
            Addr::Pan(c) => {
                if let Some(ch) = self.channels.get_mut(c) {
                    ch.pan = value;
                }
            }
            Addr::MixerVolume(t) => {
                if let Some(m) = self.mixer.get_mut(t) {
                    m.volume = value;
                }
            }
            Addr::MixerPan(t) => {
                if let Some(m) = self.mixer.get_mut(t) {
                    m.pan = value;
                }
            }
            Addr::Effect { track, slot, id } => {
                if let Some(p) = self.mixer.get_mut(track).and_then(|t| t.fx.plugins.get_mut(slot)) {
                    p.set_param(id, value);
                }
            }
            Addr::Master => self.master = value.max(0.0),
        }
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

    #[test]
    fn a_bent_note_sends_its_tuning_as_it_goes() {
        // C4 for two beats, bending up 4 semitones over its second beat. One tick = 250 samples.
        let bend = SeqBend { start: 96, len: 96, channel: 0, note_start: 0, note_key: 60, semitones: 4.0, tension: 0.0 };
        let (mut eng, mut h) = create(48_000.0);
        let log = Arc::new(Mutex::new(Vec::new()));
        h.send(Command::AddChannel(ChannelSlot::new(Box::new(Recorder { log: log.clone(), clock: 0 }), 48_000.0)));
        h.send(Command::SetTempo(120.0));
        let seq = Sequence { length: 384, notes: vec![note(0, 192)], bends: vec![bend] };
        h.send(Command::SetSequence(Box::new(seq)));
        run(&mut eng, 64);
        log.lock().unwrap().clear();
        h.send(Command::Play);
        run(&mut eng, 200 * 250);
        let events: Vec<(u64, Event)> = log.lock().unwrap().clone();
        let on = events.iter().find_map(|(_, e)| match e {
            Event::NoteOn { id, .. } => Some(*id),
            _ => None,
        });
        let tunings: Vec<(u64, f32)> = events
            .iter()
            .filter_map(|(at, e)| match e {
                Event::NoteExpression { id, expression: lyra_plugin::Expression::Tuning, value, .. } => {
                    assert_eq!(Some(*id), on, "it tunes that very note");
                    Some((*at, *value))
                }
                _ => None,
            })
            .collect();
        assert!(tunings.len() > 10, "a stream while it moves: {}", tunings.len());
        assert!(tunings.iter().all(|&(at, _)| at >= 96 * 250), "none before the bend");
        assert!(tunings.windows(2).all(|w| w[1].1 >= w[0].1), "rising");
        assert_eq!(tunings.last().map(|t| t.1), Some(4.0), "lands exactly");
        let half = tunings.iter().find(|t| t.0 >= 144 * 250).unwrap().1;
        assert!((half - 2.0).abs() < 0.2, "halfway: {half}");
    }

    fn note(start: u32, len: u32) -> SeqNote {
        SeqNote { start, len, channel: 0, key: 60, velocity: 1.0 }
    }

    fn run(eng: &mut Engine, samples: usize) {
        let mut out = vec![0.0; 2 * samples];
        eng.process(&mut out, 2);
    }

    /// (absolute sample, id, key) of every forecast entry seen.
    type Said = Arc<Mutex<Vec<(u64, NoteId, u8)>>>;

    /// Asks for one second of upcoming notes and remembers, per id, when the
    /// forecasts said it would start (absolute sample), plus every event.
    struct Seer {
        log: Log,
        said: Said,
        clock: u64,
    }

    impl Plugin for Seer {
        fn name(&self) -> &'static str {
            "seer"
        }
        fn params(&self) -> &'static [ParamInfo] {
            &[]
        }
        fn set_param(&mut self, _: u32, _: f32) {}
        fn activate(&mut self, _: f32) {}
        fn note_lookahead(&self) -> f64 {
            1.0
        }
        fn upcoming(&mut self, notes: &[UpcomingNote]) {
            let mut said = self.said.lock().unwrap();
            for w in notes.windows(2) {
                assert!(w[0].start <= w[1].start, "in start order");
            }
            for n in notes {
                assert!(n.start >= 0.0 && n.start < 1.0, "inside the look-ahead: {n:?}");
                said.push((self.clock + (n.start * 48_000.0).round() as u64, n.id, n.key));
            }
        }
        fn process(&mut self, ctx: ProcessContext<'_>) {
            let mut log = self.log.lock().unwrap();
            for e in ctx.events {
                log.push((self.clock + e.offset as u64, e.event));
            }
            self.clock += ctx.out_l.len() as u64;
            ctx.out_l.fill(0.0);
        }
    }

    #[test]
    fn the_forecast_names_every_note_before_it_plays_even_round_the_loop() {
        let (mut eng, mut h, _) = setup();
        let (log, said): (Log, Said) = Default::default();
        let seer = Seer { log: log.clone(), said: said.clone(), clock: 0 };
        h.send(Command::Arrange(Arrangement::new(vec![Place::Fresh(Some(ChannelSlot::new(
            Box::new(seer),
            48_000.0,
        )))])));
        // A one-bar loop (2 s at 120 bpm) of four quarter notes.
        let notes = (0..4).map(|b| SeqNote { key: 60 + b as u8, ..note(b * 96, 48) }).collect();
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes, bends: Vec::new() })));
        h.send(Command::Play);
        run(&mut eng, 48_000 * 5); // two and a half passes
        let (log, said) = (log.lock().unwrap(), said.lock().unwrap());
        let ons: Vec<(u64, NoteId, u8)> = log
            .iter()
            .filter_map(|&(at, e)| match e {
                Event::NoteOn { key, id, .. } => Some((at, id, key)),
                _ => None,
            })
            .collect();
        assert_eq!(ons.len(), 11, "{ons:?}");
        let mut ids: Vec<NoteId> = ons.iter().map(|o| o.1).collect();
        ids.dedup();
        assert_eq!(ids.len(), 11, "every pass of a note is a new id");
        for &(at, id, key) in &ons {
            let seen: Vec<&(u64, NoteId, u8)> = said.iter().filter(|s| s.1 == id).collect();
            assert!(!seen.is_empty(), "note {key} at {at} was never forecast");
            // Seen in advance (a full second for all but the first notes),
            // at the right time, and with the right key.
            assert!(seen.iter().all(|s| s.2 == key && s.0.abs_diff(at) <= 1), "{key} at {at}: {seen:?}");
            if at > 48_000 {
                assert!(seen.len() > 100, "seen for a second ahead, {} blocks", seen.len());
            }
            let off = log.iter().find(|(_, e)| matches!(e, Event::NoteOff { id: i, .. } if *i == id));
            assert!(off.is_none_or(|o| o.0 > at), "its note-off carries the same id");
        }
    }

    #[test]
    fn a_stopped_song_forecasts_nothing_and_an_edit_shows_at_once() {
        let (mut eng, mut h, _) = setup();
        let (log, said): (Log, Said) = Default::default();
        let seer = Seer { log, said: said.clone(), clock: 0 };
        h.send(Command::Arrange(Arrangement::new(vec![Place::Fresh(Some(ChannelSlot::new(
            Box::new(seer),
            48_000.0,
        )))])));
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![note(192, 48)], bends: Vec::new() })));
        run(&mut eng, 10_000);
        assert!(said.lock().unwrap().is_empty(), "stopped");
        h.send(Command::Play);
        run(&mut eng, 512);
        assert!(said.lock().unwrap().iter().all(|s| s.2 == 60), "the note one second ahead");
        assert!(!said.lock().unwrap().is_empty());
        said.lock().unwrap().clear();
        h.send(Command::SetSequence(Box::new(Sequence {
            length: 384,
            notes: vec![SeqNote { key: 62, ..note(192, 48) }], bends: Vec::new(),
        })));
        run(&mut eng, 512);
        let said = said.lock().unwrap();
        assert!(!said.is_empty() && said.iter().skip(1).all(|s| s.2 == 62), "edited: {said:?}");
    }

    #[test]
    fn note_off_lands_at_note_end() {
        let (mut eng, mut h, log) = setup();
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![note(0, 48)], bends: Vec::new() })));
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
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![note(0, 96)], bends: Vec::new() })));
        h.send(Command::Play);
        run(&mut eng, 5_000); // note is now sounding
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![], bends: Vec::new() }))); // user deletes it
        run(&mut eng, 30_000);
        let log = log.lock().unwrap();
        assert!(log.iter().any(|(_, e)| matches!(e, Event::NoteOff { key: 60, .. })), "the note must not hang");
    }

    #[test]
    fn a_kept_channel_rings_on_when_another_is_added_before_it() {
        let (mut eng, mut h, log) = setup();
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![note(0, 96)], bends: Vec::new() })));
        h.send(Command::Play);
        run(&mut eng, 5_000); // the note is sounding
        let other: Log = Arc::default();
        let fresh = ChannelSlot::new(Box::new(Recorder { log: other.clone(), clock: 0 }), 48_000.0);
        h.send(Command::Arrange(Arrangement::new(vec![Place::Fresh(Some(fresh)), Place::Keep(0)])));
        // The note's channel moved to index 1; the sequence follows it.
        let moved = SeqNote { channel: 1, ..note(0, 96) };
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![moved], bends: Vec::new() })));
        run(&mut eng, 30_000);
        let log = log.lock().unwrap();
        assert!(!log.iter().any(|(_, e)| matches!(e, Event::AllNotesOff)), "not cut off");
        let off = log.iter().find(|(_, e)| matches!(e, Event::NoteOff { .. })).expect("released on time").0;
        assert!((23_900..=24_100).contains(&off), "96 ticks ≈ 24000 samples, got {off}");
        assert!(other.lock().unwrap().is_empty(), "the new channel heard nothing");
        h.collect_garbage();
    }

    #[test]
    fn a_loop_region_plays_only_its_part_round_and_round() {
        let (mut eng, mut h, log) = setup();
        // A note on each beat; loop the second beat only (ticks 96..192).
        let notes = (0..4).map(|b| SeqNote { key: 60 + b as u8, ..note(b * 96, 24) }).collect();
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes, bends: Vec::new() })));
        h.send(Command::SetLoop(Some((96.0, 192.0))));
        h.send(Command::Play);
        run(&mut eng, 3 * 96 * 250 - 1000); // just under three passes (96 ticks = 24 000 samples)
        let log = log.lock().unwrap();
        let ons: Vec<(u64, u8)> = log
            .iter()
            .filter_map(|(at, e)| match e {
                Event::NoteOn { key, .. } => Some((*at, *key)),
                _ => None,
            })
            .collect();
        assert_eq!(ons.iter().map(|o| o.1).collect::<Vec<_>>(), vec![61, 61, 61], "only the looped beat");
        assert!((23_900..=24_100).contains(&(ons[1].0 - ons[0].0)), "once per pass: {ons:?}");
    }

    #[test]
    fn listening_plays_on_its_own_and_ends() {
        let (mut eng, mut h, _log) = setup();
        let data = Arc::new(SampleData { sample_rate: 48_000.0, left: vec![0.5; 48_000], right: vec![0.5; 48_000] });
        // Half a second of it, from 0.25 s, into the master; the song is stopped.
        h.send(Command::Listen(Some(Box::new(Listen { data, from: 0.25, to: 0.75, track: 0, gain: 1.0 }))));
        let mut out = vec![0.0; 2 * 4800];
        eng.process(&mut out, 2);
        assert!(out.iter().any(|x| x.abs() > 0.1), "it sounds without the song");
        let at = h.shared().listening().unwrap();
        assert!((at - 0.35).abs() < 0.01, "0.1 s in, from 0.25: {at}");
        for _ in 0..5 {
            eng.process(&mut out, 2);
        }
        assert_eq!(h.shared().listening(), None, "and stops by itself at the end");
        h.collect_garbage();
    }

    #[test]
    fn a_full_fifo_keeps_commands_in_a_backlog() {
        let (mut eng, mut h, _log) = setup();
        for _ in 0..1500 {
            h.send(Command::SetSwing(0.0));
        }
        h.send(Command::SetTempo(90.0));
        assert!(h.backlog() > 0);
        while h.backlog() > 0 {
            run(&mut eng, 64);
            h.collect_garbage();
        }
        run(&mut eng, 64);
        assert_eq!(eng.tempo, 90.0, "the last command arrived, in order");
    }

    #[test]
    fn a_live_note_chokes_its_group() {
        let (mut eng, mut h, log) = setup();
        let open: Log = Arc::default();
        h.send(Command::AddChannel(ChannelSlot::new(Box::new(Recorder { log: open.clone(), clock: 0 }), 48_000.0)));
        h.send(Command::SetMix { channel: 0, volume: 0.8, pan: 0.0, mute: false, choke: 1 });
        h.send(Command::SetMix { channel: 1, volume: 0.8, pan: 0.0, mute: false, choke: 1 });
        h.send(Command::Live { channel: 0, event: Event::note_on(42, 1.0) });
        run(&mut eng, 256);
        assert!(open.lock().unwrap().iter().any(|(_, e)| matches!(e, Event::AllNotesOff)), "the open hat is cut");
        assert!(!log.lock().unwrap().iter().any(|(_, e)| matches!(e, Event::AllNotesOff)), "not the one played");
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
            trim: (0.0, 1.0),
            gain: 1.0,
            pan: 0.0,
            fade_in: (0.0, 0.0),
            fade_out: (0.0, 0.0),
            track: 0,
            reverse: false,
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
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![], bends: Vec::new() })));
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
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![], bends: Vec::new() })));
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
        for t in [-0.8, -0.3, 0.0, 0.4, 0.9] {
            assert!((tension_through(fade_curve(0.5, t)) - t).abs() < 1e-3, "inverse at {t}");
        }
    }

    /// Remembers the last value each parameter was set to.
    struct Knobs(Arc<Mutex<Vec<f32>>>);

    static LEVEL: [ParamInfo; 1] = [ParamInfo::new(0, "Level", 0.0, 100.0, 0.0, "", lyra_plugin::Curve::Linear)];

    impl Plugin for Knobs {
        fn name(&self) -> &'static str {
            "knobs"
        }
        fn params(&self) -> &'static [ParamInfo] {
            &LEVEL
        }
        fn set_param(&mut self, _: u32, v: f32) {
            self.0.lock().unwrap().push(v); // a Mutex is fine in a *test* plugin
        }
        fn activate(&mut self, _: f32) {}
        fn process(&mut self, ctx: ProcessContext<'_>) {
            ctx.out_l.fill(0.0);
            ctx.out_r.fill(0.0);
        }
    }

    #[test]
    fn automation_moves_a_parameter_chases_and_rests_on_stop() {
        let (mut eng, mut h) = create(48_000.0);
        let set = Arc::new(Mutex::new(Vec::new()));
        h.send(Command::AddChannel(ChannelSlot::new(Box::new(Knobs(set.clone())), 48_000.0)));
        h.send(Command::SetTempo(120.0)); // a bar = 384 ticks = 2 s = 96 000 samples
        h.send(Command::SetSequence(Box::new(Sequence { length: 768, notes: vec![], bends: Vec::new() })));
        // A ramp from 0 to 1 over the first bar; the knob itself rests at 0.2.
        let lane = AutoLane {
            addr: Addr::Param { channel: 0, id: 0 },
            info: LEVEL[0],
            points: vec![(0.0, 0.0, 0.0), (384.0, 1.0, 0.0)],
            rest: 0.2,
        };
        h.send(Command::SetAutomation(Box::new(Automation { lanes: vec![lane] })));
        h.send(Command::Play);
        run(&mut eng, 48_000); // half a bar
        let last = |s: &Arc<Mutex<Vec<f32>>>| *s.lock().unwrap().last().unwrap();
        assert!((last(&set) - 50.0).abs() < 1.0, "halfway up the ramp, got {}", last(&set));
        // Seek into the second bar: the ramp has ended, its value holds.
        h.send(Command::Seek(500.0));
        run(&mut eng, 128);
        assert!((last(&set) - 100.0).abs() < 1e-3, "holds after the curve, got {}", last(&set));
        h.send(Command::Stop);
        run(&mut eng, 128);
        assert!((last(&set) - 20.0).abs() < 1e-3, "back to the knob on Stop, got {}", last(&set));
        assert_eq!(curve_at(&[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (10.0, 1.0, 0.0)], 10.0), Some(1.0), "a jump");
    }

    #[test]
    fn a_reversed_region_reads_its_window_backwards() {
        let (mut eng, mut h, _) = setup();
        // A rising ramp, 0 → 1 over one second; a half-second region of it.
        let ramp: Vec<f32> = (0..48_000).map(|i| i as f32 / 48_000.0).collect();
        let mut r = region(0.0, 96.0);
        r.data = Arc::new(SampleData { sample_rate: 48_000.0, left: ramp.clone(), right: ramp });
        r.reverse = true;
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![], bends: Vec::new() })));
        h.send(Command::SetAudio(Box::new(AudioLane { regions: vec![r] })));
        h.send(Command::SetMasterVolume(1.0));
        h.send(Command::Play);
        let out = capture(&mut eng, 24_000);
        // Backwards from 0.5 s: loud first, falling.
        assert!(
            out[2_000] > out[12_000] && out[12_000] > out[22_000],
            "{} {} {}",
            out[2_000],
            out[12_000],
            out[22_000]
        );
    }

    #[test]
    fn audio_stops_at_the_trim_end_even_inside_a_longer_clip() {
        let (mut eng, mut h, _) = setup();
        // A 4-beat clip (2 s) whose audio is trimmed to end at 0.75 s, and which
        // starts half a second in: a quarter second of sound, then silence.
        let mut r = region(0.0, 384.0);
        r.from = 0.5;
        r.trim = (0.0, 0.75);
        h.send(Command::SetSequence(Box::new(Sequence { length: 768, notes: vec![], bends: Vec::new() })));
        h.send(Command::SetAudio(Box::new(AudioLane { regions: vec![r] })));
        h.send(Command::SetMasterVolume(1.0));
        h.send(Command::Play);
        let out = capture(&mut eng, 24_000);
        assert!(out[6_000].abs() > 0.2, "sounds within the trim");
        assert!(out[12_500].abs() < 1e-4, "silent past the trim end");
    }

    #[test]
    fn note_crossing_the_loop_end_is_released_after_wrapping() {
        let (mut eng, mut h, log) = setup();
        // Starts 24 ticks before the loop end, lasts 48: ends 24 ticks into the next loop.
        h.send(Command::SetSequence(Box::new(Sequence { length: 384, notes: vec![note(360, 48)], bends: Vec::new() })));
        h.send(Command::Play);
        run(&mut eng, 384 * 250 + 24 * 250 + 2_000);
        let log = log.lock().unwrap();
        let on = log.iter().find(|(_, e)| matches!(e, Event::NoteOn { .. })).unwrap().0;
        let off = log.iter().find(|(_, e)| matches!(e, Event::NoteOff { .. })).unwrap().0;
        assert!((11_990..=12_010).contains(&(off - on)), "got {}", off - on);
    }
}
