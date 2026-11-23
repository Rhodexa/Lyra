//! Standard MIDI files → one melodic line for Octavia.
//!
//! A small reader (formats 0 and 1, tempo changes, running status) with no
//! dependencies. MIDI carries no articulation, so the notes come out bare
//! and the performer infers how to play them, like from a piano roll.

use octavia_core::score::{velocity_to_dynamic, Articulation, Note, Score};

#[derive(Clone, Debug, PartialEq)]
pub struct MidiNote {
    pub track: usize,
    pub channel: u8,
    pub key: u8,
    pub velocity: u8,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Default)]
pub struct MidiFile {
    pub notes: Vec<MidiNote>,
    /// Track names (empty when unnamed), by track index.
    pub names: Vec<String>,
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, String> {
        let v = *self.b.get(self.pos).ok_or("unexpected end of file")?;
        self.pos += 1;
        Ok(v)
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.b.get(self.pos..self.pos + n).ok_or("unexpected end of file")?;
        self.pos += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn vlq(&mut self) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..4 {
            let b = self.u8()?;
            v = (v << 7) | (b & 0x7F) as u32;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err("bad variable-length number".into())
    }
}

pub fn parse(bytes: &[u8]) -> Result<MidiFile, String> {
    let mut r = Reader { b: bytes, pos: 0 };
    if r.bytes(4)? != b"MThd" {
        return Err("not a MIDI file".into());
    }
    let hlen = r.u32()? as usize;
    let _format = r.u16()?;
    let ntracks = r.u16()? as usize;
    let division = r.u16()?;
    if division & 0x8000 != 0 {
        return Err("SMPTE-timed MIDI files aren't supported".into());
    }
    let ppq = division.max(1) as f64;
    r.bytes(hlen.saturating_sub(6))?;

    // (tick, track, channel, key, velocity or 0 for off)
    let mut events: Vec<(u64, usize, u8, u8, u8)> = Vec::new();
    let mut tempos: Vec<(u64, f64)> = vec![(0, 500_000.0)];
    let mut names = vec![String::new(); ntracks];

    for track in 0..ntracks {
        // Skip unknown chunks until a track.
        loop {
            let id = r.bytes(4)?;
            let len = r.u32()? as usize;
            if id == b"MTrk" {
                let data = r.bytes(len)?;
                read_track(data, track, &mut events, &mut tempos, &mut names[track])?;
                break;
            }
            r.bytes(len)?;
        }
    }

    tempos.sort_by_key(|t| t.0);
    let seconds = |tick: u64| -> f64 {
        let (mut secs, mut last_tick, mut us) = (0.0, 0u64, 500_000.0);
        for &(t, u) in &tempos {
            if t >= tick {
                break;
            }
            secs += (t - last_tick) as f64 / ppq * us / 1e6;
            last_tick = t;
            us = u;
        }
        secs + (tick - last_tick) as f64 / ppq * us / 1e6
    };

    // Pair note-ons with note-offs, first in first out per key.
    events.sort_by_key(|e| (e.0, e.4 != 0)); // offs before ons at the same tick
    let mut open: Vec<(usize, u8, u8, u8, u64)> = Vec::new();
    let mut notes = Vec::new();
    for (tick, track, ch, key, vel) in events {
        if vel > 0 {
            open.push((track, ch, key, vel, tick));
        } else if let Some(i) = open.iter().position(|o| o.0 == track && o.1 == ch && o.2 == key) {
            let (_, _, _, v, on) = open.remove(i);
            notes.push(MidiNote { track, channel: ch, key, velocity: v, start: seconds(on), end: seconds(tick) });
        }
    }
    notes.sort_by(|a, b| a.start.total_cmp(&b.start).then(b.key.cmp(&a.key)));
    Ok(MidiFile { notes, names })
}

fn read_track(
    data: &[u8],
    track: usize,
    events: &mut Vec<(u64, usize, u8, u8, u8)>,
    tempos: &mut Vec<(u64, f64)>,
    name: &mut String,
) -> Result<(), String> {
    let mut r = Reader { b: data, pos: 0 };
    let mut tick = 0u64;
    let mut status = 0u8;
    while r.pos < data.len() {
        tick += r.vlq()? as u64;
        let mut b = r.u8()?;
        if b == 0xFF {
            let kind = r.u8()?;
            let len = r.vlq()? as usize;
            let body = r.bytes(len)?;
            match kind {
                0x51 if len == 3 => tempos.push((tick, ((body[0] as u32) << 16 | (body[1] as u32) << 8 | body[2] as u32) as f64)),
                0x03 => *name = String::from_utf8_lossy(body).trim().to_string(),
                0x2F => break,
                _ => {}
            }
            continue;
        }
        if b == 0xF0 || b == 0xF7 {
            let len = r.vlq()? as usize;
            r.bytes(len)?;
            continue;
        }
        let first;
        if b & 0x80 != 0 {
            status = b;
            first = r.u8()?;
        } else {
            first = b; // running status
            b = status;
        }
        let ch = b & 0x0F;
        match b & 0xF0 {
            0x80 => {
                r.u8()?;
                events.push((tick, track, ch, first, 0));
            }
            0x90 => {
                let vel = r.u8()?;
                events.push((tick, track, ch, first, vel));
            }
            0xA0 | 0xB0 | 0xE0 => {
                r.u8()?;
            }
            0xC0 | 0xD0 => {}
            _ => return Err(format!("unexpected MIDI byte {b:#04x}")),
        }
    }
    Ok(())
}

/// Which note of a chord to keep.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Voice {
    Top,
    Bottom,
}

pub struct Pick {
    pub track: Option<usize>,
    pub channel: Option<u8>,
    pub voice: Voice,
    /// Semitones; `None` = shift by octaves to fit `range`.
    pub transpose: Option<i32>,
    /// Playable range (MIDI notes), lowest open string to a comfortable top.
    pub range: (f32, f32),
}

/// Turns the chosen notes into one line the performer can play.
/// Returns the score and a short description of what was done.
pub fn to_line(m: &MidiFile, pick: &Pick) -> Result<(Score, String), String> {
    let track = match pick.track {
        Some(t) => t,
        // The busiest track.
        None => {
            let mut counts = vec![0usize; m.names.len().max(1)];
            for n in &m.notes {
                counts[n.track] += 1;
            }
            counts.iter().enumerate().max_by_key(|(_, c)| **c).map(|(i, _)| i).unwrap_or(0)
        }
    };
    let chosen: Vec<&MidiNote> =
        m.notes.iter().filter(|n| n.track == track && pick.channel.is_none_or(|c| n.channel == c)).collect();
    if chosen.is_empty() {
        return Err(format!("track {track} has no notes (see `octavia midi-info`)"));
    }

    // One note per onset: chords collapse to their top (or bottom) note.
    let mut line: Vec<MidiNote> = Vec::new();
    for n in chosen {
        match line.last_mut() {
            Some(last) if (n.start - last.start).abs() < 0.02 => {
                let better = match pick.voice {
                    Voice::Top => n.key > last.key,
                    Voice::Bottom => n.key < last.key,
                };
                if better {
                    *last = n.clone();
                }
            }
            _ => line.push(n.clone()),
        }
    }

    // Overlaps: a small one is drawn legato (a slur hint for the performer);
    // a big one is another voice sustaining, so cut it.
    let mut notes = Vec::with_capacity(line.len());
    for (i, n) in line.iter().enumerate() {
        let mut end = n.end;
        let mut slur = None;
        if let Some(next) = line.get(i + 1) {
            if end > next.start {
                if end - next.start <= 0.15 {
                    slur = Some(true);
                } else {
                    end = next.start;
                }
            }
        }
        let dynamic = velocity_to_dynamic(n.velocity as f32 / 127.0);
        notes.push(Note {
            start: n.start,
            dur: (end - n.start).max(0.03),
            pitch: n.key as f32,
            dynamic,
            dynamic_end: dynamic,
            articulation: Articulation::Auto,
            slur,
        });
    }

    // Fit the range: shift by octaves so the line's middle sits well, then
    // fold any stragglers up or down an octave.
    let (lo, hi) = pick.range;
    let shift = match pick.transpose {
        Some(t) => t as f32,
        None => {
            let mut pitches: Vec<f32> = notes.iter().map(|n| n.pitch).collect();
            pitches.sort_by(f32::total_cmp);
            let median = pitches[pitches.len() / 2];
            let target = lo + (hi - lo) * 0.4;
            12.0 * ((target - median) / 12.0).round()
        }
    };
    let mut folded = 0;
    for n in &mut notes {
        n.pitch += shift;
        while n.pitch < lo {
            n.pitch += 12.0;
            folded += 1;
        }
        while n.pitch > hi && pick.transpose.is_none() {
            n.pitch -= 12.0;
            folded += 1;
        }
    }
    let name = m.names.get(track).filter(|s| !s.is_empty()).map_or(String::new(), |s| format!(" \"{s}\""));
    let note = format!(
        "track {track}{name}: {} notes, {:+} semitones{}",
        notes.len(),
        shift as i32,
        if folded > 0 { format!(", {folded} folded by an octave") } else { String::new() }
    );
    let mut score = Score { notes };
    score.tidy();
    Ok((score, note))
}

/// One line per track: name, channels, note count, range, how chordal.
pub fn describe(m: &MidiFile) -> String {
    let mut s = String::new();
    for (t, name) in m.names.iter().enumerate() {
        let notes: Vec<&MidiNote> = m.notes.iter().filter(|n| n.track == t).collect();
        if notes.is_empty() {
            continue;
        }
        let lo = notes.iter().map(|n| n.key).min().unwrap_or(0);
        let hi = notes.iter().map(|n| n.key).max().unwrap_or(0);
        let mut chans: Vec<u8> = notes.iter().map(|n| n.channel + 1).collect();
        chans.sort();
        chans.dedup();
        let onsets = {
            let mut o: Vec<i64> = notes.iter().map(|n| (n.start * 50.0) as i64).collect();
            o.dedup();
            o.len()
        };
        let chordal = 100 - onsets * 100 / notes.len();
        s += &format!(
            "  track {t:>2}  {:<24} ch {:?}  {:>4} notes  {}–{}  {}% in chords\n",
            if name.is_empty() { "(unnamed)" } else { name },
            chans,
            notes.len(),
            octavia_core::score::pitch_name(lo as f32),
            octavia_core::score::pitch_name(hi as f32),
            chordal
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vlq(mut v: u32) -> Vec<u8> {
        let mut out = vec![(v & 0x7F) as u8];
        v >>= 7;
        while v > 0 {
            out.insert(0, (v & 0x7F) as u8 | 0x80);
            v >>= 7;
        }
        out
    }

    fn chunk(id: &[u8], body: &[u8]) -> Vec<u8> {
        let mut c = id.to_vec();
        c.extend((body.len() as u32).to_be_bytes());
        c.extend(body);
        c
    }

    /// Format 1: a tempo track (120 bpm, then 60 bpm at beat 2) and a
    /// melody with running status and a C-major chord.
    fn sample() -> Vec<u8> {
        let mut file = chunk(b"MThd", &[0, 1, 0, 2, 0x01, 0xE0]); // 480 ppq
        let mut t0 = Vec::new();
        t0.extend(vlq(0));
        t0.extend([0xFF, 0x51, 3, 0x07, 0xA1, 0x20]); // 500000 us
        t0.extend(vlq(960));
        t0.extend([0xFF, 0x51, 3, 0x0F, 0x42, 0x40]); // 1000000 us
        t0.extend(vlq(0));
        t0.extend([0xFF, 0x2F, 0]);
        file.extend(chunk(b"MTrk", &t0));
        let mut t1 = Vec::new();
        t1.extend(vlq(0));
        t1.extend([0xFF, 0x03, 5]);
        t1.extend(b"Tune!");
        t1.extend(vlq(0));
        t1.extend([0x90, 60, 100]); // C4 on
        t1.extend(vlq(480));
        t1.extend([62, 90]); // running status: D4 on…
        t1.extend(vlq(0));
        t1.extend([60, 0]); // …C4 off (velocity 0)
        t1.extend(vlq(480));
        t1.extend([62, 0]);
        t1.extend(vlq(0));
        t1.extend([64, 80, 0, 67, 80, 0, 72, 80]); // a chord at beat 2
        t1.extend(vlq(480));
        t1.extend([0x80, 64, 0, 0, 67, 0, 0, 72, 0]);
        t1.extend(vlq(0));
        t1.extend([0xFF, 0x2F, 0]);
        file.extend(chunk(b"MTrk", &t1));
        file
    }

    #[test]
    fn reads_notes_tempo_changes_and_names() {
        let m = parse(&sample()).unwrap();
        assert_eq!(m.names[1], "Tune!");
        assert_eq!(m.notes.len(), 5);
        let c = &m.notes[0];
        assert_eq!((c.key, c.velocity), (60, 100));
        assert!((c.end - 0.5).abs() < 1e-9, "a beat at 120 bpm");
        // The chord starts at beat 2 (1.0 s) and lasts a beat at 60 bpm.
        let top = m.notes.iter().find(|n| n.key == 72).unwrap();
        assert!((top.start - 1.0).abs() < 1e-9 && (top.end - 2.0).abs() < 1e-9);
    }

    #[test]
    fn makes_one_fitted_line() {
        let m = parse(&sample()).unwrap();
        let pick = Pick { track: None, channel: None, voice: Voice::Top, transpose: None, range: (36.0, 76.0) };
        let (score, _) = to_line(&m, &pick).unwrap();
        let keys: Vec<f32> = score.notes.iter().map(|n| n.pitch).collect();
        // C4 D4 C5 (chord top), shifted down an octave toward the cello's middle.
        assert_eq!(keys, [48.0, 50.0, 60.0]);
        let bottom = Pick { voice: Voice::Bottom, transpose: Some(0), ..pick };
        let (score, _) = to_line(&m, &bottom).unwrap();
        assert_eq!(score.notes[2].pitch, 64.0);
    }
}
