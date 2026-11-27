//! The one place where the project (what the user edits) meets the engine
//! (what makes sound).
//!
//! Every UI frame, `sync` compares the project with the copy it last sent and
//! emits only the commands for what changed. Views never send engine commands
//! for edits themselves — they just change the project. One direction, one place.

use crate::samples::Samples;
use lyra_engine::{
    Arrangement, AudioLane, AudioRegion, ChannelSlot, Command, EngineHandle, FxChain, Place, SeqGlide, SeqNote, Sequence,
};
use lyra_model::{Note, Pattern, Project, TICKS_PER_STEP};

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
    last_automation: Option<lyra_engine::Automation>,
    /// The control the last change moved (any editor, any plugin window):
    /// what "Automate" automates. The app takes it after each sync.
    pub tweaked: Option<lyra_model::Target>,
    /// Each channel's live state for its editor (see `Plugin::telemetry`),
    /// taken when the bridge makes the plugin.
    live: std::collections::HashMap<lyra_model::ChannelId, Live>,
    /// Rebuild every channel on the next sync (a sample file changed on disk).
    force: bool,
}

/// A plugin's live state, shared with the audio thread (atomics only).
pub type Live = std::sync::Arc<dyn std::any::Any + Send + Sync>;

impl Bridge {
    /// Remake every channel's plugin on the next sync: a sample file changed,
    /// and plugins only get their sample when they are made.
    pub fn force_rebuild(&mut self) {
        self.force = true;
    }

    /// Every automated control's value at song tick `tick` (from the lanes
    /// last sent to the engine).
    pub fn automated_at(&self, tick: f64) -> Vec<(lyra_engine::Addr, f32)> {
        self.last_automation.as_ref().map_or_else(Vec::new, |a| {
            a.lanes.iter().map(|l| (l.addr, l.info.from_normalized(l.travel_at(tick)))).collect()
        })
    }

    /// The live state of a channel's plugin, if it shares one.
    pub fn live(&self, ch: lyra_model::ChannelId) -> Option<Live> {
        self.live.get(&ch).cloned()
    }

    /// A channel's plugin, made and set up on this (UI) thread.
    fn make_channel(&mut self, ch: &lyra_model::Channel, sample_rate: f32, samples: &mut Samples) -> Box<ChannelSlot> {
        let mut plugin = lyra_catalog::create_or_silence(&ch.instrument);
        match plugin.telemetry() {
            Some(live) => self.live.insert(ch.id, live),
            None => self.live.remove(&ch.id),
        };
        for (id, v) in ch.params.iter().enumerate() {
            plugin.set_param(id as u32, *v);
        }
        // Samplers get their sample here, decoded on this thread.
        if let Some(Ok(data)) = ch.sample.as_deref().map(|name| samples.get(name)) {
            plugin.set_sample(data);
        }
        ChannelSlot::new(plugin, sample_rate)
    }

    pub fn sync(&mut self, p: &Project, target: Playing, eng: &mut EngineHandle, samples: &mut Samples) {
        if !self.force && self.last.as_ref() == Some(p) && self.last_target == Some(target) {
            return; // nothing changed — the common case
        }
        let forced = std::mem::take(&mut self.force);
        let last = self.last.take();
        if let Some(t) = last.as_ref().and_then(|l| p.changed_control(l)) {
            self.tweaked = Some(t);
        }

        // Each channel as the engine has it now: where it sits there (None =
        // it needs a fresh plugin: new, swapped instrument or sample, or forced).
        let kept: Vec<Option<usize>> = p
            .channels
            .iter()
            .map(|ch| {
                let l = last.as_ref().filter(|_| !forced)?;
                let j = l.channels.iter().position(|c| c.id == ch.id)?;
                let old = &l.channels[j];
                (old.instrument == ch.instrument && old.sample == ch.sample).then_some(j)
            })
            .collect();
        let unchanged = last.as_ref().is_some_and(|l| l.channels.len() == p.channels.len())
            && kept.iter().enumerate().all(|(i, k)| *k == Some(i));
        // Any fresh plugin, or channels moved: the per-channel lists (notes,
        // automation) are re-sent, since they address channels by index.
        let rebuild = !unchanged;
        if rebuild {
            let places = p
                .channels
                .iter()
                .zip(&kept)
                .map(|(ch, k)| match k {
                    Some(j) => Place::Keep(*j),
                    None => Place::Fresh(Some(self.make_channel(ch, eng.sample_rate(), samples))),
                })
                .collect();
            eng.send(Command::Arrange(Arrangement::new(places)));
            self.live.retain(|id, _| p.channels.iter().any(|c| c.id == *id));
        }

        for (i, ch) in p.channels.iter().enumerate() {
            // A kept channel compares with itself last frame; a fresh one gets everything.
            let prev = kept[i].and_then(|j| last.as_ref().map(|l| &l.channels[j]));
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
            let prev = kept[i].and_then(|j| last.as_ref().map(|l| l.channels[j].mixer));
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
        // Automation too: song mode only. (Fresh channels don't have their
        // automated values yet, and moved ones changed address: re-send it.)
        let set = match target {
            Playing::Song => crate::automation::compile(p),
            Playing::Pattern(_) => lyra_engine::Automation::default(),
        };
        if rebuild || self.last_automation.as_ref() != Some(&set) {
            eng.send(Command::SetAutomation(Box::new(set.clone())));
            self.last_automation = Some(set);
        }
        if switched {
            eng.send(Command::Seek(0.0));
        }

        self.last = Some(p.clone());
        self.last_target = Some(target);
    }
}

/// A bent note's glide for the engine (`at`: where the note starts in the
/// sequence; `cut`: how much of the note plays, if a clip's end cuts it).
fn glide_of(n: &Note, at: u32, cut: u32, channel: u16) -> Option<SeqGlide> {
    let (start, len, key) = n.glide()?;
    let from = start - n.start;
    (from < cut).then(|| SeqGlide {
        start: at + from,
        len: len.min(cut - from),
        channel,
        note_start: at,
        note_key: n.key,
        key,
        tension: n.bend.map_or(0.0, |b| b.tension),
    })
}

/// Flatten a pattern into the engine's sorted note list.
fn compile(p: &Project, pat: &Pattern) -> Sequence {
    let (mut notes, mut glides) = (Vec::new(), Vec::new());
    for (index, ch) in p.channels.iter().enumerate() {
        for n in pat.notes(ch.id) {
            notes.push(SeqNote { start: n.start, len: n.len, channel: index as u16, key: n.key, velocity: n.velocity });
            glides.extend(glide_of(n, n.start, n.len, index as u16));
        }
    }
    notes.sort_by_key(|n| n.start);
    glides.sort_by_key(|g| g.start);
    Sequence { length: pat.play_length() * TICKS_PER_STEP, notes, glides }
}

/// Flatten the whole playlist: every clip plays its pattern from the clip's
/// start, repeating it to fill the clip and cutting notes at the clip's end.
pub fn compile_song(p: &Project) -> Sequence {
    let (mut notes, mut glides) = (Vec::new(), Vec::new());
    for clip in &p.clips {
        if p.tracks.get(clip.track).is_some_and(|t| t.mute) {
            continue;
        }
        let Some(pat) = clip.pattern().and_then(|id| p.pattern(id)) else { continue };
        for (index, ch) in p.channels.iter().enumerate() {
            for (at, len, n) in clip.pattern_notes(pat, ch.id) {
                let start = clip.start + at;
                notes.push(SeqNote { start, len, channel: index as u16, key: n.key, velocity: n.velocity });
                glides.extend(glide_of(n, start, len, index as u16));
            }
        }
    }
    notes.sort_by_key(|n| n.start);
    glides.sort_by_key(|g| g.start);
    Sequence { length: p.song_length(), notes, glides }
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
        let Some(a) = clip.audio().and_then(|id| p.audio(id)).filter(|a| !a.mute) else { continue };
        let Ok(data) = samples.get(&a.sample) else { continue };
        let fade = |f: lyra_model::Fade| (f.len, f.tension);
        regions.push(AudioRegion {
            start: clip.start as f64,
            end: clip.end() as f64,
            from: a.start + clip.offset as f64 * spt,
            trim: (a.start, a.end.min(data.seconds() as f64)),
            // The source's gain and the clip's own, together.
            // The audio clip's volume (its rack fader) and the placed clip's own gain.
            gain: a.volume * lyra_plugin::db_to_gain(clip.gain),
            pan: a.pan,
            fade_in: fade(clip.fade_in),
            fade_out: fade(clip.fade_out),
            track: a.mixer,
            reverse: clip.reverse,
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
        p.patterns[0].notes_mut(ch).push(Note { start: 0, len: 24, key: 60, velocity: 1.0, bend: None });
        p
    }

    #[test]
    fn adding_an_instrument_lets_the_others_ring_on() {
        let mut p = Project::default();
        let ch = p.add_channel("Pad", "lyra.synth", lyra_catalog::default_params("lyra.synth"));
        p.patterns[0].notes_mut(ch).push(Note { start: 0, len: 4 * TICKS_PER_BAR, key: 60, velocity: 1.0, bend: None });
        let (mut eng, mut h) = lyra_engine::create(48_000.0);
        let (mut bridge, mut samples) = (Bridge::default(), Samples::default());
        let mut out = vec![0.0; 2 * 4800];
        let mut loudness = |eng: &mut lyra_engine::Engine| {
            eng.process(&mut out, 2);
            (out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32).sqrt()
        };
        bridge.sync(&p, Playing::Pattern(0), &mut h, &mut samples);
        h.send(Command::Play);
        for _ in 0..5 {
            loudness(&mut eng); // half a second into the note
        }
        let before = loudness(&mut eng);
        // A new instrument at the top of the rack: the pad moves to index 1.
        let kick = p.add_channel("Kick", "lyra.kick", vec![]);
        let last = p.channels.len() - 1;
        p.channels.swap(0, last);
        bridge.sync(&p, Playing::Pattern(0), &mut h, &mut samples);
        let after = loudness(&mut eng);
        assert!(before > 0.01, "the pad sounds ({before})");
        assert!(after > before * 0.5, "and keeps sounding: {before} → {after}");
        assert_eq!(p.channels[0].id, kick);
        h.collect_garbage();
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
        p.audio[a].volume = 0.5;
        let id = p.audio[a].id;
        let mut c = Clip::of_audio(id, 2, TICKS_PER_BAR, 48);
        c.offset = 19; // a tenth of a second in
        c.gain = -6.0; // the clip's own gain adds to the source's
        p.clips.push(c);
        p.clips.push(Clip::of_audio(id, 3, 0, 48));
        p.audio[a].mixer = 5;
        p.tracks[3].mute = true;
        let lane = compile_audio(&p, &mut Samples::default());
        assert_eq!(lane.regions.len(), 1, "the muted track's clip is skipped");
        let r = &lane.regions[0];
        assert_eq!((r.start, r.end, r.track), (TICKS_PER_BAR as f64, (TICKS_PER_BAR + 48) as f64, 5));
        assert!((r.from - 19.0 / 192.0).abs() < 1e-9);
        assert!((r.gain - 0.251).abs() < 0.01, "half, then -6 dB, got {}", r.gain);
        assert!(r.trim.1 <= 0.3);
        assert_eq!(r.fade_in, (0.005, 0.0), "new clips fade 5 ms at each end");
    }
}

#[cfg(test)]
mod glide_tests {
    use super::*;
    use lyra_model::NoteBend;

    #[test]
    fn a_bent_note_glides_inside_itself_and_a_clip_cuts_it() {
        let bend = NoteBend { semitones: 7, start: 48, arrive: None, tension: 0.5 };
        let n = Note { start: 96, len: 96, key: 60, velocity: 0.8, bend: Some(bend) };
        let g = glide_of(&n, 96, 96, 2).unwrap();
        assert_eq!((g.start, g.len, g.key, g.note_start, g.note_key, g.channel), (144, 48, 67, 96, 60, 2));
        // In song mode at tick 1000, cut after 60 ticks: the glide is cut too.
        let g = glide_of(&n, 1000, 60, 0).unwrap();
        assert_eq!((g.start, g.len), (1048, 12));
        // Cut before the glide starts: no glide.
        assert!(glide_of(&n, 1000, 40, 0).is_none());
        // Arriving early (60 ticks in): the glide is 12 ticks, then it holds.
        let early = Note { bend: Some(NoteBend { arrive: Some(60), ..bend }), ..n };
        let g = glide_of(&early, 96, 96, 0).unwrap();
        assert_eq!((g.start, g.len), (144, 12));
    }
}
