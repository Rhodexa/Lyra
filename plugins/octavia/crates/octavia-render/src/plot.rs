//! Just enough raster drawing to see curves and spectrograms. No fonts:
//! the lanes are always in the same order (see the README).

use octavia_core::{Curves, Score, FRAME_RATE};
use std::path::Path;

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    px: Vec<[u8; 3]>,
}

const BG: [u8; 3] = [24, 24, 27];
const GRID: [u8; 3] = [48, 48, 54];

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Self { w, h, px: vec![BG; w * h] }
    }
    pub fn set(&mut self, x: i64, y: i64, c: [u8; 3]) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }
    pub fn rect(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, c: [u8; 3]) {
        for y in y0.min(y1)..=y0.max(y1) {
            for x in x0.min(x1)..=x0.max(x1) {
                self.set(x, y, c);
            }
        }
    }
    pub fn line(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, c: [u8; 3]) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.set(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), self.w as u32, self.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(std::io::Error::other)?;
        let data: Vec<u8> = self.px.iter().flatten().copied().collect();
        writer.write_image_data(&data).map_err(std::io::Error::other)?;
        Ok(())
    }
}

/// Four lanes, top to bottom: pitch (with the written notes behind it),
/// loudness, brightness, noise. One vertical grid line per second.
pub fn curves_png(curves: &Curves, score: Option<&Score>, path: &Path) -> std::io::Result<()> {
    let secs = curves.duration();
    let w = ((secs * 160.0) as usize).clamp(800, 6000);
    let lane = 140usize;
    let gap = 10usize;
    let mut c = Canvas::new(w, lane * 5 + gap * 4);
    let x_of = |t: f64| (t / secs * w as f64) as i64;
    for s in 0..=secs as usize {
        let x = x_of(s as f64);
        c.line(x, 0, x, c.h as i64, GRID);
    }

    // Pitch lane is double height.
    let ptop = 0i64;
    let ph = (lane * 2) as f64;
    let pitches: Vec<f32> = curves.frames.iter().filter(|f| f.loudness > 0.01).map(|f| f.pitch).collect();
    let (mut lo, mut hi) = pitches.iter().fold((f32::MAX, f32::MIN), |(a, b), &p| (a.min(p), b.max(p)));
    if let Some(s) = score {
        for n in &s.notes {
            lo = lo.min(n.pitch);
            hi = hi.max(n.pitch);
        }
    }
    if lo > hi {
        (lo, hi) = (48.0, 60.0);
    }
    let (lo, hi) = (lo - 1.5, hi + 1.5);
    let y_of_pitch = |p: f32| ptop + ((1.0 - (p - lo) / (hi - lo)) as f64 * ph) as i64;
    for k in lo.ceil() as i32..=hi.floor() as i32 {
        if k % 12 == 0 {
            let y = y_of_pitch(k as f32);
            c.line(0, y, w as i64, y, [70, 70, 90]);
        }
    }
    if let Some(s) = score {
        for n in &s.notes {
            let y = y_of_pitch(n.pitch);
            let col = if n.slur { [70, 60, 120] } else { [60, 80, 90] };
            c.rect(x_of(n.start), y - 3, x_of(n.end()) - 2, y + 3, col);
        }
    }
    let lanes: [(usize, [u8; 3]); 3] = [(2, [240, 180, 90]), (3, [140, 200, 255]), (4, [200, 140, 220])];
    let mut prev: Option<(i64, i64)> = None;
    for (i, f) in curves.frames.iter().enumerate() {
        let x = x_of(i as f64 / FRAME_RATE);
        if f.loudness > 0.01 {
            let y = y_of_pitch(f.pitch);
            if let Some((px, py)) = prev {
                c.line(px, py, x, y, [255, 255, 255]);
            }
            prev = Some((x, y));
        } else {
            prev = None;
        }
    }
    for (row, col) in lanes {
        let top = (row * lane + (row - 1) * gap) as i64;
        c.line(0, top, w as i64, top, GRID);
        let mut prev: Option<(i64, i64)> = None;
        for (i, f) in curves.frames.iter().enumerate() {
            let v = match row {
                2 => f.loudness,
                3 => f.brightness,
                _ => f.noise,
            };
            let x = x_of(i as f64 / FRAME_RATE);
            let y = top + ((1.0 - v.clamp(0.0, 1.0)) * (lane - 2) as f32) as i64;
            if let Some((px, py)) = prev {
                c.line(px, py, x, y, col);
            }
            prev = Some((x, y));
        }
    }
    for p in &curves.plucks {
        let x = x_of(p.time);
        c.rect(x - 1, y_of_pitch(p.pitch) - 5, x + 1, y_of_pitch(p.pitch) + 5, [120, 230, 150]);
    }
    c.save(path)
}

/// Log-frequency spectrogram, 40 Hz at the bottom to 16 kHz at the top.
pub fn spectrogram_png(x: &[f32], sr: f32, path: &Path) -> std::io::Result<()> {
    const N: usize = 4096;
    let hop = 512;
    let cols = x.len().saturating_sub(N) / hop + 1;
    let h = 400;
    let mut c = Canvas::new(cols.max(1), h);
    let win: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()).collect();
    let mut mags: Vec<Vec<f32>> = Vec::with_capacity(cols);
    let mut peak = 1e-12f32;
    for col in 0..cols {
        let mut re: Vec<f32> = (0..N).map(|i| x.get(col * hop + i).copied().unwrap_or(0.0) * win[i]).collect();
        let mut im = vec![0.0f32; N];
        crate::analyze::fft(&mut re, &mut im);
        let m: Vec<f32> = (0..N / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect();
        peak = m.iter().fold(peak, |a, &b| a.max(b));
        mags.push(m);
    }
    let (f_lo, f_hi) = (40.0f32, 16_000.0f32);
    for (col, m) in mags.iter().enumerate() {
        for y in 0..h {
            let f = f_lo * (f_hi / f_lo).powf(1.0 - y as f32 / (h - 1) as f32);
            let bin = ((f / sr * N as f32) as usize).min(N / 2 - 1);
            let db = 10.0 * (m[bin] / peak).max(1e-12).log10();
            let v = ((db + 90.0) / 90.0).clamp(0.0, 1.0);
            c.set(col as i64, y as i64, heat(v));
        }
    }
    // Octave lines at the C's (65, 131, 262 … Hz).
    let mut f = 65.41f32;
    while f < f_hi {
        let y = ((1.0 - (f / f_lo).ln() / (f_hi / f_lo).ln()) * (h - 1) as f32) as i64;
        for xx in (0..cols as i64).step_by(4) {
            c.set(xx, y, [90, 90, 90]);
        }
        f *= 2.0;
    }
    c.save(path)
}

fn heat(v: f32) -> [u8; 3] {
    let stops: [(f32, [f32; 3]); 5] = [
        (0.0, [0.0, 0.0, 4.0]),
        (0.3, [60.0, 15.0, 110.0]),
        (0.55, [180.0, 50.0, 100.0]),
        (0.8, [250.0, 140.0, 40.0]),
        (1.0, [252.0, 253.0, 190.0]),
    ];
    for w in stops.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        if v <= b {
            let t = (v - a) / (b - a);
            return [0, 1, 2].map(|i| (ca[i] + (cb[i] - ca[i]) * t) as u8);
        }
    }
    [252, 253, 190]
}
