//! Wavetables: up to 256 single-cycle *frames* of 2048 samples. Each frame is
//! kept at [`LEVELS`] band-limited copies, half an octave apart, so whatever
//! note plays, the oscillator can pick a copy whose top harmonic stays below
//! Nyquist (no aliasing from the table itself).
//!
//! Tables are built on the UI thread (they allocate and run FFTs) and are
//! read-only afterwards: the built-in set lives in one static, an imported
//! table in an `Arc` the plugin holds.

use lyra_dsp::Fft;
use lyra_plugin::SampleData;
use std::f32::consts::PI;
use std::sync::OnceLock;

/// Samples in one frame (one cycle) of a table, and the import's frame size.
pub const FRAME: usize = 2048;
/// The most frames a table may have.
pub const MAX_FRAMES: usize = 256;
/// Band-limited copies per frame: level `L` keeps harmonics up to ⌊1024 / 2^(L/2)⌋.
pub const LEVELS: usize = 20;

/// The highest harmonic each level keeps.
pub const fn level_harmonics(level: usize) -> usize {
    // ⌊1024 / 2^(L/2)⌋: even levels halve, odd ones sit a √2 between.
    let even = 1024 >> (level / 2);
    if level.is_multiple_of(2) {
        even
    } else {
        // ÷√2 ≈ × 0.70710678, in integers.
        let v = even * 7071 / 10000;
        if v == 0 {
            1
        } else {
            v
        }
    }
}

/// Samples per cycle at a level: 32 per top harmonic, at most a full frame,
/// never below 256. Linear interpolation leaves images of the harmonics
/// (around multiples of the size) that fold back as noise; at 32× they stay
/// near −80 dB, at 4× they reached −46 dB.
pub const fn level_size(level: usize) -> usize {
    let want = (32 * level_harmonics(level)).next_power_of_two();
    if want > FRAME {
        FRAME
    } else if want < 256 {
        256
    } else {
        want
    }
}

/// Extra samples stored around every frame (see [`Level::size`]).
pub const GUARD: usize = 3;

#[derive(Clone, Copy, Debug, Default)]
pub struct Level {
    /// Highest harmonic present.
    pub harmonics: usize,
    /// Samples per cycle (a power of two). Each frame is stored with three
    /// more ([`GUARD`]): the last sample before the first, and the first two
    /// after the last, so neither linear nor cubic interpolation ever wraps.
    pub size: usize,
    offset: usize,
}

pub struct Table {
    frames: usize,
    levels: [Level; LEVELS],
    data: Vec<f32>,
}

impl Table {
    pub fn frames(&self) -> usize {
        self.frames
    }

    pub fn level(&self, level: usize) -> Level {
        self.levels[level]
    }

    /// One frame at one level: `size + 1` samples (the last repeats the first).
    pub fn frame(&self, level: usize, frame: usize) -> &[f32] {
        let lv = self.levels[level];
        let start = lv.offset + frame.min(self.frames - 1) * (lv.size + GUARD) + 1;
        &self.data[start..start + lv.size + 1]
    }

    /// Every frame of a level, back to back (`size + GUARD` apart, each
    /// starting with its guard sample): the oscillator's view.
    pub fn level_data(&self, level: usize) -> &[f32] {
        let lv = self.levels[level];
        &self.data[lv.offset..lv.offset + self.frames * (lv.size + GUARD)]
    }

    /// The fullest level a note at `hz` can use without a harmonic at or
    /// above Nyquist; `None` when even its fundamental is too high to play.
    pub fn level_for(hz: f32, sample_rate: f32) -> Option<usize> {
        let nyquist = 0.5 * sample_rate;
        (0..LEVELS).find(|&l| level_harmonics(l) as f32 * hz < nyquist)
    }

    /// The oscillator's lookup: fractional frame `pos` (0 ..= frames − 1) at
    /// `phase` (0..1), linear in both. For tests and pictures; the voice
    /// inlines the same arithmetic.
    pub fn sample(&self, level: usize, pos: f32, phase: f32) -> f32 {
        let lv = self.levels[level];
        let pos = pos.clamp(0.0, (self.frames - 1) as f32);
        let f0 = pos as usize;
        let f1 = (f0 + 1).min(self.frames - 1);
        let ff = pos - f0 as f32;
        let x = phase.rem_euclid(1.0) * lv.size as f32;
        let j = (x as usize).min(lv.size - 1);
        let fr = x - j as f32;
        let (a, b) = (self.frame(level, f0), self.frame(level, f1));
        let sa = a[j] + (a[j + 1] - a[j]) * fr;
        let sb = b[j] + (b[j + 1] - b[j]) * fr;
        sa + (sb - sa) * ff
    }

    /// Build from spectra: `spectra[f]` is frame `f`'s forward FFT at
    /// [`FRAME`] points, bins `0..=FRAME/2` as (re, im). DC is dropped.
    /// `normalize_each`: every frame peaks at 1 (else the loudest one does).
    pub fn from_spectra(spectra: &[Spectrum], normalize_each: bool) -> Table {
        let frames = spectra.len().clamp(1, MAX_FRAMES);
        let mut levels = [Level::default(); LEVELS];
        let mut offset = 0;
        for (l, lv) in levels.iter_mut().enumerate() {
            let size = level_size(l);
            *lv = Level { harmonics: level_harmonics(l), size, offset };
            offset += frames * (size + GUARD);
        }
        let mut data = vec![0.0f32; offset];
        let (mut re, mut im) = (vec![0.0f32; FRAME], vec![0.0f32; FRAME]);
        let plan = Fft::new(FRAME);
        let silent = Spectrum::silent();
        for f in 0..frames {
            let s = spectra.get(f).unwrap_or(&silent);
            for lv in &levels {
                let n = lv.size;
                let (re, im) = (&mut re[..n], &mut im[..n]);
                re.fill(0.0);
                im.fill(0.0);
                // Bin n/2 is the level's own Nyquist: keep strictly below it.
                let top = lv.harmonics.min(n / 2 - 1).min(s.re.len() - 1);
                for k in 1..=top {
                    re[k] = s.re[k];
                    im[k] = s.im[k];
                    re[n - k] = s.re[k];
                    im[n - k] = -s.im[k];
                }
                plan.run(re, im, true);
                let out = &mut data[lv.offset + f * (n + GUARD)..lv.offset + (f + 1) * (n + GUARD)];
                for m in 0..n {
                    out[m + 1] = re[m] / FRAME as f32;
                }
                out[0] = out[n];
                out[n + 1] = out[1];
                out[n + 2] = out[2];
            }
        }
        let mut t = Table { frames, levels, data };
        t.normalize(normalize_each);
        t
    }

    /// Build from time-domain frames of [`FRAME`] samples each.
    pub fn from_frames(frames: &[Vec<f32>], normalize_each: bool) -> Table {
        let spectra: Vec<Spectrum> = frames.iter().map(|f| Spectrum::of(f, 1)).collect();
        Table::from_spectra(&spectra, normalize_each)
    }

    /// An imported WAV as a table: cut into frames of [`FRAME`] (a sample
    /// shorter than one frame is stretched to fill it; more than
    /// [`MAX_FRAMES`] frames are thinned out evenly). Both sides are mixed.
    pub fn from_sample(sample: &SampleData) -> Option<Table> {
        let n = sample.len();
        if n < 2 {
            return None;
        }
        let mono = |i: usize| 0.5 * (sample.left[i] + sample.right.get(i).copied().unwrap_or(sample.left[i]));
        let frames: Vec<Vec<f32>> = if n < FRAME {
            let one = (0..FRAME)
                .map(|m| {
                    let x = m as f32 * n as f32 / FRAME as f32;
                    let (j, fr) = (x as usize, x.fract());
                    mono(j) + (mono((j + 1) % n) - mono(j)) * fr
                })
                .collect();
            vec![one]
        } else {
            let whole = n / FRAME;
            let count = whole.min(MAX_FRAMES);
            (0..count)
                .map(|i| {
                    let src = i * whole / count;
                    (0..FRAME).map(|m| mono(src * FRAME + m)).collect()
                })
                .collect()
        };
        Some(Table::from_frames(&frames, false))
    }

    fn normalize(&mut self, each: bool) {
        let peak_of = |t: &Table, f: usize| t.frame(0, f).iter().fold(0.0f32, |m, x| m.max(x.abs()));
        let global = (0..self.frames).map(|f| peak_of(self, f)).fold(0.0f32, f32::max);
        for f in 0..self.frames {
            let peak = if each { peak_of(self, f) } else { global };
            if peak < 1e-9 {
                continue;
            }
            let g = 1.0 / peak;
            for l in 0..LEVELS {
                let lv = self.levels[l];
                let start = lv.offset + f * (lv.size + GUARD);
                self.data[start..start + lv.size + GUARD].iter_mut().for_each(|x| *x *= g);
            }
        }
    }
}

/// One frame's spectrum at [`FRAME`] points: bins `0..=FRAME/2`.
#[derive(Clone)]
pub struct Spectrum {
    pub re: Vec<f32>,
    pub im: Vec<f32>,
}

impl Spectrum {
    pub fn silent() -> Spectrum {
        Spectrum { re: vec![0.0; FRAME / 2 + 1], im: vec![0.0; FRAME / 2 + 1] }
    }

    /// Additive: harmonic `h` (1-based) is `amp · sin(2π·h·t + phase)`.
    pub fn additive(harmonics: impl IntoIterator<Item = (usize, f32, f32)>) -> Spectrum {
        let mut s = Spectrum::silent();
        let half = FRAME as f32 / 2.0;
        for (h, amp, phase) in harmonics {
            if (1..=FRAME / 2 - 1).contains(&h) {
                s.re[h] += half * amp * phase.sin();
                s.im[h] -= half * amp * phase.cos();
            }
        }
        s
    }

    /// The spectrum of one cycle sampled `oversample` × [`FRAME`] times
    /// (`cycle.len()` = that many samples). Oversampling a shape with hard
    /// edges keeps its folded-back harmonics far below hearing.
    pub fn of(cycle: &[f32], oversample: usize) -> Spectrum {
        let n = FRAME * oversample;
        assert_eq!(cycle.len(), n);
        let (mut re, mut im) = (cycle.to_vec(), vec![0.0; n]);
        Fft::new(n).run(&mut re, &mut im, false);
        let k = 1.0 / oversample as f32;
        Spectrum {
            re: re[..=FRAME / 2].iter().map(|x| x * k).collect(),
            im: im[..=FRAME / 2].iter().map(|x| x * k).collect(),
        }
    }

    /// Sample `f(t)` (t in 0..1) over one cycle, `oversample` × [`FRAME`] points.
    pub fn of_fn(oversample: usize, f: impl Fn(f32) -> f32) -> Spectrum {
        let n = FRAME * oversample;
        let cycle: Vec<f32> = (0..n).map(|i| f(i as f32 / n as f32)).collect();
        Spectrum::of(&cycle, oversample)
    }
}

// ───────────────────────────── the built-in set ─────────────────────────────

/// The table choices, as saved (an index). New built-ins go at the *end*
/// (after "Imported"); the editor can list them in any order.
pub const TABLE_NAMES: [&str; 10] =
    ["Basic", "Pulse", "Sweep", "Vowels", "Growl", "FM", "Sync", "Digital", "Organ", "Imported"];
/// The choice that plays the channel's imported WAV.
pub const IMPORTED: usize = 9;

/// Every built-in table, built on first use (UI thread: `Chrysalis::new`
/// calls this so the audio thread only ever reads it).
pub fn builtin() -> &'static [Table] {
    static ALL: OnceLock<Vec<Table>> = OnceLock::new();
    ALL.get_or_init(|| vec![basic(), pulse(), sweep(), vowels(), growl(), fm(), sync(), digital(), organ()])
}

/// Sine, triangle, saw, square: the morph between neighbours is the crossfade.
fn basic() -> Table {
    let tri = Spectrum::additive((1..1024).step_by(2).map(|h| {
        let sign = if (h / 2) % 2 == 0 { 1.0 } else { -1.0 };
        (h, sign * 8.0 / (PI * PI * (h * h) as f32), 0.0)
    }));
    Table::from_spectra(&[Spectrum::additive([(1, 1.0, 0.0)]), tri, saw_spectrum(|_| 1.0), square_spectrum()], true)
}

/// A saw (rising through zero at t = 0, the jump at ½), harmonic `h` scaled by `shape(h)`.
fn saw_spectrum(shape: impl Fn(usize) -> f32) -> Spectrum {
    Spectrum::additive((1..1024).map(|h| {
        let sign = if h % 2 == 1 { 1.0 } else { -1.0 };
        (h, sign * 2.0 / (PI * h as f32) * shape(h), 0.0)
    }))
}

fn square_spectrum() -> Spectrum {
    Spectrum::additive((1..1024).step_by(2).map(|h| (h, 4.0 / (PI * h as f32), 0.0)))
}

/// Pulse width from 50 % down to 4 %.
fn pulse() -> Table {
    let frames: Vec<Spectrum> = (0..32)
        .map(|f| {
            let d = 0.5 - 0.46 * f as f32 / 31.0;
            Spectrum::additive(
                (1..1024).map(|h| (h, 4.0 / (PI * h as f32) * (PI * h as f32 * d).sin(), 0.5 * PI - PI * h as f32 * d)),
            )
        })
        .collect();
    Table::from_spectra(&frames, true)
}

/// A saw through a resonant low-pass that opens from the 2nd harmonic to the top.
fn sweep() -> Table {
    let frames: Vec<Spectrum> = (0..64)
        .map(|f| {
            let c = 1.5 * 2f32.powf(9.0 * f as f32 / 63.0);
            saw_spectrum(|h| {
                let r = h as f32 / c;
                let q = 2.5;
                1.0 / (((1.0 - r * r).powi(2) + (r / q).powi(2)).sqrt() * (1.0 + r * r))
            })
        })
        .collect();
    Table::from_spectra(&frames, true)
}

/// Formants (Hz), gains, bandwidths (Hz) of a tenor's A E I O U.
pub const VOWELS: [[(f32, f32, f32); 3]; 5] = [
    [(650.0, 1.0, 80.0), (1080.0, 0.50, 90.0), (2650.0, 0.45, 120.0)],
    [(400.0, 1.0, 70.0), (1700.0, 0.20, 80.0), (2600.0, 0.25, 100.0)],
    [(290.0, 1.0, 40.0), (1870.0, 0.18, 90.0), (2800.0, 0.13, 100.0)],
    [(400.0, 1.0, 70.0), (800.0, 0.32, 80.0), (2600.0, 0.25, 100.0)],
    [(350.0, 1.0, 40.0), (600.0, 0.10, 60.0), (2700.0, 0.14, 100.0)],
];

/// The formants at vowel position `v` (0 = A … 1 = U): frequencies and
/// bandwidths glide geometrically, gains linearly.
pub fn vowel_at(v: f32) -> [(f32, f32, f32); 3] {
    let x = v.clamp(0.0, 1.0) * 4.0;
    let i = (x as usize).min(3);
    let t = x - i as f32;
    let (a, b) = (VOWELS[i], VOWELS[i + 1]);
    std::array::from_fn(|k| {
        let geo = |p: f32, q: f32| p * (q / p).powf(t);
        (geo(a[k].0, b[k].0), a[k].1 + (b[k].1 - a[k].1) * t, geo(a[k].2, b[k].2))
    })
}

/// A voice singing A E I O U at 110 Hz: the vowels glide into each other.
fn vowels() -> Table {
    let f0 = 110.0;
    let frames: Vec<Spectrum> = (0..64)
        .map(|f| {
            let fm = vowel_at(f as f32 / 63.0);
            Spectrum::additive((1..200).map(|h| {
                let hz = f0 * h as f32;
                let env: f32 = fm.iter().map(|&(c, g, bw)| g / (1.0 + ((hz - c) / (0.75 * bw + 40.0)).powi(2))).sum();
                (h, env / (h as f32).powf(0.6), 0.0)
            }))
        })
        .collect();
    Table::from_spectra(&frames, true)
}

/// Two resonances moving through a saw in opposite directions, the odd
/// harmonics growing on the way: a "yoy" from one end to the other.
fn growl() -> Table {
    let frames: Vec<Spectrum> = (0..64)
        .map(|f| {
            let t = f as f32 / 63.0;
            let p1 = 2.0 * 7f32.powf(t);
            let p2 = 24.0 * (5.0f32 / 24.0).powf(t);
            let peak = |h: f32, p: f32| 1.0 / (1.0 + ((h - p) / (0.35 * p)).powi(2));
            saw_spectrum(|h| {
                let x = h as f32;
                let odd = if h % 2 == 1 { 1.0 + t } else { 1.0 };
                (0.08 + peak(x, p1) + 0.7 * peak(x, p2)) * x.sqrt() * odd
            })
        })
        .collect();
    Table::from_spectra(&frames, true)
}

/// Sine phase-modulated by itself (ratio 1), the index rising from 0 to 9.
fn fm() -> Table {
    let frames: Vec<Spectrum> = (0..64)
        .map(|f| {
            let beta = 9.0 * (f as f32 / 63.0).powi(2);
            Spectrum::of_fn(1, |t| (2.0 * PI * t + beta * (2.0 * PI * t).sin()).sin())
        })
        .collect();
    Table::from_spectra(&frames, true)
}

/// A saw hard-synced to the cycle, its own pitch rising from 1× to 7×.
fn sync() -> Table {
    let frames: Vec<Spectrum> = (0..32)
        .map(|f| {
            let ratio = 1.0 + 6.0 * f as f32 / 31.0;
            Spectrum::of_fn(8, |t| 2.0 * (t * ratio).fract() - 1.0)
        })
        .collect();
    Table::from_spectra(&frames, true)
}

/// A sine with fewer and fewer levels: 64 steps down to 2 (a square).
fn digital() -> Table {
    let frames: Vec<Spectrum> = (0..32)
        .map(|f| {
            let steps = 2f32.powf(6.0 - 5.0 * f as f32 / 31.0).round();
            Spectrum::of_fn(8, |t| {
                let s = (2.0 * PI * t).sin();
                ((s * steps * 0.5).round() / (steps * 0.5)).clamp(-1.0, 1.0)
            })
        })
        .collect();
    Table::from_spectra(&frames, true)
}

/// Drawbars 8′ 4′ 2⅔′ 2′ 1⅗′ 1⅓′ 1′ (harmonics 1 2 3 4 5 6 8), from a
/// lone flute to everything out.
fn organ() -> Table {
    const HARM: [usize; 7] = [1, 2, 3, 4, 5, 6, 8];
    const KEYS: [[f32; 7]; 4] = [
        [8.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [8.0, 6.0, 0.0, 4.0, 0.0, 0.0, 0.0],
        [8.0, 8.0, 6.0, 8.0, 0.0, 0.0, 4.0],
        [8.0, 8.0, 8.0, 8.0, 6.0, 6.0, 8.0],
    ];
    let frames: Vec<Spectrum> = (0..16)
        .map(|f| {
            let x = f as f32 / 15.0 * 3.0;
            let i = (x as usize).min(2);
            let t = x - i as f32;
            Spectrum::additive((0..7).map(|d| {
                let level = KEYS[i][d] + (KEYS[i + 1][d] - KEYS[i][d]) * t;
                // A drawbar step is 3 dB; fully in is silent.
                let amp = if level < 0.05 { 0.0 } else { 10f32.powf((level - 8.0) * 3.0 / 20.0) };
                (HARM[d], amp, 0.0)
            }))
        })
        .collect();
    Table::from_spectra(&frames, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_halve_every_two_steps_and_fit_their_size() {
        assert_eq!(level_harmonics(0), 1024);
        assert_eq!(level_harmonics(1), 724);
        assert_eq!(level_harmonics(2), 512);
        assert_eq!(level_harmonics(LEVELS - 1), 1);
        for l in 0..LEVELS {
            assert!(level_harmonics(l) < level_size(l) / 2 || l == 0, "level {l}");
            if l > 0 {
                assert!(level_harmonics(l) <= level_harmonics(l - 1));
            }
        }
    }

    #[test]
    fn a_frame_loops_and_the_last_sample_repeats_the_first() {
        let t = &builtin()[0];
        for l in 0..LEVELS {
            for f in 0..t.frames() {
                let fr = t.frame(l, f);
                assert_eq!(fr[0], fr[fr.len() - 1]);
            }
        }
    }

    #[test]
    fn additive_sine_is_a_sine() {
        let t = Table::from_spectra(&[Spectrum::additive([(1, 1.0, 0.0)])], true);
        for l in 0..LEVELS {
            for k in 0..64 {
                let ph = k as f32 / 64.0;
                let want = (2.0 * PI * ph).sin();
                assert!((t.sample(l, 0.0, ph) - want).abs() < 1e-3, "level {l} phase {ph}");
            }
        }
    }

    #[test]
    fn a_short_sample_becomes_one_frame_and_a_long_one_many() {
        let saw = |n: usize| SampleData {
            sample_rate: 48_000.0,
            left: (0..n).map(|i| (i % 300) as f32 / 150.0 - 1.0).collect(),
            right: (0..n).map(|i| (i % 300) as f32 / 150.0 - 1.0).collect(),
        };
        assert_eq!(Table::from_sample(&saw(300)).unwrap().frames(), 1);
        assert_eq!(Table::from_sample(&saw(FRAME * 10 + 5)).unwrap().frames(), 10);
        assert_eq!(Table::from_sample(&saw(FRAME * 600)).unwrap().frames(), MAX_FRAMES);
    }

    #[test]
    fn every_builtin_is_normalized_and_finite() {
        for (t, name) in builtin().iter().zip(TABLE_NAMES) {
            for f in 0..t.frames() {
                let peak = t.frame(0, f).iter().fold(0.0f32, |m, x| m.max(x.abs()));
                assert!((peak - 1.0).abs() < 1e-3, "{name} frame {f}: peak {peak}");
                for l in 0..LEVELS {
                    assert!(t.frame(l, f).iter().all(|x| x.is_finite()), "{name}");
                }
            }
        }
    }
}
