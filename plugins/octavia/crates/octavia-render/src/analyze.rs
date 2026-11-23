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
    pub peak_db: f32,
    pub rms_db: f32,
    pub centroid_hz: f32,
    /// Energy in 8–16 kHz relative to 1–4 kHz, dB.
    pub air_db: f32,
    /// Energy in 2–5 kHz relative to 100–1000 Hz, dB.
    pub presence_db: f32,
}

pub fn report(x: &[f32], sr: f32) -> Report {
    let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let rms = (x.iter().map(|v| (v * v) as f64).sum::<f64>() / x.len().max(1) as f64).sqrt() as f32;
    const N: usize = 4096;
    let mut bands = [0.0f64; 4];
    let (mut num, mut den) = (0.0f64, 0.0f64);
    let mut pos = 0;
    while pos + N <= x.len() {
        let mut re: Vec<f32> = (0..N)
            .map(|i| x[pos + i] * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()))
            .collect();
        let mut im = vec![0.0; N];
        fft(&mut re, &mut im);
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
        }
        pos += N / 2;
    }
    let db = |a: f64, b: f64| (10.0 * (a.max(1e-20) / b.max(1e-20)).log10()) as f32;
    Report {
        peak_db: 20.0 * peak.max(1e-9).log10(),
        rms_db: 20.0 * rms.max(1e-9).log10(),
        centroid_hz: (num / den.max(1e-20)) as f32,
        air_db: db(bands[0], bands[1]),
        presence_db: db(bands[2], bands[3]),
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
