//! Small DSP building blocks shared by instruments and effects.

use lyra_plugin::{Event, ProcessContext};

/// Fast xorshift noise. Deterministic, allocation-free.
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    /// White noise sample in -1..1.
    #[inline]
    pub fn bipolar(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Per-sample multiplier that makes a value fall by 60 dB over `seconds`.
/// (`env *= coeff` each sample — no `exp()` in the inner loop.)
pub fn decay_coeff(seconds: f32, sample_rate: f32) -> f32 {
    (-6.9078 / (seconds.max(1e-4) * sample_rate)).exp()
}

/// Per-sample multiplier for a plain exponential with time constant `tau`.
pub fn tau_coeff(tau: f32, sample_rate: f32) -> f32 {
    (-1.0 / (tau.max(1e-5) * sample_rate)).exp()
}

/// Frequency ratio of a MIDI key relative to middle C (60).
pub fn key_ratio(key: u8) -> f32 {
    2f32.powf((key as f32 - 60.0) / 12.0)
}

#[derive(Clone, Copy, Debug)]
pub enum FilterKind {
    LowPass,
    HighPass,
    BandPass,
}

/// Classic RBJ-cookbook biquad, transposed direct form II.
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
    pub fn new(kind: FilterKind, sample_rate: f32, freq: f32, q: f32) -> Self {
        let mut f = Self::default();
        f.set(kind, sample_rate, freq, q);
        f
    }

    pub fn set(&mut self, kind: FilterKind, sample_rate: f32, freq: f32, q: f32) {
        let freq = freq.clamp(10.0, sample_rate * 0.45);
        let w0 = std::f32::consts::TAU * freq / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q.max(0.05));
        let (b0, b1, b2) = match kind {
            FilterKind::LowPass => ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0),
            FilterKind::HighPass => ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0),
            FilterKind::BandPass => (alpha, 0.0, -alpha),
        };
        let a0 = 1.0 + alpha;
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = -2.0 * cos / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// For simple instruments that produce one (mono) sample at a time.
pub trait MonoSource {
    fn handle(&mut self, event: Event);
    fn tick(&mut self) -> f32;
}

/// Drives a [`MonoSource`] through a block, applying events at their exact sample.
pub fn run_mono<T: MonoSource>(src: &mut T, ctx: ProcessContext<'_>) {
    let ProcessContext { events, out_l, out_r, .. } = ctx;
    let mut next = 0;
    for i in 0..out_l.len() {
        while next < events.len() && events[next].offset as usize <= i {
            src.handle(events[next].event);
            next += 1;
        }
        let s = src.tick();
        out_l[i] = s;
        out_r[i] = s;
    }
    // Events past the end of the block (shouldn't happen) still get applied.
    for e in &events[next..] {
        src.handle(e.event);
    }
}
