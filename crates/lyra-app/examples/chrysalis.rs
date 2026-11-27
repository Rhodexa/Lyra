//! "Metamorphosis": a short demo for Chrysalis, Lyra's wavetable synth.
//! Every melodic sound is a Chrysalis preset (pad, pluck, growl, talking
//! bass, reese, supersaw lead); the drums are Lyra's drum synths.
//!
//!     cargo run --release -p lyra-app --example chrysalis -- <out dir>
//!     cargo run --release -p lyra-app -- --render "<out dir>/Metamorphosis.lyra"

mod songkit;
use lyra_chrysalis::p as cs;
use lyra_model::{ChannelId, Clip, Note, Project, Scale};
use songkit::*;

const BPM: f64 = 140.0;
const CH: &str = "lyra.chrysalis";

/// A Chrysalis preset by name, plus `set` on top.
fn chrys(name: &str, set: &[(usize, f32)]) -> Vec<f32> {
    let p = lyra_catalog::presets_for(CH)
        .into_iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no preset {name}"));
    tweak(p.params(), set)
}

fn n(start: u32, len: u32, key: i32, velocity: f32) -> Note {
    Note { start, len, key: key.clamp(0, 127) as u8, velocity: velocity.clamp(0.05, 1.0), link: None }
}

/// i – VI – III – VII in F minor: (bass root, triad).
const PROG: [(i32, [i32; 3]); 4] = [(41, [0, 3, 7]), (37, [0, 4, 7]), (44, [0, 4, 7]), (39, [0, 4, 7])];

/// One chord per bar for `bars` bars, voiced around `center`.
fn chords(bars: u32, center: i32, vel: f32) -> Vec<Note> {
    (0..bars)
        .flat_map(|b| {
            let (root, tri) = PROG[b as usize % 4];
            tri.map(|i| {
                let pc = (root + i).rem_euclid(12);
                let k = center - 6 + (pc - (center - 6)).rem_euclid(12);
                n(b * BAR, BAR - 6, k, vel)
            })
        })
        .collect()
}

/// 16th arpeggio through each bar's chord, up and down two octaves' worth.
fn arp(bars: u32, center: i32, vel: f32) -> Vec<Note> {
    let mut v = Vec::new();
    for b in 0..bars {
        let (root, tri) = PROG[b as usize % 4];
        let base = center - 6 + (root.rem_euclid(12) - (center - 6)).rem_euclid(12);
        let tones = [base + tri[0], base + tri[1], base + tri[2], base + 12, base + tri[2], base + tri[1]];
        for s in 0..16 {
            let accent = if s % 4 == 0 { 1.0 } else { 0.7 };
            v.push(n(b * BAR + s * STEP, STEP, tones[s as usize % 6], vel * accent));
        }
    }
    v
}

/// The growl's rhythm over one bar (step, length, octave up?), on the root.
const GROWL: [(u32, u32, i32); 7] = [(0, 3, 0), (3, 3, 0), (6, 2, 12), (8, 2, 0), (10, 2, 0), (12, 2, 12), (14, 2, 0)];

fn growls(bars: u32) -> Vec<Note> {
    let mut v = Vec::new();
    for b in 0..bars {
        let root = PROG[b as usize % 4].0 - 12;
        for &(s, len, up) in &GROWL {
            // Every 4th bar ends on a held, rising note instead.
            if b % 4 == 3 && s >= 12 {
                if s == 12 {
                    v.push(n(b * BAR + s * STEP, 4 * STEP, root + 7, 0.95));
                }
                continue;
            }
            v.push(n(b * BAR + s * STEP, len * STEP, root + up, if s == 0 { 1.0 } else { 0.85 }));
        }
    }
    v
}

/// The talking bass: a bar-long note (the LFO speaks over it), then two half bars.
fn talk(bars: u32) -> Vec<Note> {
    let mut v = Vec::new();
    for b in 0..bars {
        let root = PROG[b as usize % 4].0 - 12;
        if b % 2 == 0 {
            v.push(n(b * BAR, BAR - 4, root, 1.0));
        } else {
            v.push(n(b * BAR, 8 * STEP - 4, root, 0.9));
            v.push(n(b * BAR + 8 * STEP, 8 * STEP - 4, root + 12, 0.9));
        }
    }
    v
}

fn reese_line(bars: u32) -> Vec<Note> {
    (0..bars).map(|b| n(b * BAR, BAR - 2, PROG[b as usize % 4].0 - 12, 0.8)).collect()
}

/// The lead melody (one token = a 16th).
const MELODY: [&str; 8] = [
    "c---=B-A-G---F-G-",
    "A-----G-F---E-F-",
    "E---F-G-A---c---",
    "B-------G---E---",
    "c---=B-A-G---F-G-",
    "A-----c-d---e-f-",
    "e---d-c-B---G---",
    "c-------z-------",
];

fn lead(vel: f32) -> Vec<Note> {
    let mut v = Vec::new();
    for (b, bar) in MELODY.iter().enumerate() {
        for t in parse(bar, F_MINOR) {
            v.push(n(b as u32 * BAR + t.step * STEP, t.len * STEP - 6, t.key + 12, vel));
        }
    }
    v
}

/// Off-beat chord stabs (the supersaw in the first drop).
fn stabs(bars: u32, center: i32, vel: f32) -> Vec<Note> {
    let mut v = Vec::new();
    for b in 0..bars {
        let (root, tri) = PROG[b as usize % 4];
        for s in [2u32, 6, 10, 13] {
            for i in tri {
                let pc = (root + i).rem_euclid(12);
                let k = center - 6 + (pc - (center - 6)).rem_euclid(12);
                v.push(n(b * BAR + s * STEP, 2 * STEP, k, vel));
            }
        }
    }
    v
}

struct Kit {
    kick: ChannelId,
    snare: ChannelId,
    clap: ChannelId,
    hat: ChannelId,
}

fn hits(bar: u32, steps: &[(u32, f32)]) -> Vec<Note> {
    steps.iter().map(|&(s, vel)| n(bar * BAR + s * STEP, STEP, 60, vel)).collect()
}

/// Half-time drums with 16th hats; the last bar of each four gets a fill.
fn half_time(k: &Kit, bars: u32) -> Vec<(ChannelId, Vec<Note>)> {
    let (mut kick, mut snare, mut clap, mut hat) = (vec![], vec![], vec![], vec![]);
    for b in 0..bars {
        let fill = b % 4 == 3;
        kick.extend(hits(b, &[(0, 1.0), (10, 0.85)]));
        if b % 2 == 1 {
            kick.extend(hits(b, &[(3, 0.6)]));
        }
        clap.extend(hits(b, &[(8, 1.0)]));
        snare.extend(hits(b, &[(8, 0.9)]));
        for s in 0..16 {
            let vel = if s % 4 == 2 {
                0.8
            } else if s % 2 == 0 {
                0.55
            } else {
                0.35
            };
            hat.extend(hits(b, &[(s, vel)]));
        }
        if fill {
            for s in 12..16 {
                snare.push(n(b * BAR + s * STEP, STEP, 60, 0.5 + 0.12 * (s - 12) as f32));
            }
        }
    }
    vec![(k.kick, kick), (k.snare, snare), (k.clap, clap), (k.hat, hat)]
}

fn four(k: &Kit, bars: u32) -> Vec<(ChannelId, Vec<Note>)> {
    let (mut kick, mut clap, mut hat) = (vec![], vec![], vec![]);
    for b in 0..bars {
        kick.extend(hits(b, &[(0, 1.0), (4, 1.0), (8, 1.0), (12, 1.0)]));
        clap.extend(hits(b, &[(4, 0.8), (12, 0.8)]));
        hat.extend(hits(b, &[(2, 0.6), (6, 0.6), (10, 0.6), (14, 0.6)]));
    }
    vec![(k.kick, kick), (k.clap, clap), (k.hat, hat)]
}

fn build() -> Project {
    let mut p = Project::default();
    p.bpm = BPM;
    p.key_root = 5; // F
    p.scale = Scale::Minor;
    p.master_volume = 1.0;

    let kick = p.add_channel("Kick", "lyra.kick", plugin("lyra.kick", &[(0, 48.0), (1, 7.0), (3, 0.45), (5, 0.3)]));
    let snare = p.add_channel("Snare", "lyra.snare", plugin("lyra.snare", &[(0, 190.0), (2, 0.8), (3, 0.22)]));
    let clap = p.add_channel("Clap", "lyra.clap", plugin("lyra.clap", &[]));
    let hat = p.add_channel("Hat", "lyra.hihat", plugin("lyra.hihat", &[(1, 0.05), (2, 9000.0)]));
    let pad = p.add_channel("Evolving pad", CH, chrys("Pad · Evolving", &[(cs::VOLUME, 0.28)]));
    let pluck = p.add_channel("Pluck", CH, chrys("Pluck · Bright", &[(cs::VOLUME, 0.35)]));
    let growl = p.add_channel("Growl", CH, chrys("Bass · Growl", &[(cs::mac(0), 0.6), (cs::VOLUME, 1.3)]));
    let talker = p.add_channel("Talking bass", CH, chrys("Bass · Talking", &[(cs::VOLUME, 1.4)]));
    let reese = p.add_channel("Reese", CH, chrys("Bass · Reese", &[(cs::VOLUME, 0.2), (cs::SUB_ON, 0.0)]));
    let lead_ch = p.add_channel("Supersaw lead", CH, chrys("Lead · Supersaw", &[(cs::VOLUME, 0.45)]));
    let stab_ch = p.add_channel(
        "Stabs",
        CH,
        chrys(
            "Supersaw lead",
            &[(cs::VOLUME, 0.5), (cs::MODE, 0.0), (cs::env(0, cs::E_DECAY), 0.2), (cs::env(0, cs::E_SUSTAIN), 0.3)],
        ),
    );

    // Mixer: drums glued, the melodic parts with some room, a limiter on the master.
    let reverb = |mix: f32| fx("lyra.reverb", &[(0, 0.75), (3, mix)]);
    let insert = |p: &Project, c: ChannelId| p.channels.iter().find(|x| x.id == c).unwrap().mixer;
    for c in [kick, snare, clap, hat] {
        let t = insert(&p, c);
        p.mixer[t].effects = vec![fx("lyra.compressor", &[(0, -12.0), (1, 3.0), (2, 0.01), (3, 0.12), (4, 2.0)])];
    }
    for (c, mix) in [(pad, 0.35), (pluck, 0.25), (lead_ch, 0.22), (stab_ch, 0.2)] {
        let t = insert(&p, c);
        p.mixer[t].effects = vec![fx("lyra.delay", &[(0, 3.0), (1, 0.25), (4, 0.14)]), reverb(mix)];
    }
    let t = insert(&p, growl);
    p.mixer[t].effects = vec![fx("lyra.eq", &[(2, 4.0), (3, 1200.0), (4, 2.0)])];
    p.mixer[0].effects = vec![
        fx("lyra.compressor", &[(0, -8.0), (1, 2.0), (2, 0.01), (3, 0.2), (4, 1.0)]),
        fx("lyra.limiter", &[(0, 2.0), (1, -0.3), (2, 0.08)]),
    ];

    let tracks = ["Drums", "Pad", "Pluck", "Bass", "Bass 2", "Lead"];
    for (t, name) in tracks.iter().enumerate() {
        p.tracks[t].name = (*name).into();
    }
    let (t_drums, t_pad, t_pluck, t_bass, t_bass2, t_lead) = (0, 1, 2, 3, 4, 5);
    p.patterns.clear();
    let kit = Kit { kick, snare, clap, hat };
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

    // 1 · Intro (0–7): the pad on its own, then the pluck joins.
    let mut at = 0;
    put(&mut p, t_pad, "Intro pad", at, 8, vec![(pad, chords(8, 60, 0.8))]);
    put(&mut p, t_pluck, "Pluck arp", at + 4, 4, vec![(pluck, arp(4, 72, 0.8))]);
    at += 8;

    // 2 · Build (8–11): four on the floor, the arp gets brighter (the pad keeps going).
    put(&mut p, t_drums, "Build beat", at, 4, four(&kit, 4));
    put(&mut p, t_pad, "Build pad", at, 4, vec![(pad, chords(4, 60, 0.85))]);
    put(&mut p, t_pluck, "Build arp", at, 4, vec![(pluck, arp(4, 72, 1.0))]);
    at += 4;

    // 3 · Drop A (12–19): the growl changes vowel on the 8ths; supersaw stabs.
    put(&mut p, t_drums, "Drop beat", at, 8, half_time(&kit, 8));
    put(&mut p, t_bass, "Growl", at, 8, vec![(growl, growls(8))]);
    put(&mut p, t_lead, "Stabs", at, 8, vec![(stab_ch, stabs(8, 67, 0.75))]);
    at += 8;

    // 4 · Drop B (20–27): the talking bass over a reese, the lead sings the theme.
    put(&mut p, t_drums, "Drop beat 2", at, 8, half_time(&kit, 8));
    put(&mut p, t_bass, "Talking", at, 8, vec![(talker, talk(8))]);
    put(&mut p, t_bass2, "Reese", at, 8, vec![(reese, reese_line(8))]);
    put(&mut p, t_lead, "Theme", at, 8, vec![(lead_ch, lead(0.9))]);
    put(&mut p, t_pad, "Drop pad", at, 8, vec![(pad, chords(8, 60, 0.6))]);
    at += 8;

    // 5 · Outro (28–35): pad and pluck, the growl one last time.
    put(&mut p, t_pad, "Outro pad", at, 8, vec![(pad, chords(8, 60, 0.75))]);
    put(&mut p, t_pluck, "Outro arp", at, 4, vec![(pluck, arp(4, 72, 0.7))]);
    put(&mut p, t_bass, "Last growl", at, 1, vec![(growl, vec![n(0, 2 * BAR, 29, 1.0)])]);
    p
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    finish(&build(), &dir, "Metamorphosis");
}
