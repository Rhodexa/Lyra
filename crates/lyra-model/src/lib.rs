//! # lyra-model
//!
//! What a Lyra project *is*: channels, patterns, notes. Plain data — no audio,
//! no UI. The UI edits this; a small bridge compiles it into what the engine plays.
//!
//! FL-style idea worth knowing: **a step IS a note.** Lighting a step in the
//! channel rack just adds a one-step note at the default key. The piano roll
//! (milestone 2) edits the very same notes, so the two views never disagree.

pub mod file;
pub mod theory;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub use theory::{Chord, Scale};

/// Musical time unit. Must equal `lyra_engine::TICKS_PER_STEP` (checked in the app).
pub const TICKS_PER_STEP: u32 = 24;
pub const STEPS_PER_BAR: u32 = 16;
/// Steps are placed at this key; drums play at their natural pitch here.
pub const DEFAULT_KEY: u8 = 60;
pub const TICKS_PER_BAR: u32 = TICKS_PER_STEP * STEPS_PER_BAR;

pub type ChannelId = u32;
/// Stable pattern identity: clips refer to patterns by id, so cloning or
/// reordering patterns never makes a clip play the wrong one.
pub type PatternId = u32;
/// Stable identity of an audio clip (the shared thing clips point at).
pub type AudioId = u32;
/// Stable identity of an automation (the shared curve automation clips play).
pub type AutomationId = u32;
/// Stable identity of an effect in a mixer track's chain (automation finds
/// it by this, wherever its slot moves).
pub type EffectId = u32;
/// Identity of one clip on the playlist, while Lyra runs (so the UI can
/// say "this clip" even as others are added and deleted). Not saved: clips
/// are numbered afresh when a project is opened.
pub type ClipId = u32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// In ticks from the pattern start.
    pub start: u32,
    pub len: u32,
    pub key: u8,
    pub velocity: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    /// Stable identity (survives reordering). Patterns refer to channels by id.
    pub id: ChannelId,
    pub name: String,
    /// Instrument id, e.g. "lyra.kick".
    pub instrument: String,
    /// Instrument parameters, indexed by parameter id.
    pub params: Vec<f32>,
    pub volume: f32,
    pub pan: f32,
    /// Solo is FL-style: it mutes the other channels, so mutes are the whole truth.
    pub mute: bool,
    /// 0 = none. Channels in the same group cut each other off.
    pub choke: u8,
    /// Index into the theme's channel palette.
    pub color: usize,
    /// Mixer track this channel plays into (0 = master).
    #[serde(default)]
    pub mixer: usize,
    /// The sample a sampler plays: "bundled:<path>" for Lyra's own library,
    /// otherwise a path on disk. None for every other instrument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub id: PatternId,
    pub name: String,
    /// The shortest this pattern may be, in steps (16 = one bar). The pattern
    /// grows by itself to fit its notes — see [`Pattern::play_length`].
    pub min_length: u32,
    pub notes: BTreeMap<ChannelId, Vec<Note>>,
}

impl Pattern {
    pub fn new(id: PatternId, name: &str) -> Self {
        Self { id, name: name.to_string(), min_length: STEPS_PER_BAR, notes: BTreeMap::new() }
    }

    pub fn notes(&self, ch: ChannelId) -> &[Note] {
        self.notes.get(&ch).map_or(&[], |v| v.as_slice())
    }

    pub fn notes_mut(&mut self, ch: ChannelId) -> &mut Vec<Note> {
        self.notes.entry(ch).or_default()
    }

    /// How long the pattern plays, in steps: up to the end of the last bar that
    /// has a note in it, but never shorter than `min_length`. Patterns are
    /// dynamic — write into bar 3 and the loop becomes 3 bars, delete it and it shrinks.
    pub fn play_length(&self) -> u32 {
        // Whole bars, up to where the last note *ends* (a note ending exactly on
        // a bar line doesn't spill into the next bar).
        let last_bar =
            self.notes.values().flatten().map(|n| (n.start + n.len.max(1) - 1) / TICKS_PER_BAR + 1).max().unwrap_or(1);
        (last_bar * STEPS_PER_BAR).max(self.min_length)
    }

    /// True when this channel's notes are all plain steps (one-step long, on
    /// the grid, one per step), so the channel rack can show them as buttons.
    pub fn is_step_channel(&self, ch: ChannelId) -> bool {
        let notes = self.notes(ch);
        notes.iter().all(|n| n.start % TICKS_PER_STEP == 0 && n.len == TICKS_PER_STEP)
            && notes.iter().enumerate().all(|(i, a)| notes[..i].iter().all(|b| b.start != a.start))
    }

    /// The note sitting exactly on `step`, if any.
    pub fn step(&self, ch: ChannelId, step: u32) -> Option<&Note> {
        self.notes(ch).iter().find(|n| n.start == step * TICKS_PER_STEP)
    }

    pub fn step_mut(&mut self, ch: ChannelId, step: u32) -> Option<&mut Note> {
        self.notes.get_mut(&ch)?.iter_mut().find(|n| n.start == step * TICKS_PER_STEP)
    }

    /// Light or clear a step. Returns true if anything changed.
    pub fn set_step(&mut self, ch: ChannelId, step: u32, on: bool, velocity: f32) -> bool {
        let start = step * TICKS_PER_STEP;
        let notes = self.notes.entry(ch).or_default();
        let existing = notes.iter().position(|n| n.start == start);
        match (on, existing) {
            (true, None) => {
                notes.push(Note { start, len: TICKS_PER_STEP, key: DEFAULT_KEY, velocity });
                notes.sort_by_key(|n| n.start);
                true
            }
            (false, Some(i)) => {
                notes.remove(i);
                true
            }
            _ => false,
        }
    }

    /// Replace a channel's steps with "one every `every` steps".
    pub fn fill_every(&mut self, ch: ChannelId, every: u32) {
        let length = self.play_length();
        let notes = self.notes.entry(ch).or_default();
        notes.clear();
        for step in (0..length).step_by(every.max(1) as usize) {
            notes.push(Note { start: step * TICKS_PER_STEP, len: TICKS_PER_STEP, key: DEFAULT_KEY, velocity: 0.8 });
        }
    }
}

/// One row of the playlist.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    pub mute: bool,
}

fn unity() -> f32 {
    1.0
}

fn is_zero<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// A fade at the start or end of an audio clip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Fade {
    /// Length in seconds (0 = no fade).
    pub len: f32,
    /// Curve, -1..1, as drawn: 0 is a straight line, below 0 it bows up
    /// (stays louder through the fade), above 0 it bows down (quieter).
    pub tension: f32,
}

/// An audio clip's *source*: a sample plus how to play it. Like a pattern,
/// it is shared: every clip on the playlist that points at it plays it, so a
/// change here (trim, gain) changes all of them. Fades belong to each placed
/// clip (see [`Clip`]), along with its own gain. Nothing touches the file on
/// disk.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Audio {
    pub id: AudioId,
    pub name: String,
    /// "bundled:<path>" for Lyra's own library, otherwise a path on disk.
    pub sample: String,
    /// Volume (a linear gain, the plugin rack's fader law), pan and mute:
    /// it sits in the Audio rack like an instrument.
    #[serde(default = "unity")]
    pub volume: f32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub mute: bool,
    /// The part of the file that plays, in seconds from its start.
    pub start: f64,
    pub end: f64,
    /// Index into the theme's palette.
    #[serde(default)]
    pub color: usize,
    /// Mixer track it plays into (0 = master), like a channel's.
    #[serde(default)]
    pub mixer: usize,
}

impl Audio {
    /// Seconds of the file that play.
    pub fn length(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }
}

/// What a clip plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Pattern(PatternId),
    Audio(AudioId),
    Automation(AutomationId),
}

/// A control automation can move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    /// An instrument's parameter.
    Param {
        channel: ChannelId,
        param: u32,
    },
    /// A channel's volume / pan (in the plugin rack).
    Volume(ChannelId),
    Pan(ChannelId),
    /// A mixer track's volume / pan (0 = master).
    MixerVolume(usize),
    MixerPan(usize),
    /// An effect's parameter, wherever its slot is.
    Effect {
        effect: EffectId,
        param: u32,
    },
    /// The master knob in the top bar.
    MasterVolume,
}

/// One control an automation moves, and how far: the curve's 0..1 maps to
/// the control's travel from `from` to `to` (also 0..1, as the knob turns;
/// `from` > `to` turns it the other way).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub target: Target,
    pub from: f32,
    pub to: f32,
}

impl Link {
    pub fn full(target: Target) -> Self {
        Link { target, from: 0.0, to: 1.0 }
    }

    /// Knob travel for curve value `v` (0..1).
    pub fn travel(&self, v: f32) -> f32 {
        self.from + (self.to - self.from) * v.clamp(0.0, 1.0)
    }
}

/// A point of an automation curve.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Ticks from the automation's start.
    pub tick: u32,
    /// 0..1.
    pub value: f32,
    /// The bend of the line *to the next point* (-1..1, as a fade's tension).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub tension: f32,
}

/// An automation: a curve, and the controls it moves. Shared like a pattern:
/// every clip that plays it plays the same curve; a clip longer than
/// `length` repeats it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Automation {
    pub id: AutomationId,
    pub name: String,
    /// Sorted by tick, at least one.
    pub points: Vec<Point>,
    /// Ticks the curve lasts before it starts over.
    pub length: u32,
    /// What it moves: one curve can drive several controls.
    pub links: Vec<Link>,
    #[serde(default)]
    pub color: usize,
}

/// Something placed on the playlist.
///
/// * A **pattern** clip is a window onto its pattern repeated forever: it
///   plays from `offset` ticks in (so a clip can start mid-pattern), and a
///   clip longer than the pattern repeats it (stretch a 2-bar beat across
///   16 bars). See [`Clip::pattern_notes`].
/// * An **audio** clip is a window onto its audio: it plays from `offset`
///   ticks in, for `len` ticks. Dragging its left edge moves both `start` and
///   `offset`, so the sound stays in place. Its gain and fades are its own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "ClipFile", into = "ClipFile")]
pub struct Clip {
    /// Given by [`Project::add_clip`]; see [`ClipId`].
    pub id: ClipId,
    pub source: Source,
    /// Playlist row.
    pub track: usize,
    /// Song position and length, in ticks.
    pub start: u32,
    pub len: u32,
    /// Ticks into the source where this clip begins (audio clips).
    pub offset: u32,
    /// This clip's own gain (dB) and fades, at its own edges (audio clips).
    pub gain: f32,
    pub fade_in: Fade,
    pub fade_out: Fade,
    /// Play the window backwards (audio clips). The fades stay where the
    /// clip starts and ends on the playlist.
    pub reverse: bool,
}

/// New audio clips fade 5 ms at each end: no clicks where the sound is cut.
pub const EDGE_FADE: Fade = Fade { len: 0.005, tension: 0.0 };

impl Clip {
    pub fn of_pattern(pattern: PatternId, track: usize, start: u32, len: u32) -> Self {
        Self {
            id: 0,
            source: Source::Pattern(pattern),
            track,
            start,
            len,
            offset: 0,
            gain: 0.0,
            fade_in: Fade::default(),
            fade_out: Fade::default(),
            reverse: false,
        }
    }

    pub fn of_audio(audio: AudioId, track: usize, start: u32, len: u32) -> Self {
        Self {
            source: Source::Audio(audio),
            fade_in: EDGE_FADE,
            fade_out: EDGE_FADE,
            ..Self::of_pattern(0, track, start, len)
        }
    }

    pub fn end(&self) -> u32 {
        self.start + self.len
    }

    /// Where this (pattern) clip plays `pat`'s notes for channel `ch`:
    /// (ticks from the clip's start, length cut at the clip's end, the note).
    /// A note that starts before the clip's window doesn't sound.
    pub fn pattern_notes<'a>(
        &self,
        pat: &'a Pattern,
        ch: ChannelId,
    ) -> impl Iterator<Item = (u32, u32, &'a Note)> + 'a {
        let loop_len = (pat.play_length() * TICKS_PER_STEP).max(1) as i64;
        let (offset, len) = ((self.offset as i64) % loop_len, self.len as i64);
        let reps = (offset + len + loop_len - 1) / loop_len;
        (0..reps).flat_map(move |k| {
            let base = k * loop_len - offset;
            pat.notes(ch).iter().filter_map(move |n| {
                let at = base + n.start as i64;
                ((n.start as i64) < loop_len && at >= 0 && at < len)
                    .then(|| (at as u32, (n.len as i64).min(len - at) as u32, n))
            })
        })
    }

    /// Where this (pattern) clip's pattern starts over, in ticks from the
    /// clip's start (not counting 0).
    pub fn pattern_restarts(&self, pat: &Pattern) -> impl Iterator<Item = u32> {
        let loop_len = (pat.play_length() * TICKS_PER_STEP).max(1);
        let first = (loop_len - self.offset % loop_len) % loop_len;
        let len = self.len;
        (0..).map(move |k| first + k * loop_len).filter(|&t| t > 0).take_while(move |&t| t < len)
    }

    pub fn pattern(&self) -> Option<PatternId> {
        match self.source {
            Source::Pattern(id) => Some(id),
            _ => None,
        }
    }

    pub fn audio(&self) -> Option<AudioId> {
        match self.source {
            Source::Audio(id) => Some(id),
            _ => None,
        }
    }

    pub fn automation(&self) -> Option<AutomationId> {
        match self.source {
            Source::Automation(id) => Some(id),
            _ => None,
        }
    }

    pub fn of_automation(automation: AutomationId, track: usize, start: u32, len: u32) -> Self {
        Self { source: Source::Automation(automation), ..Self::of_pattern(0, track, start, len) }
    }
}

/// How a clip is written in a project file: `"pattern": id` or `"audio": id`
/// (files from before audio clips only have `pattern`).
#[derive(Clone, Serialize, Deserialize)]
struct ClipFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pattern: Option<PatternId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    audio: Option<AudioId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    automation: Option<AutomationId>,
    track: usize,
    start: u32,
    len: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    offset: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    gain: f32,
    #[serde(default, skip_serializing_if = "is_zero")]
    fade_in: Fade,
    #[serde(default, skip_serializing_if = "is_zero")]
    fade_out: Fade,
    #[serde(default, skip_serializing_if = "is_zero")]
    reverse: bool,
}

impl From<ClipFile> for Clip {
    fn from(f: ClipFile) -> Self {
        let source = match (f.automation, f.audio, f.pattern) {
            (Some(a), _, _) => Source::Automation(a),
            (None, Some(a), _) => Source::Audio(a),
            (None, None, p) => Source::Pattern(p.unwrap_or(0)),
        };
        Clip {
            id: 0, // numbered by the project once it's read
            source,
            track: f.track,
            start: f.start,
            len: f.len,
            offset: f.offset,
            gain: f.gain,
            fade_in: f.fade_in,
            fade_out: f.fade_out,
            reverse: f.reverse,
        }
    }
}

impl From<Clip> for ClipFile {
    fn from(c: Clip) -> Self {
        ClipFile {
            pattern: c.pattern(),
            audio: c.audio(),
            automation: c.automation(),
            track: c.track,
            start: c.start,
            len: c.len,
            offset: c.offset,
            gain: c.gain,
            fade_in: c.fade_in,
            fade_out: c.fade_out,
            reverse: c.reverse,
        }
    }
}

pub const DEFAULT_TRACKS: usize = 12;
/// Mixer tracks: 0 = master, 1..=16 inserts (matches the engine).
pub const MIXER_TRACKS: usize = 17;

/// One effect in a mixer track's chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectSlot {
    /// This slot's identity in the project (see [`EffectId`]); given by
    /// [`Project::number_effects`]. Not to be confused with `effect`.
    #[serde(default)]
    pub id: EffectId,
    /// Effect id, e.g. "lyra.reverb".
    pub effect: String,
    pub params: Vec<f32>,
    #[serde(default)]
    pub bypass: bool,
}

impl EffectSlot {
    /// A new slot (its id is given when the project numbers its effects).
    pub fn new(effect: &str, params: Vec<f32>) -> Self {
        EffectSlot { id: 0, effect: effect.to_string(), params, bypass: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MixerTrack {
    pub name: String,
    pub volume: f32,
    pub pan: f32,
    pub mute: bool,
    pub effects: Vec<EffectSlot>,
}

impl MixerTrack {
    fn new(name: &str) -> Self {
        Self { name: name.into(), volume: 0.8, pan: 0.0, mute: false, effects: Vec::new() }
    }
}

pub fn default_mixer() -> Vec<MixerTrack> {
    let mut v = vec![MixerTrack { volume: 1.0, ..MixerTrack::new("Master") }];
    v.extend((1..MIXER_TRACKS).map(|i| MixerTrack::new(&format!("Insert {i}"))));
    v
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)] // a field missing from an older file gets its default
pub struct Project {
    pub bpm: f64,
    pub swing: f32,
    pub master_volume: f32,
    /// Key of the song (0 = C … 11 = B) and its scale: drives piano-roll help.
    pub key_root: u8,
    pub scale: Scale,
    pub channels: Vec<Channel>,
    pub patterns: Vec<Pattern>,
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    /// Audio clips (shared, like patterns).
    pub audio: Vec<Audio>,
    /// Automations (shared, like patterns).
    pub automations: Vec<Automation>,
    pub mixer: Vec<MixerTrack>,
    next_id: ChannelId,
    next_pattern_id: PatternId,
    next_audio_id: AudioId,
    next_automation_id: AutomationId,
    next_effect_id: EffectId,
    #[serde(skip)]
    next_clip_id: ClipId,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            bpm: 124.0,
            swing: 0.0,
            master_volume: 0.8,
            key_root: 9, // A
            scale: Scale::Minor,
            channels: Vec::new(),
            patterns: vec![Pattern::new(1, "Pattern 1")],
            tracks: (1..=DEFAULT_TRACKS).map(|i| Track { name: format!("Track {i}"), mute: false }).collect(),
            clips: Vec::new(),
            audio: Vec::new(),
            automations: Vec::new(),
            mixer: default_mixer(),
            next_id: 1,
            next_pattern_id: 2,
            next_audio_id: 1,
            next_automation_id: 1,
            next_effect_id: 1,
            next_clip_id: 1,
        }
    }
}

impl Project {
    pub fn add_channel(&mut self, name: &str, instrument: &str, params: Vec<f32>) -> ChannelId {
        let id = self.next_id;
        self.next_id += 1;
        let color = self.channels.len();
        let used: Vec<usize> = self.channels.iter().map(|c| c.mixer).collect();
        let mixer = (1..MIXER_TRACKS).find(|t| !used.contains(t)).unwrap_or(0);
        self.channels.push(Channel {
            id,
            name: name.to_string(),
            instrument: instrument.to_string(),
            params,
            volume: 0.8,
            pan: 0.0,
            mute: false,
            choke: 0,
            color,
            mixer,
            sample: None,
        });
        id
    }

    pub fn channel_mut(&mut self, id: ChannelId) -> Option<&mut Channel> {
        self.channels.iter_mut().find(|c| c.id == id)
    }

    /// Removes the channel and its notes from every pattern.
    pub fn remove_channel(&mut self, index: usize) {
        if index < self.channels.len() {
            let id = self.channels.remove(index).id;
            for p in &mut self.patterns {
                p.notes.remove(&id);
            }
        }
    }

    /// Adds an empty pattern (one bar) and returns its index.
    pub fn add_pattern(&mut self) -> usize {
        let name = format!("Pattern {}", self.patterns.len() + 1);
        let id = self.next_pattern_id;
        self.next_pattern_id += 1;
        self.patterns.push(Pattern::new(id, &name));
        self.patterns.len() - 1
    }

    /// Duplicates a pattern (notes and length) right after itself; returns the new index.
    pub fn clone_pattern(&mut self, index: usize) -> usize {
        let mut copy = self.patterns[index].clone();
        copy.name = format!("{} copy", copy.name);
        copy.id = self.next_pattern_id;
        self.next_pattern_id += 1;
        self.patterns.insert(index + 1, copy);
        index + 1
    }

    pub fn pattern_index(&self, id: PatternId) -> Option<usize> {
        self.patterns.iter().position(|p| p.id == id)
    }

    pub fn pattern(&self, id: PatternId) -> Option<&Pattern> {
        self.patterns.iter().find(|p| p.id == id)
    }

    /// Puts a clip on the playlist with a fresh id; returns its index.
    pub fn add_clip(&mut self, mut clip: Clip) -> usize {
        clip.id = self.next_clip_id.max(1);
        self.next_clip_id = clip.id + 1;
        self.clips.push(clip);
        self.clips.len() - 1
    }

    pub fn clip_index(&self, id: ClipId) -> Option<usize> {
        self.clips.iter().position(|c| c.id == id)
    }

    /// Give every effect slot without an id one (new slots, and files from
    /// before effects had ids). Call after adding effects.
    pub fn number_effects(&mut self) {
        let mut next = self
            .mixer
            .iter()
            .flat_map(|t| &t.effects)
            .map(|e| e.id)
            .max()
            .unwrap_or(0)
            .max(self.next_effect_id.saturating_sub(1))
            + 1;
        for e in self.mixer.iter_mut().flat_map(|t| &mut t.effects) {
            if e.id == 0 {
                e.id = next;
                next += 1;
            }
        }
        self.next_effect_id = next;
    }

    /// Where effect `id` is: (mixer track, slot).
    pub fn effect_slot(&self, id: EffectId) -> Option<(usize, usize)> {
        self.mixer
            .iter()
            .enumerate()
            .find_map(|(t, track)| track.effects.iter().position(|e| e.id == id).map(|s| (t, s)))
    }

    /// Adds an automation moving `target`, flat at `travel` (0..1), `length`
    /// ticks long; returns its index.
    pub fn add_automation(&mut self, name: &str, target: Target, travel: f32, length: u32) -> usize {
        let id = self.next_automation_id;
        self.next_automation_id += 1;
        let v = travel.clamp(0.0, 1.0);
        self.automations.push(Automation {
            id,
            name: name.to_string(),
            points: vec![Point { tick: 0, value: v, tension: 0.0 }, Point { tick: length, value: v, tension: 0.0 }],
            length: length.max(1),
            links: vec![Link::full(target)],
            color: self.patterns.len() + self.audio.len() + self.automations.len(),
        });
        self.automations.len() - 1
    }

    pub fn automation(&self, id: AutomationId) -> Option<&Automation> {
        self.automations.iter().find(|a| a.id == id)
    }

    pub fn automation_index(&self, id: AutomationId) -> Option<usize> {
        self.automations.iter().position(|a| a.id == id)
    }

    /// The first control whose value differs from `before`: what was just
    /// tweaked (by any editor, any plugin window), for "automate it".
    pub fn changed_control(&self, before: &Project) -> Option<Target> {
        for ch in &self.channels {
            let Some(old) = before.channels.iter().find(|c| c.id == ch.id) else { continue };
            if let Some(i) = (0..ch.params.len()).find(|&i| old.params.get(i) != Some(&ch.params[i])) {
                return Some(Target::Param { channel: ch.id, param: i as u32 });
            }
            if old.volume != ch.volume {
                return Some(Target::Volume(ch.id));
            }
            if old.pan != ch.pan {
                return Some(Target::Pan(ch.id));
            }
        }
        for (t, track) in self.mixer.iter().enumerate() {
            let Some(old) = before.mixer.get(t) else { continue };
            if old.volume != track.volume {
                return Some(Target::MixerVolume(t));
            }
            if old.pan != track.pan {
                return Some(Target::MixerPan(t));
            }
            for e in &track.effects {
                let Some(o) = old.effects.iter().find(|o| o.id == e.id) else { continue };
                if let Some(i) = (0..e.params.len()).find(|&i| o.params.get(i) != Some(&e.params[i])) {
                    return Some(Target::Effect { effect: e.id, param: i as u32 });
                }
            }
        }
        (self.master_volume != before.master_volume).then_some(Target::MasterVolume)
    }

    /// Give every clip a fresh id (after reading a project file).
    pub fn number_clips(&mut self) {
        for (i, c) in self.clips.iter_mut().enumerate() {
            c.id = i as ClipId + 1;
        }
        self.next_clip_id = self.clips.len() as ClipId + 1;
    }

    /// Adds an audio clip playing all `seconds` of `sample`; returns its index.
    pub fn add_audio(&mut self, name: &str, sample: &str, seconds: f64) -> usize {
        let id = self.next_audio_id;
        self.next_audio_id += 1;
        let color = self.patterns.len() + self.audio.len();
        self.audio.push(Audio {
            id,
            name: name.to_string(),
            sample: sample.to_string(),
            volume: 1.0,
            pan: 0.0,
            mute: false,
            start: 0.0,
            end: seconds.max(0.0),
            color,
            mixer: 0,
        });
        self.audio.len() - 1
    }

    /// Duplicates an audio clip (so it can be changed on its own); returns the new index.
    pub fn clone_audio(&mut self, index: usize) -> usize {
        let mut copy = self.audio[index].clone();
        copy.name = format!("{} copy", copy.name);
        copy.color = self.patterns.len() + self.audio.len(); // its own colour: visibly a different clip
        copy.id = self.next_audio_id;
        self.next_audio_id += 1;
        self.audio.insert(index + 1, copy);
        index + 1
    }

    pub fn audio_index(&self, id: AudioId) -> Option<usize> {
        self.audio.iter().position(|a| a.id == id)
    }

    /// Every file name the project refers to: samplers' samples and audio
    /// clips' sounds (the app decides what a name means).
    pub fn file_names_mut(&mut self) -> impl Iterator<Item = &mut String> {
        let channels = self.channels.iter_mut().filter_map(|c| c.sample.as_mut());
        channels.chain(self.audio.iter_mut().map(|a| &mut a.sample))
    }

    pub fn audio(&self, id: AudioId) -> Option<&Audio> {
        self.audio.iter().find(|a| a.id == id)
    }

    /// Seconds per tick at the project's tempo.
    pub fn seconds_per_tick(&self) -> f64 {
        60.0 / (self.bpm.max(1.0) * (TICKS_PER_STEP * 4) as f64)
    }

    /// The song runs to the end of the last clip (whole bars, at least one bar).
    pub fn song_length(&self) -> u32 {
        let end = self.clips.iter().map(Clip::end).max().unwrap_or(0);
        end.div_ceil(TICKS_PER_BAR).max(1) * TICKS_PER_BAR
    }
}

/// Snapshot undo: simple and bulletproof for now. (Projects are small; when
/// they aren't, this becomes command-based without changing its callers.)
#[derive(Default)]
pub struct History {
    past: Vec<Project>,
    future: Vec<Project>,
}

impl History {
    const LIMIT: usize = 200;

    /// Call *before* changing the project. A checkpoint identical to the last
    /// one is skipped (a gesture that changed nothing leaves no undo step,
    /// and doesn't throw away redo either).
    pub fn checkpoint(&mut self, project: &Project) {
        if self.past.last() == Some(project) {
            return;
        }
        self.past.push(project.clone());
        if self.past.len() > Self::LIMIT {
            self.past.remove(0);
        }
        self.future.clear();
    }

    pub fn undo(&mut self, project: &mut Project) -> bool {
        // The last checkpoint can equal the project when the gesture after it
        // changed nothing: undo goes past it, so every undo does something.
        while self.past.last() == Some(project) {
            self.past.pop();
        }
        match self.past.pop() {
            Some(prev) => {
                self.future.push(std::mem::replace(project, prev));
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self, project: &mut Project) -> bool {
        match self.future.pop() {
            Some(next) => {
                self.past.push(std::mem::replace(project, next));
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_notes() {
        let mut p = Pattern::new(1, "t");
        assert!(p.set_step(7, 4, true, 0.8));
        assert!(!p.set_step(7, 4, true, 0.8), "lighting twice changes nothing");
        assert_eq!(p.notes(7)[0].start, 4 * TICKS_PER_STEP);
        assert!(p.set_step(7, 4, false, 0.0));
        assert!(p.notes(7).is_empty());
    }

    #[test]
    fn patterns_grow_and_shrink_with_their_notes() {
        let mut p = Pattern::new(1, "t");
        assert_eq!(p.play_length(), 16, "empty = one bar");
        p.notes_mut(1).push(Note { start: 2 * TICKS_PER_BAR + 5, len: 10, key: 60, velocity: 1.0 });
        assert_eq!(p.play_length(), 48, "a note in bar 3 makes it 3 bars");
        p.notes_mut(1).clear();
        assert_eq!(p.play_length(), 16, "and it shrinks back");
        p.min_length = 32;
        assert_eq!(p.play_length(), 32, "min_length is a floor");
        p.min_length = 16;
        // A note that starts in bar 1 but rings into bar 2 makes it 2 bars...
        p.notes_mut(1).push(Note { start: TICKS_PER_BAR - 24, len: 48, key: 60, velocity: 1.0 });
        assert_eq!(p.play_length(), 32, "a note crossing the bar line lengthens the pattern");
        // ...but one that ends exactly on the bar line doesn't.
        p.notes_mut(1)[0].len = 24;
        assert_eq!(p.play_length(), 16, "ending on the bar line stays inside");
    }

    #[test]
    fn clips_keep_pointing_at_the_right_pattern() {
        let mut proj = Project::default();
        let a = proj.patterns[0].id;
        proj.clips.push(Clip::of_pattern(a, 0, 0, TICKS_PER_BAR));
        proj.add_pattern();
        let b = proj.patterns[1].id;
        proj.clone_pattern(0); // inserts at index 1, shifting "b" to index 2
        assert_eq!(proj.pattern(a).unwrap().name, "Pattern 1");
        assert_eq!(proj.pattern_index(b), Some(2));
        assert_ne!(proj.patterns[1].id, a, "a clone gets its own id");
        assert_eq!(proj.song_length(), TICKS_PER_BAR);
    }

    #[test]
    fn audio_clips_are_shared_and_clonable() {
        let mut proj = Project::default();
        let i = proj.add_audio("line 1", "/voice/line1.wav", 2.5);
        let id = proj.audio[i].id;
        let first = proj.add_clip(Clip::of_audio(id, 0, 0, TICKS_PER_BAR));
        let second = proj.add_clip(Clip::of_audio(id, 1, TICKS_PER_BAR, TICKS_PER_BAR));
        assert_ne!(proj.clips[first].id, proj.clips[second].id, "every clip is told apart");
        let second_id = proj.clips[second].id;
        proj.clips.remove(first);
        assert_eq!(proj.clip_index(second_id), Some(0), "found by id after the list shifts");
        assert_eq!(proj.audio(id).unwrap().length(), 2.5);
        let c = proj.clone_audio(i);
        assert_ne!(proj.audio[c].id, id, "a copy gets its own id");
        assert!(proj.clips.iter().all(|cl| cl.audio() == Some(id)), "clips keep their audio");
        // One beat at 120 bpm is half a second.
        proj.bpm = 120.0;
        assert!((proj.seconds_per_tick() * (TICKS_PER_STEP * 4) as f64 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn gestures_that_change_nothing_leave_no_undo_step() {
        let mut proj = Project::default();
        let mut h = History::default();
        h.checkpoint(&proj);
        proj.bpm = 128.0;
        h.checkpoint(&proj); // a click that changed nothing...
        h.checkpoint(&proj); // ...and another
        assert!(h.undo(&mut proj));
        assert_eq!(proj.bpm, 124.0, "one undo goes back past the no-ops");
        assert!(!h.undo(&mut proj));
        assert!(h.redo(&mut proj));
        h.checkpoint(&proj); // a no-op after redo keeps nothing broken
        assert_eq!(proj.bpm, 128.0);
    }

    #[test]
    fn a_pattern_clip_is_a_window_onto_its_pattern_repeated() {
        let mut pat = Pattern::new(1, "p"); // one bar: notes on beats 1 and 3
        pat.notes_mut(9).push(Note { start: 0, len: 24, key: 60, velocity: 1.0 });
        pat.notes_mut(9).push(Note { start: 2 * 96, len: 24, key: 62, velocity: 1.0 });
        let mut c = Clip::of_pattern(1, 0, 0, 2 * TICKS_PER_BAR);
        let at = |c: &Clip| c.pattern_notes(&pat, 9).map(|(t, _, n)| (t, n.key)).collect::<Vec<_>>();
        assert_eq!(at(&c), vec![(0, 60), (192, 62), (384, 60), (576, 62)], "repeats");
        c.offset = 96; // start on beat 2: beat 3's note comes first
        assert_eq!(at(&c), vec![(96, 62), (288, 60), (480, 62), (672, 60)]);
        assert_eq!(c.pattern_restarts(&pat).collect::<Vec<_>>(), vec![288, 672]);
        c.offset = 100 + TICKS_PER_BAR; // offsets wrap around the pattern
        assert_eq!(at(&c)[0], (92, 62));
    }

    #[test]
    fn the_last_tweaked_control_is_found_by_comparing() {
        let mut p = Project::default();
        let ch = p.add_channel("Bass", "lyra.synth", vec![0.0, 1.0, 0.5]);
        p.mixer[2].effects.push(EffectSlot::new("lyra.delay", vec![0.3, 0.5]));
        p.number_effects();
        let fx = p.mixer[2].effects[0].id;
        assert_ne!(fx, 0, "effects get ids");
        let before = p.clone();
        assert_eq!(p.changed_control(&before), None);
        p.channels[0].params[2] = 0.7;
        assert_eq!(p.changed_control(&before), Some(Target::Param { channel: ch, param: 2 }));
        let before = p.clone();
        p.mixer[2].effects[0].params[1] = 0.9;
        assert_eq!(p.changed_control(&before), Some(Target::Effect { effect: fx, param: 1 }));
        // Moving the effect to another slot keeps its id.
        p.mixer[2].effects.insert(0, EffectSlot::new("lyra.eq", vec![]));
        p.number_effects();
        assert_eq!(p.effect_slot(fx), Some((2, 1)));
        let i = p.add_automation("Cutoff", Target::Param { channel: ch, param: 2 }, 0.4, TICKS_PER_BAR);
        assert_eq!(p.automations[i].points.len(), 2);
        assert_eq!(p.automations[i].links[0].travel(0.5), 0.5);
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut proj = Project::default();
        let mut h = History::default();
        h.checkpoint(&proj);
        proj.bpm = 128.0;
        assert!(h.undo(&mut proj));
        assert_eq!(proj.bpm, 124.0);
        assert!(h.redo(&mut proj));
        assert_eq!(proj.bpm, 128.0);
    }
}
