//! Curves → mono audio. The instrument half of the pipeline.
//!
//! ```text
//!  frames ─▶ BowVoice ─┬─ overtones (body applied per overtone) ─────────┬─▶ out
//!                      └─ bow noise ─┐                                    │
//!  plucks ─▶ Plucks ─────────────────┴─▶ (+ sympathetic strings) ─▶ Body ─┘
//! ```

use crate::body::{Body, BodyResponse};
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
    let mut tone = vec![0.0f32; len];
    let mut noise = vec![0.0f32; len];

    let mut voice = BowVoice::new(sr, BodyResponse::new(inst, seed), seed);
    for f in 0..frames.len() - 1 {
        let (s0, s1) = (at(f), at(f + 1));
        voice.process(&frames[f], &frames[f + 1], &mut tone[s0..s1], &mut noise[s0..s1]);
    }

    let mut plucks = Plucks::new(sr, inst.pluck_ring, seed);
    let mut sym = Sympathetic::new(&inst.open_strings, sr);
    let mut body = Body::new(inst, sr, seed);
    let mut next_pluck = 0;
    let mut out = tone;
    for (i, s) in out.iter_mut().enumerate() {
        while let Some(p) = curves.plucks.get(next_pluck) {
            if (p.time * sr as f64) as usize > i {
                break;
            }
            plucks.trigger(p);
            next_pluck += 1;
        }
        let raw = noise[i] + plucks.tick();
        // The open strings hear everything at the bridge, tone included.
        let ring = sym.tick(raw + *s);
        *s += body.process(raw + ring);
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

#[cfg(test)]
mod probe {
    use crate::body::{Body, BodyResponse};
    use crate::instrument::cello;
    use crate::performer::{perform, Style};
    use crate::score::Score;
    use crate::sympathetic::Sympathetic;
    use crate::voice::BowVoice;
    use crate::curves::FRAME_RATE;

    #[test]
    #[ignore]
    fn how_loud_are_the_parts() {
        let inst = cello();
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../scores/prelude.txt")).unwrap();
        let c = perform(&Score::parse(&text).unwrap(), &inst, &Style::default());
        let sr = 48_000.0;
        let at = |f: usize| (f as f64 * sr as f64 / FRAME_RATE).round() as usize;
        let len = at(c.frames.len() - 1);
        let (mut tone, mut noise) = (vec![0.0; len], vec![0.0; len]);
        let mut v = BowVoice::new(sr, BodyResponse::new(&inst, 1), 1);
        for f in 0..c.frames.len() - 1 {
            let (a, b) = (at(f), at(f + 1));
            v.process(&c.frames[f], &c.frames[f + 1], &mut tone[a..b], &mut noise[a..b]);
        }
        let mut sym = Sympathetic::new(&inst.open_strings, sr);
        let mut body = Body::new(&inst, sr, 1);
        let (mut ring_out, mut noise_out) = (vec![0.0f32; len], vec![0.0f32; len]);
        for i in 0..len {
            let ring = sym.tick(noise[i] + tone[i]);
            ring_out[i] = body.process(ring);
        }
        let mut body2 = Body::new(&inst, sr, 1);
        for i in 0..len {
            noise_out[i] = body2.process(noise[i]);
        }
        let db = |x: &[f32]| 10.0 * (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).log10();
        println!("tone {:.1} dB | sympathetic {:.1} dB | bow noise {:.1} dB", db(&tone), db(&ring_out), db(&noise_out));
    }
}
