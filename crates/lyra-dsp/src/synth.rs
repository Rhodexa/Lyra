//! "Lyra Synth": a polyphonic subtractive synth for basses, stabs, plucks and pads.
//!
//! ```text
//!  per voice:  osc ×(1..7 unison, detuned + spread) ─┐
//!              sub sine (−1 octave) ─────────────────┼─► SVF low-pass ─► amp ADSR ─► out L/R
//!                       filter envelope ─► cutoff ──┘
//! ```
//! * Saw/square are band-limited with polyBLEP (no fizzy aliasing up high).
//! * The filter is a TPT state-variable filter: stable when swept fast.
//! * 16 voices; when all are busy, the quietest released (else oldest) voice is reused.

use crate::util::*;
use lyra_plugin::{Curve, Event, ParamGroup, ParamInfo, Plugin, ProcessContext};
use std::f32::consts::{FRAC_PI_4, PI, SQRT_2, TAU};

const MAX_VOICES: usize = 16;
const MAX_UNISON: usize = 7;
/// Filter coefficients are recomputed every this many samples.
const CONTROL_RATE: u32 = 8;

use Curve::{Exponential as Exp, Linear as Lin};

pub mod p {
    pub const WAVE: usize = 0;
    pub const OCTAVE: usize = 1;
    pub const VOICES: usize = 2;
    pub const DETUNE: usize = 3;
    pub const SPREAD: usize = 4;
    pub const SUB: usize = 5;
    pub const CUTOFF: usize = 6;
    pub const RESO: usize = 7;
    pub const ENV_AMT: usize = 8;
    pub const F_DECAY: usize = 9;
    pub const F_SUSTAIN: usize = 10;
    pub const ATTACK: usize = 11;
    pub const DECAY: usize = 12;
    pub const SUSTAIN: usize = 13;
    pub const RELEASE: usize = 14;
    pub const KEYTRACK: usize = 15;
    pub const LEVEL: usize = 16;
}

pub static SYNTH_PARAMS: [ParamInfo; 17] = [
    ParamInfo::choice(0, "Wave", &["Saw", "Square", "Triangle", "Sine"], 0),
    ParamInfo::choice(1, "Octave", &["-2", "-1", "0", "+1", "+2"], 2),
    ParamInfo::choice(2, "Voices", &["1", "2", "3", "4", "5", "6", "7"], 0),
    ParamInfo::new(3, "Detune", 0.0, 1.0, 0.25, "%", Lin),
    ParamInfo::new(4, "Spread", 0.0, 1.0, 0.6, "%", Lin),
    ParamInfo::new(5, "Sub", 0.0, 1.0, 0.0, "%", Lin),
    ParamInfo::new(6, "Cutoff", 40.0, 18_000.0, 2400.0, "Hz", Exp),
    ParamInfo::new(7, "Reso", 0.0, 1.0, 0.2, "%", Lin),
    ParamInfo::new(8, "Env amt", 0.0, 1.0, 0.35, "%", Lin),
    ParamInfo::new(9, "F. decay", 0.01, 4.0, 0.35, "s", Exp),
    ParamInfo::new(10, "F. sustain", 0.0, 1.0, 0.2, "%", Lin),
    ParamInfo::new(11, "Attack", 0.001, 4.0, 0.003, "s", Exp),
    ParamInfo::new(12, "Decay", 0.01, 4.0, 0.4, "s", Exp),
    ParamInfo::new(13, "Sustain", 0.0, 1.0, 0.7, "%", Lin),
    ParamInfo::new(14, "Release", 0.005, 6.0, 0.18, "s", Exp),
    ParamInfo::new(15, "Key track", 0.0, 1.0, 0.4, "%", Lin),
    ParamInfo::new(16, "Level", 0.0, 1.5, 0.7, "%", Lin),
];

static SYNTH_GROUPS: [ParamGroup; 4] = [
    ParamGroup { name: "Oscillator", ids: 0..6 },
    ParamGroup { name: "Filter", ids: 6..11 },
    ParamGroup { name: "Amp", ids: 11..15 },
    ParamGroup { name: "Output", ids: 15..17 },
];

#[derive(Clone, Copy, Default, PartialEq)]
enum Stage {
    #[default]
    Off,
    Attack,
    Decay,
    Release,
}

/// Topology-preserving-transform state-variable filter (low-pass output).
#[derive(Clone, Copy, Default)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    #[inline]
    fn run(&mut self, v0: f32, a1: f32, a2: f32, a3: f32) -> f32 {
        let v3 = v0 - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        v2
    }
}

#[derive(Clone, Copy, Default)]
struct Voice {
    stage: Stage,
    key: u8,
    velocity: f32,
    /// Note-on order, for stealing the oldest voice.
    serial: u64,

    unison: usize,
    phase: [f32; MAX_UNISON],
    inc: [f32; MAX_UNISON],
    gain_l: [f32; MAX_UNISON],
    gain_r: [f32; MAX_UNISON],
    sub_phase: f32,
    sub_inc: f32,

    // Amp envelope
    level: f32,
    attack_inc: f32,
    decay_k: f32,
    sustain: f32,
    release_k: f32,

    // Filter
    fenv: f32,
    fdecay_k: f32,
    fsustain: f32,
    base_cutoff: f32,
    env_octaves: f32,
    k: f32,
    a: (f32, f32, f32),
    tick: u32,
    left: Svf,
    right: Svf,
}

pub struct Synth {
    p: Vec<f32>,
    sr: f32,
    voices: [Voice; MAX_VOICES],
    serial: u64,
    rng: Rng,
}

impl Default for Synth {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

#[inline]
fn oscillator(wave: u32, t: f32, dt: f32) -> f32 {
    match wave {
        0 => 2.0 * t - 1.0 - poly_blep(t, dt),
        1 => {
            let sq = if t < 0.5 { 1.0 } else { -1.0 };
            sq + poly_blep(t, dt) - poly_blep((t + 0.5).fract(), dt)
        }
        2 => 4.0 * (t - 0.5).abs() - 1.0,
        _ => (t * TAU).sin(),
    }
}

impl Synth {
    pub fn new() -> Self {
        Self {
            p: lyra_plugin::default_params(&SYNTH_PARAMS),
            sr: 48_000.0,
            voices: [Voice::default(); MAX_VOICES],
            serial: 0,
            rng: Rng::new(0x5e7),
        }
    }

    fn pick_voice(&self) -> usize {
        if let Some(i) = self.voices.iter().position(|v| v.stage == Stage::Off) {
            return i;
        }
        let quietest_released = (0..MAX_VOICES)
            .filter(|&i| self.voices[i].stage == Stage::Release)
            .min_by(|&a, &b| self.voices[a].level.total_cmp(&self.voices[b].level));
        quietest_released.unwrap_or_else(|| (0..MAX_VOICES).min_by_key(|&i| self.voices[i].serial).unwrap_or(0))
    }

    fn note_on(&mut self, key: u8, velocity: f32) {
        let p = &self.p;
        let sr = self.sr;
        let unison = (p[p::VOICES].round() as usize + 1).clamp(1, MAX_UNISON);
        let octave = p[p::OCTAVE].round() - 2.0;
        let freq = 440.0 * 2f32.powf((key as f32 - 69.0) / 12.0 + octave);

        let mut v = Voice {
            stage: Stage::Attack,
            key,
            velocity,
            serial: self.serial,
            unison,
            sub_phase: 0.0,
            sub_inc: freq * 0.5 / sr,
            level: 0.0,
            attack_inc: 1.0 / (p[p::ATTACK].max(0.001) * sr),
            decay_k: decay_coeff(p[p::DECAY], sr),
            sustain: p[p::SUSTAIN],
            release_k: decay_coeff(p[p::RELEASE], sr),
            fenv: 1.0,
            fdecay_k: decay_coeff(p[p::F_DECAY], sr),
            fsustain: p[p::F_SUSTAIN],
            base_cutoff: p[p::CUTOFF] * 2f32.powf(p[p::KEYTRACK] * (key as f32 - 60.0) / 12.0),
            env_octaves: p[p::ENV_AMT] * 5.0,
            k: 2.0 - 1.96 * p[p::RESO],
            ..Voice::default()
        };
        // Unison: voices fanned out symmetrically in pitch and stereo position.
        let norm = SQRT_2 / (unison as f32).sqrt();
        for u in 0..unison {
            let d = if unison == 1 { 0.0 } else { u as f32 / (unison - 1) as f32 * 2.0 - 1.0 };
            let cents = d * p[p::DETUNE] * 35.0;
            v.inc[u] = freq * 2f32.powf(cents / 1200.0) / sr;
            v.phase[u] = (self.rng.bipolar() * 0.5 + 0.5).fract();
            let angle = (d * p[p::SPREAD] + 1.0) * FRAC_PI_4;
            v.gain_l[u] = angle.cos() * norm;
            v.gain_r[u] = angle.sin() * norm;
        }
        self.serial += 1;
        let slot = self.pick_voice();
        self.voices[slot] = v;
    }

    fn handle(&mut self, e: Event) {
        match e {
            Event::NoteOn { key, velocity } => self.note_on(key, velocity),
            Event::NoteOff { key } => {
                for v in self.voices.iter_mut().filter(|v| v.key == key && v.stage != Stage::Off) {
                    if v.stage != Stage::Release {
                        v.stage = Stage::Release;
                    }
                }
            }
            Event::AllNotesOff => {
                let quick = tau_coeff(0.008, self.sr);
                for v in self.voices.iter_mut().filter(|v| v.stage != Stage::Off) {
                    v.stage = Stage::Release;
                    v.release_k = quick;
                }
            }
        }
    }

    #[inline]
    fn render(&mut self) -> (f32, f32) {
        let wave = self.p[p::WAVE].round() as u32;
        let sub = self.p[p::SUB];
        let nyquist = self.sr * 0.45;
        let (mut out_l, mut out_r) = (0.0, 0.0);

        for v in self.voices.iter_mut().filter(|v| v.stage != Stage::Off) {
            // Envelopes
            match v.stage {
                Stage::Attack => {
                    v.level += v.attack_inc;
                    if v.level >= 1.0 {
                        v.level = 1.0;
                        v.stage = Stage::Decay;
                    }
                    v.fenv = v.fsustain + (v.fenv - v.fsustain) * v.fdecay_k;
                }
                Stage::Decay => {
                    v.level = v.sustain + (v.level - v.sustain) * v.decay_k;
                    v.fenv = v.fsustain + (v.fenv - v.fsustain) * v.fdecay_k;
                    if v.sustain < 1e-4 && v.level < 1e-4 {
                        v.stage = Stage::Off; // a pluck that has fully died frees its voice
                        continue;
                    }
                }
                Stage::Release => {
                    v.level *= v.release_k;
                    v.fenv *= v.release_k;
                    if v.level < 1e-4 {
                        v.stage = Stage::Off;
                        continue;
                    }
                }
                Stage::Off => continue,
            }

            // Filter coefficients at control rate.
            if v.tick == 0 {
                let cutoff = (v.base_cutoff * 2f32.powf(v.env_octaves * v.fenv)).clamp(20.0, nyquist);
                let g = (PI * cutoff / self.sr).tan();
                let a1 = 1.0 / (1.0 + g * (g + v.k));
                let a2 = g * a1;
                v.a = (a1, a2, g * a2);
            }
            v.tick = (v.tick + 1) % CONTROL_RATE;

            // Oscillators
            let (mut l, mut r) = (0.0, 0.0);
            for u in 0..v.unison {
                let s = oscillator(wave, v.phase[u], v.inc[u]);
                v.phase[u] = (v.phase[u] + v.inc[u]).fract();
                l += s * v.gain_l[u];
                r += s * v.gain_r[u];
            }
            if sub > 0.0 {
                let s = (v.sub_phase * TAU).sin() * sub;
                v.sub_phase = (v.sub_phase + v.sub_inc).fract();
                l += s;
                r += s;
            }

            let (a1, a2, a3) = v.a;
            let gain = v.level * v.velocity;
            out_l += v.left.run(l, a1, a2, a3) * gain;
            out_r += v.right.run(r, a1, a2, a3) * gain;
        }
        let level = self.p[p::LEVEL] * 0.35;
        (out_l * level, out_r * level)
    }
}

impl Plugin for Synth {
    fn name(&self) -> &'static str {
        "Lyra Synth"
    }
    fn params(&self) -> &'static [ParamInfo] {
        &SYNTH_PARAMS
    }
    fn groups(&self) -> &'static [ParamGroup] {
        &SYNTH_GROUPS
    }
    fn set_param(&mut self, id: u32, value: f32) {
        if let Some(p) = self.p.get_mut(id as usize) {
            *p = value;
        }
    }
    fn activate(&mut self, sample_rate: f32) {
        self.sr = sample_rate;
    }
    fn process(&mut self, ctx: ProcessContext<'_>) {
        let ProcessContext { events, out_l, out_r, .. } = ctx;
        let mut next = 0;
        for i in 0..out_l.len() {
            while next < events.len() && events[next].offset as usize <= i {
                self.handle(events[next].event);
                next += 1;
            }
            let (l, r) = self.render();
            out_l[i] = l;
            out_r[i] = r;
        }
        for e in &events[next..] {
            self.handle(e.event);
        }
    }
}
