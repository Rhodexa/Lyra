//! The one place where the project (what the user edits) meets the engine
//! (what makes sound).
//!
//! Every UI frame, `sync` compares the project with the copy it last sent and
//! emits only the commands for what changed. Views never send engine commands
//! for edits themselves — they just change the project. One direction, one place.

use crate::samples::Samples;
use lyra_engine::{AudioLane, AudioRegion, ChannelSlot, Command, EngineHandle, FxChain, SeqNote, Sequence};
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
    last_audio: Option<AudioLane>,
    /// Each channel's live state for its editor (see `Plugin::telemetry`),
    /// taken when the bridge makes the plugin.
    live: std::collections::HashMap<lyra_model::ChannelId, Live>,
}

/// A plugin's live state, shared with the audio thread (atomics only).
pub type Live = std::sync::Arc<dyn std::any::Any + Send + Sync>;

impl Bridge {
    /// The live state of a channel's plugin, if it shares one.
    pub fn live(&self, ch: lyra_model::ChannelId) -> Option<Live> {
        self.live.get(&ch).cloned()
    }

    pub fn sync(&mut self, p: &Project, target: Playing, eng: &mut EngineHandle, samples: &mut Samples) {
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
                    || l.channels
                        .iter()
                        .zip(&p.channels)
                        .any(|(a, b)| a.id != b.id || a.instrument != b.instrument || a.sample != b.sample)
            }
        };
        if rebuild {
            eng.send(Command::ClearChannels);
            self.live.clear();
            for ch in &p.channels {
                let mut plugin = lyra_catalog::create_or_silence(&ch.instrument);
                if let Some(live) = plugin.telemetry() {
                    self.live.insert(ch.id, live);
                }
                for (id, v) in ch.params.iter().enumerate() {
                    plugin.set_param(id as u32, *v);
                }
                // Samplers get their sample here, decoded on this (UI) thread.
                if let Some(Ok(data)) = ch.sample.as_deref().map(|name| samples.get(name)) {
                    plugin.set_sample(data);
                }
                eng.send(Command::AddChannel(ChannelSlot::new(plugin, eng.sample_rate())));
            }
        }

        for (i, ch) in p.channels.iter().enumerate() {
            let prev = if rebuild { None } else { last.as_ref().map(|l| &l.channels[i]) };
            if let Some(prev) = prev {
                for (id, v) in ch.params.iter().enumerate() {
                    if prev.params.get(id) != Some(v) {
                        eng.send(Command::SetParam { channel: i, id: id as u32, value: *v });
                    }
                }
            }
            let mix_changed =
                prev.is_none_or(|c| (c.volume, c.pan, c.mute, c.choke) != (ch.volume, ch.pan, ch.mute, ch.choke));
            if mix_changed {
                eng.send(Command::SetMix {
                    channel: i,
                    volume: ch.volume,
                    pan: ch.pan,
                    mute: ch.mute,
                    choke: ch.choke,
                });
            }
        }

        // Routing: which mixer track each channel plays into.
        for (i, ch) in p.channels.iter().enumerate() {
            let prev = if rebuild { None } else { last.as_ref().map(|l| l.channels[i].mixer) };
            if prev != Some(ch.mixer) {
                eng.send(Command::SetRoute { channel: i, track: ch.mixer });
            }
        }

        // Mixer tracks: mix settings, and effect chains (rebuilt only when the
        // list of effects changes; knob moves are plain parameter updates).
        for (t, track) in p.mixer.iter().enumerate() {
            let prev = last.as_ref().and_then(|l| l.mixer.get(t));
            if prev.is_none_or(|pt| (pt.volume, pt.pan, pt.mute) != (track.volume, track.pan, track.mute)) {
                eng.send(Command::SetTrackMix { track: t, volume: track.volume, pan: track.pan, mute: track.mute });
            }
            let same_chain = prev.is_some_and(|pt| {
                pt.effects.len() == track.effects.len()
                    && pt.effects.iter().zip(&track.effects).all(|(a, b)| a.effect == b.effect)
            });
            if !same_chain {
                let slots = track
                    .effects
                    .iter()
                    .filter_map(|slot| {
                        let mut fx = lyra_catalog::effects::create(&slot.effect)?;
                        for (id, v) in slot.params.iter().enumerate() {
                            fx.set_param(id as u32, *v);
                        }
                        Some((fx, slot.bypass))
                    })
                    .collect();
                eng.send(Command::SetFxChain { track: t, chain: FxChain::new(slots, eng.sample_rate()) });
            } else if let Some(pt) = prev {
                for (slot, (a, b)) in pt.effects.iter().zip(&track.effects).enumerate() {
                    if a.bypass != b.bypass {
                        eng.send(Command::SetFxBypass { track: t, slot, bypass: b.bypass });
                    }
                    for (id, v) in b.params.iter().enumerate() {
                        if a.params.get(id) != Some(v) {
                            eng.send(Command::SetFxParam { track: t, slot, id: id as u32, value: *v });
                        }
                    }
                }
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
        // Audio clips only play in song mode (a pattern loops on its own).
        let lane = match target {
            Playing::Song => compile_audio(p, samples),
            Playing::Pattern(_) => AudioLane::default(),
        };
        if self.last_audio.as_ref() != Some(&lane) {
            eng.send(Command::SetAudio(Box::new(lane.clone())));
            self.last_audio = Some(lane);
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
pub fn compile_song(p: &Project) -> Sequence {
    let mut notes = Vec::new();
    for clip in &p.clips {
        if p.tracks.get(clip.track).is_some_and(|t| t.mute) {
            continue;
        }
        let Some(pat) = clip.pattern().and_then(|id| p.pattern(id)) else { continue };
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

/// Every audio clip on the playlist as a region the engine plays. Samples
/// load here (once each) if they haven't yet.
pub fn compile_audio(p: &Project, samples: &mut Samples) -> AudioLane {
    let spt = p.seconds_per_tick();
    let mut regions = Vec::new();
    for clip in &p.clips {
        let Some(track) = p.tracks.get(clip.track) else { continue };
        if track.mute {
            continue;
        }
        let Some(a) = clip.audio().and_then(|id| p.audio(id)) else { continue };
        let Ok(data) = samples.get(&a.sample) else { continue };
        let fade = |f: lyra_model::Fade| (f.len, f.tension);
        regions.push(AudioRegion {
            start: clip.start as f64,
            end: clip.end() as f64,
            from: a.start + clip.offset as f64 * spt,
            trim: (a.start, a.end.min(data.seconds() as f64)),
            // The source's gain and the clip's own, together.
            gain: lyra_plugin::db_to_gain(a.gain + clip.gain),
            fade_in: fade(clip.fade_in),
            fade_out: fade(clip.fade_out),
            track: track.mixer,
            data,
        });
    }
    AudioLane { regions }
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
        p.clips.push(Clip::of_pattern(id, 0, TICKS_PER_BAR, 3 * TICKS_PER_BAR));
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
        p.clips.push(Clip::of_pattern(id, 0, 0, TICKS_PER_BAR / 2));
        p.clips.push(Clip::of_pattern(id, 1, TICKS_PER_BAR, TICKS_PER_BAR));
        p.tracks[1].mute = true;
        let seq = compile_song(&p);
        assert_eq!(seq.notes.len(), 1, "the muted track's clip is skipped");
        assert_eq!(seq.notes[0].len, TICKS_PER_BAR / 2, "notes are cut at the clip's end");
    }

    #[test]
    fn audio_clips_become_regions_on_their_tracks_insert() {
        let mut p = Project::default();
        p.bpm = 120.0; // a tick = 1/192 s
        let kick = "bundled:Starter Kit/Kicks/bd_haus.flac";
        let a = p.add_audio("kick", kick, 0.3);
        p.audio[a].gain = -6.0;
        let id = p.audio[a].id;
        let mut c = Clip::of_audio(id, 2, TICKS_PER_BAR, 48);
        c.offset = 19; // a tenth of a second in
        c.gain = -6.0; // the clip's own gain adds to the source's
        p.clips.push(c);
        p.clips.push(Clip::of_audio(id, 3, 0, 48));
        p.tracks[2].mixer = 5;
        p.tracks[3].mute = true;
        let lane = compile_audio(&p, &mut Samples::default());
        assert_eq!(lane.regions.len(), 1, "the muted track's clip is skipped");
        let r = &lane.regions[0];
        assert_eq!((r.start, r.end, r.track), (TICKS_PER_BAR as f64, (TICKS_PER_BAR + 48) as f64, 5));
        assert!((r.from - 19.0 / 192.0).abs() < 1e-9);
        assert!((r.gain - 0.251).abs() < 0.01, "-6 dB twice, got {}", r.gain);
        assert!(r.trim.1 <= 0.3);
        assert_eq!(r.fade_in, (0.005, 0.0), "new clips fade 5 ms at each end");
    }
}
