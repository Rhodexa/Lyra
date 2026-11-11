//! # lyra-dsp
//!
//! Lyra's built-in instruments (and, from milestone 3, effects).
//! Everything here implements [`lyra_plugin::Plugin`] — the same interface a
//! hosted CLAP plugin will get — so the engine never cares where sound comes from.

pub mod drums;
pub mod synth;
pub mod util;

use lyra_plugin::{ParamGroup, ParamInfo, Plugin, ProcessContext};

/// A built-in instrument the user can add from the browser / "+" menu.
pub struct InstrumentInfo {
    /// Stable id saved in project files. Never rename one.
    pub id: &'static str,
    pub name: &'static str,
    /// Drums play one-shots on steps; melodic instruments live in the piano roll.
    pub melodic: bool,
}

pub const INSTRUMENTS: &[InstrumentInfo] = &[
    InstrumentInfo { id: "lyra.kick", name: "Kick", melodic: false },
    InstrumentInfo { id: "lyra.snare", name: "Snare", melodic: false },
    InstrumentInfo { id: "lyra.clap", name: "Clap", melodic: false },
    InstrumentInfo { id: "lyra.hat", name: "Hi-hat", melodic: false },
    InstrumentInfo { id: "lyra.synth", name: "Lyra Synth", melodic: true },
];

pub fn info(id: &str) -> Option<&'static InstrumentInfo> {
    INSTRUMENTS.iter().find(|i| i.id == id)
}

/// Build an instrument by id. Runs on the UI thread (it allocates).
pub fn create(id: &str) -> Option<Box<dyn Plugin>> {
    Some(match id {
        "lyra.kick" => Box::new(drums::Kick::new()),
        "lyra.snare" => Box::new(drums::Snare::new()),
        "lyra.clap" => Box::new(drums::Clap::new()),
        "lyra.hat" => Box::new(drums::Hat::new()),
        "lyra.synth" => Box::new(synth::Synth::new()),
        _ => return None,
    })
}

/// Like [`create`], but an unknown id (e.g. a project from a newer Lyra)
/// becomes a silent placeholder instead of breaking the channel list.
pub fn create_or_silence(id: &str) -> Box<dyn Plugin> {
    create(id).unwrap_or_else(|| Box::new(Silence))
}

/// Parameter descriptions for an instrument id (empty if unknown).
pub fn params(id: &str) -> &'static [ParamInfo] {
    create(id).map_or(&[], |p| p.params())
}

/// Editor layout groups for an instrument id (empty = one flat group).
pub fn groups(id: &str) -> &'static [ParamGroup] {
    create(id).map_or(&[], |p| p.groups())
}

/// Default parameter values for an instrument id (empty if unknown).
pub fn default_params(id: &str) -> Vec<f32> {
    lyra_plugin::default_params(params(id))
}

/// A named starting point: an instrument plus the parameters that differ from its defaults.
pub struct Preset {
    pub name: &'static str,
    pub instrument: &'static str,
    pub values: &'static [(usize, f32)],
}

impl Preset {
    pub fn params(&self) -> Vec<f32> {
        let mut v = default_params(self.instrument);
        for &(i, x) in self.values {
            if let Some(slot) = v.get_mut(i) {
                *slot = x;
            }
        }
        v
    }
}

use synth::p as s;

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Deep Bass",
        instrument: "lyra.synth",
        values: &[
            (s::OCTAVE, 1.0), (s::SUB, 0.55), (s::CUTOFF, 240.0), (s::RESO, 0.3), (s::ENV_AMT, 0.45),
            (s::F_DECAY, 0.22), (s::F_SUSTAIN, 0.1), (s::ATTACK, 0.002), (s::DECAY, 0.3), (s::SUSTAIN, 0.6),
            (s::RELEASE, 0.07), (s::KEYTRACK, 0.3), (s::LEVEL, 0.9),
        ],
    },
    Preset {
        name: "Organ Bass",
        instrument: "lyra.synth",
        values: &[
            (s::WAVE, 1.0), (s::OCTAVE, 1.0), (s::SUB, 0.35), (s::CUTOFF, 700.0), (s::RESO, 0.1), (s::ENV_AMT, 0.25),
            (s::F_DECAY, 0.18), (s::ATTACK, 0.002), (s::DECAY, 0.25), (s::SUSTAIN, 0.45), (s::RELEASE, 0.06),
            (s::LEVEL, 0.8),
        ],
    },
    Preset {
        name: "Chord Stab",
        instrument: "lyra.synth",
        values: &[
            (s::VOICES, 2.0), (s::DETUNE, 0.2), (s::SPREAD, 0.5), (s::CUTOFF, 1300.0), (s::RESO, 0.3),
            (s::ENV_AMT, 0.5), (s::F_DECAY, 0.2), (s::F_SUSTAIN, 0.1), (s::ATTACK, 0.002), (s::DECAY, 0.35),
            (s::SUSTAIN, 0.25), (s::RELEASE, 0.2), (s::LEVEL, 0.6),
        ],
    },
    Preset {
        name: "Pluck",
        instrument: "lyra.synth",
        values: &[
            (s::VOICES, 1.0), (s::DETUNE, 0.15), (s::CUTOFF, 450.0), (s::RESO, 0.35), (s::ENV_AMT, 0.7),
            (s::F_DECAY, 0.16), (s::F_SUSTAIN, 0.0), (s::ATTACK, 0.001), (s::DECAY, 0.35), (s::SUSTAIN, 0.0),
            (s::RELEASE, 0.25), (s::LEVEL, 0.7),
        ],
    },
    Preset {
        name: "Supersaw Pad",
        instrument: "lyra.synth",
        values: &[
            (s::VOICES, 6.0), (s::DETUNE, 0.45), (s::SPREAD, 1.0), (s::CUTOFF, 2800.0), (s::RESO, 0.1),
            (s::ENV_AMT, 0.15), (s::F_DECAY, 1.5), (s::F_SUSTAIN, 0.6), (s::ATTACK, 0.6), (s::DECAY, 1.2),
            (s::SUSTAIN, 0.8), (s::RELEASE, 1.6), (s::LEVEL, 0.45),
        ],
    },
    Preset {
        name: "Square Lead",
        instrument: "lyra.synth",
        values: &[
            (s::WAVE, 1.0), (s::VOICES, 1.0), (s::DETUNE, 0.1), (s::CUTOFF, 3200.0), (s::RESO, 0.2),
            (s::ENV_AMT, 0.3), (s::F_DECAY, 0.4), (s::SUSTAIN, 0.8), (s::RELEASE, 0.15), (s::LEVEL, 0.55),
        ],
    },
    Preset {
        name: "Sub",
        instrument: "lyra.synth",
        values: &[
            (s::WAVE, 3.0), (s::OCTAVE, 1.0), (s::CUTOFF, 18_000.0), (s::ENV_AMT, 0.0), (s::ATTACK, 0.003),
            (s::DECAY, 0.2), (s::SUSTAIN, 0.9), (s::RELEASE, 0.1), (s::LEVEL, 0.9),
        ],
    },
];

pub fn presets_for(instrument: &str) -> impl Iterator<Item = &'static Preset> + '_ {
    PRESETS.iter().filter(move |p| p.instrument == instrument)
}

pub struct Silence;

impl Plugin for Silence {
    fn name(&self) -> &'static str {
        "Missing plugin"
    }
    fn params(&self) -> &'static [ParamInfo] {
        &[]
    }
    fn set_param(&mut self, _: u32, _: f32) {}
    fn activate(&mut self, _: f32) {}
    fn process(&mut self, ctx: ProcessContext<'_>) {
        ctx.out_l.fill(0.0);
        ctx.out_r.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyra_plugin::{Event, TimedEvent};

    /// Render `seconds` of audio in 256-frame blocks; `events` go in the first block.
    fn render(p: &mut dyn Plugin, events: &[TimedEvent], seconds: f32) -> Vec<f32> {
        let sr = 48_000.0;
        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        let mut out = Vec::new();
        let mut first = true;
        while (out.len() as f32) < seconds * sr {
            let ev = if first { events } else { &[] };
            first = false;
            p.process(ProcessContext { sample_rate: sr, events: ev, out_l: &mut l, out_r: &mut r });
            out.extend(l.iter().zip(&r).map(|(a, b)| a.abs().max(b.abs())));
        }
        out
    }

    fn on(key: u8) -> TimedEvent {
        TimedEvent { offset: 0, event: Event::NoteOn { key, velocity: 0.9 } }
    }
    fn off(key: u8) -> TimedEvent {
        TimedEvent { offset: 0, event: Event::NoteOff { key } }
    }

    #[test]
    fn every_preset_plays_a_chord_and_falls_silent() {
        for preset in PRESETS {
            let mut synth = create(preset.instrument).unwrap();
            for (i, v) in preset.params().into_iter().enumerate() {
                synth.set_param(i as u32, v);
            }
            synth.activate(48_000.0);
            let chord = [57, 60, 64, 67];
            let held = render(synth.as_mut(), &chord.map(on), 0.6);
            let tail = render(synth.as_mut(), &chord.map(off), 4.0);
            let peak = held.iter().chain(&tail).fold(0.0f32, |m, x| m.max(*x));
            assert!(held.iter().chain(&tail).all(|x| x.is_finite()), "{}: NaN/inf", preset.name);
            assert!(peak > 0.02 && peak < 1.6, "{}: peak {peak}", preset.name);
            let end = tail[tail.len() - 4800..].iter().fold(0.0f32, |m, x| m.max(*x));
            assert!(end < 1e-3, "{}: still sounding after release ({end})", preset.name);
            println!("{:13} peak {peak:.2}", preset.name);
        }
    }

    #[test]
    fn worst_case_cost() {
        let pad = PRESETS.iter().find(|p| p.name == "Supersaw Pad").unwrap();
        let mut synth = create(pad.instrument).unwrap();
        for (i, v) in pad.params().into_iter().enumerate() {
            synth.set_param(i as u32, v);
        }
        synth.activate(48_000.0);
        let keys: Vec<TimedEvent> = (48..64).map(on).collect(); // 16 notes × 7 unison = 112 oscillators
        let t = std::time::Instant::now();
        render(synth.as_mut(), &keys, 2.0);
        let cost = t.elapsed().as_secs_f64() / 2.0;
        println!("16-note supersaw: {:.1}% of one core (realtime = 100%)", cost * 100.0);
        assert!(cost < if cfg!(debug_assertions) { 2.0 } else { 0.25 });
    }
}
