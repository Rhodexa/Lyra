//! The one place where the project (what the user edits) meets the engine
//! (what makes sound).
//!
//! Every UI frame, `sync` compares the project with the copy it last sent and
//! emits only the commands for what changed. Views never send engine commands
//! for edits themselves — they just change the project. One direction, one place.

use lyra_engine::{ChannelSlot, Command, EngineHandle, SeqNote, Sequence};
use lyra_model::{Pattern, Project, TICKS_PER_STEP};

#[derive(Default)]
pub struct Bridge {
    last: Option<Project>,
    last_pattern: usize,
}

impl Bridge {
    pub fn sync(&mut self, p: &Project, pattern: usize, eng: &mut EngineHandle) {
        if self.last.as_ref() == Some(p) && self.last_pattern == pattern {
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
        let seq_changed = rebuild
            || pattern != self.last_pattern
            || l.is_none_or(|l| l.patterns.get(pattern) != p.patterns.get(pattern));
        if seq_changed {
            if let Some(pat) = p.patterns.get(pattern) {
                eng.send(Command::SetSequence(Box::new(compile(p, pat))));
            }
        }

        self.last = Some(p.clone());
        self.last_pattern = pattern;
    }
}

/// Flatten a pattern into the engine's sorted note list.
fn compile(p: &Project, pat: &Pattern) -> Sequence {
    let mut notes = Vec::new();
    for (index, ch) in p.channels.iter().enumerate() {
        for n in pat.notes(ch.id) {
            notes.push(SeqNote {
                start: n.start,
                len: n.len,
                channel: index as u16,
                key: n.key,
                velocity: n.velocity,
            });
        }
    }
    notes.sort_by_key(|n| n.start);
    Sequence { length: pat.play_length() * TICKS_PER_STEP, notes }
}
