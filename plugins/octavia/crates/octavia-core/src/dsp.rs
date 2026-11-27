//! Tiny filter building blocks.

use std::f32::consts::PI;

/// Transposed direct form II biquad.
#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// Band-pass with 0 dB at the peak (RBJ cookbook). A resonant "mode".
    pub fn bandpass(sr: f32, hz: f32, q: f32) -> Self {
        let mut b = Self::default();
        b.set_bandpass(sr, hz, q);
        b
    }
    pub fn set_bandpass(&mut self, sr: f32, hz: f32, q: f32) {
        let w = 2.0 * PI * hz.min(sr * 0.49) / sr;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = alpha / a0;
        self.b1 = 0.0;
        self.b2 = -alpha / a0;
        self.a1 = -2.0 * w.cos() / a0;
        self.a2 = (1.0 - alpha) / a0;
    }
    pub fn lowpass(sr: f32, hz: f32, q: f32) -> Self {
        let mut b = Self::default();
        b.set_lowpass(sr, hz, q);
        b
    }
    /// Changes the coefficients but keeps the state, so it can glide.
    pub fn set_lowpass(&mut self, sr: f32, hz: f32, q: f32) {
        let w = 2.0 * PI * hz.clamp(10.0, sr * 0.45) / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = (1.0 - c) * 0.5 / a0;
        self.b1 = (1.0 - c) / a0;
        self.b2 = self.b0;
        self.a1 = -2.0 * c / a0;
        self.a2 = (1.0 - alpha) / a0;
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// One-pole low-pass. `highpass` is the complement.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnePole {
    a: f32,
    y: f32,
}

impl OnePole {
    pub fn new(sr: f32, hz: f32) -> Self {
        Self { a: (-2.0 * PI * hz / sr).exp(), y: 0.0 }
    }
    #[inline]
    pub fn lowpass(&mut self, x: f32) -> f32 {
        self.y = x + self.a * (self.y - x);
        self.y
    }
    #[inline]
    pub fn highpass(&mut self, x: f32) -> f32 {
        x - self.lowpass(x)
    }
}

/// A string loop: delay line + averaging low-pass + loss. Karplus–Strong
/// shaped; used for plucks and for the sympathetic strings.
#[derive(Clone, Debug)]
pub struct StringLoop {
    buf: Vec<f32>,
    w: usize,
    delay: f32,
    gain: f32,
    damp: f32,
    lp: f32,
}

impl StringLoop {
    pub const MAX: usize = 8192;

    pub fn new() -> Self {
        Self { buf: vec![0.0; Self::MAX], w: 0, delay: 100.0, gain: 0.0, damp: 0.5, lp: 0.0 }
    }
    /// `t60` = seconds to fall 60 dB. `damp` 0..1: how much the highs die faster.
    pub fn tune(&mut self, sr: f32, hz: f32, t60: f32, damp: f32) {
        // The loop filter adds about half a sample of delay.
        self.delay = (sr / hz - 0.5 - damp * 0.5).clamp(2.0, (Self::MAX - 4) as f32);
        let period = 1.0 / hz;
        self.gain = 10f32.powf(-3.0 * period / t60.max(0.01));
        self.damp = damp.clamp(0.0, 0.95);
    }
    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|s| *s = 0.0);
        self.lp = 0.0;
    }
    #[inline]
    pub fn tick(&mut self, input: f32) -> f32 {
        let len = self.buf.len();
        let d = self.delay;
        let di = d as usize;
        let frac = d - di as f32;
        let r0 = (self.w + len - di) % len;
        let r1 = (r0 + len - 1) % len;
        let delayed = self.buf[r0] * (1.0 - frac) + self.buf[r1] * frac;
        // One-pole loss filter: more damping = darker, faster-dying highs.
        self.lp = delayed + self.damp * (self.lp - delayed);
        let y = input + self.gain * self.lp;
        self.buf[self.w] = y;
        self.w = (self.w + 1) % len;
        y
    }
}

#[inline]
pub fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
