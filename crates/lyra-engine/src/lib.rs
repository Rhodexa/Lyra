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

use lyra_plugin::{Event, Plugin, ProcessContext, TimedEvent};
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

/// Ticks per quarter note. All musical time is in ticks.
pub const PPQ: u32 = 96;
/// A step = one 16th note.
pub const TICKS_PER_STEP: u32 = PPQ / 4;
pub const MAX_CHANNELS: usize = 128;
/// The engine renders in slices of at most this many frames.
pub const BLOCK: usize = 256;
const MAX_EVENTS_PER_BLOCK: usize = 256;
const MAX_PENDING_LIVE: usize = 64;
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
#[derive(Clone, Debug, Default)]
pub struct Sequence {
    /// Loop length in ticks (0 = nothing to play).
    pub length: u32,
    pub notes: Vec<SeqNote>,
}

/// One instrument + its mixer settings, as the engine owns it.
/// Built on the UI thread with [`ChannelSlot::new`], then sent over.
pub struct ChannelSlot {
    plugin: Box<dyn Plugin>,
    events: Vec<TimedEvent>,
    pending: Vec<Event>,
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    volume: f32,
    pan: f32,
    mute: bool,
    choke: u8,
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
            buf_l: vec![0.0; BLOCK],
            buf_r: vec![0.0; BLOCK],
            volume: 0.8,
            pan: 0.0,
            mute: false,
            choke: 0,
            gain_l: 0.0,
            gain_r: 0.0,
        })
    }
}

pub enum Command {
    Play,
    Stop,
    SetTempo(f64),
    /// 0 = straight, 1 = full triplet shuffle.
    SetSwing(f32),
    SetMasterVolume(f32),
    SetSequence(Box<Sequence>),
    AddChannel(Box<ChannelSlot>),
    ClearChannels,
    SetParam { channel: usize, id: u32, value: f32 },
    /// `choke`: channels sharing a non-zero group cut each other off (closed hat kills open hat).
    SetMix { channel: usize, volume: f32, pan: f32, mute: bool, choke: u8 },
    /// Notes played live (typing keyboard, MIDI, clicking a key).
    Live { channel: usize, event: Event },
}

enum Trash {
    Sequence(#[allow(dead_code)] Box<Sequence>),
    Channel(#[allow(dead_code)] Box<ChannelSlot>),
}

// ─────────────────────────── data coming back ──────────────────────────────

/// Facts the audio thread publishes for the UI. Lock-free, read any time.
pub struct Shared {
    playhead: AtomicU64,
    playing: AtomicBool,
    peaks: Vec<AtomicU32>,
    triggers: Vec<AtomicU32>,
    master_peak: [AtomicU32; 2],
}

impl Shared {
    fn new() -> Self {
        Self {
            playhead: AtomicU64::new(0f64.to_bits()),
            playing: AtomicBool::new(false),
            peaks: (0..MAX_CHANNELS).map(|_| AtomicU32::new(0)).collect(),
            triggers: (0..MAX_CHANNELS).map(|_| AtomicU32::new(0)).collect(),
            master_peak: [AtomicU32::new(0), AtomicU32::new(0)],
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

    channels: Vec<Box<ChannelSlot>>,
    seq: Box<Sequence>,

    playing: bool,
    /// Position inside the loop, in ticks.
    pos: f64,
    tempo: f64,
    swing: f32,
    master: f32,
    master_gain: f32,

    mix_l: Vec<f32>,
    mix_r: Vec<f32>,
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
        playing: false,
        pos: 0.0,
        tempo: 124.0,
        swing: 0.0,
        master: 0.8,
        master_gain: 0.8,
        mix_l: vec![0.0; BLOCK],
        mix_r: vec![0.0; BLOCK],
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
                    self.pos = 0.0;
                }
                Command::Stop => {
                    self.playing = false;
                    self.pos = 0.0;
                    for ch in &mut self.channels {
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
                Command::SetMix { channel, volume, pan, mute, choke } => {
                    if let Some(ch) = self.channels.get_mut(channel) {
                        ch.volume = volume;
                        ch.pan = pan;
                        ch.mute = mute;
                        ch.choke = choke;
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
        // 1. Gather this block's events: live notes first, then the sequencer.
        for ch in &mut self.channels {
            let ChannelSlot { events, pending, .. } = &mut **ch;
            events.clear();
            for e in pending.drain(..) {
                events.push(TimedEvent { offset: 0, event: e });
            }
        }
        if self.playing && self.seq.length > 0 {
            self.schedule(n);
        }
        self.apply_chokes();

        // 2. Run every instrument, apply volume/pan, sum into the mix.
        self.mix_l[..n].fill(0.0);
        self.mix_r[..n].fill(0.0);
        for (i, ch) in self.channels.iter_mut().enumerate() {
            let ChannelSlot { plugin, events, buf_l, buf_r, .. } = &mut **ch;
            // AllNotesOff sorts before a NoteOn at the same sample, so a choke
            // never kills the note that's starting right then. (In-place sort: no allocation.)
            events.sort_unstable_by_key(|e| {
                (e.offset, !matches!(e.event, Event::AllNotesOff))
            });
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
                self.mix_l[s] += l;
                self.mix_r[s] += r;
            }
            raise_peak(&self.shared.peaks[i], peak);
        }

        // 3. Master volume + a hard safety clamp (protects ears and speakers).
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
        // Swing pushes every odd 16th late, up to a third of a step (triplet feel).
        let swing_ticks = self.swing as f64 * TICKS_PER_STEP as f64 / 3.0;

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
            let on = note.start as f64
                + if (note.start / TICKS_PER_STEP) % 2 == 1 { swing_ticks } else { 0.0 };
            let off = (on + note.len as f64) % len;

            if let Some(o) = offset_of(on) {
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
            if let Some(o) = offset_of(off) {
                if ch.events.len() < ch.events.capacity() {
                    ch.events.push(TimedEvent { offset: o, event: Event::NoteOff { key: note.key } });
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
