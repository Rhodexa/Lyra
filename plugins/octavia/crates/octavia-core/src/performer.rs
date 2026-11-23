//! The performer: turns notes into what a cellist's hands would do.
//!
//! Rules, no learning. Each rule is small, named, and loosely follows the
//! KTH performance-rule research (Sundberg, Friberg et al.) and ordinary
//! string-playing habits:
//!
//! * **Phrases** are split at rests. Loudness arches over a phrase, peaking
//!   a bit past the middle, and the last note fades.
//! * **High-loud**: higher notes in a phrase are played a little louder.
//! * **Short-soft**: very short notes are a little softer.
//! * **Bow changes** between separate notes dip the loudness and add a
//!   puff of bow noise; **slurred** notes keep the bow moving, only the
//!   left hand changes (a quick pitch move, a tiny dip).
//! * **Shifts**: big slurred leaps after a long note slide audibly.
//! * **Vibrato** starts a moment after the note, grows in, follows loudness,
//!   is absent on short notes and on open strings.
//! * **Intonation**: a few cents of human error that settles after the
//!   attack; leading tones lean up toward the note they resolve to.
//! * **String choice** stays on the current string while it can, and
//!   avoids open strings on long notes (so they can have vibrato).
//!
//! The performer looks at the previous and next note and at the whole
//! phrase, so it needs to see ahead. Offline that's free; inside a DAW the
//! host can hand it the clip's notes (see docs/DESIGN.md).

use crate::curves::{Curves, Frame, Pluck, FRAME_RATE};
use crate::dsp::smoothstep;
use crate::instrument::Instrument;
use crate::rand::{hash_bipolar, Wander};
use crate::score::{Articulation, Note, Score};
use std::f64::consts::TAU;

/// Knobs over the whole performance.
#[derive(Clone, Debug)]
pub struct Style {
    /// 0 = deadpan (rules off), 1 = normal, 2 = very expressive.
    pub expression: f32,
    /// Vibrato depth scale. 0 = none.
    pub vibrato: f32,
    pub seed: u64,
}

impl Default for Style {
    fn default() -> Self {
        Self { expression: 1.0, vibrato: 1.0, seed: 1 }
    }
}

/// Notes closer than this count as connected.
const TOUCH: f64 = 0.03;
/// Loudness the bow dips to at a bow change (fraction of the note's level).
const BOW_CHANGE_FLOOR: f32 = 0.3;
/// How long the bow-change dip takes, seconds.
const BOW_CHANGE_TIME: f32 = 0.035;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Entry {
    /// Bow starts from silence.
    Fresh,
    /// Bow reverses right after the previous note.
    BowChange,
    /// Same bow, the left hand changes the note.
    Legato,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Exit {
    Legato,
    BowChange,
    /// Bow stops; the tone dies away with this time constant.
    Decay(f32),
}

#[derive(Clone, Debug)]
struct Plan {
    pitch: f32,
    settle: f32,
    entry: Entry,
    exit: Exit,
    attack: f32,
    accent: f32,
    sound_end: f64,
    level: f32,
    /// Level change over the note (hairpins inside it).
    ramp: f32,
    swell: f32,
    fade: f32,
    vib_depth: f32,
    vib_delay: f32,
    glide: f32,
    string: u8,
    crossing: bool,
    pizz: bool,
}

struct Phrase {
    start: f64,
    len: f64,
    mean_pitch: f32,
}

pub fn perform(score: &Score, inst: &Instrument, style: &Style) -> Curves {
    let notes = &score.notes;
    if notes.is_empty() {
        return Curves::default();
    }
    let plans = plan(notes, inst, style);
    let dt = 1.0 / FRAME_RATE;
    let frames_len = ((score.end() + 2.0) * FRAME_RATE).ceil() as usize;

    let mut curves = Curves { frames: Vec::with_capacity(frames_len), plucks: Vec::new() };
    for (n, p) in notes.iter().zip(&plans) {
        if p.pizz {
            curves.plucks.push(Pluck {
                time: n.start,
                pitch: p.pitch,
                velocity: p.level,
                brightness: (0.35 + 0.5 * p.level + inst.string_tone[p.string as usize]).clamp(0.0, 1.0),
                string: p.string,
            });
        }
    }

    let mut rate_wander = Wander::new(style.seed ^ 0x5EED, 0.8);
    let mut vib_phase = 0.0f64;
    let mut owner: Option<usize> = None;
    let mut smooth_loud = 0.0f32;
    // Loudness can't jump: the bow has mass. ~8 ms smoothing.
    let loud_keep = (-(dt as f32) / 0.008).exp();

    for f in 0..frames_len {
        let t = f as f64 * dt;
        loop {
            let next = owner.map_or(0, |o| o + 1);
            if next < notes.len() && notes[next].start <= t {
                owner = Some(next);
            } else {
                break;
            }
        }
        let rate = inst.vibrato_rate + 0.35 * rate_wander.next(dt as f32);
        vib_phase = (vib_phase + TAU * rate as f64 * dt) % TAU;

        let mut frame = match owner {
            None => Frame { pitch: plans[0].pitch, brightness: 0.4, string: plans[0].string, ..Frame::default() },
            Some(i) => note_frame(notes, &plans, i, t, vib_phase as f32, inst),
        };
        smooth_loud = frame.loudness + loud_keep * (smooth_loud - frame.loudness);
        frame.loudness = smooth_loud;
        curves.frames.push(frame);
    }
    curves
}

fn note_frame(notes: &[Note], plans: &[Plan], i: usize, t: f64, vib_phase: f32, inst: &Instrument) -> Frame {
    let n = &notes[i];
    let p = &plans[i];
    let since = (t - n.start) as f32;

    // Bow envelope: how the attack rises…
    let rise = match p.entry {
        Entry::Legato => 1.0 - 0.06 * (-(since / 0.03).powi(2)).exp(),
        Entry::BowChange => BOW_CHANGE_FLOOR + (1.0 - BOW_CHANGE_FLOOR) * smoothstep(since / p.attack),
        Entry::Fresh => smoothstep(since / p.attack),
    };
    let bite = 1.0 + 0.35 * p.accent * (-since / 0.08).exp() * smoothstep(since / p.attack);
    // …and how the note leaves.
    let fall = match p.exit {
        Exit::Legato => 1.0,
        Exit::BowChange => {
            let to_end = (p.sound_end - t) as f32;
            1.0 - (1.0 - BOW_CHANGE_FLOOR) * smoothstep(1.0 - to_end / BOW_CHANGE_TIME)
        }
        Exit::Decay(tau) => {
            if t > p.sound_end {
                (-((t - p.sound_end) as f32) / tau).exp()
            } else {
                1.0
            }
        }
    };
    let prog = (since / n.dur as f32).clamp(0.0, 1.0);
    let shape = 1.0 + p.swell * (std::f32::consts::PI * prog).sin() - p.fade * prog;
    let loudness = if p.pizz { 0.0 } else { ((p.level + p.ramp * prog) * shape).max(0.0) * rise * bite * fall };

    // Left hand: settling intonation, vibrato, glides into the next note.
    let mut pitch = p.pitch + p.settle * (-since / 0.05).exp();
    let mut vib = p.vib_depth * smoothstep((since - p.vib_delay) / 0.3);
    if let (Some(qn), Some(q)) = (notes.get(i + 1), plans.get(i + 1)) {
        let to_next = (qn.start - t) as f32;
        if q.glide > 0.0 && to_next < q.glide {
            let s = smoothstep(1.0 - to_next / q.glide);
            pitch += (q.pitch - pitch) * s;
            vib *= 1.0 - s;
        }
    }
    pitch += vib * vib_phase.sin();

    // Bow contact: louder and accented means closer to the bridge, brighter.
    let tone = inst.string_tone[p.string as usize];
    let attack_bite = if p.entry == Entry::Legato { 0.0 } else { (0.12 + 0.25 * p.accent) * (-since / 0.09).exp() };
    let brightness = (0.5 + 0.45 * (loudness - 0.5) + tone + attack_bite).clamp(0.0, 1.0);

    let spike = match p.entry {
        Entry::Legato if p.crossing => 0.12,
        Entry::Legato => 0.0,
        Entry::BowChange => 0.3,
        Entry::Fresh => 0.35,
    } + 0.4 * p.accent;
    let noise = (0.05 + spike * (-since / 0.035).exp() + if loudness < 0.25 { 0.04 } else { 0.0 }).min(1.0);

    Frame { pitch, loudness, brightness, noise, string: p.string }
}

fn phrases(notes: &[Note]) -> (Vec<usize>, Vec<Phrase>) {
    let mut ids = vec![0; notes.len()];
    let mut id = 0;
    for i in 1..notes.len() {
        let gap = notes[i].start - notes[i - 1].end();
        let pizz_change = (notes[i].articulation == Articulation::Pizzicato)
            != (notes[i - 1].articulation == Articulation::Pizzicato);
        if gap > 0.15 || pizz_change {
            id += 1;
        }
        ids[i] = id;
    }
    let mut out: Vec<Phrase> = Vec::new();
    for k in 0..=id {
        let members: Vec<&Note> = notes.iter().zip(&ids).filter(|(_, &p)| p == k).map(|(n, _)| n).collect();
        let start = members[0].start;
        let end = members.iter().map(|n| n.end()).fold(start, f64::max);
        // Weight by duration: long notes define the register more than passing ones.
        let w: f64 = members.iter().map(|n| n.dur).sum();
        let mean = members.iter().map(|n| n.pitch as f64 * n.dur).sum::<f64>() / w.max(1e-9);
        out.push(Phrase { start, len: end - start, mean_pitch: mean as f32 });
    }
    (ids, out)
}

fn plan(notes: &[Note], inst: &Instrument, style: &Style) -> Vec<Plan> {
    let e = style.expression;
    let (phrase_of, phrases) = phrases(notes);
    let mut plans: Vec<Plan> = Vec::with_capacity(notes.len());
    let mut prev_string: Option<u8> = None;

    for (i, n) in notes.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| &notes[j]);
        let next = notes.get(i + 1);
        let pizz = n.articulation == Articulation::Pizzicato;
        let touch_prev = prev.is_some_and(|p| n.start - p.end() < TOUCH);
        let touch_next = next.is_some_and(|q| q.start - n.end() < TOUCH);
        let last_in_phrase = next.is_none() || phrase_of[i + 1] != phrase_of[i];

        let entry = match prev {
            _ if pizz => Entry::Fresh,
            Some(p) if touch_prev && p.articulation != Articulation::Pizzicato && p.articulation != Articulation::Staccato => {
                if p.slur {
                    Entry::Legato
                } else {
                    Entry::BowChange
                }
            }
            _ => Entry::Fresh,
        };
        let exit = match next {
            _ if pizz => Exit::Decay(0.1),
            Some(q) if touch_next && q.articulation != Articulation::Pizzicato && n.articulation != Articulation::Staccato => {
                if n.slur {
                    Exit::Legato
                } else {
                    Exit::BowChange
                }
            }
            _ => Exit::Decay(match n.articulation {
                Articulation::Staccato => 0.03,
                _ if last_in_phrase => 0.2,
                _ => 0.07,
            }),
        };

        // Which string.
        let string = choose_string(n.pitch, prev_string, n.dur > 0.5 && !pizz, inst);
        let crossing = prev_string.is_some_and(|s| s != string);
        prev_string = Some(string);
        let open = (n.pitch - inst.open_strings[string as usize]).abs() < 0.01;

        // Loudness.
        let ph = &phrases[phrase_of[i]];
        let x = if ph.len > 0.0 { ((n.start + n.dur * 0.5 - ph.start) / ph.len) as f32 } else { 0.5 };
        let arch = 0.16 * hump(x, 0.62) - 0.08;
        let mut level = n.dynamic + e * (arch + 0.012 * (n.pitch - ph.mean_pitch));
        if n.dur < 0.18 {
            level -= e * 0.04;
        }
        let level = level.clamp(0.05, 1.0);
        let accent = match n.articulation {
            Articulation::Accent => 1.0,
            Articulation::Tenuto => 0.0,
            _ if entry == Entry::Fresh => 0.2,
            _ => 0.0,
        };

        // Intonation, in semitones.
        let mut cents = 3.0 * e * hash_bipolar(style.seed, i);
        if let Some(q) = next {
            if (q.pitch - n.pitch - 1.0).abs() < 0.01 && n.dur > 0.15 {
                cents += 7.0 * e; // leading tone leans up
            }
        }
        if open {
            cents = 0.0;
        }
        let settle = if entry == Entry::Legato || open || pizz {
            0.0
        } else {
            e * 0.05 * hash_bipolar(style.seed ^ 0xA77AC, i)
        };

        let vib_depth = if pizz || open || n.dur < 0.22 { 0.0 } else { (0.13 + 0.14 * level) * style.vibrato };
        let prev_vibrated = plans.last().is_some_and(|p: &Plan| p.vib_depth > 0.0);
        let vib_delay = if entry == Entry::Legato && prev_vibrated { 0.05 } else { 0.14 };

        let glide = match (entry, prev) {
            (Entry::Legato, Some(p)) => {
                let k = (n.pitch - p.pitch).abs();
                if k < 0.5 {
                    0.0
                } else if k >= 5.0 && p.dur > 0.3 {
                    0.06 + 0.05 * e // an audible shift
                } else if crossing {
                    0.02
                } else {
                    0.03
                }
            }
            (Entry::BowChange, _) => 0.012,
            _ => 0.0,
        };

        let attack = match n.articulation {
            Articulation::Accent => 0.025,
            Articulation::Staccato => 0.02,
            Articulation::Tenuto => 0.07,
            Articulation::Pizzicato => 0.01,
            Articulation::Detache => match entry {
                Entry::BowChange => 0.045,
                _ => 0.05 + 0.08 * (1.0 - n.dynamic), // soft starts are slower
            },
        };
        let sound_end = n.start + n.dur * if n.articulation == Articulation::Staccato { 0.45 } else { 1.0 };

        plans.push(Plan {
            pitch: n.pitch + cents / 100.0,
            settle,
            entry,
            exit,
            attack,
            accent,
            sound_end,
            level,
            ramp: n.dynamic_end - n.dynamic,
            // A written hairpin overrides the automatic swell and fade.
            swell: if n.dur > 0.9 && !pizz && n.dynamic_end == n.dynamic { 0.07 * e } else { 0.0 },
            fade: if last_in_phrase && !pizz && n.dynamic_end <= n.dynamic { 0.25 * e } else { 0.0 },
            vib_depth,
            vib_delay,
            glide,
            string,
            crossing,
            pizz,
        });
    }
    plans
}

/// 0 at the ends, 1 at `peak`.
fn hump(x: f32, peak: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let u = if x < peak { x / peak } else { (1.0 - x) / (1.0 - peak) };
    (std::f32::consts::FRAC_PI_2 * u).sin()
}

/// Stay on the current string while the note is in reach; otherwise take the
/// highest string where the note sits in the first few positions. Long notes
/// avoid open strings so they can sing with vibrato.
fn choose_string(pitch: f32, prev: Option<u8>, avoid_open: bool, inst: &Instrument) -> u8 {
    let os = &inst.open_strings;
    let is_open = |s: usize| (pitch - os[s]).abs() < 0.01;
    if let Some(s) = prev {
        let s = s as usize;
        let reach = if s == 3 { 24.0 } else { 9.0 };
        if pitch >= os[s] - 0.01 && pitch <= os[s] + reach && !(avoid_open && is_open(s) && s > 0) {
            return s as u8;
        }
    }
    let mut best = 0;
    for s in 0..4 {
        if pitch >= os[s] - 0.01 && (s == 3 || pitch <= os[s] + 7.01) {
            best = s;
        }
    }
    if avoid_open && is_open(best) && best > 0 {
        best -= 1;
    }
    best as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::cello;

    fn frame_at(c: &Curves, t: f64) -> Frame {
        c.frames[(t * FRAME_RATE) as usize]
    }

    #[test]
    fn silence_before_and_sound_during() {
        let s = Score::parse("tempo 60\nr:q mf C3:q D3").unwrap();
        let c = perform(&s, &cello(), &Style::default());
        assert!(frame_at(&c, 0.5).loudness < 1e-4);
        assert!(frame_at(&c, 1.5).loudness > 0.3);
        // Two seconds of tail after the last note, then nothing.
        assert!(c.frames.last().unwrap().loudness < 1e-3);
    }

    #[test]
    fn slurs_stay_up_and_bow_changes_dip() {
        let inst = cello();
        let slurred = perform(&Score::parse("tempo 60\n(C3:q D3)").unwrap(), &inst, &Style::default());
        let separate = perform(&Score::parse("tempo 60\nC3:q D3").unwrap(), &inst, &Style::default());
        let min_around = |c: &Curves| {
            (240..262).map(|f| c.frames[f].loudness).fold(f32::MAX, f32::min) // 0.96 s .. 1.05 s
        };
        assert!(min_around(&slurred) > 0.4, "slur dipped to {}", min_around(&slurred));
        assert!(min_around(&separate) < 0.3, "bow change only dipped to {}", min_around(&separate));
    }

    #[test]
    fn pitch_arrives_and_short_notes_have_no_vibrato() {
        let inst = cello();
        let s = Score::parse("tempo 60\nE3:s F3 G3 A3 B3 C4 D4 E4 F3:h").unwrap();
        let c = perform(&s, &inst, &Style::default());
        // Middle of the 4th sixteenth (A3 = 57, an open string on cello).
        let f = frame_at(&c, 0.875);
        assert!((f.pitch - 57.0).abs() < 0.08, "pitch {}", f.pitch);
        // The long F3 has vibrato after it settles.
        let late: Vec<f32> = (0..50).map(|k| frame_at(&c, 3.0 + k as f64 * 0.004).pitch).collect();
        let swing = late.iter().cloned().fold(f32::MIN, f32::max) - late.iter().cloned().fold(f32::MAX, f32::min);
        assert!(swing > 0.15, "vibrato swing {swing}");
    }

    #[test]
    fn long_open_string_notes_move_to_a_lower_string() {
        let inst = cello();
        assert_eq!(choose_string(50.0, None, false, &inst), 2); // D3, open D
        assert_eq!(choose_string(50.0, None, true, &inst), 1); // D3 on the G string
    }

    #[test]
    fn deadpan_has_flat_dynamics() {
        let s = Score::parse("tempo 60\nmf C3:q D3 E3 F3 G3").unwrap();
        let style = Style { expression: 0.0, vibrato: 0.0, seed: 1 };
        let c = perform(&s, &cello(), &style);
        let mids: Vec<f32> = (0..5).map(|k| frame_at(&c, k as f64 + 0.5).loudness).collect();
        for m in &mids[..4] {
            assert!((m - 0.58).abs() < 0.03, "{mids:?}");
        }
    }

    #[test]
    fn pizzicato_makes_plucks_not_bowing() {
        let s = Score::parse("pizz G2:q D3 arco").unwrap();
        let c = perform(&s, &cello(), &Style::default());
        assert_eq!(c.plucks.len(), 2);
        assert!(c.frames.iter().all(|f| f.loudness < 1e-4));
    }
}
