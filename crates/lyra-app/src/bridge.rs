//! The one place where the project (what the user edits) meets the engine
//! (what makes sound).
//!
//! Every UI frame, `sync` compares the project with the copy it last sent and
//! emits only the commands for what changed. Views never send engine commands
//! for edits themselves — they just change the project. One direction, one place.

use lyra_engine::{ChannelSlot, Command, EngineHandle, SeqNote, Sequence};
use lyra_model::{Pattern, Project, TICKS_PER_STEP};

/// What the engine should loop: one pattern, or the whole arrangement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Playing {
    Pattern(usize),
    Song,
}

#[derive(Default)]
pub struct Bridge {
    last: Option<Project>,
    last_target: Option<Playing>,
    /// The sequence the engine has now — compiled again on every change and
    /// only re-sent when it actually differs.
    last_seq: Option<Sequence>,
}

impl Bridge {
    pub fn sync(&mut self, p: &Project, target: Playing, eng: &mut EngineHandle) {
        if self.last.as_ref() == Some(p) && self.last_target == Some(target) {
            return; // nothing changed — the common case
        }
        let last = self.last.take();

        // Channels added/removed/reordered or instrument swapped → rebuild them all.
        // (Simple and correct; fine-grained insert/remove can come later.)
        let rebuild = match &last {
            None => true,
            Some(l) => {
                l.channels.len() != p.channels.len()
                    || l.channels.iter().zip(&p.channels).any(|(a, b)| a.id != b.id || a.instrument != b.instrument)
            }
        };
        if rebuild {
            eng.send(Command::ClearChannels);
            for ch in &p.channels {
                let mut plugin = lyra_dsp::create_or_silence(&ch.instrument);
                for (id, v) in ch.params.iter().enumerate() {
                    plugin.set_param(id as u32, *v);
                }
                eng.send(Command::AddChannel(ChannelSlot::new(plugin, eng.sample_rate())));
            }
        }

        let any_solo = p.any_solo();
        let solo_changed = last.as_ref().is_none_or(|l| l.any_solo() != any_solo);
        for (i, ch) in p.channels.iter().enumerate() {
            let prev = if rebuild { None } else { last.as_ref().map(|l| &l.channels[i]) };
            if let Some(prev) = prev {
                for (id, v) in ch.params.iter().enumerate() {
                    if prev.params.get(id) != Some(v) {
                        eng.send(Command::SetParam { channel: i, id: id as u32, value: *v });
                    }
                }
            }
            let mix_changed = solo_changed
                || prev.is_none_or(|c| {
                    (c.volume, c.pan, c.mute, c.solo, c.choke) != (ch.volume, ch.pan, ch.mute, ch.solo, ch.choke)
                });
            if mix_changed {
                eng.send(Command::SetMix {
                    channel: i,
                    volume: ch.volume,
                    pan: ch.pan,
                    mute: ch.mute || (any_solo && !ch.solo),
                    choke: ch.choke,
                });
            }
        }

        let l = last.as_ref();
        if l.is_none_or(|l| l.bpm != p.bpm) {
            eng.send(Command::SetTempo(p.bpm));
        }
        if l.is_none_or(|l| l.swing != p.swing) {
            eng.send(Command::SetSwing(p.swing));
        }
        if l.is_none_or(|l| l.master_volume != p.master_volume) {
            eng.send(Command::SetMasterVolume(p.master_volume));
        }
        // Song mode → seek to the top when the mode flips, so you hear it from bar 1.
        let switched = self.last_target.is_some_and(|t| std::mem::discriminant(&t) != std::mem::discriminant(&target));
        let seq = match target {
            Playing::Pattern(i) => p.patterns.get(i).map(|pat| compile(p, pat)),
            Playing::Song => Some(compile_song(p)),
        };
        if let Some(seq) = seq {
            if rebuild || self.last_seq.as_ref() != Some(&seq) {
                eng.send(Command::SetSequence(Box::new(seq.clone())));
                self.last_seq = Some(seq);
            }
        }
        if switched {
            eng.send(Command::Seek(0.0));
        }

        self.last = Some(p.clone());
        self.last_target = Some(target);
    }
}

/// Flatten a pattern into the engine's sorted note list.
fn compile(p: &Project, pat: &Pattern) -> Sequence {
    let mut notes = Vec::new();
    for (index, ch) in p.channels.iter().enumerate() {
        for n in pat.notes(ch.id) {
            notes.push(SeqNote { start: n.start, len: n.len, channel: index as u16, key: n.key, velocity: n.velocity });
        }
    }
    notes.sort_by_key(|n| n.start);
    Sequence { length: pat.play_length() * TICKS_PER_STEP, notes }
}

/// Flatten the whole playlist: every clip plays its pattern from the clip's
/// start, repeating it to fill the clip and cutting notes at the clip's end.
fn compile_song(p: &Project) -> Sequence {
    let mut notes = Vec::new();
    for clip in &p.clips {
        if p.tracks.get(clip.track).is_some_and(|t| t.mute) {
            continue;
        }
        let Some(pat) = p.pattern(clip.pattern) else { continue };
        let loop_len = pat.play_length() * TICKS_PER_STEP;
        for (index, ch) in p.channels.iter().enumerate() {
            for rep in (0..clip.len).step_by(loop_len.max(1) as usize) {
                for n in pat.notes(ch.id) {
                    let at = rep + n.start;
                    if at >= clip.len || n.start >= loop_len {
                        continue;
                    }
                    notes.push(SeqNote {
                        start: clip.start + at,
                        len: n.len.min(clip.len - at),
                        channel: index as u16,
                        key: n.key,
                        velocity: n.velocity,
                    });
                }
            }
        }
    }
    notes.sort_by_key(|n| n.start);
    Sequence { length: p.song_length(), notes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyra_model::{Clip, Note, TICKS_PER_BAR};

    fn project_with_one_note() -> Project {
        let mut p = Project::default();
        let ch = p.add_channel("k", "lyra.kick", vec![]);
        p.patterns[0].notes_mut(ch).push(Note { start: 0, len: 24, key: 60, velocity: 1.0 });
        p
    }

    #[test]
    fn a_long_clip_repeats_its_pattern() {
        let mut p = project_with_one_note();
        let id = p.patterns[0].id;
        p.clips.push(Clip { pattern: id, track: 0, start: TICKS_PER_BAR, len: 3 * TICKS_PER_BAR });
        let seq = compile_song(&p);
        let starts: Vec<u32> = seq.notes.iter().map(|n| n.start).collect();
        assert_eq!(starts, vec![TICKS_PER_BAR, 2 * TICKS_PER_BAR, 3 * TICKS_PER_BAR]);
        assert_eq!(seq.length, 4 * TICKS_PER_BAR);
    }

    #[test]
    fn muted_tracks_and_clip_ends_are_respected() {
        let mut p = project_with_one_note();
        let id = p.patterns[0].id;
        p.patterns[0].notes.values_mut().next().unwrap()[0].len = 2 * TICKS_PER_BAR; // a long note
        p.clips.push(Clip { pattern: id, track: 0, start: 0, len: TICKS_PER_BAR / 2 });
        p.clips.push(Clip { pattern: id, track: 1, start: TICKS_PER_BAR, len: TICKS_PER_BAR });
        p.tracks[1].mute = true;
        let seq = compile_song(&p);
        assert_eq!(seq.notes.len(), 1, "the muted track's clip is skipped");
        assert_eq!(seq.notes[0].len, TICKS_PER_BAR / 2, "notes are cut at the clip's end");
    }
}
