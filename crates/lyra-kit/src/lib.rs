//! The plugin kit: what a plugin's editor needs from Lyra, and nothing more.
//!
//! An editor is a function that draws with egui into the `Ui` Lyra gives it
//! and talks to Lyra only through a [`Host`]: read and change parameters
//! (one undo step per gesture), read the plugin's telemetry, list and save
//! presets, use the channel's sample slot. It registers with one [`Editor`]
//! line, so it can live in its plugin's own crate and folder.
//!
//! What the kit must never contain: any plugin's colours, drawings, logos or
//! layout, nor anything from Lyra's own theme. A plugin owns its look
//! completely; Lyra's redesigns can't reach it. The kit offers *behaviour*
//! only (how a knob turns, how the wheel steps, how a rectangle is split),
//! so hands learn it once while each plugin paints its own.
//!
//! The shape follows CLAP where CLAP has a word for it (parameter gestures,
//! the plugin GUI reading its own DSP), so a CLAP build stays possible.

pub mod assets;
pub mod flex;
pub mod input;
pub mod knob;
pub mod presets;

pub use input::{double_click, wheel_step};
pub use knob::{knob_painted, knob_turn};
pub use presets::PresetEntry;

use egui::{FontDefinitions, Response, Ui};
use lyra_plugin::SampleData;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What the plugin's sound shares with its editor: its telemetry object
/// (atomics the audio thread writes). Downcast it to the plugin's own type.
pub type Telemetry = Arc<dyn std::any::Any + Send + Sync>;

/// Lyra, as one editor sees it: the channel it edits and the services around it.
///
/// Parameters change in **gestures**, like CLAP's begin/end: call
/// [`begin_edit`](Host::begin_edit) once when a drag or click starts (that's
/// one undo step), then [`set_param`](Host::set_param) as often as needed.
/// [`tracked`] does this for knob-like widgets.
pub trait Host {
    /// Tells this editor apart from another of the same plugin (for egui ids).
    fn instance(&self) -> usize;

    /// A parameter as the sound has it right now: where automation puts it
    /// while the song plays, else its own value.
    fn param(&self, id: usize) -> f32;
    /// The stored parameters (not automated): for matching presets, saving.
    fn params(&self) -> &[f32];
    /// A gesture starts: everything changed until the next one undoes together.
    fn begin_edit(&mut self);
    /// Change a parameter within the gesture. The editor clamps.
    fn set_param(&mut self, id: usize, value: f32);
    /// Replace every parameter (loading a preset), within the gesture.
    fn set_params(&mut self, params: Vec<f32>);

    /// The plugin's telemetry, once the engine runs it.
    fn telemetry(&self) -> Option<Telemetry>;
    /// The channel's name ("Bass").
    fn channel_name(&self) -> &str;
    /// The song's tempo in beats per minute.
    fn tempo(&self) -> f64;

    /// The plugin's presets: its own first, then the user's.
    fn presets(&self) -> Vec<PresetEntry>;
    /// Save the user's preset `name` (replacing one of the same name).
    fn save_preset(&mut self, name: &str, params: &[f32]) -> Result<(), String>;
    /// Delete the user's preset `name`.
    fn delete_preset(&mut self, name: &str);

    /// The channel's sample (Lyra's name for it), if it has one.
    fn sample(&self) -> Option<&str>;
    /// Give the channel a sample, within the gesture.
    fn set_sample(&mut self, name: Option<String>);
    /// Lyra's name for the audio file at `path`.
    fn sample_name_for(&self, path: &Path) -> String;
    /// A sample's name for people (the file name, no folders).
    fn sample_label(&self, name: &str) -> String;
    /// Decode a sample (for pictures; the engine loads its own copy).
    fn load_sample(&self, name: &str) -> Result<SampleData, String>;
    /// The file behind `name` was rewritten: the engine reads it again.
    fn sample_changed(&mut self, name: &str);
    /// Lyra's data folder (user tables, presets…), for files the plugin makes.
    fn data_dir(&self) -> PathBuf;
}

/// One undo step per drag: begins the gesture when a knob-like widget starts
/// moving (or changes in one go, like a click), and says whether it changed.
/// Set the parameter when it returns true.
pub fn tracked(r: &Response, host: &mut dyn Host) -> bool {
    if r.drag_started() || (r.changed() && !r.dragged()) {
        host.begin_edit();
    }
    r.changed()
}

/// A plugin's editor, registered with Lyra in one line.
#[derive(Clone, Copy)]
pub struct Editor {
    /// The plugin id it edits (`lyra.chrysalis`).
    pub plugin: &'static str,
    /// Draw the editor into `ui` (the room Lyra gives it, at Lyra's scale).
    /// Ask for repaints (`ui.ctx().request_repaint()`) while something moves.
    pub show: fn(&mut Ui, &mut dyn Host),
    /// Add the plugin's own font families, once at startup, before Lyra adds
    /// its own (so Lyra's fonts can't replace them).
    pub fonts: fn(&mut FontDefinitions),
}
