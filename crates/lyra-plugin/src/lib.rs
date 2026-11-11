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
    NoteOn { key: u8, velocity: f32 },
    NoteOff { key: u8 },
    /// Silence everything quickly (transport stop, choke groups, panic).
    AllNotesOff,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimedEvent {
    /// Sample offset inside the current block (0 = first sample).
    pub offset: u32,
    pub event: Event,
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
        Self { id, name, min, max, default, unit, curve }
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
        match self.curve {
            Curve::Linear => self.min + n * (self.max - self.min),
            Curve::Exponential => self.min * (self.max / self.min).powf(n),
        }
    }

    /// True for parameters centred on zero (pan, detune...): knobs draw from the middle.
    pub fn is_bipolar(&self) -> bool {
        self.min < 0.0 && self.max > 0.0
    }

    /// Human-readable value, e.g. "1.20 kHz", "45 ms", "L 30".
    pub fn format(&self, v: f32) -> String {
        match self.unit {
            "Hz" if v >= 1000.0 => format!("{:.2} kHz", v / 1000.0),
            "Hz" => format!("{v:.0} Hz"),
            "s" if v < 1.0 => format!("{:.0} ms", v * 1000.0),
            "s" => format!("{v:.2} s"),
            "%" => format!("{:.0}%", v * 100.0),
            "st" => format!("{v:+.0} st"),
            "pan" if v.abs() < 0.01 => "Centre".to_string(),
            "pan" if v < 0.0 => format!("L {:.0}", -v * 100.0),
            "pan" => format!("R {:.0}", v * 100.0),
            _ => format!("{v:.2}"),
        }
    }
}

/// Everything a plugin gets for one block of audio.
pub struct ProcessContext<'a> {
    pub sample_rate: f32,
    /// Sorted by `offset`.
    pub events: &'a [TimedEvent],
    /// Output buffers; the plugin must OVERWRITE every sample (not add).
    pub out_l: &'a mut [f32],
    pub out_r: &'a mut [f32],
}

pub trait Plugin: Send {
    fn name(&self) -> &'static str;
    fn params(&self) -> &'static [ParamInfo];
    fn set_param(&mut self, id: u32, value: f32);
    /// Called on the UI thread before the plugin is handed to the audio thread.
    fn activate(&mut self, sample_rate: f32);
    /// REAL-TIME. See the module docs.
    fn process(&mut self, ctx: ProcessContext<'_>);
}

/// Default values of a parameter list, in id order.
pub fn default_params(params: &[ParamInfo]) -> Vec<f32> {
    params.iter().map(|p| p.default).collect()
}
