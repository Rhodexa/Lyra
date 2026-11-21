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
        ("Bass · Growl", growl()),
        ("Bass · Monster growl", monster_growl()),
        ("Bass · Metal growl", metal_growl()),
        ("Bass · Yoi", yoi()),
        ("Bass · Talking", talking_bass()),
        ("Bass · Wub", wub()),
        ("Bass · Glitch", glitch_bass()),
        ("Bass · Reese", reese()),
        ("Bass · Neuro reese", neuro_reese()),
        ("Lead · Supersaw", supersaw_lead()),
        ("Lead · Hero", hero_lead()),
        ("Lead · Chip", chip_lead()),
        ("Lead · Mega saw", mega_saw()),
        ("Pluck · Bright", pluck()),
        ("Pluck · Arp", arp_pluck()),
        ("Pluck · Glitch stab", glitch_stab()),
        ("Keys · Electric", electric_keys()),
        ("Keys · Synth piano", synth_piano()),
        ("Keys · Bells", bells()),
        ("Pad · Evolving", evolving_pad()),
        ("Pad · Warm", warm_pad()),
        ("Pad · Glass", glass_pad()),
        ("FX · Riser", riser()),
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

// ───────────────────────────── more basses ─────────────────────────────

/// A growl table phase-modulated by a sine (FM warp), folded, through a
/// formant filter: LFO 1 moves the table, the vowel and the FM together on
/// the 8ths; a random LFO jumps the second filter. Macro 1 = grit, 2 = FM.
fn monster_growl() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "Growl")
        .set(p::osc(0, p::POS), 0.3)
        .unison(0, 3, 0.1, 0.4)
        .set(p::osc(0, p::RAND), 0.1)
        .set(p::osc(0, p::WARP_MODE), 6.0)
        .set(p::osc(0, p::WARP), 0.3)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), 0.0)
        .unison(1, 1, 0.0, 0.0)
        .set(p::osc(1, p::LEVEL), 0.0)
        .set(p::SUB_ON, 1.0)
        .set(p::SUB_LEVEL, 0.6)
        .set(p::DIST_MODE, 3.0)
        .set(p::DIST_DRIVE, 0.5)
        .set(p::DIST_MIX, 0.8)
        .filter(0, ft::FORMANT, 1000.0, 0.5)
        .set(p::filter(0, p::F_MIX), 0.8)
        .filter(1, ft::LOW24, 4500.0, 0.2)
        .env(0, 0.002, 0.5, 1.0, 0.08)
        .lfo(0, "1/8", 2.0)
        .shape(0, &[(0.0, 0.0, 0.6), (0.45, 1.0, -0.4), (0.7, 0.35, 0.0)])
        .lfo(1, "1/16", 4.0)
        .set(p::lfo_style(1), 1.0)
        .route("LFO 1", "Osc 1 position", 0.6)
        .route("LFO 1", "Filter 1 vowel", 0.8)
        .route("LFO 1", "Osc 1 warp", 0.4)
        .route("LFO 2", "Filter 2 cutoff", 0.25)
        .route("Macro 1", "Drive", 0.45)
        .route("Macro 2", "Osc 1 warp", 0.5)
        .set(p::VOLUME, 0.79)
        .mono(0.03)
        .done()
}

/// Hard sync swept on the 8ths into a comb tuned to the note: a metallic
/// scream, clipped hard.
fn metal_growl() -> &'static [(usize, f32)] {
    B::new()
        .set(p::osc(0, p::POS), SAW)
        .unison(0, 2, 0.12, 0.3)
        .set(p::osc(0, p::WARP_MODE), 3.0)
        .set(p::osc(0, p::WARP), 0.2)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .table(1, "Digital")
        .set(p::osc(1, p::POS), 0.4)
        .set(p::osc(1, p::LEVEL), 0.4)
        .unison(1, 1, 0.0, 0.0)
        .set(p::SUB_ON, 1.0)
        .set(p::SUB_LEVEL, 0.5)
        .set(p::DIST_MODE, 2.0)
        .set(p::DIST_DRIVE, 0.4)
        .filter(0, ft::COMB_PLUS, 220.0, 0.6)
        .set(p::filter(0, p::F_KEY), 1.0)
        .set(p::filter(0, p::F_MIX), 0.5)
        .filter(1, ft::LOW24, 6000.0, 0.15)
        .env(0, 0.002, 0.5, 1.0, 0.08)
        .lfo(0, "1/8", 2.0)
        .shape(0, &[(0.0, 0.0, 0.7), (0.6, 1.0, -0.6)])
        .route("LFO 1", "Osc 1 warp", 0.7)
        .route("LFO 1", "Filter 2 cutoff", 0.2)
        .set(p::VOLUME, 0.2)
        .mono(0.03)
        .done()
}

/// "Yoi": a vowel sweep from ee to ah and back on every beat, restarted by
/// each note.
fn yoi() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "Vowels")
        .set(p::osc(0, p::POS), 0.5)
        .unison(0, 2, 0.1, 0.3)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), SAW)
        .octave(1, -1)
        .unison(1, 1, 0.0, 0.0)
        .set(p::osc(1, p::LEVEL), 0.4)
        .set(p::SUB_ON, 1.0)
        .set(p::SUB_LEVEL, 0.55)
        .set(p::DIST_MODE, 1.0)
        .set(p::DIST_DRIVE, 0.4)
        .filter(0, ft::FORMANT, 1100.0, 0.55)
        .set(p::filter(0, p::F_MIX), 0.85)
        .env(0, 0.002, 0.5, 1.0, 0.1)
        .lfo(0, "1/4", 2.0)
        .lfo_mode(0, 1.0)
        .shape(0, &[(0.0, 0.05, 0.5), (0.55, 1.0, -0.4), (0.8, 0.6, 0.0)])
        .route("LFO 1", "Filter 1 vowel", -0.5)
        .route("LFO 1", "Osc 1 position", 0.5)
        .route("LFO 1", "Filter 1 cutoff", 0.2)
        .set(p::filter(0, p::F_VOWEL), 0.5)
        .set(p::VOLUME, 1.0)
        .mono(0.05)
        .done()
}

/// The classic wobble: a resonant low-pass opening on the beat. Macro 1
/// doubles the wobble's speed.
fn wub() -> &'static [(usize, f32)] {
    B::new()
        .unison(0, 2, 0.15, 0.3)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), 1.0)
        .octave(1, -1)
        .unison(1, 1, 0.0, 0.0)
        .set(p::osc(1, p::LEVEL), 0.6)
        .set(p::SUB_ON, 1.0)
        .set(p::SUB_LEVEL, 0.55)
        .set(p::DIST_MODE, 1.0)
        .set(p::DIST_DRIVE, 0.3)
        .filter(0, ft::LOW24, 300.0, 0.45)
        .set(p::filter(0, p::F_DRIVE), 0.3)
        .env(0, 0.002, 0.5, 1.0, 0.1)
        .lfo(0, "1/4", 2.0)
        .lfo_mode(0, 1.0)
        .preset_shape(0, "Sine")
        .route("LFO 1", "Filter 1 cutoff", 0.7)
        .route("Macro 1", "LFO 1 rate", 0.25)
        .set(p::VOLUME, 0.2)
        .mono(0.04)
        .done()
}

/// Glitch hop: a crushed digital table jumping to a random frame and a
/// random brightness on every 16th.
fn glitch_bass() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "Digital")
        .set(p::osc(0, p::POS), 0.3)
        .unison(0, 2, 0.08, 0.3)
        .set(p::SUB_ON, 1.0)
        .set(p::SUB_LEVEL, 0.5)
        .set(p::DIST_MODE, 4.0)
        .set(p::DIST_DRIVE, 0.35)
        .set(p::DIST_MIX, 0.5)
        .filter(0, ft::LOW24, 1800.0, 0.35)
        .env(0, 0.002, 0.25, 0.6, 0.08)
        .lfo(0, "1/16", 4.0)
        .set(p::lfo_style(0), 1.0)
        .route("LFO 1", "Osc 1 position", 0.8)
        .route("LFO 1", "Filter 1 cutoff", 0.35)
        .set(p::VOLUME, 0.34)
        .mono(0.02)
        .done()
}

/// A reese with teeth: one saw phase-modulates the other, a negative comb
/// hollows it out, and the whole thing breathes over two beats.
fn neuro_reese() -> &'static [(usize, f32)] {
    B::new()
        .unison(0, 2, 0.3, 0.4)
        .set(p::osc(0, p::WARP_MODE), 6.0)
        .set(p::osc(0, p::WARP), 0.2)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), SAW)
        .unison(1, 2, 0.36, 0.4)
        .set(p::osc(1, p::FINE), 9.0)
        .set(p::osc(1, p::LEVEL), 0.55)
        .set(p::SUB_ON, 1.0)
        .set(p::SUB_LEVEL, 0.5)
        .set(p::DIST_MODE, 1.0)
        .set(p::DIST_DRIVE, 0.35)
        .filter(0, ft::COMB_MINUS, 110.0, 0.5)
        .set(p::filter(0, p::F_KEY), 1.0)
        .set(p::filter(0, p::F_MIX), 0.4)
        .filter(1, ft::LOW24, 1500.0, 0.2)
        .env(0, 0.004, 0.5, 1.0, 0.15)
        .lfo(0, "1/2", 1.0)
        .preset_shape(0, "Sine")
        .route("LFO 1", "Osc 1 warp", 0.3)
        .route("LFO 1", "Filter 2 cutoff", 0.25)
        .set(p::VOLUME, 0.21)
        .mono(0.05)
        .done()
}

// ───────────────────────────── more leads ─────────────────────────────

/// Big, bright and sweet: seven saws with a pulse an octave up, a
/// vibrato that fades in, glide between held notes.
fn hero_lead() -> &'static [(usize, f32)] {
    B::new()
        .unison(0, 7, 0.32, 0.9)
        .set(p::osc(0, p::BLEND), 0.85)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .table(1, "Pulse")
        .set(p::osc(1, p::POS), 0.25)
        .octave(1, 1)
        .unison(1, 3, 0.15, 0.6)
        .set(p::osc(1, p::LEVEL), 0.35)
        .filter(0, ft::LOW12, 7000.0, 0.15)
        .env(0, 0.004, 0.6, 0.8, 0.35)
        .env(1, 0.5, 1.0, 1.0, 0.3)
        .lfo(0, "Free", 5.8)
        .preset_shape(0, "Sine")
        .route_via("LFO 1", "Pitch", 0.007, "Env 2")
        .set(p::slot(0, p::S_BIPOLAR), 1.0)
        .set(p::VOLUME, 0.42)
        .mono(0.06)
        .done()
}

/// A square lead from a game console: pulse width wobbling, a crisp octave
/// above, a little crush, vibrato that fades in.
fn chip_lead() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "Pulse")
        .set(p::osc(0, p::POS), 0.0)
        .unison(0, 1, 0.0, 0.0)
        .set(p::osc(0, p::RAND), 0.0)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .table(1, "Pulse")
        .set(p::osc(1, p::POS), 0.6)
        .octave(1, 1)
        .unison(1, 1, 0.0, 0.0)
        .set(p::osc(1, p::LEVEL), 0.25)
        .set(p::osc(1, p::PAN), 0.3)
        .set(p::DIST_MODE, 4.0)
        .set(p::DIST_DRIVE, 0.15)
        .set(p::DIST_MIX, 0.3)
        .env(0, 0.001, 0.2, 0.85, 0.12)
        .env(1, 0.35, 1.0, 1.0, 0.2)
        .lfo(0, "Free", 6.0)
        .preset_shape(0, "Triangle")
        .lfo(1, "Free", 6.5)
        .preset_shape(1, "Sine")
        .route("LFO 1", "Osc 1 position", 0.3)
        .route_via("LFO 2", "Pitch", 0.008, "Env 2")
        .set(p::slot(1, p::S_BIPOLAR), 1.0)
        .set(p::VOLUME, 0.32)
        .mono(0.035)
        .done()
}

/// Sixteen saws and sixteen more an octave up, wide as it gets.
fn mega_saw() -> &'static [(usize, f32)] {
    B::new()
        .unison(0, 16, 0.45, 1.0)
        .set(p::osc(0, p::BLEND), 0.9)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), SAW)
        .octave(1, 1)
        .unison(1, 16, 0.4, 1.0)
        .set(p::osc(1, p::LEVEL), 0.4)
        .filter(0, ft::HIGH12, 150.0, 0.1)
        .env(0, 0.01, 0.5, 1.0, 0.5)
        .set(p::VOLUME, 0.4)
        .done()
}

// ───────────────────────────── plucks, keys, pads ─────────────────────────────

/// Short and sparkly for arpeggios: five saws and an FM bell an octave up,
/// a filter that snaps shut.
fn arp_pluck() -> &'static [(usize, f32)] {
    B::new()
        .unison(0, 5, 0.2, 0.8)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .table(1, "FM")
        .set(p::osc(1, p::POS), 0.25)
        .octave(1, 1)
        .set(p::osc(1, p::LEVEL), 0.3)
        .filter(0, ft::LOW24, 600.0, 0.25)
        .set(p::filter(0, p::F_KEY), 0.5)
        .env(0, 0.001, 0.35, 0.0, 0.25)
        .env(1, 0.001, 0.18, 0.0, 0.2)
        .set(p::env(1, p::E_D_CURVE), 0.7)
        .route("Env 2", "Filter 1 cutoff", 0.6)
        .set(p::VOLUME, 0.8)
        .done()
}

/// A glitchy chord stab: pulse and FM, short, its colour re-rolled on every
/// 32nd while it rings.
fn glitch_stab() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "Pulse")
        .set(p::osc(0, p::POS), 0.6)
        .unison(0, 3, 0.15, 0.7)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .table(1, "FM")
        .set(p::osc(1, p::POS), 0.5)
        .octave(1, 1)
        .set(p::osc(1, p::LEVEL), 0.35)
        .set(p::DIST_MODE, 4.0)
        .set(p::DIST_DRIVE, 0.25)
        .set(p::DIST_MIX, 0.4)
        .filter(0, ft::LOW24, 1200.0, 0.3)
        .env(0, 0.001, 0.18, 0.0, 0.15)
        .env(1, 0.001, 0.12, 0.0, 0.1)
        .lfo(0, "1/32", 8.0)
        .lfo_mode(0, 1.0)
        .set(p::lfo_style(0), 1.0)
        .route("Env 2", "Filter 1 cutoff", 0.5)
        .route("LFO 1", "Osc 1 position", 0.5)
        .route("LFO 1", "Osc 2 position", 0.4)
        .set(p::VOLUME, 0.5)
        .done()
}

/// An electric piano: a soft FM tine over a sine, brighter the harder you
/// play, fading over a couple of seconds.
fn electric_keys() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "FM")
        .set(p::osc(0, p::POS), 0.12)
        .unison(0, 2, 0.05, 0.5)
        .set(p::osc(0, p::RAND), 0.0)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), 0.0)
        .unison(1, 1, 0.0, 0.0)
        .set(p::osc(1, p::RAND), 0.0)
        .set(p::osc(1, p::LEVEL), 0.6)
        .filter(0, ft::LOW12, 6000.0, 0.1)
        .set(p::filter(0, p::F_KEY), 0.6)
        .env(0, 0.002, 1.6, 0.25, 0.45)
        .set(p::env(0, p::E_D_CURVE), 0.7)
        .route("Velocity", "Osc 1 position", 0.35)
        .set(p::VOLUME, 0.5)
        .done()
}

/// A synth piano: a struck triangle-saw with an organ octave above, the
/// filter opening with how hard you play and closing as the note dies.
fn synth_piano() -> &'static [(usize, f32)] {
    B::new()
        .set(p::osc(0, p::POS), 0.45)
        .unison(0, 2, 0.04, 0.5)
        .set(p::osc(0, p::RAND), 0.0)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .table(1, "Organ")
        .set(p::osc(1, p::POS), 0.3)
        .octave(1, 1)
        .set(p::osc(1, p::LEVEL), 0.35)
        .set(p::osc(1, p::RAND), 0.0)
        .filter(0, ft::LOW12, 900.0, 0.1)
        .set(p::filter(0, p::F_KEY), 0.8)
        .env(0, 0.001, 2.5, 0.0, 0.5)
        .set(p::env(0, p::E_D_CURVE), 0.75)
        .env(1, 0.001, 0.8, 0.0, 0.4)
        .route_via("Env 2", "Filter 1 cutoff", 0.55, "Velocity")
        .set(p::VOLUME, 0.6)
        .done()
}

/// Bells: a bright FM frame with a sine two octaves up, ringing long.
fn bells() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "FM")
        .set(p::osc(0, p::POS), 0.6)
        .unison(0, 2, 0.04, 0.6)
        .set(p::osc(0, p::RAND), 0.0)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), 0.0)
        .octave(1, 2)
        .set(p::osc(1, p::LEVEL), 0.25)
        .set(p::osc(1, p::RAND), 0.0)
        .env(0, 0.001, 3.0, 0.0, 1.5)
        .set(p::env(0, p::E_D_CURVE), 0.7)
        .set(p::VOLUME, 0.45)
        .done()
}

/// A warm analog pad: seven saws and a square below, a low-pass breathing
/// slowly, the detune drifting.
fn warm_pad() -> &'static [(usize, f32)] {
    B::new()
        .unison(0, 7, 0.22, 1.0)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .set(p::osc(1, p::POS), 1.0)
        .octave(1, -1)
        .unison(1, 3, 0.15, 0.8)
        .set(p::osc(1, p::LEVEL), 0.35)
        .filter(0, ft::LOW12, 1800.0, 0.15)
        .env(0, 0.9, 1.0, 1.0, 1.8)
        .set(p::env(0, p::E_A_CURVE), 0.3)
        .lfo(0, "Free", 0.12)
        .preset_shape(0, "Sine")
        .lfo(1, "Free", 0.07)
        .set(p::lfo_style(1), 2.0)
        .route("LFO 1", "Filter 1 cutoff", 0.15)
        .route("LFO 2", "Osc 1 detune", 0.15)
        .set(p::VOLUME, 0.4)
        .done()
}

/// Glass: organ and sweep tables high and clean, wandering at random.
fn glass_pad() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "Organ")
        .set(p::osc(0, p::POS), 0.7)
        .unison(0, 5, 0.15, 1.0)
        .set(p::osc(1, p::OSC_ON), 1.0)
        .table(1, "Sweep")
        .set(p::osc(1, p::POS), 0.8)
        .octave(1, 1)
        .unison(1, 3, 0.12, 1.0)
        .set(p::osc(1, p::LEVEL), 0.3)
        .filter(0, ft::HIGH12, 300.0, 0.1)
        .env(0, 1.2, 1.0, 1.0, 2.5)
        .set(p::env(0, p::E_A_CURVE), 0.3)
        .lfo(0, "Free", 0.15)
        .set(p::lfo_style(0), 2.0)
        .lfo(1, "Free", 0.11)
        .set(p::lfo_style(1), 2.0)
        .route("LFO 1", "Osc 1 position", 0.3)
        .route("LFO 2", "Osc 2 position", 0.3)
        .set(p::VOLUME, 0.72)
        .done()
}

/// A build-up: noise and a sweep table rising for four bars (a one-shot
/// LFO), up an octave and wide open by the end.
fn riser() -> &'static [(usize, f32)] {
    B::new()
        .table(0, "Sweep")
        .set(p::osc(0, p::POS), 0.0)
        .unison(0, 7, 0.3, 1.0)
        .set(p::NOISE_ON, 1.0)
        .set(p::NOISE_COLOR, 0.4)
        .set(p::NOISE_LEVEL, 0.35)
        .filter(0, ft::LOW12, 500.0, 0.3)
        .env(0, 0.1, 1.0, 1.0, 1.0)
        .lfo(0, "4 bars", 0.25)
        .lfo_mode(0, 2.0)
        .preset_shape(0, "Ramp up")
        .route("LFO 1", "Osc 1 position", 1.0)
        .route("LFO 1", "Filter 1 cutoff", 0.75)
        .route("LFO 1", "Pitch", 0.5)
        .route("LFO 1", "Noise level", 0.8)
        .set(p::VOLUME, 0.35)
        .done()
}
