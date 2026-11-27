//! Chrysalis's parameters: ids, names, ranges, and the editor's groups.
//!
//! Everything is a parameter, the drawn LFO shapes too (32 points × x, y,
//! bend per LFO). That keeps the plugin CLAP-shaped (its whole state is its
//! parameters), so undo, saving, presets and automation need nothing extra.
//! The one thing that isn't a number, an imported WAV table, travels as the
//! channel's sample (`Plugin::set_sample`).

use crate::curve::MAX_POINTS;
use crate::table::TABLE_NAMES;
use lyra_dsp::SYNC_LABELS;
use lyra_plugin::{Curve, ParamGroup, ParamInfo};
use std::sync::LazyLock;

/// Parameter ids. `osc(i, POS)` is oscillator `i`'s position (i = 0, 1),
/// `point_y(l, k)` is LFO `l`'s point `k` value, and so on.
pub mod p {
    use crate::curve::MAX_POINTS;

    pub const OSCS: usize = 2;
    pub const OSC_ON: usize = 0;
    pub const TABLE: usize = 1;
    /// Position in the table: 0 = first frame, 1 = last.
    pub const POS: usize = 2;
    pub const OCTAVE: usize = 3;
    pub const SEMI: usize = 4;
    pub const FINE: usize = 5;
    pub const LEVEL: usize = 6;
    pub const PAN: usize = 7;
    pub const WARP_MODE: usize = 8;
    pub const WARP: usize = 9;
    /// Unison voices: the value is the count − 1 (a choice).
    pub const UNISON: usize = 10;
    pub const DETUNE: usize = 11;
    /// Side voices' level against the centre one.
    pub const BLEND: usize = 12;
    /// How far apart the unison voices are spread, left to right.
    pub const SPREAD: usize = 13;
    /// Start phase on every note, and how much of it is random.
    pub const PHASE: usize = 14;
    pub const RAND: usize = 15;
    pub const OSC_STRIDE: usize = 16;
    pub const fn osc(i: usize, k: usize) -> usize {
        i * OSC_STRIDE + k
    }

    pub const SUB_ON: usize = OSCS * OSC_STRIDE;
    pub const SUB_SHAPE: usize = SUB_ON + 1;
    pub const SUB_OCT: usize = SUB_ON + 2;
    pub const SUB_LEVEL: usize = SUB_ON + 3;
    pub const NOISE_ON: usize = SUB_ON + 4;
    /// −1 dark … 0 white … 1 bright.
    pub const NOISE_COLOR: usize = SUB_ON + 5;
    pub const NOISE_LEVEL: usize = SUB_ON + 6;
    /// Per-voice distortion, before the filters.
    pub const DIST_MODE: usize = SUB_ON + 7;
    pub const DIST_DRIVE: usize = SUB_ON + 8;
    pub const DIST_MIX: usize = SUB_ON + 9;

    pub const FILTERS: usize = SUB_ON + 10;
    pub const F_ON: usize = 0;
    pub const F_TYPE: usize = 1;
    /// Cutoff; on a comb its pitch, on the formant filter a shift (1 kHz = none).
    pub const F_CUTOFF: usize = 2;
    pub const F_RESO: usize = 3;
    pub const F_DRIVE: usize = 4;
    pub const F_KEY: usize = 5;
    /// Formant filter: A (0) … E … I … O … U (1).
    pub const F_VOWEL: usize = 6;
    pub const F_MIX: usize = 7;
    pub const F_STRIDE: usize = 8;
    pub const fn filter(i: usize, k: usize) -> usize {
        FILTERS + i * F_STRIDE + k
    }
    /// 0 = filter 1 into filter 2, 1 = side by side.
    pub const ROUTING: usize = FILTERS + 2 * F_STRIDE;

    /// Four envelopes; the first is the amp envelope.
    pub const ENVS: usize = ROUTING + 1;
    pub const ENV_COUNT: usize = 4;
    pub const E_ATTACK: usize = 0;
    pub const E_DECAY: usize = 1;
    pub const E_SUSTAIN: usize = 2;
    pub const E_RELEASE: usize = 3;
    /// Bends (−1..1) of the attack, decay and release.
    pub const E_A_CURVE: usize = 4;
    pub const E_D_CURVE: usize = 5;
    pub const E_R_CURVE: usize = 6;
    pub const E_STRIDE: usize = 7;
    pub const fn env(i: usize, k: usize) -> usize {
        ENVS + i * E_STRIDE + k
    }

    pub const LFOS: usize = ENVS + ENV_COUNT * E_STRIDE;
    pub const LFO_COUNT: usize = 4;
    pub const L_RATE: usize = 0;
    /// A `SYNC_LABELS` index: 0 = free (use the rate).
    pub const L_SYNC: usize = 1;
    /// Free (shared, song-locked when synced) / Retrig (per note) / One-shot /
    /// Phrase (shared, runs while notes sound, restarts after silence).
    pub const L_MODE: usize = 2;
    /// How many of the points are in use (1..32).
    pub const L_POINTS: usize = 3;
    pub const L_POINT: usize = 4;
    pub const L_STRIDE: usize = L_POINT + 3 * MAX_POINTS;
    pub const fn lfo(i: usize, k: usize) -> usize {
        LFOS + i * L_STRIDE + k
    }
    pub const fn point_x(l: usize, k: usize) -> usize {
        lfo(l, L_POINT + 3 * k)
    }
    pub const fn point_y(l: usize, k: usize) -> usize {
        lfo(l, L_POINT + 3 * k + 1)
    }
    pub const fn point_bend(l: usize, k: usize) -> usize {
        lfo(l, L_POINT + 3 * k + 2)
    }

    /// The mod matrix: each slot sends a source to a destination by a
    /// bipolar amount, optionally scaled by a second source ("via").
    pub const SLOTS: usize = LFOS + LFO_COUNT * L_STRIDE;
    pub const SLOT_COUNT: usize = 16;
    pub const S_SRC: usize = 0;
    pub const S_DEST: usize = 1;
    pub const S_AMT: usize = 2;
    pub const S_VIA: usize = 3;
    /// 0 = the source as it is (0..1), 1 = centred (−1..1).
    pub const S_BIPOLAR: usize = 4;
    pub const S_STRIDE: usize = 5;
    pub const fn slot(i: usize, k: usize) -> usize {
        SLOTS + i * S_STRIDE + k
    }

    /// Four macro knobs: sources for the matrix.
    pub const MACROS: usize = SLOTS + SLOT_COUNT * S_STRIDE;
    pub const fn mac(i: usize) -> usize {
        MACROS + i
    }

    pub const VOLUME: usize = MACROS + 4;
    /// How much note velocity sets the loudness (1 = fully).
    pub const AMP_VEL: usize = VOLUME + 1;
    /// 0 = poly, 1 = mono.
    pub const MODE: usize = VOLUME + 2;
    pub const GLIDE: usize = VOLUME + 3;
    /// 0 = glide between overlapping notes only, 1 = always.
    pub const GLIDE_MODE: usize = VOLUME + 4;

    // ── Added after the first release (ids continue at the end) ──
    /// Each LFO's style: its drawn shape, random steps, or smooth random.
    pub const LFO_STYLE: usize = GLIDE_MODE + 1;
    pub const fn lfo_style(l: usize) -> usize {
        LFO_STYLE + l
    }

    pub const COUNT: usize = LFO_STYLE + LFO_COUNT;
}

pub const OFF_ON: [&str; 2] = ["Off", "On"];
pub const OCTAVES: [&str; 7] = ["−3", "−2", "−1", "0", "+1", "+2", "+3"];
pub const WARP_MODES: [&str; 8] = ["Off", "Bend +", "Bend −", "Sync", "Mirror", "Quantize", "FM", "PWM"];
/// Warp modes by name, for the engine.
pub mod warp {
    pub const OFF: u8 = 0;
    pub const BEND_UP: u8 = 1;
    pub const BEND_DOWN: u8 = 2;
    pub const SYNC: u8 = 3;
    pub const MIRROR: u8 = 4;
    pub const QUANTIZE: u8 = 5;
    pub const FM: u8 = 6;
    pub const PWM: u8 = 7;
}
pub const UNISON_LABELS: [&str; 16] =
    ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16"];
pub const SUB_SHAPES: [&str; 2] = ["Sine", "Square"];
pub const SUB_OCTAVES: [&str; 2] = ["−1 oct", "−2 oct"];
pub const DIST_MODES: [&str; 5] = ["Off", "Tanh", "Hard", "Fold", "Crush"];
pub const FILTER_TYPES: [&str; 8] = ["Low 12", "Low 24", "High 12", "Band", "Notch", "Formant", "Comb +", "Comb −"];
/// Filter types by name, for the engine.
pub mod ft {
    pub const LOW12: u8 = 0;
    pub const LOW24: u8 = 1;
    pub const HIGH12: u8 = 2;
    pub const BAND: u8 = 3;
    pub const NOTCH: u8 = 4;
    pub const FORMANT: u8 = 5;
    pub const COMB_PLUS: u8 = 6;
    pub const COMB_MINUS: u8 = 7;
}
pub const ROUTINGS: [&str; 2] = ["Serial", "Parallel"];
pub const LFO_MODES: [&str; 4] = ["Free", "Retrig", "One-shot", "Phrase"];
/// LFO modes by name, for the engine.
pub mod lfo_mode {
    pub const FREE: u8 = 0;
    pub const RETRIG: u8 = 1;
    pub const ONE_SHOT: u8 = 2;
    pub const PHRASE: u8 = 3;
    /// One LFO for every voice (free and phrase), rather than one per note.
    pub const fn shared(mode: u8) -> bool {
        mode == FREE || mode == PHRASE
    }
}
pub const LFO_STYLES: [&str; 3] = ["Drawn", "Steps", "Smooth"];
pub const POLARITIES: [&str; 2] = ["Uni", "Bi"];
pub const VOICE_MODES: [&str; 2] = ["Poly", "Mono"];
pub const GLIDE_MODES: [&str; 2] = ["Legato", "Always"];

/// What a mod slot listens to. Every source is 0..1 (a slot's Bi switch
/// centres it): LFOs as drawn, envelopes, velocity, key (C-1 … G9),
/// random (new on every note), macros.
pub const SOURCES: [&str; 16] = [
    "Off", "LFO 1", "LFO 2", "LFO 3", "LFO 4", "Amp env", "Env 2", "Env 3", "Env 4", "Velocity", "Key", "Random",
    "Macro 1", "Macro 2", "Macro 3", "Macro 4",
];
/// Source indices.
pub mod src {
    pub const LFO: usize = 1;
    pub const ENV: usize = 5;
    pub const VELOCITY: usize = 9;
    pub const KEY: usize = 10;
    pub const RANDOM: usize = 11;
    pub const MACRO: usize = 12;
}

/// Where a slot goes, and what an amount of 1 at a full source does:
/// pitch ±24 st; cutoff ±6 oct; LFO rate ×/÷ 16; levels and volume ±100 %
/// (of themselves); everything else its whole range.
#[rustfmt::skip]
pub const DESTS: [&str; 37] = [
    "Off", "Pitch", "Volume", "Pan",
    "Osc 1 position", "Osc 1 pitch", "Osc 1 level", "Osc 1 pan", "Osc 1 warp", "Osc 1 detune", "Osc 1 blend",
    "Osc 2 position", "Osc 2 pitch", "Osc 2 level", "Osc 2 pan", "Osc 2 warp", "Osc 2 detune", "Osc 2 blend",
    "Sub level", "Noise level", "Noise colour", "Drive", "Drive mix",
    "Filter 1 cutoff", "Filter 1 reso", "Filter 1 drive", "Filter 1 vowel", "Filter 1 mix",
    "Filter 2 cutoff", "Filter 2 reso", "Filter 2 drive", "Filter 2 vowel", "Filter 2 mix",
    "LFO 1 rate", "LFO 2 rate", "LFO 3 rate", "LFO 4 rate",
];
/// Destination indices.
pub mod dest {
    pub const COUNT: usize = 37;
    pub const PITCH: usize = 1;
    pub const VOLUME: usize = 2;
    pub const PAN: usize = 3;
    /// Oscillator `i`'s block starts at `OSC + i * OSC_STRIDE`.
    pub const OSC: usize = 4;
    pub const OSC_STRIDE: usize = 7;
    pub const O_POS: usize = 0;
    pub const O_PITCH: usize = 1;
    pub const O_LEVEL: usize = 2;
    pub const O_PAN: usize = 3;
    pub const O_WARP: usize = 4;
    pub const O_DETUNE: usize = 5;
    pub const O_BLEND: usize = 6;
    pub const fn osc(i: usize, k: usize) -> usize {
        OSC + i * OSC_STRIDE + k
    }
    pub const SUB_LEVEL: usize = 18;
    pub const NOISE_LEVEL: usize = 19;
    pub const NOISE_COLOR: usize = 20;
    pub const DRIVE: usize = 21;
    pub const DRIVE_MIX: usize = 22;
    pub const FILTER: usize = 23;
    pub const F_STRIDE: usize = 5;
    pub const F_CUTOFF: usize = 0;
    pub const F_RESO: usize = 1;
    pub const F_DRIVE: usize = 2;
    pub const F_VOWEL: usize = 3;
    pub const F_MIX: usize = 4;
    pub const fn filter(i: usize, k: usize) -> usize {
        FILTER + i * F_STRIDE + k
    }
    pub const LFO_RATE: usize = 33;
}

use Curve::{Exponential as Exp, Linear as Lin};

/// Names live for the whole run (they are built once, here).
fn name(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// The parameter table, in id order.
pub static PARAMS: LazyLock<Vec<ParamInfo>> = LazyLock::new(build);

fn build() -> Vec<ParamInfo> {
    use p::*;
    let mut a: Vec<Option<ParamInfo>> = vec![None; COUNT];
    let mut new = |i: usize, n: &'static str, min: f32, max: f32, def: f32, unit: &'static str, c: Curve| {
        a[i] = Some(ParamInfo::new(i as u32, n, min, max, def, unit, c));
    };
    for o in 0..OSCS {
        let n = |what: &str| name(format!("Osc {} {what}", o + 1));
        new(osc(o, POS), n("position"), 0.0, 1.0, 2.0 / 3.0, "%", Lin);
        new(osc(o, SEMI), n("semi"), -24.0, 24.0, 0.0, "st", Lin);
        new(osc(o, FINE), n("fine"), -100.0, 100.0, 0.0, "ct", Lin);
        new(osc(o, LEVEL), n("level"), 0.0, 1.0, 0.7, "gain", Curve::Fader);
        new(osc(o, PAN), n("pan"), -1.0, 1.0, 0.0, "pan", Lin);
        new(osc(o, WARP), n("warp"), 0.0, 1.0, 0.0, "%", Lin);
        new(osc(o, DETUNE), n("detune"), 0.0, 1.0, 0.18, "%", Lin);
        new(osc(o, BLEND), n("blend"), 0.0, 1.0, 0.75, "%", Lin);
        new(osc(o, SPREAD), n("spread"), 0.0, 1.0, 0.7, "%", Lin);
        new(osc(o, PHASE), n("phase"), 0.0, 1.0, 0.0, "%", Lin);
        new(osc(o, RAND), n("random phase"), 0.0, 1.0, 1.0, "%", Lin);
    }
    new(SUB_LEVEL, "Sub level", 0.0, 1.0, 0.5, "gain", Curve::Fader);
    new(NOISE_COLOR, "Noise colour", -1.0, 1.0, 0.0, "", Lin);
    new(NOISE_LEVEL, "Noise level", 0.0, 1.0, 0.25, "gain", Curve::Fader);
    new(DIST_DRIVE, "Drive", 0.0, 1.0, 0.3, "%", Lin);
    new(DIST_MIX, "Drive mix", 0.0, 1.0, 1.0, "%", Lin);
    for f in 0..2 {
        let n = |what: &str| name(format!("Filter {} {what}", f + 1));
        let cutoff = if f == 0 { 2000.0 } else { 80.0 };
        new(filter(f, F_CUTOFF), n("cutoff"), 20.0, 20_000.0, cutoff, "Hz", Exp);
        new(filter(f, F_RESO), n("reso"), 0.0, 1.0, 0.2, "%", Lin);
        new(filter(f, F_DRIVE), n("drive"), 0.0, 1.0, 0.0, "%", Lin);
        new(filter(f, F_KEY), n("key track"), 0.0, 1.0, 0.0, "%", Lin);
        new(filter(f, F_VOWEL), n("vowel"), 0.0, 1.0, 0.0, "%", Lin);
        new(filter(f, F_MIX), n("mix"), 0.0, 1.0, 1.0, "%", Lin);
    }
    for e in 0..ENV_COUNT {
        let n = |what: &str| if e == 0 { name(format!("Amp {what}")) } else { name(format!("Env {} {what}", e + 1)) };
        let (d, s, r) = if e == 0 { (0.6, 0.8, 0.2) } else { (0.4, 0.0, 0.3) };
        new(env(e, E_ATTACK), n("attack"), 0.001, 10.0, 0.003, "s", Exp);
        new(env(e, E_DECAY), n("decay"), 0.001, 10.0, d, "s", Exp);
        new(env(e, E_SUSTAIN), n("sustain"), 0.0, 1.0, s, "%", Lin);
        new(env(e, E_RELEASE), n("release"), 0.001, 10.0, r, "s", Exp);
        new(env(e, E_A_CURVE), n("attack curve"), -1.0, 1.0, 0.0, "", Lin);
        new(env(e, E_D_CURVE), n("decay curve"), -1.0, 1.0, 0.5, "", Lin);
        new(env(e, E_R_CURVE), n("release curve"), -1.0, 1.0, 0.5, "", Lin);
    }
    for l in 0..LFO_COUNT {
        let n = |what: String| name(format!("LFO {} {what}", l + 1));
        new(lfo(l, L_RATE), n("rate".into()), 0.01, 30.0, 2.0, "Hz", Exp);
        new(lfo(l, L_POINTS), n("points".into()), 1.0, MAX_POINTS as f32, 2.0, "", Lin);
        for k in 0..MAX_POINTS {
            // The default shape is a triangle: (0, 0) up to (½, 1), back down to the start.
            let (x, y) = match k {
                0 => (0.0, 0.0),
                1 => (0.5, 1.0),
                _ => (1.0, 0.0),
            };
            new(point_x(l, k), n(format!("point {} x", k + 1)), 0.0, 1.0, x, "%", Lin);
            new(point_y(l, k), n(format!("point {} y", k + 1)), 0.0, 1.0, y, "%", Lin);
            new(point_bend(l, k), n(format!("point {} bend", k + 1)), -1.0, 1.0, 0.0, "", Lin);
        }
    }
    for s in 0..SLOT_COUNT {
        new(slot(s, S_AMT), name(format!("Mod {} amount", s + 1)), -1.0, 1.0, 0.0, "%", Lin);
    }
    for m in 0..4 {
        new(mac(m), name(format!("Macro {}", m + 1)), 0.0, 1.0, 0.0, "%", Lin);
    }
    new(VOLUME, "Volume", 0.0, 2.0, 0.5, "gain", Curve::Fader);
    new(AMP_VEL, "Velocity", 0.0, 1.0, 1.0, "%", Lin);
    new(GLIDE, "Glide", 0.0, 2.0, 0.08, "s", Lin);

    // Choices.
    let mut choice = |i: usize, n: &'static str, labels: &'static [&'static str], def: u32| {
        a[i] = Some(ParamInfo::choice(i as u32, n, labels, def));
    };
    for o in 0..OSCS {
        let n = |what: &str| name(format!("Osc {} {what}", o + 1));
        choice(osc(o, OSC_ON), name(format!("Osc {}", o + 1)), &OFF_ON, if o == 0 { 1 } else { 0 });
        choice(osc(o, TABLE), n("table"), &TABLE_NAMES, 0);
        choice(osc(o, OCTAVE), n("octave"), &OCTAVES, 3);
        choice(osc(o, WARP_MODE), n("warp mode"), &WARP_MODES, 0);
        choice(osc(o, UNISON), n("unison"), &UNISON_LABELS, 2);
    }
    choice(SUB_ON, "Sub", &OFF_ON, 0);
    choice(SUB_SHAPE, "Sub shape", &SUB_SHAPES, 0);
    choice(SUB_OCT, "Sub octave", &SUB_OCTAVES, 0);
    choice(NOISE_ON, "Noise", &OFF_ON, 0);
    choice(DIST_MODE, "Drive mode", &DIST_MODES, 0);
    for f in 0..2 {
        choice(filter(f, F_ON), name(format!("Filter {}", f + 1)), &OFF_ON, 0);
        let ty = if f == 0 { ft::LOW24 } else { ft::HIGH12 } as u32;
        choice(filter(f, F_TYPE), name(format!("Filter {} type", f + 1)), &FILTER_TYPES, ty);
    }
    choice(ROUTING, "Filter routing", &ROUTINGS, 0);
    for l in 0..LFO_COUNT {
        choice(lfo(l, L_SYNC), name(format!("LFO {} sync", l + 1)), &SYNC_LABELS, 5); // 1/4
        choice(lfo(l, L_MODE), name(format!("LFO {} mode", l + 1)), &LFO_MODES, 0);
    }
    for s in 0..SLOT_COUNT {
        let n = |what: &str| name(format!("Mod {} {what}", s + 1));
        choice(slot(s, S_SRC), n("source"), &SOURCES, 0);
        choice(slot(s, S_DEST), n("destination"), &DESTS, 0);
        choice(slot(s, S_VIA), n("via"), &SOURCES, 0);
        choice(slot(s, S_BIPOLAR), n("polarity"), &POLARITIES, 0);
    }
    choice(MODE, "Mode", &VOICE_MODES, 0);
    choice(GLIDE_MODE, "Glide when", &GLIDE_MODES, 0);
    for l in 0..LFO_COUNT {
        choice(lfo_style(l), name(format!("LFO {} style", l + 1)), &LFO_STYLES, 0);
    }

    a.into_iter().enumerate().map(|(i, x)| x.unwrap_or_else(|| panic!("parameter {i} has no description"))).collect()
}

/// Editor layout: one group per panel, in id order.
pub static GROUPS: LazyLock<Vec<ParamGroup>> = LazyLock::new(|| {
    use p::*;
    let g = |name: &'static str, a: usize, b: usize| ParamGroup { name, ids: a as u32..b as u32 };
    let mut v = vec![
        g("Osc 1", osc(0, 0), osc(1, 0)),
        g("Osc 2", osc(1, 0), SUB_ON),
        g("Sub, noise, drive", SUB_ON, FILTERS),
        g("Filters", FILTERS, ENVS),
        g("Amp envelope", env(0, 0), env(1, 0)),
        g("Env 2", env(1, 0), env(2, 0)),
        g("Env 3", env(2, 0), env(3, 0)),
        g("Env 4", env(3, 0), LFOS),
    ];
    const LFO_NAMES: [&str; 4] = ["LFO 1", "LFO 2", "LFO 3", "LFO 4"];
    for (l, n) in LFO_NAMES.iter().enumerate() {
        v.push(g(n, lfo(l, 0), lfo(l + 1, 0)));
    }
    v.push(g("Mod matrix", SLOTS, MACROS));
    v.push(g("Macros", MACROS, VOLUME));
    v.push(g("Voice", VOLUME, LFO_STYLE));
    v.push(g("LFO styles", LFO_STYLE, COUNT));
    v
});
