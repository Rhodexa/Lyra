//! # lyra-model
//!
//! What a Lyra project *is*: channels, patterns, notes. Plain data — no audio,
//! no UI. The UI edits this; a small bridge compiles it into what the engine plays.
//!
//! FL-style idea worth knowing: **a step IS a note.** Lighting a step in the
//! channel rack just adds a one-step note at the default key. The piano roll
//! (milestone 2) edits the very same notes, so the two views never disagree.

pub mod theory;

use std::collections::BTreeMap;
pub use theory::{Chord, Scale};

/// Musical time unit. Must equal `lyra_engine::TICKS_PER_STEP` (checked in the app).
pub const TICKS_PER_STEP: u32 = 24;
pub const STEPS_PER_BAR: u32 = 16;
/// Steps are placed at this key; drums play at their natural pitch here.
pub const DEFAULT_KEY: u8 = 60;
pub const TICKS_PER_BAR: u32 = TICKS_PER_STEP * STEPS_PER_BAR;

pub type ChannelId = u32;

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// In ticks from the pattern start.
    pub start: u32,
    pub len: u32,
    pub key: u8,
    pub velocity: f32,
}

#[derive(Clone, Debug, PartialEq)]
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
    pub mute: bool,
    pub solo: bool,
    /// 0 = none. Channels in the same group cut each other off.
    pub choke: u8,
    /// Index into the theme's channel palette.
    pub color: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub name: String,
    /// The shortest this pattern may be, in steps (16 = one bar). The pattern
    /// grows by itself to fit its notes — see [`Pattern::play_length`].
    pub min_length: u32,
    pub notes: BTreeMap<ChannelId, Vec<Note>>,
}

impl Pattern {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), min_length: STEPS_PER_BAR, notes: BTreeMap::new() }
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
        // A note "fills" the bar it starts in; a long tail alone doesn't add a bar.
        let last_bar = self.notes.values().flatten().map(|n| n.start / TICKS_PER_BAR + 1).max().unwrap_or(1);
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
            notes.push(Note {
                start: step * TICKS_PER_STEP,
                len: TICKS_PER_STEP,
                key: DEFAULT_KEY,
                velocity: 0.8,
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub bpm: f64,
    pub swing: f32,
    pub master_volume: f32,
    /// Key of the song (0 = C … 11 = B) and its scale: drives piano-roll help.
    pub key_root: u8,
    pub scale: Scale,
    pub channels: Vec<Channel>,
    pub patterns: Vec<Pattern>,
    next_id: ChannelId,
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
            patterns: vec![Pattern::new("Pattern 1")],
            next_id: 1,
        }
    }
}

impl Project {
    pub fn add_channel(&mut self, name: &str, instrument: &str, params: Vec<f32>) -> ChannelId {
        let id = self.next_id;
        self.next_id += 1;
        let color = self.channels.len();
        self.channels.push(Channel {
            id,
            name: name.to_string(),
            instrument: instrument.to_string(),
            params,
            volume: 0.8,
            pan: 0.0,
            mute: false,
            solo: false,
            choke: 0,
            color,
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
        self.patterns.push(Pattern::new(&name));
        self.patterns.len() - 1
    }

    /// Duplicates a pattern (notes and length) right after itself; returns the new index.
    pub fn clone_pattern(&mut self, index: usize) -> usize {
        let mut copy = self.patterns[index].clone();
        copy.name = format!("{} copy", copy.name);
        self.patterns.insert(index + 1, copy);
        index + 1
    }

    /// True when any channel is soloed — then only soloed channels are heard.
    pub fn any_solo(&self) -> bool {
        self.channels.iter().any(|c| c.solo)
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

    /// Call *before* changing the project.
    pub fn checkpoint(&mut self, project: &Project) {
        self.past.push(project.clone());
        if self.past.len() > Self::LIMIT {
            self.past.remove(0);
        }
        self.future.clear();
    }

    pub fn undo(&mut self, project: &mut Project) -> bool {
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
        let mut p = Pattern::new("t");
        assert!(p.set_step(7, 4, true, 0.8));
        assert!(!p.set_step(7, 4, true, 0.8), "lighting twice changes nothing");
        assert_eq!(p.notes(7)[0].start, 4 * TICKS_PER_STEP);
        assert!(p.set_step(7, 4, false, 0.0));
        assert!(p.notes(7).is_empty());
    }

    #[test]
    fn patterns_grow_and_shrink_with_their_notes() {
        let mut p = Pattern::new("t");
        assert_eq!(p.play_length(), 16, "empty = one bar");
        p.notes_mut(1).push(Note { start: 2 * TICKS_PER_BAR + 5, len: 10, key: 60, velocity: 1.0 });
        assert_eq!(p.play_length(), 48, "a note in bar 3 makes it 3 bars");
        p.notes_mut(1).clear();
        assert_eq!(p.play_length(), 16, "and it shrinks back");
        p.min_length = 32;
        assert_eq!(p.play_length(), 32, "min_length is a floor");
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
