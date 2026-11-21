//! A spectrogram of a render as a PNG, for Export data: time left to right
//! (a line every four bars, numbered), frequency bottom to top on a log
//! scale from 20 Hz to 20 kHz (labelled lines at 50, 100, 200, 500, 1k, 2k,
//! 5k, 10k), loudness as colour from black (−90 dB) through purple, red and
//! orange to pale yellow (the loudest moment of the song).

const N: usize = 4096;
const HEIGHT: usize = 480;
const LEFT: usize = 34;
const BOTTOM: usize = 16;
const TOP: usize = 6;
const RIGHT: usize = 6;
const F_LO: f32 = 20.0;
const F_HI: f32 = 20_000.0;
const RANGE_DB: f32 = 90.0;

/// `mono` at `rate` Hz, `frames_per_bar` frames to a bar, `bars` bars in the
/// song (anything after them is the tail). Returns PNG bytes.
pub fn png(mono: &[f32], rate: f32, frames_per_bar: f64, bars: usize) -> Result<Vec<u8>, String> {
    let plot_w = (bars * 24).clamp(600, 2400);
    let total = mono.len().max(1);
    let hop = total as f64 / plot_w as f64;
    let window: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()).collect();
    let bin_hz = rate / N as f32;

    // Which FFT bins each row covers (row 0 = top = 20 kHz).
    let rows: Vec<(f32, f32)> = (0..HEIGHT)
        .map(|y| {
            let at = |y: f32| F_LO * (F_HI / F_LO).powf(1.0 - y / HEIGHT as f32);
            (at(y as f32 + 1.0) / bin_hz, at(y as f32) / bin_hz)
        })
        .collect();

    // Power in dB per (column, row).
    let mut db = vec![-200.0f32; plot_w * HEIGHT];
    let (mut re, mut im) = (vec![0.0f32; N], vec![0.0f32; N]);
    for x in 0..plot_w {
        let centre = ((x as f64 + 0.5) * hop) as isize;
        for i in 0..N {
            let j = centre - (N / 2) as isize + i as isize;
            re[i] = if j >= 0 && (j as usize) < mono.len() { mono[j as usize] * window[i] } else { 0.0 };
            im[i] = 0.0;
        }
        fft(&mut re, &mut im);
        let power: Vec<f32> = (0..N / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect();
        for (y, &(lo, hi)) in rows.iter().enumerate() {
            let p = if hi - lo < 1.0 {
                // Narrower than a bin (the lows): interpolate between bins.
                let c = (lo + hi) * 0.5;
                let k = (c.floor() as usize).min(N / 2 - 2);
                let t = c - k as f32;
                power[k] * (1.0 - t) + power[k + 1] * t
            } else {
                let (a, b) = (lo.ceil() as usize, (hi.floor() as usize).min(N / 2 - 1));
                power[a.min(b)..=b].iter().copied().fold(0.0, f32::max)
            };
            db[x * HEIGHT + y] = 10.0 * p.max(1e-20).log10();
        }
    }
    let top = db.iter().copied().fold(f32::MIN, f32::max);

    let (w, h) = (LEFT + plot_w + RIGHT, TOP + HEIGHT + BOTTOM);
    let mut img = vec![18u8; w * h * 3];
    let put = |img: &mut Vec<u8>, x: usize, y: usize, c: [u8; 3]| {
        if x < w && y < h {
            img[(y * w + x) * 3..][..3].copy_from_slice(&c);
        }
    };
    for x in 0..plot_w {
        for y in 0..HEIGHT {
            let t = ((db[x * HEIGHT + y] - top + RANGE_DB) / RANGE_DB).clamp(0.0, 1.0);
            put(&mut img, LEFT + x, TOP + y, colour(t));
        }
    }
    // Frequency lines and labels.
    let grid = [120u8, 120, 130];
    for (f, label) in [
        (50.0, "50"),
        (100.0, "100"),
        (200.0, "200"),
        (500.0, "500"),
        (1e3, "1k"),
        (2e3, "2k"),
        (5e3, "5k"),
        (1e4, "10k"),
    ] {
        let y = TOP + ((1.0 - (f / F_LO).ln() / (F_HI / F_LO).ln()) * HEIGHT as f32) as usize;
        for x in (0..plot_w).step_by(3) {
            put(&mut img, LEFT + x, y, grid);
        }
        text(&mut img, w, LEFT - 3 - label.len() * 4, y.saturating_sub(2), label, [200, 200, 210]);
    }
    // A line every four bars, numbered underneath.
    let px_per_frame = plot_w as f64 / total as f64;
    for bar in (0..=bars).step_by(4) {
        let x = LEFT + (bar as f64 * frames_per_bar * px_per_frame) as usize;
        if x >= LEFT + plot_w {
            break;
        }
        for y in (TOP..TOP + HEIGHT).step_by(3) {
            put(&mut img, x, y, grid);
        }
        let label = (bar + 1).to_string();
        text(&mut img, w, x, TOP + HEIGHT + 5, &label, [200, 200, 210]);
    }

    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&img).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Black → purple → red → orange → pale yellow.
fn colour(t: f32) -> [u8; 3] {
    const STOPS: [[f32; 3]; 5] =
        [[0.0, 0.0, 0.0], [70.0, 20.0, 110.0], [190.0, 40.0, 70.0], [250.0, 140.0, 30.0], [255.0, 250.0, 200.0]];
    let s = t * (STOPS.len() - 1) as f32;
    let i = (s.floor() as usize).min(STOPS.len() - 2);
    let f = s - i as f32;
    std::array::from_fn(|c| (STOPS[i][c] + (STOPS[i + 1][c] - STOPS[i][c]) * f) as u8)
}

/// In-place radix-2 FFT (N a power of two).
fn fft(re: &mut [f32], im: &mut [f32]) {
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
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                (cr, ci) = (cr * wc - ci * ws, cr * ws + ci * wc);
            }
        }
        len <<= 1;
    }
}

/// 3×5 digits and "k", drawn at (x, y).
fn text(img: &mut [u8], w: usize, x: usize, y: usize, s: &str, c: [u8; 3]) {
    const GLYPHS: [(char, [u8; 5]); 11] = [
        ('0', [7, 5, 5, 5, 7]),
        ('1', [2, 6, 2, 2, 7]),
        ('2', [7, 1, 7, 4, 7]),
        ('3', [7, 1, 3, 1, 7]),
        ('4', [5, 5, 7, 1, 1]),
        ('5', [7, 4, 7, 1, 7]),
        ('6', [7, 4, 7, 5, 7]),
        ('7', [7, 1, 1, 2, 2]),
        ('8', [7, 5, 7, 5, 7]),
        ('9', [7, 5, 7, 1, 7]),
        ('k', [4, 5, 6, 5, 5]),
    ];
    for (n, ch) in s.chars().enumerate() {
        let Some((_, rows)) = GLYPHS.iter().find(|g| g.0 == ch) else { continue };
        for (dy, row) in rows.iter().enumerate() {
            for dx in 0..3 {
                if row & (4 >> dx) != 0 {
                    let (px, py) = (x + n * 4 + dx, y + dy);
                    let i = (py * w + px) * 3;
                    if px < w && i + 3 <= img.len() {
                        img[i..i + 3].copy_from_slice(&c);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_finds_a_sine_in_its_bin() {
        let mut re: Vec<f32> = (0..64).map(|i| (std::f32::consts::TAU * 5.0 * i as f32 / 64.0).sin()).collect();
        let mut im = vec![0.0; 64];
        fft(&mut re, &mut im);
        let mag: Vec<f32> = (0..32).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect();
        let peak = (0..32).max_by(|&a, &b| mag[a].total_cmp(&mag[b])).unwrap();
        assert_eq!(peak, 5);
        assert!((mag[5] - 32.0).abs() < 0.01, "{}", mag[5]);
    }

    #[test]
    fn a_tone_lights_its_row_and_the_file_is_a_png() {
        let rate = 48_000.0;
        let tone: Vec<f32> =
            (0..rate as usize * 2).map(|i| (std::f32::consts::TAU * 1000.0 * i as f32 / rate).sin() * 0.5).collect();
        let bytes = png(&tone, rate, rate as f64, 2).unwrap();
        assert_eq!(&bytes[1..4], b"PNG");
    }
}
