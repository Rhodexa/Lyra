//! "The Two-Week Reel": a chiptune Celtic set written by Claude *with* Lyra.
//! Builds a real project (channels, patterns, playlist, mixer) and saves it as
//! a .lyra you can open, or render with `lyra --render`.
//!
//!     cargo run --release -p lyra-app --example celtic -- <out dir>
//!     cargo run --release -p lyra-app -- --render "<out dir>/<title>.lyra"

mod songkit;
use lyra_fluttersine::p::{self as fm, op};
use lyra_model::{Clip, EffectSlot, Note, Project, TICKS_PER_BAR, TICKS_PER_STEP};
use songkit::{parse, D_MAJOR};

const STEP: u32 = TICKS_PER_STEP;

// ───────────────────────────── the tune ─────────────────────────────
// ABC-ish: one token = one step (a reel quaver). Upper case = octave 4,
// lower case = octave 5, ' up / , down, ^ sharp, = natural, - hold, z rest.
// Key of D: F and C are sharp.

const AIR: &str = "A---d---f---e-dc d-----------z-FA";
const TUNE_A: &str = "d-AF DFAF GBdB AFDF E-GB eBGB AFAc ecAc \
                      d-AF DFAF GBdg fdec dBAF EGce dfec d--z";
const TUNE_B: &str = "f-fe fafd gfed cAAc e-ed egec dcBA Bcde \
                      f-fe fafd gfga bgeg fage cecA dfec d--z";
// One chord per half reel bar (4 steps).
const CHORDS_A: &str = "D D G G Em Em A A D D G A D A D D";
const CHORDS_B: &str = "D D G A A A G G D D G Em D A D D";

/// D major, as semitones above C, for "a third below" harmony.
const SCALE: [i32; 7] = [2, 4, 6, 7, 9, 11, 13]; // D E F# G A B C#

/// A quaver's weight in the reel: strong on the beat, lighter off it.
fn accent(step: u32) -> f32 {
    match step % 8 {
        0 => 1.0,
        4 => 0.92,
        2 | 6 => 0.82,
        _ => 0.7,
    }
}

/// Melody notes; `cuts` adds the fiddle/whistle "cut" (a flick off the note
/// above) on every held note.
fn melody(tune: &str, shift: i32, cuts: bool) -> Vec<Note> {
    let mut v = Vec::new();
    for t in parse(tune, D_MAJOR) {
        let (start, len) = (t.step * STEP, t.len * STEP - 3);
        let key = (t.key + shift) as u8;
        let vel = accent(t.step);
        if cuts && t.len >= 2 {
            // a third up, kept in the key (keys are in D, or E when shifted by 2)
            let in_key = |k: u8| SCALE.iter().any(|&s| (s + shift).rem_euclid(12) == k as i32 % 12);
            let grace = if in_key(key + 3) { key + 3 } else { key + 4 };
            v.push(Note { start, len: 5, key: grace, velocity: vel * 0.8 });
            v.push(Note { start: start + 6, len: len - 6, key, velocity: vel });
        } else {
            v.push(Note { start, len, key, velocity: vel });
        }
    }
    v
}

/// The same line a diatonic third lower.
fn third_below(tune: &str, shift: i32) -> Vec<Note> {
    parse(tune, D_MAJOR)
        .into_iter()
        .map(|t| {
            let oct = (t.key - 60).div_euclid(12);
            let pc = (t.key - 60).rem_euclid(12);
            let deg = SCALE.iter().position(|&s| s % 12 == pc).expect("diatonic");
            let lower = if deg >= 2 { SCALE[deg - 2] } else { SCALE[deg + 5] - 12 };
            let key = 60 + oct * 12 + lower - if pc < 2 { 12 } else { 0 };
            Note {
                start: t.step * STEP,
                len: t.len * STEP - 3,
                key: (key + shift) as u8,
                velocity: accent(t.step) * 0.9,
            }
        })
        .collect()
}

fn chord(name: &str) -> (i32, [i32; 3]) {
    // (bass root, triad for the arp)
    match name {
        "D" => (50, [74, 78, 81]),
        "G" => (43, [67, 71, 74]),
        "Em" => (52, [64, 67, 71]),
        "A" => (45, [69, 73, 76]),
        _ => panic!("chord {name}"),
    }
}

/// Root on the beat, fifth on the off-beat: the bouncy reel bass.
fn bass(chords: &str, shift: i32) -> Vec<Note> {
    let mut v = Vec::new();
    for (i, c) in chords.split_whitespace().enumerate() {
        let (root, _) = chord(c);
        let at = i as u32 * 4 * STEP;
        v.push(Note { start: at, len: 2 * STEP - 6, key: (root + shift) as u8, velocity: 1.0 });
        v.push(Note { start: at + 2 * STEP, len: 2 * STEP - 6, key: (root + 7 + shift) as u8, velocity: 0.8 });
    }
    v
}

/// The chip "chord": a triad flicked through every 8 ticks.
fn arp(chords: &str, shift: i32) -> Vec<Note> {
    let mut v = Vec::new();
    for (i, c) in chords.split_whitespace().enumerate() {
        let (_, triad) = chord(c);
        let at = i as u32 * 4 * STEP;
        for k in 0..(4 * STEP / 8) {
            let key = triad[k as usize % 3] + shift;
            v.push(Note { start: at + k * 8, len: 7, key: key as u8, velocity: if k % 3 == 0 { 0.9 } else { 0.7 } });
        }
    }
    v
}

fn hits(steps: &[(u32, f32)]) -> Vec<Note> {
    steps.iter().map(|&(s, vel)| Note { start: s * STEP, len: STEP, key: 60, velocity: vel }).collect()
}

// ───────────────────────────── the sounds ─────────────────────────────

fn fm_patch(set: &[(usize, f32)]) -> Vec<f32> {
    songkit::fm(set)
}

const PULSE: f32 = 8.0;
const TRI: f32 = 6.0;
const SAW: f32 = 7.0;
const NOISE: f32 = 10.0;

fn build() -> Project {
    let mut p = Project::default();
    p.bpm = 112.0; // half-note pulse of a reel: one bar of Lyra = two bars of the tune
    p.swing = 0.12; // a little lilt
    p.key_root = 2; // D
    p.scale = lyra_model::Scale::Major;
    p.master_volume = 0.8;

    let env = |o: usize, a: f32, d: f32, s: f32, r: f32| {
        [(op(o, fm::OP_ATTACK), a), (op(o, fm::OP_DECAY), d), (op(o, fm::OP_SUSTAIN), s), (op(o, fm::OP_RELEASE), r)]
    };
    let mut lead = vec![(op(0, fm::OP_WAVE), PULSE), (op(0, fm::OP_PW), 0.25), (op(0, fm::OP_VEL), 0.5)];
    lead.extend(env(0, 0.002, 0.3, 0.55, 0.07));
    lead.extend([(fm::VIBRATO, 14.0), (fm::LFO_RATE, 5.5), (fm::VOLUME, 0.85)]);
    let mut harm = vec![(op(0, fm::OP_WAVE), PULSE), (op(0, fm::OP_PW), 0.125), (op(0, fm::OP_VEL), 0.5)];
    harm.extend(env(0, 0.002, 0.3, 0.5, 0.07));
    harm.push((fm::VOLUME, 0.6));
    let mut arp_p = vec![(op(0, fm::OP_WAVE), PULSE), (op(0, fm::OP_PW), 0.5)];
    arp_p.extend(env(0, 0.001, 0.05, 0.6, 0.01));
    arp_p.push((fm::VOLUME, 0.45));
    let mut bass_p = vec![(op(0, fm::OP_WAVE), TRI), (op(0, fm::OP_VEL), 0.3)];
    bass_p.extend(env(0, 0.001, 0.2, 0.9, 0.03));
    bass_p.push((fm::VOLUME, 0.82));
    let mut drone = vec![(op(0, fm::OP_WAVE), SAW), (fm::LP_ON, 1.0), (fm::CUTOFF, 1100.0)];
    drone.extend(env(0, 0.5, 1.0, 1.0, 0.6));
    drone.push((fm::VOLUME, 0.33));
    let mut snare = vec![(op(0, fm::OP_WAVE), NOISE), (fm::LP_ON, 1.0), (fm::CUTOFF, 7000.0)];
    snare.extend(env(0, 0.001, 0.12, 0.0, 0.08));
    snare.push((fm::VOLUME, 1.0));
    let mut hat = vec![(op(0, fm::OP_WAVE), NOISE), (fm::HP_ON, 1.0), (fm::HP_CUTOFF, 7000.0)];
    hat.extend(env(0, 0.001, 0.03, 0.0, 0.02));
    hat.push((fm::VOLUME, 0.7));

    let fs = "lyra.fluttersine";
    let ch_lead = p.add_channel("Lead 25%", fs, fm_patch(&lead));
    let ch_harm = p.add_channel("Harmony 12%", fs, fm_patch(&harm));
    let ch_arp = p.add_channel("Arp", fs, fm_patch(&arp_p));
    let ch_bass = p.add_channel("Triangle bass", fs, fm_patch(&bass_p));
    let ch_drone = p.add_channel("Drone", fs, fm_patch(&drone));
    let mut kick = lyra_catalog::default_params("lyra.kick");
    kick[..6].copy_from_slice(&[52.0, 6.0, 0.03, 0.18, 0.25, 0.4]);
    let ch_kick = p.add_channel("Kick", "lyra.kick", kick);
    let ch_snare = p.add_channel("Noise snare", fs, fm_patch(&snare));
    let ch_hat = p.add_channel("Noise hat", fs, fm_patch(&hat));

    let mix = |p: &mut Project, id, vol: f32, pan: f32, mixer: usize| {
        let c = p.channel_mut(id).unwrap();
        (c.volume, c.pan, c.mixer) = (vol, pan, mixer);
    };
    mix(&mut p, ch_lead, 0.85, 0.12, 1);
    mix(&mut p, ch_harm, 0.8, -0.35, 2);
    mix(&mut p, ch_arp, 0.8, 0.4, 2);
    mix(&mut p, ch_bass, 0.85, 0.0, 3);
    mix(&mut p, ch_drone, 0.7, 0.0, 2);
    mix(&mut p, ch_kick, 0.65, 0.0, 4);
    mix(&mut p, ch_snare, 0.8, -0.1, 4);
    mix(&mut p, ch_hat, 0.7, 0.2, 4);

    // Mixer: echo + room on the lead, room on the backing, drums dry.
    let fx = |id: &str, set: &[(usize, f32)]| {
        let mut params = lyra_catalog::effects::default_params(id);
        for &(i, x) in set {
            params[i] = x;
        }
        EffectSlot { effect: id.into(), params, bypass: false }
    };
    for (t, name) in [(1, "Lead"), (2, "Backing"), (3, "Bass"), (4, "Drums")] {
        p.mixer[t].name = name.into();
    }
    p.mixer[1].effects =
        vec![fx("lyra.delay", &[(0, 2.0), (1, 0.28), (4, 0.16)]), fx("lyra.reverb", &[(0, 0.55), (3, 0.16)])];
    p.mixer[2].effects = vec![fx("lyra.reverb", &[(0, 0.55), (3, 0.2)])];

    // ── patterns ──
    let pat = |p: &mut Project, name: &str, notes: Vec<(u32, Vec<Note>)>| -> u32 {
        let i = p.add_pattern();
        p.patterns[i].name = name.into();
        for (ch, n) in notes {
            p.patterns[i].notes_mut(ch).extend(n);
        }
        p.patterns[i].id
    };
    p.patterns.clear();
    let drums_full = hits(&[(0, 1.0), (8, 1.0), (10, 0.7)]);
    let hats: Vec<(u32, f32)> = (0..16).step_by(2).map(|s| (s, if s % 4 == 2 { 0.9 } else { 0.6 })).collect();

    let air = pat(&mut p, "Air", vec![(ch_lead, melody(AIR, 0, true))]);
    let drone_d = pat(
        &mut p,
        "Drone D",
        vec![(
            ch_drone,
            vec![
                Note { start: 0, len: 2 * TICKS_PER_BAR, key: 50, velocity: 0.8 },
                Note { start: 0, len: 2 * TICKS_PER_BAR, key: 57, velocity: 0.7 },
            ],
        )],
    );
    let drone_e = pat(
        &mut p,
        "Drone E",
        vec![(
            ch_drone,
            vec![
                Note { start: 0, len: 2 * TICKS_PER_BAR, key: 52, velocity: 0.8 },
                Note { start: 0, len: 2 * TICKS_PER_BAR, key: 59, velocity: 0.7 },
            ],
        )],
    );
    let drums = pat(
        &mut p,
        "Drums",
        vec![(ch_kick, drums_full), (ch_snare, hits(&[(4, 0.9), (12, 0.9), (15, 0.35)])), (ch_hat, hits(&hats))],
    );
    let drums_light =
        pat(&mut p, "Drums light", vec![(ch_kick, hits(&[(0, 0.9), (8, 0.9)])), (ch_hat, hits(&hats[..]))]);
    let roll: Vec<(u32, f32)> = (8..16).map(|s| (s, 0.4 + 0.07 * (s - 8) as f32)).collect();
    let fill = pat(
        &mut p,
        "Fill",
        vec![
            (ch_kick, hits(&[(0, 1.0)])),
            (ch_snare, hits(&[&[(4, 0.9)][..], &roll].concat())),
            (ch_hat, hits(&[(0, 0.6), (2, 0.9)])),
        ],
    );

    // The tune, in D and then up a step in E (sets change key to lift).
    let mut parts = Vec::new();
    for (key, shift) in [("D", 0), ("E", 2)] {
        let a = pat(&mut p, &format!("Tune A ({key})"), vec![(ch_lead, melody(TUNE_A, shift, false))]);
        let a_hi = pat(&mut p, &format!("Tune A high ({key})"), vec![(ch_lead, melody(TUNE_A, shift + 12, true))]);
        let b = pat(&mut p, &format!("Tune B ({key})"), vec![(ch_lead, melody(TUNE_B, shift, true))]);
        let ha = pat(&mut p, &format!("Harmony A ({key})"), vec![(ch_harm, third_below(TUNE_A, shift))]);
        let hb = pat(&mut p, &format!("Harmony B ({key})"), vec![(ch_harm, third_below(TUNE_B, shift))]);
        let ba = pat(&mut p, &format!("Bass A ({key})"), vec![(ch_bass, bass(CHORDS_A, shift))]);
        let bb = pat(&mut p, &format!("Bass B ({key})"), vec![(ch_bass, bass(CHORDS_B, shift))]);
        let aa = pat(&mut p, &format!("Arp A ({key})"), vec![(ch_arp, arp(CHORDS_A, shift))]);
        let ab = pat(&mut p, &format!("Arp B ({key})"), vec![(ch_arp, arp(CHORDS_B, shift))]);
        parts.push((a, a_hi, b, ha, hb, ba, bb, aa, ab));
    }
    let long = |key: u8, vel: f32| Note { start: 0, len: TICKS_PER_BAR - 12, key, velocity: vel };
    let ending = pat(
        &mut p,
        "Ending",
        vec![
            (ch_lead, vec![Note { start: 0, len: 5, key: 83, velocity: 0.8 }, Note { start: 6, ..long(80, 1.0) }]),
            (ch_harm, vec![long(76, 0.9)]),
            (ch_bass, vec![long(52, 1.0)]),
            (
                ch_arp,
                arp("D D D D", 2)
                    .into_iter()
                    .map(|mut n| {
                        n.velocity *= 0.8;
                        n
                    })
                    .collect(),
            ),
            (ch_kick, hits(&[(0, 1.0)])),
            (ch_snare, hits(&[(0, 1.0)])),
            (ch_drone, vec![long(52, 0.8), long(59, 0.7)]),
        ],
    );
    // The pattern must be two bars, so its second bar is the ring-out.
    let ei = p.pattern_index(ending).unwrap();
    p.patterns[ei].min_length = 32;

    // ── the playlist ──
    for (t, name) in ["Lead", "Harmony", "Arp", "Bass", "Drums", "Drone"].iter().enumerate() {
        p.tracks[t].name = (*name).into();
    }
    let (lead_t, harm_t, arp_t, bass_t, drum_t, drone_t) = (0, 1, 2, 3, 4, 5);
    let bar = |b: u32| b * TICKS_PER_BAR;
    let put = |p: &mut Project, pat: u32, track: usize, from: u32, bars: u32| {
        p.add_clip(Clip::of_pattern(pat, track, bar(from), bar(bars)));
    };
    // Drums for a stretch of bars, with a fill in the last one.
    let drum_run = |p: &mut Project, from: u32, bars: u32, kind: u32, with_fill: bool| {
        let body = if with_fill { bars - 1 } else { bars };
        put(p, kind, drum_t, from, body);
        if with_fill {
            put(p, fill, drum_t, from + body, 1);
        }
    };

    // Intro: the air over the drone.
    put(&mut p, air, lead_t, 0, 2);
    put(&mut p, drone_d, drone_t, 0, 2);
    let mut at = 2;
    for (round, &(a, a_hi, b, ha, hb, ba, bb, aa, ab)) in parts.iter().enumerate() {
        let drone = if round == 0 { drone_d } else { drone_e };
        if round == 0 {
            // A, plain, light drums.
            put(&mut p, a, lead_t, at, 4);
            put(&mut p, ba, bass_t, at, 4);
            put(&mut p, drone, drone_t, at, 4);
            drum_run(&mut p, at, 4, drums_light, true);
            at += 4;
        }
        // A, an octave up with cuts and harmony.
        put(&mut p, a_hi, lead_t, at, 4);
        put(&mut p, ha, harm_t, at, 4);
        put(&mut p, ba, bass_t, at, 4);
        if round == 1 {
            put(&mut p, aa, arp_t, at, 4);
            put(&mut p, drone, drone_t, at, 4);
        }
        drum_run(&mut p, at, 4, drums, true);
        at += 4;
        // B twice: the arp joins, then the harmony.
        for pass in 0..2 {
            put(&mut p, b, lead_t, at, 4);
            put(&mut p, bb, bass_t, at, 4);
            put(&mut p, ab, arp_t, at, 4);
            if pass == 1 || round == 1 {
                put(&mut p, hb, harm_t, at, 4);
            }
            drum_run(&mut p, at, 4, drums, pass == 1);
            at += 4;
        }
    }
    // Last time through A in E, everyone, then the final chord.
    let (_, a_hi, _, ha, _, ba, _, aa, _) = parts[1];
    put(&mut p, a_hi, lead_t, at, 4);
    put(&mut p, ha, harm_t, at, 4);
    put(&mut p, ba, bass_t, at, 4);
    put(&mut p, aa, arp_t, at, 4);
    put(&mut p, drone_e, drone_t, at, 4);
    drum_run(&mut p, at, 4, drums, true);
    at += 4;
    put(&mut p, ending, lead_t, at, 2);
    p
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    songkit::finish(&build(), &dir, "The Two-Week Reel");
}
