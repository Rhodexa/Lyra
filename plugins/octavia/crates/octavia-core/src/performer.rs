//! The performer: turns notes into what a cellist's hands would do.
//!
//! Rules, no learning. Each rule is small, named, and loosely follows the
//! KTH performance-rule research (Sundberg, Friberg et al.) and ordinary
//! string-playing habits.
//!
//! **It sees one second ahead, no more** ([`LOOKAHEAD`]). Every decision is
//! made only from notes already started or starting within that window, so
//! the same performer works offline, in Lyra (which can hand over upcoming
//! notes) and in any other host (which would delay the sound by one second
//! and get identical results).
//!
//! What it infers when the notes carry no hints:
//! * **Slurs**: overlapping notes are slurred. Fast runs of touching notes
//!   are slurred in groups of four, as a cellist would bow them. Other
//!   touching notes get separate bows.
//! * **Staccato**: short notes followed by a clear gap.
//! * **Accents**: notes clearly louder than their neighbours.
//! * **Spiccato**: fast notes bowed separately bounce (a drop, a tick of
//!   hair, then the string rings free). Repeated notes are never slurred.
//!
//! How it plays:
//! * **Phrases** are split at rests. Loudness grows over the first couple of
//!   seconds of a phrase and relaxes as the visible end approaches.
//! * **High-loud**: higher notes are played a little louder.
//! * **Short-soft**: very short notes are a little softer.
//! * **Bow changes** dip the loudness and puff some bow noise; **slurred**
//!   notes keep the bow moving: a finger lands in ~8 ms, a string crossing
//!   is instant, and only big expressive shifts slide.
//! * **Vibrato** starts a moment after the note, grows in, follows loudness,
//!   and is absent on short notes and open strings.
//! * **Intonation**: a few cents of human error that settles after the
//!   attack; leading tones lean up toward the note they resolve to.
//! * **Finger taps**: a finger landing knocks the fingerboard, hardest when
//!   it hammers down mid-slur; lifting off makes a smaller one.
//! * **Releases**: when the bow stops, the string rings on and darkens.
//!   When the bow crosses to another string, the one it left rings on by
//!   itself (open strings for long).
//! * **Fingering**: a left hand covering ~4 semitones on all strings at once.
//!   Open strings and notes under the hand are free, crossing strings is
//!   cheap, shifting the hand is expensive. So arpeggios cross strings (the
//!   Bach prelude: open G, open D, A string) instead of leaping up one string.
//!   Long notes avoid open strings so they can have vibrato.

use crate::curves::{Curves, Frame, Pluck, Ring, Tap, FRAME_RATE};
use crate::dsp::smoothstep;
use crate::instrument::Instrument;
use crate::rand::{hash_bipolar, Wander};
use crate::score::{Articulation, Note, Score};
use std::f64::consts::TAU;

/// How far ahead the performer looks, in seconds.
pub const LOOKAHEAD: f64 = 1.0;

/// Knobs over the whole performance.
#[derive(Clone, Debug)]
pub struct Style {
    /// 0 = deadpan (rules off), 1 = normal, 2 = very expressive.
    pub expression: f32,
    /// Vibrato depth scale. 0 = none.
    pub vibrato: f32,
    /// Steady bow noise under the tone (rosin, hair). 1 = normal.
    pub breath: f32,
    /// Pluck every note (the pizzicato patch).
    pub pizzicato: bool,
    pub seed: u64,
}

impl Default for Style {
    fn default() -> Self {
        Self { expression: 1.0, vibrato: 1.0, breath: 1.0, pizzicato: false, seed: 1 }
    }
}

/// Something the instrument must do at an exact time, besides the frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Pluck(Pluck),
    Tap(Tap),
    Ring(Ring),
}

/// Notes closer than this count as connected.
const TOUCH: f64 = 0.03;
/// A gap longer than this ends a phrase.
const PHRASE_GAP: f64 = 0.15;
/// Notes are planned this long before they start (before any glide or tap).
const PLAN_LEAD: f64 = 0.12;
/// Notes shorter than this are "fast" (bowed in slurred groups).
const FAST: f64 = 0.23;
/// Loudness the bow dips to at a bow change (fraction of the note's level).
const BOW_CHANGE_FLOOR: f32 = 0.3;
/// How long the bow-change dip takes, seconds.
const BOW_CHANGE_TIME: f32 = 0.035;
/// The phrase-end diminuendo happens over this many seconds (< LOOKAHEAD).
const FADE_TIME: f64 = 0.85;

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
    attack: f32,
    accent: f32,
    staccato: bool,
    /// Bounced bow: a drop, a tick of hair, then the string rings free.
    spiccato: bool,
    level: f32,
    ramp: f32,
    swell: f32,
    fade: f32,
    vib_depth: f32,
    vib_delay: f32,
    glide: f32,
    string: u8,
    crossing: bool,
    pizz: bool,
    phrase_start: f64,
}

/// A streaming performer. Feed it notes (in start order, ideally at least
/// [`LOOKAHEAD`] before they start) and pull one frame at a time.
pub struct Performer {
    inst: Instrument,
    style: Style,
    notes: Vec<Note>,
    plans: Vec<Plan>,
    frame: u64,
    owner: Option<usize>,
    vib_phase: f64,
    rate_wander: Wander,
    smooth_loud: f32,
    loud_keep: f32,
    prev_string: Option<u8>,
    /// Where the left hand sits (see [`choose_fingering`]).
    hand: f32,
}

impl Performer {
    pub fn new(inst: &Instrument, style: &Style) -> Self {
        Self {
            inst: inst.clone(),
            style: style.clone(),
            notes: Vec::with_capacity(1024),
            plans: Vec::with_capacity(1024),
            frame: 0,
            owner: None,
            vib_phase: 0.0,
            rate_wander: Wander::new(style.seed ^ 0x5EED, 0.8),
            smooth_loud: 0.0,
            // Loudness can't jump: the bow has mass. ~8 ms smoothing.
            loud_keep: (-(1.0 / FRAME_RATE as f32) / 0.008).exp(),
            prev_string: None,
            hand: 1.0,
        }
    }

    /// Adds a note. Notes already planned can't be moved, so a note arriving
    /// out of order is placed after them.
    pub fn add_note(&mut self, n: Note) {
        let pos = self.notes.partition_point(|m| m.start <= n.start).max(self.plans.len());
        self.notes.insert(pos, n);
    }

    /// Seconds since the start, of the next frame.
    pub fn time(&self) -> f64 {
        self.frame as f64 / FRAME_RATE
    }

    /// When the last known note has fully died away.
    pub fn end(&self) -> f64 {
        self.notes.iter().map(Note::end).fold(0.0, f64::max) + 2.0
    }

    /// The next frame. Pluck and tap events due soon are appended to `events`
    /// (with their exact times, which are never in the past).
    pub fn next_frame(&mut self, events: &mut Vec<Event>) -> Frame {
        let t = self.time();
        let dt = 1.0 / FRAME_RATE;
        let visible = self.visible(t);
        while self.plans.len() < visible && self.notes[self.plans.len()].start - PLAN_LEAD <= t {
            self.plan(self.plans.len(), t, events);
        }
        loop {
            let next = self.owner.map_or(0, |o| o + 1);
            if next < self.plans.len() && self.notes[next].start <= t {
                self.owner = Some(next);
            } else {
                break;
            }
        }
        let rate = self.inst.vibrato_rate + 0.35 * self.rate_wander.next(dt as f32);
        self.vib_phase = (self.vib_phase + TAU * rate as f64 * dt) % TAU;

        let mut frame = match self.owner {
            None => Frame {
                pitch: self.notes.first().map_or(48.0, |n| n.pitch),
                brightness: 0.4,
                ..Frame::default()
            },
            Some(i) => self.note_frame(i, t),
        };
        self.smooth_loud = frame.loudness + self.loud_keep * (self.smooth_loud - frame.loudness);
        frame.loudness = self.smooth_loud;
        self.frame += 1;
        frame
    }

    // ── what can be seen ────────────────────────────────────────────────

    /// How many notes start within the look-ahead window at time `t`.
    fn visible(&self, t: f64) -> usize {
        self.notes.partition_point(|n| n.start <= t + LOOKAHEAD)
    }

    fn next_visible(&self, i: usize, t: f64) -> Option<&Note> {
        (i + 1 < self.visible(t)).then(|| &self.notes[i + 1])
    }

    fn is_pizz(&self, n: &Note) -> bool {
        self.style.pizzicato || n.articulation == Articulation::Pizzicato
    }

    /// Is note `i` slurred into note `i + 1`? Needs `plans[i]`.
    fn slurred(&self, i: usize) -> bool {
        self.slur_rule(i, self.plans[i].staccato || self.plans[i].spiccato)
    }

    /// The slur decision, given whether note `i` itself is short and lifted.
    fn slur_rule(&self, i: usize, lifted: bool) -> bool {
        let (n, q) = (&self.notes[i], &self.notes[i + 1]);
        if !touching(n, q) || self.is_pizz(n) || self.is_pizz(q) || lifted {
            return false;
        }
        if let Some(s) = n.slur {
            return s;
        }
        if (q.pitch - n.pitch).abs() < 0.01 {
            return false; // a repeated note is always re-articulated
        }
        if q.start < n.end() - 0.005 {
            return true; // drawn overlapping
        }
        if n.dur < FAST && q.dur < FAST {
            return self.run_index(i) % 4 != 3; // fast runs: four to a bow
        }
        false
    }

    /// Position of note `i` in its run of fast, touching notes.
    fn run_index(&self, i: usize) -> usize {
        let mut j = i;
        while j > 0 {
            let (a, b) = (&self.notes[j - 1], &self.notes[j]);
            if touching(a, b) && a.dur < FAST && b.dur < FAST && a.slur.is_none() {
                j -= 1;
            } else {
                break;
            }
        }
        i - j
    }

    /// Where the phrase containing note `i` ends, if that's visible yet.
    fn phrase_end(&self, i: usize, t: f64) -> Option<f64> {
        let visible = self.visible(t);
        let mut j = i;
        loop {
            let end = self.notes[j].end();
            if j + 1 >= visible {
                // Nothing more in sight: the phrase ends if the window
                // reaches past this note's end plus a phrase gap.
                return (end + PHRASE_GAP < t + LOOKAHEAD).then_some(end);
            }
            if self.notes[j + 1].start - end > PHRASE_GAP {
                return Some(end);
            }
            j += 1;
        }
    }

    fn exit(&self, i: usize, t: f64) -> Exit {
        let (n, p) = (&self.notes[i], &self.plans[i]);
        if p.pizz {
            return Exit::Decay(0.1);
        }
        if p.spiccato {
            return Exit::Decay(0.05); // the bow is already off the string
        }
        let ring = if p.staccato { 0.07 } else { 0.3 };
        match self.next_visible(i, t) {
            Some(q) if touching(n, q) && !self.is_pizz(q) && !p.staccato => {
                if self.slurred(i) {
                    Exit::Legato
                } else {
                    Exit::BowChange
                }
            }
            Some(q) if q.start - n.end() <= PHRASE_GAP => Exit::Decay(if p.staccato { 0.07 } else { 0.1 }),
            _ => Exit::Decay(ring),
        }
    }

    /// When note `i` stops sounding (an overlapping next note cuts it).
    fn sound_end(&self, i: usize, t: f64) -> f64 {
        let n = &self.notes[i];
        match self.next_visible(i, t) {
            Some(q) if q.start < n.end() => q.start,
            _ => n.end(),
        }
    }

    // ── planning a note, just before it starts ─────────────────────────

    fn plan(&mut self, i: usize, t: f64, events: &mut Vec<Event>) {
        let e = self.style.expression;
        let inst = &self.inst;
        let n = &self.notes[i];
        let pizz = self.is_pizz(n);
        let prev = i.checked_sub(1).map(|j| (&self.notes[j], &self.plans[j]));
        let next = self.next_visible(i, t);

        let entry = match prev {
            _ if pizz => Entry::Fresh,
            Some((pn, pp)) if touching(pn, n) && !pp.pizz && !pp.staccato && !pp.spiccato => {
                if self.slurred(i - 1) {
                    Entry::Legato
                } else {
                    Entry::BowChange
                }
            }
            _ => Entry::Fresh,
        };
        let staccato = !pizz
            && match n.articulation {
                Articulation::Staccato => true,
                Articulation::Auto => {
                    n.dur < 0.3 && next.is_none_or(|q| q.start - n.end() >= (0.35 * n.dur).max(0.05))
                }
                _ => false,
            };
        // Fast notes bowed separately (no slur on either side) bounce.
        let fast = next.is_some_and(|q| q.start - n.start < FAST + 0.02)
            || prev.is_some_and(|(pn, _)| n.start - pn.start < FAST + 0.02);
        let slur_out = next.is_some() && i + 1 < self.notes.len() && self.slur_rule(i, staccato);
        let spiccato = !pizz
            && match n.articulation {
                Articulation::Spiccato => true,
                Articulation::Auto => fast && !staccato && entry != Entry::Legato && !slur_out,
                _ => false,
            };

        // Neighbourhood: what's been and what's in sight.
        let window = self.notes[..self.visible(t)].iter().filter(|m| m.start >= n.start - 4.0);
        let (mut w, mut wp, mut wd, mut count) = (0.0f64, 0.0f64, 0.0f32, 0usize);
        for m in window {
            w += m.dur;
            wp += m.pitch as f64 * m.dur;
            wd += m.dynamic;
            count += 1;
        }
        let mean_pitch = if w > 0.0 { (wp / w) as f32 } else { n.pitch };
        let mean_dyn = if count > 0 { wd / count as f32 } else { n.dynamic };

        let accent = match n.articulation {
            Articulation::Accent => 1.0,
            Articulation::Auto if n.dynamic > mean_dyn + 0.12 => 1.0,
            Articulation::Tenuto => 0.0,
            _ if entry == Entry::Fresh => 0.2,
            _ => 0.0,
        };

        let fingering = choose_fingering(n.pitch, self.prev_string, self.hand, n.dur > 0.5 && !pizz, inst);
        let string = fingering.string;
        let crossing = self.prev_string.is_some_and(|s| s != string);
        let open = (n.pitch - inst.open_strings[string as usize]).abs() < 0.01;

        // Phrase shape: grow into the phrase, relax toward its visible end.
        let phrase_start = match prev {
            Some((pn, pp)) if n.start - pn.end() <= PHRASE_GAP => pp.phrase_start,
            _ => n.start,
        };
        let into = (n.start - phrase_start) as f32;
        let arch = -0.04 + 0.08 * smoothstep(into / 2.0);
        let mut level = n.dynamic + e * (arch + 0.012 * (n.pitch - mean_pitch));
        if n.dur < 0.18 {
            level -= e * 0.04;
        }
        let level = level.clamp(0.05, 1.0);

        // Intonation, in semitones.
        let mut cents = 3.0 * e * hash_bipolar(self.style.seed, i);
        if let Some(q) = next {
            if (q.pitch - n.pitch - 1.0).abs() < 0.01 && n.dur > 0.15 {
                cents += 7.0 * e; // leading tone leans up
            }
        }
        if open {
            cents = 0.0;
        }
        let settle =
            if entry == Entry::Legato || open || pizz { 0.0 } else { e * 0.05 * hash_bipolar(self.style.seed ^ 0xA77AC, i) };

        let vib_depth = if pizz || open || spiccato || n.dur < 0.22 { 0.0 } else { (0.13 + 0.14 * level) * self.style.vibrato };
        let prev_vibrated = prev.is_some_and(|(_, pp)| pp.vib_depth > 0.0);
        let vib_delay = if entry == Entry::Legato && prev_vibrated { 0.05 } else { 0.14 };

        let glide = match (entry, prev) {
            (Entry::Legato, Some((pn, _))) => {
                let k = (n.pitch - pn.pitch).abs();
                if k < 0.5 {
                    0.0
                } else if crossing {
                    0.004 // another string: nothing to slide on
                } else if fingering.shift > 0.0 && k >= 5.0 && pn.dur > 0.3 {
                    0.05 + 0.04 * e // an expressive shift: a real slide
                } else if fingering.shift > 0.0 {
                    (0.02 + 0.007 * fingering.shift).min(0.06) // the hand travels
                } else {
                    0.008 // a finger lands: fast, no audible slide
                }
            }
            (Entry::BowChange, _) => 0.012,
            _ => 0.0,
        };

        let attack = if pizz {
            0.01
        } else if accent >= 1.0 {
            0.025
        } else if staccato {
            0.02
        } else if n.articulation == Articulation::Tenuto {
            0.07
        } else if entry == Entry::BowChange {
            0.045
        } else {
            0.05 + 0.08 * (1.0 - n.dynamic) // soft starts are slower
        };

        let hairpin = n.dynamic_end != n.dynamic;
        let plan = Plan {
            pitch: n.pitch + cents / 100.0,
            settle,
            entry,
            attack,
            accent,
            staccato,
            spiccato,
            level,
            ramp: n.dynamic_end - n.dynamic,
            swell: if n.dur > 0.9 && !pizz && !hairpin { 0.07 * e } else { 0.0 },
            fade: if pizz || n.dynamic_end > n.dynamic { 0.0 } else { 0.25 * e },
            vib_depth,
            vib_delay,
            glide,
            string,
            crossing,
            pizz,
            phrase_start,
        };

        // Events.
        if pizz {
            events.push(Event::Pluck(Pluck {
                time: n.start,
                pitch: plan.pitch,
                velocity: level,
                brightness: (0.35 + 0.5 * level + inst.string_tone[string as usize]).clamp(0.0, 1.0),
                string,
            }));
        } else if let Some(strength) = tap_strength(entry, prev.map(|(pn, pp)| (pn, pp.string)), n, string, open) {
            let lead = if entry == Entry::Legato { glide as f64 } else { 0.015 };
            events.push(Event::Tap(Tap { time: (n.start - lead).max(t), strength }));
        }

        // Crossing away from a string that was sounding: it rings on.
        if let (true, false, Some((pn, pp))) = (crossing, pizz, prev) {
            if entry != Entry::Fresh {
                let open_prev = (pn.pitch - inst.open_strings[pp.string as usize]).abs() < 0.01;
                events.push(Event::Ring(Ring {
                    time: (n.start - glide as f64).max(t),
                    pitch: pp.pitch,
                    level: pp.level,
                    string: pp.string,
                    open: open_prev,
                }));
            }
        }

        self.prev_string = Some(string);
        self.hand = fingering.hand;
        self.plans.push(plan);
    }

    // ── one frame of a sounding note ────────────────────────────────────

    fn note_frame(&self, i: usize, t: f64) -> Frame {
        let n = &self.notes[i];
        let p = &self.plans[i];
        let since = (t - n.start) as f32;
        let sound_end = self.sound_end(i, t);
        let exit = self.exit(i, t);

        // Bow envelope: how the attack rises…
        let rise = match p.entry {
            Entry::Legato => 1.0 - 0.06 * (-(since / 0.03).powi(2)).exp(),
            Entry::BowChange => BOW_CHANGE_FLOOR + (1.0 - BOW_CHANGE_FLOOR) * smoothstep(since / p.attack),
            Entry::Fresh => smoothstep(since / p.attack),
        };
        let bite = 1.0 + 0.35 * p.accent * (-since / 0.08).exp() * smoothstep(since / p.attack);
        // …and how the note leaves.
        let fall = match exit {
            Exit::Legato => 1.0,
            Exit::BowChange => {
                let to_end = (sound_end - t) as f32;
                1.0 - (1.0 - BOW_CHANGE_FLOOR) * smoothstep(1.0 - to_end / BOW_CHANGE_TIME)
            }
            Exit::Decay(tau) => {
                if t > sound_end {
                    (-((t - sound_end) as f32) / tau).exp()
                } else {
                    1.0
                }
            }
        };
        let prog = (since / n.dur as f32).clamp(0.0, 1.0);
        // Relax into the end of the phrase, once it's in sight.
        let fade = match self.phrase_end(i, t) {
            Some(end) if end <= n.end() + 1e-9 => p.fade * smoothstep(1.0 - ((end - t) / FADE_TIME) as f32),
            _ => 0.0,
        };
        let shape = 1.0 + p.swell * (std::f32::consts::PI * prog).sin() - fade;
        // A bounce: the bow drops, grips for a moment and lifts off; the
        // string keeps ringing until the next drop.
        let bounce = 0.18 + 0.82 * smoothstep(since / 0.01) * (-(since - 0.01).max(0.0) / 0.065).exp();
        let loudness = if p.pizz {
            0.0
        } else if p.spiccato {
            (p.level * (1.0 + 0.25 * p.accent)).max(0.0) * bounce * fall
        } else {
            ((p.level + p.ramp * prog) * shape).max(0.0) * rise * bite * fall
        };

        // Left hand: settling intonation, vibrato, moving into the next note.
        let mut pitch = p.pitch + p.settle * (-since / 0.05).exp();
        let mut vib = p.vib_depth * smoothstep((since - p.vib_delay) / 0.3);
        if let (Some(qn), Some(q)) = (self.next_visible(i, t), self.plans.get(i + 1)) {
            let to_next = (qn.start - t) as f32;
            if q.glide > 0.0 && to_next < q.glide {
                let s = smoothstep(1.0 - to_next / q.glide);
                pitch += (q.pitch - pitch) * s;
                vib *= 1.0 - s;
            }
        }
        pitch += vib * (self.vib_phase as f32).sin();

        // Bow contact: louder and accented means closer to the bridge, brighter.
        let tone = self.inst.string_tone[p.string as usize];
        let attack_bite = if p.entry == Entry::Legato { 0.0 } else { (0.12 + 0.25 * p.accent) * (-since / 0.09).exp() };
        // A free-ringing string (bow lifted or stopped) loses its highs first.
        let release_dark = if matches!(exit, Exit::Decay(_)) && t > sound_end { 0.35 * (1.0 - fall) } else { 0.0 };
        // A lifted bow leaves the string ringing free: darker by the moment.
        let lifted = if p.spiccato { 0.15 * (-since / 0.012).exp() - 0.22 * (1.0 - (-since / 0.08).exp()) } else { 0.0 };
        let brightness = (0.5 + 0.45 * (loudness - 0.5) + tone + attack_bite + lifted - release_dark).clamp(0.0, 1.0);

        let spike = match p.entry {
            Entry::Legato if p.crossing => 0.15,
            Entry::Legato => 0.08, // the finger landing
            Entry::BowChange => 0.3,
            Entry::Fresh => 0.35,
        } + 0.4 * p.accent;
        // A bowed string always breathes: a steady layer of rosin noise under the tone.
        let noise = if p.spiccato {
            // The hair hits the string (a tick), then hardly any rosin: the bow is off.
            (0.3 * self.style.breath * bounce + 0.6 * (-since / 0.015).exp()).min(1.0)
        } else {
            (0.3 * self.style.breath + spike * (-since / 0.035).exp() + if loudness < 0.25 { 0.08 } else { 0.0 }).min(1.0)
        };

        Frame { pitch, loudness, brightness, noise, string: p.string }
    }
}

fn touching(a: &Note, b: &Note) -> bool {
    b.start - a.end() < TOUCH
}

/// How hard a finger knocks the fingerboard for this note, if at all.
fn tap_strength(entry: Entry, prev: Option<(&Note, u8)>, n: &Note, string: u8, open: bool) -> Option<f32> {
    let s = match (entry, prev) {
        // Mid-slur on the same string: hammer down, or lift off.
        (Entry::Legato, Some((pn, ps))) if ps == string => {
            if n.pitch > pn.pitch {
                0.7
            } else if open {
                0.45
            } else {
                0.3
            }
        }
        (Entry::Legato, Some(_)) if !open => 0.25,
        (Entry::Legato, _) => 0.0,
        // A new bow: the finger went down just before it.
        (_, Some((pn, _))) if (pn.pitch - n.pitch).abs() > 0.01 && !open => 0.3,
        (_, None) if !open => 0.3,
        _ => 0.0,
    };
    (s > 0.0).then_some(s)
}

/// Performs a whole score offline. Exactly what the streaming performer does,
/// with every note known up front.
pub fn perform(score: &Score, inst: &Instrument, style: &Style) -> Curves {
    let mut p = Performer::new(inst, style);
    for n in &score.notes {
        p.add_note(n.clone());
    }
    let frames = (p.end() * FRAME_RATE).ceil() as usize;
    let mut curves = Curves { frames: Vec::with_capacity(frames), ..Curves::default() };
    let mut events = Vec::new();
    for _ in 0..frames {
        curves.frames.push(p.next_frame(&mut events));
        for e in events.drain(..) {
            match e {
                Event::Pluck(x) => curves.plucks.push(x),
                Event::Tap(x) => curves.taps.push(x),
                Event::Ring(x) => curves.rings.push(x),
            }
        }
    }
    curves
}

/// How many semitones the left hand covers without moving (cello, lower
/// positions: four fingers over about a major third, plus a stretch).
const HAND_SPAN: f32 = 4.0;

/// A fingering choice: which string, where the hand ends up, how far it moved.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fingering {
    string: u8,
    hand: f32,
    shift: f32,
}

/// Picks the cheapest way to reach `pitch`, as a cellist would: an open
/// string or a note already under the hand is free, crossing to a neighbour
/// string is cheap, moving the hand (a shift) is expensive and more so the
/// further it goes, and climbing high on the low strings is discouraged.
/// Long notes avoid open strings so they can have vibrato.
///
/// `hand` is the lowest offset (semitones above any open string) the hand
/// covers; the strings are tuned in fifths, so it's the same on all of them.
fn choose_fingering(pitch: f32, prev: Option<u8>, hand: f32, avoid_open: bool, inst: &Instrument) -> Fingering {
    let mut best = Fingering { string: 0, hand, shift: 0.0 };
    let mut best_cost = f32::MAX;
    for (s, &open) in inst.open_strings.iter().enumerate() {
        let off = pitch - open;
        if off < -0.01 {
            continue;
        }
        let (mut cost, new_hand, shift) = if off.abs() < 0.01 {
            (if avoid_open { 2.5 } else { 0.0 }, hand, 0.0)
        } else if off >= hand - 0.01 && off <= hand + HAND_SPAN + 0.01 {
            (0.0, hand, 0.0)
        } else {
            let to = if off < hand { off } else { off - HAND_SPAN }.max(1.0);
            let shift = (to - hand).abs();
            (1.0 + 0.25 * shift, to, shift)
        };
        if s < 3 {
            cost += 0.15 * (off - 7.0).max(0.0);
        }
        cost += prev.map_or(0.0, |p| 0.35 * (s as f32 - p as f32).abs());
        if cost < best_cost {
            best_cost = cost;
            best = Fingering { string: s as u8, hand: new_hand, shift };
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::cello;

    fn frame_at(c: &Curves, t: f64) -> Frame {
        c.frames[(t * FRAME_RATE) as usize]
    }

    fn roll(text: &str) -> Score {
        Score::parse_roll(&format!("tempo 60\n{text}")).unwrap()
    }

    #[test]
    fn silence_before_and_sound_during() {
        let s = Score::parse("tempo 60\nr:q mf C3:q D3").unwrap();
        let c = perform(&s, &cello(), &Style::default());
        assert!(frame_at(&c, 0.5).loudness < 1e-4);
        assert!(frame_at(&c, 1.5).loudness > 0.3);
        assert!(c.frames.last().unwrap().loudness < 1e-3);
    }

    #[test]
    fn slurs_stay_up_and_bow_changes_dip() {
        let inst = cello();
        let slurred = perform(&Score::parse("tempo 60\n(C3:q D3)").unwrap(), &inst, &Style::default());
        let separate = perform(&Score::parse("tempo 60\nC3:q D3").unwrap(), &inst, &Style::default());
        let min_around = |c: &Curves| (240..262).map(|f| c.frames[f].loudness).fold(f32::MAX, f32::min);
        assert!(min_around(&slurred) > 0.4, "slur dipped to {}", min_around(&slurred));
        assert!(min_around(&separate) < 0.3, "bow change only dipped to {}", min_around(&separate));
    }

    #[test]
    fn infers_slurs_from_overlap_and_fast_runs() {
        let inst = cello();
        // Overlapping quarter notes: slurred, though nothing says so.
        let c = perform(&roll("0 1.05 C3 0.6\n1 1 D3 0.6"), &inst, &Style::default());
        let dip = (240..262).map(|f| c.frames[f].loudness).fold(f32::MAX, f32::min);
        assert!(dip > 0.4, "overlap should slur, dipped to {dip}");
        // Eight touching notes of 0.125 s are "fast" (under 0.23 s)
        // and get slurred four to a bow: the only bow
        // change is between notes 4 and 5.
        let mut text = String::new();
        for k in 0..8 {
            text += &format!("{} 0.125 {} 0.6\n", k as f64 * 0.125, ["C3", "D3", "E3", "F3"][k % 4]);
        }
        let c = perform(&roll(&text), &inst, &Style::default());
        let dip_at = |k: f64| {
            let f = (k * 0.125 * FRAME_RATE) as usize;
            (f - 6..f + 6).map(|f| c.frames[f].loudness).fold(f32::MAX, f32::min)
        };
        assert!(dip_at(2.0) > 0.35, "inside a slur: {}", dip_at(2.0));
        assert!(dip_at(4.0) < dip_at(2.0) - 0.1, "bow change at 4: {} vs {}", dip_at(4.0), dip_at(2.0));
    }

    #[test]
    fn repeated_fast_notes_bounce_instead_of_smearing() {
        let inst = cello();
        // Four touching C4s, 0.19 s each: re-articulated, each a bounce.
        let mut text = String::new();
        for k in 0..4 {
            text += &format!("{} 0.19 C4 0.6\n", k as f64 * 0.19);
        }
        let c = perform(&roll(&text), &inst, &Style::default());
        for k in 1..4 {
            let at = k as f64 * 0.19;
            let before = frame_at(&c, at - 0.01).loudness;
            let peak = (0..10).map(|j| frame_at(&c, at + j as f64 * 0.004).loudness).fold(0.0, f32::max);
            assert!(peak > before * 2.0, "note {k} should bounce: {before} -> {peak}");
        }
    }

    #[test]
    fn infers_staccato_and_accents() {
        let inst = cello();
        // Short notes with gaps: staccato (fast decay, sharp attack).
        let c = perform(&roll("0 0.1 C3 0.6\n0.5 0.1 D3 0.6\n1 0.1 E3 0.6\n1.5 1 F3 0.9\n2.5 1 G3 0.6"), &inst, &Style::default());
        assert!(frame_at(&c, 0.3).loudness < 0.05, "staccato should be gone by 0.3 s");
        // The loud F3 is accented: its noise spike is bigger than the plain G3's.
        let spike = |t: f64| frame_at(&c, t + 0.008).noise;
        assert!(spike(1.5) > spike(2.5) + 0.2, "{} vs {}", spike(1.5), spike(2.5));
    }

    #[test]
    fn pitch_arrives_and_short_notes_have_no_vibrato() {
        let inst = cello();
        let s = Score::parse("tempo 60\nE3:s F3 G3 A3 B3 C4 D4 E4 F3:h").unwrap();
        let c = perform(&s, &inst, &Style::default());
        let f = frame_at(&c, 0.875);
        assert!((f.pitch - 57.0).abs() < 0.08, "pitch {}", f.pitch);
        let late: Vec<f32> = (0..50).map(|k| frame_at(&c, 3.0 + k as f64 * 0.004).pitch).collect();
        let swing = late.iter().cloned().fold(f32::MIN, f32::max) - late.iter().cloned().fold(f32::MAX, f32::min);
        assert!(swing > 0.15, "vibrato swing {swing}");
    }

    #[test]
    fn streaming_matches_offline() {
        let inst = cello();
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../scores/lament.txt")).unwrap();
        let score = Score::parse(&text).unwrap();
        let style = Style::default();
        let offline = perform(&score, &inst, &style);
        // Live: each note arrives exactly LOOKAHEAD before it starts.
        let mut p = Performer::new(&inst, &style);
        let mut next = 0;
        let mut events = Vec::new();
        for (f, want) in offline.frames.iter().enumerate() {
            let t = f as f64 / FRAME_RATE;
            while next < score.notes.len() && score.notes[next].start - LOOKAHEAD <= t {
                p.add_note(score.notes[next].clone());
                next += 1;
            }
            assert_eq!(&p.next_frame(&mut events), want, "frame {f}");
        }
    }

    #[test]
    fn long_open_string_notes_move_to_a_lower_string() {
        let inst = cello();
        assert_eq!(choose_fingering(50.0, None, 1.0, false, &inst).string, 2);
        assert_eq!(choose_fingering(50.0, None, 1.0, true, &inst).string, 1);
    }

    #[test]
    fn arpeggios_cross_strings_instead_of_leaping() {
        // Bach prelude, bar 1: G2 D3 B3 A3 B3 D3 B3 D3, played on open G,
        // open D, then the A string (B3 under the hand, A3 open).
        let inst = cello();
        let (mut prev, mut hand) = (None, 1.0);
        let mut strings = Vec::new();
        for p in [43.0, 50.0, 59.0, 57.0, 59.0, 50.0, 59.0, 50.0] {
            let f = choose_fingering(p, prev, hand, false, &inst);
            assert_eq!(f.shift, 0.0, "no hand shifts in bar 1");
            strings.push(f.string);
            prev = Some(f.string);
            hand = f.hand;
        }
        assert_eq!(strings, [1, 2, 3, 3, 3, 2, 3, 2]);
    }

    #[test]
    fn deadpan_has_flat_dynamics() {
        let s = Score::parse("tempo 60\nmf C3:q D3 E3 F3 G3").unwrap();
        let style = Style { expression: 0.0, vibrato: 0.0, ..Style::default() };
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
        let patch = Style { pizzicato: true, ..Style::default() };
        let c = perform(&Score::parse("C3:q D3 E3").unwrap(), &cello(), &patch);
        assert_eq!(c.plucks.len(), 3);
    }
}
