//! Small DSP building blocks shared by instruments and effects.

use lyra_plugin::{Event, ProcessContext};

/// Tempo-sync choices for LFOs and the like, as labels; `SYNC_BEATS` gives
/// each one's length in quarter notes (0 = free, not synced). Saved as an
/// index, so new lengths only ever go at the end.
pub const SYNC_LABELS: [&str; 13] =
    ["Free", "4 bars", "2 bars", "1 bar", "1/2", "1/4", "1/4 T", "1/8", "1/8 D", "1/8 T", "1/16", "1/16 T", "1/32"];
pub const SYNC_BEATS: [f32; 13] =
    [0.0, 16.0, 8.0, 4.0, 2.0, 1.0, 2.0 / 3.0, 0.5, 0.75, 1.0 / 3.0, 0.25, 1.0 / 6.0, 0.125];

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

// ───────────────────────────── fast math ─────────────────────────────

/// 1.5 × 2^23: adding and subtracting it rounds an f32 to the nearest integer.
const ROUND_MAGIC: f32 = 12_582_912.0;

/// Nearest integer, without a libm call (baseline x86-64 has no `roundss`). |x| < 2^22.
#[inline]
pub fn round_fast(x: f32) -> f32 {
    (x + ROUND_MAGIC) - ROUND_MAGIC
}

/// `x - floor(x)`: wraps a modulated phase back into `0..=1`, branch-free.
#[inline]
pub fn wrap(x: f32) -> f32 {
    x - round_fast(x - 0.5)
}

/// `sin(2π·x)` for any phase `x` (in turns), as an odd Taylor polynomial evaluated in
/// Estrin form (error < 1e-7). Much cheaper than `f32::sin`, and it sits on the critical
/// path of every FM chain.
#[inline]
pub fn sin_turns(x: f32) -> f32 {
    use std::f32::consts::TAU;
    const C: [f32; 6] = [
        TAU,
        -41.341_7,   // -(2π)^3 / 3!
        81.605_25,   //  (2π)^5 / 5!
        -76.705_86,  // -(2π)^7 / 7!
        42.058_693,  //  (2π)^9 / 9!
        -15.094_643, // -(2π)^11 / 11!
    ];
    // Reduce to -0.5..0.5, then fold into -0.25..0.25 using sin(π - θ) = sin θ. Arithmetic, not branches:
    // the branches would mispredict on every quarter cycle.
    let x = x - round_fast(x);
    let x = (0.25 - (x.abs() - 0.25).abs()).copysign(x);
    let z = x * x;
    let z2 = z * z;
    x * ((C[0] + C[1] * z) + z2 * ((C[2] + C[3] * z) + z2 * (C[4] + C[5] * z)))
}

/// PolyBLEP correction for a step at phase 0 (`t` in 0..1, `dt` = phase increment).
#[inline]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
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

/// A cheap `tanh` (Padé 7/6 with a hard limit): within 2e-3 of the real one, smooth, odd.
#[inline]
pub fn tanh_fast(x: f32) -> f32 {
    let x = x.clamp(-4.97, 4.97);
    let x2 = x * x;
    let y = x * (135_135.0 + x2 * (17_325.0 + x2 * (378.0 + x2)))
        / (135_135.0 + x2 * (62_370.0 + x2 * (3150.0 + 28.0 * x2)));
    y.clamp(-1.0, 1.0)
}

// ───────────────────────────── state-variable filter ─────────────────────────────

/// Coefficients of a TPT (zero-delay, "Cytomic") state-variable filter at `fc`
/// with damping `k` (2 = no resonance, → 0 = self-oscillation).
#[inline]
pub fn svf_coeffs(fc: f32, sr: f32, k: f32) -> (f32, f32, f32) {
    svf_coeffs_g((std::f32::consts::PI * fc / sr).tan(), k)
}

/// The same from the prewarped gain `g = tan(π·fc/sr)`.
#[inline]
pub fn svf_coeffs_g(g: f32, k: f32) -> (f32, f32, f32) {
    let a1 = 1.0 / (1.0 + g * (g + k));
    let a2 = g * a1;
    (a1, a2, g * a2)
}

/// One TPT SVF step; returns (band, low). High = `x - k·band - low`.
#[inline]
pub fn svf(s: &mut (f32, f32), x: f32, (a1, a2, a3): (f32, f32, f32)) -> (f32, f32) {
    let v3 = x - s.1;
    let v1 = a1 * s.0 + a2 * v3;
    let v2 = s.1 + a2 * s.0 + a3 * v3;
    s.0 = 2.0 * v1 - s.0;
    s.1 = 2.0 * v2 - s.1;
    (v1, v2)
}

// ───────────────────────────── FFT ─────────────────────────────

/// In-place radix-2 FFT of `re`/`im` (length a power of two). `inverse` runs
/// the inverse transform *without* the 1/n scale. A UI-thread tool
/// (wavetable building, analysis); for many transforms, keep an [`Fft`].
pub fn fft(re: &mut [f32], im: &mut [f32], inverse: bool) {
    Fft::new(re.len()).run(re, im, inverse);
}

/// A reusable FFT: the twiddles for size `n`, good for every power of two up to `n`.
pub struct Fft {
    n: usize,
    /// e^(−2πik/n) for k < n/2, computed in f64 (accurate to f32's last bit).
    tw: Vec<(f32, f32)>,
}

impl Fft {
    pub fn new(n: usize) -> Fft {
        assert!(n.is_power_of_two(), "fft sizes are powers of two");
        let ang = -std::f64::consts::TAU / n as f64;
        let tw = (0..n / 2)
            .map(|k| {
                let (s, c) = (ang * k as f64).sin_cos();
                (c as f32, s as f32)
            })
            .collect();
        Fft { n, tw }
    }

    /// Transform `re`/`im` in place (any power-of-two length up to this one's size).
    pub fn run(&self, re: &mut [f32], im: &mut [f32], inverse: bool) {
        let n = re.len();
        assert!(n.is_power_of_two() && im.len() == n && n <= self.n, "fft needs two equal power-of-two buffers");
        // Bit-reversal permutation.
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j |= bit;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let conj = if inverse { -1.0 } else { 1.0 };
        let mut len = 2;
        while len <= n {
            // A stage of length `len` uses every (self.n / len)-th twiddle.
            let stride = self.n / len;
            let half = len / 2;
            for start in (0..n).step_by(len) {
                for k in 0..half {
                    let (wr, wi) = self.tw[k * stride];
                    let wi = wi * conj;
                    let (a, b) = (start + k, start + k + half);
                    let (xr, xi) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                    re[b] = re[a] - xr;
                    im[b] = im[a] - xi;
                    re[a] += xr;
                    im[a] += xi;
                }
            }
            len <<= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_round_trips_and_finds_a_harmonic() {
        let n = 64;
        let mut re: Vec<f32> = (0..n).map(|i| (std::f32::consts::TAU * 5.0 * i as f32 / n as f32).cos()).collect();
        let orig = re.clone();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im, false);
        // A cosine at bin 5: n/2 in bins 5 and n-5, nothing elsewhere.
        for k in 0..n {
            let mag = re[k].hypot(im[k]);
            let want = if k == 5 || k == n - 5 { n as f32 / 2.0 } else { 0.0 };
            assert!((mag - want).abs() < 1e-3, "bin {k}: {mag}");
        }
        fft(&mut re, &mut im, true);
        for (a, b) in re.iter().zip(&orig) {
            assert!((a / n as f32 - b).abs() < 1e-5);
        }
    }

    #[test]
    fn tanh_fast_is_close() {
        for k in -800..800 {
            let x = k as f32 / 100.0;
            assert!((tanh_fast(x) - x.tanh()).abs() < 2e-3, "x = {x}: {} vs {}", tanh_fast(x), x.tanh());
        }
    }
}
