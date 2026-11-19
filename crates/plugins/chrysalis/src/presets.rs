//! Chrysalis's factory patches. Built once at startup by a small builder, so
//! a drawn LFO shape or a matrix route reads as one line.

use crate::curve::SHAPES;
use crate::params::{ft, p, DESTS, SOURCES};
use crate::table::TABLE_NAMES;
use lyra_dsp::SYNC_LABELS;
use lyra_plugin::Preset;
use std::sync::LazyLock;

/// (name, parameters that differ from the defaults).
pub static PRESETS: LazyLock<Vec<Preset>> = LazyLock::new(|| {
    vec![
        ("Init", &[][..]),
        ("Growl", growl()),
        ("Talking bass", talking_bass()),
        ("Reese", reese()),
        ("Pluck", pluck()),
        ("Supersaw lead", supersaw_lead()),
        ("Evolving pad", evolving_pad()),
    ]
});

fn index(list: &[&str], name: &str) -> f32 {
    list.iter().position(|x| *x == name).unwrap_or_else(|| panic!("no \"{name}\" in {list:?}")) as f32
}

/// A patch under construction: parameter changes, in order.
struct B {
    v: Vec<(usize, f32)>,
    slots: usize,
}

impl B {
    fn new() -> Self {
        B { v: Vec::new(), slots: 0 }
    }
    fn set(mut self, id: usize, x: f32) -> Self {
        self.v.push((id, x));
        self
    }
    fn table(self, o: usize, name: &str) -> Self {
        self.set(p::osc(o, p::TABLE), index(&TABLE_NAMES, name))
    }
    /// Octave as an offset (−3..3).
    fn octave(self, o: usize, oct: i32) -> Self {
        self.set(p::osc(o, p::OCTAVE), (oct + 3) as f32)
    }
    fn unison(self, o: usize, voices: usize, detune: f32, spread: f32) -> Self {
        self.set(p::osc(o, p::UNISON), (voices - 1) as f32)
            .set(p::osc(o, p::DETUNE), detune)
            .set(p::osc(o, p::SPREAD), spread)
    }
    fn filter(self, f: usize, kind: u8, cutoff: f32, reso: f32) -> Self {
        self.set(p::filter(f, p::F_ON), 1.0)
            .set(p::filter(f, p::F_TYPE), kind as f32)
            .set(p::filter(f, p::F_CUTOFF), cutoff)
            .set(p::filter(f, p::F_RESO), reso)
    }
    /// Envelope `e`: attack, decay, sustain, release.
    fn env(self, e: usize, a: f32, d: f32, s: f32, r: f32) -> Self {
        self.set(p::env(e, p::E_ATTACK), a)
            .set(p::env(e, p::E_DECAY), d)
            .set(p::env(e, p::E_SUSTAIN), s)
            .set(p::env(e, p::E_RELEASE), r)
    }
    /// LFO `l`: synced to `sync` (a `SYNC_LABELS` label; "Free" uses `hz`).
    fn lfo(self, l: usize, sync: &str, hz: f32) -> Self {
        self.set(p::lfo(l, p::L_SYNC), index(&SYNC_LABELS, sync)).set(p::lfo(l, p::L_RATE), hz)
    }
    fn lfo_mode(self, l: usize, mode: f32) -> Self {
        self.set(p::lfo(l, p::L_MODE), mode)
    }
    /// LFO `l`'s shape, as points (x, y, bend).
    fn shape(mut self, l: usize, pts: &[(f32, f32, f32)]) -> Self {
        self.v.push((p::lfo(l, p::L_POINTS), pts.len() as f32));
        for (k, &(x, y, b)) in pts.iter().enumerate() {
            self.v.push((p::point_x(l, k), x));
            self.v.push((p::point_y(l, k), y));
            self.v.push((p::point_bend(l, k), b));
        }
        self
    }
    fn preset_shape(self, l: usize, name: &str) -> Self {
        let pts = SHAPES.iter().find(|s| s.0 == name).unwrap_or_else(|| panic!("no shape {name}")).1;
        self.shape(l, pts)
    }
    /// The next free matrix slot: `source` → `dest` × `amount`.
    fn route(self, source: &str, dest: &str, amount: f32) -> Self {
        self.route_via(source, dest, amount, "Off")
    }
    fn route_via(mut self, source: &str, dest: &str, amount: f32, via: &str) -> Self {
        let k = self.slots;
        self.slots += 1;
        self.set(p::slot(k, p::S_SRC), index(&SOURCES, source))
            .set(p::slot(k, p::S_DEST), index(&DESTS, dest))
            .set(p::slot(k, p::S_AMT), amount)
            .set(p::slot(k, p::S_VIA), index(&SOURCES, via))
    }
    fn mono(self, glide: f32) -> Self {
        self.set(p::MODE, 1.0).set(p::GLIDE, glide)
    }
    fn done(self) -> &'static [(usize, f32)] {
        Box::leak(self.v.into_boxed_slice())
    }
}

/// Basic table: sine 0, triangle ⅓, saw ⅔, square 1.
const SAW: f32 = 2.0 / 3.0;

/// A growl that changes vowel on the 8ths: the growl table and a formant
/// filter moved by one synced LFO, through tanh. Macro 1 = grit.
fn growl() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "Growl")
        .set(p::osc(0, p::POS), 0.2)
        .unison(0, 4, 0.15, 0.5)
        .set(p::osc(0, p::BLEND), 0.6)
        .set(p::osc(0, p::RAND), 0.2)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), SAW)
        .octave(1, -1)
        .unison(1, 1, 0.0, 0.0)
        .set(p::osc(1, p::LEVEL), 0.4)
        .set(p::SUB_ON, 1.0)
        .set(p::SUB_LEVEL, 0.6)
        .set(p::DIST_MODE, 1.0)
        .set(p::DIST_DRIVE, 0.45)
        .filter(0, ft::FORMANT, 1000.0, 0.4)
        .set(p::filter(0, p::F_VOWEL), 0.3)
        .set(p::filter(0, p::F_MIX), 0.7)
        .filter(1, ft::LOW24, 5000.0, 0.15)
        .env(0, 0.003, 0.5, 1.0, 0.1)
        .lfo(0, "1/8", 2.0)
        .shape(0, &[(0.0, 0.0, 0.5), (0.5, 1.0, -0.4)])
        .route("LFO 1", "Osc 1 position", 0.7)
        .route("LFO 1", "Filter 1 vowel", 0.6)
        .route("LFO 1", "Filter 2 cutoff", 0.25)
        .route("Macro 1", "Drive", 0.5)
        .route("Macro 1", "Filter 1 mix", 0.3)
        .set(p::VOLUME, 0.63)
        .mono(0.04)
        .done()
}

/// "Wah-yo-wa-oo" over a bar, restarted by every note: vowels table and
/// formant filter on one drawn LFO.
fn talking_bass() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "Vowels")
        .set(p::osc(0, p::POS), 0.0)
        .unison(0, 2, 0.08, 0.3)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), 1.0)
        .octave(1, -1)
        .unison(1, 1, 0.0, 0.0)
        .set(p::osc(1, p::LEVEL), 0.35)
        .set(p::SUB_ON, 1.0)
        .set(p::SUB_LEVEL, 0.5)
        .set(p::DIST_MODE, 3.0)
        .set(p::DIST_DRIVE, 0.25)
        .set(p::DIST_MIX, 0.6)
        .filter(0, ft::FORMANT, 1000.0, 0.55)
        .set(p::filter(0, p::F_MIX), 0.8)
        .filter(1, ft::LOW12, 3500.0, 0.1)
        .env(0, 0.003, 0.5, 1.0, 0.12)
        .lfo(0, "1 bar", 1.0)
        .lfo_mode(0, 1.0)
        .shape(
            0,
            &[(0.0, 0.1, 0.4), (0.2, 0.9, -0.3), (0.35, 0.4, 0.0), (0.5, 0.8, 0.5), (0.7, 0.2, 0.0), (0.85, 1.0, -0.5)],
        )
        .route("LFO 1", "Osc 1 position", 0.9)
        .route("LFO 1", "Filter 1 vowel", 1.0)
        .set(p::VOLUME, 0.7)
        .mono(0.06)
        .done()
}

/// Two detuned saw pairs beating against each other, a slow drift on the
/// cutoff and the detune.
fn reese() -> &'static [(usize, f32)] {
    B::new()
        .unison(0, 2, 0.35, 0.4)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), SAW)
        .unison(1, 2, 0.42, 0.4)
        .set(p::osc(1, p::FINE), 7.0)
        .set(p::osc(1, p::LEVEL), 0.6)
        .set(p::SUB_ON, 1.0)
        .set(p::SUB_LEVEL, 0.55)
        .set(p::DIST_MODE, 1.0)
        .set(p::DIST_DRIVE, 0.25)
        .set(p::DIST_MIX, 0.6)
        .filter(0, ft::LOW24, 900.0, 0.25)
        .set(p::filter(0, p::F_DRIVE), 0.3)
        .set(p::filter(0, p::F_KEY), 0.3)
        .env(0, 0.005, 0.6, 1.0, 0.15)
        .lfo(0, "Free", 0.25)
        .preset_shape(0, "Sine")
        .route("LFO 1", "Filter 1 cutoff", 0.15)
        .route("LFO 1", "Osc 2 detune", 0.15)
        .set(p::VOLUME, 0.21)
        .mono(0.05)
        .done()
}

/// A bright pluck: a saw-to-square with a pulse an octave up, and a filter
/// envelope that snaps shut.
fn pluck() -> &'static [(usize, f32)] {
    B::new()
        .set(p::osc(0, p::POS), 0.8)
        .unison(0, 3, 0.12, 0.8)
        .set(p::osc(0, p::RAND), 0.3)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .table(1, "Pulse")
        .set(p::osc(1, p::POS), 0.4)
        .octave(1, 1)
        .set(p::osc(1, p::LEVEL), 0.35)
        .filter(0, ft::LOW24, 400.0, 0.3)
        .set(p::filter(0, p::F_KEY), 0.5)
        .env(0, 0.001, 0.45, 0.0, 0.3)
        .env(1, 0.001, 0.25, 0.0, 0.2)
        .set(p::env(1, p::E_D_CURVE), 0.6)
        .route("Env 2", "Filter 1 cutoff", 0.55)
        .done()
}

/// Nine saws plus five an octave up, wide open; a vibrato that fades in
/// (an LFO on pitch, *via* a slow envelope).
fn supersaw_lead() -> &'static [(usize, f32)] {
    B::new()
        .unison(0, 9, 0.5, 1.0)
        .set(p::osc(0, p::BLEND), 0.8)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), SAW)
        .octave(1, 1)
        .unison(1, 5, 0.4, 0.8)
        .set(p::osc(1, p::LEVEL), 0.35)
        .filter(0, ft::LOW12, 9000.0, 0.1)
        .env(0, 0.008, 0.4, 0.85, 0.35)
        .env(1, 0.6, 1.0, 1.0, 0.3)
        .set(p::env(1, p::E_A_CURVE), -0.4)
        .lfo(0, "Free", 5.5)
        .preset_shape(0, "Sine")
        .route_via("LFO 1", "Pitch", 0.006, "Env 2")
        .set(p::slot(0, p::S_BIPOLAR), 1.0)
        .set(p::VOLUME, 0.45)
        .mono(0.04)
        .done()
}

/// Two tables drifting at three unrelated slow rates; it never repeats.
fn evolving_pad() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "Sweep")
        .set(p::osc(0, p::POS), 0.3)
        .unison(0, 6, 0.25, 1.0)
        .set(p::osc(0, p::LEVEL), 0.6)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .table(1, "Vowels")
        .set(p::osc(1, p::POS), 0.5)
        .unison(1, 4, 0.2, 1.0)
        .set(p::osc(1, p::FINE), -5.0)
        .set(p::osc(1, p::LEVEL), 0.45)
        .set(p::NOISE_ON, 1.0)
        .set(p::NOISE_COLOR, -0.4)
        .set(p::NOISE_LEVEL, 0.08)
        .filter(0, ft::LOW12, 4000.0, 0.15)
        .env(0, 1.8, 1.0, 1.0, 2.5)
        .set(p::env(0, p::E_A_CURVE), 0.3)
        .lfo(0, "Free", 0.06)
        .preset_shape(0, "Sine")
        .lfo(1, "Free", 0.09)
        .preset_shape(1, "Triangle")
        .lfo(2, "Free", 0.13)
        .preset_shape(2, "Sine")
        .route("LFO 1", "Osc 1 position", 0.5)
        .route("LFO 2", "Osc 2 position", 0.6)
        .route("LFO 3", "Filter 1 cutoff", 0.15)
        .done()
}
