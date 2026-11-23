//! Curves → mono audio. The instrument half of the pipeline.
//!
//! ```text
//!  frames ─▶ BowVoice ─┐
//!                      ├─▶ (+ sympathetic strings) ─▶ Body ─▶ out
//!  plucks ─▶ Plucks ───┘
//! ```

use crate::body::Body;
use crate::curves::{Curves, FRAME_RATE};
use crate::instrument::Instrument;
use crate::pluck::Plucks;
use crate::sympathetic::Sympathetic;
use crate::voice::BowVoice;

pub fn render(curves: &Curves, inst: &Instrument, sr: f32, seed: u64) -> Vec<f32> {
    let frames = &curves.frames;
    if frames.len() < 2 {
        return Vec::new();
    }
    let at = |f: usize| (f as f64 * sr as f64 / FRAME_RATE).round() as usize;
    let len = at(frames.len() - 1);
    let mut bow = vec![0.0f32; len];

    let mut voice = BowVoice::new(sr, seed);
    for f in 0..frames.len() - 1 {
        voice.process(&frames[f], &frames[f + 1], &mut bow[at(f)..at(f + 1)]);
    }

    let mut plucks = Plucks::new(sr, inst.pluck_ring, seed);
    let mut sym = Sympathetic::new(&inst.open_strings, sr);
    let mut body = Body::new(inst, sr, seed);
    let mut next_pluck = 0;
    let mut out = bow;
    for (i, s) in out.iter_mut().enumerate() {
        while let Some(p) = curves.plucks.get(next_pluck) {
            if (p.time * sr as f64) as usize > i {
                break;
            }
            plucks.trigger(p);
            next_pluck += 1;
        }
        let bridge = *s + plucks.tick();
        let ring = sym.tick(bridge);
        *s = body.process(bridge + ring);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::cello;
    use crate::performer::{perform, Style};
    use crate::score::Score;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn renders_clean_audible_audio() {
        let inst = cello();
        let s = Score::parse("tempo 80\n(G2:e D3 B3) A3 B3 D3 B3 D3 pizz G2:q D3 arco C3:h").unwrap();
        let c = perform(&s, &inst, &Style::default());
        let out = render(&c, &inst, 48_000.0, 1);
        assert!(out.iter().all(|v| v.is_finite()));
        let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.01 && peak < 4.0, "peak {peak}");
        // The pizzicato section is audible too (notes 9-10 at ~3.0-3.75 s).
        let pizz = &out[(3.05 * 48_000.0) as usize..(3.7 * 48_000.0) as usize];
        assert!(rms(pizz) > 0.005, "pizz rms {}", rms(pizz));
    }

    #[test]
    fn same_seed_same_sound() {
        let inst = cello();
        let s = Score::parse("C3:q E3 G3").unwrap();
        let c = perform(&s, &inst, &Style::default());
        assert_eq!(render(&c, &inst, 44_100.0, 7), render(&c, &inst, 44_100.0, 7));
    }
}
