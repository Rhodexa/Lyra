//! Built-in mixer effects. Every one implements [`Plugin`] and processes **in
//! place** (the buffers arrive holding the input). Delay lines are allocated in
//! `activate()` (UI thread), never in `process()`.
//!
//! | id | what it does |
//! |---|---|
//! | `lyra.eq` | low cut, low shelf, a mid bell, high shelf |
//! | `lyra.filter` | sweepable low/high-pass |
//! | `lyra.compressor` | stereo-linked compressor |
//! | `lyra.delay` | tempo-synced echo, optional ping-pong |
//! | `lyra.reverb` | Freeverb-style room |
//! | `lyra.pump` | beat-synced ducking: the house "sidechain" breathing |
//! | `lyra.limiter` | look-ahead brickwall: nothing passes the ceiling (1.5 ms latency) |
//! | `lyra.chorus` | two modulated delays, a quarter cycle apart: width and shimmer |
//! | `lyra.drive` | saturation: soft, hard, fold, or bit-crush, with a tone control |

use lyra_dsp::{tau_coeff, Biquad, FilterKind};
use lyra_plugin::{Curve, ParamInfo, Plugin, PluginInfo, ProcessContext, Transport};
use std::f32::consts::{PI, TAU};

use Curve::{Exponential as Exp, Linear as Lin};

/// The effects, as Lyra lists them.
pub const PLUGINS: &[PluginInfo] = &[
    effect("lyra.eq", "EQ", &EQ_PARAMS, || Box::new(Eq::new())),
    effect("lyra.filter", "Filter", &FILTER_PARAMS, || Box::new(Filter::new())),
    effect("lyra.compressor", "Compressor", &COMP_PARAMS, || Box::new(Comp::new())),
    effect("lyra.delay", "Delay", &DELAY_PARAMS, || Box::new(Delay::new())),
    effect("lyra.reverb", "Reverb", &REVERB_PARAMS, || Box::new(Reverb::new())),
    effect("lyra.pump", "Pump", &PUMP_PARAMS, || Box::new(Pump::new())),
    effect("lyra.limiter", "Limiter", &LIMITER_PARAMS, || Box::new(Limiter::new())),
    effect("lyra.chorus", "Chorus", &CHORUS_PARAMS, || Box::new(Chorus::new())),
    effect("lyra.drive", "Drive", &DRIVE_PARAMS, || Box::new(Drive::new())),
];

const fn effect(
    id: &'static str,
    name: &'static str,
    params: &'static [ParamInfo],
    create: fn() -> Box<dyn Plugin>,
) -> PluginInfo {
    PluginInfo { id, name, melodic: false, params, groups: &[], presets: &[], create }
}

fn db(v: f32) -> f32 {
    lyra_plugin::db_to_gain(v)
}

/// Shared boilerplate: parameter storage + the Plugin methods that don't vary.
macro_rules! effect_plugin {
    ($ty:ty, $name:literal, $params:ident) => {
        impl Plugin for $ty {
            fn name(&self) -> &'static str {
                $name
            }
            fn params(&self) -> &'static [ParamInfo] {
                &$params
            }
            fn set_param(&mut self, id: u32, value: f32) {
                if let Some(p) = self.p.get_mut(id as usize) {
                    *p = value;
                }
                self.dirty = true;
            }
            fn activate(&mut self, sample_rate: f32) {
                self.sr = sample_rate;
                self.prepare();
                self.dirty = true;
            }
            fn process(&mut self, ctx: ProcessContext<'_>) {
                if self.dirty {
                    self.update();
                    self.dirty = false;
                }
                self.run(ctx.out_l, ctx.out_r);
            }
            fn transport(&mut self, t: Transport) {
                self.on_transport(t);
            }
        }
    };
}

// ─────────────────────────────── EQ ───────────────────────────────

/// RBJ shelves and bells (the util Biquad only has LP/HP/BP).
fn rbj(sr: f32, kind: u8, freq: f32, q: f32, gain_db: f32) -> [f32; 5] {
    let a = 10f32.powf(gain_db / 40.0);
    let w = TAU * freq.clamp(10.0, sr * 0.45) / sr;
    let (sin, cos) = w.sin_cos();
    let alpha = sin / (2.0 * q);
    let (b0, b1, b2, a0, a1, a2) = match kind {
        0 => {
            // low shelf
            let s = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cos + s),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - s),
                (a + 1.0) + (a - 1.0) * cos + s,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                (a + 1.0) + (a - 1.0) * cos - s,
            )
        }
        1 => {
            // high shelf
            let s = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cos + s),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - s),
                (a + 1.0) - (a - 1.0) * cos + s,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - s,
            )
        }
        _ => (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a),
    };
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

#[derive(Clone, Copy, Default)]
struct Section {
    c: [f32; 5],
    z: [[f32; 2]; 2],
}

impl Section {
    #[inline]
    fn run(&mut self, ch: usize, x: f32) -> f32 {
        let [b0, b1, b2, a1, a2] = self.c;
        let z = &mut self.z[ch];
        let y = b0 * x + z[0];
        z[0] = b1 * x - a1 * y + z[1];
        z[1] = b2 * x - a2 * y;
        y
    }
}

pub static EQ_PARAMS: [ParamInfo; 5] = [
    ParamInfo::new(0, "Low cut", 20.0, 600.0, 20.0, "Hz", Exp),
    ParamInfo::new(1, "Low", -18.0, 18.0, 0.0, "dB", Lin),
    ParamInfo::new(2, "Mid", -18.0, 18.0, 0.0, "dB", Lin),
    ParamInfo::new(3, "Mid freq", 150.0, 8000.0, 1000.0, "Hz", Exp),
    ParamInfo::new(4, "High", -18.0, 18.0, 0.0, "dB", Lin),
];

pub struct Eq {
    p: Vec<f32>,
    sr: f32,
    dirty: bool,
    cut: [Biquad; 2],
    bands: [Section; 3],
}

impl Eq {
    fn new() -> Self {
        Self {
            p: lyra_plugin::default_params(&EQ_PARAMS),
            sr: 48_000.0,
            dirty: true,
            cut: [Biquad::default(); 2],
            bands: [Section::default(); 3],
        }
    }
    fn prepare(&mut self) {}
    fn update(&mut self) {
        let (sr, p) = (self.sr, &self.p);
        for c in &mut self.cut {
            c.set(FilterKind::HighPass, sr, p[0], 0.7);
        }
        self.bands[0].c = rbj(sr, 0, 120.0, 0.7, p[1]);
        self.bands[1].c = rbj(sr, 2, p[3], 0.9, p[2]);
        self.bands[2].c = rbj(sr, 1, 8000.0, 0.7, p[4]);
    }
    fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let cut_on = self.p[0] > 21.0;
        for (ch, buf) in [l, r].into_iter().enumerate() {
            for x in buf.iter_mut() {
                let mut y = if cut_on { self.cut[ch].run(*x) } else { *x };
                for b in &mut self.bands {
                    y = b.run(ch, y);
                }
                *x = y;
            }
        }
    }
    fn on_transport(&mut self, _: Transport) {}
}
effect_plugin!(Eq, "EQ", EQ_PARAMS);

// ─────────────────────────────── Filter ───────────────────────────────

pub static FILTER_PARAMS: [ParamInfo; 3] = [
    ParamInfo::choice(0, "Mode", &["Low-pass", "High-pass"], 0),
    ParamInfo::new(1, "Cutoff", 20.0, 20_000.0, 20_000.0, "Hz", Exp),
    ParamInfo::new(2, "Reso", 0.0, 1.0, 0.15, "%", Lin),
];

pub struct Filter {
    p: Vec<f32>,
    sr: f32,
    dirty: bool,
    /// Smoothed cutoff, so sweeping the knob never zippers.
    cutoff: f32,
    s: [[f32; 2]; 2],
}

impl Filter {
    fn new() -> Self {
        Self {
            p: lyra_plugin::default_params(&FILTER_PARAMS),
            sr: 48_000.0,
            dirty: true,
            cutoff: 20_000.0,
            s: [[0.0; 2]; 2],
        }
    }
    fn prepare(&mut self) {}
    fn update(&mut self) {}
    fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let hp = self.p[0] >= 0.5;
        let k = 2.0 - 1.96 * self.p[2];
        let target = self.p[1].clamp(20.0, self.sr * 0.45);
        for i in 0..l.len() {
            self.cutoff += (target - self.cutoff) * 0.002;
            let g = (PI * self.cutoff / self.sr).tan();
            let a1 = 1.0 / (1.0 + g * (g + k));
            let (a2, a3) = (g * a1, g * g * a1);
            for (ch, x) in [&mut l[i], &mut r[i]].into_iter().enumerate() {
                let [ic1, ic2] = &mut self.s[ch];
                let v3 = *x - *ic2;
                let v1 = a1 * *ic1 + a2 * v3;
                let v2 = *ic2 + a2 * *ic1 + a3 * v3;
                *ic1 = 2.0 * v1 - *ic1;
                *ic2 = 2.0 * v2 - *ic2;
                *x = if hp { *x - k * v1 - v2 } else { v2 };
            }
        }
    }
    fn on_transport(&mut self, _: Transport) {}
}
effect_plugin!(Filter, "Filter", FILTER_PARAMS);

// ─────────────────────────────── Compressor ───────────────────────────────

pub static COMP_PARAMS: [ParamInfo; 5] = [
    ParamInfo::new(0, "Threshold", -40.0, 0.0, -14.0, "dB", Lin),
    ParamInfo::new(1, "Ratio", 1.0, 20.0, 4.0, "", Exp),
    ParamInfo::new(2, "Attack", 0.0002, 0.1, 0.005, "s", Exp),
    ParamInfo::new(3, "Release", 0.01, 1.0, 0.15, "s", Exp),
    ParamInfo::new(4, "Makeup", 0.0, 24.0, 3.0, "dB", Lin),
];

pub struct Comp {
    p: Vec<f32>,
    sr: f32,
    dirty: bool,
    env: f32,
    att: f32,
    rel: f32,
}

impl Comp {
    fn new() -> Self {
        Self { p: lyra_plugin::default_params(&COMP_PARAMS), sr: 48_000.0, dirty: true, env: 0.0, att: 0.0, rel: 0.0 }
    }
    fn prepare(&mut self) {}
    fn update(&mut self) {
        self.att = tau_coeff(self.p[2], self.sr);
        self.rel = tau_coeff(self.p[3], self.sr);
    }
    fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let (threshold, ratio, makeup) = (self.p[0], self.p[1], db(self.p[4]));
        for i in 0..l.len() {
            let level = l[i].abs().max(r[i].abs());
            let k = if level > self.env { self.att } else { self.rel };
            self.env = level + (self.env - level) * k;
            let over = 20.0 * self.env.max(1e-6).log10() - threshold;
            let gain = if over > 0.0 { db(-over * (1.0 - 1.0 / ratio)) } else { 1.0 } * makeup;
            l[i] *= gain;
            r[i] *= gain;
        }
    }
    fn on_transport(&mut self, _: Transport) {}
}
effect_plugin!(Comp, "Compressor", COMP_PARAMS);

// ─────────────────────────────── Delay ───────────────────────────────

const DELAY_BEATS: [f32; 6] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0];

pub static DELAY_PARAMS: [ParamInfo; 5] = [
    ParamInfo::choice(0, "Time", &["1/16", "1/8", "3/16", "1/4", "3/8", "1/2"], 2),
    ParamInfo::new(1, "Feedback", 0.0, 0.9, 0.35, "%", Lin),
    ParamInfo::choice(2, "Ping-pong", &["Off", "On"], 1),
    ParamInfo::new(3, "Tone", 500.0, 16_000.0, 5000.0, "Hz", Exp),
    ParamInfo::new(4, "Mix", 0.0, 1.0, 0.25, "%", Lin),
];

pub struct Delay {
    p: Vec<f32>,
    sr: f32,
    dirty: bool,
    buf: [Vec<f32>; 2],
    write: usize,
    tempo: f64,
    /// Delay time in samples, smoothed so tempo changes glide instead of click.
    time: f32,
    tone: [Biquad; 2],
}

impl Delay {
    fn new() -> Self {
        Self {
            p: lyra_plugin::default_params(&DELAY_PARAMS),
            sr: 48_000.0,
            dirty: true,
            buf: [Vec::new(), Vec::new()],
            write: 0,
            tempo: 120.0,
            time: 0.0,
            tone: [Biquad::default(); 2],
        }
    }
    fn prepare(&mut self) {
        let len = (self.sr * 4.0) as usize; // up to 4 s (a 1/2 note at 30 bpm)
        self.buf = [vec![0.0; len], vec![0.0; len]];
        self.write = 0;
    }
    fn update(&mut self) {
        for t in &mut self.tone {
            t.set(FilterKind::LowPass, self.sr, self.p[3], 0.7);
        }
    }
    fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let len = self.buf[0].len();
        if len == 0 {
            return;
        }
        let beats = DELAY_BEATS[(self.p[0].round() as usize).min(5)];
        let target = (beats * 60.0 / self.tempo.max(20.0) as f32 * self.sr).min(len as f32 - 2.0);
        if self.time == 0.0 {
            self.time = target;
        }
        let (fb, mix, ping) = (self.p[1], self.p[4], self.p[2] >= 0.5);
        for i in 0..l.len() {
            self.time += (target - self.time) * 0.0005;
            let (i0, i1, frac) = ring_read(self.write, self.time, len);
            let tap = |b: &Vec<f32>| b[i0] + (b[i1] - b[i0]) * frac;
            let (dl, dr) = (self.tone[0].run(tap(&self.buf[0])), self.tone[1].run(tap(&self.buf[1])));
            let (inl, inr) = (l[i], r[i]);
            // Ping-pong: the input enters on the left, echoes swap sides each repeat.
            let (wl, wr) = if ping { (0.5 * (inl + inr) + dr * fb, dl * fb) } else { (inl + dl * fb, inr + dr * fb) };
            self.buf[0][self.write] = wl;
            self.buf[1][self.write] = wr;
            self.write = (self.write + 1) % len;
            // Mix adds echo on top; the dry signal stays at full level.
            l[i] = inl + dl * mix;
            r[i] = inr + dr * mix;
        }
    }
    fn on_transport(&mut self, t: Transport) {
        self.tempo = t.tempo;
    }
}
effect_plugin!(Delay, "Delay", DELAY_PARAMS);

/// Where to read a ring buffer `delay` samples behind `write`: the two
/// neighbouring slots and the blend between them.
///
/// The wrap is done in floats, and a position a hair below zero wraps to
/// `len - 0.0005`, which f32 can't represent near 192 000 and rounds to
/// exactly `len`: one past the end. That crashed the audio thread. Wrapping
/// the integer index too makes it land on slot 0, which is the right sample.
fn ring_read(write: usize, delay: f32, len: usize) -> (usize, usize, f32) {
    let read = (write as f32 - delay).rem_euclid(len as f32);
    let i0 = (read as usize) % len;
    (i0, (i0 + 1) % len, read.fract())
}

// ─────────────────────────────── Reverb ───────────────────────────────

/// Freeverb's tunings at 44.1 kHz; scaled to the real sample rate in `prepare`.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

pub static REVERB_PARAMS: [ParamInfo; 4] = [
    ParamInfo::new(0, "Size", 0.0, 1.0, 0.7, "%", Lin),
    ParamInfo::new(1, "Damp", 0.0, 1.0, 0.45, "%", Lin),
    ParamInfo::new(2, "Width", 0.0, 1.0, 1.0, "%", Lin),
    ParamInfo::new(3, "Mix", 0.0, 1.0, 0.22, "%", Lin),
];

struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

pub struct Reverb {
    p: Vec<f32>,
    sr: f32,
    dirty: bool,
    combs: [Vec<Comb>; 2],
    alls: [Vec<Allpass>; 2],
}

impl Reverb {
    fn new() -> Self {
        Self {
            p: lyra_plugin::default_params(&REVERB_PARAMS),
            sr: 48_000.0,
            dirty: true,
            combs: [Vec::new(), Vec::new()],
            alls: [Vec::new(), Vec::new()],
        }
    }
    fn prepare(&mut self) {
        let scale = self.sr / 44_100.0;
        for ch in 0..2 {
            let extra = ch * STEREO_SPREAD;
            self.combs[ch] = COMBS
                .iter()
                .map(|&n| Comb { buf: vec![0.0; ((n + extra) as f32 * scale) as usize], pos: 0, store: 0.0 })
                .collect();
            self.alls[ch] = ALLPASSES
                .iter()
                .map(|&n| Allpass { buf: vec![0.0; ((n + extra) as f32 * scale) as usize], pos: 0 })
                .collect();
        }
    }
    fn update(&mut self) {}
    fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let room = 0.7 + 0.28 * self.p[0];
        let damp = 0.4 * self.p[1];
        let (width, mix) = (self.p[2], self.p[3]);
        let (wet1, wet2) = (0.5 * (1.0 + width), 0.5 * (1.0 - width));
        for i in 0..l.len() {
            let input = (l[i] + r[i]) * 0.015;
            let mut out = [0.0f32; 2];
            for ((o, combs), alls) in out.iter_mut().zip(&mut self.combs).zip(&mut self.alls) {
                for c in combs.iter_mut() {
                    let y = c.buf[c.pos];
                    c.store = y * (1.0 - damp) + c.store * damp;
                    c.buf[c.pos] = input + c.store * room;
                    c.pos = (c.pos + 1) % c.buf.len();
                    *o += y;
                }
                for a in alls.iter_mut() {
                    let b = a.buf[a.pos];
                    a.buf[a.pos] = *o + b * 0.5;
                    a.pos = (a.pos + 1) % a.buf.len();
                    *o = b - *o;
                }
            }
            let (wl, wr) = (out[0] * wet1 + out[1] * wet2, out[1] * wet1 + out[0] * wet2);
            l[i] += wl * 3.0 * mix;
            r[i] += wr * 3.0 * mix;
        }
    }
    fn on_transport(&mut self, _: Transport) {}
}
effect_plugin!(Reverb, "Reverb", REVERB_PARAMS);

// ─────────────────────────────── Pump ───────────────────────────────

const PUMP_BEATS: [f64; 3] = [1.0, 0.5, 2.0];

pub static PUMP_PARAMS: [ParamInfo; 3] = [
    ParamInfo::new(0, "Depth", 0.0, 1.0, 0.6, "%", Lin),
    ParamInfo::new(1, "Recovery", 0.1, 1.0, 0.55, "%", Lin),
    ParamInfo::choice(2, "Rate", &["1/4", "1/8", "1/2"], 0),
];

/// Ducks on every beat and swells back: the "breathing" a kick-triggered
/// sidechain compressor gives a house track, but locked to the grid.
pub struct Pump {
    p: Vec<f32>,
    sr: f32,
    dirty: bool,
    t: Transport,
    gain: f32,
}

impl Pump {
    fn new() -> Self {
        Self {
            p: lyra_plugin::default_params(&PUMP_PARAMS),
            sr: 48_000.0,
            dirty: true,
            t: Transport::default(),
            gain: 1.0,
        }
    }
    fn prepare(&mut self) {}
    fn update(&mut self) {}
    /// Gain at phase 0..1 within one pump period.
    fn shape(&self, phase: f64) -> f32 {
        let (depth, recovery) = (self.p[0], self.p[1]);
        let x = (phase as f32 / recovery).min(1.0);
        1.0 - depth * (1.0 - x) * (1.0 - x)
    }
    fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let period = PUMP_BEATS[(self.p[2].round() as usize).min(2)] * 96.0;
        let ticks_per_sample = self.t.tempo * 96.0 / 60.0 / self.sr as f64;
        for i in 0..l.len() {
            let target = if self.t.playing {
                let pos = self.t.position + i as f64 * ticks_per_sample;
                self.shape((pos % period) / period)
            } else {
                1.0
            };
            self.gain += (target - self.gain) * 0.02; // de-click the duck
            l[i] *= self.gain;
            r[i] *= self.gain;
        }
    }
    fn on_transport(&mut self, t: Transport) {
        self.t = t;
    }
}
effect_plugin!(Pump, "Pump", PUMP_PARAMS);

// ─────────────────────────────── Limiter ───────────────────────────────

pub static LIMITER_PARAMS: [ParamInfo; 3] = [
    ParamInfo::new(0, "Gain", 0.0, 18.0, 0.0, "dB", Lin),
    ParamInfo::new(1, "Ceiling", -12.0, 0.0, -0.3, "dB", Lin),
    ParamInfo::new(2, "Release", 0.01, 1.0, 0.1, "s", Exp),
];

/// Look-ahead time: the signal is delayed by this much so the gain can be
/// down before a peak arrives.
const LOOKAHEAD_S: f32 = 0.0015;

/// A brickwall limiter. Each sample's required gain (ceiling / peak) goes
/// through a sliding minimum over the look-ahead window, then a moving
/// average over the same window: the average of values that are all at or
/// below a peak's gain is at or below it too, so by the time that peak leaves
/// the delay the gain is low enough, and it got there along a smooth ramp
/// instead of a click. Release then lets the gain back up gently.
pub struct Limiter {
    p: Vec<f32>,
    sr: f32,
    dirty: bool,
    /// Window length in samples (the delay is `len - 1`).
    len: usize,
    delay: [Vec<f32>; 2],
    at: usize,
    /// Sliding minimum: a monotonic queue of (sample index, gain) in a ring.
    q: Vec<(u64, f32)>,
    q_head: usize,
    q_len: usize,
    n: u64,
    /// Moving average of the minimum.
    avg: Vec<f32>,
    sum: f64,
    gain: f32,
    release_k: f32,
}

impl Limiter {
    fn new() -> Self {
        let mut l = Self {
            p: lyra_plugin::default_params(&LIMITER_PARAMS),
            sr: 48_000.0,
            dirty: true,
            len: 1,
            delay: [Vec::new(), Vec::new()],
            at: 0,
            q: Vec::new(),
            q_head: 0,
            q_len: 0,
            n: 0,
            avg: Vec::new(),
            sum: 0.0,
            gain: 1.0,
            release_k: 0.0,
        };
        l.prepare();
        l
    }
    fn prepare(&mut self) {
        self.len = ((LOOKAHEAD_S * self.sr) as usize).max(2);
        self.delay = [vec![0.0; self.len], vec![0.0; self.len]];
        self.q = vec![(0, 1.0); self.len + 1];
        self.avg = vec![1.0; self.len];
        self.sum = self.len as f64;
        (self.at, self.q_head, self.q_len, self.n, self.gain) = (0, 0, 0, 0, 1.0);
    }
    fn update(&mut self) {
        self.release_k = tau_coeff(self.p[2] / 4.6, self.sr); // ~99% back up over `release`
    }
    fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let input = db(self.p[0]);
        let ceiling = db(self.p[1]);
        let (len, cap) = (self.len, self.q.len());
        for i in 0..l.len() {
            let (xl, xr) = (l[i] * input, r[i] * input);
            let peak = xl.abs().max(xr.abs());
            let need = if peak > ceiling { ceiling / peak } else { 1.0 };
            // Sliding minimum of `need` over the last `len` samples.
            while self.q_len > 0 && self.q[(self.q_head + self.q_len - 1) % cap].1 >= need {
                self.q_len -= 1;
            }
            self.q[(self.q_head + self.q_len) % cap] = (self.n, need);
            self.q_len += 1;
            if self.q[self.q_head].0 + len as u64 <= self.n {
                self.q_head = (self.q_head + 1) % cap;
                self.q_len -= 1;
            }
            let min = self.q[self.q_head].1;
            // Moving average of the minimum over the same window.
            let slot = (self.n % len as u64) as usize;
            self.sum += (min - self.avg[slot]) as f64;
            self.avg[slot] = min;
            let smooth = (self.sum / len as f64) as f32;
            self.gain = if smooth < self.gain { smooth } else { smooth + (self.gain - smooth) * self.release_k };
            // The delayed sample leaves now (the window is `len`, the delay `len - 1`).
            let out = (self.at + 1) % len;
            let (dl, dr) = (self.delay[0][out], self.delay[1][out]);
            self.delay[0][self.at] = xl;
            self.delay[1][self.at] = xr;
            self.at = out;
            self.n += 1;
            // The guard is for float rounding only; the gain already did the work.
            l[i] = (dl * self.gain).clamp(-ceiling, ceiling);
            r[i] = (dr * self.gain).clamp(-ceiling, ceiling);
        }
    }
    fn on_transport(&mut self, _: Transport) {}
}
effect_plugin!(Limiter, "Limiter", LIMITER_PARAMS);

// ─────────────────────────────── Chorus ───────────────────────────────

pub static CHORUS_PARAMS: [ParamInfo; 4] = [
    ParamInfo::new(0, "Rate", 0.05, 5.0, 0.6, "Hz", Exp),
    ParamInfo::new(1, "Depth", 0.0, 1.0, 0.5, "%", Lin),
    ParamInfo::new(2, "Delay", 0.002, 0.03, 0.012, "s", Exp),
    ParamInfo::new(3, "Mix", 0.0, 1.0, 0.5, "%", Lin),
];

/// The most the delay swings either way, at full depth (seconds).
const CHORUS_SWING_S: f32 = 0.005;

/// One delay line per side, each swung by the same LFO a quarter cycle apart,
/// so even a mono sound comes out wide.
pub struct Chorus {
    p: Vec<f32>,
    sr: f32,
    dirty: bool,
    buf: [Vec<f32>; 2],
    write: usize,
    phase: f32,
}

impl Chorus {
    fn new() -> Self {
        let mut c = Self {
            p: lyra_plugin::default_params(&CHORUS_PARAMS),
            sr: 48_000.0,
            dirty: true,
            buf: [Vec::new(), Vec::new()],
            write: 0,
            phase: 0.0,
        };
        c.prepare();
        c
    }
    fn prepare(&mut self) {
        let len = ((0.03 + CHORUS_SWING_S) * self.sr) as usize + 4;
        self.buf = [vec![0.0; len], vec![0.0; len]];
        self.write = 0;
    }
    fn update(&mut self) {}
    fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let len = self.buf[0].len();
        let inc = self.p[0] / self.sr;
        let base = self.p[2] * self.sr;
        let swing = self.p[1] * CHORUS_SWING_S * self.sr;
        let (wet, dry) = (self.p[3], 1.0 - self.p[3] * 0.5);
        for i in 0..l.len() {
            self.phase = (self.phase + inc).fract();
            for (side, x) in [&mut l[i], &mut r[i]].into_iter().enumerate() {
                self.buf[side][self.write] = *x;
                let lfo = (TAU * (self.phase + 0.25 * side as f32)).sin();
                let d = (base + swing * lfo).clamp(1.0, (len - 2) as f32);
                let (i0, i1, frac) = ring_read(self.write, d, len);
                let delayed = self.buf[side][i0] + (self.buf[side][i1] - self.buf[side][i0]) * frac;
                *x = *x * dry + delayed * wet;
            }
            self.write = (self.write + 1) % len;
        }
    }
    fn on_transport(&mut self, _: Transport) {}
}
effect_plugin!(Chorus, "Chorus", CHORUS_PARAMS);

// ─────────────────────────────── Drive ───────────────────────────────

pub static DRIVE_PARAMS: [ParamInfo; 5] = [
    ParamInfo::choice(0, "Shape", &["Soft", "Hard", "Fold", "Crush"], 0),
    ParamInfo::new(1, "Drive", 0.0, 36.0, 12.0, "dB", Lin),
    ParamInfo::new(2, "Tone", 500.0, 20_000.0, 12_000.0, "Hz", Exp),
    ParamInfo::new(3, "Mix", 0.0, 1.0, 1.0, "%", Lin),
    ParamInfo::new(4, "Output", -24.0, 6.0, -6.0, "dB", Lin),
];

/// Saturation. Soft is tanh, hard clips at ±1, fold mirrors the wave back
/// (sin), crush keeps fewer bits the harder you drive (16 down to 3). Then a
/// gentle low-pass ("Tone") tames the fizz, and Mix blends the dry signal.
/// No oversampling: the hard shapes alias a little, which suits grit.
pub struct Drive {
    p: Vec<f32>,
    sr: f32,
    dirty: bool,
    tone: [f32; 2],
    tone_k: f32,
}

impl Drive {
    fn new() -> Self {
        Self { p: lyra_plugin::default_params(&DRIVE_PARAMS), sr: 48_000.0, dirty: true, tone: [0.0; 2], tone_k: 0.0 }
    }
    fn prepare(&mut self) {}
    fn update(&mut self) {
        // Fully open = no filter at all (a crushed signal stays exactly stepped).
        self.tone_k =
            if self.p[2] >= 19_999.0 { 1.0 } else { 1.0 - (-TAU * self.p[2].min(self.sr * 0.45) / self.sr).exp() };
    }
    fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let shape = self.p[0].round() as u8;
        let drive = db(self.p[1]);
        let (mix, out) = (self.p[3], db(self.p[4]));
        // Crush: 16 bits at no drive, 3 at full.
        let steps = 2f32.powf(16.0 - 13.0 * self.p[1] / 36.0) * 0.5;
        for i in 0..l.len() {
            for (side, x) in [&mut l[i], &mut r[i]].into_iter().enumerate() {
                let d = *x * drive;
                let y = match shape {
                    1 => d.clamp(-1.0, 1.0),
                    2 => (d * std::f32::consts::FRAC_PI_2).sin(),
                    3 => (*x * steps).round() / steps,
                    _ => d.tanh(),
                };
                self.tone[side] += (y - self.tone[side]) * self.tone_k;
                *x = *x * (1.0 - mix) + self.tone[side] * out * mix;
            }
        }
    }
    fn on_transport(&mut self, _: Transport) {}
}
effect_plugin!(Drive, "Drive", DRIVE_PARAMS);

#[cfg(test)]
mod tests {
    use super::*;

    fn create(id: &str) -> Option<Box<dyn Plugin>> {
        PLUGINS.iter().find(|e| e.id == id).map(|e| (e.create)())
    }

    #[test]
    fn ring_read_never_leaves_the_buffer() {
        // The crash: write at 0, a delay a hair above zero, a 4 s buffer at 48 kHz.
        let len = 192_000;
        for delay in [0.0005f32, 0.001, 0.004, 1e-6, 0.0] {
            let (i0, i1, frac) = ring_read(0, delay, len);
            assert!(i0 < len && i1 < len, "delay {delay}: {i0} {i1}");
            assert!((0.0..1.0).contains(&frac));
        }
        // And everywhere else, for a sweep of positions and delays.
        for write in (0..len).step_by(997) {
            for delay in [0.25f32, 1.5, 4800.75, 191_998.9] {
                let (i0, i1, _) = ring_read(write, delay, len);
                assert!(i0 < len && i1 < len);
            }
        }
    }

    fn render(fx: &mut dyn Plugin, input: impl Fn(usize) -> f32, seconds: f32) -> Vec<f32> {
        let sr = 48_000.0;
        let mut out = Vec::new();
        let mut n = 0;
        while (out.len() as f32) < seconds * sr {
            let mut l: Vec<f32> = (0..256).map(|i| input(n + i)).collect();
            let mut r = l.clone();
            fx.transport(Transport {
                tempo: 120.0,
                playing: true,
                position: n as f64 * 120.0 * 96.0 / 60.0 / sr as f64,
            });
            fx.process(ProcessContext { sample_rate: sr, events: &[], out_l: &mut l, out_r: &mut r });
            out.extend(l.iter().zip(&r).map(|(a, b)| a.abs().max(b.abs())));
            n += 256;
        }
        out
    }

    #[test]
    fn the_limiter_never_passes_the_ceiling() {
        let mut fx = create("lyra.limiter").unwrap();
        fx.activate(48_000.0);
        fx.set_param(0, 12.0); // push a loud signal 12 dB harder
        fx.set_param(1, -1.0);
        // A loud sine with sudden spikes, the worst case for a look-ahead.
        let input = |n: usize| (n as f32 * 0.05).sin() * 0.9 + if n.is_multiple_of(4801) { 3.0 } else { 0.0 };
        let out = render(fx.as_mut(), input, 2.0);
        let ceiling = lyra_plugin::db_to_gain(-1.0);
        let peak = out.iter().fold(0.0f32, |m, x| m.max(*x));
        assert!(peak <= ceiling + 1e-6, "peak {peak} over the ceiling {ceiling}");
        assert!(peak > ceiling * 0.9, "and it still gets loud: {peak}");
    }

    #[test]
    fn the_limiter_leaves_quiet_audio_alone() {
        let mut fx = create("lyra.limiter").unwrap();
        fx.activate(48_000.0);
        let input = |n: usize| (n as f32 * 0.05).sin() * 0.5;
        let out = render(fx.as_mut(), input, 0.5);
        let lat = (LOOKAHEAD_S * 48_000.0) as usize - 1;
        for (n, y) in out.iter().enumerate().take(20_000).skip(1000) {
            assert!((y - input(n - lat).abs()).abs() < 1e-6, "sample {n}");
        }
    }

    #[test]
    fn chorus_widens_a_mono_sound() {
        let mut fx = create("lyra.chorus").unwrap();
        fx.activate(48_000.0);
        let mut l: Vec<f32> = (0..48_000).map(|n| (n as f32 * 0.04).sin()).collect();
        let mut r = l.clone();
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            fx.process(ProcessContext { sample_rate: 48_000.0, events: &[], out_l: cl, out_r: cr });
        }
        let diff = l.iter().zip(&r).skip(4800).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(diff > 0.1, "left and right should differ, max {diff}");
    }

    #[test]
    fn crush_keeps_few_levels_at_full_drive() {
        let mut fx = create("lyra.drive").unwrap();
        fx.activate(48_000.0);
        fx.set_param(0, 3.0); // Crush
        fx.set_param(1, 36.0); // 3 bits
        fx.set_param(2, 20_000.0);
        fx.set_param(4, 0.0);
        let mut l: Vec<f32> = (0..4800).map(|n| (n as f32 * 0.01).sin() * 0.9).collect();
        let mut r = l.clone();
        fx.process(ProcessContext { sample_rate: 48_000.0, events: &[], out_l: &mut l, out_r: &mut r });
        let mut levels: Vec<i32> = l.iter().map(|x| (x * 1000.0).round() as i32).collect();
        levels.sort();
        levels.dedup();
        assert!(levels.len() <= 9, "3 bits is at most 9 levels here, got {}", levels.len());
    }

    #[test]
    fn every_effect_is_stable_and_passes_audio() {
        for e in PLUGINS {
            let mut fx = create(e.id).unwrap();
            fx.activate(48_000.0);
            let out = render(fx.as_mut(), |n| (n as f32 * 0.03).sin() * 0.5, 1.0);
            let peak = out.iter().fold(0.0f32, |m, x| m.max(*x));
            assert!(out.iter().all(|x| x.is_finite()), "{}: NaN", e.name);
            assert!(peak > 0.05 && peak < 2.0, "{}: peak {peak}", e.name);
            assert_eq!(fx.params().len(), e.params.len());
        }
    }

    #[test]
    fn delay_echoes_after_the_synced_time() {
        let mut fx = create("lyra.delay").unwrap();
        fx.activate(48_000.0);
        fx.set_param(0, 3.0); // 1/4 note at 120 bpm = 0.5 s
        fx.set_param(2, 0.0); // no ping-pong
        fx.set_param(4, 1.0);
        let out = render(fx.as_mut(), |n| if n < 48 { 1.0 } else { 0.0 }, 1.0);
        let echo = out[20_000..].iter().position(|x| *x > 0.1).map(|i| i + 20_000).expect("an echo");
        assert!((echo as i64 - 24_000).abs() < 200, "echo at sample {echo}, expected ~24000");
    }

    #[test]
    fn pump_ducks_on_the_beat_and_recovers() {
        let mut fx = create("lyra.pump").unwrap();
        fx.activate(48_000.0);
        let out = render(fx.as_mut(), |_| 0.5, 2.0);
        let beat = 24_000; // 120 bpm
        let at_beat = out[beat + 500];
        let mid_beat = out[beat + beat * 3 / 4];
        assert!(at_beat < 0.35 && mid_beat > 0.48, "ducked {at_beat}, recovered {mid_beat}");
    }
}
