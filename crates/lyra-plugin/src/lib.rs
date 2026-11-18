//! # lyra-plugin
//!
//! The ONE interface every sound unit in Lyra implements: built-in drums,
//! synths, effects — and later, CLAP plugins wrapped to look the same.
//!
//! ## The real-time rule
//! [`Plugin::process`] runs on the audio thread. Treat it like an interrupt
//! handler on a microcontroller: it must never allocate memory, lock a mutex,
//! touch files, print, or wait on anything. Do all of that in `new()` or
//! `activate()`, which run on the UI thread.
//!
//! ## Parameters
//! Parameters are plain `f32`s in their *real* units (Hz, seconds, 0..1 ...).
//! By convention a parameter's `id` equals its index in [`Plugin::params`].

/// Something that happens to a plugin at a precise sample inside a block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    NoteOn {
        key: u8,
        velocity: f32,
    },
    NoteOff {
        key: u8,
    },
    /// Silence everything quickly (transport stop, choke groups, panic).
    AllNotesOff,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimedEvent {
    /// Sample offset inside the current block (0 = first sample).
    pub offset: u32,
    pub event: Event,
}

/// Decibels to a linear gain (0 dB = 1).
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// A linear gain to decibels (silence comes out as -120 dB, not -inf).
pub fn gain_to_db(gain: f32) -> f32 {
    20.0 * gain.max(1e-6).log10()
}

/// How a knob maps its 0..1 travel onto a parameter's range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    Linear,
    /// For frequencies and times: equal knob travel = equal *ratio*. `min` must be > 0.
    Exponential,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamInfo {
    pub id: u32,
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    /// "Hz", "s", "%", "pan", "st" (semitones) or "" — used for display only.
    pub unit: &'static str,
    pub curve: Curve,
    /// Non-empty = a choice parameter: the value is an index into these labels.
    pub labels: &'static [&'static str],
}

impl ParamInfo {
    pub const fn new(
        id: u32,
        name: &'static str,
        min: f32,
        max: f32,
        default: f32,
        unit: &'static str,
        curve: Curve,
    ) -> Self {
        Self { id, name, min, max, default, unit, curve, labels: &[] }
    }

    /// A parameter that picks one of `labels` (value = index).
    pub const fn choice(id: u32, name: &'static str, labels: &'static [&'static str], default: u32) -> Self {
        Self {
            id,
            name,
            min: 0.0,
            max: (labels.len() - 1) as f32,
            default: default as f32,
            unit: "",
            curve: Curve::Linear,
            labels,
        }
    }

    pub fn is_choice(&self) -> bool {
        !self.labels.is_empty()
    }

    /// Real value -> knob position (0..1).
    pub fn to_normalized(&self, value: f32) -> f32 {
        let v = value.clamp(self.min, self.max);
        match self.curve {
            Curve::Linear => (v - self.min) / (self.max - self.min),
            Curve::Exponential => (v / self.min).ln() / (self.max / self.min).ln(),
        }
    }

    /// Knob position (0..1) -> real value.
    pub fn from_normalized(&self, n: f32) -> f32 {
        let n = n.clamp(0.0, 1.0);
        let v = match self.curve {
            Curve::Linear => self.min + n * (self.max - self.min),
            Curve::Exponential => self.min * (self.max / self.min).powf(n),
        };
        if self.is_choice() {
            v.round()
        } else {
            v
        }
    }

    /// True for parameters centred on zero (pan, detune...): knobs draw from the middle.
    pub fn is_bipolar(&self) -> bool {
        self.min < 0.0 && self.max > 0.0
    }

    /// Human-readable value, e.g. "1.20 kHz", "45 ms", "L 30".
    pub fn format(&self, v: f32) -> String {
        if self.is_choice() {
            let i = (v.round().max(0.0) as usize).min(self.labels.len() - 1);
            return self.labels[i].to_string();
        }
        match self.unit {
            "Hz" if v >= 1000.0 => format!("{:.2} kHz", v / 1000.0),
            "Hz" => format!("{v:.0} Hz"),
            "s" if v < 1.0 => format!("{:.0} ms", v * 1000.0),
            "s" => format!("{v:.2} s"),
            "%" => format!("{:.0}%", v * 100.0),
            "st" => format!("{v:+.0} st"),
            "ct" => format!("{v:+.0} ct"),
            "oct" => format!("{v:+.2} oct"),
            "dB" => format!("{v:+.1} dB"),
            "pan" if v.abs() < 0.01 => "Centre".to_string(),
            "pan" if v < 0.0 => format!("L {:.0}", -v * 100.0),
            "pan" => format!("R {:.0}", v * 100.0),
            "note" => {
                const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
                let k = v.round().clamp(0.0, 127.0) as i32;
                format!("{}{}", NAMES[k as usize % 12], k / 12 - 1)
            }
            _ => format!("{v:.2}"),
        }
    }
}

/// A titled run of parameters, for laying out an editor ("Filter": ids 6..11).
#[derive(Clone, Debug)]
pub struct ParamGroup {
    pub name: &'static str,
    pub ids: std::ops::Range<u32>,
}

/// Where the song is, given to plugins before each block (see [`Plugin::transport`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transport {
    pub tempo: f64,
    pub playing: bool,
    /// Position at the start of this block, in ticks (96 per quarter note).
    pub position: f64,
}

/// Everything a plugin gets for one block of audio.
///
/// * **Instruments** must overwrite every sample of `out_l` / `out_r`.
/// * **Effects** process *in place*: the buffers arrive holding the input
///   signal, and the effect writes its result back into them.
pub struct ProcessContext<'a> {
    pub sample_rate: f32,
    /// Sorted by `offset`.
    pub events: &'a [TimedEvent],
    /// Output buffers (input too, for effects — see above).
    pub out_l: &'a mut [f32],
    pub out_r: &'a mut [f32],
}

/// Decoded audio, ready to play: one buffer per side (a mono file has the
/// same samples in both). Shared between the UI and audio threads by `Arc`;
/// decoded on the UI thread, never on the audio thread.
#[derive(Debug, Default)]
pub struct SampleData {
    pub sample_rate: f32,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl SampleData {
    pub fn len(&self) -> usize {
        self.left.len()
    }
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }
    pub fn seconds(&self) -> f32 {
        self.len() as f32 / self.sample_rate.max(1.0)
    }
}

pub trait Plugin: Send {
    fn name(&self) -> &'static str;
    fn params(&self) -> &'static [ParamInfo];
    /// Optional layout hint for editors: named groups of parameter ids.
    fn groups(&self) -> &'static [ParamGroup] {
        &[]
    }
    fn set_param(&mut self, id: u32, value: f32);
    /// Called on the UI thread before the plugin is handed to the audio thread.
    fn activate(&mut self, sample_rate: f32);
    /// REAL-TIME. See the module docs.
    fn process(&mut self, ctx: ProcessContext<'_>);
    /// REAL-TIME. Tempo and song position, called right before `process`.
    /// Tempo-synced plugins (delays, sidechain pumps) override this.
    fn transport(&mut self, _transport: Transport) {}
    /// Give the plugin a sample to play (samplers). Called on the UI thread,
    /// before the plugin goes to the audio thread. Others ignore it.
    fn set_sample(&mut self, _sample: std::sync::Arc<SampleData>) {}
}

/// Default values of a parameter list, in id order.
pub fn default_params(params: &[ParamInfo]) -> Vec<f32> {
    params.iter().map(|p| p.default).collect()
}
