//! Lyra's side of the plugin kit (`lyra-kit`): the list of hand-made editors,
//! and the [`Host`] each one edits its channel through.
//!
//! An editor lives in its plugin's own crate and knows nothing of Lyra but
//! the kit. Adding one is one line in [`EDITORS`] (and its crate in
//! Cargo.toml).

use lyra_kit::{Editor, Host, PresetEntry, Telemetry};
use lyra_model::{History, Project};
use lyra_plugin::SampleData;
use std::path::{Path, PathBuf};

/// Every plugin with its own editor. The rest get the generic one (editor.rs).
pub const EDITORS: &[Editor] =
    &[lyra_fluttersine_editor::EDITOR, lyra_chrysalis_editor::EDITOR, octavia_editor::EDITOR];

/// The hand-made editor for `plugin`, if it has one.
pub fn editor_for(plugin: &str) -> Option<&'static Editor> {
    EDITORS.iter().find(|e| e.plugin == plugin)
}

/// Every editor's own font families, added before Lyra's (theme::install_fonts).
pub fn add_editor_fonts(defs: &mut egui::FontDefinitions) {
    for e in EDITORS {
        (e.fonts)(defs);
    }
}

/// One channel of the project, as its editor sees it.
pub struct ChannelHost<'a> {
    pub project: &'a mut Project,
    pub history: &'a mut History,
    pub channel: usize,
    pub live: Option<Telemetry>,
}

impl ChannelHost<'_> {
    fn plugin(&self) -> &str {
        &self.project.channels[self.channel].instrument
    }
}

impl Host for ChannelHost<'_> {
    fn instance(&self) -> usize {
        self.channel
    }
    fn param(&self, id: usize) -> f32 {
        crate::automation::shown_param(self.channel, id).unwrap_or(self.project.channels[self.channel].params[id])
    }
    fn params(&self) -> &[f32] {
        &self.project.channels[self.channel].params
    }
    fn begin_edit(&mut self) {
        self.history.checkpoint(self.project);
    }
    fn set_param(&mut self, id: usize, value: f32) {
        self.project.channels[self.channel].params[id] = value;
    }
    fn set_params(&mut self, params: Vec<f32>) {
        self.project.channels[self.channel].params = params;
    }
    fn telemetry(&self) -> Option<Telemetry> {
        self.live.clone()
    }
    fn channel_name(&self) -> &str {
        &self.project.channels[self.channel].name
    }
    fn tempo(&self) -> f64 {
        self.project.bpm
    }
    fn presets(&self) -> Vec<PresetEntry> {
        crate::user_presets::entries(self.plugin())
    }
    fn save_preset(&mut self, name: &str, params: &[f32]) -> Result<(), String> {
        crate::user_presets::save(self.plugin(), name, params)
    }
    fn delete_preset(&mut self, name: &str) {
        crate::user_presets::delete(self.plugin(), name)
    }
    fn sample(&self) -> Option<&str> {
        self.project.channels[self.channel].sample.as_deref()
    }
    fn set_sample(&mut self, name: Option<String>) {
        self.project.channels[self.channel].sample = name;
    }
    fn sample_name_for(&self, path: &Path) -> String {
        crate::samples::name_for(path)
    }
    fn sample_label(&self, name: &str) -> String {
        crate::samples::display_name(name)
    }
    fn load_sample(&self, name: &str) -> Result<SampleData, String> {
        crate::samples::decode(&crate::samples::path_for(name))
    }
    fn sample_changed(&mut self, name: &str) {
        crate::samples::reload(name)
    }
    fn data_dir(&self) -> PathBuf {
        crate::files::data_dir()
    }
}
