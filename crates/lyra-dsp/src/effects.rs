//! Built-in mixer effects. Every one implements [`Plugin`] and processes **in
//! place** (the buffers arrive holding the input). Delay lines are allocated in
//! `activate()` (UI thread), never in `process()`.
//!
//! | id | what it does |
//! |---|---|
//! | `lyra.eq` | low cut, low shelf, a mid bell, high shelf |
//! | `lyra.filter` | sweepable low/high-pass |
//! | `lyra.comp` | stereo-linked compressor |
//! | `lyra.delay` | tempo-synced echo, optional ping-pong |
//! | `lyra.reverb` | Freeverb-style room |
//! | `lyra.pump` | beat-synced ducking: the house "sidechain" breathing |

use crate::util::{tau_coeff, Biquad, FilterKind};
use lyra_plugin::{Curve, ParamGroup, ParamInfo, Plugin, ProcessContext, Transport};
use std::f32::consts::{PI, TAU};

use Curve::{Exponential as Exp, Linear as Lin};

pub struct EffectInfo {
    /// Stable id saved in project files. Never rename one.
    pub id: &'static str,
    pub name: &'static str,
}

pub const EFFECTS: &[EffectInfo] = &[
    EffectInfo { id: "lyra.eq", name: "EQ" },
    EffectInfo { id: "lyra.filter", name: "Filter" },
    EffectInfo { id: "lyra.comp", name: "Compressor" },
    EffectInfo { id: "lyra.delay", name: "Delay" },
    EffectInfo { id: "lyra.reverb", name: "Reverb" },
    EffectInfo { id: "lyra.pump", name: "Pump" },
];

pub fn info(id: &str) -> Option<&'static EffectInfo> {
    EFFECTS.iter().find(|e| e.id == id)
}

/// Build an effect by id. UI thread (it allocates).
pub fn create(id: &str) -> Option<Box<dyn Plugin>> {
    Some(match id {
        "lyra.eq" => Box::new(Eq::new()),
        "lyra.filter" => Box::new(Filter::new()),
        "lyra.comp" => Box::new(Comp::new()),
        "lyra.delay" => Box::new(Delay::new()),
        "lyra.reverb" => Box::new(Reverb::new()),
        "lyra.pump" => Box::new(Pump::new()),
        _ => return None,
    })
}

/// Parameter table for an effect id — static, no instance needed.
pub fn params(id: &str) -> &'static [ParamInfo] {
    match id {
        "lyra.eq" => &EQ_PARAMS,
        "lyra.filter" => &FILTER_PARAMS,
        "lyra.comp" => &COMP_PARAMS,
        "lyra.delay" => &DELAY_PARAMS,
        "lyra.reverb" => &REVERB_PARAMS,
        "lyra.pump" => &PUMP_PARAMS,
        _ => &[],
    }
}

pub fn default_params(id: &str) -> Vec<f32> {
    lyra_plugin::default_params(params(id))
}

fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
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

static EQ_PARAMS: [ParamInfo; 5] = [
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
            p: default_params("lyra.eq"),
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

static FILTER_PARAMS: [ParamInfo; 3] = [
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
        Self { p: default_params("lyra.filter"), sr: 48_000.0, dirty: true, cutoff: 20_000.0, s: [[0.0; 2]; 2] }
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

static COMP_PARAMS: [ParamInfo; 5] = [
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
        Self { p: default_params("lyra.comp"), sr: 48_000.0, dirty: true, env: 0.0, att: 0.0, rel: 0.0 }
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

static DELAY_PARAMS: [ParamInfo; 5] = [
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
            p: default_params("lyra.delay"),
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
            let read = (self.write as f32 - self.time).rem_euclid(len as f32);
            let (i0, frac) = (read as usize, read.fract());
            let i1 = (i0 + 1) % len;
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

// ─────────────────────────────── Reverb ───────────────────────────────

/// Freeverb's tunings at 44.1 kHz; scaled to the real sample rate in `prepare`.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

static REVERB_PARAMS: [ParamInfo; 4] = [
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
            p: default_params("lyra.reverb"),
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

static PUMP_PARAMS: [ParamInfo; 3] = [
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
        Self { p: default_params("lyra.pump"), sr: 48_000.0, dirty: true, t: Transport::default(), gain: 1.0 }
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

/// Effects show as one flat group in the editor.
pub fn groups(_id: &str) -> &'static [ParamGroup] {
    &[]
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn every_effect_is_stable_and_passes_audio() {
        for e in EFFECTS {
            let mut fx = create(e.id).unwrap();
            fx.activate(48_000.0);
            let out = render(fx.as_mut(), |n| (n as f32 * 0.03).sin() * 0.5, 1.0);
            let peak = out.iter().fold(0.0f32, |m, x| m.max(*x));
            assert!(out.iter().all(|x| x.is_finite()), "{}: NaN", e.name);
            assert!(peak > 0.05 && peak < 2.0, "{}: peak {peak}", e.name);
            assert_eq!(fx.params().len(), params(e.id).len());
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
