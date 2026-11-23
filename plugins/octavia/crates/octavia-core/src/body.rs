//! The wooden body: a bank of resonant modes in parallel, plus a direct path.

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
