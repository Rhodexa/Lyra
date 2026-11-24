//! MIDI files, both ways.
//!
//! **Writing** (Export data, and exporting a pattern or a selection of
//! notes): a type-1 file at 96 ticks per quarter note, which is Lyra's own
//! tick, so no time is rounded. A tempo track, then one plain track per
//! instrument: its name and its notes, no program changes.
//!
//! **Reading** (importing), with `midly` for the file format: every track's
//! notes, split by MIDI channel when a track mixes several (a type-0 file
//! has everything in one track). Times are converted to Lyra ticks. Notes
//! held by the sustain pedal are lengthened to where the pedal lets go,
//! since Lyra has no pedal. Pitch bend and other controllers are left out.

use lyra_model::{Note, TICKS_PER_STEP};

/// Lyra's ticks per quarter note.
pub const PPQ: u32 = TICKS_PER_STEP * 4;

/// One track to write: a name, the MIDI channel (0–15) and the notes.
pub struct TrackOut {
    pub name: String,
    pub channel: u8,
    pub notes: Vec<Note>,
}

/// A standard MIDI file. `length` (ticks) is where the tempo track ends.
pub fn write(title: &str, bpm: f64, length: u32, tracks: &[TrackOut]) -> Vec<u8> {
    let mut chunks = Vec::new();
    // Tempo track: name, tempo, 4/4.
    let mut t0 = Vec::new();
    meta(&mut t0, 0, 0x03, title.as_bytes());
    let us = (60_000_000.0 / bpm.max(1.0)).round() as u32;
    meta(&mut t0, 0, 0x51, &us.to_be_bytes()[1..]);
    meta(&mut t0, 0, 0x58, &[4, 2, 24, 8]);
    let end = tracks.iter().flat_map(|t| &t.notes).map(|n| n.start + n.len).max().unwrap_or(0);
    meta(&mut t0, length.max(end), 0x2F, &[]);
    chunks.push(t0);

    for track in tracks {
        // (tick, is_on, key, velocity): offs sort before ons at the same tick.
        let mut ev: Vec<(u32, bool, u8, u8)> = Vec::new();
        for n in &track.notes {
            let vel = (n.velocity * 127.0).round().clamp(1.0, 127.0) as u8;
            ev.push((n.start, true, n.key.min(127), vel));
            ev.push((n.start + n.len.max(1), false, n.key.min(127), 0));
        }
        ev.sort_by_key(|e| (e.0, e.1, e.2));
        let mut t = Vec::new();
        meta(&mut t, 0, 0x03, track.name.as_bytes());
        let mut last = 0;
        let ch = track.channel & 0x0F;
        for (tick, on, key, vel) in ev {
            varlen(&mut t, tick - last);
            last = tick;
            t.extend_from_slice(&[if on { 0x90 } else { 0x80 } | ch, key, vel]);
        }
        meta(&mut t, 0, 0x2F, &[]);
        chunks.push(t);
    }

    let mut out = b"MThd".to_vec();
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&(chunks.len() as u16).to_be_bytes());
    out.extend_from_slice(&(PPQ as u16).to_be_bytes());
    for t in chunks {
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(t.len() as u32).to_be_bytes());
        out.extend_from_slice(&t);
    }
    out
}

fn meta(t: &mut Vec<u8>, delta: u32, kind: u8, data: &[u8]) {
    varlen(t, delta);
    t.extend_from_slice(&[0xFF, kind]);
    varlen(t, data.len() as u32);
    t.extend_from_slice(data);
}

/// A MIDI variable-length number: 7 bits per byte, high bit = "more follows".
pub fn varlen(t: &mut Vec<u8>, mut v: u32) {
    let mut bytes = [0u8; 5];
    let mut n = 0;
    loop {
        bytes[n] = (v & 0x7F) as u8;
        n += 1;
        v >>= 7;
        if v == 0 {
            break;
        }
    }
    for i in (0..n).rev() {
        t.push(bytes[i] | if i > 0 { 0x80 } else { 0 });
    }
}

/// The notes that start in `from..to` (ticks), moved to start at 0 and cut
/// at `to`: a section of a pattern or a song, ready to write.
pub fn section(notes: &[Note], from: u32, to: u32) -> Vec<Note> {
    let mut out: Vec<Note> = notes
        .iter()
        .filter(|n| (from..to).contains(&n.start))
        .map(|n| Note { start: n.start - from, len: n.len.min(to - n.start).max(1), ..n.clone() })
        .collect();
    out.sort_by_key(|n| (n.start, n.key));
    out
}

/// What an editor hands over to be written as a MIDI file: a suggested
/// file name, the length in ticks, and the tracks.
pub struct Export {
    pub name: String,
    pub length: u32,
    pub tracks: Vec<TrackOut>,
}

impl Export {
    /// One plain track per (name, notes) that has any notes, MIDI channels
    /// in order (no drum channel, no programs).
    pub fn new(name: String, length: u32, parts: impl IntoIterator<Item = (String, Vec<Note>)>) -> Option<Self> {
        let tracks: Vec<TrackOut> = parts
            .into_iter()
            .filter(|(_, notes)| !notes.is_empty())
            .enumerate()
            .map(|(i, (name, notes))| TrackOut { name, channel: (i % 16) as u8, notes })
            .collect();
        (!tracks.is_empty()).then_some(Self { name, length, tracks })
    }
}

/// "bars 3–4" for a span in ticks (bars counted from 1).
pub fn bars_label(from: u32, to: u32) -> String {
    let bar = lyra_model::TICKS_PER_BAR;
    let (a, b) = (from / bar + 1, to.div_ceil(bar).max(from / bar + 1));
    if a == b {
        format!("bar {a}")
    } else {
        format!("bars {a}–{b}")
    }
}

/// A MIDI file read back: its first tempo, and its tracks (one per track
/// and channel that has notes), notes in Lyra ticks.
pub struct Song {
    pub bpm: Option<f64>,
    pub tracks: Vec<TrackIn>,
}

pub struct TrackIn {
    pub name: String,
    pub notes: Vec<Note>,
}

pub fn read(bytes: &[u8]) -> Result<Song, String> {
    use midly::{MetaMessage, MidiMessage, Timing, TrackEventKind};
    let smf = midly::Smf::parse(bytes).map_err(|e| format!("not a MIDI file Lyra can read ({e})"))?;
    let ppq = match smf.header.timing {
        Timing::Metrical(t) => t.as_int().max(1) as u64,
        Timing::Timecode(..) => return Err("this MIDI file counts time in film frames (SMPTE), not beats".into()),
    };
    let to_lyra = |tick: u64| ((tick * PPQ as u64 + ppq / 2) / ppq) as u32;
    let mut bpm = None;
    let mut tracks = Vec::new();
    for (index, events) in smf.tracks.iter().enumerate() {
        let mut name = None;
        let mut now = 0u64;
        // Per channel: notes being held (key, start, velocity), the pedal,
        // notes the pedal holds on, and the finished notes.
        let mut held: Vec<Vec<(u8, u64, u8)>> = vec![Vec::new(); 16];
        let mut pedal = [false; 16];
        let mut sustained: Vec<Vec<(u8, u64, u8)>> = vec![Vec::new(); 16];
        let mut done: Vec<Vec<(u64, u64, u8, u8)>> = vec![Vec::new(); 16];
        for ev in events {
            now += ev.delta.as_int() as u64;
            match ev.kind {
                TrackEventKind::Meta(MetaMessage::TrackName(n)) if name.is_none() => {
                    name = Some(String::from_utf8_lossy(n).trim().to_string()).filter(|n| !n.is_empty());
                }
                TrackEventKind::Meta(MetaMessage::Tempo(us)) if bpm.is_none() => {
                    bpm = Some(60_000_000.0 / us.as_int().max(1) as f64);
                }
                TrackEventKind::Midi { channel, message } => {
                    let c = channel.as_int() as usize;
                    match message {
                        MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                            // Striking a key the pedal holds ends the held one.
                            if let Some(i) = sustained[c].iter().position(|s| s.0 == key.as_int()) {
                                let s = sustained[c].remove(i);
                                done[c].push((s.1, now, s.0, s.2));
                            }
                            held[c].push((key.as_int(), now, vel.as_int()));
                        }
                        MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                            // The oldest note on that key ends (notes on one key can overlap).
                            if let Some(i) = held[c].iter().position(|h| h.0 == key.as_int()) {
                                let h = held[c].remove(i);
                                if pedal[c] {
                                    sustained[c].push(h);
                                } else {
                                    done[c].push((h.1, now, h.0, h.2));
                                }
                            }
                        }
                        MidiMessage::Controller { controller, value } if controller.as_int() == 64 => {
                            pedal[c] = value.as_int() >= 64;
                            if !pedal[c] {
                                for s in sustained[c].drain(..) {
                                    done[c].push((s.1, now, s.0, s.2));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        // Anything still sounding ends with the track.
        for c in 0..16 {
            for h in held[c].drain(..).chain(sustained[c].drain(..)) {
                done[c].push((h.1, now, h.0, h.2));
            }
        }
        let used: Vec<usize> = (0..16).filter(|&c| !done[c].is_empty()).collect();
        for &c in &used {
            let mut notes: Vec<Note> = done[c]
                .iter()
                .map(|&(start, end, key, vel)| {
                    let (s, e) = (to_lyra(start), to_lyra(end));
                    Note { start: s, len: e.saturating_sub(s).max(1), key, velocity: vel as f32 / 127.0 }
                })
                .collect();
            notes.sort_by_key(|n| (n.start, n.key));
            let base = name.clone().unwrap_or_else(|| format!("Track {}", index + 1));
            let name = if used.len() > 1 { format!("{base} · ch {}", c + 1) } else { base };
            tracks.push(TrackIn { name, notes });
        }
    }
    Ok(Song { bpm, tracks })
}

// ── Importing ──

/// What new instruments from a MIDI file play: Lyra Synth, a plain start.
const IMPORT_INSTRUMENT: &str = "lyra.synth";

/// Reads a `.mid` file.
pub fn load(path: &std::path::Path) -> Result<Song, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    read(&bytes)
}

/// The whole file into the song: each track becomes an instrument with a
/// pattern of its notes, placed at tick `at` on its own arrangement track,
/// from `track` down (tracks are added when the song runs out of them).
/// A song with nothing in it yet takes the file's tempo. Returns how many
/// tracks came in.
pub fn import(p: &mut lyra_model::Project, song: &Song, at: u32, track: usize) -> usize {
    use lyra_model::{Clip, Track};
    if p.clips.is_empty() {
        if let Some(bpm) = song.bpm {
            p.bpm = (bpm * 100.0).round() / 100.0;
        }
    }
    for (k, t) in song.tracks.iter().enumerate() {
        let ch = p.add_channel(&t.name, IMPORT_INSTRUMENT, lyra_catalog::default_params(IMPORT_INSTRUMENT));
        let i = p.add_pattern();
        p.patterns[i].name = t.name.clone();
        p.patterns[i].notes_mut(ch).extend(t.notes.iter().cloned());
        let len = p.patterns[i].play_length() * TICKS_PER_STEP;
        let row = track + k;
        while p.tracks.len() <= row {
            let name = format!("Track {}", p.tracks.len() + 1);
            p.tracks.push(Track { name, mute: false });
        }
        // An empty track still called "Track n" takes the part's name.
        let empty = !p.clips.iter().any(|c| c.track == row);
        if empty && p.tracks[row].name == format!("Track {}", row + 1) {
            p.tracks[row].name = t.name.clone();
        }
        let id = p.patterns[i].id;
        p.add_clip(Clip::of_pattern(id, row, at, len));
    }
    song.tracks.len()
}

/// The arrangement track below every track that has clips: where a whole
/// imported file goes when it isn't dropped somewhere.
pub fn free_track(p: &lyra_model::Project) -> usize {
    p.clips.iter().map(|c| c.track + 1).max().unwrap_or(0)
}

/// Every track's notes as one list, moved to start at `at`: a file dropped
/// onto the piano roll goes into the instrument being edited.
pub fn merged(song: &Song, at: u32) -> Vec<Note> {
    let mut notes: Vec<Note> =
        song.tracks.iter().flat_map(|t| &t.notes).map(|n| Note { start: n.start + at, ..n.clone() }).collect();
    notes.sort_by_key(|n| (n.start, n.key));
    notes
}

/// A MIDI file being dragged out of the browser.
#[derive(Clone, Debug)]
pub struct MidiDrag(pub std::path::PathBuf);

pub fn is_midi(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "mid" | "midi"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(start: u32, len: u32, key: u8) -> Note {
        Note { start, len, key, velocity: 100.0 / 127.0 }
    }

    #[test]
    fn varlen_matches_the_spec() {
        for (v, bytes) in [
            (0u32, vec![0x00]),
            (0x7F, vec![0x7F]),
            (0x80, vec![0x81, 0x00]),
            (0x3FFF, vec![0xFF, 0x7F]),
            (0x0FFF_FFFF, vec![0xFF, 0xFF, 0xFF, 0x7F]),
        ] {
            let mut t = Vec::new();
            varlen(&mut t, v);
            assert_eq!(t, bytes, "{v:#x}");
        }
    }

    #[test]
    fn what_lyra_writes_it_reads_back() {
        let lead =
            TrackOut { name: "Lead".into(), channel: 0, notes: vec![n(0, 96, 60), n(96, 48, 64), n(96, 48, 67)] };
        let bass = TrackOut { name: "Bass".into(), channel: 1, notes: vec![n(0, 384, 36)] };
        let song = read(&write("t", 120.0, 384, &[lead, bass])).unwrap();
        assert_eq!(song.bpm.map(|b| b.round()), Some(120.0));
        assert_eq!(song.tracks.len(), 2);
        assert_eq!(song.tracks[0].name, "Lead");
        assert_eq!(song.tracks[0].notes, vec![n(0, 96, 60), n(96, 48, 64), n(96, 48, 67)]);
        assert_eq!(song.tracks[1].notes, vec![n(0, 384, 36)]);
    }

    #[test]
    fn importing_makes_an_instrument_a_pattern_and_a_clip_per_track() {
        let lead = TrackOut { name: "Lead".into(), channel: 0, notes: vec![n(0, 96, 60), n(96, 96, 64)] };
        let bass = TrackOut { name: "Bass".into(), channel: 1, notes: vec![n(0, 768, 36)] };
        let song = read(&write("t", 90.0, 768, &[lead, bass])).unwrap();
        let mut p = lyra_model::Project::default();
        let (channels, patterns) = (p.channels.len(), p.patterns.len());
        let row = free_track(&p);
        assert_eq!(import(&mut p, &song, 0, row), 2);
        assert_eq!(p.bpm, 90.0, "an empty song takes the file's tempo");
        assert_eq!(p.channels.len(), channels + 2);
        assert_eq!(p.patterns.len(), patterns + 2);
        let bass_pattern = p.patterns.last().unwrap();
        assert_eq!(bass_pattern.name, "Bass");
        assert_eq!(p.clips.len(), 2);
        assert_eq!((p.clips[0].track, p.clips[1].track), (0, 1), "a track each");
        assert_eq!(p.tracks[1].name, "Bass", "named after its part");
        assert_eq!(p.clips[1].len, 2 * lyra_model::TICKS_PER_BAR, "as long as its notes");
        // A second file goes below, and keeps the song's tempo.
        p.bpm = 120.0;
        let row = free_track(&p);
        import(&mut p, &song, 0, row);
        assert_eq!(p.clips[2].track, 2);
        assert_eq!(p.bpm, 120.0);
        // Merged for the piano roll: everything, moved.
        let all = merged(&song, 384);
        assert_eq!(all.iter().map(|n| (n.start, n.key)).collect::<Vec<_>>(), vec![(384, 36), (384, 60), (480, 64)]);
    }

    /// A hand-made file at 480 ticks a beat: one track, two channels, the pedal.
    #[test]
    fn other_resolutions_mixed_channels_and_the_pedal() {
        let mut t = Vec::new();
        let ev = |t: &mut Vec<u8>, delta: u32, bytes: &[u8]| {
            varlen(t, delta);
            t.extend_from_slice(bytes);
        };
        ev(&mut t, 0, &[0x90, 60, 100]); // ch 1: C4 at 0
        ev(&mut t, 0, &[0x99, 36, 90]); //  ch 10: kick at 0
        ev(&mut t, 120, &[0x89, 36, 0]); // kick off a sixteenth later
        ev(&mut t, 0, &[0xB0, 64, 127]); // pedal down
        ev(&mut t, 120, &[0x90, 60, 0]); // C4 "off" (vel 0) — the pedal holds it
        ev(&mut t, 720, &[0xB0, 64, 0]); // pedal up two beats in: C4 ends here
        ev(&mut t, 0, &[0xFF, 0x2F, 0]);
        let mut file = b"MThd\0\0\0\x06\0\0\0\x01\x01\xE0MTrk".to_vec(); // type 0, 480 ppq
        file.extend_from_slice(&(t.len() as u32).to_be_bytes());
        file.extend_from_slice(&t);
        let song = read(&file).unwrap();
        assert_eq!(song.tracks.len(), 2, "split by channel");
        assert_eq!(song.tracks[0].notes[0].len, 192, "held by the pedal for two beats");
        assert_eq!(song.tracks[1].notes[0].len, 24, "a sixteenth");
        assert!(song.tracks[1].name.ends_with("ch 10"));
    }
}
