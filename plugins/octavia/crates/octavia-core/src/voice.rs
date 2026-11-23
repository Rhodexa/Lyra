//! The bowed string: a bank of exactly-harmonic overtones plus bow noise.
//!
//! A bowed string locks its overtones into exact multiples of the
//! fundamental (Helmholtz motion), so all overtones share one phase: overtone
//! n is `sin(n·φ)`. That lets us compute all of them with the Chebyshev
//! recurrence `sin((n+1)φ) = 2cos φ · sin(nφ) − sin((n−1)φ)`: one multiply-add
//! per overtone per sample, no sine calls.
//!
//! The overtone recipe (until a trained model replaces it):
//! * a sawtooth-like `1/n^tilt` slope, steeper when dark;
//! * notches where the bow position `β` sits on a node (`sin(nπβ)`);
//! * a soft high cut from string damping;
//! * a slow random shimmer per overtone, more in the highs.
//!
//! Bow noise is high-passed white noise, low-passed by brightness and pulsed
//! once per period, because the bow slips once per cycle.

use crate::curves::Frame;
use crate::dsp::{Biquad, OnePole};
use crate::midi_to_hz;
use crate::rand::{Rng, Wander};
use std::f64::consts::{PI, TAU};

pub const MAX_PARTIALS: usize = 160;

pub struct BowVoice {
    sr: f32,
    phase: f64,
    amps: [f32; MAX_PARTIALS],
    steps: [f32; MAX_PARTIALS],
    shimmer: [f32; MAX_PARTIALS],
    count: usize,
    rng: Rng,
    jitter: Wander,
    noise_hp: OnePole,
    noise_lp: Biquad,
    noise_amp: f32,
}

impl BowVoice {
    pub fn new(sr: f32, seed: u64) -> Self {
        Self {
            sr,
            phase: 0.0,
            amps: [0.0; MAX_PARTIALS],
            steps: [0.0; MAX_PARTIALS],
            shimmer: [0.0; MAX_PARTIALS],
            count: 0,
            rng: Rng::new(seed ^ 0xB0B0),
            jitter: Wander::new(seed ^ 0x717, 28.0),
            noise_hp: OnePole::new(sr, 500.0),
            noise_lp: Biquad::lowpass(sr, 3000.0, 0.7),
            noise_amp: 0.0,
        }
    }

    /// Overtone levels for one frame, already scaled by loudness.
    fn targets(&mut self, fr: &Frame, count: usize, out: &mut [f32; MAX_PARTIALS]) {
        let b = fr.brightness.clamp(0.0, 1.0);
        let beta = 0.13 - 0.07 * b; // bow moves toward the bridge when bright
        let tilt = 1.25 - 0.6 * b;
        let f0 = midi_to_hz(fr.pitch);
        let fc = 3500.0 + 8000.0 * b;
        let mut energy = 0.0f32;
        for (k, slot) in out.iter_mut().enumerate().take(count) {
            let n = (k + 1) as f32;
            let notch = 0.15 + 0.85 * (n * std::f32::consts::PI * beta).sin().abs();
            let hf = n * f0 / fc;
            let a = n.powf(-tilt) * notch / (1.0 + hf * hf);
            energy += a * a;
            // ~25 Hz random shimmer, deeper on the upper overtones.
            self.shimmer[k] = 0.85 * self.shimmer[k] + 0.15 * self.rng.bipolar();
            let depth = 0.05 + 0.6 * (k as f32 / count as f32);
            *slot = a * (1.0 + depth * self.shimmer[k]);
        }
        let gain = 0.25 * fr.loudness.max(0.0).powf(1.4) / energy.sqrt().max(1e-9);
        let nyq = self.sr * 0.45;
        for (k, slot) in out.iter_mut().enumerate().take(count) {
            // Fade overtones out before they would alias.
            let f = (k + 1) as f32 * f0;
            let edge = ((nyq - f) / (0.05 * self.sr)).clamp(0.0, 1.0);
            *slot *= gain * edge;
        }
        out[count..].iter_mut().for_each(|s| *s = 0.0);
    }

    /// Renders `out.len()` samples gliding from frame `a` to frame `b`.
    pub fn process(&mut self, a: &Frame, b: &Frame, out: &mut [f32]) {
        let len = out.len();
        if len == 0 {
            return;
        }
        let f_hi = midi_to_hz(a.pitch.max(b.pitch) + 0.5);
        let count = ((self.sr * 0.45 / f_hi) as usize).clamp(1, MAX_PARTIALS);
        // Overtones that no longer fit must fade, not vanish (clicks).
        let active = count.max(self.count);
        let mut target = [0.0f32; MAX_PARTIALS];
        self.targets(b, count, &mut target);
        let inv = 1.0 / len as f32;
        for k in 0..active {
            self.steps[k] = (target[k] - self.amps[k]) * inv;
        }
        self.count = count;

        let fc = 2000.0 + 6000.0 * b.brightness.clamp(0.0, 1.0);
        self.noise_lp.set_lowpass(self.sr, fc, 0.7);
        let noise_target = 0.12 * b.loudness.max(0.0).powf(1.2) * b.noise;
        let noise_step = (noise_target - self.noise_amp) * inv;

        let sr = self.sr as f64;
        for (i, o) in out.iter_mut().enumerate() {
            let u = i as f32 * inv;
            let pitch = a.pitch + (b.pitch - a.pitch) * u;
            let jitter = 1.0 + 0.0012 * self.jitter.next(1.0 / self.sr);
            let f = midi_to_hz(pitch) as f64 * jitter as f64;
            self.phase += TAU * f / sr;
            if self.phase >= TAU {
                self.phase -= TAU;
            }
            let (s1, c) = self.phase.sin_cos();
            let two_c = 2.0 * c;
            let (mut prev, mut s) = (0.0f64, s1);
            let mut acc = 0.0f64;
            for k in 0..active {
                acc += self.amps[k] as f64 * s;
                let nx = two_c * s - prev;
                prev = s;
                s = nx;
                self.amps[k] += self.steps[k];
            }

            let w = self.rng.bipolar();
            let shaped = self.noise_lp.process(self.noise_hp.highpass(w));
            // One slip per period: noise bunches up around the same phase.
            let pulse = (0.5 + 0.5 * (self.phase - PI).cos()).powi(3) as f32;
            self.noise_amp += noise_step;
            *o = acc as f32 + shaped * (0.35 + 2.6 * pulse) * self.noise_amp;
        }
    }
}
