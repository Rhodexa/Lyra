//! Fluttersine (id `lyra.fluttersine`): a six-operator FM (phase-modulation) synth, ported from the owner's
//! Fluttersine web synth.
//!
//! ```text
//!  per voice:  6 operators ──► 6×6 matrix (row modulates column, diagonal = feedback)
//!              └─ OUT mix ──► [24 dB Butterworth] ─► resonant SVF low-pass ─► SVF high-pass ─► sum
//!  global:     sum × volume ─► DC blocker ─► tanh soft clip ─► out L = R
//! ```
//! * Each operator: 11 waveforms (OPL-style sines, triangle, saw, pulse, 7-saw supersaw,
//!   noise), a phase warp ("PW") that works on every shape, ratio or fixed Hz, fine tune,
//!   its own ADSR and velocity sensitivity.
//! * Modulation is phase offset in cycles: a matrix value of 1 shifts the target by up to
//!   one full cycle. Self-feedback averages the last two samples (DX7 style) to stay tame.
//! * Saw, pulse and supersaw are band-limited with polyBLEP while unmodulated (once a
//!   phase is modulated the edge no longer lands where BLEP expects it, so it is skipped).
//! * Pitch, pulse width, the filter envelope and filter coefficients run at a control
//!   rate of 16 samples; operators and envelopes run per sample.
//! * Operators that cannot reach the output (directly or through the matrix) are skipped.
//! * 16 voices, stolen like `synth.rs` (free, else quietest released, else oldest),
//!   plus a mono mode with a note stack, legato and glide.

use lyra_dsp::Rng;
use lyra_plugin::{Curve, Event, ParamGroup, ParamInfo, Plugin, PluginInfo, ProcessContext, Transport};
use std::f32::consts::{PI, TAU};

const MAX_VOICES: usize = 16;
const OPS: usize = p::OPS;
/// Pitch, PWM, filter envelope and filter coefficients are updated every this many samples.
const CR: usize = 16;
/// Voices rendered side by side (see `render_lanes`).
const LANES: usize = 2;
/// Notes remembered by mono mode (for falling back to a held key on release).
const STACK: usize = 16;
/// Detune of the seven supersaw voices relative to the centre one (JP-8000 style).
const SSD: [f32; 7] = [-0.110_023_13, -0.062_884_39, -0.019_523_56, 0.0, 0.019_912_21, 0.062_165_38, 0.107_452_42];

use Curve::{Exponential as Exp, Linear as Lin};

/// Fluttersine, as Lyra lists it.
pub const PLUGINS: &[PluginInfo] = &[PluginInfo {
    id: "lyra.fluttersine",
    name: "Fluttersine",
    melodic: true,
    params: &FM_PARAMS,
    groups: &FM_GROUPS,
    presets: FM_PRESETS,
    create: || Box::new(Fm::new()),
}];

/// Parameter indices. `op(i, OP_RATIO)` is operator `i`'s ratio (i = 0..6),
/// `matrix(row, col)` is "operator `row` modulates operator `col`".
pub mod p {
    pub const OPS: usize = 6;

    // Per-operator parameters (offsets inside one operator's block).
    pub const OP_WAVE: usize = 0;
    /// 0 = follow the key × ratio, 1 = fixed Hz.
    pub const OP_FIXED: usize = 1;
    /// 1 = restart the phase on note-on.
    pub const OP_RESET: usize = 2;
    pub const OP_RATIO: usize = 3;
    pub const OP_HZ: usize = 4;
    /// Cents.
    pub const OP_FINE: usize = 5;
    /// Phase warp: pulse width on Pulse, "PWM on anything" on the other shapes.
    pub const OP_PW: usize = 6;
    /// LFO > pulse width.
    pub const OP_PWM: usize = 7;
    /// Supersaw detune spread.
    pub const OP_SPREAD: usize = 8;
    /// Supersaw side-voice mix.
    pub const OP_SS_MIX: usize = 9;
    pub const OP_VEL: usize = 10;
    pub const OP_ATTACK: usize = 11;
    pub const OP_DECAY: usize = 12;
    pub const OP_SUSTAIN: usize = 13;
    pub const OP_RELEASE: usize = 14;
    pub const OP_STRIDE: usize = 15;

    pub const fn op(i: usize, param: usize) -> usize {
        i * OP_STRIDE + param
    }

    pub const MATRIX: usize = OPS * OP_STRIDE;
    pub const fn matrix(row: usize, col: usize) -> usize {
        MATRIX + row * OPS + col
    }

    // Filter
    pub const LP_ON: usize = MATRIX + OPS * OPS;
    pub const LP_SLOPE: usize = LP_ON + 1;
    pub const CUTOFF: usize = LP_ON + 2;
    pub const RESO: usize = LP_ON + 3;
    /// Filter envelope > cutoff, in octaves.
    pub const F_ENV: usize = LP_ON + 4;
    pub const KEYTRACK: usize = LP_ON + 5;
    /// LFO > cutoff, in octaves.
    pub const F_LFO: usize = LP_ON + 6;
    pub const F_ATTACK: usize = LP_ON + 7;
    pub const F_DECAY: usize = LP_ON + 8;
    pub const F_SUSTAIN: usize = LP_ON + 9;
    pub const F_RELEASE: usize = LP_ON + 10;
    pub const HP_ON: usize = LP_ON + 11;
    pub const HP_CUTOFF: usize = LP_ON + 12;
    pub const HP_RESO: usize = LP_ON + 13;

    // LFO
    pub const LFO_SHAPE: usize = LP_ON + 14;
    pub const LFO_RATE: usize = LFO_SHAPE + 1;
    pub const LFO_RETRIG: usize = LFO_SHAPE + 2;
    /// LFO > pitch, in cents.
    pub const VIBRATO: usize = LFO_SHAPE + 3;

    // Output
    pub const OUT: usize = LFO_SHAPE + 4;
    pub const fn out(i: usize) -> usize {
        OUT + i
    }
    pub const VOLUME: usize = OUT + OPS;
    /// 0 = poly, 1 = mono.
    pub const MODE: usize = VOLUME + 1;
    pub const GLIDE: usize = VOLUME + 2;
    /// 0 = glide only between overlapping (legato) notes, 1 = always.
    pub const GLIDE_MODE: usize = VOLUME + 3;

    // ── Added in Lyra (after the port; ids continue at the end) ──
    /// LFO 1 tempo sync: 0 = free (use LFO_RATE), else a note length (`SYNC_BEATS`).
    pub const LFO_SYNC: usize = GLIDE_MODE + 1;
    pub const LFO2_SHAPE: usize = LFO_SYNC + 1;
    pub const LFO2_RATE: usize = LFO_SYNC + 2;
    pub const LFO2_SYNC: usize = LFO_SYNC + 3;
    pub const LFO2_RETRIG: usize = LFO_SYNC + 4;

    /// Two modulation envelopes (attack, decay, sustain, release), sources for the mod slots.
    pub const ENV1: usize = LFO2_RETRIG + 1;
    pub const ENV2: usize = ENV1 + 4;
    pub const ENV_ATTACK: usize = 0;
    pub const ENV_DECAY: usize = 1;
    pub const ENV_SUSTAIN: usize = 2;
    pub const ENV_RELEASE: usize = 3;
    pub const fn env(e: usize, param: usize) -> usize {
        ENV1 + e * 4 + param
    }

    /// Mod slots: each routes a source (`MOD_SOURCES`) to a destination
    /// (`MOD_DESTS`) by a bipolar amount.
    pub const MOD_SLOTS: usize = 6;
    pub const MODS: usize = ENV2 + 4;
    pub const fn mod_src(k: usize) -> usize {
        MODS + k * 3
    }
    pub const fn mod_dest(k: usize) -> usize {
        MODS + k * 3 + 1
    }
    pub const fn mod_amt(k: usize) -> usize {
        MODS + k * 3 + 2
    }

    pub const COUNT: usize = MODS + MOD_SLOTS * 3;
}

/// LFO sync choices, as labels; `SYNC_BEATS` gives each one's length in quarter notes.
pub const SYNC_LABELS: [&str; 13] =
    ["Free", "4 bars", "2 bars", "1 bar", "1/2", "1/4", "1/4 T", "1/8", "1/8 D", "1/8 T", "1/16", "1/16 T", "1/32"];
pub const SYNC_BEATS: [f32; 13] =
    [0.0, 16.0, 8.0, 4.0, 2.0, 1.0, 2.0 / 3.0, 0.5, 0.75, 1.0 / 3.0, 0.25, 1.0 / 6.0, 0.125];

/// What a mod slot listens to. LFOs are bipolar (-1..1), envelopes and
/// velocity 0..1, key -1..1 around C4, random -1..1 (new on every note).
pub const MOD_SOURCES: [&str; 8] = ["Off", "LFO 1", "LFO 2", "Env 1", "Env 2", "Velocity", "Key", "Random"];

/// Where a mod slot goes, and what an amount of 1 at a full source means:
/// pitch ±24 st, cutoff ±6 oct, volume and op levels ±100%, FM depth ×(1 ± 2)
/// (it scales the modulation already set in the matrix), op warp ±0.48.
#[rustfmt::skip]
pub const MOD_DESTS: [&str; 29] = [
    "Off", "Pitch", "Cutoff", "Volume", "All FM",
    "Op 1 level", "Op 2 level", "Op 3 level", "Op 4 level", "Op 5 level", "Op 6 level",
    "Op 1 FM in", "Op 2 FM in", "Op 3 FM in", "Op 4 FM in", "Op 5 FM in", "Op 6 FM in",
    "Op 1 pitch", "Op 2 pitch", "Op 3 pitch", "Op 4 pitch", "Op 5 pitch", "Op 6 pitch",
    "Op 1 warp", "Op 2 warp", "Op 3 warp", "Op 4 warp", "Op 5 warp", "Op 6 warp",
];
/// First index of each per-operator block in `MOD_DESTS`.
pub const DEST_LEVEL: usize = 5;
pub const DEST_FM_IN: usize = 11;
pub const DEST_OP_PITCH: usize = 17;
pub const DEST_WARP: usize = 23;

const W_TRI: u8 = 6;
const W_SAW: u8 = 7;
const W_PULSE: u8 = 8;
const W_SSAW: u8 = 9;
const W_NOISE: u8 = 10;

const WAVES: [&str; 11] = [
    "Sine",
    "Half sine",
    "Abs sine",
    "Quarter sine",
    "Alt sine",
    "Camel sine",
    "Triangle",
    "Saw",
    "Pulse",
    "Supersaw",
    "Noise",
];
const OFF_ON: [&str; 2] = ["Off", "On"];

#[rustfmt::skip]
const MATRIX_NAMES: [&str; OPS * OPS] = [
    "Op 1 feedback", "Op 1 > Op 2", "Op 1 > Op 3", "Op 1 > Op 4", "Op 1 > Op 5", "Op 1 > Op 6",
    "Op 2 > Op 1", "Op 2 feedback", "Op 2 > Op 3", "Op 2 > Op 4", "Op 2 > Op 5", "Op 2 > Op 6",
    "Op 3 > Op 1", "Op 3 > Op 2", "Op 3 feedback", "Op 3 > Op 4", "Op 3 > Op 5", "Op 3 > Op 6",
    "Op 4 > Op 1", "Op 4 > Op 2", "Op 4 > Op 3", "Op 4 feedback", "Op 4 > Op 5", "Op 4 > Op 6",
    "Op 5 > Op 1", "Op 5 > Op 2", "Op 5 > Op 3", "Op 5 > Op 4", "Op 5 feedback", "Op 5 > Op 6",
    "Op 6 > Op 1", "Op 6 > Op 2", "Op 6 > Op 3", "Op 6 > Op 4", "Op 6 > Op 5", "Op 6 feedback",
];
const OUT_NAMES: [&str; OPS] = ["Op 1 out", "Op 2 out", "Op 3 out", "Op 4 out", "Op 5 out", "Op 6 out"];

const fn build_params() -> [ParamInfo; p::COUNT] {
    use p::*;
    let mut a = [ParamInfo::new(0, "", 0.0, 1.0, 0.0, "", Lin); COUNT];
    macro_rules! set {
        ($i:expr, new $name:expr, $min:expr, $max:expr, $def:expr, $unit:expr, $curve:expr) => {
            a[$i] = ParamInfo::new($i as u32, $name, $min, $max, $def, $unit, $curve)
        };
        ($i:expr, choice $name:expr, $labels:expr, $def:expr) => {
            a[$i] = ParamInfo::choice($i as u32, $name, $labels, $def)
        };
    }
    let mut i = 0;
    while i < OPS {
        set!(op(i, OP_WAVE), choice "Wave", &WAVES, 0);
        set!(op(i, OP_FIXED), choice "Pitch", &["Ratio", "Fixed Hz"], 0);
        set!(op(i, OP_RESET), choice "Phase", &["Free", "Reset"], 1);
        set!(op(i, OP_RATIO), new "Ratio", 0.125, 32.0, 1.0, "", Exp);
        set!(op(i, OP_HZ), new "Fixed", 1.0, 20_000.0, 440.0, "Hz", Exp);
        set!(op(i, OP_FINE), new "Fine", -100.0, 100.0, 0.0, "ct", Lin);
        set!(op(i, OP_PW), new "Warp / PW", 0.02, 0.98, 0.5, "%", Lin);
        set!(op(i, OP_PWM), new "PWM", 0.0, 1.0, 0.0, "%", Lin);
        set!(op(i, OP_SPREAD), new "SS spread", 0.0, 1.0, 0.35, "%", Lin);
        set!(op(i, OP_SS_MIX), new "SS mix", 0.0, 1.0, 0.6, "%", Lin);
        set!(op(i, OP_VEL), new "Velocity", 0.0, 1.0, 0.5, "%", Lin);
        set!(op(i, OP_ATTACK), new "Attack", 0.001, 10.0, 0.003, "s", Exp);
        set!(op(i, OP_DECAY), new "Decay", 0.001, 10.0, 0.4, "s", Exp);
        set!(op(i, OP_SUSTAIN), new "Sustain", 0.0, 1.0, 1.0, "%", Lin);
        set!(op(i, OP_RELEASE), new "Release", 0.001, 10.0, 0.25, "s", Exp);
        i += 1;
    }
    let mut m = 0;
    while m < OPS * OPS {
        set!(MATRIX + m, new MATRIX_NAMES[m], 0.0, 1.0, 0.0, "%", Lin);
        m += 1;
    }
    set!(LP_ON, choice "Low-pass", &OFF_ON, 0);
    set!(LP_SLOPE, choice "Slope", &["12 dB", "24 dB"], 1);
    set!(CUTOFF, new "Cutoff", 20.0, 20_000.0, 8000.0, "Hz", Exp);
    set!(RESO, new "Reso", 0.0, 1.0, 0.1, "%", Lin);
    set!(F_ENV, new "Env amt", -6.0, 6.0, 0.0, "oct", Lin);
    set!(KEYTRACK, new "Key track", 0.0, 1.0, 0.0, "%", Lin);
    set!(F_LFO, new "LFO amt", -4.0, 4.0, 0.0, "oct", Lin);
    set!(F_ATTACK, new "F. attack", 0.001, 10.0, 0.003, "s", Exp);
    set!(F_DECAY, new "F. decay", 0.001, 10.0, 0.4, "s", Exp);
    set!(F_SUSTAIN, new "F. sustain", 0.0, 1.0, 0.5, "%", Lin);
    set!(F_RELEASE, new "F. release", 0.001, 10.0, 0.3, "s", Exp);
    set!(HP_ON, choice "High-pass", &OFF_ON, 0);
    set!(HP_CUTOFF, new "HP cutoff", 20.0, 12_000.0, 30.0, "Hz", Exp);
    set!(HP_RESO, new "HP reso", 0.0, 1.0, 0.0, "%", Lin);

    set!(LFO_SHAPE, choice "Shape", &["Sine", "Triangle", "Saw", "Square", "S&H"], 0);
    set!(LFO_RATE, new "Rate", 0.05, 30.0, 5.0, "Hz", Exp);
    set!(LFO_RETRIG, choice "Retrigger", &OFF_ON, 0);
    set!(VIBRATO, new "Vibrato", 0.0, 1200.0, 0.0, "ct", Lin);

    let mut o = 0;
    while o < OPS {
        set!(out(o), new OUT_NAMES[o], 0.0, 1.0, if o == 0 { 1.0 } else { 0.0 }, "%", Lin);
        o += 1;
    }
    set!(VOLUME, new "Volume", 0.0, 1.0, 0.7, "%", Lin);
    set!(MODE, choice "Mode", &["Poly", "Mono"], 0);
    set!(GLIDE, new "Glide", 0.0, 2.0, 0.08, "s", Lin);
    set!(GLIDE_MODE, choice "Glide when", &["Legato", "Always"], 0);

    set!(LFO_SYNC, choice "Sync", &SYNC_LABELS, 0);
    set!(LFO2_SHAPE, choice "LFO 2 shape", &["Sine", "Triangle", "Saw", "Square", "S&H"], 0);
    set!(LFO2_RATE, new "LFO 2 rate", 0.05, 30.0, 1.0, "Hz", Exp);
    set!(LFO2_SYNC, choice "LFO 2 sync", &SYNC_LABELS, 0);
    set!(LFO2_RETRIG, choice "LFO 2 retrigger", &OFF_ON, 0);
    let mut e = 0;
    while e < 2 {
        let names = if e == 0 {
            ["Env 1 attack", "Env 1 decay", "Env 1 sustain", "Env 1 release"]
        } else {
            ["Env 2 attack", "Env 2 decay", "Env 2 sustain", "Env 2 release"]
        };
        set!(env(e, ENV_ATTACK), new names[0], 0.001, 10.0, 0.001, "s", Exp);
        set!(env(e, ENV_DECAY), new names[1], 0.001, 10.0, 0.3, "s", Exp);
        set!(env(e, ENV_SUSTAIN), new names[2], 0.0, 1.0, 0.0, "%", Lin);
        set!(env(e, ENV_RELEASE), new names[3], 0.001, 10.0, 0.2, "s", Exp);
        e += 1;
    }
    const SLOT_NAMES: [[&str; 3]; MOD_SLOTS] = [
        ["Mod 1 source", "Mod 1 dest", "Mod 1 amount"],
        ["Mod 2 source", "Mod 2 dest", "Mod 2 amount"],
        ["Mod 3 source", "Mod 3 dest", "Mod 3 amount"],
        ["Mod 4 source", "Mod 4 dest", "Mod 4 amount"],
        ["Mod 5 source", "Mod 5 dest", "Mod 5 amount"],
        ["Mod 6 source", "Mod 6 dest", "Mod 6 amount"],
    ];
    let mut k = 0;
    while k < MOD_SLOTS {
        set!(mod_src(k), choice SLOT_NAMES[k][0], &MOD_SOURCES, 0);
        set!(mod_dest(k), choice SLOT_NAMES[k][1], &MOD_DESTS, 0);
        set!(mod_amt(k), new SLOT_NAMES[k][2], -1.0, 1.0, 0.0, "%", Lin);
        k += 1;
    }
    a
}

pub static FM_PARAMS: [ParamInfo; p::COUNT] = build_params();

const fn op_group(name: &'static str, i: usize) -> ParamGroup {
    ParamGroup { name, ids: (i * p::OP_STRIDE) as u32..((i + 1) * p::OP_STRIDE) as u32 }
}

static FM_GROUPS: [ParamGroup; 14] = [
    op_group("Op 1", 0),
    op_group("Op 2", 1),
    op_group("Op 3", 2),
    op_group("Op 4", 3),
    op_group("Op 5", 4),
    op_group("Op 6", 5),
    ParamGroup { name: "Matrix", ids: p::MATRIX as u32..p::LP_ON as u32 },
    ParamGroup { name: "Filter", ids: p::LP_ON as u32..p::LFO_SHAPE as u32 },
    ParamGroup { name: "LFO", ids: p::LFO_SHAPE as u32..p::OUT as u32 },
    ParamGroup { name: "Output", ids: p::OUT as u32..p::LFO_SYNC as u32 },
    ParamGroup { name: "LFO sync, LFO 2", ids: p::LFO_SYNC as u32..p::ENV1 as u32 },
    ParamGroup { name: "Env 1", ids: p::ENV1 as u32..p::ENV2 as u32 },
    ParamGroup { name: "Env 2", ids: p::ENV2 as u32..p::MODS as u32 },
    ParamGroup { name: "Mod slots", ids: p::MODS as u32..p::COUNT as u32 },
];

/// Fluttersine's factory patches: (name, parameters that differ from the defaults).
/// Its chorus/delay settings are not included (those become Lyra effects), nor
/// mod-wheel depth (Lyra has no mod wheel yet).
pub const FM_PRESETS: &[(&str, &[(usize, f32)])] = {
    use p::*;
    &[
        ("Init", &[]),
        (
            "FM E-Piano",
            &[
                (op(0, OP_ATTACK), 0.002),
                (op(0, OP_DECAY), 2.5),
                (op(0, OP_SUSTAIN), 0.0),
                (op(0, OP_RELEASE), 0.6),
                (op(0, OP_VEL), 0.6),
                (op(1, OP_ATTACK), 0.002),
                (op(1, OP_DECAY), 1.4),
                (op(1, OP_SUSTAIN), 0.0),
                (op(1, OP_RELEASE), 0.5),
                (op(1, OP_VEL), 0.9),
                (op(2, OP_FINE), 4.0),
                (op(2, OP_ATTACK), 0.002),
                (op(2, OP_DECAY), 2.0),
                (op(2, OP_SUSTAIN), 0.0),
                (op(2, OP_RELEASE), 0.6),
                (op(2, OP_VEL), 0.6),
                (op(3, OP_RATIO), 14.0),
                (op(3, OP_ATTACK), 0.001),
                (op(3, OP_DECAY), 0.22),
                (op(3, OP_SUSTAIN), 0.0),
                (op(3, OP_RELEASE), 0.1),
                (op(3, OP_VEL), 1.0),
                (matrix(1, 0), 0.3),
                (matrix(3, 2), 0.1),
                (out(0), 0.6),
                (out(2), 0.45),
            ],
        ),
        (
            "FM Bell",
            &[
                (op(0, OP_ATTACK), 0.001),
                (op(0, OP_DECAY), 4.0),
                (op(0, OP_SUSTAIN), 0.0),
                (op(0, OP_RELEASE), 3.0),
                (op(1, OP_RATIO), 3.5),
                (op(1, OP_ATTACK), 0.001),
                (op(1, OP_DECAY), 2.5),
                (op(1, OP_SUSTAIN), 0.0),
                (op(1, OP_RELEASE), 2.0),
                (op(2, OP_FINE), 7.0),
                (op(2, OP_ATTACK), 0.001),
                (op(2, OP_DECAY), 3.0),
                (op(2, OP_SUSTAIN), 0.0),
                (op(2, OP_RELEASE), 2.5),
                (op(3, OP_RATIO), 5.19),
                (op(3, OP_ATTACK), 0.001),
                (op(3, OP_DECAY), 1.2),
                (op(3, OP_SUSTAIN), 0.0),
                (op(3, OP_RELEASE), 1.0),
                (matrix(1, 0), 0.45),
                (matrix(3, 2), 0.3),
                (out(0), 0.5),
                (out(2), 0.35),
            ],
        ),
        (
            "FM Brass",
            &[
                (op(0, OP_ATTACK), 0.04),
                (op(0, OP_DECAY), 0.6),
                (op(0, OP_SUSTAIN), 0.8),
                (op(0, OP_RELEASE), 0.2),
                (op(1, OP_ATTACK), 0.08),
                (op(1, OP_DECAY), 0.8),
                (op(1, OP_SUSTAIN), 0.55),
                (op(1, OP_RELEASE), 0.25),
                (op(1, OP_VEL), 0.8),
                (matrix(1, 0), 0.55),
                (matrix(1, 1), 0.2),
                (LP_ON, 1.0),
                (CUTOFF, 6000.0),
                (RESO, 0.1),
            ],
        ),
        (
            "Feedback Growl",
            &[
                (MODE, 1.0),
                (GLIDE, 0.05),
                (op(0, OP_ATTACK), 0.005),
                (op(0, OP_DECAY), 0.3),
                (op(0, OP_SUSTAIN), 1.0),
                (op(0, OP_RELEASE), 0.15),
                (matrix(0, 0), 0.45),
                (op(1, OP_RATIO), 0.5),
                (op(1, OP_ATTACK), 0.005),
                (op(1, OP_SUSTAIN), 1.0),
                (op(1, OP_RELEASE), 0.15),
                (out(0), 0.7),
                (out(1), 0.6),
                (LP_ON, 1.0),
                (CUTOFF, 400.0),
                (RESO, 0.35),
                (F_LFO, 3.0),
                (LFO_RATE, 4.0),
                (LFO_RETRIG, 1.0),
            ],
        ),
        (
            "Supersaw Chords",
            &[
                (op(0, OP_WAVE), W_SSAW as f32),
                (op(0, OP_SPREAD), 0.4),
                (op(0, OP_SS_MIX), 0.7),
                (op(0, OP_ATTACK), 0.01),
                (op(0, OP_DECAY), 0.5),
                (op(0, OP_SUSTAIN), 0.85),
                (op(0, OP_RELEASE), 0.45),
                (op(1, OP_WAVE), W_SSAW as f32),
                (op(1, OP_RATIO), 2.0),
                (op(1, OP_SPREAD), 0.4),
                (op(1, OP_SS_MIX), 0.6),
                (op(1, OP_ATTACK), 0.01),
                (op(1, OP_DECAY), 0.5),
                (op(1, OP_SUSTAIN), 0.85),
                (op(1, OP_RELEASE), 0.45),
                (out(0), 0.7),
                (out(1), 0.3),
                (LP_ON, 1.0),
                (CUTOFF, 7000.0),
                (RESO, 0.1),
                (HP_ON, 1.0),
                (HP_CUTOFF, 120.0),
            ],
        ),
        (
            "Supersaw Wobble",
            &[
                (op(0, OP_WAVE), W_SSAW as f32),
                (op(0, OP_SPREAD), 0.5),
                (op(0, OP_SS_MIX), 0.75),
                (op(0, OP_ATTACK), 0.005),
                (op(0, OP_SUSTAIN), 1.0),
                (op(0, OP_RELEASE), 0.3),
                (op(1, OP_RATIO), 0.5),
                (op(1, OP_ATTACK), 0.005),
                (op(1, OP_SUSTAIN), 1.0),
                (op(1, OP_RELEASE), 0.3),
                (out(0), 0.8),
                (out(1), 0.4),
                (LP_ON, 1.0),
                (CUTOFF, 500.0),
                (RESO, 0.3),
                (F_LFO, 3.5),
                (LFO_RATE, 3.2),
                (LFO_RETRIG, 1.0),
            ],
        ),
        (
            "Saw Pluck (LPF)",
            &[
                (op(0, OP_WAVE), W_SAW as f32),
                (op(0, OP_ATTACK), 0.001),
                (op(0, OP_DECAY), 0.8),
                (op(0, OP_SUSTAIN), 0.3),
                (op(0, OP_RELEASE), 0.3),
                (op(1, OP_WAVE), W_SAW as f32),
                (op(1, OP_FINE), 8.0),
                (op(1, OP_ATTACK), 0.001),
                (op(1, OP_DECAY), 0.8),
                (op(1, OP_SUSTAIN), 0.3),
                (op(1, OP_RELEASE), 0.3),
                (out(0), 0.6),
                (out(1), 0.6),
                (LP_ON, 1.0),
                (CUTOFF, 250.0),
                (RESO, 0.25),
                (F_ENV, 5.5),
                (F_ATTACK, 0.001),
                (F_DECAY, 0.25),
                (F_SUSTAIN, 0.0),
                (F_RELEASE, 0.2),
            ],
        ),
        (
            "PWM Strings",
            &[
                (op(0, OP_WAVE), W_PULSE as f32),
                (op(0, OP_PWM), 0.4),
                (op(0, OP_ATTACK), 0.35),
                (op(0, OP_DECAY), 1.0),
                (op(0, OP_SUSTAIN), 0.85),
                (op(0, OP_RELEASE), 0.9),
                (op(1, OP_WAVE), W_PULSE as f32),
                (op(1, OP_FINE), 9.0),
                (op(1, OP_PWM), 0.35),
                (op(1, OP_ATTACK), 0.35),
                (op(1, OP_DECAY), 1.0),
                (op(1, OP_SUSTAIN), 0.85),
                (op(1, OP_RELEASE), 0.9),
                (out(0), 0.6),
                (out(1), 0.5),
                (LFO_RATE, 0.8),
                (LP_ON, 1.0),
                (CUTOFF, 4000.0),
                (RESO, 0.05),
                (KEYTRACK, 0.3),
            ],
        ),
        (
            "Chip Lead 25% (Mono Glide)",
            &[
                (MODE, 1.0),
                (GLIDE, 0.06),
                (op(0, OP_WAVE), W_PULSE as f32),
                (op(0, OP_PW), 0.25),
                (op(0, OP_ATTACK), 0.001),
                (op(0, OP_DECAY), 0.15),
                (op(0, OP_SUSTAIN), 0.85),
                (op(0, OP_RELEASE), 0.04),
                (op(0, OP_VEL), 0.3),
                (LFO_RATE, 6.0),
            ],
        ),
        (
            "Chip Pulse 12.5%",
            &[
                (op(0, OP_WAVE), W_PULSE as f32),
                (op(0, OP_PW), 0.125),
                (op(0, OP_ATTACK), 0.001),
                (op(0, OP_DECAY), 0.2),
                (op(0, OP_SUSTAIN), 0.6),
                (op(0, OP_RELEASE), 0.05),
            ],
        ),
        (
            "Chip Triangle Bass",
            &[
                (MODE, 1.0),
                (GLIDE, 0.0),
                (op(0, OP_WAVE), W_TRI as f32),
                (op(0, OP_ATTACK), 0.001),
                (op(0, OP_DECAY), 0.1),
                (op(0, OP_SUSTAIN), 1.0),
                (op(0, OP_RELEASE), 0.03),
            ],
        ),
        (
            "OPL Organ",
            &[
                (op(0, OP_WAVE), 4.0),
                (op(0, OP_ATTACK), 0.004),
                (op(0, OP_SUSTAIN), 1.0),
                (op(0, OP_RELEASE), 0.06),
                (op(1, OP_WAVE), 5.0),
                (op(1, OP_RATIO), 2.0),
                (op(1, OP_ATTACK), 0.004),
                (op(1, OP_SUSTAIN), 1.0),
                (op(1, OP_RELEASE), 0.06),
                (op(2, OP_WAVE), 0.0),
                (op(2, OP_RATIO), 4.0),
                (op(2, OP_ATTACK), 0.004),
                (op(2, OP_SUSTAIN), 1.0),
                (op(2, OP_RELEASE), 0.06),
                (op(3, OP_WAVE), 1.0),
                (op(3, OP_RATIO), 0.5),
                (op(3, OP_ATTACK), 0.004),
                (op(3, OP_SUSTAIN), 1.0),
                (op(3, OP_RELEASE), 0.06),
                (out(0), 0.6),
                (out(1), 0.4),
                (out(2), 0.25),
                (out(3), 0.3),
                (HP_ON, 1.0),
                (HP_CUTOFF, 30.0),
            ],
        ),
        (
            "OPL Half-Sine Keys",
            &[
                (op(0, OP_WAVE), 1.0),
                (op(0, OP_ATTACK), 0.002),
                (op(0, OP_DECAY), 1.5),
                (op(0, OP_SUSTAIN), 0.2),
                (op(0, OP_RELEASE), 0.4),
                (op(1, OP_WAVE), 2.0),
                (op(1, OP_RATIO), 2.0),
                (op(1, OP_ATTACK), 0.002),
                (op(1, OP_DECAY), 0.5),
                (op(1, OP_SUSTAIN), 0.1),
                (op(1, OP_RELEASE), 0.3),
                (matrix(1, 0), 0.3),
                (out(0), 0.8),
                (HP_ON, 1.0),
                (HP_CUTOFF, 40.0),
            ],
        ),
        (
            "Reese Bass",
            &[
                (MODE, 1.0),
                (GLIDE, 0.08),
                (op(0, OP_WAVE), W_SAW as f32),
                (op(0, OP_ATTACK), 0.005),
                (op(0, OP_SUSTAIN), 1.0),
                (op(0, OP_RELEASE), 0.15),
                (op(1, OP_WAVE), W_SAW as f32),
                (op(1, OP_FINE), 22.0),
                (op(1, OP_ATTACK), 0.005),
                (op(1, OP_SUSTAIN), 1.0),
                (op(1, OP_RELEASE), 0.15),
                (out(0), 0.6),
                (out(1), 0.6),
                (LP_ON, 1.0),
                (CUTOFF, 700.0),
                (RESO, 0.2),
                (F_LFO, 0.6),
                (LFO_RATE, 0.3),
            ],
        ),
        (
            "Sub Bass",
            &[
                (MODE, 1.0),
                (GLIDE, 0.04),
                (op(0, OP_ATTACK), 0.003),
                (op(0, OP_SUSTAIN), 1.0),
                (op(0, OP_RELEASE), 0.08),
                (op(1, OP_RATIO), 2.0),
                (op(1, OP_ATTACK), 0.003),
                (op(1, OP_SUSTAIN), 1.0),
                (op(1, OP_RELEASE), 0.08),
                (out(0), 1.0),
                (out(1), 0.12),
            ],
        ),
        (
            "Metallic (Fixed Hz)",
            &[
                (op(0, OP_ATTACK), 0.001),
                (op(0, OP_DECAY), 1.5),
                (op(0, OP_SUSTAIN), 0.0),
                (op(0, OP_RELEASE), 1.0),
                (op(1, OP_FIXED), 1.0),
                (op(1, OP_HZ), 1234.0),
                (op(1, OP_ATTACK), 0.001),
                (op(1, OP_DECAY), 1.0),
                (op(1, OP_SUSTAIN), 0.0),
                (op(1, OP_RELEASE), 0.8),
                (op(2, OP_FIXED), 1.0),
                (op(2, OP_HZ), 377.0),
                (op(2, OP_ATTACK), 0.001),
                (op(2, OP_DECAY), 0.6),
                (op(2, OP_SUSTAIN), 0.0),
                (op(2, OP_RELEASE), 0.5),
                (matrix(1, 0), 0.5),
                (matrix(2, 0), 0.3),
                (out(0), 0.7),
            ],
        ),
        (
            "Noise Hat (HPF)",
            &[
                (op(0, OP_WAVE), W_NOISE as f32),
                (op(0, OP_ATTACK), 0.001),
                (op(0, OP_DECAY), 0.07),
                (op(0, OP_SUSTAIN), 0.0),
                (op(0, OP_RELEASE), 0.06),
                (HP_ON, 1.0),
                (HP_CUTOFF, 7000.0),
                (HP_RESO, 0.2),
            ],
        ),
    ]
};

// ---------------------------------------------------------------- DSP helpers

#[inline]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// 1.5 × 2^23: adding and subtracting it rounds an f32 to the nearest integer.
const ROUND_MAGIC: f32 = 12_582_912.0;

/// Nearest integer, without a libm call (baseline x86-64 has no `roundss`). |x| < 2^22.
#[inline]
fn round_fast(x: f32) -> f32 {
    (x + ROUND_MAGIC) - ROUND_MAGIC
}

/// `x - floor(x)`: wraps a modulated phase back into `0..=1`, branch-free.
#[inline]
fn wrap(x: f32) -> f32 {
    x - round_fast(x - 0.5)
}

/// `sin(2π·x)` for any phase `x` (in turns), as an odd Taylor polynomial evaluated in
/// Estrin form (error < 1e-7). Much cheaper than `f32::sin`, and it sits on the critical
/// path of every FM chain.
#[inline]
fn sin_turns(x: f32) -> f32 {
    const C: [f32; 6] = [
        TAU,
        -41.341_7,   // -(2π)^3 / 3!
        81.605_25,   //  (2π)^5 / 5!
        -76.705_86,  // -(2π)^7 / 7!
        42.058_693,  //  (2π)^9 / 9!
        -15.094_643, // -(2π)^11 / 11!
    ];
    // Reduce to -0.5..0.5, then fold into -0.25..0.25 using sin(π - θ) = sin θ. Arithmetic, not branches:
    // the branches would mispredict on every quarter cycle.
    let x = x - round_fast(x);
    let x = (0.25 - (x.abs() - 0.25).abs()).copysign(x);
    let z = x * x;
    let z2 = z * z;
    x * ((C[0] + C[1] * z) + z2 * ((C[2] + C[3] * z) + z2 * (C[4] + C[5] * z)))
}

/// Phase warp: the first half of the cycle is squeezed into `0..pw`, the second half into
/// `pw..1`. On a pulse this is classic PWM; on the other shapes it is "PWM on anything".
/// `lo`/`hi` are the slopes of the two halves; the map is piecewise linear and
/// continuous, so it is the min (pw < ½) or max (pw > ½) of the two lines: no
/// data-dependent branch.
#[inline]
fn warp(p: f32, pw: f32, (lo, hi): (f32, f32)) -> f32 {
    let (a, b) = (p * lo, 0.5 + (p - pw) * hi);
    // Written as compares (not f32::min/max) so they compile to bare minss/maxss.
    if pw < 0.5 {
        if a < b {
            a
        } else {
            b
        }
    } else if a > b {
        a
    } else {
        b
    }
}

/// The plain (non-band-limited) shapes, phase `p` in `0..1`.
#[inline(always)]
fn shape(w: u8, p: f32) -> f32 {
    match w {
        0 => sin_turns(p),
        1 => {
            if p < 0.5 {
                sin_turns(p)
            } else {
                0.0
            }
        }
        2 => sin_turns(p).abs(),
        3 => {
            let h = if p >= 0.5 { p - 0.5 } else { p };
            if h < 0.25 {
                sin_turns(p).abs()
            } else {
                0.0
            }
        }
        4 => {
            if p < 0.5 {
                sin_turns(2.0 * p)
            } else {
                0.0
            }
        }
        5 => {
            if p < 0.5 {
                sin_turns(2.0 * p).abs()
            } else {
                0.0
            }
        }
        W_TRI => {
            if p < 0.25 {
                4.0 * p
            } else if p < 0.75 {
                2.0 - 4.0 * p
            } else {
                4.0 * p - 4.0
            }
        }
        W_SAW => 2.0 * p - 1.0,
        W_PULSE => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        _ => 0.0,
    }
}

/// Coefficients of a TPT state-variable filter (`k` = 1/Q damping).
#[inline]
fn svf_coeffs(fc: f32, sr: f32, k: f32) -> (f32, f32, f32) {
    svf_coeffs_g((PI * fc / sr).tan(), k)
}

#[inline]
fn svf_coeffs_g(g: f32, k: f32) -> (f32, f32, f32) {
    let a1 = 1.0 / (1.0 + g * (g + k));
    let a2 = g * a1;
    (a1, a2, g * a2)
}

/// One TPT SVF step; returns (band, low).
#[inline]
fn svf(s: &mut (f32, f32), x: f32, (a1, a2, a3): (f32, f32, f32)) -> (f32, f32) {
    let v3 = x - s.1;
    let v1 = a1 * s.0 + a2 * v3;
    let v2 = s.1 + a2 * s.0 + a3 * v3;
    s.0 = 2.0 * v1 - s.0;
    s.1 = 2.0 * v2 - s.1;
    (v1, v2)
}

// ---------------------------------------------------------------- derived patch

/// Envelope stages (operators and the filter envelope).
const OFF: u8 = 0;
const ATTACK: u8 = 1;
const DECAY: u8 = 2;
const SUSTAIN: u8 = 3;
const RELEASE: u8 = 4;

#[derive(Clone, Copy, Default)]
struct OpPrep {
    wave: u8,
    fixed: bool,
    reset: bool,
    ratio: f32,
    hz: f32,
    /// Fine tune as a frequency multiplier.
    mul: f32,
    pw: f32,
    pwm: f32,
    spread: f32,
    vel: f32,
    /// Supersaw centre / side gains and normaliser.
    cen: f32,
    side: f32,
    ssn: f32,
    attack_inc: f32,
    decay_k: f32,
    sustain: f32,
    release_k: f32,
}

/// Everything derived from the parameters, recomputed (allocation-free) when one changes.
#[derive(Clone, Copy, Default)]
struct Prep {
    ops: [OpPrep; OPS],
    /// Per target operator: (source, amount) pairs from its matrix column, self excluded.
    /// Sources that run later (last sample's output) come first, then earlier ones in
    /// order, so the value computed most recently is added last (shortest dependency chain).
    mods: [[(u8, f32); OPS - 1]; OPS],
    n_mods: [usize; OPS],
    /// Self-feedback amount per operator (the matrix diagonal).
    feedback: [f32; OPS],
    /// Operators that can reach the output.
    used: [bool; OPS],
    out: [f32; OPS],
    quick_k: f32,

    lp_on: bool,
    lp24: bool,
    cutoff: f32,
    lp_k: f32,
    f_env: f32,
    keytrack: f32,
    f_lfo: f32,
    f_attack_inc: f32,
    f_decay_k: f32,
    f_sustain: f32,
    f_release_k: f32,
    hp_on: bool,
    hp: (f32, f32, f32),
    hp_k: f32,

    lfo_shape: u8,
    lfo_inc: f32,
    lfo_retrig: bool,
    /// Vibrato depth in semitones.
    vibrato: f32,
    lfo2_shape: u8,
    lfo2_inc: f32,
    lfo2_retrig: bool,
    /// Tempo sync per LFO, in quarter notes per cycle (0 = free).
    lfo_beats: [f32; 2],
    env: [EnvPrep; 2],
    /// The active mod slots: (source, destination, amount).
    slots: [(u8, u8, f32); p::MOD_SLOTS],
    n_slots: usize,

    volume: f32,
    mono: bool,
    glide_k: f32,
    glide_on: bool,
    glide_always: bool,
}

/// A modulation envelope's rates, per control-rate tick.
#[derive(Clone, Copy, Default)]
struct EnvPrep {
    attack_inc: f32,
    decay_k: f32,
    sustain: f32,
    release_k: f32,
}

/// One control tick of an ADSR (`(stage, level)`), shared by the mod envelopes.
fn env_tick(e: &EnvPrep, (st, lv): (u8, f32)) -> (u8, f32) {
    match st {
        ATTACK => {
            let lv = lv + e.attack_inc;
            if lv >= 1.0 {
                (DECAY, 1.0)
            } else {
                (ATTACK, lv)
            }
        }
        DECAY => {
            let lv = e.sustain + (lv - e.sustain) * e.decay_k;
            if lv - e.sustain < 1e-4 {
                (SUSTAIN, e.sustain)
            } else {
                (DECAY, lv)
            }
        }
        RELEASE => {
            let lv = lv * e.release_k;
            if lv < 1e-4 {
                (OFF, 0.0)
            } else {
                (RELEASE, lv)
            }
        }
        _ => (st, lv),
    }
}

/// What the mod slots add up to for one voice at one control tick (all zero = untouched).
#[derive(Default)]
struct Mods {
    pitch: f32,
    cutoff: f32,
    volume: f32,
    all_fm: f32,
    level: [f32; OPS],
    fm_in: [f32; OPS],
    op_pitch: [f32; OPS],
    warp: [f32; OPS],
}

/// Per-sample multiplier that falls by 60 dB (×0.001) over `seconds` (Fluttersine's curve).
fn fall(seconds: f32, samples_per_second: f32) -> f32 {
    (-6.9 / (seconds.max(0.001) * samples_per_second)).exp()
}

impl Prep {
    fn compute(p: &[f32; p::COUNT], sr: f32) -> Self {
        use p::*;
        let mut q = Prep::default();
        for i in 0..OPS {
            let at = |k: usize| p[op(i, k)];
            let mix = at(OP_SS_MIX);
            let cen = -0.55366 * mix + 0.99785;
            let side = -0.73764 * mix * mix + 1.2841 * mix + 0.044372;
            q.ops[i] = OpPrep {
                wave: (at(OP_WAVE).round().max(0.0) as u8).min(W_NOISE),
                fixed: at(OP_FIXED) >= 0.5,
                reset: at(OP_RESET) >= 0.5,
                ratio: at(OP_RATIO),
                hz: at(OP_HZ),
                mul: 2f32.powf(at(OP_FINE) / 1200.0),
                pw: at(OP_PW),
                pwm: at(OP_PWM),
                spread: at(OP_SPREAD),
                vel: at(OP_VEL).clamp(0.0, 1.0),
                cen,
                side,
                ssn: 1.0 / (cen + side * 2.45),
                attack_inc: 1.0 / (at(OP_ATTACK).max(0.0005) * sr),
                decay_k: fall(at(OP_DECAY), sr),
                sustain: at(OP_SUSTAIN).clamp(0.0, 1.0),
                release_k: fall(at(OP_RELEASE), sr),
            };
            q.out[i] = p[out(i)].max(0.0);
        }
        // Matrix: row j modulates column i.
        for i in 0..OPS {
            q.feedback[i] = p[matrix(i, i)].max(0.0);
            for j in (i + 1..OPS).chain(0..i) {
                let amt = p[matrix(j, i)];
                if amt > 0.0 {
                    q.mods[i][q.n_mods[i]] = (j as u8, amt);
                    q.n_mods[i] += 1;
                }
            }
        }
        // An operator is used if it reaches the output directly or through a modulation chain.
        q.used = q.out.map(|o| o > 0.0);
        for _ in 0..OPS {
            for i in 0..OPS {
                if !q.used[i] {
                    q.used[i] = (0..OPS).any(|j| j != i && q.used[j] && p[matrix(i, j)] > 0.0);
                }
            }
        }
        q.quick_k = fall(0.03, sr);

        let csr = sr / CR as f32; // control-rate updates per second
        q.lp_on = p[LP_ON] >= 0.5;
        q.lp24 = p[LP_SLOPE] >= 0.5;
        q.cutoff = p[CUTOFF];
        q.lp_k = 2.0 - 1.98 * p[RESO].clamp(0.0, 1.0);
        q.f_env = p[F_ENV];
        q.keytrack = p[KEYTRACK];
        q.f_lfo = p[F_LFO];
        q.f_attack_inc = 1.0 / (p[F_ATTACK].max(0.0005) * csr);
        q.f_decay_k = fall(p[F_DECAY], csr);
        q.f_sustain = p[F_SUSTAIN].clamp(0.0, 1.0);
        q.f_release_k = fall(p[F_RELEASE], csr);
        q.hp_on = p[HP_ON] >= 0.5;
        q.hp_k = 2.0 - 1.98 * p[HP_RESO].clamp(0.0, 1.0);
        q.hp = svf_coeffs(p[HP_CUTOFF].clamp(10.0, sr * 0.45), sr, q.hp_k);

        q.lfo_shape = p[LFO_SHAPE].round().max(0.0) as u8;
        q.lfo_inc = p[LFO_RATE] / csr;
        q.lfo_retrig = p[LFO_RETRIG] >= 0.5;
        q.vibrato = p[VIBRATO] / 100.0;
        q.lfo2_shape = p[LFO2_SHAPE].round().max(0.0) as u8;
        q.lfo2_inc = p[LFO2_RATE] / csr;
        q.lfo2_retrig = p[LFO2_RETRIG] >= 0.5;
        let beats = |id: usize| SYNC_BEATS[(p[id].round().max(0.0) as usize).min(SYNC_BEATS.len() - 1)];
        q.lfo_beats = [beats(LFO_SYNC), beats(LFO2_SYNC)];
        for e in 0..2 {
            q.env[e] = EnvPrep {
                attack_inc: 1.0 / (p[env(e, ENV_ATTACK)].max(0.0005) * csr),
                decay_k: fall(p[env(e, ENV_DECAY)], csr),
                sustain: p[env(e, ENV_SUSTAIN)].clamp(0.0, 1.0),
                release_k: fall(p[env(e, ENV_RELEASE)], csr),
            };
        }
        for k in 0..MOD_SLOTS {
            let src = p[mod_src(k)].round().clamp(0.0, (MOD_SOURCES.len() - 1) as f32) as u8;
            let dest = p[mod_dest(k)].round().clamp(0.0, (MOD_DESTS.len() - 1) as f32) as u8;
            let amt = p[mod_amt(k)].clamp(-1.0, 1.0);
            if src != 0 && dest != 0 && amt != 0.0 {
                q.slots[q.n_slots] = (src, dest, amt);
                q.n_slots += 1;
            }
        }

        let v = p[VOLUME].max(0.0);
        q.volume = v * v * 0.6;
        q.mono = p[MODE] >= 0.5;
        q.glide_on = p[GLIDE] > 0.0;
        q.glide_k = if q.glide_on { 1.0 - (-3.0 / (p[GLIDE] * csr)).exp() } else { 1.0 };
        q.glide_always = p[GLIDE_MODE] >= 0.5;
        q
    }
}

// ---------------------------------------------------------------- voice

struct Voice {
    active: bool,
    gate: bool,
    /// Fast fade (AllNotesOff).
    quick: bool,
    key: u8,
    /// Note-on order, for stealing the oldest voice.
    serial: u64,
    /// Current pitch (MIDI semitones, gliding towards `target`).
    semi: f32,
    target: f32,

    ph: [f32; OPS],
    /// Supersaw phases, 7 per operator.
    ss: [[f32; 7]; OPS],
    /// Last and second-to-last output per operator (modulation and feedback).
    cur: [f32; OPS],
    fb: [f32; OPS],
    est: [u8; OPS],
    ev: [f32; OPS],
    inc: [f32; OPS],
    pw: [f32; OPS],
    /// Supersaw phase increments, 7 per operator.
    ss_inc: [[f32; 7]; OPS],
    /// Phase-warp slopes for the two halves of the cycle, `(0.5 / pw, 0.5 / (1 - pw))`.
    warp: [(f32, f32); OPS],
    /// Velocity scaling per operator.
    vs: [f32; OPS],

    fst: u8,
    fv: f32,
    lp: (f32, f32, f32),
    bw: (f32, f32, f32),
    s1: (f32, f32),
    s2: (f32, f32),
    sh: (f32, f32),
    rng: Rng,

    /// Mod envelopes: (stage, level), ticked at control rate.
    menv: [(u8, f32); 2],
    /// This note's velocity (0..1) and random value (-1..1), as mod sources.
    vel: f32,
    rnd: f32,
    /// Results of the mod slots, applied per sample: operator level and
    /// FM-depth multipliers, and the voice's gain.
    lvl: [f32; OPS],
    fmx: [f32; OPS],
    gain: f32,
}

impl Voice {
    fn new(seed: u32) -> Self {
        Self {
            active: false,
            gate: false,
            quick: false,
            key: 0,
            serial: 0,
            semi: 60.0,
            target: 60.0,
            ph: [0.0; OPS],
            ss: [[0.0; 7]; OPS],
            cur: [0.0; OPS],
            fb: [0.0; OPS],
            est: [OFF; OPS],
            ev: [0.0; OPS],
            inc: [0.0; OPS],
            pw: [0.5; OPS],
            warp: [(1.0, 1.0); OPS],
            ss_inc: [[0.0; 7]; OPS],
            vs: [1.0; OPS],
            fst: OFF,
            fv: 0.0,
            lp: (0.0, 0.0, 0.0),
            bw: (0.0, 0.0, 0.0),
            s1: (0.0, 0.0),
            s2: (0.0, 0.0),
            sh: (0.0, 0.0),
            rng: Rng::new(seed),
            menv: [(OFF, 0.0); 2],
            vel: 0.0,
            rnd: 0.0,
            lvl: [1.0; OPS],
            fmx: [1.0; OPS],
            gain: 1.0,
        }
    }

    /// Loudness estimate for voice stealing: the strongest audible operator envelope.
    fn loudness(&self, q: &Prep) -> f32 {
        (0..OPS).filter(|&i| q.used[i]).map(|i| self.ev[i] * q.out[i]).fold(0.0, f32::max)
    }

    fn release(&mut self) {
        self.gate = false;
        for st in self.est.iter_mut().filter(|s| **s != OFF) {
            *st = RELEASE;
        }
        if self.fst != OFF {
            self.fst = RELEASE;
        }
        for e in self.menv.iter_mut().filter(|e| e.0 != OFF) {
            e.0 = RELEASE;
        }
    }

    /// Sum the mod slots for this voice right now.
    fn modulate(&self, q: &Prep, lfo: [f32; 2]) -> Mods {
        let mut m = Mods::default();
        for &(src, dest, amt) in &q.slots[..q.n_slots] {
            let s = match src {
                1 => lfo[0],
                2 => lfo[1],
                3 => self.menv[0].1,
                4 => self.menv[1].1,
                5 => self.vel,
                6 => (self.key as f32 - 60.0) / 60.0,
                7 => self.rnd,
                _ => 0.0,
            };
            let x = amt * s;
            let d = dest as usize;
            match d {
                1 => m.pitch += 24.0 * x,
                2 => m.cutoff += 6.0 * x,
                3 => m.volume += x,
                4 => m.all_fm += x,
                _ if d >= DEST_WARP => m.warp[d - DEST_WARP] += 0.48 * x,
                _ if d >= DEST_OP_PITCH => m.op_pitch[d - DEST_OP_PITCH] += 24.0 * x,
                _ if d >= DEST_FM_IN => m.fm_in[d - DEST_FM_IN] += x,
                _ if d >= DEST_LEVEL => m.level[d - DEST_LEVEL] += x,
                _ => {}
            }
        }
        m
    }

    /// Control-rate update: pitch, pulse width, filter envelope and coefficients.
    fn ctrl(&mut self, q: &Prep, lfos: [f32; 2], sr: f32) {
        if self.semi != self.target {
            self.semi += (self.target - self.semi) * q.glide_k;
            if (self.target - self.semi).abs() < 1e-4 {
                self.semi = self.target;
            }
        }
        for (e, prep) in self.menv.iter_mut().zip(&q.env) {
            *e = env_tick(prep, *e);
        }
        // With no slots in use every term below is exactly what it was before
        // the mod slots existed (+ 0.0, × 1.0): old patches sound bit-identical.
        let m = if q.n_slots > 0 { self.modulate(q, lfos) } else { Mods::default() };
        let lfo = lfos[0];
        let f = 440.0 * 2f32.powf((self.semi + lfo * q.vibrato + m.pitch - 69.0) / 12.0);
        let all_fm = (1.0 + 2.0 * m.all_fm).max(0.0);
        self.gain = (1.0 + m.volume).clamp(0.0, 2.0);
        for i in 0..OPS {
            let o = &q.ops[i];
            let hz = if o.fixed { o.hz } else { f * o.ratio };
            let hz = if m.op_pitch[i] != 0.0 { hz * 2f32.powf(m.op_pitch[i] / 12.0) } else { hz };
            self.lvl[i] = (1.0 + m.level[i]).max(0.0);
            self.fmx[i] = all_fm * (1.0 + 2.0 * m.fm_in[i]).max(0.0);
            let inc = (hz * o.mul / sr).min(0.45);
            self.inc[i] = inc;
            if o.wave == W_SSAW {
                self.ss_inc[i] = SSD.map(|d| inc * (1.0 + d * o.spread));
            }
            let pw = (o.pw + o.pwm * lfo * 0.48 + m.warp[i]).clamp(0.02, 0.98);
            self.pw[i] = pw;
            self.warp[i] = (0.5 / pw, 0.5 / (1.0 - pw));
        }
        let mut fv = self.fv;
        match self.fst {
            ATTACK => {
                fv += q.f_attack_inc;
                if fv >= 1.0 {
                    fv = 1.0;
                    self.fst = DECAY;
                }
            }
            DECAY => {
                fv = q.f_sustain + (fv - q.f_sustain) * q.f_decay_k;
                if fv - q.f_sustain < 1e-4 {
                    fv = q.f_sustain;
                    self.fst = SUSTAIN;
                }
            }
            RELEASE => {
                fv *= q.f_release_k;
                if fv < 1e-4 {
                    fv = 0.0;
                    self.fst = OFF;
                }
            }
            _ => {}
        }
        self.fv = fv;
        if q.lp_on {
            let oct = q.f_env * fv + q.keytrack * (self.semi - 60.0) / 12.0 + q.f_lfo * lfo + m.cutoff;
            let fc = (q.cutoff * 2f32.powf(oct)).clamp(16.0, sr * 0.45);
            let g = (PI * fc / sr).tan();
            self.lp = svf_coeffs_g(g, q.lp_k);
            self.bw = svf_coeffs_g(g, std::f32::consts::SQRT_2);
        }
    }

    /// One sample of operator `i`; returns its contribution to the voice output.
    #[inline(always)]
    fn op(&mut self, q: &Prep, i: usize) -> f32 {
        let o = &q.ops[i];
        let mut st = self.est[i];
        let mut lv = self.ev[i];
        match st {
            ATTACK => {
                lv += o.attack_inc;
                if lv >= 1.0 {
                    lv = 1.0;
                    st = DECAY;
                }
            }
            DECAY => {
                lv = o.sustain + (lv - o.sustain) * o.decay_k;
                if lv - o.sustain < 1e-5 {
                    lv = o.sustain;
                    st = SUSTAIN;
                }
            }
            RELEASE => {
                lv *= if self.quick { q.quick_k } else { o.release_k };
                if lv < 1e-5 {
                    lv = 0.0;
                    st = OFF;
                }
            }
            _ => {}
        }
        self.est[i] = st;
        self.ev[i] = lv;
        if st == OFF {
            self.fb[i] = self.cur[i];
            self.cur[i] = 0.0;
            return 0.0;
        }

        let mut m = q.feedback[i] * 0.5 * (self.cur[i] + self.fb[i]);
        for &(j, amt) in &q.mods[i][..q.n_mods[i]] {
            m += amt * self.cur[j as usize];
        }
        m *= self.fmx[i];
        let dt = self.inc[i];
        let mut ph = self.ph[i] + dt;
        if ph >= 1.0 {
            ph -= 1.0;
        }
        self.ph[i] = ph;

        let y = match o.wave {
            W_SSAW => {
                let (ss, inc) = (&mut self.ss[i], &self.ss_inc[i]);
                for (t, d) in ss.iter_mut().zip(inc) {
                    let x = *t + d;
                    *t = if x >= 1.0 { x - 1.0 } else { x };
                }
                // Plain loops (not array::map / from_fn, which don't always inline).
                let mut saws = [0.0f32; 7];
                if m != 0.0 {
                    for (y, t) in saws.iter_mut().zip(ss.iter()) {
                        *y = 2.0 * wrap(t + m) - 1.0;
                    }
                } else {
                    for ((y, t), d) in saws.iter_mut().zip(ss.iter()).zip(inc) {
                        *y = 2.0 * t - 1.0 - poly_blep(*t, *d);
                    }
                }
                let all: f32 = saws.iter().sum();
                (o.side * all + (o.cen - o.side) * saws[3]) * o.ssn
            }
            W_NOISE => self.rng.bipolar(),
            0 if self.pw[i] == 0.5 => sin_turns(ph + m), // plain sine: no warp, no wrap needed
            w => {
                let t = if m != 0.0 { wrap(ph + m) } else { ph };
                let pw = self.pw[i];
                if w == W_PULSE {
                    let mut y = if t < pw { 1.0 } else { -1.0 };
                    if m == 0.0 {
                        let t2 = if t < pw { t - pw + 1.0 } else { t - pw };
                        y += poly_blep(t, dt) - poly_blep(t2, dt);
                    }
                    y
                } else {
                    let y = shape(w, if pw == 0.5 { t } else { warp(t, pw, self.warp[i]) });
                    if w == W_SAW && m == 0.0 {
                        y - poly_blep(t, dt)
                    } else {
                        y
                    }
                }
            }
        };
        let y = y * (lv * self.vs[i] * self.lvl[i]);
        self.fb[i] = self.cur[i];
        self.cur[i] = y;
        q.out[i] * y
    }

    /// The voice's filters, one sample.
    #[inline(always)]
    fn filter(&mut self, q: &Prep, mut y: f32) -> f32 {
        if q.lp_on {
            if q.lp24 {
                y = svf(&mut self.s2, y, self.bw).1;
            }
            y = svf(&mut self.s1, y, self.lp).1;
        }
        if q.hp_on {
            let (band, low) = svf(&mut self.sh, y, q.hp);
            y = y - q.hp_k * band - low;
        }
        y
    }

    /// A voice is done once every operator you can hear has died (released or a
    /// finished pluck), so held-but-silent notes don't burn CPU either.
    fn check_done(&mut self, q: &Prep) {
        let audible = (0..OPS).any(|i| {
            q.used[i] && q.out[i] > 0.0 && self.est[i] != OFF && !(self.est[i] == SUSTAIN && self.ev[i] == 0.0)
        });
        if !audible {
            self.active = false;
            self.gate = false;
        }
    }
}

/// Render `N` voices together and ADD them into `out`. The voices are independent, so
/// stepping them side by side, operator by operator, lets the CPU overlap their long,
/// serial modulation chains instead of waiting on one voice at a time.
fn render_lanes<const N: usize>(q: &Prep, mut lanes: [&mut Voice; N], out: &mut [f32]) {
    for x in out.iter_mut() {
        let mut s = [0.0f32; N];
        for i in 0..OPS {
            if q.used[i] {
                for (v, s) in lanes.iter_mut().zip(&mut s) {
                    *s += v.op(q, i);
                }
            }
        }
        for (v, s) in lanes.iter_mut().zip(s) {
            *x += v.filter(q, s) * v.gain;
        }
    }
    for v in lanes {
        v.check_done(q);
    }
}

/// An LFO's value (-1..1) at phase `ph` (0..1); `sh` is the held random value.
fn lfo_value(shape: u8, ph: f32, sh: f32) -> f32 {
    match shape {
        1 => {
            if ph < 0.5 {
                4.0 * ph - 1.0
            } else {
                3.0 - 4.0 * ph
            }
        }
        2 => 1.0 - 2.0 * ph,
        3 => {
            if ph < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        4 => sh,
        _ => sin_turns(ph),
    }
}

// ---------------------------------------------------------------- plugin

pub struct Fm {
    p: [f32; p::COUNT],
    q: Prep,
    dirty: bool,
    sr: f32,
    voices: [Voice; MAX_VOICES],
    serial: u64,
    rng: Rng,
    /// Mono-mode held keys, oldest first.
    stack: [u8; STACK],
    stack_len: usize,

    lfo_ph: f32,
    lfo: f32,
    sh: f32,
    /// LFO 2 keeps its own random numbers, so it never changes LFO 1's or
    /// the supersaw phases (which share `rng`).
    lfo2_ph: f32,
    lfo2: f32,
    sh2: f32,
    rng2: Rng,
    /// This block's LFO increments per control tick (synced ones follow the tempo).
    lfo_inc: [f32; 2],
    transport: Transport,
    /// Samples until the next control-rate tick.
    ctrl_left: usize,
    dc_x: f32,
    dc_y: f32,
}

impl Default for Fm {
    fn default() -> Self {
        Self::new()
    }
}

impl Fm {
    pub fn new() -> Self {
        let mut p = [0.0; p::COUNT];
        for (v, info) in p.iter_mut().zip(&FM_PARAMS) {
            *v = info.default;
        }
        let sr = 48_000.0;
        Self {
            q: Prep::compute(&p, sr),
            p,
            dirty: false,
            sr,
            voices: std::array::from_fn(|i| Voice::new(0x9e37_79b9 ^ (i as u32 + 1).wrapping_mul(0x85eb_ca6b))),
            serial: 0,
            rng: Rng::new(0xf1_77e5),
            stack: [0; STACK],
            stack_len: 0,
            lfo_ph: 0.0,
            lfo: 0.0,
            sh: 0.0,
            lfo2_ph: 0.0,
            lfo2: 0.0,
            sh2: 0.0,
            rng2: Rng::new(0x2f_10_a2),
            lfo_inc: [0.0; 2],
            transport: Transport { tempo: 120.0, playing: false, position: 0.0 },
            ctrl_left: 0,
            dc_x: 0.0,
            dc_y: 0.0,
        }
    }

    fn pick_voice(&self) -> usize {
        if let Some(i) = self.voices.iter().position(|v| !v.active) {
            return i;
        }
        let quietest_released = (0..MAX_VOICES)
            .filter(|&i| !self.voices[i].gate)
            .min_by(|&a, &b| self.voices[a].loudness(&self.q).total_cmp(&self.voices[b].loudness(&self.q)));
        quietest_released.unwrap_or_else(|| (0..MAX_VOICES).min_by_key(|&i| self.voices[i].serial).unwrap_or(0))
    }

    /// Start (or restart) voice `slot`. `retrig` restarts envelopes; `glide` keeps the pitch sliding.
    fn start(&mut self, slot: usize, key: u8, velocity: f32, retrig: bool, glide: bool) {
        let q = &self.q;
        let v = &mut self.voices[slot];
        let fresh = !v.active;
        v.key = key;
        v.target = key as f32;
        if !glide {
            v.semi = v.target;
        }
        v.serial = self.serial;
        self.serial += 1;
        v.gate = true;
        v.quick = false;
        if fresh {
            v.cur = [0.0; OPS];
            v.fb = [0.0; OPS];
            v.ev = [0.0; OPS];
            v.fv = 0.0;
            v.s1 = (0.0, 0.0);
            v.s2 = (0.0, 0.0);
            v.sh = (0.0, 0.0);
        }
        if retrig {
            for i in 0..OPS {
                let o = &q.ops[i];
                v.vs[i] = 1.0 - o.vel + o.vel * velocity;
                if !q.used[i] {
                    v.est[i] = OFF;
                    v.ev[i] = 0.0;
                    continue;
                }
                v.est[i] = ATTACK;
                if o.reset || fresh {
                    if o.reset {
                        v.ph[i] = 0.0;
                    }
                    for s in v.ss[i].iter_mut() {
                        *s = self.rng.bipolar() * 0.5 + 0.5;
                    }
                }
            }
            v.fst = ATTACK;
            if fresh {
                v.menv = [(ATTACK, 0.0); 2];
            } else {
                v.menv.iter_mut().for_each(|e| e.0 = ATTACK);
            }
            v.vel = velocity;
            // Random per note without drawing from `rng` (that would move the supersaw phases).
            let mut h = (v.serial as u32).wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
            h ^= h >> 15;
            h = h.wrapping_mul(0x2c1b_3c6d);
            h ^= h >> 12;
            v.rnd = (h >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
        }
        v.active = true;
        v.ctrl(q, [self.lfo, self.lfo2], self.sr);
    }

    fn note_on(&mut self, key: u8, velocity: f32) {
        if self.q.lfo_retrig {
            self.lfo_ph = 0.0;
        }
        if self.q.lfo2_retrig {
            self.lfo2_ph = 0.0;
        }
        if self.q.mono {
            for v in self.voices[1..].iter_mut().filter(|v| v.gate) {
                v.release();
            }
            let legato = self.stack_len > 0 && self.voices[0].gate;
            self.stack_remove(key);
            if self.stack_len == STACK {
                self.stack.copy_within(1.., 0);
                self.stack_len -= 1;
            }
            self.stack[self.stack_len] = key;
            self.stack_len += 1;
            let glide = self.q.glide_on && self.voices[0].active && (legato || self.q.glide_always);
            self.start(0, key, velocity, !legato, glide);
        } else {
            let slot = self.pick_voice();
            self.start(slot, key, velocity, true, false);
        }
    }

    fn note_off(&mut self, key: u8) {
        if self.q.mono {
            let top = self.stack_len > 0 && self.stack[self.stack_len - 1] == key;
            self.stack_remove(key);
            let v = &mut self.voices[0];
            if self.stack_len == 0 {
                if v.gate {
                    v.release();
                }
            } else if top {
                // Fall back to the previous held key, legato.
                v.key = self.stack[self.stack_len - 1];
                v.target = v.key as f32;
                if !self.q.glide_on {
                    v.semi = v.target;
                }
            }
        }
        // Poly voices, and in mono mode any voices left over from before the switch.
        let first = if self.q.mono { 1 } else { 0 };
        for v in self.voices[first..].iter_mut().filter(|v| v.active && v.gate && v.key == key) {
            v.release();
        }
    }

    fn stack_remove(&mut self, key: u8) {
        let mut w = 0;
        for r in 0..self.stack_len {
            if self.stack[r] != key {
                self.stack[w] = self.stack[r];
                w += 1;
            }
        }
        self.stack_len = w;
    }

    fn handle(&mut self, e: Event) {
        match e {
            Event::NoteOn { key, velocity } => self.note_on(key, velocity.clamp(0.0, 1.0)),
            Event::NoteOff { key } => self.note_off(key),
            Event::AllNotesOff => {
                self.stack_len = 0;
                for v in self.voices.iter_mut().filter(|v| v.active) {
                    v.release();
                    v.quick = true;
                }
            }
        }
    }

    fn lfo_tick(&mut self) {
        let mut ph = self.lfo_ph + self.lfo_inc[0];
        if ph >= 1.0 {
            ph = wrap(ph);
            self.sh = self.rng.bipolar();
        }
        self.lfo_ph = ph;
        self.lfo = lfo_value(self.q.lfo_shape, ph, self.sh);

        let mut ph = self.lfo2_ph + self.lfo_inc[1];
        if ph >= 1.0 {
            ph = wrap(ph);
            self.sh2 = self.rng2.bipolar();
        }
        self.lfo2_ph = ph;
        self.lfo2 = lfo_value(self.q.lfo2_shape, ph, self.sh2);
    }

    /// Set this block's LFO speeds. A synced LFO runs at its note length at
    /// the song's tempo and, while the song plays (and it isn't retriggered
    /// per note), stays locked to the song position, so a 1/8 wobble lands
    /// on the 8ths wherever playback starts.
    fn sync_lfos(&mut self) {
        let csr = self.sr / CR as f32;
        let t = self.transport;
        let free = [self.q.lfo_inc, self.q.lfo2_inc];
        let retrig = [self.q.lfo_retrig, self.q.lfo2_retrig];
        for k in 0..2 {
            let beats = self.q.lfo_beats[k];
            if beats <= 0.0 {
                self.lfo_inc[k] = free[k];
                continue;
            }
            let hz = t.tempo.max(1.0) as f32 / 60.0 / beats;
            self.lfo_inc[k] = hz / csr;
            if t.playing && !retrig[k] {
                let cycle = 96.0 * beats as f64; // ticks per cycle (96 per quarter)
                let ph = (t.position / cycle).fract() as f32;
                if k == 0 {
                    self.lfo_ph = ph;
                } else {
                    self.lfo2_ph = ph;
                }
            }
        }
    }

    #[cfg(test)]
    fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }
}

impl Plugin for Fm {
    fn name(&self) -> &'static str {
        "Fluttersine"
    }
    fn params(&self) -> &'static [ParamInfo] {
        &FM_PARAMS
    }
    fn groups(&self) -> &'static [ParamGroup] {
        &FM_GROUPS
    }
    fn set_param(&mut self, id: u32, value: f32) {
        if let Some(p) = self.p.get_mut(id as usize) {
            if *p != value {
                *p = value;
                self.dirty = true;
            }
        }
    }
    fn transport(&mut self, transport: Transport) {
        self.transport = transport;
    }
    fn activate(&mut self, sample_rate: f32) {
        self.sr = sample_rate;
        self.q = Prep::compute(&self.p, sample_rate);
        self.dirty = false;
    }
    fn process(&mut self, ctx: ProcessContext<'_>) {
        let ProcessContext { events, out_l, out_r, .. } = ctx;
        if self.dirty {
            self.q = Prep::compute(&self.p, self.sr);
            self.dirty = false;
        }
        self.sync_lfos();
        let n = out_l.len().min(out_r.len());
        let mut next = 0;
        let mut i = 0;
        let mut mix = [0.0f32; CR];
        while i < n {
            while next < events.len() && events[next].offset as usize <= i {
                self.handle(events[next].event);
                next += 1;
            }
            if self.ctrl_left == 0 {
                self.lfo_tick();
                let (q, lfo, sr) = (&self.q, [self.lfo, self.lfo2], self.sr);
                for v in self.voices.iter_mut().filter(|v| v.active) {
                    v.ctrl(q, lfo, sr);
                }
                self.ctrl_left = CR;
            }
            let mut end = (i + self.ctrl_left).min(n);
            if let Some(e) = events.get(next) {
                end = end.min((e.offset as usize).max(i + 1));
            }
            let len = end - i;
            let buf = &mut mix[..len];
            buf.fill(0.0);
            let q = &self.q;
            let mut group: [Option<&mut Voice>; LANES] = Default::default();
            let mut n_group = 0;
            for v in self.voices.iter_mut().filter(|v| v.active) {
                group[n_group] = Some(v);
                n_group += 1;
                if n_group == LANES {
                    render_lanes(q, group.each_mut().map(|v| v.take().unwrap()), buf);
                    n_group = 0;
                }
            }
            for v in group.iter_mut().filter_map(Option::take) {
                render_lanes(q, [v], buf);
            }
            for (k, &m) in buf.iter().enumerate() {
                let x = m * q.volume;
                let y = x - self.dc_x + 0.9995 * self.dc_y;
                self.dc_x = x;
                self.dc_y = y;
                let y = y.tanh();
                out_l[i + k] = y;
                out_r[i + k] = y;
            }
            self.ctrl_left -= len;
            i = end;
        }
        for e in &events[next..] {
            self.handle(e.event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyra_plugin::TimedEvent;

    const SR: f32 = 48_000.0;

    /// Render `seconds` of audio in 256-frame blocks; `events` go in the first block.
    fn render(p: &mut Fm, events: &[TimedEvent], seconds: f32) -> Vec<f32> {
        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        let mut out = Vec::new();
        let mut first = true;
        while (out.len() as f32) < seconds * SR {
            let ev = if first { events } else { &[] };
            first = false;
            p.process(ProcessContext { sample_rate: SR, events: ev, out_l: &mut l, out_r: &mut r });
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

    fn with(values: &[(usize, f32)]) -> Fm {
        let mut fm = Fm::new();
        for &(i, v) in values {
            fm.set_param(i as u32, v);
        }
        fm.activate(SR);
        fm
    }

    #[test]
    fn params_and_groups_line_up() {
        for (i, info) in FM_PARAMS.iter().enumerate() {
            assert_eq!(info.id as usize, i);
            assert!(!info.name.is_empty(), "param {i} has no name");
            assert!(info.min < info.max && (info.min..=info.max).contains(&info.default), "{}", info.name);
        }
        let mut next = 0;
        for g in &FM_GROUPS {
            assert_eq!(g.ids.start, next, "{} does not follow on", g.name);
            next = g.ids.end;
        }
        assert_eq!(next as usize, p::COUNT);
        for (name, values) in FM_PRESETS {
            for &(i, v) in *values {
                let info = &FM_PARAMS[i];
                assert!((info.min..=info.max).contains(&v), "{name}: {} = {v} out of range", info.name);
            }
        }
    }

    /// Index of a sync choice by its label.
    fn sync(label: &str) -> f32 {
        SYNC_LABELS.iter().position(|l| *l == label).unwrap() as f32
    }

    #[test]
    fn a_synced_lfo_follows_the_tempo() {
        let mut fm = with(&[(p::LFO_SYNC, sync("1/4"))]);
        fm.transport(Transport { tempo: 120.0, playing: false, position: 0.0 });
        render(&mut fm, &[], 0.25); // a quarter note at 120 is 0.5 s: half a cycle
        assert!((fm.lfo_ph - 0.5).abs() < 0.01, "phase {}", fm.lfo_ph);
        fm.transport(Transport { tempo: 60.0, playing: false, position: 0.0 });
        render(&mut fm, &[], 0.25); // at 60 it takes a full second
        assert!((fm.lfo_ph - 0.75).abs() < 0.01, "phase {}", fm.lfo_ph);
    }

    #[test]
    fn a_synced_lfo_locks_to_the_song_while_playing() {
        let mut fm = with(&[(p::LFO2_SYNC, sync("1/8"))]);
        // 1/8 = 48 ticks per cycle; at tick 60 the cycle is a quarter through.
        fm.transport(Transport { tempo: 140.0, playing: true, position: 60.0 });
        fm.process(ProcessContext { sample_rate: SR, events: &[], out_l: &mut [0.0; 1], out_r: &mut [0.0; 1] });
        assert!((fm.lfo2_ph - 0.25).abs() < 0.01, "phase {}", fm.lfo2_ph);
    }

    #[test]
    fn an_envelope_on_pitch_falls_an_octave() {
        use p::*;
        let mut fm = with(&[
            (env(0, ENV_DECAY), 0.4),
            (env(0, ENV_SUSTAIN), 0.0),
            (mod_src(0), 3.0),  // Env 1
            (mod_dest(0), 1.0), // Pitch
            (mod_amt(0), 0.5),  // +12 st at the envelope's peak
        ]);
        render(&mut fm, &[on(69)], 0.004);
        let early = fm.voices[0].inc[0] * SR;
        render(&mut fm, &[], 1.0);
        let late = fm.voices[0].inc[0] * SR;
        assert!(early > 800.0 && (late - 440.0).abs() < 1.0, "{early} Hz, then {late} Hz");
    }

    #[test]
    fn fm_in_scales_what_the_matrix_sends() {
        use p::*;
        let loud = TimedEvent { offset: 0, event: Event::NoteOn { key: 60, velocity: 1.0 } };
        let plain = render(&mut with(&[(op(0, OP_VEL), 0.0)]), &[loud], 0.2);
        // Op 2 modulates op 1, but velocity × -0.5 on "Op 1 FM in" scales that to zero.
        let cancelled = render(
            &mut with(&[
                (op(0, OP_VEL), 0.0),
                (matrix(1, 0), 0.6),
                (mod_src(0), 5.0),                // Velocity
                (mod_dest(0), DEST_FM_IN as f32), // Op 1 FM in
                (mod_amt(0), -0.5),
            ]),
            &[loud],
            0.2,
        );
        assert!(plain.iter().zip(&cancelled).all(|(a, b)| (a - b).abs() < 1e-6));
        // And with it doubled instead, the matrix does change the sound.
        let doubled = render(
            &mut with(&[
                (op(0, OP_VEL), 0.0),
                (matrix(1, 0), 0.6),
                (mod_src(0), 5.0),
                (mod_dest(0), DEST_FM_IN as f32),
                (mod_amt(0), 0.5),
            ]),
            &[loud],
            0.2,
        );
        assert!(plain.iter().zip(&doubled).any(|(a, b)| (a - b).abs() > 0.1));
    }

    #[test]
    fn an_lfo_on_volume_gates_the_sound() {
        use p::*;
        // A square LFO at 1/8 on Volume, amount -1: off half the time, on the other half.
        let mut fm = with(&[
            (LFO_SHAPE, 3.0),
            (LFO_SYNC, sync("1/8")),
            (mod_src(0), 1.0),
            (mod_dest(0), 3.0),
            (mod_amt(0), -1.0),
        ]);
        fm.transport(Transport { tempo: 120.0, playing: false, position: 0.0 });
        let out = render(&mut fm, &[on(60)], 1.0);
        let quarter = (0.25 * SR) as usize; // one 1/8 cycle at 120
        let half =
            |k: usize| out[k * quarter / 2 + 200..(k + 1) * quarter / 2 - 200].iter().fold(0.0f32, |m, x| m.max(*x));
        assert!(half(0) < 1e-3 && half(1) > 0.1, "first half {}, second half {}", half(0), half(1));
    }

    #[test]
    fn sine_poly_is_accurate() {
        for k in -4000..4000 {
            let x = k as f32 / 1000.0;
            assert!((sin_turns(x) - (x * TAU).sin()).abs() < 2e-6, "x = {x}");
        }
    }

    #[test]
    fn every_preset_plays_a_chord_and_falls_silent() {
        for (name, values) in FM_PRESETS {
            let mut fm = with(values);
            let longest_release = (0..OPS).map(|i| fm.p[p::op(i, p::OP_RELEASE)]).fold(0.0f32, f32::max);
            let tail_s = if longest_release > 1.5 { 4.0 + 2.0 * longest_release } else { 4.0 };
            let chord = [57, 60, 64, 67];
            let held = render(&mut fm, &chord.map(on), 0.6);
            let tail = render(&mut fm, &chord.map(off), tail_s);
            let all = || held.iter().chain(&tail);
            assert!(all().all(|x| x.is_finite()), "{name}: NaN/inf");
            let peak = all().fold(0.0f32, |m, x| m.max(*x));
            assert!(peak > 0.02 && peak < 1.6, "{name}: peak {peak}");
            let end = tail[tail.len() - 4800..].iter().fold(0.0f32, |m, x| m.max(*x));
            assert!(end < 1e-3, "{name}: still sounding after release ({end})");
            assert_eq!(fm.active_voices(), 0, "{name}: voices never freed");
            println!("{name:28} peak {peak:.2}  tail {tail_s:.0} s");
        }
    }

    #[test]
    fn matrix_really_modulates() {
        use p::*;
        let alone = render(&mut with(&[]), &[on(60)], 0.3);
        let fm = render(&mut with(&[(matrix(1, 0), 0.5)]), &[on(60)], 0.3);
        let diff = alone.iter().zip(&fm).fold(0.0f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(diff > 0.1, "op 2 > op 1 changed nothing (max diff {diff})");
        // Op 2 on its own isn't routed to the output, so it must stay silent...
        let silent = render(&mut with(&[(out(0), 0.0), (out(1), 0.0), (matrix(1, 0), 0.5)]), &[on(60)], 0.3);
        assert!(silent.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn steals_when_full() {
        let mut fm = with(&[]);
        let keys: Vec<TimedEvent> = (40..60).map(on).collect(); // 20 notes, 16 voices
        let out = render(&mut fm, &keys, 0.2);
        assert!(out.iter().all(|x| x.is_finite()));
        assert_eq!(fm.active_voices(), MAX_VOICES);
        // The 4 oldest notes were stolen; the newest are all still playing.
        assert!((44..60).all(|k| fm.voices.iter().any(|v| v.active && v.key == k)));
    }

    /// Seconds of CPU per second of audio for 16 held notes: best of three 1-second runs
    /// (the minimum filters out the scheduler; the CPU's clock still varies a lot).
    fn cost_of(values: &[(usize, f32)]) -> f64 {
        let mut fm = with(values);
        let keys: Vec<TimedEvent> = (48..64).map(on).collect();
        render(&mut fm, &keys, 0.3); // warm up
        assert_eq!(fm.active_voices(), MAX_VOICES);
        (0..3)
            .map(|_| {
                let t = std::time::Instant::now();
                render(&mut fm, &[], 1.0);
                t.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    }

    #[test]
    fn worst_case_cost() {
        use p::*;
        // Every operator audible and modulating every other one plus itself, PWM (phase
        // warp) on, both filters, vibrato and filter modulation: the most expensive
        // patch there is, first with sines, then with 7-voice supersaws.
        let mut values = vec![(LP_ON, 1.0), (HP_ON, 1.0), (F_ENV, 2.0), (F_LFO, 1.0), (VIBRATO, 20.0)];
        for i in 0..OPS {
            values.push((op(i, OP_RATIO), (i + 1) as f32));
            values.push((op(i, OP_PWM), 0.3));
            values.push((out(i), 0.3));
            for j in 0..OPS {
                values.push((matrix(i, j), 0.05));
            }
        }
        let sines = cost_of(&values);
        println!("16 notes × 6 sine ops, full matrix:     {:.1}% of one core (realtime = 100%)", sines * 100.0);
        values.extend((0..OPS).map(|i| (op(i, OP_WAVE), W_SSAW as f32)));
        let saws = cost_of(&values);
        println!("16 notes × 6 supersaw ops, full matrix: {:.1}% of one core", saws * 100.0);
        let limit = if cfg!(debug_assertions) { 2.0 } else { 0.25 };
        assert!(sines < limit && saws < limit);
    }
}
