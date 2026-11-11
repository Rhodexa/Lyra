//! How expensive is the engine? Renders the demo groove as fast as possible
//! with realistic buffer sizes and reports CPU cost per callback.
//!
//!     cargo run --release -p lyra-engine --example bench

use lyra_engine::*;
use std::time::Instant;

/// (instrument id, param tweaks, volume, choke group)
type ChannelSpec = (&'static str, &'static [(u32, f32)], f32, u8);

fn main() {
    let sr = 48_000.0f32;
    let (mut eng, mut h) = create(sr);
    let channels: [ChannelSpec; 5] = [
        ("lyra.kick", &[], 0.9, 0),
        ("lyra.clap", &[], 0.7, 0),
        ("lyra.hat", &[(1, 0.045), (2, 8000.0)], 0.5, 1),
        ("lyra.hat", &[(1, 0.32), (2, 6500.0)], 0.45, 1),
        ("lyra.snare", &[(0, 220.0), (3, 0.12)], 0.45, 0),
    ];
    for (i, (id, tweaks, volume, choke)) in channels.iter().enumerate() {
        let mut p = lyra_dsp::create(id).unwrap();
        for &(pid, v) in *tweaks {
            p.set_param(pid, v);
        }
        h.send(Command::AddChannel(ChannelSlot::new(p, sr)));
        h.send(Command::SetMix { channel: i, volume: *volume, pan: 0.0, mute: false, choke: *choke });
    }
    let mut notes = vec![];
    let mut n = |s: u32, c: u16, v: f32| notes.push(SeqNote { start: s * 24, len: 24, channel: c, key: 60, velocity: v });
    for s in 0..16 {
        if s % 4 == 0 { n(s, 0, 1.0) }
        if s == 4 || s == 12 { n(s, 1, 0.9) }
        if s % 4 == 2 { n(s, 3, 0.7) } else { n(s, 2, 0.5) }
    }
    h.send(Command::SetSequence(Box::new(Sequence { length: 16 * 24, notes })));
    h.send(Command::SetTempo(124.0));
    h.send(Command::Play);

    for frames in [64usize, 256, 1024] {
        let mut buf = vec![0.0f32; frames * 2];
        let seconds = 20.0;
        let calls = (seconds * sr / frames as f32) as usize;
        let (mut total, mut worst) = (0.0f64, 0.0f64);
        for _ in 0..calls {
            let t = Instant::now();
            eng.process(&mut buf, 2);
            let dt = t.elapsed().as_secs_f64();
            total += dt;
            worst = worst.max(dt);
        }
        let budget = frames as f64 / sr as f64;
        println!(
            "{frames:5} frames: avg {:6.1} µs = {:5.2}% of the {:5.2} ms budget · worst {:6.1} µs ({:5.2}%) · {:.0}x realtime",
            total / calls as f64 * 1e6,
            total / calls as f64 / budget * 100.0,
            budget * 1000.0,
            worst * 1e6,
            worst / budget * 100.0,
            seconds as f64 / total
        );
        h.collect_garbage();
    }
}
