//! # lyra-dsp
//!
//! Lyra's built-in instruments (and, from milestone 3, effects).
//! Everything here implements [`lyra_plugin::Plugin`] — the same interface a
//! hosted CLAP plugin will get — so the engine never cares where sound comes from.

pub mod drums;
pub mod util;

use lyra_plugin::{ParamInfo, Plugin, ProcessContext};

/// A built-in instrument the user can add from the browser / "+" menu.
pub struct InstrumentInfo {
    /// Stable id saved in project files. Never rename one.
    pub id: &'static str,
    pub name: &'static str,
}

pub const INSTRUMENTS: &[InstrumentInfo] = &[
    InstrumentInfo { id: "lyra.kick", name: "Kick" },
    InstrumentInfo { id: "lyra.snare", name: "Snare" },
    InstrumentInfo { id: "lyra.clap", name: "Clap" },
    InstrumentInfo { id: "lyra.hat", name: "Hi-hat" },
];

/// Build an instrument by id. Runs on the UI thread (it allocates).
pub fn create(id: &str) -> Option<Box<dyn Plugin>> {
    Some(match id {
        "lyra.kick" => Box::new(drums::Kick::new()),
        "lyra.snare" => Box::new(drums::Snare::new()),
        "lyra.clap" => Box::new(drums::Clap::new()),
        "lyra.hat" => Box::new(drums::Hat::new()),
        _ => return None,
    })
}

/// Like [`create`], but an unknown id (e.g. a project from a newer Lyra)
/// becomes a silent placeholder instead of breaking the channel list.
pub fn create_or_silence(id: &str) -> Box<dyn Plugin> {
    create(id).unwrap_or_else(|| Box::new(Silence))
}

/// Default parameter values for an instrument id (empty if unknown).
pub fn default_params(id: &str) -> Vec<f32> {
    create(id).map(|p| lyra_plugin::default_params(p.params())).unwrap_or_default()
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
