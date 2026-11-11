//! Synthesized drum kit: kick, snare, clap, hi-hat.
//!
//! Every drum keeps a tiny fixed pool of voices so tails can overlap
//! (an open hat ringing under the next hit). Pools are arrays — nothing is
//! allocated after construction.

use crate::util::*;
use lyra_plugin::{Curve, Event, ParamInfo, Plugin, ProcessContext};

const VOICES: usize = 4;
/// Samples of fade-in on every hit, so retriggers don't click.
const DECLICK: f32 = 24.0;

use Curve::{Exponential as Exp, Linear as Lin};

// ───────────────────────────────── Kick ─────────────────────────────────

static KICK_PARAMS: [ParamInfo; 7] = [
    ParamInfo::new(0, "Tune", 30.0, 120.0, 48.0, "Hz", Exp),
    ParamInfo::new(1, "Punch", 1.0, 10.0, 5.0, "", Lin),
    ParamInfo::new(2, "Sweep", 0.005, 0.2, 0.04, "s", Exp),
    ParamInfo::new(3, "Decay", 0.05, 1.5, 0.45, "s", Exp),
    ParamInfo::new(4, "Click", 0.0, 1.0, 0.3, "%", Lin),
    ParamInfo::new(5, "Drive", 0.0, 1.0, 0.25, "%", Lin),
    ParamInfo::new(6, "Level", 0.0, 1.5, 0.9, "%", Lin),
];

#[derive(Clone, Copy, Default)]
struct KickVoice {
    active: bool,
    age: f32,
    phase: f32,
    f0: f32,
    punch: f32,
    amp: f32,
    amp_k: f32,
    sweep: f32,
    sweep_k: f32,
    click: f32,
    click_k: f32,
}

pub struct Kick {
    p: Vec<f32>,
    sr: f32,
    voices: [KickVoice; VOICES],
    next: usize,
    rng: Rng,
}

impl Kick {
    pub fn new() -> Self {
        Self {
            p: lyra_plugin::default_params(&KICK_PARAMS),
            sr: 48_000.0,
            voices: [KickVoice::default(); VOICES],
            next: 0,
            rng: Rng::new(0x1234),
        }
    }
}

impl MonoSource for Kick {
    fn handle(&mut self, e: Event) {
        match e {
            Event::NoteOn { key, velocity } => {
                let p = &self.p;
                self.voices[self.next] = KickVoice {
                    active: true,
                    age: 0.0,
                    phase: 0.0,
                    f0: p[0] * key_ratio(key),
                    punch: p[1],
                    amp: velocity,
                    amp_k: decay_coeff(p[3], self.sr),
                    sweep: 1.0,
                    sweep_k: tau_coeff(p[2], self.sr),
                    click: p[4] * velocity,
                    click_k: tau_coeff(0.0015, self.sr),
                };
                self.next = (self.next + 1) % VOICES;
            }
            Event::AllNotesOff => {
                for v in &mut self.voices {
                    v.amp_k = tau_coeff(0.004, self.sr);
                }
            }
            Event::NoteOff { .. } => {} // drums are one-shots
        }
    }

    fn tick(&mut self) -> f32 {
        let mut out = 0.0;
        for v in self.voices.iter_mut().filter(|v| v.active) {
            let freq = v.f0 * (1.0 + (v.punch - 1.0) * v.sweep);
            v.phase = (v.phase + freq / self.sr).fract();
            let body = (v.phase * std::f32::consts::TAU).sin();
            let click = self.rng.bipolar() * v.click;
            let declick = (v.age / DECLICK).min(1.0);
            out += (body + click) * v.amp * declick;

            v.age += 1.0;
            v.sweep *= v.sweep_k;
            v.click *= v.click_k;
            v.amp *= v.amp_k;
            v.active = v.amp > 1e-4;
        }
        let k = 1.0 + self.p[5] * 8.0;
        (k * out).tanh() / k.tanh() * self.p[6]
    }
}

impl Plugin for Kick {
    fn name(&self) -> &'static str {
        "Kick"
    }
    fn params(&self) -> &'static [ParamInfo] {
        &KICK_PARAMS
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
        run_mono(self, ctx);
    }
}

// ───────────────────────────────── Snare ────────────────────────────────

static SNARE_PARAMS: [ParamInfo; 6] = [
    ParamInfo::new(0, "Tune", 120.0, 400.0, 190.0, "Hz", Exp),
    ParamInfo::new(1, "Body", 0.02, 0.4, 0.12, "s", Exp),
    ParamInfo::new(2, "Snap", 0.0, 1.0, 0.7, "%", Lin),
    ParamInfo::new(3, "Decay", 0.03, 0.8, 0.2, "s", Exp),
    ParamInfo::new(4, "Color", 500.0, 8000.0, 1800.0, "Hz", Exp),
    ParamInfo::new(5, "Level", 0.0, 1.5, 0.8, "%", Lin),
];

#[derive(Clone, Copy, Default)]
struct SnareVoice {
    active: bool,
    age: f32,
    phase: f32,
    freq: f32,
    bend: f32,
    bend_k: f32,
    tone: f32,
    tone_k: f32,
    noise: f32,
    noise_k: f32,
    hp: Biquad,
}

pub struct Snare {
    p: Vec<f32>,
    sr: f32,
    voices: [SnareVoice; VOICES],
    next: usize,
    rng: Rng,
}

impl Snare {
    pub fn new() -> Self {
        Self {
            p: lyra_plugin::default_params(&SNARE_PARAMS),
            sr: 48_000.0,
            voices: [SnareVoice::default(); VOICES],
            next: 0,
            rng: Rng::new(0x5151),
        }
    }
}

impl MonoSource for Snare {
    fn handle(&mut self, e: Event) {
        match e {
            Event::NoteOn { key, velocity } => {
                let p = &self.p;
                let snap = p[2];
                self.voices[self.next] = SnareVoice {
                    active: true,
                    age: 0.0,
                    phase: 0.0,
                    freq: p[0] * key_ratio(key),
                    bend: 1.0,
                    bend_k: tau_coeff(0.008, self.sr),
                    tone: velocity * (1.0 - 0.5 * snap),
                    tone_k: decay_coeff(p[1], self.sr),
                    noise: velocity * snap,
                    noise_k: decay_coeff(p[3], self.sr),
                    hp: Biquad::new(FilterKind::HighPass, self.sr, p[4], 0.7),
                };
                self.next = (self.next + 1) % VOICES;
            }
            Event::AllNotesOff => {
                for v in &mut self.voices {
                    v.tone_k = tau_coeff(0.004, self.sr);
                    v.noise_k = v.tone_k;
                }
            }
            Event::NoteOff { .. } => {}
        }
    }

    fn tick(&mut self) -> f32 {
        let mut out = 0.0;
        for v in self.voices.iter_mut().filter(|v| v.active) {
            let freq = v.freq * (1.0 + 0.6 * v.bend);
            v.phase = (v.phase + freq / self.sr).fract();
            let tri = 4.0 * (v.phase - 0.5).abs() - 1.0;
            let noise = v.hp.run(self.rng.bipolar());
            let declick = (v.age / DECLICK).min(1.0);
            out += (tri * v.tone + noise * v.noise) * declick;

            v.age += 1.0;
            v.bend *= v.bend_k;
            v.tone *= v.tone_k;
            v.noise *= v.noise_k;
            v.active = v.tone.max(v.noise) > 1e-4;
        }
        out * self.p[5]
    }
}

impl Plugin for Snare {
    fn name(&self) -> &'static str {
        "Snare"
    }
    fn params(&self) -> &'static [ParamInfo] {
        &SNARE_PARAMS
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
        run_mono(self, ctx);
    }
}

// ───────────────────────────────── Clap ─────────────────────────────────

static CLAP_PARAMS: [ParamInfo; 4] = [
    ParamInfo::new(0, "Tone", 600.0, 4000.0, 1300.0, "Hz", Exp),
    ParamInfo::new(1, "Spread", 0.005, 0.03, 0.011, "s", Exp),
    ParamInfo::new(2, "Decay", 0.05, 1.0, 0.22, "s", Exp),
    ParamInfo::new(3, "Level", 0.0, 1.5, 0.9, "%", Lin),
];

#[derive(Clone, Copy, Default)]
struct ClapVoice {
    active: bool,
    age: f32,
    vel: f32,
    gap: f32,
    burst_tau: f32,
    tail: f32,
    tail_k: f32,
    kill: f32,
    kill_k: f32,
    bp: Biquad,
    hp: Biquad,
}

pub struct Clap {
    p: Vec<f32>,
    sr: f32,
    voices: [ClapVoice; VOICES],
    next: usize,
    rng: Rng,
}

impl Clap {
    pub fn new() -> Self {
        Self {
            p: lyra_plugin::default_params(&CLAP_PARAMS),
            sr: 48_000.0,
            voices: [ClapVoice::default(); VOICES],
            next: 0,
            rng: Rng::new(0xc1a9),
        }
    }
}

impl MonoSource for Clap {
    fn handle(&mut self, e: Event) {
        match e {
            Event::NoteOn { key, velocity } => {
                let p = &self.p;
                self.voices[self.next] = ClapVoice {
                    active: true,
                    age: 0.0,
                    vel: velocity,
                    gap: p[1] * self.sr,
                    burst_tau: 0.004 * self.sr,
                    tail: 0.85,
                    tail_k: decay_coeff(p[2], self.sr),
                    kill: 1.0,
                    kill_k: 1.0,
                    bp: Biquad::new(FilterKind::BandPass, self.sr, p[0] * key_ratio(key), 1.2),
                    hp: Biquad::new(FilterKind::HighPass, self.sr, 400.0, 0.7),
                };
                self.next = (self.next + 1) % VOICES;
            }
            Event::AllNotesOff => {
                for v in &mut self.voices {
                    v.kill_k = tau_coeff(0.004, self.sr);
                }
            }
            Event::NoteOff { .. } => {}
        }
    }

    fn tick(&mut self) -> f32 {
        let mut out = 0.0;
        for v in self.voices.iter_mut().filter(|v| v.active) {
            // Three quick noise bursts (hands not clapping in perfect sync), then a tail.
            let env = if v.age < 3.0 * v.gap {
                (-(v.age % v.gap) / v.burst_tau).exp()
            } else {
                let e = v.tail;
                v.tail *= v.tail_k;
                e
            };
            let x = v.hp.run(v.bp.run(self.rng.bipolar()));
            out += x * env * v.vel * v.kill * 2.5;

            v.age += 1.0;
            v.kill *= v.kill_k;
            v.active = v.tail > 1e-4 && v.kill > 1e-4;
        }
        out * self.p[3]
    }
}

impl Plugin for Clap {
    fn name(&self) -> &'static str {
        "Clap"
    }
    fn params(&self) -> &'static [ParamInfo] {
        &CLAP_PARAMS
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
        run_mono(self, ctx);
    }
}

// ───────────────────────────────── Hi-hat ───────────────────────────────

static HAT_PARAMS: [ParamInfo; 5] = [
    ParamInfo::new(0, "Tune", 30.0, 80.0, 40.0, "Hz", Exp),
    ParamInfo::new(1, "Decay", 0.015, 1.5, 0.06, "s", Exp),
    ParamInfo::new(2, "Tone", 2000.0, 12000.0, 7000.0, "Hz", Exp),
    ParamInfo::new(3, "Metal", 0.0, 1.0, 0.6, "%", Lin),
    ParamInfo::new(4, "Level", 0.0, 1.5, 0.7, "%", Lin),
];

/// The classic "six detuned square waves" metallic ratios.
const HAT_RATIOS: [f32; 6] = [2.0, 3.0, 4.16, 5.43, 6.79, 8.21];

#[derive(Clone, Copy, Default)]
struct HatVoice {
    active: bool,
    age: f32,
    phases: [f32; 6],
    base: f32,
    amp: f32,
    amp_k: f32,
    bp: Biquad,
    hp: Biquad,
}

pub struct Hat {
    p: Vec<f32>,
    sr: f32,
    voices: [HatVoice; VOICES],
    next: usize,
    rng: Rng,
}

impl Hat {
    pub fn new() -> Self {
        Self {
            p: lyra_plugin::default_params(&HAT_PARAMS),
            sr: 48_000.0,
            voices: [HatVoice::default(); VOICES],
            next: 0,
            rng: Rng::new(0x4a7),
        }
    }
}

impl MonoSource for Hat {
    fn handle(&mut self, e: Event) {
        match e {
            Event::NoteOn { key, velocity } => {
                let p = &self.p;
                self.voices[self.next] = HatVoice {
                    active: true,
                    age: 0.0,
                    phases: [0.0, 0.13, 0.27, 0.41, 0.58, 0.77],
                    base: p[0] * key_ratio(key),
                    amp: velocity,
                    amp_k: decay_coeff(p[1], self.sr),
                    bp: Biquad::new(FilterKind::BandPass, self.sr, 10_000.0, 0.5),
                    hp: Biquad::new(FilterKind::HighPass, self.sr, p[2], 0.7),
                };
                self.next = (self.next + 1) % VOICES;
            }
            Event::AllNotesOff => {
                for v in &mut self.voices {
                    v.amp_k = tau_coeff(0.003, self.sr);
                }
            }
            Event::NoteOff { .. } => {}
        }
    }

    fn tick(&mut self) -> f32 {
        let metal = self.p[3];
        let mut out = 0.0;
        for v in self.voices.iter_mut().filter(|v| v.active) {
            let mut squares = 0.0;
            for (ph, r) in v.phases.iter_mut().zip(HAT_RATIOS) {
                *ph = (*ph + v.base * r / self.sr).fract();
                squares += if *ph < 0.5 { 1.0 } else { -1.0 };
            }
            let x = metal * squares / 6.0 + (1.0 - metal) * self.rng.bipolar();
            let declick = (v.age / DECLICK).min(1.0);
            out += v.hp.run(v.bp.run(x)) * v.amp * declick * 3.0;

            v.age += 1.0;
            v.amp *= v.amp_k;
            v.active = v.amp > 1e-4;
        }
        out * self.p[4]
    }
}

impl Plugin for Hat {
    fn name(&self) -> &'static str {
        "Hi-hat"
    }
    fn params(&self) -> &'static [ParamInfo] {
        &HAT_PARAMS
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
        run_mono(self, ctx);
    }
}
