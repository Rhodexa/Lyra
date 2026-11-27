//! "Dawnbreaker": a heroic trailer cue, and a workout for automation clips.
//! 96 BPM, D minor, ending on D major; about two minutes.
//!
//! The build-ups are automation, not tricks with long envelopes:
//! * **Choir vowels**: one curve, two links, opens the choir from "oo" to
//!   "oh" (the formant filter's vowel and the table's position together;
//!   the vowel knob runs A-E-I-O-U, so "oo" to "ah" would pass "ee").
//! * **Strings open**: the ostinato's cutoff over eight bars; the breakdown
//!   replays only its second half (a clip with an offset).
//! * **Brass swell**: one bar, repeated by long clips: FM depth on both
//!   carriers and the cutoff, three links, so every bar of brass blooms.
//! * **Riser**: pitch, cutoff and volume of a noise-and-saw riser, used by
//!   two clips (a shared curve).
//! * **Choir wash** and **choir close**: the breakdown's reverb and vowel.
//!
//! ```text
//! cargo run --release -p lyra-app --example heroic -- <out dir>
//! cargo run --release -p lyra-app -- --render "<out dir>/Dawnbreaker.lyra"
//! ```

mod songkit;
use lyra_chrysalis::{p as cs, PARAMS};
use lyra_fluttersine::p as fs;
use lyra_model::{ChannelId, Clip, Link, Note, Project, Scale, Target};
use songkit::*;

const BPM: f64 = 96.0;
const CH: &str = "lyra.chrysalis";

fn n(start: u32, len: u32, key: i32, velocity: f32) -> Note {
    Note { start, len, key: key.clamp(0, 127) as u8, velocity: velocity.clamp(0.05, 1.0), link: None }
}

// ── Patches ─────────────────────────────────────────────────────────────

/// A Chrysalis patch from a preset, with choices set by label.
struct Chrys(Vec<f32>);

impl Chrys {
    fn preset(name: &str) -> Self {
        let p = lyra_catalog::presets_for(CH).into_iter().find(|p| p.name == name).expect("preset");
        Chrys(p.params())
    }
    fn set(mut self, id: usize, x: f32) -> Self {
        self.0[id] = x;
        self
    }
    fn pick(self, id: usize, label: &str) -> Self {
        let info = &PARAMS[id];
        let i = info.labels.iter().position(|l| *l == label).unwrap_or_else(|| panic!("{}: no {label}", info.name));
        self.set(id, i as f32)
    }
    fn env(self, e: usize, a: f32, d: f32, s: f32, r: f32) -> Self {
        self.set(cs::env(e, cs::E_ATTACK), a)
            .set(cs::env(e, cs::E_DECAY), d)
            .set(cs::env(e, cs::E_SUSTAIN), s)
            .set(cs::env(e, cs::E_RELEASE), r)
    }
    /// Matrix slot `k`: `source` → `dest` × `amount` (indices into SOURCES / DESTS).
    fn route(self, k: usize, source: &str, dest: &str, amount: f32, bipolar: bool) -> Self {
        let ix = |list: &[&str], name: &str| list.iter().position(|x| *x == name).expect(name) as f32;
        self.set(cs::slot(k, cs::S_SRC), ix(&lyra_chrysalis::SOURCES, source))
            .set(cs::slot(k, cs::S_DEST), ix(&lyra_chrysalis::DESTS, dest))
            .set(cs::slot(k, cs::S_AMT), amount)
            .set(cs::slot(k, cs::S_BIPOLAR), if bipolar { 1.0 } else { 0.0 })
    }
}

/// Voices on the vowels table through the formant filter: the vowel and
/// the position are left for automation ("oo" at rest).
fn choir() -> Vec<f32> {
    Chrys::preset("Init")
        .pick(cs::osc(0, cs::TABLE), "Vowels")
        .set(cs::osc(0, cs::POS), 0.85)
        .pick(cs::osc(0, cs::UNISON), "6")
        .set(cs::osc(0, cs::DETUNE), 0.22)
        .set(cs::osc(0, cs::SPREAD), 0.6)
        .set(cs::osc(0, cs::BLEND), 0.8)
        .pick(cs::osc(1, cs::OSC_ON), "On")
        .pick(cs::osc(1, cs::TABLE), "Vowels")
        .set(cs::osc(1, cs::POS), 0.6)
        .pick(cs::osc(1, cs::UNISON), "4")
        .set(cs::osc(1, cs::FINE), -6.0)
        .set(cs::osc(1, cs::LEVEL), 0.5)
        .pick(cs::NOISE_ON, "On")
        .set(cs::NOISE_COLOR, -0.3)
        .set(cs::NOISE_LEVEL, 0.04)
        .pick(cs::filter(0, cs::F_ON), "On")
        .pick(cs::filter(0, cs::F_TYPE), "Formant")
        .set(cs::filter(0, cs::F_CUTOFF), 1000.0)
        .set(cs::filter(0, cs::F_RESO), 0.5)
        .set(cs::filter(0, cs::F_VOWEL), 1.0)
        .set(cs::filter(0, cs::F_MIX), 0.85)
        .pick(cs::filter(1, cs::F_ON), "On")
        .pick(cs::filter(1, cs::F_TYPE), "Low 12")
        .set(cs::filter(1, cs::F_CUTOFF), 5000.0)
        .env(0, 0.6, 1.0, 1.0, 1.2)
        .pick(cs::lfo(0, cs::L_SYNC), "Free")
        .set(cs::lfo(0, cs::L_RATE), 4.8)
        .route(0, "LFO 1", "Pitch", 0.002, true)
        .set(cs::VOLUME, 0.75)
        .0
}

/// Spiccato strings: five saws, a short bow, the cutoff left for automation.
fn ostinato() -> Vec<f32> {
    Chrys::preset("Init")
        .pick(cs::osc(0, cs::UNISON), "5")
        .set(cs::osc(0, cs::DETUNE), 0.2)
        .set(cs::osc(0, cs::SPREAD), 0.8)
        .pick(cs::filter(0, cs::F_ON), "On")
        .pick(cs::filter(0, cs::F_TYPE), "Low 24")
        .set(cs::filter(0, cs::F_CUTOFF), 350.0)
        .set(cs::filter(0, cs::F_RESO), 0.2)
        .set(cs::filter(0, cs::F_KEY), 0.3)
        .env(0, 0.002, 0.18, 0.25, 0.1)
        .env(1, 0.001, 0.08, 0.0, 0.05)
        .route(0, "Env 2", "Filter 1 cutoff", 0.15, false)
        .set(cs::VOLUME, 0.9)
        .0
}

/// "BRAAAM": wide saws an octave apart, driven, the filter opening slowly.
fn braam() -> Vec<f32> {
    Chrys::preset("Init")
        .pick(cs::osc(0, cs::UNISON), "7")
        .set(cs::osc(0, cs::DETUNE), 0.3)
        .pick(cs::osc(1, cs::OSC_ON), "On")
        .set(cs::osc(1, cs::POS), 2.0 / 3.0)
        .pick(cs::osc(1, cs::OCTAVE), "−1")
        .set(cs::osc(1, cs::LEVEL), 0.6)
        .pick(cs::SUB_ON, "On")
        .pick(cs::DIST_MODE, "Tanh")
        .set(cs::DIST_DRIVE, 0.5)
        .pick(cs::filter(0, cs::F_ON), "On")
        .pick(cs::filter(0, cs::F_TYPE), "Low 24")
        .set(cs::filter(0, cs::F_CUTOFF), 300.0)
        .set(cs::filter(0, cs::F_RESO), 0.25)
        .env(0, 0.02, 3.0, 0.6, 1.5)
        .env(1, 0.25, 2.0, 0.2, 1.0)
        .route(0, "Env 2", "Filter 1 cutoff", 0.5, false)
        .set(cs::VOLUME, 0.35)
        .0
}

/// Saws and noise; automation does all the rising.
fn riser() -> Vec<f32> {
    Chrys::preset("Init")
        .pick(cs::osc(0, cs::UNISON), "9")
        .set(cs::osc(0, cs::DETUNE), 0.5)
        .pick(cs::NOISE_ON, "On")
        .set(cs::NOISE_LEVEL, 0.6)
        .set(cs::NOISE_COLOR, 0.2)
        .pick(cs::filter(0, cs::F_ON), "On")
        .pick(cs::filter(0, cs::F_TYPE), "Low 12")
        .set(cs::filter(0, cs::F_CUTOFF), 300.0)
        .set(cs::filter(0, cs::F_RESO), 0.3)
        .env(0, 0.01, 1.0, 1.0, 0.4)
        .set(cs::VOLUME, 0.3)
        .0
}

/// FM Brass as a section: a second carrier on the same modulator, detuned
/// and panned the other way, and a slow vibrato. Darker at rest: the swell
/// automation brightens it.
fn brass() -> Vec<f32> {
    use fs::*;
    fm_preset(
        "FM Brass",
        &[
            (op(0, OP_FINE), -5.0),
            (op(2, OP_ATTACK), 0.05),
            (op(2, OP_DECAY), 0.6),
            (op(2, OP_SUSTAIN), 0.8),
            (op(2, OP_RELEASE), 0.25),
            (op(2, OP_FINE), 8.0),
            (out(2), 1.0),
            (matrix(1, 0), 0.35),
            (matrix(1, 2), 0.35),
            (pan(0), -0.35),
            (pan(2), 0.35),
            (CUTOFF, 2500.0),
            (LFO_RATE, 4.5),
            (VIBRATO, 8.0),
            (VOLUME, 0.3),
        ],
    )
}

// ── Music ───────────────────────────────────────────────────────────────

/// Dm B♭ F C | Dm B♭ Gm A: (bass root, triad).
const PROG: [(i32, [i32; 3]); 8] = [
    (38, [0, 3, 7]),
    (34, [0, 4, 7]),
    (41, [0, 4, 7]),
    (36, [0, 4, 7]),
    (38, [0, 3, 7]),
    (34, [0, 4, 7]),
    (31, [0, 3, 7]),
    (33, [0, 4, 7]),
];

/// The theme, 8 bars (one token = a 16th).
const THEME: [&str; 8] = [
    "D-----A---------",
    "B---A-G-F-------",
    "A-----c-A---F---",
    "G---------E-F-G-",
    "A-----d---------",
    "d---c-B-c---d---",
    "G---B---d---g---",
    "e-----^c-A-------",
];

/// Theme bars `from..to` from bar `at`, shifted by `shift` semitones.
fn theme(at: u32, from: usize, to: usize, shift: i32, vel: f32) -> Vec<Note> {
    let first = from as u32 * 16;
    let last = to as u32 * 16;
    parse(&THEME.concat(), D_MINOR)
        .into_iter()
        .filter(|t| t.step >= first && t.step < last)
        .map(|t| n(at * BAR + (t.step - first) * STEP, (t.len * STEP).saturating_sub(6), t.key + shift, vel))
        .collect()
}

/// The chord of bar `b` voiced around `center`.
fn chord(b: u32, center: i32) -> Vec<i32> {
    let (root, tri) = PROG[b as usize % 8];
    tri.iter().map(|i| center - 6 + ((root + i).rem_euclid(12) - (center - 6)).rem_euclid(12)).collect()
}

/// Whole-bar chords for `bars` bars (from progression bar `first`).
fn chords(bars: u32, first: u32, center: i32, vel: f32) -> Vec<Note> {
    (0..bars).flat_map(|b| chord(first + b, center).into_iter().map(move |k| n(b * BAR, BAR - 4, k, vel))).collect()
}

/// Bass roots: root + fifth an octave below `center`-ish, whole bars.
fn low(bars: u32, first: u32, vel: f32) -> Vec<Note> {
    (0..bars)
        .flat_map(|b| {
            let root = PROG[(first + b) as usize % 8].0;
            [n(b * BAR, BAR - 4, root, vel), n(b * BAR, BAR - 4, root + 7, vel * 0.8)]
        })
        .collect()
}

/// The 3-3-2 ostinato in 16ths: root, octave, fifth.
fn ostinato_notes(bars: u32, first: u32, vel: f32) -> Vec<Note> {
    const SHAPE: [i32; 16] = [0, 12, 7, 0, 12, 7, 0, 7, 0, 12, 7, 0, 12, 7, 12, 7];
    let mut v = Vec::new();
    for b in 0..bars {
        let root = 50 + (PROG[(first + b) as usize % 8].0 - 38);
        for (s, &up) in SHAPE.iter().enumerate() {
            let accent = if [0, 3, 6, 8, 11, 14].contains(&s) { 1.0 } else { 0.7 };
            v.push(n(b * BAR + s as u32 * STEP, STEP - 4, root + up, vel * accent));
        }
    }
    v
}

/// The power chord on the downbeat of every `every` bars.
fn braams(bars: u32, first: u32, every: u32) -> Vec<Note> {
    (0..bars)
        .step_by(every as usize)
        .flat_map(|b| {
            let root = PROG[(first + b) as usize % 8].0 - 12;
            [n(b * BAR, every * BAR - 8, root, 1.0), n(b * BAR, every * BAR - 8, root + 7, 0.9)]
        })
        .collect()
}

struct Kit {
    boom: ChannelId,
    lo: ChannelId,
    hi: ChannelId,
    snare: ChannelId,
    roll: ChannelId,
    crash: ChannelId,
}

fn hits(bar: u32, steps: &[(u32, f32)]) -> Vec<Note> {
    steps.iter().map(|&(s, vel)| n(bar * BAR + s * STEP, 2 * STEP, 60, vel)).collect()
}

/// Taikos for the rise: sparse for four bars, then answering each other.
fn taiko_rise(k: &Kit) -> Vec<(ChannelId, Vec<Note>)> {
    let (mut lo, mut hi) = (vec![], vec![]);
    for b in 0..8 {
        if b < 4 {
            lo.extend(hits(b, &[(0, 0.9), (10, 0.6)]));
            hi.extend(hits(b, &[(12, 0.5)]));
        } else {
            lo.extend(hits(b, &[(0, 1.0), (3, 0.6), (6, 0.8), (8, 0.9), (11, 0.6), (14, 0.8)]));
            hi.extend(hits(b, &[(4, 0.7), (12, 0.8), (13, 0.5), (15, 0.6)]));
        }
    }
    vec![(k.lo, lo), (k.hi, hi), (k.roll, vec![n(7 * BAR, BAR - 2 * STEP, 60, 0.9)])]
}

/// The battle rhythm: taikos, snares on 2 and 4, a boom every 4 bars, a
/// crash every 8, a roll into every 4th bar line.
fn battle(k: &Kit, bars: u32) -> Vec<(ChannelId, Vec<Note>)> {
    let (mut boom, mut lo, mut hi, mut snare, mut crash) = (vec![], vec![], vec![], vec![], vec![]);
    for b in 0..bars {
        if b % 4 == 0 {
            boom.extend(hits(b, &[(0, 1.0)]));
        }
        if b % 8 == 0 {
            crash.extend(hits(b, &[(0, 0.9)]));
        }
        lo.extend(hits(b, &[(0, 1.0), (3, 0.7), (6, 0.85), (8, 1.0), (11, 0.7), (14, 0.85)]));
        hi.extend(hits(b, &[(4, 0.8), (10, 0.5), (12, 0.8), (15, 0.6)]));
        snare.extend(hits(b, &[(4, 0.9), (12, 0.9)]));
        if b % 4 == 3 {
            for s in 12..16 {
                for half in 0..2 {
                    let at = b * BAR + s * STEP + half * STEP / 2;
                    snare.push(n(at, STEP / 2, 60, 0.4 + 0.12 * (s - 12) as f32));
                }
            }
        }
    }
    vec![(k.boom, boom), (k.lo, lo), (k.hi, hi), (k.snare, snare), (k.crash, crash)]
}

fn build() -> Project {
    let mut p = Project::default();
    p.bpm = BPM;
    p.key_root = 2; // D
    p.scale = Scale::Minor;
    p.master_volume = 1.0;

    // Percussion: the kick synth as the boom, the Starter Kit for the rest.
    let boom =
        p.add_channel("Boom", "lyra.kick", plugin("lyra.kick", &[(0, 36.0), (1, 3.0), (2, 0.1), (3, 1.4), (5, 0.4)]));
    let kit_sample = |p: &mut Project, name: &str, file: &str, set: &[(usize, f32)]| {
        let id = p.add_channel(name, "lyra.sampler", plugin("lyra.sampler", set));
        let ch = p.channels.iter_mut().find(|c| c.id == id).unwrap();
        ch.sample = Some(format!("bundled:Starter Kit/{file}"));
        id
    };
    use lyra_sampler::p::{GAIN, RELEASE, TUNE};
    let lo =
        kit_sample(&mut p, "Taiko low", "Toms/drum_tom_lo_hard.flac", &[(TUNE, -5.0), (RELEASE, 1.0), (GAIN, -5.0)]);
    let hi =
        kit_sample(&mut p, "Taiko high", "Toms/drum_tom_mid_hard.flac", &[(TUNE, -3.0), (RELEASE, 0.6), (GAIN, -5.0)]);
    let snare = kit_sample(&mut p, "Snare", "Snares/drum_snare_hard.flac", &[(RELEASE, 0.4), (GAIN, -3.0)]);
    let roll = kit_sample(&mut p, "Roll", "Percussion/drum_roll.flac", &[(RELEASE, 0.3)]);
    let crash = kit_sample(&mut p, "Crash", "Hats & cymbals/drum_cymbal_hard.flac", &[(RELEASE, 3.0), (GAIN, 6.0)]);

    // The orchestra.
    let strings = p.add_channel("Low strings", "lyra.fluttersine", fm_preset("PWM Strings", &[]));
    p.channels.iter_mut().find(|c| c.id == strings).unwrap().volume = 0.5;
    let ost = p.add_channel("Ostinato", CH, ostinato());
    let choir_ch = p.add_channel("Choir", CH, choir());
    let brass_ch = p.add_channel("Brass", "lyra.fluttersine", brass());
    let saw = p.add_channel("High saws", CH, Chrys::preset("Supersaw lead").set(cs::VOLUME, 0.3).0);
    let braam_ch = p.add_channel("Braam", CH, braam());
    let bell = p.add_channel("Bell", "lyra.fluttersine", fm_preset("FM Bell", &[]));
    let rise = p.add_channel("Riser", CH, riser());

    // Mixer: a hall on almost everything (there are no sends yet).
    let insert = |p: &Project, c: ChannelId| p.channels.iter().find(|x| x.id == c).unwrap().mixer;
    let hall = |mix: f32| fx("lyra.reverb", &[(0, 0.9), (1, 0.5), (3, mix)]);
    for (c, mix) in [(lo, 0.25), (hi, 0.25), (snare, 0.3), (roll, 0.3), (crash, 0.2), (boom, 0.15)] {
        let t = insert(&p, c);
        p.mixer[t].effects = vec![hall(mix)];
    }
    for (c, mix) in [(strings, 0.3), (ost, 0.2), (brass_ch, 0.28), (saw, 0.3), (bell, 0.45), (rise, 0.35)] {
        let t = insert(&p, c);
        p.mixer[t].effects = vec![hall(mix)];
    }
    let choir_track = insert(&p, choir_ch);
    p.mixer[choir_track].effects = vec![fx("lyra.chorus", &[(0, 0.3), (1, 0.4), (3, 0.3)]), hall(0.35)];
    let t = insert(&p, braam_ch);
    p.mixer[t].effects = vec![fx("lyra.eq", &[(0, 35.0), (2, -3.0), (3, 400.0)]), hall(0.25)];
    p.mixer[0].effects = vec![
        fx("lyra.compressor", &[(0, -10.0), (1, 2.0), (2, 0.02), (3, 0.25), (4, 1.0)]),
        fx("lyra.limiter", &[(0, 3.0), (1, -0.3), (2, 0.1)]),
    ];
    p.number_effects();
    let choir_hall = p.mixer[choir_track].effects[1].id;

    // Tracks: music first, then a row per automation family.
    let names = [
        "Drums",
        "Low",
        "Strings",
        "Choir",
        "Brass",
        "High",
        "FX",
        "Bell",
        "A · Choir",
        "A · Strings",
        "A · Brass",
        "A · FX",
    ];
    for (t, name) in names.iter().enumerate() {
        p.tracks[t].name = (*name).into();
    }
    let (t_drums, t_low, t_str, t_choir, t_brass, t_high, t_fx, t_bell) = (0, 1, 2, 3, 4, 5, 6, 7);
    let (a_choir, a_str, a_brass, a_fx) = (8, 9, 10, 11);
    p.patterns.clear();
    let kit = Kit { boom, lo, hi, snare, roll, crash };
    let put = |p: &mut Project, track: usize, name: &str, at: u32, bars: u32, notes: Vec<(ChannelId, Vec<Note>)>| {
        let i = p.add_pattern();
        let pat = &mut p.patterns[i];
        pat.name = name.into();
        pat.min_length = bars * 16;
        for (ch, ns) in notes {
            pat.notes_mut(ch).extend(ns.into_iter().filter(|x| x.start < bars * BAR));
        }
        let id = pat.id;
        p.add_clip(Clip::of_pattern(id, track, at * BAR, bars * BAR));
    };

    // ── Automations ──
    let vowel = cs::filter(0, cs::F_VOWEL);
    let links = [param_link(&p, choir_ch, vowel, 1.0, 0.75), param_link(&p, choir_ch, cs::osc(0, cs::POS), 0.85, 0.3)];
    let open_choir =
        automate(&mut p, "Choir vowels", 8 * BAR, &[(0, 0.0, 0.0), (6 * BAR, 1.0, 0.0), (8 * BAR, 1.0, 0.0)], &links);
    let links = [param_link(&p, choir_ch, vowel, 0.75, 1.0)];
    let close_choir = automate(
        &mut p,
        "Choir close",
        4 * BAR,
        &[(0, 0.0, 0.0), (BAR, 1.0, 0.0), (3 * BAR, 1.0, 0.0), (4 * BAR, 0.0, 0.5)],
        &links,
    );
    let links = [Link {
        target: Target::Effect { effect: choir_hall, param: 3 },
        from: fx_travel("lyra.reverb", 3, 0.35),
        to: fx_travel("lyra.reverb", 3, 0.7),
    }];
    let wash = automate(
        &mut p,
        "Choir wash",
        4 * BAR,
        &[(0, 0.0, -0.5), (BAR, 1.0, 0.0), (3 * BAR, 1.0, 0.5), (4 * BAR, 0.0, 0.0)],
        &links,
    );
    let links = [param_link(&p, ost, cs::filter(0, cs::F_CUTOFF), 350.0, 7000.0)];
    let open_strings = automate(&mut p, "Strings open", 8 * BAR, &[(0, 0.0, 0.6), (8 * BAR, 1.0, 0.0)], &links);
    let links = [
        param_link(&p, brass_ch, fs::matrix(1, 0), 0.3, 0.75),
        param_link(&p, brass_ch, fs::matrix(1, 2), 0.3, 0.75),
        param_link(&p, brass_ch, fs::CUTOFF, 1800.0, 9000.0),
    ];
    let swell =
        automate(&mut p, "Brass swell", BAR, &[(0, 0.15, -0.4), (10 * STEP, 1.0, 0.3), (BAR, 0.3, 0.0)], &links);
    let links = [
        param_link(&p, rise, cs::osc(0, cs::SEMI), -12.0, 12.0),
        param_link(&p, rise, cs::filter(0, cs::F_CUTOFF), 300.0, 9000.0),
        Link { target: Target::Volume(rise), from: volume_travel(0.05), to: volume_travel(0.5) },
    ];
    let rising = automate(
        &mut p,
        "Riser",
        4 * BAR,
        &[(0, 0.0, 0.7), (4 * BAR - 4 * STEP, 1.0, 0.0), (4 * BAR, 1.0, 0.0)],
        &links,
    );

    // 1 · Dawn (0–7): the choir opens its mouth; a lone horn; distant drums.
    put(&mut p, t_choir, "Dawn choir", 0, 8, vec![(choir_ch, chords(8, 0, 62, 0.7))]);
    place(&mut p, open_choir, a_choir, 0, 8, 0);
    put(&mut p, t_low, "Drone", 0, 8, vec![(strings, low(8, 0, 0.5))]);
    put(&mut p, t_brass, "Horn call", 0, 8, vec![(brass_ch, theme(0, 0, 8, 0, 0.45))]);
    put(
        &mut p,
        t_drums,
        "Distant",
        0,
        8,
        vec![(lo, hits(0, &[(0, 0.6)]).into_iter().chain(hits(4, &[(0, 0.7)])).collect())],
    );

    // 2 · Rise (8–15): the ostinato opens up, the taikos gather, the riser.
    put(&mut p, t_str, "Ostinato", 8, 8, vec![(ost, ostinato_notes(8, 0, 0.8))]);
    place(&mut p, open_strings, a_str, 8, 8, 0);
    put(&mut p, t_drums, "Gathering", 8, 8, taiko_rise(&kit));
    put(&mut p, t_low, "Low chords", 8, 8, vec![(strings, low(8, 0, 0.6)), (strings, chords(8, 0, 57, 0.5))]);
    put(&mut p, t_choir, "Rise choir", 8, 8, vec![(choir_ch, chords(8, 0, 64, 0.8))]);
    put(&mut p, t_fx, "Riser", 12, 4, vec![(rise, vec![n(0, 4 * BAR - 4 * STEP, 50, 1.0)])]);
    place(&mut p, rising, a_fx, 12, 4, 0);

    // 3 · Charge (16–31): the theme on brass, twice; the second time the
    // high saws double it and the choir climbs.
    put(&mut p, t_drums, "Battle", 16, 16, battle(&kit, 16));
    put(&mut p, t_brass, "Theme", 16, 8, vec![(brass_ch, theme(0, 0, 8, 0, 0.9))]);
    put(&mut p, t_brass, "Theme again", 24, 8, vec![(brass_ch, theme(0, 0, 8, 0, 0.95))]);
    place(&mut p, swell, a_brass, 16, 16, 0);
    put(&mut p, t_high, "Saws", 24, 8, vec![(saw, theme(0, 0, 8, 12, 0.8))]);
    put(&mut p, t_str, "Ostinato 2", 16, 16, vec![(ost, ostinato_notes(16, 0, 0.9))]);
    put(&mut p, t_low, "Low 2", 16, 16, vec![(strings, low(16, 0, 0.7)), (braam_ch, braams(16, 0, 4))]);
    put(&mut p, t_choir, "Choir chords", 16, 8, vec![(choir_ch, chords(8, 0, 64, 0.85))]);
    put(&mut p, t_choir, "Choir high", 24, 8, vec![(choir_ch, chords(8, 0, 69, 0.9))]);

    // 4 · Breath (32–35): choir and bell alone, a long hall; the riser again.
    put(&mut p, t_choir, "Breath choir", 32, 4, vec![(choir_ch, chords(4, 0, 64, 0.6))]);
    place(&mut p, close_choir, a_choir, 32, 4, 0);
    place(&mut p, wash, a_fx, 32, 3, 0);
    put(&mut p, t_bell, "Bell", 32, 4, vec![(bell, theme(0, 0, 4, 12, 0.6))]);
    put(&mut p, t_low, "Breath drone", 32, 4, vec![(strings, low(4, 0, 0.4))]);
    put(&mut p, t_str, "Ostinato returns", 34, 2, vec![(ost, ostinato_notes(2, 2, 0.7))]);
    place(&mut p, open_strings, a_str, 34, 2, 6 * BAR);
    put(&mut p, t_fx, "Riser 2", 32, 4, vec![(rise, vec![n(0, 4 * BAR - 4 * STEP, 50, 1.0)])]);
    place(&mut p, rising, a_fx, 35, 1, 3 * BAR);

    // 5 · Daybreak (36–45): everything; the last bar turns to D major.
    put(&mut p, t_drums, "Battle 2", 36, 8, battle(&kit, 8));
    put(&mut p, t_brass, "Theme last", 36, 8, vec![(brass_ch, theme(0, 0, 8, 0, 1.0))]);
    put(&mut p, t_high, "Saws last", 36, 8, vec![(saw, theme(0, 0, 8, 12, 0.85))]);
    place(&mut p, swell, a_brass, 36, 8, 0);
    put(&mut p, t_str, "Ostinato last", 36, 8, vec![(ost, ostinato_notes(8, 0, 1.0))]);
    put(&mut p, t_low, "Low last", 36, 8, vec![(strings, low(8, 0, 0.8)), (braam_ch, braams(8, 0, 2))]);
    put(&mut p, t_choir, "Choir last", 36, 8, vec![(choir_ch, chords(8, 0, 69, 0.8))]);
    place(&mut p, open_choir, a_choir, 36, 1, 7 * BAR);
    let major = [50, 54, 57, 62, 66, 69];
    let hold = 2 * BAR;
    put(
        &mut p,
        t_choir,
        "Daybreak",
        44,
        2,
        vec![
            (choir_ch, major.iter().map(|&k| n(0, hold, k + 12, 1.0)).collect()),
            (brass_ch, [62, 66, 69, 74].iter().map(|&k| n(0, hold, k, 1.0)).collect()),
            (saw, vec![n(0, hold, 86, 0.85)]),
            (strings, [26, 38, 45, 50].iter().map(|&k| n(0, hold, k, 0.9)).collect()),
            (braam_ch, vec![n(0, hold, 26, 1.0), n(0, hold, 33, 0.9)]),
            (boom, hits(0, &[(0, 1.0)])),
            (lo, hits(0, &[(0, 1.0)])),
            (crash, hits(0, &[(0, 1.0)])),
        ],
    );
    p
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    finish(&build(), &dir, "Dawnbreaker");
}
