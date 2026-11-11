//! Opens the sound card and moves the [`Engine`] into its callback.
//! After `start()` returns, the UI only ever talks to the engine through the
//! [`EngineHandle`] — plus the read-only [`AudioStats`] below.
//!
//! ## Buffer size (measured, not guessed — see `examples/audio_probe.rs`)
//! cpal's ALSA backend turns a requested size N into an N-frame ring refilled in
//! N/4 slices, from a normal-priority thread. On PipeWire with all 16 cores busy:
//! * 256 → real underruns (ring 5.8 ms, refills arrived up to 6 ms apart)
//! * 512 → none, 4.6 ms of margin  ← default
//! * 1024 → none, lots of margin (but sluggish for live playing)
//! * "system default" → ~43 ms callbacks: safe but laggy
//!
//! Override with `LYRA_BUFFER=<frames>`, e.g. `LYRA_BUFFER=1024 cargo run --release`.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use lyra_engine::{Engine, EngineHandle};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Instant;

const DEFAULT_BUFFER: u32 = 512;
/// PipeWire's graph usually runs at 48 kHz; matching it avoids resampling.
const PREFERRED_RATE: u32 = 48_000;

pub struct Audio {
    /// Dropping this stops the sound. Keep it alive for the app's lifetime.
    _stream: cpal::Stream,
    pub host: String,
    pub device_name: String,
    pub sample_rate: u32,
    /// The ring size we asked for (None = the system's default).
    pub requested: Option<u32>,
    pub stats: Arc<AudioStats>,
}

/// Health of the audio callback, measured *inside* it (atomics only).
///
/// cpal's ALSA backend silently recovers from underruns, so we detect them
/// ourselves. Each callback refills the ring completely, so if the gap between
/// two callbacks is longer than the whole ring, the device certainly ran dry.
#[derive(Default)]
pub struct AudioStats {
    /// Frames in the most recent callback (what the system really gave us).
    pub frames: AtomicU32,
    /// Worst DSP load since last read, in permille of the callback's time budget.
    max_load: AtomicU32,
    /// Underruns detected, total.
    pub late: AtomicU64,
    pub callbacks: AtomicU64,
    /// Errors reported by the audio backend, total.
    pub errors: AtomicU64,
}

impl AudioStats {
    /// Worst DSP load (0.0–1.0+) since the last call; resets it.
    pub fn take_max_load(&self) -> f32 {
        self.max_load.swap(0, Relaxed) as f32 / 1000.0
    }
}

pub fn start() -> Result<(Audio, EngineHandle), String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no audio output device found")?;
    let device_name = device.name().unwrap_or_else(|_| "unknown device".into());
    let default = device.default_output_config().map_err(|e| format!("audio config: {e}"))?;
    let format = default.sample_format();

    // Prefer 48 kHz in the device's default format, if the device offers it.
    let supports_preferred = device.supported_output_configs().is_ok_and(|mut configs| {
        configs.any(|c| {
            c.sample_format() == format
                && c.channels() == default.channels()
                && (c.min_sample_rate().0..=c.max_sample_rate().0).contains(&PREFERRED_RATE)
        })
    });
    let mut config = default.config();
    if supports_preferred {
        config.sample_rate = cpal::SampleRate(PREFERRED_RATE);
    }
    let sample_rate = config.sample_rate.0;

    let requested = std::env::var("LYRA_BUFFER")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(DEFAULT_BUFFER);
    // Try our size first; if the device refuses it, fall back to the system default.
    let mut last_err = String::new();
    for size in [Some(requested), None] {
        config.buffer_size = match size {
            Some(n) => cpal::BufferSize::Fixed(n),
            None => cpal::BufferSize::Default,
        };
        let stats = Arc::new(AudioStats::default());
        let (engine, handle) = lyra_engine::create(sample_rate as f32);
        match build(&device, &config, format, engine, stats.clone(), size) {
            Ok(stream) => {
                stream.play().map_err(|e| format!("audio start: {e}"))?;
                let audio = Audio {
                    _stream: stream,
                    host: format!("{:?}", host.id()),
                    device_name,
                    sample_rate,
                    requested: size,
                    stats,
                };
                return Ok((audio, handle));
            }
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

fn build(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
    engine: Engine,
    stats: Arc<AudioStats>,
    ring: Option<u32>,
) -> Result<cpal::Stream, String> {
    match format {
        cpal::SampleFormat::F32 => build_typed::<f32>(device, config, engine, stats, ring),
        cpal::SampleFormat::I16 => build_typed::<i16>(device, config, engine, stats, ring),
        cpal::SampleFormat::I32 => build_typed::<i32>(device, config, engine, stats, ring),
        cpal::SampleFormat::U16 => build_typed::<u16>(device, config, engine, stats, ring),
        other => Err(format!("unsupported sample format {other:?}")),
    }
}

fn build_typed<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut engine: Engine,
    stats: Arc<AudioStats>,
    ring: Option<u32>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0 as f64;
    // Dev switch: run everything, but send silence to the speakers.
    let silent = std::env::var("LYRA_SILENT").is_ok_and(|v| !v.is_empty() && v != "0");
    let ring_seconds = ring.map(|n| n as f64 / sample_rate);
    // Allocated once, here, before the callback ever runs.
    let mut scratch = vec![0.0f32; 4096 * channels];
    let mut last: Option<(Instant, f64)> = None; // (start of previous callback, its audio duration)
    let error_stats = stats.clone();

    device
        .build_output_stream(
            config,
            // ↓ This closure IS the audio thread. Real-time rules apply.
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let start = Instant::now(); // vDSO clock read: no syscall, no allocation
                let frames = data.len() / channels.max(1);
                let duration = frames as f64 / sample_rate;

                if let Some((prev_start, prev_duration)) = last {
                    let gap = start.duration_since(prev_start).as_secs_f64();
                    // Ring size unknown (system default)? Fall back to a looser guess.
                    let limit = ring_seconds.unwrap_or(prev_duration * 2.0 + 0.004);
                    if gap > limit {
                        stats.late.fetch_add(1, Relaxed);
                    }
                }
                last = Some((start, duration));

                for chunk in data.chunks_mut(scratch.len()) {
                    let buf = &mut scratch[..chunk.len()];
                    engine.process(buf, channels);
                    for (out, s) in chunk.iter_mut().zip(buf.iter()) {
                        *out = T::from_sample(if silent { 0.0 } else { *s });
                    }
                }

                if duration > 0.0 {
                    let load = (start.elapsed().as_secs_f64() / duration * 1000.0) as u32;
                    stats.max_load.fetch_max(load, Relaxed);
                }
                stats.frames.store(frames as u32, Relaxed);
                stats.callbacks.fetch_add(1, Relaxed);
            },
            move |err| {
                error_stats.errors.fetch_add(1, Relaxed);
                eprintln!("lyra: audio stream error: {err}");
            },
            None,
        )
        .map_err(|e| e.to_string())
}
