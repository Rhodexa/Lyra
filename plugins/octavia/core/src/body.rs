//! The wooden body, two ways.
//!
//! * [`BodyResponse`]: a dense, jagged frequency response (hundreds of
//!   interfering modes) stored as a complex table and applied **per
//!   overtone** by the bowed voice. Each overtone gets its own gain and phase,
//!   and both move when vibrato sweeps it across the peaks and valleys. That
//!   flicker is a big part of why a real string doesn't sound like a filtered
//!   oscillator. It costs nothing per sample.
//! * [`Body`]: a time-domain bank of resonators for everything that isn't
//!   a steady overtone: bow noise, plucks, the sympathetic strings.

use crate::dsp::{Biquad, OnePole};
use crate::instrument::Instrument;
use crate::rand::Rng;

pub struct Body {
    modes: Vec<(Biquad, f32)>,
    direct: f32,
    /// A body this size barely radiates its lowest notes' fundamentals.
    radiation: [OnePole; 2],
}

impl Body {
    pub fn new(inst: &Instrument, sr: f32, seed: u64) -> Self {
        let mut modes: Vec<(Biquad, f32)> =
            inst.body_modes.iter().map(|m| (Biquad::bandpass(sr, m.hz, m.q), m.gain)).collect();
        // The body's grain: many small modes, log-spread, seeded.
        let mut rng = Rng::new(seed ^ 0xB0D1);
        let (lo, hi, count) = inst.body_grain;
        for _ in 0..count {
            let hz = lo * (hi / lo).powf(rng.uniform());
            let q = 18.0 + 25.0 * rng.uniform();
            // More modes, each quieter, so the total stays about the same.
            let gain = (0.12 + 0.22 * rng.uniform()) * (28.0 / count.max(1) as f32).sqrt();
            modes.push((Biquad::bandpass(sr, hz, q), gain));
        }
        Self { modes, direct: inst.body_direct, radiation: [OnePole::new(sr, inst.radiation_hz); 2] }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let mut y = self.direct * x;
        for (bq, g) in &mut self.modes {
            y += *g * bq.process(x);
        }
        let y = self.radiation[0].highpass(y);
        self.radiation[1].highpass(y)
    }
}

/// The body as a complex gain per frequency, on a fine log grid.
pub struct BodyResponse {
    ln_lo: f32,
    per_ln: f32,
    re: Vec<f32>,
    im: Vec<f32>,
}

impl BodyResponse {
    const POINTS: usize = 8192;
    const LO: f32 = 25.0;
    const HI: f32 = 22_000.0;

    pub fn new(inst: &Instrument, seed: u64) -> Self {
        // (hz, q, signed gain)
        let mut modes: Vec<(f32, f32, f32)> = inst.body_modes.iter().map(|m| (m.hz, m.q * 1.6, m.gain * 0.7)).collect();
        let mut rng = Rng::new(seed ^ 0xDE7A11);
        let (lo, hi, count) = inst.body_detail;
        for _ in 0..count {
            let hz = lo * (hi / lo).powf(rng.uniform());
            let q = 20.0 + 45.0 * rng.uniform();
            // Mixed signs: neighbouring modes cancel, giving deep valleys
            // (anti-resonances) between the peaks, as real bodies do.
            let sign = if rng.uniform() < 0.5 { -1.0 } else { 1.0 };
            let gain = sign * (0.15 + 0.5 * rng.uniform()) * (200.0 / count.max(1) as f32).sqrt();
            modes.push((hz, q, gain));
        }
        let (ln_lo, ln_hi) = (Self::LO.ln(), Self::HI.ln());
        let per_ln = (Self::POINTS - 1) as f32 / (ln_hi - ln_lo);
        let mut re = Vec::with_capacity(Self::POINTS);
        let mut im = Vec::with_capacity(Self::POINTS);
        for i in 0..Self::POINTS {
            let f = (ln_lo + i as f32 / per_ln).exp();
            let (mut r, mut m) = (inst.body_direct, 0.0f32);
            for &(fk, q, g) in &modes {
                // A resonance: 1 / (1 + jQ(f/fk − fk/f)).
                let x = q * (f / fk - fk / f);
                let d = 1.0 + x * x;
                r += g / d;
                m -= g * x / d;
            }
            // Radiation: second-order high-pass, magnitude only.
            let u = (f / inst.radiation_hz).powi(2);
            let rad = u / (1.0 + u);
            re.push(r * rad);
            im.push(m * rad);
        }
        // Average power over the playing range = 1, so loudness stays put.
        let (mut sum, mut n) = (0.0f64, 0usize);
        for i in 0..Self::POINTS {
            let f = (ln_lo + i as f32 / per_ln).exp();
            if (100.0..5000.0).contains(&f) {
                sum += (re[i] * re[i] + im[i] * im[i]) as f64;
                n += 1;
            }
        }
        let norm = 1.0 / ((sum / n.max(1) as f64).sqrt() as f32).max(1e-9);
        re.iter_mut().for_each(|v| *v *= norm);
        im.iter_mut().for_each(|v| *v *= norm);
        Self { ln_lo, per_ln, re, im }
    }

    /// Complex gain at `hz`: (real, imaginary).
    #[inline]
    pub fn at(&self, hz: f32) -> (f32, f32) {
        let x = ((hz.max(Self::LO).ln() - self.ln_lo) * self.per_ln).min((Self::POINTS - 2) as f32);
        let i = x as usize;
        let t = x - i as f32;
        (
            self.re[i] + (self.re[i + 1] - self.re[i]) * t,
            self.im[i] + (self.im[i + 1] - self.im[i]) * t,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::cello;

    #[test]
    fn response_is_jagged_but_normalised() {
        let r = BodyResponse::new(&cello(), 1);
        let db = |f: f32| {
            let (a, b) = r.at(f);
            10.0 * (a * a + b * b).max(1e-12).log10()
        };
        let samples: Vec<f32> = (0..400).map(|k| db(300.0 * (10f32).powf(k as f32 / 400.0))).collect();
        let max = samples.iter().cloned().fold(f32::MIN, f32::max);
        let min = samples.iter().cloned().fold(f32::MAX, f32::min);
        assert!(max - min > 15.0, "too smooth: {min:.1}..{max:.1} dB");
        assert!(max < 20.0 && max > 0.0, "badly normalised: max {max:.1} dB");
        // The lowest fundamentals barely radiate.
        assert!(db(40.0) < db(400.0) - 10.0);
    }
}
