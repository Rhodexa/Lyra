//! Pizzicato: a few plucked-string loops, reused round-robin.

use crate::curves::Pluck;
use crate::dsp::{OnePole, StringLoop};
use crate::midi_to_hz;
use crate::rand::Rng;

const VOICES: usize = 4;

pub struct Plucks {
    sr: f32,
    ring: f32,
    loops: Vec<StringLoop>,
    /// Excitation still being fed into each loop.
    excite: Vec<Vec<f32>>,
    pos: Vec<usize>,
    next: usize,
    rng: Rng,
}

impl Plucks {
    pub fn new(sr: f32, ring: f32, seed: u64) -> Self {
        Self {
            sr,
            ring,
            loops: (0..VOICES).map(|_| StringLoop::new()).collect(),
            excite: vec![Vec::new(); VOICES],
            pos: vec![0; VOICES],
            next: 0,
            rng: Rng::new(seed ^ 0x9122),
        }
    }

    pub fn trigger(&mut self, p: &Pluck) {
        let v = self.next;
        self.next = (self.next + 1) % VOICES;
        let hz = midi_to_hz(p.pitch);
        // Low notes ring longest.
        let t60 = (self.ring * (55.0 / hz).powf(0.5)).clamp(0.25, self.ring * 1.5);
        let b = p.brightness.clamp(0.0, 1.0);
        let lp = self.loops.get_mut(v).unwrap();
        lp.clear();
        lp.tune(self.sr, hz, t60, 0.55 - 0.35 * b);
        // A finger is soft: filtered noise, one period long, shaped like a
        // pluck near the end of the string.
        let period = (self.sr / hz) as usize;
        let mut smooth = OnePole::new(self.sr, 600.0 + 5000.0 * b);
        let mut ex: Vec<f32> = (0..period)
            .map(|k| {
                let x = k as f32 / period as f32;
                let shape = if x < 0.2 { x / 0.2 } else { (1.0 - x) / 0.8 };
                smooth.lowpass(self.rng.bipolar() * 0.5 + shape - 0.5)
            })
            .collect();
        let mean = ex.iter().sum::<f32>() / ex.len().max(1) as f32;
        let gain = 0.5 * p.velocity.powf(1.3);
        ex.iter_mut().for_each(|s| *s = (*s - mean) * gain);
        self.excite[v] = ex;
        self.pos[v] = 0;
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        let mut y = 0.0;
        for v in 0..VOICES {
            let input = match self.excite[v].get(self.pos[v]) {
                Some(&x) => {
                    self.pos[v] += 1;
                    x
                }
                None => 0.0,
            };
            y += self.loops[v].tick(input);
        }
        y
    }
}
