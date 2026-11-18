//! # lyra-catalog
//!
//! Which plugins Lyra knows. Every built-in lives in its own crate under
//! `crates/plugins` and describes itself there (a `PLUGINS` list of
//! [`PluginInfo`]: id, name, parameters, presets, how to make one); this crate
//! only gathers those lists and looks things up by id. The engine never sees
//! any of it: it plays whatever `Box<dyn Plugin>` it is handed.
//!
//! Hosted CLAP plugins will join the same lists later.

pub use lyra_plugin::PluginInfo;
use lyra_plugin::{ParamGroup, ParamInfo, Plugin, ProcessContext};
use std::sync::OnceLock;

/// Every built-in instrument, in menu order.
pub fn instruments() -> &'static [&'static PluginInfo] {
    static ALL: OnceLock<Vec<&'static PluginInfo>> = OnceLock::new();
    ALL.get_or_init(|| {
        lyra_drums::PLUGINS
            .iter()
            .chain(lyra_synth::PLUGINS)
            .chain(lyra_fluttersine::PLUGINS)
            .chain(lyra_sampler::PLUGINS)
            .collect()
    })
}

pub fn info(id: &str) -> Option<&'static PluginInfo> {
    instruments().iter().copied().find(|i| i.id == id)
}

/// Build an instrument by id. Runs on the UI thread (it allocates).
pub fn create(id: &str) -> Option<Box<dyn Plugin>> {
    info(id).map(|i| (i.create)())
}

/// Like [`create`], but an unknown id (e.g. a project from a newer Lyra)
/// becomes a silent placeholder instead of breaking the channel list.
pub fn create_or_silence(id: &str) -> Box<dyn Plugin> {
    create(id).unwrap_or_else(|| Box::new(Silence))
}

/// Parameter descriptions for an instrument id (empty if unknown).
pub fn params(id: &str) -> &'static [ParamInfo] {
    info(id).map_or(&[], |i| i.params)
}

/// Editor layout groups for an instrument id (empty = one flat group).
pub fn groups(id: &str) -> &'static [ParamGroup] {
    info(id).map_or(&[], |i| i.groups)
}

/// Default parameter values for an instrument id (empty if unknown).
pub fn default_params(id: &str) -> Vec<f32> {
    lyra_plugin::default_params(params(id))
}

/// A preset of a known instrument, ready to load.
#[derive(Clone, Copy)]
pub struct Preset {
    pub name: &'static str,
    pub instrument: &'static str,
    pub values: &'static [(usize, f32)],
}

impl Preset {
    /// The instrument's defaults with this preset's values on top.
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

/// One instrument's presets.
pub fn presets_for(instrument: &str) -> Vec<Preset> {
    info(instrument).map_or_else(Vec::new, |i| {
        i.presets.iter().map(|&(name, values)| Preset { name, instrument: i.id, values }).collect()
    })
}

/// Every instrument's presets.
pub fn presets() -> impl Iterator<Item = Preset> {
    instruments().iter().flat_map(|i| presets_for(i.id))
}

/// The mixer effects.
pub mod effects {
    use super::*;

    pub fn all() -> &'static [PluginInfo] {
        lyra_effects::PLUGINS
    }

    pub fn info(id: &str) -> Option<&'static PluginInfo> {
        all().iter().find(|e| e.id == id)
    }

    /// Build an effect by id. UI thread (it allocates).
    pub fn create(id: &str) -> Option<Box<dyn Plugin>> {
        info(id).map(|e| (e.create)())
    }

    pub fn params(id: &str) -> &'static [ParamInfo] {
        info(id).map_or(&[], |e| e.params)
    }

    pub fn default_params(id: &str) -> Vec<f32> {
        lyra_plugin::default_params(params(id))
    }
}

/// Stands in for a plugin this Lyra doesn't have.
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

    /// Each entry names its plugin's tables itself: they must be the ones
    /// the plugin really uses. And ids are unique.
    #[test]
    fn entries_match_their_plugins() {
        let all: Vec<&PluginInfo> = instruments().iter().copied().chain(effects::all()).collect();
        for e in &all {
            let p = (e.create)();
            assert!(std::ptr::eq(e.params, p.params()), "{}: params", e.id);
            assert!(std::ptr::eq(e.groups, p.groups()) || p.groups().is_empty(), "{}: groups", e.id);
            assert_eq!(all.iter().filter(|o| o.id == e.id).count(), 1, "{} listed twice", e.id);
        }
        assert!(!groups("lyra.fm").is_empty() && groups("lyra.kick").is_empty());
        assert!(presets_for("lyra.synth").len() > 3 && presets_for("lyra.fm").len() > 3);
    }
}
