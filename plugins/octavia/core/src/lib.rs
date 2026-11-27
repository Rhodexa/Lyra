//! Octavia: an expressive string player and the instrument it plays.
//!
//! The pipeline has two halves joined by one plain data format:
//!
//! ```text
//!   Score ──[performer]──▶ Curves ──[instrument]──▶ audio
//!   (notes,                (pitch, loudness,         (voice + plucks
//!    articulations)         brightness, noise,        → sympathetic strings
//!                           string @ 250 Hz, plucks,  → body)
//!                           finger taps)
//! ```
//!
//! [`Curves`] is the contract: it can be saved, read back, plotted and
//! edited by hand, so each half can be tested without the other.
//!
//! The core has no dependencies and knows nothing about plugin formats.

pub mod body;
pub mod curves;
mod dsp;
pub mod instrument;
pub mod performer;
pub mod pluck;
pub mod rand;
pub mod render;
pub mod ring;
pub mod score;
pub mod sympathetic;
pub mod telemetry;
pub mod voice;

pub use curves::{Curves, Frame, Pluck, Ring, Tap, FRAME_RATE};
pub use instrument::Instrument;
pub use performer::{perform, Decision, Event, Performer, Played, Skill, Style, LOOKAHEAD, NO_ID};
pub use render::{render, render_score, Engine, Rendered, Sound};
pub use score::{Articulation, Note, Score};
pub use telemetry::Snapshot;

/// MIDI note number (fractional allowed) to Hz. A4 = 69 = 440 Hz.
#[inline]
pub fn midi_to_hz(pitch: f32) -> f32 {
    440.0 * ((pitch - 69.0) / 12.0).exp2()
}
