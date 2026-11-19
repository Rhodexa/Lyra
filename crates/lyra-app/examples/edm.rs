//! "Lyra Ascendant": heroic, high-stakes electro / dubstep, written by Claude
//! with Lyra. Almost every sound is Fluttersine, started from a preset and
//! pushed somewhere.
//!
//!     cargo run --release -p lyra-app --example edm -- <out dir>
//!     cargo run --release -p lyra-app -- --render "<out dir>/<title>.lyra"

mod songkit;
use lyra_fluttersine::p::{self as fm, matrix, op, out};
use lyra_fluttersine::{DEST_FM_IN, DEST_WARP, SYNC_LABELS};
use lyra_model::{ChannelId, Clip, Note, Project, Scale};
use songkit::*;

const BPM: f64 = 140.0;
/// LFO rates locked to the tempo.
const HZ_4: f32 = (BPM / 60.0) as f32; // quarter
const HZ_8: f32 = HZ_4 * 2.0;
const HZ_8T: f32 = HZ_4 * 3.0;
const HZ_16: f32 = HZ_4 * 4.0;

/// A Fluttersine LFO sync choice by its label ("1/8", "1 bar"…).
fn sync(label: &str) -> f32 {
    SYNC_LABELS.iter().position(|l| *l == label).expect("sync label") as f32
}

// ───────────────────────────── harmony ─────────────────────────────

const MAIN: [&str; 8] = ["Fm", "Db", "Ab", "Eb", "Fm", "Db", "Bbm", "C"];
const VERSE: [&str; 8] = ["Fm", "Eb", "Db", "C", "Fm", "Eb", "Db", "C"];
/// Over a held C in the bass: the dominant pedal that makes a build ache.
const BUILD: [&str; 8] = ["C", "Db", "Bbm", "C", "C", "Db", "Bbm", "C"];

/// (pitch class of the root, the triad as semitones)
fn chord(name: &str) -> (i32, [i32; 3]) {
    match name {
        "Fm" => (5, [0, 3, 7]),
        "Db" => (1, [0, 4, 7]),
        "Ab" => (8, [0, 4, 7]),
        "Eb" => (3, [0, 4, 7]),
        "Bbm" => (10, [0, 3, 7]),
        "C" => (0, [0, 4, 7]),
        _ => panic!("chord {name}"),
    }
}

/// Bass root between C2 and B2.
fn root(name: &str) -> i32 {
    36 + chord(name).0
}

/// The chord's tones, each in the octave nearest `center` (a close voicing).
fn voicing(name: &str, center: i32) -> Vec<i32> {
    let (r, tri) = chord(name);
    let mut v: Vec<i32> = tri
        .iter()
        .map(|&i| {
            let pc = (r + i) % 12;
            let mut k = center - 6 + (pc - (center - 6)).rem_euclid(12);
            if k >= center + 6 {
                k -= 12;
            }
            k
        })
        .collect();
    v.sort();
    v
}

fn n(start: u32, len: u32, key: i32, velocity: f32) -> Note {
    Note { start, len, key: key.clamp(0, 127) as u8, velocity: velocity.clamp(0.05, 1.0) }
}

// ───────────────────────────── the theme ─────────────────────────────
// F minor; one token = a 16th.

const THEME: [&str; 8] = [
    "F---C---F-G-A---",
    "A-----G-F---D---",
    "E---C---E-F-A-c-",
    "B-------G---B---",
    "c---F---c-d-e---",
    "f-----e-d---A---",
    "d---B---d-e-f---",
    "g-------=e-------",
];
/// The theme's last bar when it lands home for good.
const THEME_END: &str = "g---=e---g---c'---";

/// The verse's chip riff (over VERSE).
const RIFF: [&str; 4] = ["c-A-F-A-c-f-c-A-", "B-G-E-G-B-e-B-G-", "A-F-D-F-A-d-A-F-", "G-=E-C-=E-G-c-=B-c-"];
const RIFF_TURN: &str = "G-c-=e-g-=b-g-=e-c-";

fn line(bars: &[&str], shift: i32, gap: u32, vel: f32) -> Vec<Note> {
    let mut v = Vec::new();
    for (b, s) in bars.iter().enumerate() {
        for t in parse(s, F_MINOR) {
            let accent = if t.step % 4 == 0 { 1.0 } else { 0.85 };
            v.push(n(b as u32 * BAR + t.step * STEP, t.len * STEP - gap, t.key + shift, vel * accent));
        }
    }
    v
}

/// Under each melody note, the nearest chord tone at least a minor third below.
fn harmony(notes: &[Note], prog: &[&str]) -> Vec<Note> {
    notes
        .iter()
        .map(|m| {
            let (r, tri) = chord(prog[(m.start / BAR) as usize % prog.len()]);
            let key = (1..=9)
                .map(|d| m.key as i32 - 2 - d)
                .find(|k| tri.iter().any(|i| (r + i) % 12 == k.rem_euclid(12)))
                .unwrap_or(m.key as i32 - 5);
            n(m.start, m.len, key, m.velocity * 0.8)
        })
        .collect()
}

// ───────────────────────────── parts ─────────────────────────────

/// Bass rhythms: R root, O octave up, F fifth, - hold, . rest.
fn bass(prog: &[&str], rhythms: &[&str], pedal: Option<&str>, vel: f32) -> Vec<Note> {
    let mut v = Vec::new();
    for (b, c) in prog.iter().enumerate() {
        let r = root(pedal.unwrap_or(c));
        let mut last: Option<usize> = None;
        for (s, ch) in rhythms[b % rhythms.len()].chars().enumerate() {
            let at = b as u32 * BAR + s as u32 * STEP;
            let key = match ch {
                'R' => r,
                'O' => r + 12,
                'F' => r + 7,
                '-' => {
                    if let Some(i) = last {
                        let note: &mut Note = &mut v[i];
                        note.len += STEP;
                    }
                    continue;
                }
                _ => {
                    last = None;
                    continue;
                }
            };
            v.push(n(at, STEP - 4, key, vel * if s % 4 == 0 { 1.0 } else { 0.85 }));
            last = Some(v.len() - 1);
        }
    }
    v
}

/// One held chord per bar (`cut` steps shorter at the end of the last bar).
fn pads(prog: &[&str], center: i32, vel: f32, cut: u32) -> Vec<Note> {
    let mut v = Vec::new();
    for (b, c) in prog.iter().enumerate() {
        let len = if b + 1 == prog.len() { BAR - cut * STEP } else { BAR - 6 };
        for k in voicing(c, center) {
            v.push(n(b as u32 * BAR, len, k, vel));
        }
    }
    v
}

/// Chords on chosen steps of every bar, each `len` steps long.
fn stabs(prog: &[&str], center: i32, steps: &[u32], len: u32, vel: f32) -> Vec<Note> {
    let mut v = Vec::new();
    for (b, c) in prog.iter().enumerate() {
        for &s in steps {
            for k in voicing(c, center) {
                v.push(n(b as u32 * BAR + s * STEP, len * STEP - 6, k, vel * if s % 4 == 0 { 1.0 } else { 0.85 }));
            }
        }
    }
    v
}

/// Chip arpeggio: root, third, fifth, octave, flicked through every `every` ticks.
fn arp(prog: &[&str], center: i32, every: u32, vel: f32) -> Vec<Note> {
    let mut v = Vec::new();
    for (b, c) in prog.iter().enumerate() {
        let t = voicing(c, center);
        let tones = [t[0], t[1], t[2], t[0] + 12];
        for k in 0..BAR / every {
            // One tick of overlap: on a mono voice that's legato, so the pitch
            // steps without restarting the envelope (a tracker arp, not a stutter).
            let key = tones[k as usize % 4];
            v.push(n(b as u32 * BAR + k * every, every + 1, key, vel));
        }
    }
    v
}

/// Bell arpeggio in 8ths, up and back down over two octaves.
fn bells(prog: &[&str], center: i32, vel: f32) -> Vec<Note> {
    let mut v = Vec::new();
    for (b, c) in prog.iter().enumerate() {
        let t = voicing(c, center);
        let tones = [t[0], t[1], t[2], t[0] + 12, t[1] + 12, t[2] + 12, t[0] + 24, t[2] + 12];
        for (i, k) in tones.iter().enumerate() {
            v.push(n(b as u32 * BAR + i as u32 * 2 * STEP, 2 * STEP, *k, vel * if i == 0 { 1.0 } else { 0.8 }));
        }
    }
    v
}

fn hits(bar: u32, steps: &[(u32, f32)]) -> Vec<Note> {
    steps.iter().map(|&(s, vel)| n(bar * BAR + s * STEP, STEP, 60, vel)).collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Beat {
    /// Hats only.
    Hats,
    /// Kick on every beat, clap on 2 and 4.
    Four,
    /// Half time: kick on 1, snare on 3.
    Half,
}

struct Kit {
    kick: ChannelId,
    clap: ChannelId,
    snare: ChannelId,
    hat: ChannelId,
    open: ChannelId,
}

/// `bars` of a beat; `fills` lists which bars (0-based) end in a snare fill.
fn beat(k: &Kit, bars: u32, kind: Beat, fills: &[u32], open_hats: bool) -> Vec<(ChannelId, Vec<Note>)> {
    let (mut kick, mut clap, mut snare, mut hat, mut open) = (vec![], vec![], vec![], vec![], vec![]);
    for b in 0..bars {
        let fill = fills.contains(&b);
        let half = if fill { 8 } else { 16 }; // where the fill takes over
        match kind {
            Beat::Hats => {}
            Beat::Four => {
                let before = |v: Vec<Note>| v.into_iter().filter(|x| x.start < b * BAR + half * STEP);
                kick.extend(before(hits(b, &[(0, 1.0), (4, 1.0), (8, 1.0), (12, 1.0)])));
                clap.extend(before(hits(b, &[(4, 1.0), (12, 1.0)])));
                snare.extend(before(hits(b, &[(4, 0.7), (12, 0.7)])));
            }
            Beat::Half => {
                let ghost = if b % 2 == 1 {
                    vec![(0, 1.0), (3, 0.6), (10, 0.85)]
                } else {
                    vec![(0, 1.0), (10, 0.85), (13, 0.5)]
                };
                kick.extend(hits(b, &ghost.into_iter().filter(|(s, _)| *s < half).collect::<Vec<_>>()));
                if !fill {
                    clap.extend(hits(b, &[(8, 1.0)]));
                    snare.extend(hits(b, &[(8, 0.9), (15, 0.25)]));
                }
            }
        }
        // Hats: 16ths with the off-beat 8ths opened up.
        for s in 0..half {
            if kind == Beat::Hats && s % 2 == 1 {
                continue;
            }
            if open_hats && s % 4 == 2 {
                open.extend(hits(b, &[(s, 0.7)]));
            } else {
                let vel = if s % 4 == 2 {
                    0.8
                } else if s % 2 == 0 {
                    0.6
                } else {
                    0.4
                };
                hat.extend(hits(b, &[(s, vel)]));
            }
        }
        if fill {
            for s in 8..16 {
                snare.push(n(b * BAR + s * STEP, STEP, 60, 0.45 + 0.07 * (s - 8) as f32));
                if s % 2 == 0 {
                    snare.push(n(b * BAR + s * STEP + 12, 12, 60, 0.4 + 0.07 * (s - 8) as f32));
                }
            }
            kick.extend(hits(b, &[(8, 0.9), (14, 0.9)]));
            clap.extend(hits(b, &[(15, 1.0)]));
        }
    }
    vec![(k.kick, kick), (k.clap, clap), (k.snare, snare), (k.hat, hat), (k.open, open)]
}

/// An 8-bar build: a snare roll that keeps doubling, kick under the first
/// part, and the last beat left empty before the drop.
fn roll(k: &Kit, every: [f32; 8]) -> Vec<(ChannelId, Vec<Note>)> {
    let (mut kick, mut snare, mut clap) = (vec![], vec![], vec![]);
    let total = 8 * BAR - 4 * STEP; // the silent beat
    for b in 0..8u32 {
        let gap = (every[b as usize] * STEP as f32) as u32;
        let mut t = b * BAR;
        while t < (b + 1) * BAR && t < total {
            let ramp = t as f32 / total as f32;
            snare.push(n(t, gap.min(STEP), 60, 0.3 + 0.7 * ramp));
            if every[b as usize] >= 2.0 {
                clap.push(n(t, STEP, 60, 0.3 + 0.6 * ramp));
            }
            t += gap;
        }
        if b < 6 {
            kick.extend(hits(b, &[(0, 1.0), (4, 1.0), (8, 1.0), (12, 1.0)]));
        }
    }
    vec![(k.kick, kick), (k.snare, snare), (k.clap, clap)]
}

// ───────────────────────────── the song ─────────────────────────────

fn build() -> Project {
    let mut p = Project::default();
    p.bpm = BPM;
    p.key_root = 5; // F
    p.scale = Scale::Minor;
    p.master_volume = 1.0; // after the master chain: the limiter sets the final level
    let fs = "lyra.fluttersine";
    const NOISE: f32 = 10.0;

    // Drums (Lyra's drum synths) and an impact made of a detuned-down kick.
    let kick = p.add_channel(
        "Kick",
        "lyra.kick",
        plugin("lyra.kick", &[(0, 50.0), (1, 7.0), (2, 0.04), (3, 0.4), (4, 0.3), (5, 0.3)]),
    );
    let clap = p.add_channel("Clap", "lyra.clap", plugin("lyra.clap", &[]));
    let snare =
        p.add_channel("Snare", "lyra.snare", plugin("lyra.snare", &[(0, 200.0), (1, 0.15), (2, 0.8), (3, 0.25)]));
    let hat = p.add_channel("Hat", "lyra.hihat", plugin("lyra.hihat", &[(1, 0.04), (2, 9000.0)]));
    let open = p.add_channel("Open hat", "lyra.hihat", plugin("lyra.hihat", &[(1, 0.28), (2, 7000.0)]));
    let boom = p.add_channel(
        "Boom",
        "lyra.kick",
        plugin("lyra.kick", &[(0, 32.0), (1, 9.0), (2, 0.2), (3, 1.5), (4, 0.8), (5, 0.7)]),
    );
    // FX: white-noise crash that darkens as it rings, a noise riser that
    // opens its filter over 10 seconds, and a supersaw that glides upward.
    let crash = p.add_channel(
        "Crash",
        fs,
        fm(&[
            (op(0, fm::OP_WAVE), NOISE),
            (op(0, fm::OP_DECAY), 3.0),
            (op(0, fm::OP_SUSTAIN), 0.0),
            (op(0, fm::OP_RELEASE), 2.0),
            (fm::LP_ON, 1.0),
            (fm::CUTOFF, 1500.0),
            (fm::F_ENV, 3.0),
            (fm::F_ATTACK, 0.001),
            (fm::F_DECAY, 1.5),
            (fm::F_SUSTAIN, 0.0),
            (fm::VOLUME, fm_volume(0.75)),
        ]),
    );
    let riser = p.add_channel(
        "Noise riser",
        fs,
        fm(&[
            (op(0, fm::OP_WAVE), NOISE),
            (op(0, fm::OP_ATTACK), 8.0),
            (op(0, fm::OP_RELEASE), 0.05),
            (fm::LP_ON, 1.0),
            (fm::CUTOFF, 150.0),
            (fm::RESO, 0.45),
            (fm::F_ENV, 6.0),
            (fm::F_ATTACK, 10.0),
            (fm::F_SUSTAIN, 1.0),
            (fm::VOLUME, fm_volume(0.6)),
        ]),
    );
    let lift = p.add_channel(
        "Pitch riser",
        fs,
        fm_preset(
            "Supersaw Chords",
            &[
                (fm::MODE, 1.0),
                (fm::GLIDE, 1.6),
                (op(0, fm::OP_ATTACK), 2.0),
                (op(1, fm::OP_ATTACK), 2.0),
                (fm::VOLUME, fm_volume(0.4)),
            ],
        ),
    );

    // Bass: a sine sub under everything, and FM growls above it (high-passed
    // so they never fight the sub). Each growl's LFO is locked to the tempo.
    let sub = p.add_channel("Sub", fs, fm_preset("Sub Bass", &[(fm::VOLUME, fm_volume(0.8))]));
    let growl8 = p.add_channel(
        "Growl 1/8",
        fs,
        fm_preset(
            "Feedback Growl",
            &[
                (fm::LFO_RATE, HZ_8),
                (fm::LFO_SYNC, sync("1/8")),
                (fm::LFO_RETRIG, 0.0), // locked to the song, not to each note
                (fm::mod_src(0), 1.0), // LFO 1 → Op 1 FM in: the vowel opens and closes
                (fm::mod_dest(0), DEST_FM_IN as f32),
                (fm::mod_amt(0), 0.7),
                (fm::LFO2_SYNC, sync("1 bar")),
                (fm::LFO2_SHAPE, 2.0), // saw over the bar: the snarl drifts
                (fm::mod_src(1), 2.0),
                (fm::mod_dest(1), (DEST_WARP + 1) as f32),
                (fm::mod_amt(1), 0.6),
                (matrix(1, 0), 0.28), // op 2 (half speed) bends op 1: a hollow, vowel-ish snarl
                (op(1, fm::OP_PWM), 0.6),
                (fm::RESO, 0.55),
                (fm::F_LFO, 1.2),
                (fm::HP_ON, 1.0),
                (fm::HP_CUTOFF, 110.0),
                (out(0), 1.0),
                (out(1), 0.85),
                (fm::VOLUME, fm_volume(0.85)),
            ],
        ),
    );
    let growl16 = p.add_channel(
        "Growl 1/16",
        fs,
        fm_preset(
            "Feedback Growl",
            &[
                (fm::LFO_RATE, HZ_16),
                (fm::LFO_SHAPE, 3.0), // square: chops instead of sweeps
                (fm::LFO_SYNC, sync("1/16")),
                (fm::LFO_RETRIG, 0.0),
                (fm::mod_src(0), 1.0), // LFO 1 → all FM: bright/dark on every 16th
                (fm::mod_dest(0), 4.0),
                (fm::mod_amt(0), 0.5),
                (fm::env(0, fm::ENV_DECAY), 0.12),
                (fm::mod_src(1), 3.0), // Env 1 → pitch: a little drop into each note
                (fm::mod_dest(1), 1.0),
                (fm::mod_amt(1), 0.25),
                (op(1, fm::OP_RATIO), 2.0),
                (matrix(1, 0), 0.4),
                (matrix(0, 0), 0.6),
                (fm::RESO, 0.45),
                (fm::F_LFO, 1.0),
                (fm::HP_ON, 1.0),
                (fm::HP_CUTOFF, 110.0),
                (out(0), 1.0),
                (out(1), 0.85),
                (fm::VOLUME, fm_volume(0.8)),
            ],
        ),
    );
    let yoi = p.add_channel(
        "Yoi 1/8T",
        fs,
        fm_preset(
            "Supersaw Wobble",
            &[
                (fm::MODE, 1.0),
                (fm::LFO_RATE, HZ_8T),
                (fm::LFO_SHAPE, 1.0),
                (fm::LFO_SYNC, sync("1/8 T")),
                (fm::LFO_RETRIG, 0.0),
                (fm::mod_src(0), 1.0),
                (fm::mod_dest(0), DEST_FM_IN as f32),
                (fm::mod_amt(0), 0.5),
                (fm::RESO, 0.6),
                (fm::F_LFO, 2.0),
                (matrix(1, 0), 0.2),
                (fm::HP_ON, 1.0),
                (fm::HP_CUTOFF, 110.0),
                (out(0), 1.0),
                (out(1), 0.85),
                (fm::VOLUME, fm_volume(0.75)),
            ],
        ),
    );
    let reese = p.add_channel(
        "Reese",
        fs,
        fm_preset("Reese Bass", &[(fm::F_LFO, 1.2), (fm::LFO_RATE, 0.4), (fm::VOLUME, fm_volume(0.75))]),
    );
    // The verse's FM pluck bass: the modulator dies fast, so each note starts
    // bright and closes to a sine.
    let pluck = p.add_channel(
        "FM pluck bass",
        fs,
        fm(&[
            (fm::MODE, 1.0),
            (fm::GLIDE, 0.0),
            (op(0, fm::OP_DECAY), 0.3),
            (op(0, fm::OP_SUSTAIN), 0.6),
            (op(0, fm::OP_RELEASE), 0.05),
            (op(1, fm::OP_RATIO), 1.0),
            (op(1, fm::OP_DECAY), 0.12),
            (op(1, fm::OP_SUSTAIN), 0.0),
            (op(1, fm::OP_VEL), 0.9),
            (matrix(1, 0), 0.6),
            (matrix(1, 1), 0.3),
            (fm::VOLUME, fm_volume(0.8)),
        ]),
    );

    // Harmony and melody.
    let pad = p.add_channel("Strings", fs, fm_preset("PWM Strings", &[(fm::VOLUME, fm_volume(0.5))]));
    let keys = p.add_channel("E-piano", fs, fm_preset("FM E-Piano", &[(fm::VOLUME, fm_volume(0.6))]));
    let bell = p.add_channel("Bells", fs, fm_preset("FM Bell", &[(fm::VOLUME, fm_volume(0.45))]));
    let brass = p.add_channel(
        "FM brass",
        fs,
        fm_preset(
            "FM Brass",
            &[
                // a second, detuned brass voice on op 3/4 for width
                (op(2, fm::OP_FINE), 9.0),
                (op(2, fm::OP_ATTACK), 0.05),
                (op(2, fm::OP_DECAY), 0.6),
                (op(2, fm::OP_SUSTAIN), 0.8),
                (op(2, fm::OP_RELEASE), 0.2),
                (op(3, fm::OP_ATTACK), 0.1),
                (op(3, fm::OP_DECAY), 0.8),
                (op(3, fm::OP_SUSTAIN), 0.5),
                (op(3, fm::OP_RELEASE), 0.25),
                (matrix(3, 2), 0.5),
                (out(2), 0.8),
                (fm::VIBRATO, 14.0),
                (fm::LFO_RATE, 5.0),
                (fm::VOLUME, fm_volume(0.6)),
            ],
        ),
    );
    let lead = p.add_channel(
        "Supersaw lead",
        fs,
        fm_preset(
            "Supersaw Chords",
            &[
                (op(0, fm::OP_SPREAD), 0.5),
                (fm::F_ENV, 1.5),
                (fm::F_DECAY, 0.35),
                (fm::F_SUSTAIN, 0.6),
                (fm::CUTOFF, 3500.0),
                (fm::VIBRATO, 10.0),
                (fm::LFO_RATE, 5.5),
                (fm::VOLUME, fm_volume(0.55)),
            ],
        ),
    );
    let stab = p.add_channel(
        "Saw stabs",
        fs,
        fm_preset(
            "Supersaw Chords",
            &[
                (op(0, fm::OP_DECAY), 0.22),
                (op(0, fm::OP_SUSTAIN), 0.25),
                (op(1, fm::OP_DECAY), 0.22),
                (op(1, fm::OP_SUSTAIN), 0.25),
                (op(0, fm::OP_RELEASE), 0.15),
                (op(1, fm::OP_RELEASE), 0.15),
                (fm::VOLUME, fm_volume(0.5)),
            ],
        ),
    );
    let chip = p.add_channel(
        "Chip lead",
        fs,
        fm_preset("Chip Lead 25% (Mono Glide)", &[(fm::GLIDE, 0.02), (fm::VOLUME, fm_volume(0.55))]),
    );
    let chiparp = p.add_channel(
        "Chip arp",
        fs,
        fm_preset("Chip Pulse 12.5%", &[(fm::VOLUME, fm_volume(0.4)), (fm::MODE, 1.0), (fm::GLIDE, 0.0)]),
    );

    // ── mixer ──
    let routes: [(ChannelId, usize, f32, f32); 23] = [
        (kick, 1, 0.9, 0.0),
        (clap, 1, 0.7, 0.0),
        (snare, 1, 0.6, 0.0),
        (hat, 1, 0.5, 0.25),
        (open, 1, 0.45, -0.25),
        (boom, 2, 0.9, 0.0),
        (crash, 2, 0.7, 0.0),
        (riser, 2, 0.7, 0.0),
        (lift, 2, 0.7, 0.0),
        (sub, 3, 0.85, 0.0),
        (growl8, 4, 0.8, 0.0),
        (growl16, 4, 0.8, 0.0),
        (yoi, 4, 0.8, 0.0),
        (reese, 4, 0.8, 0.0),
        (pluck, 4, 0.8, 0.0),
        (pad, 5, 0.7, 0.0),
        (keys, 5, 0.7, -0.15),
        (bell, 7, 0.7, 0.3),
        (brass, 8, 0.8, -0.1),
        (lead, 6, 0.8, 0.0),
        (stab, 5, 0.7, 0.1),
        (chip, 7, 0.75, -0.2),
        (chiparp, 7, 0.65, 0.35),
    ];
    for (id, track, vol, pan) in routes {
        let c = p.channel_mut(id).unwrap();
        (c.mixer, c.volume, c.pan) = (track, vol, pan);
    }
    p.channel_mut(hat).unwrap().choke = 1;
    p.channel_mut(open).unwrap().choke = 1;
    let names = ["Master", "Drums", "FX", "Sub", "Bass", "Chords", "Lead", "Chip", "Brass"];
    for (t, name) in names.iter().enumerate() {
        p.mixer[t].name = (*name).into();
    }
    let pump = |depth: f32| fx("lyra.pump", &[(0, depth), (1, 0.5), (2, 0.0)]);
    p.mixer[0].effects = vec![
        fx("lyra.compressor", &[(0, -8.0), (1, 2.0), (2, 0.01), (3, 0.2), (4, 1.0)]),
        fx("lyra.limiter", &[(0, 2.0), (1, -0.3), (2, 0.08)]),
    ];
    p.mixer[1].effects = vec![fx("lyra.compressor", &[(0, -14.0), (1, 3.0), (2, 0.008), (3, 0.12), (4, 3.0)])];
    p.mixer[2].effects = vec![fx("lyra.reverb", &[(0, 0.85), (3, 0.3)])];
    p.mixer[3].effects = vec![pump(0.55)];
    p.mixer[4].volume = 1.2; // the growls are the show
    p.mixer[4].effects = vec![
        fx("lyra.drive", &[(0, 0.0), (1, 9.0), (2, 9000.0), (3, 0.6), (4, -4.0)]),
        pump(0.3),
        fx("lyra.eq", &[(2, 5.0), (3, 1000.0), (4, 3.0)]),
    ];
    p.mixer[5].effects =
        vec![pump(0.7), fx("lyra.chorus", &[(0, 0.4), (1, 0.6), (3, 0.4)]), fx("lyra.reverb", &[(0, 0.7), (3, 0.25)])];
    p.mixer[6].effects =
        vec![fx("lyra.delay", &[(0, 2.0), (1, 0.3), (4, 0.18)]), fx("lyra.reverb", &[(0, 0.75), (3, 0.22)])];
    p.mixer[7].effects = vec![
        fx("lyra.chorus", &[(0, 0.8), (1, 0.4), (3, 0.35)]),
        fx("lyra.delay", &[(0, 1.0), (1, 0.35), (4, 0.22)]),
        fx("lyra.reverb", &[(0, 0.6), (3, 0.2)]),
    ];
    p.mixer[8].effects = vec![fx("lyra.reverb", &[(0, 0.8), (3, 0.3)])];

    // ── playlist ──
    let tracks = ["Drums", "FX", "Sub", "Bass", "Chords", "Keys", "Lead", "Chip", "Bells", "Brass"];
    for (t, name) in tracks.iter().enumerate() {
        p.tracks[t].name = (*name).into();
    }
    let (t_drums, t_fx, t_sub, t_bass, t_chords, t_keys, t_lead, t_chip, t_bells, t_brass) =
        (0, 1, 2, 3, 4, 5, 6, 7, 8, 9);
    p.patterns.clear();
    let kit = Kit { kick, clap, snare, hat, open };

    // Put `notes` (ticks from `at`) in a new pattern of `bars`, as one clip.
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
    let sub_line = |prog: &[&str], pedal: Option<&str>, cut: u32| -> Vec<Note> {
        prog.iter()
            .enumerate()
            .map(|(b, c)| {
                let len = if b + 1 == prog.len() { BAR - cut * STEP } else { BAR - 8 };
                n(b as u32 * BAR, len, root(pedal.unwrap_or(c)) - 12, 1.0)
            })
            .collect()
    };
    let impact = |p: &mut Project, at: u32, name: &str| {
        put(p, t_fx, name, at, 2, vec![(boom, hits(0, &[(0, 1.0)])), (crash, hits(0, &[(0, 1.0)]))]);
    };
    let risers = |p: &mut Project, at: u32| {
        let ramp: Vec<Note> = [48, 53, 55, 60, 65, 67, 72, 77]
            .iter()
            .enumerate()
            .map(|(b, &k)| n(b as u32 * BAR, if b == 7 { BAR - 4 * STEP } else { BAR + 24 }, k, 0.6 + 0.05 * b as f32))
            .collect();
        put(p, t_fx, "Risers", at, 8, vec![(riser, vec![n(0, 8 * BAR - 4 * STEP, 60, 0.9)]), (lift, ramp)]);
    };

    // 1 · Intro (bars 0–7): strings and bells over the theme's chords, then
    // the brass hints at the first half of the theme.
    let mut at = 0;
    impact(&mut p, at, "Impact");
    put(&mut p, t_chords, "Intro strings", at, 8, vec![(pad, pads(&MAIN, 62, 0.7, 0))]);
    put(&mut p, t_bells, "Intro bells", at, 8, vec![(bell, bells(&MAIN, 62, 0.7))]);
    put(&mut p, t_brass, "Theme hint", at + 4, 4, vec![(brass, line(&THEME[..4], 0, 6, 0.75))]);
    put(&mut p, t_sub, "Intro sub", at + 4, 4, vec![(sub, sub_line(&MAIN[4..], None, 0))]);
    put(&mut p, t_drums, "Intro hats", at + 4, 4, beat(&kit, 4, Beat::Hats, &[3], false));
    at += 8;

    // 2 · Verse (8–15): four on the floor, FM pluck bass, the chip riff.
    let verse_bass = ["R.ROR.RFR.ROR.RO", "R.ROR.RFR.RORFRO"];
    put(
        &mut p,
        t_drums,
        "Verse beat",
        at,
        8,
        beat(&kit, 8, Beat::Four, &[3, 7], false)
            .into_iter()
            .map(|(c, mut v)| {
                if c == open {
                    v.clear()
                }
                v.iter_mut().for_each(|x| x.velocity *= 0.8); // the verse holds back
                (c, v)
            })
            .collect(),
    );
    put(&mut p, t_bass, "Verse pluck bass", at, 8, vec![(pluck, bass(&VERSE, &verse_bass, None, 0.7))]);
    put(&mut p, t_sub, "Verse sub", at + 4, 4, vec![(sub, sub_line(&VERSE[4..], None, 0))]);
    put(&mut p, t_chords, "Verse strings", at, 8, vec![(pad, pads(&VERSE, 60, 0.55, 0))]);
    let mut riff: Vec<&str> = RIFF.to_vec();
    riff.extend(&RIFF[..3]);
    riff.push(RIFF_TURN);
    put(&mut p, t_chip, "Chip riff", at, 8, vec![(chip, line(&riff, 0, 4, 0.75))]);
    put(&mut p, t_chip, "Chip arp", at + 4, 4, vec![(chiparp, arp(&VERSE[4..], 72, 8, 0.7))]);
    at += 8;

    // Builds start quiet and grow.
    let swell = |mut x: Note| {
        x.velocity *= 0.5 + 0.5 * x.start as f32 / (8 * BAR) as f32;
        x
    };
    // 3 · Build (16–23): everything leans on C; rolls double; risers.
    put(&mut p, t_drums, "Build roll", at, 8, roll(&kit, [4.0, 4.0, 4.0, 4.0, 2.0, 2.0, 1.0, 0.5]));
    put(
        &mut p,
        t_bass,
        "Build reese",
        at,
        8,
        vec![(
            reese,
            bass(&BUILD, &["R-------R-------", "R-------R---O---"], Some("C"), 0.9)
                .into_iter()
                .filter(|x| x.start < 8 * BAR - 4 * STEP)
                .map(swell)
                .collect(),
        )],
    );
    put(&mut p, t_sub, "Build sub", at + 4, 4, vec![(sub, sub_line(&BUILD[4..], Some("C"), 4))]);
    put(&mut p, t_chords, "Build strings", at, 8, vec![(pad, pads(&BUILD, 64, 0.7, 4))]);
    put(
        &mut p,
        t_chip,
        "Build arp",
        at + 4,
        4,
        vec![(chiparp, arp(&BUILD[4..], 76, 8, 0.8).into_iter().filter(|x| x.start < 4 * BAR - 4 * STEP).collect())],
    );
    put(
        &mut p,
        t_lead,
        "Build theme",
        at + 4,
        4,
        vec![(lead, line(&THEME[4..], 0, 6, 0.8).into_iter().filter(|x| x.start < 4 * BAR - 4 * STEP).collect())],
    );
    risers(&mut p, at);
    at += 8;

    // 4 · Drop 1 (24–39): half time. The growls trade bars; the second
    // eight bars bring the theme in on the supersaw, an octave up.
    impact(&mut p, at, "Impact");
    let drop_rhythms_a = ["R-------O---R-F-", "R--R--O-R---O-F-", "R-------O---R-F-", "R-.RR-.RO-.OR---"];
    let drop_rhythms_b = ["R---------------", "R--R--O-R---O-F-", "R-------O---R---", "R-.RR-.RO-.OR-O-"];
    let mut drop1 = [MAIN, MAIN].concat();
    drop1.truncate(16);
    // Which growl plays each bar.
    let who = [
        growl8, growl8, growl8, growl16, growl8, yoi, growl8, growl16, growl8, growl8, yoi, growl16, growl8, yoi,
        growl16, growl16,
    ];
    let mut lines: Vec<(ChannelId, Vec<Note>)> = vec![(growl8, vec![]), (growl16, vec![]), (yoi, vec![])];
    for (b, ch) in who.iter().enumerate() {
        let rh = if b < 8 { drop_rhythms_a[b % 4] } else { drop_rhythms_b[b % 4] };
        let notes = bass(&[drop1[b]], &[rh], None, 1.0).into_iter().map(|mut x| {
            x.start += b as u32 * BAR;
            x
        });
        lines.iter_mut().find(|(c, _)| c == ch).unwrap().1.extend(notes);
    }
    put(&mut p, t_bass, "Drop growls", at, 16, lines);
    put(&mut p, t_sub, "Drop sub", at, 16, vec![(sub, sub_line(&drop1, None, 0))]);
    put(&mut p, t_drums, "Half-time beat", at, 16, beat(&kit, 16, Beat::Half, &[7, 15], true));
    put(&mut p, t_keys, "Drop strings", at, 16, vec![(pad, pads(&drop1, 55, 0.5, 0))]);
    put(&mut p, t_chords, "Drop stabs", at, 16, vec![(stab, stabs(&drop1, 64, &[0, 3, 6, 10], 2, 0.8))]);
    let theme_hi = line(&THEME, 12, 6, 0.9);
    put(&mut p, t_lead, "Theme (high)", at + 8, 8, vec![(lead, theme_hi.clone())]);
    put(&mut p, t_chip, "Drop arp", at + 12, 4, vec![(chiparp, arp(&MAIN[4..], 79, 8, 0.6))]);
    impact(&mut p, at + 8, "Impact");
    at += 16;

    // 5 · Breakdown (40–47): the theme in full on the FM brass, e-piano and
    // bells under it; a heartbeat kick brings the energy back.
    impact(&mut p, at, "Impact");
    put(&mut p, t_brass, "Theme (brass)", at, 8, vec![(brass, line(&THEME, 0, 6, 0.85))]);
    put(&mut p, t_keys, "E-piano", at, 8, vec![(keys, stabs(&MAIN, 60, &[0, 6, 10], 4, 0.7))]);
    put(&mut p, t_chords, "Breakdown strings", at, 8, vec![(pad, pads(&MAIN, 55, 0.6, 0))]);
    put(&mut p, t_bells, "Breakdown bells", at + 4, 4, vec![(bell, bells(&MAIN[4..], 67, 0.55))]);
    put(&mut p, t_sub, "Breakdown sub", at + 4, 4, vec![(sub, sub_line(&MAIN[4..], None, 0))]);
    put(
        &mut p,
        t_drums,
        "Heartbeat",
        at + 4,
        4,
        vec![(kick, (0..4).flat_map(|b| hits(b, &[(0, 0.8), (3, 0.5)])).collect())],
    );
    at += 8;

    // 6 · Build 2 (48–55): faster rolls from the start; the lead climbs.
    put(&mut p, t_drums, "Build roll 2", at, 8, roll(&kit, [2.0, 2.0, 2.0, 2.0, 1.0, 1.0, 0.5, 0.5]));
    put(
        &mut p,
        t_bass,
        "Build reese 2",
        at,
        8,
        vec![(
            reese,
            bass(&BUILD, &["R---R---R---R---", "R---R---O---O---"], Some("C"), 0.9)
                .into_iter()
                .filter(|x| x.start < 8 * BAR - 4 * STEP)
                .map(swell)
                .collect(),
        )],
    );
    put(&mut p, t_sub, "Build sub", at + 4, 4, vec![(sub, sub_line(&BUILD[4..], Some("C"), 4))]);
    put(
        &mut p,
        t_chords,
        "Build stabs",
        at,
        8,
        vec![(
            stab,
            stabs(&BUILD, 67, &[0, 2, 4, 6, 8, 10, 12, 14], 1, 0.75)
                .into_iter()
                .filter(|x| x.start < 8 * BAR - 4 * STEP)
                .collect(),
        )],
    );
    put(
        &mut p,
        t_chip,
        "Build arp 2",
        at,
        8,
        vec![(chiparp, arp(&BUILD, 79, 8, 0.75).into_iter().filter(|x| x.start < 8 * BAR - 4 * STEP).collect())],
    );
    put(
        &mut p,
        t_lead,
        "Climb",
        at + 4,
        4,
        vec![(
            lead,
            line(&["c---c---c---c---", "d---d---d---d---", "f---f---f---f---", "g-g-g-g-c'-c'-...."], 0, 6, 0.85),
        )],
    );
    risers(&mut p, at);
    at += 8;

    // 7 · Drop 2 (56–71): four on the floor; the theme on the supersaw with
    // its own harmony, doubled by the brass; growls trade 1/16 and 1/8T.
    impact(&mut p, at, "Impact");
    let mut theme2: Vec<&str> = THEME.to_vec();
    theme2.extend(&THEME[..7]);
    theme2.push(THEME_END);
    let mut drop2 = [MAIN, MAIN].concat();
    drop2.truncate(16);
    let lead2 = line(&theme2, 12, 6, 0.9);
    let mut lead_notes = lead2.clone();
    lead_notes.extend(harmony(&lead2, &drop2));
    put(&mut p, t_lead, "Theme (high, harmony)", at, 16, vec![(lead, lead_notes)]);
    put(&mut p, t_brass, "Theme (brass double)", at + 8, 8, vec![(brass, line(&theme2[8..], 0, 6, 0.8))]);
    put(&mut p, t_drums, "Drop beat", at, 16, beat(&kit, 16, Beat::Four, &[7, 15], true));
    let who2 = [
        growl16, yoi, growl16, growl8, growl16, yoi, growl8, growl16, growl16, yoi, growl16, growl8, yoi, growl16,
        growl8, growl16,
    ];
    let electro = ["R.R.O.R.R.F.O.R.", "R-.RR-.RO-.OR-O-", "R.R.O.R.R.F.O.R.", "R-R-O-O-F-F-O-O-"];
    let mut lines: Vec<(ChannelId, Vec<Note>)> = vec![(growl8, vec![]), (growl16, vec![]), (yoi, vec![])];
    for (b, ch) in who2.iter().enumerate() {
        let notes = bass(&[drop2[b]], &[electro[b % 4]], None, 1.0).into_iter().map(|mut x| {
            x.start += b as u32 * BAR;
            x
        });
        lines.iter_mut().find(|(c, _)| c == ch).unwrap().1.extend(notes);
    }
    put(&mut p, t_bass, "Drop 2 growls", at, 16, lines);
    put(&mut p, t_sub, "Drop sub", at, 16, vec![(sub, sub_line(&drop2, None, 0))]);
    put(&mut p, t_chords, "Drop 2 strings", at, 16, vec![(pad, pads(&drop2, 60, 0.6, 0))]);
    put(&mut p, t_chip, "Drop 2 arp", at, 16, vec![(chiparp, arp(&drop2, 79, 8, 0.6))]);
    impact(&mut p, at + 8, "Impact");
    at += 16;

    // 8 · Outro (72–75): home on F minor, everything at once, then the ring.
    impact(&mut p, at, "Impact");
    let fm_hit = |center: i32, vel: f32| voicing("Fm", center).into_iter().map(move |k| n(0, 2 * BAR, k, vel));
    put(&mut p, t_lead, "Final chord", at, 4, vec![(lead, fm_hit(77, 0.9).chain([n(0, 2 * BAR, 89, 0.8)]).collect())]);
    put(&mut p, t_brass, "Final brass", at, 4, vec![(brass, fm_hit(60, 0.8).collect())]);
    put(
        &mut p,
        t_chords,
        "Final strings",
        at,
        4,
        vec![(
            pad,
            fm_hit(60, 0.7)
                .map(|mut x| {
                    x.len = 3 * BAR;
                    x
                })
                .collect(),
        )],
    );
    put(&mut p, t_sub, "Final sub", at, 4, vec![(sub, vec![n(0, BAR + BAR / 2, 29, 1.0)])]);
    put(
        &mut p,
        t_bells,
        "Final bells",
        at,
        4,
        vec![(
            bell,
            bells(&["Fm", "Fm"], 67, 0.6)
                .into_iter()
                .enumerate()
                .map(|(i, mut x)| {
                    x.velocity *= 1.0 - i as f32 / 20.0;
                    x
                })
                .collect(),
        )],
    );
    put(&mut p, t_drums, "Final hit", at, 4, vec![(clap, hits(0, &[(0, 1.0)])), (snare, hits(0, &[(0, 1.0)]))]);
    // ── levels ── dB trims from soloed-stem peaks (target in the comment).
    let trims = [
        (kick, -1.3),   // -6
        (clap, -4.9),   // -9
        (snare, -5.8),  // -11
        (hat, -5.1),    // -18
        (open, -6.4),   // -18
        (boom, -5.5),   // -6
        (crash, 5.2),   // -14
        (riser, 2.1),   // -14
        (lift, 9.9),    // -16
        (sub, 5.7),     // -10
        (growl8, 7.5),  // -9
        (growl16, 9.3), // -9
        (yoi, 7.7),     // -9
        (reese, 5.7),   // -11
        (pluck, 2.2),   // -11
        (pad, -2.1),    // -15
        (keys, 1.2),    // -13
        (bell, 6.6),    // -15
        (brass, -3.9),  // -11
        (lead, 8.9),    // -8
        (stab, 10.1),   // -14
        (chip, -4.3),   // -11
        (chiparp, 4.4), // -17
    ];
    for (id, db) in trims {
        let c = p.channel_mut(id).unwrap();
        let mut db: f32 = db;
        if c.instrument == "lyra.fluttersine" {
            // Fluttersine's Volume is a gain up to 0 dB; what it can't give goes to the channel.
            let v = &mut c.params[fm::VOLUME];
            let want = *v * 10f32.powf(db / 20.0);
            let got = want.min(1.0);
            db = 20.0 * (want / got).log10();
            *v = got;
        }
        c.volume = (c.volume * 10f32.powf(db / 20.0)).min(1.25);
    }
    p
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    finish(&build(), &dir, "Lyra Ascendant");
}
