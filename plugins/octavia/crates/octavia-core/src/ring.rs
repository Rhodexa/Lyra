//! Strings left ringing after the bow moves to another string.
//!
//! A bowed voice is one tone, so on its own a crossing sounds like one
//! string leaping. On a real cello the string the bow just left keeps
//! vibrating: long if it's open, shorter if a finger still rests on it. One
//! free-ringing voice per string: harmonics decaying exponentially (the
//! highs first), silenced quickly when the bow comes back to that string.

use crate::curves::Ring;
use crate::midi_to_hz;
use std::f64::consts::TAU;

const PARTIALS: usize = 24;

#[derive(Clone)]
struct Voice {
    active: bool,
    phase: f64,
    step: f64,
    amps: [f32; PARTIALS],
    decay: [f32; PARTIALS],
    count: usize,
}

impl Default for Voice {
    fn default() -> Self {
        Self { active: false, phase: 0.0, step: 0.0, amps: [0.0; PARTIALS], decay: [1.0; PARTIALS], count: 0 }
    }
}

pub struct Rings {
    sr: f32,
    voices: [Voice; 4],
    /// Per-sample decay for a string the bow has come back to (~8 ms).
    choke: f32,
}

impl Rings {
    pub fn new(sr: f32) -> Self {
        Self { sr, voices: Default::default(), choke: (-1.0 / (0.008 * sr)).exp() }
    }

    pub fn start(&mut self, r: &Ring) {
        let Some(v) = self.voices.get_mut(r.string as usize) else { return };
        let f0 = midi_to_hz(r.pitch);
        v.active = true;
        v.step = TAU * f0 as f64 / self.sr as f64;
        v.count = ((self.sr * 0.45 / f0) as usize).clamp(1, PARTIALS);
        // An open string rings for a second or two; a stopped one is damped
        // by the fingertip.
        let t60 = if r.open { 1.6 } else { 0.5 };
        let mut energy = 0.0;
        for k in 0..v.count {
            let n = (k + 1) as f32;
            let a = n.powf(-1.4);
            energy += a * a;
            v.amps[k] = a;
            let t = t60 / (1.0 + 0.2 * (n - 1.0));
            v.decay[k] = 10f32.powf(-3.0 / (t * self.sr));
        }
        let gain = 0.2 * r.level.max(0.0).powf(1.4) / energy.sqrt();
        v.amps[..v.count].iter_mut().for_each(|a| *a *= gain);
    }

    /// The bow is back on `string`: let its free ringing die fast.
    pub fn choke(&mut self, string: u8) {
        if let Some(v) = self.voices.get_mut(string as usize) {
            if v.active {
                let c = self.choke;
                v.decay[..v.count].iter_mut().for_each(|d| *d = d.min(c));
            }
        }
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        let mut y = 0.0f64;
        for v in self.voices.iter_mut().filter(|v| v.active) {
            v.phase = (v.phase + v.step) % TAU;
            let (s1, c1) = v.phase.sin_cos();
            let (mut s, mut c) = (s1, c1);
            for k in 0..v.count {
                y += v.amps[k] as f64 * s;
                let ns = s * c1 + c * s1;
                c = c * c1 - s * s1;
                s = ns;
                v.amps[k] *= v.decay[k];
            }
            if v.amps[0] < 1e-6 {
                v.active = false;
            }
        }
        y as f32
    }
}
