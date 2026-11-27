//! Numbers to check a render by, without ears.

/// In-place radix-2 FFT. `re.len()` must be a power of two.
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
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
    let mut len = 2;
    while len <= n {
        let ang = -std::f32::consts::TAU / len as f32;
        let (ws, wc) = ang.sin_cos();
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wc - ci * ws;
                ci = cr * ws + ci * wc;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

pub struct Report {
    /// Long-term spectrum per octave band (63 Hz … 16 kHz), dB relative to the loudest band.
    pub octaves: [f32; 9],
    pub peak_db: f32,
    pub rms_db: f32,
    pub centroid_hz: f32,
    /// Energy in 8–16 kHz relative to 1–4 kHz, dB.
    pub air_db: f32,
    /// Energy in 2–5 kHz relative to 100–1000 Hz, dB.
    pub presence_db: f32,
    /// Noise between the overtones, 200 Hz–4 kHz: the quiet floor of each
    /// third-octave band (its 20th-percentile bin) relative to the band's
    /// total, over the loud frames. A pure tone is around −40 dB or lower,
    /// pure noise about −7 dB.
    pub floor_db: f32,
}

pub fn report(x: &[f32], sr: f32) -> Report {
    let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let rms = (x.iter().map(|v| (v * v) as f64).sum::<f64>() / x.len().max(1) as f64).sqrt() as f32;
    const N: usize = 4096;
    let mut bands = [0.0f64; 4];
    let mut oct = [0.0f64; 9];
    let (mut num, mut den) = (0.0f64, 0.0f64);
    let (mut floor, mut floor_total) = (0.0f64, 0.0f64);
    // First pass: frame energies, to skip the quiet frames.
    let mut energies = Vec::new();
    let mut pos = 0;
    while pos + N <= x.len() {
        energies.push(x[pos..pos + N].iter().map(|v| (v * v) as f64).sum::<f64>());
        pos += N / 2;
    }
    let loudest = energies.iter().cloned().fold(0.0, f64::max);
    let mut frame = 0;
    let mut pos = 0;
    while pos + N <= x.len() {
        let mut re: Vec<f32> = (0..N)
            .map(|i| x[pos + i] * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()))
            .collect();
        let mut im = vec![0.0; N];
        fft(&mut re, &mut im);
        if energies[frame] > loudest * 0.01 {
            let mut lo = 200.0f32;
            while lo < 4000.0 {
                let hi = lo * 2f32.powf(1.0 / 3.0);
                let (k0, k1) = ((lo / sr * N as f32) as usize, (hi / sr * N as f32) as usize);
                let mut p: Vec<f64> = (k0..k1.max(k0 + 1)).map(|k| (re[k] * re[k] + im[k] * im[k]) as f64).collect();
                let total: f64 = p.iter().sum();
                p.sort_by(|a, b| a.total_cmp(b));
                floor += p[p.len() / 5] * p.len() as f64;
                floor_total += total;
                lo = hi;
            }
        }
        frame += 1;
        for k in 1..N / 2 {
            let f = k as f32 * sr / N as f32;
            let p = (re[k] * re[k] + im[k] * im[k]) as f64;
            num += p * f as f64;
            den += p;
            let band = |lo: f32, hi: f32| f >= lo && f < hi;
            if band(8000.0, 16000.0) {
                bands[0] += p;
            }
            if band(1000.0, 4000.0) {
                bands[1] += p;
            }
            if band(2000.0, 5000.0) {
                bands[2] += p;
            }
            if band(100.0, 1000.0) {
                bands[3] += p;
            }
            for (b, slot) in oct.iter_mut().enumerate() {
                let centre = 62.5 * 2f32.powi(b as i32);
                if band(centre / 2f32.sqrt(), centre * 2f32.sqrt()) {
                    *slot += p;
                }
            }
        }
        pos += N / 2;
    }
    let db = |a: f64, b: f64| (10.0 * (a.max(1e-20) / b.max(1e-20)).log10()) as f32;
    let top = oct.iter().cloned().fold(1e-20, f64::max);
    Report {
        octaves: oct.map(|v| db(v, top)),
        peak_db: 20.0 * peak.max(1e-9).log10(),
        rms_db: 20.0 * rms.max(1e-9).log10(),
        centroid_hz: (num / den.max(1e-20)) as f32,
        air_db: db(bands[0], bands[1]),
        presence_db: db(bands[2], bands[3]),
        floor_db: db(floor, floor_total),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_finds_a_sine() {
        let n = 1024;
        let mut re: Vec<f32> = (0..n).map(|i| (std::f32::consts::TAU * 32.0 * i as f32 / n as f32).sin()).collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        let mags: Vec<f32> = (0..n / 2).map(|k| re[k].hypot(im[k])).collect();
        let top = mags.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert_eq!(top, 32);
    }
}
