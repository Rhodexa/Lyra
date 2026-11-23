//! The bowed string: a bank of harmonic overtones plus bow noise.
//!
//! A bowed string locks its overtones into exact multiples of the
//! fundamental (Helmholtz motion), so overtone n turns at n·φ. We step
//! `e^{inφ}` up the series with one complex multiply per overtone, no sine
//! calls. Each overtone carries a **complex** amplitude: the string's level
//! times the body's gain *and phase* at that frequency ([`BodyResponse`]).
//! So overtones don't all line up into a pulse (the "Game Boy" sound), and
//! when vibrato sweeps them across the body's peaks, each flickers on its own.
//!
//! The string's overtone recipe (until a trained model replaces it):
//! * a sawtooth-like `1/n^tilt` slope, steeper when dark;
//! * soft dips where the bow position `β` sits near a node. Not clean
//!   `sin(nπβ)` notches: those are exactly a narrow pulse wave's spectrum.
//!   The bow hair is a wide ribbon and its position drifts, so the dips are
//!   averaged over a spread of `β` and wander;
//! * a soft high cut from string damping;
//! * random shimmer per overtone, more in the highs, and period jitter, so
//!   no two cycles are identical.
//!
//! Bow noise is high-passed white noise, low-passed by brightness and pulsed
//! once per period, because the bow slips once per cycle. Half of it runs
//! through a comb tuned to the note, so it gathers around the overtones the
//! way real bow noise does. Without a steady layer of it the sustain is a
//! "perfect waveform" (heard as a Game Boy in unison with the cello). It comes out
//! separately ([`BowVoice::process`]) because it goes through the
//! time-domain body, not the per-overtone one.

use crate::body::BodyResponse;
use crate::curves::{Frame, FRAME_RATE};
use crate::dsp::{Biquad, OnePole};
use crate::midi_to_hz;
use crate::rand::{Rng, Wander};
use std::f64::consts::{PI, TAU};

pub const MAX_PARTIALS: usize = 160;

pub struct BowVoice {
    sr: f32,
    body: BodyResponse,
    phase: f64,
    re: [f32; MAX_PARTIALS],
    im: [f32; MAX_PARTIALS],
    d_re: [f32; MAX_PARTIALS],
    d_im: [f32; MAX_PARTIALS],
    shimmer: [f32; MAX_PARTIALS],
    count: usize,
    rng: Rng,
    jitter: Wander,
    wobble: Wander,
    grain: Wander,
    scratch: Wander,
    bow_drift: Wander,
    noise_hp: OnePole,
    noise_lp: Biquad,
    noise_amp: f32,
    comb: Vec<f32>,
    comb_w: usize,
}

impl BowVoice {
    pub fn new(sr: f32, body: BodyResponse, seed: u64) -> Self {
        Self {
            sr,
            body,
            phase: 0.0,
            re: [0.0; MAX_PARTIALS],
            im: [0.0; MAX_PARTIALS],
            d_re: [0.0; MAX_PARTIALS],
            d_im: [0.0; MAX_PARTIALS],
            shimmer: [0.0; MAX_PARTIALS],
            count: 0,
            rng: Rng::new(seed ^ 0xB0B0),
            jitter: Wander::new(seed ^ 0x717, 70.0),
            wobble: Wander::new(seed ^ 0x3C3, 9.0),
            grain: Wander::new(seed ^ 0x6A1, 180.0),
            scratch: Wander::new(seed ^ 0x5C7, 14.0),
            bow_drift: Wander::new(seed ^ 0xD71F7, 1.3),
            noise_hp: OnePole::new(sr, 250.0),
            noise_lp: Biquad::lowpass(sr, 3000.0, 0.7),
            noise_amp: 0.0,
            comb: vec![0.0; 4096],
            comb_w: 0,
        }
    }

    /// Complex overtone amplitudes for one frame, scaled by loudness.
    fn targets(&mut self, fr: &Frame, count: usize, re: &mut [f32; MAX_PARTIALS], im: &mut [f32; MAX_PARTIALS]) {
        let b = fr.brightness.clamp(0.0, 1.0);
        // Bow moves toward the bridge when bright, and never holds still.
        let beta = (0.13 - 0.07 * b) * (1.0 + 0.12 * self.bow_drift.next(1.0 / FRAME_RATE as f32));
        let tilt = 1.5 - 0.5 * b;
        let f0 = midi_to_hz(fr.pitch);
        let fc = 1800.0 + 3500.0 * b;
        let mut amps = [0.0f32; MAX_PARTIALS];
        let mut energy = 0.0f32;
        for (k, slot) in amps.iter_mut().enumerate().take(count) {
            let n = (k + 1) as f32;
            let x = n * std::f32::consts::PI * beta;
            let ribbon = ((x * 0.8).sin().abs() + x.sin().abs() + (x * 1.3).sin().abs()) / 3.0;
            let notch = 0.45 + 0.55 * ribbon;
            let hf = n * f0 / fc;
            let a = n.powf(-tilt) * notch / (1.0 + hf * hf);
            energy += a * a;
            // Shimmer: ~10 Hz random level wobble, deeper on the upper overtones.
            self.shimmer[k] = 0.7 * self.shimmer[k] + 0.3 * self.rng.bipolar();
            let depth = 0.08 + 0.7 * (k as f32 / count as f32);
            *slot = a * (1.0 + depth * self.shimmer[k]).max(0.0);
        }
        let gain = 0.25 * fr.loudness.max(0.0).powf(1.4) / energy.sqrt().max(1e-9);
        let nyq = self.sr * 0.45;
        for k in 0..count {
            let f = (k + 1) as f32 * f0;
            // Fade overtones out before they would alias.
            let edge = ((nyq - f) / (0.05 * self.sr)).clamp(0.0, 1.0);
            let (hr, hi) = self.body.at(f);
            let a = amps[k] * gain * edge;
            re[k] = a * hr;
            im[k] = a * hi;
        }
        re[count..].iter_mut().for_each(|s| *s = 0.0);
        im[count..].iter_mut().for_each(|s| *s = 0.0);
    }

    /// Renders `tone.len()` samples gliding from frame `a` to frame `b`:
    /// the overtones (already through the body) into `tone`, raw bow noise
    /// into `noise`.
    pub fn process(&mut self, a: &Frame, b: &Frame, tone: &mut [f32], noise: &mut [f32]) {
        let len = tone.len();
        if len == 0 {
            return;
        }
        let f_hi = midi_to_hz(a.pitch.max(b.pitch) + 0.5);
        let count = ((self.sr * 0.45 / f_hi) as usize).clamp(1, MAX_PARTIALS);
        // Overtones that no longer fit must fade, not vanish (clicks).
        let active = count.max(self.count);
        let (mut t_re, mut t_im) = ([0.0f32; MAX_PARTIALS], [0.0f32; MAX_PARTIALS]);
        self.targets(b, count, &mut t_re, &mut t_im);
        let inv = 1.0 / len as f32;
        for k in 0..active {
            self.d_re[k] = (t_re[k] - self.re[k]) * inv;
            self.d_im[k] = (t_im[k] - self.im[k]) * inv;
        }
        self.count = count;

        let fc = 1000.0 + 2500.0 * b.brightness.clamp(0.0, 1.0);
        self.noise_lp.set_lowpass(self.sr, fc, 0.7);
        // Level matched to clean cello recordings: ~−22 dB floor between overtones.
        let noise_target = 0.3 * b.loudness.max(0.0).powf(1.2) * b.noise;
        let noise_step = (noise_target - self.noise_amp) * inv;

        let sr = self.sr as f64;
        let dt = 1.0 / self.sr;
        for i in 0..len {
            let u = i as f32 * inv;
            let pitch = a.pitch + (b.pitch - a.pitch) * u;
            // Stick-slip is never perfectly periodic: fast jitter plus a slower wobble.
            let jitter = 1.0 + 0.0025 * self.jitter.next(dt) + 0.0015 * self.wobble.next(dt);
            let f = midi_to_hz(pitch) as f64 * jitter as f64;
            self.phase += TAU * f / sr;
            if self.phase >= TAU {
                self.phase -= TAU;
            }
            let (s1, c1) = self.phase.sin_cos();
            let (mut s, mut c) = (s1, c1);
            let mut acc = 0.0f64;
            for k in 0..active {
                // Im((re + i·im) · e^{i(k+1)φ}) = re·sin + im·cos
                acc += self.re[k] as f64 * s + self.im[k] as f64 * c;
                let ns = s * c1 + c * s1;
                c = c * c1 - s * s1;
                s = ns;
                self.re[k] += self.d_re[k];
                self.im[k] += self.d_im[k];
            }
            tone[i] = acc as f32;

            let w = self.rng.bipolar();
            let shaped = self.noise_lp.process(self.noise_hp.highpass(w));
            // One slip per period: noise bunches up around the same phase.
            let pulse = (0.5 + 0.5 * (self.phase - PI).cos()).powi(3) as f32;
            self.noise_amp += noise_step;
            // Paper texture: the hair's grip comes and goes in grains, and in
            // slower scratchy patches. (Rhodexa: "a pencil on paper".)
            let g = 0.5 + 0.5 * self.grain.next(dt);
            let sc = 0.5 + 0.5 * self.scratch.next(dt);
            let texture = (0.2 + 1.6 * g * g) * (0.6 + 0.8 * sc);
            let raw = shaped * (0.35 + 2.6 * pulse) * texture;
            // Harmonic noise: a comb one period long.
            let len = self.comb.len();
            let d = (sr / f).clamp(2.0, (len - 2) as f64) as f32;
            let di = d as usize;
            let fr = d - di as f32;
            let r0 = (self.comb_w + len - di) % len;
            let r1 = (r0 + len - 1) % len;
            let delayed = self.comb[r0] * (1.0 - fr) + self.comb[r1] * fr;
            let y = raw + 0.9 * delayed;
            self.comb[self.comb_w] = y;
            self.comb_w = (self.comb_w + 1) % len;
            noise[i] = (0.5 * raw + 0.25 * y) * self.noise_amp;
        }
    }
}
