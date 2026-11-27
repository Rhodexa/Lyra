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
//! * **Slurs**: overlapping notes are slurred. Runs of four or more fast
//!   touching notes are slurred where the line flows (steps, or arpeggios
//!   sweeping one way): very fast ones four to a bow, moderately fast ones
//!   in pairs. Zig-zag figures rocking across strings are bowed note by
//!   note (and, being short, bounce). Other touching notes get separate bows.
//! * **Staccato**: short notes followed by a clear gap.
//! * **Accents**: notes clearly louder than their neighbours, and (more
//!   gently) notes reached by a leap up of a fifth or more from a quick note.
//! * **Spiccato**: fast notes bowed separately bounce (a drop, a tick of
//!   hair, then the string rings free). Repeated notes are never slurred.
//!
//! How it plays:
//! * **Phrases** are split at rests. Loudness grows over the first couple of
//!   seconds of a phrase and relaxes as the visible end approaches.
//! * **High-loud**: higher notes are played a little louder.
//! * **Short-soft**: very short notes are a little softer.
//! * **The bow arm**: strokes alternate down and up (down-bows heavier, with
//!   more bite); no two are alike and the bow bites a few ms late, more so
//!   for a less skilled or busier arm. Short strokes dip less. A bow can't
//!   bounce faster than about 12 notes a second: beyond that, tiny strokes
//!   on the string.
//! * **Tremolo** (asked for, like pizzicato: a `~` mark or the tremolo
//!   patch): the bow shakes on one long note, 13–16 strokes a second and a
//!   little uneven, each reversal a small dip and scrape; faster and rougher
//!   under load.
//! * **Grit**: until the bow catches the string it slips irregularly, so
//!   every stroke starts with a little scratch: more for hard starts,
//!   strokes with little time to prepare, a weaker or busier arm. Now and
//!   then a start fails (a crunch and a stall before it catches).
//! * **Bow changes** dip the loudness and puff some bow noise; **slurred**
//!   notes keep the bow moving: a finger lands in ~8 ms, a string crossing
//!   is instant, and only big expressive shifts slide.
//! * **Vibrato** starts a moment after the note, grows in, follows loudness,
//!   and is absent on short notes and open strings.
//! * **Workload and composure**: how hard the music is right now (speed,
//!   hurried shifts, string skips, leaps, two strings, bounces), eased by
//!   dexterity; composure drains under load and recovers in rests. Mistakes
//!   come from these meeting the player's [`Skill`], never from plain dice.
//! * **Intonation**: the finger lands slightly off (hurried shifts tend to
//!   overshoot) and is pulled in by ear, quicker for a better player; a
//!   little stays. Errors drift slowly rather than re-rolling each note.
//!   Each string is tuned a touch off; leading tones lean up.
//! * **Vibrato** is a choice: some notes stay straight (more when busy or
//!   short), each note's depth, onset and speed vary; nerves make it
//!   quicker and narrower. Double stops vibrate less, the lower string less.
//! * **Finger taps**: a finger landing knocks the fingerboard, hardest when
//!   it hammers down mid-slur; lifting off makes a smaller one. After a rest
//!   the finger is placed silently. A bounced bow knocks as it lands.
//! * **Releases**: when the bow stops, the string rings on and darkens.
//!   Before a rest, once a stopped note has nearly died, the finger lifts:
//!   the note is choked and the open string rings on faintly, with a tiny
//!   pluck of the finger leaving.
//!   When the bow crosses to another string, the one it left rings on by
//!   itself (open strings for long).
//! * **Fingering**: a left hand covering a few semitones on all strings at
//!   once (about 4 on a cello, a fifth on a violin).
//!   Open strings and notes under the hand are free, crossing strings is
//!   cheap, shifting the hand is expensive. So arpeggios cross strings (the
//!   Bach prelude: open G, open D, A string) instead of leaping up one string.
//!   Long notes avoid open strings so they can have vibrato.

use crate::curves::{Curves, Frame, Pluck, Ring, Tap, FRAME_RATE};
use crate::dsp::smoothstep;
use crate::instrument::Instrument;
use crate::rand::{hash_bipolar, Wander};
use crate::score::{Articulation, Note, Score};
use crate::telemetry::{Intent, NoteInfo, Snapshot, StringSound, StringState, Tuner, RECENT, UPCOMING};
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
    /// Shake the bow on every note of a quarter second or more (the tremolo
    /// patch); shorter notes are played normally.
    pub tremolo: bool,
    /// How good the player is (separate from style and from who decides).
    pub skill: Skill,
    pub seed: u64,
}

impl Default for Style {
    fn default() -> Self {
        Self { expression: 1.0, vibrato: 1.0, breath: 1.0, pizzicato: false, tremolo: false, skill: Skill::default(), seed: 1 }
    }
}

/// The player's abilities, each 0..1. Mistakes come from these meeting the
/// music's workload, never from plain dice: a skilled player under little
/// load is nearly clean; anyone gets ragged when it gets frantic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skill {
    /// Landing fingers in tune, and how fast a miss is corrected.
    pub intonation: f32,
    /// How hard fast notes, shifts and string skips feel.
    pub dexterity: f32,
    /// Bow control: clean starts, steady contact.
    pub bowing: f32,
    /// Keeping calm under load, and recovering in rests.
    pub composure: f32,
}

impl Default for Skill {
    /// A good professional on a normal day.
    fn default() -> Self {
        Self { intonation: 0.9, dexterity: 0.85, bowing: 0.88, composure: 0.8 }
    }
}

impl Skill {
    pub fn profile(name: &str) -> Option<Skill> {
        let s = |intonation, dexterity, bowing, composure| Skill { intonation, dexterity, bowing, composure };
        Some(match name {
            "concert" => Skill::default(),
            "student" => s(0.6, 0.5, 0.65, 0.45),
            "virtuoso" => s(0.98, 0.98, 0.96, 0.92),
            "session" => s(0.85, 0.9, 0.92, 0.95),
            _ => return None,
        })
    }

    /// How large a player's mistakes in one skill are right now: a little
    /// from the skill itself, more under load, more still when rattled.
    /// Grows faster than the skill drops, and much faster under load: a
    /// student is fine when calm and slow, and falls apart when it's fast.
    fn error(skill: f32, workload: f32, composure: f32) -> f32 {
        let stress = workload * (1.5 - composure);
        let base = (1.0 - skill) + 1.2 * stress * (1.1 - skill);
        base + 1.5 * base * base
    }
}

/// How the performer decided to play a note (for plots and debugging).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Played {
    /// Bow starts from silence.
    Fresh,
    /// A new bow stroke right after the previous note.
    NewBow,
    /// Same bow as the previous note.
    Slurred,
    Staccato,
    /// Bounced bow (spiccato).
    Bounced,
    /// Bowed tremolo.
    Tremolo,
    Plucked,
}

/// One note's decision: how it's played and where on the instrument.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decision {
    pub played: Played,
    /// Played as a double stop.
    pub double: bool,
    /// String (0 = lowest).
    pub string: u8,
    /// Hand position (lowest semitone offset it covers) and how far it moved.
    pub hand: f32,
    pub shift: f32,
    /// How loaded the player was at this note, and how composed (0..1).
    pub workload: f32,
    pub composure: f32,
    /// Down-bow (true) or up-bow.
    pub down: bool,
}

/// Something the instrument must do at an exact time, besides the frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Pluck(Pluck),
    Tap(Tap),
    Ring(Ring),
}

/// "No id" from a live host: match the note by its key.
pub const NO_ID: u32 = u32::MAX;
/// A live note with no known end yet lasts this long until its note-off.
const OPEN: f64 = 3600.0;
/// A committed forecast note whose note-on is this late was deleted.
const MISSED: f64 = 0.02;

/// Who a note is to a live host.
#[derive(Clone, Copy, Debug)]
struct Tag {
    /// The host's ids for the note and, in a double stop, its lower note.
    id: u32,
    id2: u32,
    /// The lower key of a live double stop (to match its note-off).
    key2: f32,
    /// Its note-on came (offline notes count as heard).
    heard: bool,
}

impl Tag {
    const HEARD: Tag = Tag { id: NO_ID, id2: NO_ID, key2: -1.0, heard: true };
}

/// How many notes the left hand plans ahead, and how many hand states
/// it weighs per note.
const PLAN_NOTES: usize = 12;
const PLAN_STATES: usize = 48;

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
/// The finger lifts off a ringing stopped note once it has died to this
/// fraction of its level.
const LIFT_AT: f32 = 0.25;
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
    /// Where the finger first lands, relative to `pitch` (semitones), and
    /// how fast the ear pulls it in.
    settle: f32,
    settle_tau: f32,
    entry: Entry,
    attack: f32,
    accent: f32,
    staccato: bool,
    /// Bounced bow: a drop, a tick of hair, then the string rings free.
    spiccato: bool,
    /// Bowed tremolo: its speed (strokes a second, before the arm's drift)
    /// and how deep each reversal dips.
    tremolo: bool,
    trem_rate: f32,
    trem_depth: f32,
    level: f32,
    ramp: f32,
    swell: f32,
    fade: f32,
    vib_depth: f32,
    vib_delay: f32,
    /// This note's vibrato speed relative to the player's usual.
    vib_rate: f32,
    glide: f32,
    string: u8,
    crossing: bool,
    pizz: bool,
    phrase_start: f64,
    /// A double stop's lower note (with its intonation) and its string.
    double: Option<(f32, u8)>,
    hand: f32,
    shift: f32,
    workload: f32,
    composure: f32,
    /// Down-bow (true) or up-bow.
    down: bool,
    /// How late the bow bites after the written start, seconds.
    late: f32,
    /// The dip into the next bow change: how deep (the floor) and how long.
    /// Short strokes are tiny, so they dip less.
    stroke_floor: f32,
    stroke_time: f32,
    /// How scratchy the start is, and for how long; how deep a failed start
    /// stalls the loudness (0 = it caught cleanly).
    grit: f32,
    grit_time: f32,
    stall: f32,
}

/// A streaming performer. Feed it notes (in start order, ideally at least
/// [`LOOKAHEAD`] before they start) and pull one frame at a time.
pub struct Performer {
    inst: Instrument,
    style: Style,
    notes: Vec<Note>,
    /// Per note: who it is to a live host (see "Live hosts" below).
    tags: Vec<Tag>,
    plans: Vec<Plan>,
    frame: u64,
    owner: Option<usize>,
    vib_phase: f64,
    /// Tremolo strokes so far (one per whole number), and the arm's drift.
    trem_cycles: f64,
    trem_wander: Wander,
    rate_wander: Wander,
    smooth_loud: f32,
    loud_keep: f32,
    prev_string: Option<u8>,
    /// The last note whose finger has lifted off (see [`Performer::lift_at`]).
    lifted: Option<usize>,
    /// Where the left hand sits (see [`choose_fingering`]).
    hand: f32,
    /// How loaded the player is (smoothed over notes) and how composed.
    workload: f32,
    composure: f32,
    /// When the last note was planned (for the two above).
    last_start: f64,
    /// A slow drift in where the fingers land: errors that persist for a
    /// while instead of re-rolling every note.
    hand_bias: Wander,
    bias: f32,
    /// The bow arm: which way the next stroke goes, and its slow drift.
    bow_down: bool,
    arm: Wander,
    arm_now: f32,
    /// When the last bow stroke began (slurred notes share one).
    last_stroke: f64,
    /// Fingers still holding a ringing, stopped string after the bow left
    /// it: per string, the note's offset above the open string and until
    /// when it can still be ringing.
    held: [Option<(f32, f64)>; 4],
    /// Two-note chords become double stops (live hosts; see `set_doubles`).
    style_doubles: bool,
    /// The tuner: (time, cents) of recent frames of the current note.
    tuner_buf: [(f64, f32); TUNER_FRAMES],
    tuner_len: usize,
    tuner_owner: Option<usize>,
}

/// The tuner's memory: a little over 0.15 s of frames.
const TUNER_FRAMES: usize = 40;

impl Performer {
    pub fn new(inst: &Instrument, style: &Style) -> Self {
        Self {
            inst: inst.clone(),
            style: style.clone(),
            notes: Vec::with_capacity(1024),
            tags: Vec::with_capacity(1024),
            plans: Vec::with_capacity(1024),
            frame: 0,
            owner: None,
            vib_phase: 0.0,
            trem_cycles: 0.0,
            trem_wander: Wander::new(style.seed ^ 0x7E30, 3.0),
            rate_wander: Wander::new(style.seed ^ 0x5EED, 0.8),
            smooth_loud: 0.0,
            // Loudness can't jump: the bow has mass. ~8 ms smoothing.
            loud_keep: (-(1.0 / FRAME_RATE as f32) / 0.008).exp(),
            prev_string: None,
            hand: 1.0,
            lifted: None,
            workload: 0.0,
            composure: 1.0,
            last_start: 0.0,
            hand_bias: Wander::new(style.seed ^ 0xB1A5, 0.25),
            bias: 0.0,
            bow_down: true,
            arm: Wander::new(style.seed ^ 0xA2A2, 0.6),
            arm_now: 0.0,
            last_stroke: f64::MIN,
            held: [None; 4],
            style_doubles: true,
            tuner_buf: [(0.0, 0.0); TUNER_FRAMES],
            tuner_len: 0,
            tuner_owner: None,
        }
    }

    /// Adds a note. Notes already planned can't be moved, so a note arriving
    /// out of order is placed after them.
    pub fn add_note(&mut self, n: Note) {
        self.insert(n, Tag::HEARD);
    }

    fn insert(&mut self, n: Note, tag: Tag) {
        let pos = self.notes.partition_point(|m| m.start <= n.start).max(self.plans.len());
        self.notes.insert(pos, n);
        self.tags.insert(pos, tag);
    }

    // ── live hosts: a forecast, then the notes themselves ───────────────
    //
    // A host like Lyra says what's coming (a forecast, refreshed every
    // block) and then plays the notes (note-on, note-off). The forecast lets
    // the performer phrase; the notes are the truth:
    // * a forecast note whose note-on comes is played as planned;
    // * a note-on nobody forecast (played live, or added at the last
    //   moment) is sight-read: played at once, held until its note-off;
    // * a note-off ends its note then, even if it was meant to be longer;
    // * a forecast note that never comes is dropped, or, if the hands had
    //   already committed to it (`PLAN_LEAD`), cut as soon as it's missed.
    // Nothing here allocates while fewer than 1024 notes are in play.

    /// The host's forecast: every note it expects to start in the next
    /// [`LOOKAHEAD`], in start order, with its id; start times in the
    /// performer's clock. Replaces everything not yet planned. Notes starting
    /// together (10 ms) become a double stop when `doubles`, else the highest
    /// alone.
    pub fn forecast(&mut self, notes: impl IntoIterator<Item = (u32, Note)>, doubles: bool) {
        // Drop the old forecast, but keep notes actually played and not yet
        // planned (a key pressed a moment ago).
        let planned = self.plans.len();
        let mut keep = planned;
        for r in planned..self.notes.len() {
            if self.tags[r].heard {
                self.notes.swap(keep, r);
                self.tags.swap(keep, r);
                keep += 1;
            }
        }
        self.notes.truncate(keep);
        self.tags.truncate(keep);
        for (id, n) in notes {
            if self.find(id).is_some() {
                continue; // planned or played: it can't move any more
            }
            // A chord: the highest note leads, the next one down is its
            // double stop (like `Score::merge_chords`).
            let chord = (planned..self.notes.len())
                .rev()
                .find(|&j| !self.tags[j].heard && (self.notes[j].start - n.start).abs() < 0.01);
            if let Some(j) = chord {
                self.tags[j].id2 = id;
                let m = &mut self.notes[j];
                let (hi, lo) = if n.pitch > m.pitch { (n.pitch, m.pitch) } else { (m.pitch, n.pitch) };
                m.pitch = hi;
                m.double = (doubles && hi - lo > 0.01).then_some(lo.max(m.double.unwrap_or(f32::MIN)));
                m.dur = m.dur.max(n.dur);
            } else {
                self.insert(n, Tag { id, heard: false, ..Tag::HEARD });
            }
        }
    }

    /// A note-on, now. Forecast: it's confirmed. Otherwise it's played from
    /// now on, until its note-off. `id` may be [`NO_ID`] (match by key).
    pub fn note_on(&mut self, id: u32, pitch: f32, velocity: f32) {
        if let Some(i) = self.find(id) {
            self.tags[i].heard = true;
            return;
        }
        let dynamic = crate::score::velocity_to_dynamic(velocity);
        let now = self.time();
        // A second key a moment after an unplanned one: a double stop.
        if let Some(i) = self.notes.len().checked_sub(1).filter(|&i| i >= self.plans.len()) {
            let n = &mut self.notes[i];
            if (n.start - now).abs() < 0.01 && n.double.is_none() && (n.pitch - pitch).abs() > 0.01 {
                let (hi, lo) = if pitch > n.pitch { (pitch, n.pitch) } else { (n.pitch, pitch) };
                n.pitch = hi;
                n.double = self.style_doubles.then_some(lo);
                self.tags[i].id2 = id;
                self.tags[i].key2 = pitch;
                return;
            }
        }
        let n = Note {
            start: now,
            dur: OPEN,
            pitch,
            dynamic,
            dynamic_end: dynamic,
            articulation: crate::score::Articulation::Auto,
            slur: None,
            double: None,
        };
        self.insert(n, Tag { id, ..Tag::HEARD });
    }

    /// A note-off, now: its note ends here if it was meant to last longer.
    pub fn note_off(&mut self, id: u32, pitch: f32) {
        let now = self.time();
        let i = self.find(id).or_else(|| {
            // No id: the latest note on this key still sounding.
            (0..self.notes.len()).rev().find(|&i| {
                let (n, t) = (&self.notes[i], &self.tags[i]);
                t.id == NO_ID && ((n.pitch - pitch).abs() < 0.01 || (t.key2 - pitch).abs() < 0.01) && n.end() > now
            })
        });
        if let Some(i) = i {
            let n = &mut self.notes[i];
            if n.start <= now && n.end() > now + 0.005 {
                n.dur = now - n.start;
            }
        }
    }

    /// Stop: every note ends now and nothing more is expected.
    pub fn all_off(&mut self) {
        let now = self.time();
        let planned = self.plans.len();
        self.notes.truncate(planned);
        self.tags.truncate(planned);
        for n in &mut self.notes {
            if n.end() > now {
                n.dur = (now - n.start).max(0.0);
            }
        }
    }

    /// Once per block in a live host: cut committed notes whose note-on
    /// never came, and forget everything once all has died away (so the
    /// lists don't grow through a long session).
    pub fn settle(&mut self) {
        let now = self.time();
        let from = self.plans.len().saturating_sub(16);
        for i in from..self.plans.len() {
            let n = &mut self.notes[i];
            if !self.tags[i].heard && n.start < now - MISSED && n.end() > now {
                n.dur = now - n.start;
            }
        }
        if self.plans.len() == self.notes.len() && !self.notes.is_empty() && now > self.end() {
            self.notes.clear();
            self.tags.clear();
            self.plans.clear();
            self.owner = None;
            self.lifted = None;
            self.held = [None; 4];
        }
    }

    /// Whether two-note chords become double stops (live hosts).
    pub fn set_doubles(&mut self, on: bool) {
        self.style_doubles = on;
    }

    /// The knobs, changed while playing (the instrument stays).
    pub fn set_style(&mut self, style: &Style) {
        self.style = style.clone();
    }

    /// Notes not yet started or still sounding, among the last ones.
    fn find(&self, id: u32) -> Option<usize> {
        if id == NO_ID {
            return None;
        }
        // Only recent notes can still matter: search back from the end.
        (0..self.tags.len()).rev().take(256).find(|&i| self.tags[i].id == id || self.tags[i].id2 == id)
    }

    /// How each note planned so far is being played, in note order.
    pub fn decisions(&self) -> Vec<Decision> {
        (0..self.plans.len()).map(|i| self.decision(i)).collect()
    }

    /// How planned note `i` is being played.
    fn decision(&self, i: usize) -> Decision {
        let p = &self.plans[i];
        let how = if p.pizz {
            Played::Plucked
        } else if p.tremolo {
            Played::Tremolo
        } else if p.spiccato {
            Played::Bounced
        } else if p.staccato {
            Played::Staccato
        } else {
            match p.entry {
                Entry::Fresh => Played::Fresh,
                Entry::BowChange => Played::NewBow,
                Entry::Legato => Played::Slurred,
            }
        };
        Decision {
            played: how,
            double: p.double.is_some(),
            string: p.string,
            hand: p.hand,
            shift: p.shift,
            workload: p.workload,
            composure: p.composure,
            down: p.down,
        }
    }

    // ── the left hand plans ahead ───────────────────────────────────────

    /// The cheapest way to finger the visible passage from note `i` on (up
    /// to [`PLAN_NOTES`] notes, stopping before a double stop), from where
    /// the hand is now: each note's [`reach_cost`], a hurried shift dearer.
    /// Fills `path` and returns note `i`'s fingering (the greedy choice if
    /// nothing can be planned). Pure and allocation-free: the performer
    /// calls it to commit a note, a faceplate to show the intention.
    fn finger_plan(&self, i: usize, t: f64, path: &mut [Fingering; PLAN_NOTES]) -> Fingering {
        let inst = &self.inst;
        let n = self.notes[i].clone();
        let greedy = || choose_fingering(n.pitch, self.prev_string, self.hand, n.dur > 0.5 && !self.is_pizz(&n), inst);
        let len = self.plan_len(i, t);
        if len == 0 {
            path[0] = greedy();
            return path[0];
        }
        #[derive(Clone, Copy, Default)]
        struct Node {
            cost: f32,
            f: Fingering,
            back: u8,
        }
        let mut layers = [[Node::default(); PLAN_STATES]; PLAN_NOTES];
        let mut counts = [0usize; PLAN_NOTES];
        let start = Node { cost: 0.0, f: Fingering { string: self.prev_string.unwrap_or(u8::MAX), hand: self.hand, shift: 0.0 }, back: 0 };
        let mut len = len;
        for k in 0..len {
            let note = &self.notes[i + k];
            let avoid_open = note.dur > 0.5 && !self.is_pizz(note);
            let ioi = (i + k).checked_sub(1).map_or(1.0, |j| note.start - self.notes[j].start) as f32;
            let hurry = 1.0 - smoothstep(ioi / 0.5);
            let sources = if k == 0 { 1 } else { counts[k - 1] };
            for src in 0..sources {
                let from = if k == 0 { start } else { layers[k - 1][src] };
                let prev = (from.f.string != u8::MAX).then_some(from.f.string);
                for s in 0..inst.open_strings.len() {
                    let Some((c, f)) = reach_cost(note.pitch, prev, from.f.hand, avoid_open, hurry, s, inst) else { continue };
                    let node = Node { cost: from.cost + c, f, back: src as u8 };
                    // Same string and hand = same state: keep the cheaper.
                    let layer = &mut layers[k][..counts[k]];
                    if let Some(same) = layer.iter_mut().find(|m| m.f.string == f.string && (m.f.hand - f.hand).abs() < 0.01) {
                        if node.cost < same.cost {
                            *same = node;
                        }
                    } else if counts[k] < PLAN_STATES {
                        layers[k][counts[k]] = node;
                        counts[k] += 1;
                    } else if let Some(worst) = layers[k].iter_mut().max_by(|a, b| a.cost.total_cmp(&b.cost)) {
                        if node.cost < worst.cost {
                            *worst = node;
                        }
                    }
                }
            }
            if counts[k] == 0 {
                len = k; // unplayable: plan up to here
                break;
            }
        }
        if len == 0 {
            path[0] = greedy();
            return path[0];
        }
        // Walk back from the cheapest end.
        let mut at = (0..counts[len - 1]).min_by(|&a, &b| layers[len - 1][a].cost.total_cmp(&layers[len - 1][b].cost)).unwrap_or(0);
        for k in (0..len).rev() {
            path[k] = layers[k][at].f;
            at = layers[k][at].back as usize;
        }
        path[0]
    }

    /// How many notes from `i` the hand can plan: visible ones, up to
    /// [`PLAN_NOTES`], stopping before a double stop (chosen on its own).
    fn plan_len(&self, i: usize, t: f64) -> usize {
        let visible = self.visible(t).max(i + 1);
        (i..visible.min(i + PLAN_NOTES)).take_while(|&j| j == i || self.notes[j].double.is_none()).count()
            * usize::from(self.notes[i].double.is_none())
    }

    // ── what a faceplate sees ───────────────────────────────────────────

    /// Fills everything but the time and frame of a [`Snapshot`], at time
    /// `t` with the sound at `frame`, given the last ring per string.
    pub(crate) fn fill_snapshot(&self, t: f64, frame: &Frame, rings: &[Option<Ring>; 4], out: &mut Snapshot) {
        let open = self.inst.open_strings;
        let sounding = frame.loudness > 0.02;
        let mut st = [StringState::default(); 4];
        // Strings ringing on their own (crossed away from, or let go)…
        for (s, r) in rings.iter().enumerate() {
            let Some(r) = r.filter(|r| r.time <= t) else {
                continue;
            };
            let tau = if r.open { 1.5 } else { (0.12 + 0.5 * r.bright as f64) / 2.0 };
            let level = r.level * (-(t - r.time) / tau).exp() as f32;
            if level > 0.02 {
                st[s].sound = StringSound::Ringing(level.min(1.0));
                if !r.open && t < r.time + 0.12 + 0.5 * r.bright as f64 {
                    st[s].finger = Some(r.pitch);
                }
            }
        }
        // …fingers still holding them…
        for (s, h) in self.held.iter().enumerate() {
            if let Some((off, until)) = *h {
                if t < until && t >= self.last_start {
                    st[s].finger = Some(open[s] + off);
                }
            }
        }
        // …and the note being played, on top.
        if let Some(i) = self.owner {
            let (n, p) = (&self.notes[i], &self.plans[i]);
            let s = p.string as usize;
            let playing = t < self.sound_end(i, t);
            let fingered = playing || self.lift_at(i, t).is_some_and(|l| t < l);
            if p.pizz {
                if t < n.start + 0.25 {
                    st[s].sound = StringSound::Plucked;
                }
            } else if sounding && playing {
                st[s].sound = StringSound::Bowed;
                if let Some((_, s2)) = p.double {
                    st[s2 as usize].sound = StringSound::Bowed;
                }
            } else if fingered && sounding {
                st[s].sound = StringSound::Ringing(frame.loudness.min(1.0));
            }
            if fingered {
                // Where the finger really is: the sound's pitch while it's on
                // this string, else where the note is written.
                let real = |pitch: f32, string: u8, written: f32| {
                    if !p.pizz && pitch > 0.0 && string == p.string {
                        pitch
                    } else {
                        written
                    }
                };
                let fp = real(frame.pitch, frame.string, n.pitch);
                st[s].finger = (fp - open[s] >= 0.5).then_some(fp);
                if let Some((lo, s2)) = p.double {
                    let fp2 = if frame.pitch2 > 0.0 { frame.pitch2 } else { lo };
                    st[s2 as usize].finger = (fp2 - open[s2 as usize] >= 0.5).then_some(fp2);
                }
            }
            if p.entry == Entry::Legato
                && p.shift > 0.0
                && i > 0
                && self.plans[i - 1].string == p.string
                && t - n.start < 0.5
            {
                st[s].sliding_from = Some(self.notes[i - 1].pitch);
            }
        }
        out.strings = st;
        out.hand = self.hand;
        out.workload = self.workload;
        out.composure = self.composure;
        out.tuner = if sounding && self.owner.is_some_and(|i| !self.plans[i].pizz) { self.tuner(t) } else { None };
        // What the hand means to do next: the plan from the first undecided
        // note, as the performer would make it now.
        let mut path = [Fingering::default(); PLAN_NOTES];
        let first = self.plans.len();
        let planned = if first < self.notes.len() {
            self.finger_plan(first, t, &mut path);
            self.plan_len(first, t).max(1)
        } else {
            0
        };
        let info = |i: usize| {
            let n = &self.notes[i];
            let decision = (i < self.plans.len()).then(|| self.decision(i));
            let intent = (i >= first && i - first < planned).then(|| {
                let f = path[i - first];
                Intent { string: f.string, hand: f.hand, shift: f.shift }
            });
            NoteInfo {
                start: n.start,
                len: n.dur.min(OPEN / 2.0),
                pitch: n.pitch,
                pitch2: n.double,
                decision,
                intent,
                open: decision.map(|d| d.string).or(intent.map(|f| f.string))
                    .is_some_and(|s| (n.pitch - open[s as usize]).abs() < 0.01),
            }
        };
        let now = self.owner.map_or(0, |o| o + 1);
        let from = now.saturating_sub(RECENT);
        out.recent = [None; RECENT];
        for (k, i) in (from..now).enumerate() {
            out.recent[k] = Some(info(i));
        }
        out.upcoming = [None; UPCOMING];
        for (k, i) in (now..self.notes.len()).take(UPCOMING).enumerate() {
            out.upcoming[k] = Some(info(i));
        }
    }

    /// The last 0.15 s of the note against its written pitch (frames past
    /// the onset glide, sounding, within a semitone).
    fn track_tuner(&mut self, frame: &Frame, t: f64) {
        if self.owner != self.tuner_owner {
            self.tuner_owner = self.owner;
            self.tuner_len = 0;
        }
        let Some(i) = self.owner else { return };
        let c = (frame.pitch - self.notes[i].pitch) * 100.0;
        if frame.loudness < 0.02 || t < self.notes[i].start + 0.08 || c.abs() > 100.0 {
            return;
        }
        self.tuner_buf[self.tuner_len % TUNER_FRAMES] = (t, c);
        self.tuner_len += 1;
    }

    fn tuner(&self, t: f64) -> Option<Tuner> {
        let seen = &self.tuner_buf[..self.tuner_len.min(TUNER_FRAMES)];
        let (mut sum, mut k, mut lo, mut hi) = (0.0, 0, f32::MAX, f32::MIN);
        for &(_, c) in seen.iter().filter(|(at, _)| *at >= t - 0.15) {
            sum += c;
            k += 1;
            lo = lo.min(c);
            hi = hi.max(c);
        }
        (k > 0).then(|| Tuner { cents: sum / k as f32, lo, hi })
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
        let note_rate = self.owner.map_or(1.0, |i| self.plans[i].vib_rate);
        let rate = (self.inst.vibrato_rate + 0.35 * self.rate_wander.next(dt as f32)) * note_rate;
        self.vib_phase = (self.vib_phase + TAU * rate as f64 * dt) % TAU;
        self.bias = self.hand_bias.next(dt as f32);
        self.arm_now = self.arm.next(dt as f32);
        let trem = self.owner.and_then(|i| self.plans[i].tremolo.then_some(self.plans[i].trem_rate)).unwrap_or(14.0);
        self.trem_cycles += (trem * (1.0 + 0.08 * self.trem_wander.next(dt as f32))) as f64 * dt;

        let mut frame = match self.owner {
            None => Frame {
                pitch: self.notes.first().map_or(48.0, |n| n.pitch),
                brightness: 0.4,
                ..Frame::default()
            },
            Some(i) => {
                let f = self.note_frame(i, t);
                // The finger lifts: the open string takes over what's left.
                if let Some(lift) = self.lift_at(i, t) {
                    if t >= lift && self.lifted != Some(i) {
                        self.lifted = Some(i);
                        let p = &self.plans[i];
                        // Never twice the same: sometimes the finger rolls off
                        // cleanly and nothing rings; otherwise a ghost of
                        // varying strength and colour, and a small tap.
                        let h = |salt: u64| 0.5 + 0.5 * hash_bipolar(self.style.seed ^ salt, i);
                        if h(0x11F7) > 0.3 {
                            events.push(Event::Ring(Ring {
                                time: t,
                                pitch: self.inst.open_strings[p.string as usize] + string_detune(self.style.seed, p.string),
                                level: p.level * LIFT_AT * (0.25 + 0.5 * h(0x6057)),
                                string: p.string,
                                open: true,
                                bright: 0.15 + 0.25 * h(0xB216),
                            }));
                        }
                        events.push(Event::Tap(Tap { time: t, strength: 0.05 + 0.12 * h(0x7A9F) }));
                    }
                }
                f
            }
        };
        self.smooth_loud = frame.loudness + self.loud_keep * (self.smooth_loud - frame.loudness);
        frame.loudness = self.smooth_loud;
        self.track_tuner(&frame, t);
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
            // Only a real run (four or more fast notes) is slurred, and only
            // very fast notes four to a bow; moderately fast ones in pairs. A
            // quick pair inside a mixed rhythm gets separate bows.
            let back = self.run_index(i);
            let mut fwd = 0;
            while fwd < 3 && i + 2 + fwd < self.notes.len() && self.fast_link(i + 1 + fwd) {
                fwd += 1;
            }
            if back + 1 + fwd < 3 || !self.flowing(i) {
                return false;
            }
            let group = if n.dur < 0.15 { 4 } else { 2 };
            return back % group != group - 1;
        }
        false
    }

    /// Does the line flow from note `i` to `i + 1`: a small step, or a leap
    /// carrying on in the same direction (an arpeggio sweeping)? Zig-zag
    /// leaps, a figure rocking across strings, are bowed note by note.
    fn flowing(&self, i: usize) -> bool {
        let d = self.notes[i + 1].pitch - self.notes[i].pitch;
        if d.abs() <= 2.0 {
            return true; // a step
        }
        let before = (i > 0 && self.fast_link(i - 1)).then(|| self.notes[i].pitch - self.notes[i - 1].pitch);
        let after = (i + 2 < self.notes.len() && self.fast_link(i + 1)).then(|| self.notes[i + 2].pitch - self.notes[i + 1].pitch);
        let reverses = |x: Option<f32>| x.is_some_and(|x| x.abs() >= 3.0 && x.signum() != d.signum());
        d.abs() <= 9.0 && !reverses(before) && !reverses(after)
    }

    /// Notes `j` and `j + 1` belong to the same fast run.
    fn fast_link(&self, j: usize) -> bool {
        let (a, b) = (&self.notes[j], &self.notes[j + 1]);
        touching(a, b) && a.dur < FAST && b.dur < FAST && a.slur.is_none() && (a.pitch - b.pitch).abs() > 0.01
    }

    /// Position of note `i` in its run of fast, touching notes.
    fn run_index(&self, i: usize) -> usize {
        let mut j = i;
        while j > 0 {
            if self.fast_link(j - 1) {
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
            // After a bounce the bow is already moving: a normal new stroke.
            Some((pn, pp)) if n.start - pn.end() < PHRASE_GAP && pp.spiccato && !pizz => Entry::BowChange,
            Some((pn, pp)) if touching(pn, n) && !pp.pizz && !pp.staccato && !pp.spiccato => {
                if self.slurred(i - 1) {
                    Entry::Legato
                } else {
                    Entry::BowChange
                }
            }
            _ => Entry::Fresh,
        };
        // After a rest the finger is placed quietly, long before the bow.
        let from_silence = prev.is_none_or(|(pn, _)| n.start - pn.end() > PHRASE_GAP);
        let staccato = !pizz
            && match n.articulation {
                Articulation::Staccato => true,
                Articulation::Auto => {
                    n.dur < 0.3 && next.is_none_or(|q| q.start - n.end() >= (0.35 * n.dur).max(0.05))
                }
                _ => false,
            };
        // Fast notes bowed separately (no slur on either side) bounce.
        // A bow can only bounce so fast (about 12 strokes a second; quicker
        // hands a bit more). Beyond that it stays on the string in tiny
        // strokes. Strokes, not notes: a slurred pickup is one stroke.
        let quickest = [next.map(|q| q.start - n.start), Some(n.start - self.last_stroke)]
            .into_iter()
            .flatten()
            .fold(f64::MAX, f64::min) as f32;
        let can_bounce = 1.0 / quickest.max(1e-3) <= 9.0 + 4.0 * self.style.skill.dexterity;
        let fast = next.is_some_and(|q| q.start - n.start < FAST + 0.02)
            || prev.is_some_and(|(pn, _)| n.start - pn.start < FAST + 0.02);
        let slur_out = next.is_some() && i + 1 < self.notes.len() && self.slur_rule(i, staccato);
        let tremolo = !pizz
            && (n.articulation == Articulation::Tremolo
                || (self.style.tremolo
                    && n.dur >= 0.25
                    && !matches!(n.articulation, Articulation::Staccato | Articulation::Spiccato)));
        let staccato = staccato && !tremolo;
        let spiccato = !pizz
            && !tremolo
            && match n.articulation {
                Articulation::Spiccato => true,
                // Only short notes bounce: a long note after a fast run is bowed.
                Articulation::Auto => {
                    n.dur < FAST + 0.02 && fast && can_bounce && !staccato && entry != Entry::Legato && !slur_out
                }
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

        // Leaping up a fifth or more out of a quick pickup note: lean into it.
        let leap = entry != Entry::Legato
            && prev.is_some_and(|(pn, _)| touching(pn, n) && pn.dur < 0.3 && n.pitch - pn.pitch >= 7.0);
        let accent = match n.articulation {
            Articulation::Accent => 1.0,
            Articulation::Auto if n.dynamic > mean_dyn + 0.12 => 1.0,
            Articulation::Auto if leap => 0.7,
            Articulation::Tenuto => 0.0,
            _ if entry == Entry::Fresh => 0.2,
            _ => 0.0,
        };

        // A double stop if a hand can play it and the note is long enough to
        // bother (plucked chords are always fine); otherwise the top note.
        let pair = n
            .double
            .filter(|_| n.dur >= 0.25 || pizz)
            .and_then(|d| choose_double(n.pitch, d, self.prev_string, self.hand, inst).map(|f| (f, d)));
        let (fingering, double) = match pair {
            Some(((f, lower), d)) => (f, Some((d + 0.02 * e * hash_bipolar(self.style.seed ^ 0xD0B1, i), lower))),
            // The hand plans the whole visible passage, and takes its first step.
            None => (self.finger_plan(i, t, &mut [Fingering::default(); PLAN_NOTES]), None),
        };
        let string = fingering.string;
        let crossing = self.prev_string.is_some_and(|s| s != string);
        let open = (n.pitch - inst.open_strings[string as usize]).abs() < 0.01;

        // How loaded is the player? Effort from this note (speed, a hurried
        // shift, skipped strings, a big leap, two strings, a bounce), eased
        // by dexterity and smoothed over notes; composure drains under load
        // and comes back in rests.
        let sk = self.style.skill;
        let seed = self.style.seed;
        let h01 = |salt: u64| 0.5 + 0.5 * hash_bipolar(seed ^ salt, i);
        let ioi = prev.map_or(1.0, |(pn, _)| (n.start - pn.start) as f32).max(0.01);
        let speed = smoothstep((1.0 / ioi - 3.0) / 9.0); // 3 … 12 notes a second
        let hurried_shift = (fingering.shift / 7.0).min(1.0) * (1.0 - smoothstep(ioi / 0.5));
        let skip = self.prev_string.map_or(0.0, |s| ((s as f32 - string as f32).abs() - 1.0).max(0.0) * 0.25);
        let leap = prev.map_or(0.0, |(pn, _)| (((n.pitch - pn.pitch).abs() - 7.0) / 12.0).clamp(0.0, 1.0) * 0.3);
        let extra = if double.is_some() { 0.2 } else { 0.0 } + if spiccato { 0.1 } else { 0.0 };
        let effort =
            ((0.55 * speed + 0.6 * hurried_shift + skip + leap + extra) * (1.3 - 0.6 * sk.dexterity)).clamp(0.0, 1.0);
        let dt = (n.start - self.last_start).max(0.0) as f32;
        let wtau = if effort > self.workload { 0.5 } else { 1.5 };
        let workload = self.workload + (effort - self.workload) * (1.0 - (-dt / wtau).exp());
        let calm = 1.0 - workload * (1.2 - sk.composure);
        let ctau = if calm < self.composure { 2.0 } else { 4.0 - 2.0 * sk.composure };
        let composure = (self.composure + (calm - self.composure) * (1.0 - (-dt / ctau).exp())).clamp(0.0, 1.0);

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

        // The bow arm: strokes alternate down and up (a down-bow is heavier,
        // and bites a little more), no two quite alike, and the bow bites a
        // few milliseconds late. A less skilled or busier arm varies more;
        // the variation drifts slowly rather than jittering.
        let new_stroke = entry != Entry::Legato && !pizz;
        let down = if !new_stroke {
            self.bow_down
        } else if from_silence {
            true
        } else {
            !self.bow_down
        };
        let berr = Skill::error(self.style.skill.bowing, workload, composure);
        let (level, late) = if new_stroke {
            let wobble = (0.03 + 0.12 * berr) * (0.6 * self.arm_now + 0.4 * hash_bipolar(seed ^ 0xA2E, i));
            let dir = if down { 0.04 } else { -0.04 };
            let late = e.min(1.0) * (0.002 + 0.014 * berr) * h01(0x1A7E);
            ((level * (1.0 + e * (dir + wobble))).clamp(0.05, 1.0), late)
        } else {
            (level, 0.0)
        };
        // Grit: until the bow catches the string it slips irregularly. Hard
        // starts (accents, staccato) scrape more; so does a stroke with
        // little time to prepare, a less skilled arm, or a busy one. Now and
        // then a start fails: a crunch and a stall before it catches. A
        // slurred crossing catches the new string with a little scrape.
        let prep = if from_silence { 1.0 } else { smoothstep((n.start - self.last_stroke) as f32 / 0.4) };
        let (grit, grit_time, stall) = if new_stroke {
            let kind = if spiccato {
                0.15
            } else if accent >= 1.0 {
                0.35
            } else if staccato {
                0.25
            } else if entry == Entry::BowChange {
                0.12
            } else {
                0.18
            };
            let mut g = kind * (1.3 - 0.6 * prep) + 0.5 * berr * h01(0x6217);
            let failed = h01(0xFA11) < 0.01 + 0.25 * berr * (1.0 - 0.5 * prep);
            if failed {
                g = g.max(0.6);
            }
            let g = (e.min(1.0) * g).min(1.0);
            let failed = failed && e > 0.0;
            (g, 0.025 + 0.05 * g + if failed { 0.05 } else { 0.0 }, if failed { 0.45 } else { 0.0 })
        } else if crossing && !pizz {
            (0.08 * e.min(1.0), 0.02, 0.0)
        } else {
            (0.0, 0.02, 0.0)
        };
        let step_next = next.map_or(1.0, |q| (q.start - n.start) as f32);
        let stroke_floor = BOW_CHANGE_FLOOR + 0.4 * (1.0 - smoothstep((step_next - 0.06) / 0.14));
        let stroke_time = BOW_CHANGE_TIME.min(0.3 * step_next);

        // Intonation (cents). Where the finger first lands is corrected by
        // ear; a little stays. Both grow with load and nerves, and both carry
        // the hand's slow bias, so a rattled player stays off for a while. A
        // hurried shift tends to overshoot. Open strings are as tuned.
        let err = Skill::error(sk.intonation, workload, composure);
        // What's left once the ear has done its work: everyone hears well,
        // so it stays small (a little more for a weaker player).
        let ear = 1.0 - sk.intonation;
        let mut cents = e * ear * (5.0 * self.bias + 3.0 * hash_bipolar(seed, i));
        if let Some(q) = next {
            if (q.pitch - n.pitch - 1.0).abs() < 0.01 && n.dur > 0.15 {
                cents += 7.0 * e; // leading tone leans up
            }
        }
        if open {
            cents = 100.0 * string_detune(seed, string);
        }
        let settle = if open || pizz {
            0.0
        } else {
            let mut land = e * err * (12.0 * self.bias + 16.0 * hash_bipolar(seed ^ 0xA77AC, i));
            if let (true, Some((pn, _))) = (fingering.shift > 0.0, prev) {
                let dir = (n.pitch - pn.pitch).signum();
                land += dir * e * err * (8.0 + 4.0 * fingering.shift) * (0.5 + 0.5 * h01(0x0E5));
            }
            if entry == Entry::Legato {
                land *= 0.6;
            }
            land / 100.0
        };
        // How fast the miss is pulled in: quick for a good player, slower for
        // a student, slower still when rattled. Long notes always get there;
        // fast ones end before the fix.
        let settle_tau = 0.03 + 0.12 * ear + 0.05 * workload * (1.5 - composure);

        // Vibrato is a choice: some notes stay straight (more when busy,
        // short, or on two strings), and each note's depth, onset and speed
        // differ a little. Nerves make it quicker and narrower.
        let straight_chance =
            0.12 + 0.5 * workload + if n.dur < 0.4 { 0.25 } else { 0.0 } + if double.is_some() { 0.2 } else { 0.0 };
        let straight = h01(0x5741) < straight_chance;
        let vib_depth = if pizz || open || spiccato || n.dur < 0.22 || straight {
            0.0
        } else {
            (0.13 + 0.14 * level)
                * self.style.vibrato
                * (0.75 + 0.5 * h01(0x7D))
                * (1.0 - 0.3 * workload)
                * if double.is_some() { 0.6 } else { 1.0 }
        };
        let prev_vibrated = prev.is_some_and(|(_, pp)| pp.vib_depth > 0.0);
        let vib_delay =
            if entry == Entry::Legato && prev_vibrated { 0.04 } else { 0.08 + 0.17 * h01(0xDE1A) + 0.08 * composure };
        let vib_rate = (0.94 + 0.12 * h01(0x7A7E)) * (1.0 + 0.1 * workload * (1.5 - composure));

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
        } else if prev.is_some_and(|(pn, _)| n.start - pn.end() <= PHRASE_GAP) {
            0.03 // the bow only lifted for a moment: a crisp new start
        } else {
            0.05 + 0.08 * (1.0 - n.dynamic) // from real silence; soft starts are slower
        };

        let hairpin = n.dynamic_end != n.dynamic;
        let plan = Plan {
            pitch: n.pitch + cents / 100.0,
            settle,
            settle_tau,
            entry,
            attack,
            accent: accent + if down && new_stroke { 0.1 * e.min(1.0) } else { 0.0 },
            staccato,
            spiccato,
            tremolo,
            // Fast and a little uneven; faster and rougher under load, a
            // clumsier arm a bit slower with deeper reversals.
            trem_rate: (13.0 + 3.0 * h01(0x7E31)) * (1.0 + 0.15 * workload) * (0.85 + 0.15 * self.style.skill.dexterity),
            trem_depth: 0.3 + 0.2 * berr.min(1.0),
            level,
            ramp: n.dynamic_end - n.dynamic,
            swell: if n.dur > 0.9 && !pizz && !hairpin { 0.07 * e } else { 0.0 },
            fade: if pizz || n.dynamic_end > n.dynamic { 0.0 } else { 0.25 * e },
            vib_depth,
            vib_delay,
            vib_rate,
            glide,
            string,
            crossing,
            pizz,
            phrase_start,
            double,
            hand: fingering.hand,
            shift: fingering.shift,
            workload,
            composure,
            down,
            late,
            stroke_floor,
            stroke_time,
            grit,
            grit_time,
            stall,
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
            if let Some((d, lower)) = double {
                events.push(Event::Pluck(Pluck {
                    time: n.start + 0.012, // a finger rolls across the strings
                    pitch: d,
                    velocity: level * 0.9,
                    brightness: (0.35 + 0.5 * level + inst.string_tone[lower as usize]).clamp(0.0, 1.0),
                    string: lower,
                }));
            }
        } else if spiccato {
            // The bow lands: hair and stick knock the string.
            events.push(Event::Tap(Tap { time: n.start.max(t), strength: 0.45 }));
        } else if let Some(strength) = tap_strength(entry, prev.map(|(pn, pp)| (pn, pp.string)), n, string, open)
            .filter(|_| !from_silence)
        {
            let lead = if entry == Entry::Legato { glide as f64 } else { 0.015 };
            // Busier hands hit harder; no two taps alike.
            let strength = (strength * (0.6 + 0.8 * workload) * (0.8 + 0.4 * h01(0x7A9))).min(1.0);
            events.push(Event::Tap(Tap { time: (n.start - lead).max(t), strength }));
        }

        // Crossing away from a string that was sounding: it rings on. If a
        // finger was holding that note, it can only keep holding it while
        // the hand stays in reach; a finger that has to go lifts, and the
        // string rings open (faintly, or not at all).
        let reach = |off: f32, hand: f32| off >= hand - 0.01 && off <= hand + inst.hand_span + 0.01;
        let at = (n.start - glide as f64).max(t);
        let lift = |s: u8, salt: u64, events: &mut Vec<Event>| {
            if h01(salt) > 0.3 {
                events.push(Event::Ring(Ring {
                    time: at,
                    pitch: inst.open_strings[s as usize] + string_detune(seed, s),
                    level: 0.2 * level * (0.4 + 0.6 * h01(salt ^ 0x5)),
                    string: s,
                    open: true,
                    bright: 0.15 + 0.25 * h01(salt ^ 0x9),
                }));
            }
            events.push(Event::Tap(Tap { time: at, strength: 0.04 + 0.1 * h01(salt ^ 0xD) }));
        };
        let mut held = self.held;
        held[string as usize] = None; // the bow is on it now
        if let (true, false, Some((pn, pp))) = (crossing, pizz, prev) {
            if entry != Entry::Fresh {
                let s = pp.string as usize;
                let off = pn.pitch - inst.open_strings[s];
                let open_prev = off.abs() < 0.01;
                if open_prev || reach(off, fingering.hand) {
                    // How it rings depends on how the bow left it: never the same.
                    events.push(Event::Ring(Ring {
                        time: at,
                        pitch: pp.pitch,
                        level: pp.level * (0.6 + 0.4 * h01(0x217A)),
                        string: pp.string,
                        open: open_prev,
                        bright: 0.4 + 0.6 * h01(0xB21C),
                    }));
                    held[s] = (!open_prev).then_some((off, at + 0.8));
                } else {
                    lift(pp.string, 0x11F8, events);
                    held[s] = None;
                }
            }
        }
        // Earlier fingers the hand has now moved away from let go.
        for s in 0..4 {
            if let Some((off, until)) = held[s] {
                if at > until {
                    held[s] = None;
                } else if !reach(off, fingering.hand) {
                    lift(s as u8, 0x11F9 ^ s as u64, events);
                    held[s] = None;
                }
            }
        }

        self.held = held;
        self.prev_string = Some(string);
        self.hand = fingering.hand;
        self.workload = workload;
        self.composure = composure;
        self.bow_down = down;
        if new_stroke {
            self.last_stroke = n.start;
        }
        self.last_start = n.start;
        self.plans.push(plan);
    }

    /// When the finger lets go of a stopped note ringing into a rest: once
    /// the ring has died to about a quarter, if the next note isn't coming
    /// soon. `None` for open strings, plucks and notes that run on.
    fn lift_at(&self, i: usize, t: f64) -> Option<f64> {
        let (n, p) = (&self.notes[i], &self.plans[i]);
        let open = (n.pitch - self.inst.open_strings[p.string as usize]).abs() < 0.01;
        if p.pizz || open || p.double.is_some() {
            return None;
        }
        let Exit::Decay(tau) = self.exit(i, t) else { return None };
        let lift = self.sound_end(i, t) + (tau * (1.0 / LIFT_AT).ln()) as f64;
        match self.next_visible(i, t) {
            Some(q) if q.start < lift + 0.05 => None,
            _ => Some(lift),
        }
    }

    // ── one frame of a sounding note ────────────────────────────────────

    fn note_frame(&self, i: usize, t: f64) -> Frame {
        let n = &self.notes[i];
        let p = &self.plans[i];
        let since = (t - n.start) as f32;
        // The bow bites a touch late; the finger is already there.
        let since_a = (since - p.late).max(0.0);
        let floor_in = i.checked_sub(1).map_or(BOW_CHANGE_FLOOR, |j| self.plans[j].stroke_floor);
        let sound_end = self.sound_end(i, t);
        let exit = self.exit(i, t);

        // Bow envelope: how the attack rises…
        let rise = match p.entry {
            Entry::Legato => 1.0 - 0.06 * (-(since / 0.03).powi(2)).exp(),
            Entry::BowChange => floor_in + (1.0 - floor_in) * smoothstep(since_a / p.attack),
            Entry::Fresh => smoothstep(since_a / p.attack),
        };
        let bite = 1.0 + 0.35 * p.accent * (-since_a / 0.08).exp() * smoothstep(since_a / p.attack);
        // …and how the note leaves.
        let fall = match exit {
            Exit::Legato => 1.0,
            Exit::BowChange => {
                let to_end = (sound_end - t) as f32;
                1.0 - (1.0 - p.stroke_floor) * smoothstep(1.0 - to_end / p.stroke_time)
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
        let bounce = 0.18 + 0.82 * smoothstep(since_a / 0.01) * (-(since_a - 0.01).max(0.0) / 0.065).exp();
        // Once the finger lifts, the stopped note is choked (the open string
        // carries on as a separate ring).
        let lifted = self.lift_at(i, t).map_or(1.0, |lift| if t > lift { (-((t - lift) as f32) / 0.008).exp() } else { 1.0 });
        // Tremolo: the bow reverses once per stroke; each reversal dips and
        // scrapes a little, down-strokes a touch heavier than up-strokes.
        let (shake, reversal) = if p.tremolo {
            let frac = self.trem_cycles.fract() as f32;
            let rev = (-(frac.min(1.0 - frac) / 0.12).powi(2)).exp() * smoothstep(since / 0.05);
            let down = (self.trem_cycles as i64) % 2 == 0;
            ((1.0 - p.trem_depth * rev) * if down { 1.03 } else { 0.97 }, rev)
        } else {
            (1.0, 0.0)
        };
        // A failed start stalls for a moment before it catches.
        let stall = 1.0 - p.stall * (-((since_a - 0.03) / 0.02).powi(2)).exp();
        let loudness = lifted
            * stall
            * if p.pizz {
                0.0
            } else if p.spiccato {
                (p.level * (1.0 + 0.25 * p.accent)).max(0.0) * bounce * fall
            } else if p.tremolo {
                ((p.level + p.ramp * prog) * shape).max(0.0) * rise * bite * fall * shake
            } else {
                ((p.level + p.ramp * prog) * shape).max(0.0) * rise * bite * fall
            };

        // Left hand: settling intonation, vibrato, moving into the next note.
        let settle = p.settle * (-since / p.settle_tau).exp();
        let mut pitch = p.pitch + settle;
        let mut pitch2 = p.double.map_or(0.0, |(d, _)| d + settle);
        let mut vib = p.vib_depth * smoothstep((since - p.vib_delay) / 0.3) * if p.tremolo { 0.4 } else { 1.0 };
        if let (Some(qn), Some(q)) = (self.next_visible(i, t), self.plans.get(i + 1)) {
            let to_next = (qn.start - t) as f32;
            if q.glide > 0.0 && to_next < q.glide {
                let s = smoothstep(1.0 - to_next / q.glide);
                pitch += (q.pitch - pitch) * s;
                if let (true, Some((d, _))) = (pitch2 > 0.0, q.double) {
                    pitch2 += (d - pitch2) * s;
                }
                vib *= 1.0 - s;
            }
        }
        pitch += vib * (self.vib_phase as f32).sin();
        if pitch2 > 0.0 {
            // One hand, one motion; the lower string moves a little less.
            pitch2 += 0.8 * vib * (self.vib_phase as f32).sin();
        }

        // Bow contact: louder and accented means closer to the bridge, brighter.
        let tone = self.inst.string_tone[p.string as usize];
        let attack_bite = if p.entry == Entry::Legato { 0.0 } else { (0.12 + 0.25 * p.accent) * (-since_a / 0.09).exp() };
        // A free-ringing string (bow lifted or stopped) loses its highs first.
        let release_dark = if matches!(exit, Exit::Decay(_)) && t > sound_end { 0.35 * (1.0 - fall) } else { 0.0 };
        // A lifted bow leaves the string ringing free: darker by the moment.
        let lifted = if p.spiccato { 0.25 * (-since / 0.015).exp() - 0.22 * (1.0 - (-since / 0.08).exp()) } else { 0.0 };
        let grit = if since < p.late { 0.0 } else { p.grit * (-since_a / p.grit_time).exp() } + 0.15 * reversal;
        let brightness =
            (0.5 + 0.45 * (loudness - 0.5) + tone + attack_bite + lifted - release_dark + 0.1 * grit).clamp(0.0, 1.0);

        // No scrape before the (slightly late) bow actually lands.
        let bitten = if since < p.late { 0.0 } else { 1.0 };
        let spike = match p.entry {
            Entry::Legato if p.crossing => 0.15,
            Entry::Legato => 0.08, // the finger landing
            Entry::BowChange => 0.3,
            Entry::Fresh => 0.35,
        } + 0.4 * p.accent;
        // A bowed string always breathes: a steady layer of rosin noise under the tone.
        let noise = if p.spiccato {
            // The hair hits the string (a tick), then hardly any rosin: the bow is off.
            (0.3 * self.style.breath * bounce + 0.95 * (-since / 0.02).exp()).min(1.0)
        } else {
            (0.3 * self.style.breath + spike * bitten * (-since_a / 0.035).exp() + if loudness < 0.25 { 0.08 } else { 0.0 }).min(1.0)
        };

        let string2 = p.double.map_or(0, |(_, s)| s);
        let noise = (noise + 0.25 * reversal).min(1.0);
        let brightness = (brightness + if p.tremolo { 0.08 } else { 0.0 }).min(1.0);
        Frame { pitch, loudness, brightness, noise, string: p.string, pitch2, string2, grit }
    }
}

/// Each string is tuned a touch off (a couple of cents, fixed per seed):
/// open strings ring exactly there, while stopped notes are fixed by ear.
fn string_detune(seed: u64, string: u8) -> f32 {
    0.025 * hash_bipolar(seed ^ 0x5791, string as usize)
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

/// A fingering choice: which string, where the hand ends up, how far it moved.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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
    for s in 0..inst.open_strings.len() {
        if let Some((cost, f)) = reach_cost(pitch, prev, hand, avoid_open, 0.0, s, inst) {
            if cost < best_cost {
                best_cost = cost;
                best = f;
            }
        }
    }
    best
}

/// What playing `pitch` on string `s` costs from where the hand is (see
/// [`choose_fingering`]), and the fingering; `None` if the string is too
/// high. `hurry` (0..1): how little time there is to get there; a hurried
/// shift costs up to twice as much, so shifts move to where there's time.
fn reach_cost(
    pitch: f32,
    prev: Option<u8>,
    hand: f32,
    avoid_open: bool,
    hurry: f32,
    s: usize,
    inst: &Instrument,
) -> Option<(f32, Fingering)> {
    let off = pitch - inst.open_strings[s];
    if off < -0.01 {
        return None;
    }
    let (mut cost, new_hand, shift) = if off.abs() < 0.01 {
        (if avoid_open { 2.5 } else { 0.0 }, hand, 0.0)
    } else if off >= hand - 0.01 && off <= hand + inst.hand_span + 0.01 {
        (0.0, hand, 0.0)
    } else {
        let to = if off < hand { off } else { off - inst.hand_span }.max(1.0);
        let shift = (to - hand).abs();
        ((1.0 + 0.25 * shift) * (1.0 + hurry), to, shift)
    };
    if s < 3 {
        cost += 0.15 * (off - 7.0).max(0.0);
    }
    cost += prev.map_or(0.0, |p| 0.35 * (s as f32 - p as f32).abs());
    Some((cost, Fingering { string: s as u8, hand: new_hand, shift }))
}

/// A fingering for two notes on neighbouring strings (lower on `s`, upper
/// on `s + 1`) that one hand can reach. Returns the upper note's fingering
/// and the lower string, or `None` if no hand could play it.
fn choose_double(high: f32, low: f32, prev: Option<u8>, hand: f32, inst: &Instrument) -> Option<(Fingering, u8)> {
    let os = &inst.open_strings;
    let mut best = None;
    let mut best_cost = f32::MAX;
    for s in 0..os.len() - 1 {
        let (o1, o2) = (low - os[s], high - os[s + 1]);
        if o1 < -0.01 || o2 < -0.01 {
            continue;
        }
        let stopped: Vec<f32> = [o1, o2].into_iter().filter(|o| o.abs() > 0.01).collect();
        let (lo, hi) = stopped.iter().fold((f32::MAX, f32::MIN), |(a, b), &o| (a.min(o), b.max(o)));
        let (mut cost, new_hand, shift) = if stopped.is_empty() || (lo >= hand - 0.01 && hi <= hand + inst.hand_span + 0.01) {
            (0.0, hand, 0.0)
        } else if hi - lo > inst.hand_span + 0.01 {
            continue; // the fingers can't stretch that far
        } else {
            let to = if lo < hand { lo } else { hi - inst.hand_span }.max(1.0);
            (1.0 + 0.25 * (to - hand).abs(), to, (to - hand).abs())
        };
        cost += 0.15 * (o1.max(o2) - 7.0).max(0.0);
        cost += prev.map_or(0.0, |p| 0.35 * (s as f32 + 1.0 - p as f32).abs());
        if cost < best_cost {
            best_cost = cost;
            best = Some((Fingering { string: s as u8 + 1, hand: new_hand, shift }, s as u8));
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
    fn long_notes_after_a_fast_run_are_bowed_and_follow_on_with_a_new_stroke() {
        let inst = cello();
        // Three repeated (so bounced) 16ths, a long note, then a quarter.
        let text = "0 0.12 C4 0.6\n0.13 0.12 C4 0.6\n0.26 0.12 C4 0.6\n0.39 1.0 F4 0.6\n1.4 0.5 G4 0.6";
        let mut p = Performer::new(&inst, &Style::default());
        for n in roll(text).notes {
            p.add_note(n);
        }
        let mut ev = Vec::new();
        for _ in 0..(3.0 * FRAME_RATE) as usize {
            p.next_frame(&mut ev);
        }
        let d: Vec<Played> = p.decisions().iter().map(|d| d.played).collect();
        assert_eq!(d[1], Played::Bounced);
        assert_ne!(d[3], Played::Bounced, "a 1 s note doesn't bounce");
        assert_eq!(d[3], Played::NewBow, "the bow is already moving after a bounce");
    }

    #[test]
    fn zig_zag_figures_are_bowed_separately_and_scales_are_slurred() {
        let inst = cello();
        let run = |keys: &[&str]| {
            let mut text = String::new();
            for (k, key) in keys.iter().enumerate() {
                text += &format!("{} 0.09 {key} 0.6\n", k as f64 * 0.09);
            }
            let mut p = Performer::new(&inst, &Style::default());
            for n in roll(&text).notes {
                p.add_note(n);
            }
            let mut ev = Vec::new();
            for _ in 0..(2.0 * FRAME_RATE) as usize {
                p.next_frame(&mut ev);
            }
            p.decisions().iter().map(|d| d.played).collect::<Vec<_>>()
        };
        // A pedal D2 rocking against notes on the next string (Beethoven Virus).
        let zig = run(&["D2", "A2", "D2", "G2", "D2", "F2", "D2", "G2"]);
        assert!(!zig.contains(&Played::Slurred), "{zig:?}");
        assert!(zig[1..7].iter().all(|p| *p == Played::Bounced), "{zig:?}");
        // A scale flows: slurred.
        let scale = run(&["C3", "D3", "E3", "F3", "G3", "A3", "B3", "C4"]);
        assert!(scale.iter().filter(|p| **p == Played::Slurred).count() >= 4, "{scale:?}");
    }

    #[test]
    fn the_finger_lifts_before_a_rest_and_the_open_string_takes_over() {
        let inst = cello();
        // E3 (stopped, D string) then a long rest; a short D3 (open) then a rest.
        let c = perform(&Score::parse("tempo 60\nE3:q r:h D3:e r:h").unwrap(), &inst, &Style::default());
        let rings: Vec<_> = c.rings.iter().filter(|r| r.open && r.time > 1.0 && r.time < 2.0).collect();
        assert_eq!(rings.len(), 1, "one lift after E3: {:?}", c.rings);
        assert!((rings[0].pitch - 50.0).abs() < 0.04, "the open D rings (a touch off-tune): {}", rings[0].pitch);
        assert!(frame_at(&c, rings[0].time + 0.06).loudness < 0.01, "the stopped note is choked");
        assert!(!c.rings.iter().any(|r| r.time > 3.0), "no lift for an open string: {:?}", c.rings);
    }

    fn decide(text: &str, style: &Style) -> (Curves, Vec<Decision>) {
        let inst = cello();
        let score = roll(text);
        let mut p = Performer::new(&inst, style);
        for n in score.notes.iter().cloned() {
            p.add_note(n);
        }
        let mut c = Curves::default();
        let mut ev = Vec::new();
        for _ in 0..(p.end() * FRAME_RATE) as usize {
            c.frames.push(p.next_frame(&mut ev));
        }
        (c, p.decisions())
    }

    #[test]
    fn workload_rises_when_frantic_and_composure_recovers_in_rests() {
        // Calm quarters, a frantic zig-zag across strings, a long rest, calm again.
        let mut text = String::new();
        for k in 0..4 {
            text += &format!("{} 1 {} 0.6\n", k, ["C3", "D3", "E3", "F3"][k]);
        }
        for k in 0..24 {
            text += &format!("{} 0.09 {} 0.6\n", 4.0 + k as f64 * 0.09, ["D2", "A2", "D2", "E3"][k % 4]);
        }
        text += "12 1 C3 0.6\n13 1 D3 0.6\n";
        let (_, d) = decide(&text, &Style::default());
        let calm = d[3];
        let frantic = d[4 + 23];
        let after = d[d.len() - 1];
        assert!(frantic.workload > calm.workload + 0.3, "{} vs {}", frantic.workload, calm.workload);
        assert!(frantic.composure < calm.composure - 0.05, "{} vs {}", frantic.composure, calm.composure);
        assert!(after.composure > frantic.composure + 0.05, "recovers in the rest: {} vs {}", after.composure, frantic.composure);
    }

    #[test]
    fn a_student_plays_less_in_tune_than_a_virtuoso_and_deadpan_is_exact() {
        // Long notes with shifts; measure how far each sits from the written
        // pitch once vibrato's averaged out (no vibrato here).
        let text = "0 0.8 C3 0.6\n0.8 0.8 A3 0.6\n1.6 0.8 F3 0.6\n2.4 0.8 D4 0.6\n3.2 0.8 G3 0.6\n4 0.8 E4 0.6\n4.8 0.8 B3 0.6\n5.6 0.8 C4 0.6";
        let notes = roll(text).notes;
        let miss = |skill: Skill, expression: f32| {
            let style = Style { skill, vibrato: 0.0, expression, ..Style::default() };
            let (c, _) = decide(text, &style);
            let mut total = 0.0;
            for n in &notes {
                // Just after landing: the initial miss, before the ear corrects it.
                let f = c.frames[((n.start + 0.02) * FRAME_RATE) as usize];
                total += (f.pitch - n.pitch).abs();
            }
            total / notes.len() as f32
        };
        let student = miss(Skill::profile("student").unwrap(), 1.0);
        let virtuoso = miss(Skill::profile("virtuoso").unwrap(), 1.0);
        assert!(student > virtuoso * 2.0, "student {student} vs virtuoso {virtuoso} semitones");
        assert!(miss(Skill::profile("student").unwrap(), 0.0) < 1e-4, "deadpan plays exactly in tune");
        // But a student has good ears: given time, a long note ends in tune.
        let style = Style { skill: Skill::profile("student").unwrap(), vibrato: 0.0, ..Style::default() };
        let (c, _) = decide(text, &style);
        let settled: f32 = notes
            .iter()
            .map(|n| (c.frames[((n.start + 0.6) * FRAME_RATE) as usize].pitch - n.pitch).abs())
            .sum::<f32>()
            / notes.len() as f32;
        assert!(settled < 0.06, "a student's long notes settle in tune: {settled} semitones off");
    }

    #[test]
    fn repeated_strokes_alternate_and_vary_but_deadpan_is_mechanical() {
        // Twelve separate, gapped notes on one pitch (like Beethoven's opening).
        let mut text = String::new();
        for k in 0..12 {
            text += &format!("{} 0.08 A3 0.6\n", k as f64 * 0.167);
        }
        let peaks = |style: &Style| {
            let (c, d) = decide(&text, style);
            let peaks: Vec<f32> = (0..12)
                .map(|k| {
                    let f = (k as f64 * 0.167 * FRAME_RATE) as usize;
                    (f..f + 20).map(|j| c.frames[j].loudness).fold(0.0, f32::max)
                })
                .collect();
            (peaks, d)
        };
        let (human, d) = peaks(&Style::default());
        let downs: Vec<bool> = d.iter().map(|x| x.down).collect();
        assert!(downs.windows(2).all(|w| w[0] != w[1]), "down, up, down, up…: {downs:?}");
        let spread = human.iter().cloned().fold(f32::MIN, f32::max) - human.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread > 0.03, "strokes should differ: {human:?}");
        let (machine, _) = peaks(&Style { expression: 0.0, vibrato: 0.0, ..Style::default() });
        let flat = machine.iter().cloned().fold(f32::MIN, f32::max) - machine.iter().cloned().fold(f32::MAX, f32::min);
        assert!(flat < 0.01, "deadpan is a machine: {machine:?}");
    }

    #[test]
    fn a_bow_cannot_bounce_beyond_about_twelve_a_second() {
        let run = |step: f64| {
            let mut text = String::new();
            for k in 0..8 {
                text += &format!("{} {} C4 0.6\n", k as f64 * step, step * 0.95);
            }
            decide(&text, &Style::default()).1.iter().filter(|d| d.played == Played::Bounced).count()
        };
        assert!(run(0.1) >= 6, "10 a second bounces");
        assert_eq!(run(0.06), 0, "16 a second can't: tiny strokes on the string");
    }

    #[test]
    fn strokes_start_with_grit_that_fades_and_students_scrape_more() {
        let mut text = String::new();
        for k in 0..40 {
            text += &format!("{} 0.2 {} 0.6\n", k as f64 * 0.25, ["C3", "D3", "E3", "G3"][k % 4]);
        }
        let grit_at = |style: &Style, after: f64| {
            let (c, _) = decide(&text, style);
            (0..40).map(|k| c.frames[((k as f64 * 0.25 + after) * FRAME_RATE) as usize].grit).sum::<f32>() / 40.0
        };
        let pro = Style::default();
        assert!(grit_at(&pro, 0.02) > 0.05, "a little scratch at the start");
        assert!(grit_at(&pro, 0.15) < 0.01, "gone once the bow has caught");
        let student = Style { skill: Skill::profile("student").unwrap(), ..Style::default() };
        let virtuoso = Style { skill: Skill::profile("virtuoso").unwrap(), ..Style::default() };
        assert!(grit_at(&student, 0.02) > grit_at(&virtuoso, 0.02) * 1.5);
        let fails = |style: &Style| {
            let mut p = Performer::new(&cello(), style);
            for n in roll(&text).notes {
                p.add_note(n);
            }
            let mut ev = Vec::new();
            for _ in 0..(p.end() * FRAME_RATE) as usize {
                p.next_frame(&mut ev);
            }
            p.plans.iter().filter(|x| x.stall > 0.0).count()
        };
        assert!(fails(&student) > fails(&virtuoso), "{} vs {}", fails(&student), fails(&virtuoso));
        assert_eq!(grit_at(&Style { expression: 0.0, ..Style::default() }, 0.02), 0.0, "deadpan is clean");
    }

    #[test]
    fn tremolo_shakes_the_bow_and_the_patch_spares_short_notes() {
        let inst = cello();
        let c = perform(&Score::parse("tempo 60\n~D3:w").unwrap(), &inst, &Style::default());
        // Count dips (bow reversals) over two seconds in the middle.
        let l: Vec<f32> = (250..750).map(|f| c.frames[f].loudness).collect();
        let dips = (1..l.len() - 1).filter(|&k| l[k] < l[k - 1] && l[k] <= l[k + 1] && l[k] < 0.97 * l[k - 1].max(l[k + 1]) + 0.0).count();
        assert!((24..=36).contains(&dips), "12–18 strokes a second, got {dips} in 2 s");
        let patch = Style { tremolo: true, ..Style::default() };
        let (_, d) = decide("0 1 C3 0.6\n1 0.12 D3 0.6\n1.12 0.12 E3 0.6\n1.24 1 F3 0.6", &patch);
        let how: Vec<Played> = d.iter().map(|x| x.played).collect();
        assert_eq!(how[0], Played::Tremolo);
        assert_ne!(how[1], Played::Tremolo, "short notes are played normally");
        assert_eq!(how[3], Played::Tremolo);
    }

    #[test]
    fn a_finger_left_ringing_lets_go_when_the_hand_moves_away() {
        // E3 (D string, finger at +2) crossing to A3's string, then the hand
        // shifts high up the A string: the D string can't still sound E3.
        let c = perform(&Score::parse("tempo 120\nE3:q B3 A4:h").unwrap(), &cello(), &Style::default());
        let stopped: Vec<&Ring> = c.rings.iter().filter(|r| r.string == 2 && !r.open).collect();
        assert_eq!(stopped.len(), 1, "E3 rings on after the crossing: {:?}", c.rings);
        let later: Vec<&Ring> = c.rings.iter().filter(|r| r.string == 2 && r.time > stopped[0].time).collect();
        let lift_tap = c.taps.iter().any(|k| k.time > stopped[0].time + 0.1 && k.strength < 0.15);
        assert!(lift_tap, "the finger lifts when the hand shifts away: rings {:?} taps {:?}", later, c.taps);
        assert!(later.iter().all(|r| r.open), "and the string rings open, if at all");
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
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../scores/lament.txt")).unwrap();
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
    fn double_stops_need_neighbouring_strings_and_a_hand_that_reaches() {
        let inst = cello();
        // D3 + A3: open D and open A? Not neighbours (strings 2 and 3 are D, A: yes).
        let (f, lower) = choose_double(57.0, 50.0, None, 1.0, &inst).unwrap();
        assert_eq!((f.string, lower), (3, 2));
        // A sixth (C3 + A3): G string + D string... or neighbours that reach.
        assert!(choose_double(57.0, 48.0, None, 1.0, &inst).is_some());
        // Two octaves apart: impossible on neighbouring strings.
        assert!(choose_double(62.0, 38.0, None, 1.0, &inst).is_none());
        // In a performance: both pitches present for a long note, not a short one.
        let c = perform(&Score::parse("tempo 60\nA3+D3:h C4+E3:t").unwrap(), &inst, &Style::default());
        assert!((frame_at(&c, 0.5).pitch2 - 50.0).abs() < 0.3);
        assert_eq!(frame_at(&c, 2.06).pitch2, 0.0, "a 32nd is too short to bother");
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
