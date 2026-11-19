//! Chrysalis (id `lyra.chrysalis`): a wavetable synth for modern sound
//! design: growls, talking basses, reeses, evolving pads, supersaw leads.
//!
//! ```text
//!  per voice:  osc 1, osc 2 (table × position, warp, unison ≤ 16) + sub + noise
//!              ─► drive (tanh / hard / fold / crush)
//!              ─► filter 1 ─► filter 2   (or side by side)
//!              ─► amp envelope × velocity × volume/pan mods ─► L/R
//!  global:     L/R × volume ─► DC blocker ─► soft clip above −2 dB ─► out
//! ```
//! * **Tables** ([`table`]): up to 256 frames of 2048 samples, each kept at
//!   20 band-limited levels half an octave apart; the oscillator picks the
//!   fullest level whose top harmonic stays below Nyquist. Position morphs
//!   linearly between neighbouring frames. Built-ins are generated once (on
//!   the UI thread); an imported WAV arrives as the channel's sample.
//! * **Warp** bends the phase before the lookup: bend ±, sync, mirror,
//!   quantize, FM from the other oscillator, PWM.
//! * **Unison**: up to 16 voices, detuned evenly (±1 semitone at full), the
//!   side voices at *blend*, spread across the stereo field alternately.
//! * **Modulation** ([`params`]): 4 drawn LFOs ([`curve`]), 4 envelopes (the
//!   first is the amp), 16 matrix slots (source × amount × optional "via"
//!   source → destination), 4 macros. Runs at a control rate of 32 samples;
//!   position, levels and the output gain ramp per sample in between.
//! * 16 voices (stolen like Fluttersine's), or mono with legato and glide.

pub mod curve;
mod engine;
pub mod params;
mod presets;
pub mod table;
pub mod telemetry;

pub use engine::{drive_consts, drive_shape, warp_consts, warp_preview, Chrysalis, CR, MAX_UNISON, MAX_VOICES};
pub use params::{p, DESTS, GROUPS, PARAMS, SOURCES};
pub use presets::PRESETS;

use lyra_plugin::PluginInfo;
use std::sync::LazyLock;

/// Chrysalis, as Lyra lists it.
pub static PLUGINS: LazyLock<[PluginInfo; 1]> = LazyLock::new(|| {
    [PluginInfo {
        id: "lyra.chrysalis",
        name: "Chrysalis",
        melodic: true,
        params: &PARAMS,
        groups: &GROUPS,
        presets: &PRESETS,
        create: || Box::new(Chrysalis::new()),
    }]
});

/// Build the built-in tables on a background thread now, so the first
/// Chrysalis opens instantly. (Anyone who needs them sooner just waits.)
pub fn warm_up() {
    std::thread::spawn(|| {
        table::builtin();
    });
}

#[cfg(test)]
mod tests;
