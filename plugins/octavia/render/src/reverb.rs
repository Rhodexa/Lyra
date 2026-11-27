//! A small 8-line feedback-delay-network room, only so renders sound like a
//! cello in a room instead of in a box. (In Lyra, Lyra's own reverb does this.)

pub struct Reverb {
    lines: Vec<Vec<f32>>,
    idx: Vec<usize>,
    gain: Vec<f32>,
    lp: Vec<f32>,
    damp: f32,
    pre: Vec<f32>,
    pre_idx: usize,
}

impl Reverb {
    pub fn new(sr: f32, rt60: f32) -> Self {
        const BASE: [usize; 8] = [1557, 1617, 1491, 1422, 1277, 1356, 1188, 1116];
        let scale = sr / 44_100.0 * 1.4;
        let lens: Vec<usize> = BASE.iter().map(|&l| (l as f32 * scale) as usize).collect();
        Self {
            gain: lens.iter().map(|&l| 10f32.powf(-3.0 * l as f32 / sr / rt60)).collect(),
            lines: lens.iter().map(|&l| vec![0.0; l]).collect(),
            idx: vec![0; 8],
            lp: vec![0.0; 8],
            damp: 0.35,
            pre: vec![0.0; (sr * 0.018) as usize],
            pre_idx: 0,
        }
    }

    pub fn tick(&mut self, x: f32) -> (f32, f32) {
        let delayed = self.pre[self.pre_idx];
        self.pre[self.pre_idx] = x;
        self.pre_idx = (self.pre_idx + 1) % self.pre.len();

        let mut o = [0.0f32; 8];
        for (k, slot) in o.iter_mut().enumerate() {
            *slot = self.lines[k][self.idx[k]];
        }
        // Fast Hadamard mix (orthogonal, energy-preserving).
        let mut h = o;
        let mut span = 1;
        while span < 8 {
            for i in (0..8).step_by(span * 2) {
                for j in i..i + span {
                    let (a, b) = (h[j], h[j + span]);
                    h[j] = a + b;
                    h[j + span] = a - b;
                }
            }
            span *= 2;
        }
        let norm = 1.0 / 8f32.sqrt();
        for k in 0..8 {
            let v = h[k] * norm;
            self.lp[k] = v + self.damp * (self.lp[k] - v);
            let len = self.lines[k].len();
            self.lines[k][self.idx[k]] = delayed * 0.35 + self.gain[k] * self.lp[k];
            self.idx[k] = (self.idx[k] + 1) % len;
        }
        (o[0] + o[2] + o[4] + o[6], o[1] + o[3] + o[5] + o[7])
    }
}
