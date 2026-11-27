//! "Wingbeat": a short glitch hop drop on Chrysalis. 110 BPM, D minor,
//! swung 16ths, about a minute. Every melodic sound is Chrysalis; the drums
//! are Lyra's drum synths.
//!
//! The talking bass's wobble speed rides on note velocity: one matrix slot
//! sends Velocity (bipolar) → LFO 1 rate, and the bass's own Velocity knob
//! is 0, so velocity changes the wobble and not the loudness. In the piano
//! roll's velocity lane: 50 % = one "wah-yo-wa" per beat, 79 % = triplets,
//! 100 % = 8ths.
//!
//!     cargo run --release -p lyra-app --example glitchhop -- <out dir>
//!     cargo run --release -p lyra-app -- --render "<out dir>/Wingbeat.lyra"

mod songkit;
use lyra_chrysalis::{p as cs, DESTS, PARAMS, SOURCES};
use lyra_model::{ChannelId, Clip, Note, Project, Scale};
use songkit::*;

const BPM: f64 = 110.0;
const CH: &str = "lyra.chrysalis";

/// Wobble speeds, as velocities for the talking bass (its LFO is a 1/4 that
/// velocity scales by 2^(2v − 1)).
const BEAT: f32 = 0.5;
const TRIP: f32 = 0.7925;
const FAST: f32 = 1.0;

/// A Chrysalis patch under construction: a preset, then changes.
struct Patch {
    v: Vec<f32>,
    /// The next free matrix slot.
    slot: usize,
}

impl Patch {
    fn preset(name: &str) -> Self {
        let p = lyra_catalog::presets_for(CH)
            .into_iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no preset {name}"));
        let v = p.params();
        let slot = (0..cs::SLOT_COUNT).take_while(|&k| v[cs::slot(k, cs::S_SRC)] != 0.0).count();
        Patch { v, slot }
    }
    fn set(mut self, id: usize, x: f32) -> Self {
        self.v[id] = x;
        self
    }
    /// A choice parameter by its label ("Pulse", "1/4", "Mono"…).
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
    /// The next matrix slot: `source` → `dest` × `amount` (× `via`, unless "Off").
    fn route(self, source: &str, dest: &str, amount: f32, bipolar: bool, via: &str) -> Self {
        let k = self.slot;
        assert!(k < cs::SLOT_COUNT, "matrix full");
        let ix =
            |list: &[&str], name: &str| list.iter().position(|x| *x == name).unwrap_or_else(|| panic!("no {name}"));
        let mut me = self
            .set(cs::slot(k, cs::S_SRC), ix(&SOURCES, source) as f32)
            .set(cs::slot(k, cs::S_DEST), ix(&DESTS, dest) as f32)
            .set(cs::slot(k, cs::S_AMT), amount)
            .set(cs::slot(k, cs::S_VIA), ix(&SOURCES, via) as f32)
            .set(cs::slot(k, cs::S_BIPOLAR), if bipolar { 1.0 } else { 0.0 });
        me.slot += 1;
        me
    }
    fn mod_(self, source: &str, dest: &str, amount: f32) -> Self {
        self.route(source, dest, amount, false, "Off")
    }
}

/// The lead: a narrow pulse with a saw an octave up, a little scoop into
/// every note (Env 2 → pitch), a filter blip (Env 3) and a vibrato that
/// fades in (LFO 1 via Env 4).
fn chirp_lead() -> Vec<f32> {
    Patch::preset("Init")
        .pick(cs::osc(0, cs::TABLE), "Pulse")
        .set(cs::osc(0, cs::POS), 0.35)
        .pick(cs::osc(0, cs::UNISON), "3")
        .set(cs::osc(0, cs::DETUNE), 0.12)
        .set(cs::osc(0, cs::SPREAD), 0.6)
        .set(cs::osc(0, cs::BLEND), 0.6)
        .pick(cs::osc(1, cs::OSC_ON), "On")
        .set(cs::osc(1, cs::POS), 2.0 / 3.0)
        .pick(cs::osc(1, cs::OCTAVE), "+1")
        .pick(cs::osc(1, cs::UNISON), "1")
        .set(cs::osc(1, cs::LEVEL), 0.3)
        .pick(cs::filter(0, cs::F_ON), "On")
        .pick(cs::filter(0, cs::F_TYPE), "Low 24")
        .set(cs::filter(0, cs::F_CUTOFF), 2800.0)
        .set(cs::filter(0, cs::F_RESO), 0.25)
        .set(cs::filter(0, cs::F_KEY), 0.5)
        .env(0, 0.002, 0.35, 0.55, 0.12)
        .env(1, 0.001, 0.07, 0.0, 0.05)
        .env(2, 0.001, 0.25, 0.3, 0.2)
        .env(3, 0.35, 0.1, 1.0, 0.2)
        .pick(cs::lfo(0, cs::L_SYNC), "Free")
        .set(cs::lfo(0, cs::L_RATE), 5.8)
        .mod_("Env 2", "Pitch", -0.08)
        .mod_("Env 3", "Filter 1 cutoff", 0.45)
        .route("LFO 1", "Pitch", 0.006, true, "Env 4")
        .pick(cs::MODE, "Mono")
        .set(cs::GLIDE, 0.035)
        .set(cs::VOLUME, 0.5)
        .v
}

/// The talking bass, its LFO on 1/4 and its speed on velocity.
fn talker() -> Vec<f32> {
    Patch::preset("Talking bass")
        .pick(cs::lfo(0, cs::L_SYNC), "1/4")
        .route("Velocity", "LFO 1 rate", 0.25, true, "Off")
        .set(cs::AMP_VEL, 0.0)
        .set(cs::VOLUME, 1.0)
        .v
}

/// The growl, gritty, falling two octaves in a second: the tape-stop.
fn dive() -> Vec<f32> {
    Patch::preset("Growl")
        .set(cs::mac(0), 0.7)
        .env(2, 0.9, 0.1, 1.0, 0.1)
        .mod_("Env 3", "Pitch", -1.0)
        .set(cs::VOLUME, 1.0)
        .v
}

/// Seven saws and noise through a low-pass that opens over the build.
fn riser() -> Vec<f32> {
    Patch::preset("Init")
        .pick(cs::osc(0, cs::UNISON), "7")
        .set(cs::osc(0, cs::DETUNE), 0.4)
        .pick(cs::NOISE_ON, "On")
        .set(cs::NOISE_LEVEL, 0.5)
        .set(cs::NOISE_COLOR, 0.3)
        .pick(cs::filter(0, cs::F_ON), "On")
        .pick(cs::filter(0, cs::F_TYPE), "Low 12")
        .set(cs::filter(0, cs::F_CUTOFF), 250.0)
        .set(cs::filter(0, cs::F_RESO), 0.35)
        .env(0, 1.5, 1.0, 1.0, 0.3)
        .env(1, 7.5, 1.0, 1.0, 0.3)
        .mod_("Env 2", "Filter 1 cutoff", 0.75)
        .mod_("Env 2", "Pitch", 0.5)
        .set(cs::VOLUME, 0.2)
        .v
}

fn n(start: u32, len: u32, key: i32, velocity: f32) -> Note {
    Note { start, len, key: key.clamp(0, 127) as u8, velocity: velocity.clamp(0.05, 1.0), link: None }
}

/// i – iv – VI – V in D minor: (bass root, triad).
const PROG: [(i32, [i32; 3]); 4] = [(38, [0, 3, 7]), (31, [0, 3, 7]), (34, [0, 4, 7]), (33, [0, 4, 7])];

/// The theme, 8 bars (one token = a 16th).
const MELODY: [&str; 8] = [
    "a-.a-.c'-a-g-f-g-",
    "b-.a-.g-f-d-.f-g",
    "-.f-d-f-g-a-b-c'-",
    "^c'---e'-.d'-^c'-a----",
    "a-.a-.c'-a-g-f-d'-",
    "-.b-.a-g-.g-a-b-",
    "c'-d'-f'-.d'-c'-b-a--",
    "a-^c'-e'-a'---------",
];

/// The theme from bar `at`, transposed by `shift`. The end of every 4th bar
/// stutters in 32nds instead of holding.
fn theme(at: u32, shift: i32, vel: f32, stutter: bool) -> Vec<Note> {
    let mut v: Vec<Note> = parse(&MELODY.concat(), D_MINOR)
        .into_iter()
        .map(|t| n(at * BAR + t.step * STEP, t.len * STEP - 4, t.key + shift, vel))
        .collect();
    if stutter {
        for bar in [3, 7] {
            let from = (at + bar) * BAR + 12 * STEP;
            stutter_at(&mut v, from, from + 4 * STEP);
        }
    }
    v
}

/// Whatever note is held over `from` is cut there and repeats in 32nds
/// (fading) until `to`.
fn stutter_at(v: &mut Vec<Note>, from: u32, to: u32) {
    let Some(held) = v.iter_mut().find(|x| x.start < from && x.start + x.len > from) else {
        return;
    };
    held.len = from - held.start;
    let (key, vel) = (held.key as i32, held.velocity);
    let k = (to - from) / (STEP / 2);
    for i in 0..k {
        v.push(n(from + i * STEP / 2, STEP / 2 - 3, key, vel * (1.0 - 0.5 * i as f32 / k as f32)));
    }
}

/// Chord stabs on `steps` of every bar, voiced around `center`.
fn stabs(bars: u32, steps: &[u32], center: i32, vel: f32) -> Vec<Note> {
    let mut v = Vec::new();
    for b in 0..bars {
        let (root, tri) = PROG[b as usize % 4];
        for &s in steps {
            for i in tri {
                let pc = (root + i).rem_euclid(12);
                let k = center - 6 + (pc - (center - 6)).rem_euclid(12);
                v.push(n(b * BAR + s * STEP, 2 * STEP, k, vel));
            }
        }
    }
    v
}

/// The talking bass's rhythm over one bar: (step, length, up, wobble).
type Riff = &'static [(u32, u32, i32, f32)];
const RIFF_A: Riff = &[(0, 4, 0, BEAT), (4, 2, 0, FAST), (6, 2, 12, FAST), (8, 6, 0, TRIP), (14, 2, 12, FAST)];
const RIFF_B: Riff = &[(0, 3, 0, BEAT), (3, 3, 0, TRIP), (6, 2, 12, FAST), (8, 4, 0, BEAT), (12, 4, 0, FAST)];
/// Every 4th bar: the last beat is left for a glitch (a stutter or the dive).
const RIFF_END: Riff = &[(0, 4, 0, BEAT), (4, 4, 12, TRIP), (8, 4, 0, FAST)];

/// The drop's bass for `bars` bars (`flip` swaps the A and B riffs), and the
/// dive at the end of every 8th bar.
fn bass(bars: u32, flip: bool) -> (Vec<Note>, Vec<Note>) {
    let (mut talk, mut dives) = (Vec::new(), Vec::new());
    for b in 0..bars {
        let root = PROG[b as usize % 4].0;
        let riff = match (b % 4, flip) {
            (3, _) => RIFF_END,
            (0 | 2, false) | (1, true) => RIFF_A,
            _ => RIFF_B,
        };
        let at = b * BAR;
        if b % 8 == 7 {
            // Two beats, then the bass falls away.
            for &(s, len, up, wob) in &riff[..2] {
                talk.push(n(at + s * STEP, len * STEP - 4, root + up, wob));
            }
            dives.push(n(at + 8 * STEP, 8 * STEP, root + 12, 1.0));
            continue;
        }
        for &(s, len, up, wob) in riff {
            talk.push(n(at + s * STEP, len * STEP - 4, root + up, wob));
        }
        if b % 4 == 3 {
            // The glitch: the octave in 32nds, each a fresh "t".
            for i in 0..8 {
                talk.push(n(at + 12 * STEP + i * STEP / 2, STEP / 2 - 3, root + 12, FAST));
            }
        }
    }
    (talk, dives)
}

struct Kit {
    kick: ChannelId,
    snare: ChannelId,
    clap: ChannelId,
    hat: ChannelId,
    open: ChannelId,
}

fn hits(bar: u32, steps: &[(u32, f32)]) -> Vec<Note> {
    steps.iter().map(|&(s, vel)| n(bar * BAR + s * STEP, STEP, 60, vel)).collect()
}

/// A 32nd roll over steps `from..to` of `bar`, rising from `lo` to `hi`.
fn roll(bar: u32, from: u32, to: u32, lo: f32, hi: f32) -> Vec<Note> {
    let k = 2 * (to - from);
    (0..k)
        .map(|i| n(bar * BAR + from * STEP + i * STEP / 2, STEP / 2, 60, lo + (hi - lo) * i as f32 / k as f32))
        .collect()
}

/// The drop's groove: kick and snare with ghosts, swung 16th hats, open hats
/// on the offbeats. Bar 4 of 8 ends in a snare roll, bar 8 in silence.
fn groove(k: &Kit, bars: u32) -> Vec<(ChannelId, Vec<Note>)> {
    let (mut kick, mut snare, mut clap, mut hat, mut open) = (vec![], vec![], vec![], vec![], vec![]);
    for b in 0..bars {
        let second = b % 2 == 1;
        let (fill, gap) = (b % 8 == 3, b % 8 == 7);
        let end = if fill || gap { 12 } else { 16 };
        let keep = |v: Vec<Note>| v.into_iter().filter(|x| x.start < b * BAR + end * STEP);
        kick.extend(keep(if second {
            hits(b, &[(0, 1.0), (3, 0.6), (10, 0.95), (13, 0.6)])
        } else {
            hits(b, &[(0, 1.0), (7, 0.8), (10, 0.95)])
        }));
        snare.extend(keep(hits(b, &[(4, 1.0), (9, 0.22), (12, 1.0)])));
        if second {
            snare.extend(keep(hits(b, &[(15, 0.28)])));
        }
        clap.extend(keep(hits(b, &[(4, 0.7), (12, 0.7)])));
        for s in (0..16).filter(|s| s % 8 != 6) {
            let vel = if s % 4 == 0 {
                0.65
            } else if s % 2 == 0 {
                0.5
            } else {
                0.3
            };
            hat.extend(keep(hits(b, &[(s, vel)])));
        }
        open.extend(keep(hits(b, &[(6, 0.45), (14, 0.45)])));
        if fill {
            snare.extend(roll(b, 12, 16, 0.35, 0.95));
        }
    }
    vec![(k.kick, kick), (k.snare, snare), (k.clap, clap), (k.hat, hat), (k.open, open)]
}

/// The build: kicks on the beat, a snare roll that doubles every bar (4ths,
/// 8ths, 16ths, then 32nds), and half a bar of nothing before the drop.
fn build_drums(k: &Kit) -> Vec<(ChannelId, Vec<Note>)> {
    let (mut kick, mut snare, mut hat) = (vec![], vec![], vec![]);
    for b in 0..4 {
        let beats: &[u32] = if b == 3 { &[0, 4] } else { &[0, 4, 8, 12] };
        kick.extend(beats.iter().flat_map(|&s| hits(b, &[(s, 0.45 + 0.05 * b as f32)])));
        if b < 3 {
            let every = 4 >> b;
            for s in (0..16).step_by(every as usize) {
                snare.extend(hits(b, &[(s, 0.2 + 0.12 * b as f32 + 0.015 * s as f32)]));
            }
            for s in (2..16).step_by(4) {
                hat.extend(hits(b, &[(s, 0.45)]));
            }
        } else {
            snare.extend(roll(b, 0, 8, 0.35, 0.75));
        }
    }
    vec![(k.kick, kick), (k.snare, snare), (k.hat, hat)]
}

fn build() -> Project {
    let mut p = Project::default();
    p.bpm = BPM;
    p.swing = 0.4;
    p.key_root = 2; // D
    p.scale = Scale::Minor;
    p.master_volume = 1.0;

    let kick = p.add_channel("Kick", "lyra.kick", plugin("lyra.kick", &[(0, 50.0), (1, 6.0), (3, 0.4), (5, 0.35)]));
    let snare =
        p.add_channel("Snare", "lyra.snare", plugin("lyra.snare", &[(0, 200.0), (2, 0.85), (3, 0.18), (5, 1.2)]));
    let clap = p.add_channel("Clap", "lyra.clap", plugin("lyra.clap", &[(3, 1.2)]));
    let hat = p.add_channel("Hat", "lyra.hihat", plugin("lyra.hihat", &[(1, 0.04), (2, 9500.0), (4, 1.0)]));
    let open = p.add_channel("Open hat", "lyra.hihat", plugin("lyra.hihat", &[(1, 0.22), (2, 8500.0), (4, 0.75)]));
    let talk = p.add_channel("Talker", CH, talker());
    let dive_ch = p.add_channel("Dive", CH, dive());
    let lead = p.add_channel("Chirp lead", CH, chirp_lead());
    let high = p.add_channel("Sparkle", CH, Patch::preset("Pluck").set(cs::VOLUME, 0.3).v);
    let keys = p.add_channel("Keys", CH, Patch::preset("Pluck").set(cs::VOLUME, 0.35).v);
    let rise = p.add_channel("Riser", CH, riser());

    // Mixer: drums squeezed, the lead in a dotted-8th ping-pong, keys
    // breathing with the beat, a limiter on the master.
    let insert = |p: &Project, c: ChannelId| p.channels.iter().find(|x| x.id == c).unwrap().mixer;
    let reverb = |mix: f32| fx("lyra.reverb", &[(0, 0.7), (3, mix)]);
    for c in [kick, snare, clap, hat, open] {
        let t = insert(&p, c);
        p.mixer[t].effects = vec![fx("lyra.compressor", &[(0, -12.0), (1, 3.0), (2, 0.01), (3, 0.12), (4, 2.0)])];
    }
    let t = insert(&p, lead);
    p.mixer[t].effects = vec![fx("lyra.delay", &[(0, 2.0), (1, 0.3), (4, 0.18)]), reverb(0.18)];
    let t = insert(&p, high);
    p.mixer[t].effects = vec![fx("lyra.delay", &[(0, 2.0), (1, 0.35), (4, 0.25)]), reverb(0.3)];
    let t = insert(&p, keys);
    p.mixer[t].effects = vec![
        fx("lyra.pump", &[(0, 0.5), (1, 0.5), (2, 0.0)]),
        fx("lyra.chorus", &[(0, 0.4), (1, 0.6), (3, 0.4)]),
        reverb(0.25),
    ];
    for c in [talk, dive_ch] {
        let t = insert(&p, c);
        p.mixer[t].effects = vec![fx("lyra.eq", &[(0, 30.0), (2, 3.0), (3, 1400.0), (4, 2.0)])];
    }
    let t = insert(&p, rise);
    p.mixer[t].effects = vec![reverb(0.4)];
    p.mixer[0].effects = vec![
        fx("lyra.compressor", &[(0, -8.0), (1, 2.0), (2, 0.01), (3, 0.2), (4, 1.0)]),
        fx("lyra.limiter", &[(0, 4.0), (1, -0.3), (2, 0.08)]),
    ];

    let tracks = ["Drums", "Bass", "Lead", "Sparkle", "Keys", "FX"];
    for (t, name) in tracks.iter().enumerate() {
        p.tracks[t].name = (*name).into();
    }
    let (t_drums, t_bass, t_lead, t_high, t_keys, t_fx) = (0, 1, 2, 3, 4, 5);
    p.patterns.clear();
    let kit = Kit { kick, snare, clap, hat, open };
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

    // 1 · Intro (0–3): keys and the theme on the sparkle pluck; hats join.
    put(&mut p, t_keys, "Intro keys", 0, 8, vec![(keys, stabs(8, &[0, 3, 6, 10], 64, 0.7))]);
    put(&mut p, t_high, "Theme preview", 0, 8, vec![(high, theme(0, 0, 0.8, false))]);
    let hats: Vec<Note> = (0..2).flat_map(|b| (0..16).step_by(2).flat_map(move |s| hits(b, &[(s, 0.35)]))).collect();
    put(&mut p, t_drums, "Intro hats", 2, 2, vec![(hat, hats)]);

    // 2 · Build (4–7): the roll, the riser, a talker "yo" in the gap.
    put(&mut p, t_drums, "Build", 4, 4, build_drums(&kit));
    put(&mut p, t_fx, "Riser", 4, 4, vec![(rise, vec![n(0, 3 * BAR + 8 * STEP, 50, 0.9)])]);
    put(&mut p, t_bass, "Pickup", 7, 1, vec![(talk, vec![n(10 * STEP, 6 * STEP - 4, 38, TRIP)])]);

    // 3 · Drop (8–23): the theme on the chirp lead over the talker; the
    // second half flips the riffs, adds the sparkle an octave up and keys.
    let (talk_a, dive_a) = bass(8, false);
    let (talk_b, dive_b) = bass(8, true);
    put(&mut p, t_drums, "Groove", 8, 8, groove(&kit, 8));
    put(&mut p, t_bass, "Talk A", 8, 8, vec![(talk, talk_a), (dive_ch, dive_a)]);
    put(&mut p, t_lead, "Theme", 8, 8, vec![(lead, theme(0, -12, 0.95, true))]);
    put(&mut p, t_drums, "Groove 2", 16, 8, groove(&kit, 8));
    put(&mut p, t_bass, "Talk B", 16, 8, vec![(talk, talk_b), (dive_ch, dive_b)]);
    put(&mut p, t_lead, "Theme 2", 16, 8, vec![(lead, theme(0, -12, 0.95, true))]);
    put(&mut p, t_high, "Theme high", 16, 8, vec![(high, theme(0, 0, 0.6, true))]);
    put(&mut p, t_keys, "Offbeat keys", 16, 8, vec![(keys, stabs(8, &[2, 6, 10, 14], 64, 0.5))]);

    // 4 · Outro (24–25): home on D, one last wah.
    put(&mut p, t_drums, "Last kick", 24, 1, vec![(kick, hits(0, &[(0, 1.0)])), (clap, hits(0, &[(0, 0.6)]))]);
    put(&mut p, t_bass, "Last talk", 24, 2, vec![(talk, vec![n(0, BAR + 8 * STEP, 38, BEAT)])]);
    put(&mut p, t_lead, "Last note", 24, 2, vec![(lead, vec![n(0, 2 * BAR - 12, 74, 0.9)])]);
    put(&mut p, t_high, "Last sparkle", 24, 2, vec![(high, vec![n(0, BAR, 86, 0.6), n(0, BAR, 81, 0.5)])]);
    put(&mut p, t_keys, "Last chord", 24, 2, vec![(keys, stabs(1, &[0], 64, 0.7))]);
    p
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    finish(&build(), &dir, "Wingbeat");
}
