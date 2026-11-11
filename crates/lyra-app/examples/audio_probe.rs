//! Audio health probe: opens the default output exactly like Lyra does, plays
//! SILENCE, and measures callback timing. No sound, no window.
//!
//!     cargo run --release -p lyra-app --example audio_probe -- [seconds] [buffer sizes...]
//!     cargo run --release -p lyra-app --example audio_probe -- 5 default 1024 512 256
//!
//! "late" = the gap between two refills was longer than the whole ring buffer
//! (when we know its size), i.e. the device certainly ran dry: an underrun.
//! With the default size we don't know the ring, so we fall back to
//! "gap > 2x the previous callback + 4 ms" (a probable underrun).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Report {
    sizes: BTreeMap<usize, u64>,
    late: u64,
    worst_gap_ms: f64,
    callbacks: u64,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    let mut sizes: Vec<Option<u32>> = args.map(|a| a.parse().ok()).collect();
    if sizes.is_empty() {
        sizes = vec![None, Some(1024), Some(512), Some(256)];
    }

    let host = cpal::default_host();
    let device = host.default_output_device().expect("no output device");
    let supported = device.default_output_config().expect("no output config");
    println!(
        "host {:?} · device {} · {} Hz · {} ch · {:?}\n",
        host.id(),
        device.name().unwrap_or_default(),
        supported.sample_rate().0,
        supported.channels(),
        supported.sample_format()
    );
    if let Ok(configs) = device.supported_output_configs() {
        for c in configs.filter(|c| c.channels() == supported.channels()) {
            println!(
                "  supports {:?} · {} ch · {}–{} Hz",
                c.sample_format(),
                c.channels(),
                c.min_sample_rate().0,
                c.max_sample_rate().0
            );
        }
        println!();
    }
    assert_eq!(supported.sample_format(), cpal::SampleFormat::F32, "probe only handles f32 devices");

    for size in sizes {
        let mut config = supported.config();
        config.buffer_size = size.map_or(cpal::BufferSize::Default, cpal::BufferSize::Fixed);
        let channels = config.channels as usize;
        let rate = config.sample_rate.0 as f64;
        // A Mutex is fine HERE: this is a diagnostic tool, not the engine.
        let report = Arc::new(Mutex::new(Report::default()));
        let r = report.clone();
        let mut last: Option<(Instant, f64)> = None;
        let ring = size.map(|n| n as f64 / rate);

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                data.fill(0.0);
                let now = Instant::now();
                let frames = data.len() / channels;
                let mut rep = r.lock().unwrap();
                if let Some((prev, prev_dur)) = last {
                    let gap = now.duration_since(prev).as_secs_f64();
                    rep.worst_gap_ms = rep.worst_gap_ms.max(gap * 1000.0);
                    let limit = ring.unwrap_or(prev_dur * 2.0 + 0.004);
                    if gap > limit {
                        rep.late += 1;
                    }
                }
                last = Some((now, frames as f64 / rate));
                *rep.sizes.entry(frames).or_default() += 1;
                rep.callbacks += 1;
            },
            |e| eprintln!("  stream error: {e}"),
            None,
        );
        let label = size.map_or("default".to_string(), |n| format!("{n}"));
        match stream {
            Err(e) => println!("buffer {label:>7}: could not open ({e})"),
            Ok(stream) => {
                stream.play().unwrap();
                std::thread::sleep(Duration::from_secs(seconds));
                drop(stream);
                let rep = report.lock().unwrap();
                let common: Vec<String> = rep.sizes.iter().map(|(f, n)| format!("{f}×{n}")).collect();
                println!(
                    "buffer {label:>7}: {:>5} callbacks · late {:>4} · worst gap {:>6.1} ms · callback sizes [{}]",
                    rep.callbacks,
                    rep.late,
                    rep.worst_gap_ms,
                    common.join(", ")
                );
            }
        }
    }
}
