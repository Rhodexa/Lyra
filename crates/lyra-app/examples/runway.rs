//! "Night Runway": a city-pop flight over Rhodexa's chords. 124 BPM.
//!
//! The intro and the A section play Rhodexa's own chords: the first eight
//! bars of the improvised "Overglider" (used with permission). They were
//! built by ear, bass first: each chord's top note doubles the bass, and the
//! chord is stacked downward in thirds from it, so the bass is usually the
//! chord's seventh (Fmaj7 over E, Em7 over D, F#m7 over E…). It floats; it
//! never lands.
//!
//! Over them, one melodic motif in the chords' own 3 + 3 + 4 rhythm, whose
//! long note "goes with the chord" each time the harmony moves (F# over Em7,
//! B♭ over Cm7, F over B♭m7…).
//!
//! The B section is the runway: the bass holds E while the chords blink
//! between F and F#, then everything glides down a semitone a bar
//! (Amaj7/G#, A♭maj7/G, Gmaj7/F#), chords built the same way. At the very
//! end the bass finally steps up from E to F, the root the song floated
//! over the whole time: touchdown.
//!
//! ```text
//! cargo run --release -p lyra-app --example runway -- <out dir>
//! cargo run --release -p lyra-app -- --render "<out dir>/Night Runway.lyra"
//! ```

mod songkit;
use lyra_chrysalis::p as cs;
use lyra_fluttersine::p as fs;
use lyra_model::{ChannelId, Clip, Note, Project, Scale};
use songkit::*;

const BPM: f64 = 124.0;
const CH: &str = "lyra.chrysalis";

fn n(start: u32, len: u32, key: i32, velocity: f32) -> Note {
    Note { start, len, key: key.clamp(0, 127) as u8, velocity: velocity.clamp(0.05, 1.0) }
}

/// Rhodexa's chords, bars 1–8 of "Overglider": (tick, length, keys); keys
/// under 48 are the bass.
#[rustfmt::skip]
const OVERGLIDER: &[(u32, u32, &[u8])] = &[
    (0, 48, &[40, 65, 69, 72, 76]),
    (72, 24, &[40, 65, 69, 72, 76]),
    (144, 48, &[40, 65, 69, 72, 76]),
    (240, 48, &[38, 64, 67, 71, 74]),
    (336, 48, &[38, 64, 67, 71, 74]),
    (384, 24, &[38, 64, 67, 71, 74]),
    (432, 144, &[40, 66, 69, 73, 76]),
    (600, 48, &[40, 66, 69, 73, 76]),
    (672, 48, &[40, 66, 69, 73, 76]),
    (768, 48, &[39, 63, 67, 72, 75]),
    (840, 24, &[39, 63, 67, 72, 75]),
    (912, 48, &[39, 63, 67, 72, 75]),
    (1008, 48, &[42, 67, 72, 75, 79]),
    (1056, 48, &[42, 68, 72, 75, 79]),
    (1152, 48, &[42, 68, 72, 75, 77]),
    (1248, 96, &[42, 68, 70, 74, 77]),
    (1344, 24, &[42, 68, 70, 74, 77]),
    (1392, 48, &[42, 68, 70, 74, 77]),
    (1464, 48, &[42, 68, 70, 74, 77]),
    (1536, 48, &[44, 64, 68, 72, 75]),
    (1608, 24, &[44, 64, 68, 72, 75]),
    (1632, 48, &[64, 68, 72, 75]),
    (1680, 48, &[44]),
    (1704, 24, &[64, 68, 72, 75]),
    (1728, 48, &[68, 70, 73, 77]),
    (1776, 48, &[45]),
    (1800, 48, &[68, 70, 73, 77]),
    (1872, 48, &[45, 68, 70, 73, 77]),
    (1920, 144, &[45, 66, 69, 73, 76]),
    (1968, 144, &[40]),
    (2088, 48, &[66, 69, 73, 76]),
    (2136, 48, &[40]),
    (2160, 48, &[66, 69, 73, 76]),
    (2208, 48, &[40]),
    (2304, 48, &[39, 63, 67, 72, 75]),
    (2352, 18, &[63, 67, 72, 75]),
    (2376, 24, &[39]),
    (2400, 42, &[63, 67, 72, 75]),
    (2448, 48, &[39, 63, 67, 72, 75]),
    (2496, 42, &[63, 67, 72, 75]),
    (2544, 48, &[42, 63, 67, 72, 75]),
    (2592, 48, &[42, 61, 66, 73, 78]),
    (2629, 24, &[61, 66, 73, 78]),
    (2666, 8, &[61, 66, 73, 78]),
    (2688, 48, &[42, 68, 72, 75, 79]),
    (2760, 96, &[68, 72, 75, 79]),
    (2784, 96, &[42]),
    (2880, 24, &[42, 68, 72, 75, 79]),
    (2928, 48, &[42, 68, 72, 75, 79]),
    (3000, 48, &[42, 68, 72, 75, 79]),
];

/// The runway (B section): (bass, chord) per bar, in the rhythm of
/// Rhodexa's first bar. Each chord: the bass's note on top, thirds below.
const RUNWAY: [(i32, [i32; 4]); 8] = [
    (40, [65, 69, 72, 76]), // Fmaj7/E
    (40, [66, 69, 73, 76]), // F#m7/E
    (40, [65, 69, 72, 76]), // Fmaj7/E
    (40, [66, 70, 73, 76]), // F#7/E
    (44, [69, 73, 76, 80]), // Amaj7/G#
    (43, [68, 72, 75, 79]), // A♭maj7/G
    (42, [67, 71, 74, 78]), // Gmaj7/F#
    (40, [66, 69, 73, 76]), // F#m7/E
];
/// Rhodexa's first-bar rhythm: (step, length in steps).
const HITS: [(u32, u32); 5] = [(0, 2), (3, 1), (6, 2), (10, 2), (14, 2)];

/// The lead (one token = a 16th; no key signature, accidentals written).
/// Bars 1, 3, 5 are the motif (up, up, up, land) landing on the chord's
/// colour: F#, then B♭, then F. The even bars answer it.
const MELODY: [&str; 8] = [
    "c--e--g---^f-----",
    "e-^g-----^f-e-----",
    "c--_e--g---_b-----",
    "_a-f-d---f-d-----",
    "c--_e--e---f-----",
    "e-^c'-----b-a-----",
    "g--_e--c---_e-^f---",
    "g-----f-_e-c-----",
];
/// The runway's lead: resting four bars, then the motif gliding down a
/// semitone a bar with the chords.
const GLIDE: [&str; 4] = ["^g--b--^c'---e'-----", "g--_b--c'---_e'-----", "^f--a--b---d'-----", "e--^f--a---^c'-----"];
/// The landing: the motif once more, its long note now the A of F major.
const LANDING: [&str; 2] = ["c--e--g---a-----", "a---------------"];

fn line(bars: &[&str], shift: i32, vel: f32) -> Vec<Note> {
    parse(&bars.concat(), &[]).into_iter().map(|t| n(t.step * STEP, t.len * STEP - 6, t.key + shift, vel)).collect()
}

/// Rhodexa's chords for bars `from..to` of the eight: (chords, bass).
fn overglider(from: u32, to: u32, vel: f32) -> (Vec<Note>, Vec<Note>) {
    let (mut chords, mut bass) = (Vec::new(), Vec::new());
    for &(tick, len, keys) in OVERGLIDER {
        if tick < from * BAR || tick >= to * BAR {
            continue;
        }
        let at = tick - from * BAR;
        let accent = if tick % BAR == 0 { 1.0 } else { 0.85 };
        for &k in keys {
            let note = n(at, len, k as i32, vel * accent);
            if k < 48 {
                bass.push(note);
            } else {
                chords.push(note);
            }
        }
    }
    (chords, bass)
}

/// The runway's chords and bass, bars `from..to`.
fn runway(from: usize, to: usize, vel: f32) -> (Vec<Note>, Vec<Note>) {
    let (mut chords, mut bass) = (Vec::new(), Vec::new());
    for (b, &(root, chord)) in RUNWAY[from..to].iter().enumerate() {
        for &(s, len) in &HITS {
            let at = b as u32 * BAR + s * STEP;
            let accent = if s == 0 { 1.0 } else { 0.85 };
            bass.push(n(at, len * STEP, root, vel * accent));
            chords.extend(chord.iter().map(|&k| n(at, len * STEP, k, vel * accent)));
        }
    }
    (chords, bass)
}

/// Runway lights: the chord's notes an octave up, in 16ths, up and down.
fn lights(vel: f32) -> Vec<Note> {
    const ORDER: [usize; 6] = [0, 1, 2, 3, 2, 1];
    let mut v = Vec::new();
    for (b, &(_, chord)) in RUNWAY.iter().enumerate() {
        for s in 0..16u32 {
            let k = chord[ORDER[s as usize % 6]] + 12;
            v.push(n(b as u32 * BAR + s * STEP, STEP, k, vel * if s % 4 == 0 { 1.0 } else { 0.7 }));
        }
    }
    v
}

/// The bass an octave down, for the sub.
fn sub(bass: &[Note]) -> Vec<Note> {
    bass.iter().map(|x| Note { key: x.key - 12, ..*x }).collect()
}

/// Whole-bar pads under the runway.
fn pads(vel: f32) -> Vec<Note> {
    RUNWAY.iter().enumerate().flat_map(|(b, &(_, c))| c.map(|k| n(b as u32 * BAR, BAR - 6, k - 12, vel))).collect()
}

struct Kit {
    kick: ChannelId,
    snare: ChannelId,
    clap: ChannelId,
    hat: ChannelId,
    open: ChannelId,
    ride: ChannelId,
    crash: ChannelId,
}

fn hits(bar: u32, steps: &[(u32, f32)]) -> Vec<Note> {
    steps.iter().map(|&(s, vel)| n(bar * BAR + s * STEP, STEP, 60, vel)).collect()
}

/// The groove: the kick plays the chords' 3 + 3 + 4, the snare and clap the
/// backbeat, 16th hats, open hats on the offbeats; a crash on bar one.
/// `ride` adds a ride cymbal on the quarters (the runway).
fn groove(k: &Kit, bars: u32, ride: bool) -> Vec<(ChannelId, Vec<Note>)> {
    let (mut kick, mut snare, mut clap, mut hat, mut open, mut rides) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    for b in 0..bars {
        kick.extend(hits(b, &[(0, 1.0), (3, 0.7), (6, 0.85), (10, 0.9)]));
        if b % 2 == 1 {
            kick.extend(hits(b, &[(14, 0.6)]));
        }
        snare.extend(hits(b, &[(4, 0.9), (12, 0.9)]));
        clap.extend(hits(b, &[(4, 0.7), (12, 0.7)]));
        for s in (0..16).filter(|s| s % 4 != 2) {
            hat.extend(hits(b, &[(s, if s % 2 == 0 { 0.6 } else { 0.35 })]));
        }
        open.extend(hits(b, &[(2, 0.5), (6, 0.45), (10, 0.5), (14, 0.45)]));
        if ride {
            rides.extend(hits(b, &[(0, 0.7), (4, 0.5), (8, 0.6), (12, 0.5)]));
        }
        if b % 4 == 3 {
            snare.extend((0..4).map(|i| n(b * BAR + (12 + i) * STEP, STEP, 60, 0.35 + 0.15 * i as f32)));
        }
    }
    vec![
        (k.kick, kick),
        (k.snare, snare),
        (k.clap, clap),
        (k.hat, hat),
        (k.open, open),
        (k.ride, rides),
        (k.crash, hits(0, &[(0, 0.9)])),
    ]
}

fn build() -> Project {
    let mut p = Project::default();
    p.bpm = BPM;
    p.swing = 0.35;
    p.key_root = 5; // F: where it lands
    p.scale = Scale::Major;
    p.master_volume = 1.0;

    let kick = p.add_channel("Kick", "lyra.kick", plugin("lyra.kick", &[(0, 52.0), (1, 5.0), (3, 0.35), (4, 0.5)]));
    let snare =
        p.add_channel("Snare", "lyra.snare", plugin("lyra.snare", &[(0, 210.0), (2, 0.9), (3, 0.16), (5, 1.2)]));
    let clap = p.add_channel("Clap", "lyra.clap", plugin("lyra.clap", &[]));
    let hat = p.add_channel("Hat", "lyra.hihat", plugin("lyra.hihat", &[(1, 0.045), (2, 11000.0), (4, 0.9)]));
    let open = p.add_channel("Open hat", "lyra.hihat", plugin("lyra.hihat", &[(1, 0.2), (2, 10000.0), (4, 0.6)]));
    let sample = |p: &mut Project, name: &str, file: &str, gain: f32| {
        let id = p.add_channel(name, "lyra.sampler", plugin("lyra.sampler", &[(lyra_sampler::p::GAIN, gain)]));
        p.channels.iter_mut().find(|c| c.id == id).unwrap().sample = Some(format!("bundled:Starter Kit/{file}"));
        id
    };
    let ride = sample(&mut p, "Ride", "Hats & cymbals/drum_cymbal_soft.flac", -6.0);
    let crash = sample(&mut p, "Crash", "Hats & cymbals/drum_splash_hard.flac", -3.0);

    let ep = p.add_channel("E-piano", "lyra.fluttersine", fm_preset("FM E-Piano", &[(fs::VOLUME, 0.5)]));
    let sub_ch = p.add_channel("Sub", "lyra.fluttersine", fm_preset("Sub Bass", &[(fs::VOLUME, 0.45)]));
    let pluck = p.add_channel("Pluck bass", CH, plugin_preset("Pluck", &[(cs::VOLUME, 0.5), (cs::MODE, 1.0)]));
    let lead =
        p.add_channel("Chip lead", "lyra.fluttersine", fm_preset("Chip Lead 25% (Mono Glide)", &[(fs::VOLUME, 0.3)]));
    let saw = p.add_channel("Saw lead", CH, plugin_preset("Supersaw lead", &[(cs::VOLUME, 0.25)]));
    let bell = p.add_channel("Runway lights", "lyra.fluttersine", fm_preset("FM Bell", &[(fs::VOLUME, 0.18)]));
    let pad = p.add_channel("Pad", "lyra.fluttersine", fm_preset("PWM Strings", &[(fs::VOLUME, 0.12)]));

    // Mixer: bright rooms (little damping), the city-pop chorus on the keys,
    // a dotted-8th echo on the lead, the keys breathing with the kick.
    let insert = |p: &Project, c: ChannelId| p.channels.iter().find(|x| x.id == c).unwrap().mixer;
    let room = |mix: f32| fx("lyra.reverb", &[(0, 0.6), (1, 0.2), (3, mix)]);
    for c in [kick, snare, clap, hat, open] {
        let t = insert(&p, c);
        p.mixer[t].effects = vec![fx("lyra.compressor", &[(0, -12.0), (1, 3.0), (2, 0.008), (3, 0.1), (4, 2.0)])];
    }
    let t = insert(&p, ep);
    p.mixer[t].effects = vec![
        fx("lyra.pump", &[(0, 0.35), (1, 0.5), (2, 0.0)]),
        fx("lyra.chorus", &[(0, 0.5), (1, 0.6), (3, 0.45)]),
        room(0.2),
    ];
    for c in [lead, saw] {
        let t = insert(&p, c);
        p.mixer[t].effects = vec![fx("lyra.delay", &[(0, 2.0), (1, 0.3), (3, 9000.0), (4, 0.2)]), room(0.18)];
    }
    for (c, mix) in [(bell, 0.3), (pad, 0.35), (crash, 0.15), (ride, 0.1)] {
        let t = insert(&p, c);
        p.mixer[t].effects = vec![room(mix)];
    }
    p.mixer[0].effects = vec![
        fx("lyra.eq", &[(0, 25.0), (4, 2.5)]),
        fx("lyra.compressor", &[(0, -10.0), (1, 2.0), (2, 0.01), (3, 0.15), (4, 1.5)]),
        fx("lyra.limiter", &[(0, 3.0), (1, -0.3), (2, 0.08)]),
    ];

    let names = ["Drums", "Bass", "Keys", "Lead", "Lead 2", "Lights", "Pad"];
    for (t, name) in names.iter().enumerate() {
        p.tracks[t].name = (*name).into();
    }
    let (t_drums, t_bass, t_keys, t_lead, t_lead2, t_lights, t_pad) = (0, 1, 2, 3, 4, 5, 6);
    p.patterns.clear();
    let kit = Kit { kick, snare, clap, hat, open, ride, crash };
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
    let hats_only: Vec<Note> =
        (0..4).flat_map(|b| (0..16).step_by(2).flat_map(move |s| hits(b, &[(s, 0.4)]))).collect();

    // 1 · Intro (0–3): Rhodexa's first four bars on the e-piano, hats.
    let (c, _) = overglider(0, 4, 0.7);
    put(&mut p, t_keys, "Overglider 1–4", 0, 4, vec![(ep, c)]);
    put(&mut p, t_drums, "Hats", 0, 4, vec![(hat, hats_only)]);

    // 2 · A (4–11): all eight bars, the band, the motif.
    let (c, b) = overglider(0, 8, 0.85);
    put(&mut p, t_keys, "Overglider", 4, 8, vec![(ep, c)]);
    put(&mut p, t_bass, "Overglider bass", 4, 8, vec![(pluck, b.clone()), (sub_ch, sub(&b))]);
    put(&mut p, t_drums, "Groove", 4, 8, groove(&kit, 8, false));
    put(&mut p, t_lead, "Motif", 4, 8, vec![(lead, line(&MELODY, 0, 0.9))]);

    // 3 · B (12–19): the runway; the lead glides down in the second half.
    let (c, b) = runway(0, 8, 0.8);
    put(&mut p, t_keys, "Runway", 12, 8, vec![(ep, c)]);
    put(&mut p, t_bass, "Runway bass", 12, 8, vec![(pluck, b.clone()), (sub_ch, sub(&b))]);
    put(&mut p, t_drums, "Runway groove", 12, 8, groove(&kit, 8, true));
    put(&mut p, t_lights, "Lights", 12, 8, vec![(bell, lights(0.6))]);
    put(&mut p, t_pad, "Runway pad", 12, 8, vec![(pad, pads(0.6))]);
    put(&mut p, t_lead, "Glide", 16, 4, vec![(lead, line(&GLIDE, 0, 0.9))]);
    put(&mut p, t_lead2, "Glide low", 16, 4, vec![(saw, line(&GLIDE, -12, 0.75))]);

    // 4 · A' (20–27): Rhodexa's chords again, the motif doubled below.
    let (c, b) = overglider(0, 8, 0.9);
    put(&mut p, t_keys, "Overglider again", 20, 8, vec![(ep, c)]);
    put(&mut p, t_bass, "Overglider bass 2", 20, 8, vec![(pluck, b.clone()), (sub_ch, sub(&b))]);
    put(&mut p, t_drums, "Groove 2", 20, 8, groove(&kit, 8, false));
    put(&mut p, t_lead, "Motif again", 20, 8, vec![(lead, line(&MELODY, 0, 0.95))]);
    put(&mut p, t_lead2, "Motif low", 20, 8, vec![(saw, line(&MELODY, -12, 0.8))]);

    // 5 · Landing (28–29): Fmaj7 over E once more, then F under it at last.
    let fmaj9 = [65, 69, 72, 76, 79];
    let mut keys: Vec<Note> =
        HITS.iter().flat_map(|&(s, len)| [65, 69, 72, 76].map(|k| n(s * STEP, len * STEP, k, 0.85))).collect();
    keys.extend(fmaj9.map(|k| n(BAR, BAR + 8 * STEP, k, 0.9)));
    let mut bass: Vec<Note> = HITS.iter().map(|&(s, len)| n(s * STEP, len * STEP, 40, 0.85)).collect();
    bass.push(n(BAR, BAR + 8 * STEP, 41, 1.0));
    put(&mut p, t_keys, "Landing", 28, 2, vec![(ep, keys), (pad, fmaj9.map(|k| n(BAR, BAR, k - 12, 0.6)).to_vec())]);
    put(&mut p, t_bass, "Touchdown", 28, 2, vec![(pluck, bass.clone()), (sub_ch, sub(&bass))]);
    put(&mut p, t_lead, "Landing", 28, 2, vec![(lead, line(&LANDING, 0, 0.9)), (saw, line(&LANDING, -12, 0.75))]);
    let mut last = groove(&kit, 1, false);
    last.push((kick, hits(1, &[(0, 1.0)])));
    last.push((crash, hits(1, &[(0, 1.0)])));
    put(&mut p, t_drums, "Last groove", 28, 2, last);
    p
}

/// A Chrysalis preset by name, plus `set`.
fn plugin_preset(name: &str, set: &[(usize, f32)]) -> Vec<f32> {
    let p = lyra_catalog::presets_for(CH).into_iter().find(|p| p.name == name).expect("preset");
    tweak(p.params(), set)
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    finish(&build(), &dir, "Night Runway");
}
