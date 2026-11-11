//! Opens the sound card and moves the [`Engine`] into its callback.
//! After `start()` returns, the UI only ever talks to the engine through the
//! [`EngineHandle`].

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Sample;
use lyra_engine::{Engine, EngineHandle};

pub struct Audio {
    /// Dropping this stops the sound. Keep it alive for the app's lifetime.
    _stream: cpal::Stream,
    pub device_name: String,
    pub sample_rate: u32,
    pub buffer: String,
}

pub fn start() -> Result<(Audio, EngineHandle), String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no audio output device found")?;
    let device_name = device.name().unwrap_or_else(|_| "unknown device".into());
    let supported = device.default_output_config().map_err(|e| format!("audio config: {e}"))?;
    let format = supported.sample_format();
    let mut config = supported.config();
    let sample_rate = config.sample_rate.0;

    // Ask for a small buffer (~5 ms) so live playing feels instant;
    // fall back to the device default if the device refuses.
    let mut last_err = String::new();
    for fixed in [Some(256u32), None] {
        config.buffer_size = match fixed {
            Some(n) => cpal::BufferSize::Fixed(n),
            None => cpal::BufferSize::Default,
        };
        let (engine, handle) = lyra_engine::create(sample_rate as f32);
        match build(&device, &config, format, engine) {
            Ok(stream) => {
                stream.play().map_err(|e| format!("audio start: {e}"))?;
                let buffer = fixed.map_or("default buffer".into(), |n| format!("{n} frames"));
                return Ok((Audio { _stream: stream, device_name, sample_rate, buffer }, handle));
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
) -> Result<cpal::Stream, String> {
    match format {
        cpal::SampleFormat::F32 => build_typed::<f32>(device, config, engine),
        cpal::SampleFormat::I16 => build_typed::<i16>(device, config, engine),
        cpal::SampleFormat::I32 => build_typed::<i32>(device, config, engine),
        cpal::SampleFormat::U16 => build_typed::<u16>(device, config, engine),
        other => Err(format!("unsupported sample format {other:?}")),
    }
}

fn build_typed<T>(device: &cpal::Device, config: &cpal::StreamConfig, mut engine: Engine) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    // Allocated once, here, before the callback ever runs.
    let mut scratch = vec![0.0f32; 4096 * channels];

    device
        .build_output_stream(
            config,
            // ↓ This closure IS the audio thread. Real-time rules apply.
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for chunk in data.chunks_mut(scratch.len()) {
                    let buf = &mut scratch[..chunk.len()];
                    engine.process(buf, channels);
                    for (out, s) in chunk.iter_mut().zip(buf.iter()) {
                        *out = T::from_sample(*s);
                    }
                }
            },
            |err| eprintln!("lyra: audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}
