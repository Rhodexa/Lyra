//! What the performer reads: notes with articulations, in seconds.
//!
//! Also a small text format for writing test phrases by hand:
//!
//! ```text
//! # comment
//! tempo 66
//! mp
//! (G2:e D3 B3) A3 B3 D3 B3 D3     # ( ) = slur, durations are sticky
//! < ^C4:q 'D4 -E4 f               # < or > starts a hairpin, ending at the next dynamic
//! pizz G2:q r D3 arco
//! ```
//!
//! Durations: `w h q e s t` (whole … 32nd), `3` after the letter for a
//! triplet, dots for dotted. Marks before a note: `^` accent, `'` staccato,
//! `-` tenuto. Dynamics: `pp p mp mf f ff`. `r` is a rest.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Articulation {
    /// Ordinary separate bow (the default).
    Detache,
    Staccato,
    Accent,
    Tenuto,
    Pizzicato,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// Seconds.
    pub start: f64,
    /// Seconds.
    pub dur: f64,
    /// MIDI note number (60 = C4).
    pub pitch: f32,
    /// 0..1 (pp ≈ 0.2, ff ≈ 0.86) at the start of the note…
    pub dynamic: f32,
    /// …and at its end (different under a hairpin).
    pub dynamic_end: f32,
    pub articulation: Articulation,
    /// Slurred into the next note (same bow, no re-attack).
    pub slur: bool,
}

impl Note {
    pub fn end(&self) -> f64 {
        self.start + self.dur
    }
}

/// One line of melody: notes sorted by start, not overlapping (Octavia plays
/// one string at a time for now).
#[derive(Clone, Debug, Default)]
pub struct Score {
    pub notes: Vec<Note>,
}

impl Score {
    pub fn end(&self) -> f64 {
        self.notes.iter().map(Note::end).fold(0.0, f64::max)
    }

    /// Sorts, and trims overlaps (an overlapping note means "legato").
    pub fn tidy(&mut self) {
        self.notes.sort_by(|a, b| a.start.total_cmp(&b.start));
        for i in 1..self.notes.len() {
            let start = self.notes[i].start;
            let prev = &mut self.notes[i - 1];
            if prev.end() > start {
                prev.dur = (start - prev.start).max(0.01);
                prev.slur = true;
            }
        }
    }

    pub fn parse(text: &str) -> Result<Score, String> {
        let mut notes: Vec<Note> = Vec::new();
        let mut beat = 60.0 / 90.0;
        let mut time = 0.0f64;
        let mut beats = 1.0f64;
        let mut dynamic = dyn_value("mf").unwrap();
        let mut pizz = false;
        let mut in_slur = false;
        let mut hairpin: Option<(usize, f32)> = None;

        for (ln, line) in text.lines().enumerate() {
            // A comment is a `#` at the start or after a space (`C#4` is a note).
            let cut = line
                .char_indices()
                .find(|&(i, c)| c == '#' && (i == 0 || line[..i].ends_with(char::is_whitespace)))
                .map_or(line.len(), |(i, _)| i);
            let line = &line[..cut];
            let mut tokens = line.split_whitespace();
            while let Some(tok) = tokens.next() {
                let err = |m: &str| format!("line {}: {m} ({tok:?})", ln + 1);
                if tok == "tempo" {
                    let bpm: f64 = tokens.next().and_then(|t| t.parse().ok()).ok_or_else(|| err("tempo needs a number"))?;
                    beat = 60.0 / bpm.max(1.0);
                    continue;
                }
                if tok == "pizz" || tok == "arco" {
                    pizz = tok == "pizz";
                    continue;
                }
                if tok == "<" || tok == ">" {
                    hairpin = Some((notes.len(), dynamic));
                    continue;
                }
                if let Some(d) = dyn_value(tok) {
                    if let Some((from, d0)) = hairpin.take() {
                        // Interpolate by time, so a hairpin can live inside one long note.
                        if let (Some(first), Some(last)) = (notes.get(from), notes.last()) {
                            let (t0, t1) = (first.start, last.end().max(first.start + 1e-6));
                            let at = |t: f64| d0 + (d - d0) * ((t - t0) / (t1 - t0)) as f32;
                            for n in &mut notes[from..] {
                                n.dynamic = at(n.start);
                                n.dynamic_end = at(n.end());
                            }
                        }
                    }
                    dynamic = d;
                    continue;
                }
                let mut t = tok;
                if let Some(rest) = t.strip_prefix('(') {
                    in_slur = true;
                    t = rest;
                }
                let mut close = false;
                if let Some(rest) = t.strip_suffix(')') {
                    close = true;
                    t = rest;
                }
                if t.is_empty() {
                    if close {
                        end_slur(&mut notes, &mut in_slur);
                    }
                    continue;
                }
                let mut art = if pizz { Articulation::Pizzicato } else { Articulation::Detache };
                loop {
                    if let Some(r) = t.strip_prefix('^') {
                        art = Articulation::Accent;
                        t = r;
                    } else if let Some(r) = t.strip_prefix('\'') {
                        art = Articulation::Staccato;
                        t = r;
                    } else if let Some(r) = t.strip_prefix('-') {
                        art = Articulation::Tenuto;
                        t = r;
                    } else {
                        break;
                    }
                }
                if pizz {
                    art = Articulation::Pizzicato;
                }
                let (name, dur) = match t.split_once(':') {
                    Some((n, d)) => (n, Some(d)),
                    None => (t, None),
                };
                if let Some(d) = dur {
                    beats = parse_duration(d).ok_or_else(|| err("bad duration"))?;
                }
                let secs = beats * beat;
                if name == "r" {
                    time += secs;
                } else {
                    let pitch = parse_pitch(name).ok_or_else(|| err("bad note name"))?;
                    notes.push(Note {
                        start: time,
                        dur: secs,
                        pitch,
                        dynamic,
                        dynamic_end: dynamic,
                        articulation: art,
                        slur: in_slur,
                    });
                    time += secs;
                }
                if close {
                    end_slur(&mut notes, &mut in_slur);
                }
            }
        }
        let mut s = Score { notes };
        s.tidy();
        Ok(s)
    }
}

fn end_slur(notes: &mut [Note], in_slur: &mut bool) {
    if let Some(n) = notes.last_mut() {
        n.slur = false;
    }
    *in_slur = false;
}

fn dyn_value(tok: &str) -> Option<f32> {
    Some(match tok {
        "pp" => 0.2,
        "p" => 0.32,
        "mp" => 0.45,
        "mf" => 0.58,
        "f" => 0.72,
        "ff" => 0.86,
        _ => return None,
    })
}

/// `q`, `e.`, `e3`, `h..` → beats.
fn parse_duration(d: &str) -> Option<f64> {
    let mut chars = d.chars();
    let mut beats = match chars.next()? {
        'w' => 4.0,
        'h' => 2.0,
        'q' => 1.0,
        'e' => 0.5,
        's' => 0.25,
        't' => 0.125,
        _ => return None,
    };
    let mut add = beats;
    for c in chars {
        match c {
            '3' => {
                beats *= 2.0 / 3.0;
                add = beats;
            }
            '.' => {
                add *= 0.5;
                beats += add;
            }
            _ => return None,
        }
    }
    Some(beats)
}

/// `C4` → 60, `F#3` → 54, `Bb2` → 46.
pub fn parse_pitch(name: &str) -> Option<f32> {
    let mut chars = name.chars().peekable();
    let base = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let mut acc = 0;
    while let Some(&c) = chars.peek() {
        match c {
            '#' => acc += 1,
            'b' => acc -= 1,
            _ => break,
        }
        chars.next();
    }
    let octave: i32 = chars.collect::<String>().parse().ok()?;
    Some(((octave + 1) * 12 + base + acc) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitches() {
        assert_eq!(parse_pitch("C4"), Some(60.0));
        assert_eq!(parse_pitch("A4"), Some(69.0));
        assert_eq!(parse_pitch("F#3"), Some(54.0));
        assert_eq!(parse_pitch("Bb2"), Some(46.0));
        assert_eq!(parse_pitch("C-1"), Some(0.0));
        let s = Score::parse("C#4:q F#3").unwrap();
        assert_eq!(s.notes[0].pitch, 61.0);
        assert_eq!(s.notes[1].pitch, 54.0);
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("q"), Some(1.0));
        assert_eq!(parse_duration("q."), Some(1.5));
        assert_eq!(parse_duration("h.."), Some(3.5));
        assert!((parse_duration("e3").unwrap() - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn slurs_marks_and_sticky_durations() {
        let s = Score::parse("# a comment\ntempo 60 # another\n(C3:e D3 E3) ^F3:q 'G3 r A3").unwrap();
        let n = &s.notes;
        assert_eq!(n.len(), 6);
        assert!(n[0].slur && n[1].slur && !n[2].slur);
        assert_eq!(n[3].articulation, Articulation::Accent);
        assert_eq!(n[4].articulation, Articulation::Staccato);
        assert!((n[2].dur - 0.5).abs() < 1e-9);
        assert!((n[3].dur - 1.0).abs() < 1e-9);
        // The rest is a quarter: A3 starts at 0.5*3 + 1 + 1 + 1.
        assert!((n[5].start - 4.5).abs() < 1e-9);
    }

    #[test]
    fn hairpins_interpolate() {
        let s = Score::parse("p < C3:q D3 E3 F3 f G3").unwrap();
        let d: Vec<f32> = s.notes.iter().map(|n| n.dynamic).collect();
        assert!(d[0] < d[1] && d[1] < d[2] && d[2] < d[3] && d[3] < d[4]);
        assert_eq!(d[4], 0.72);
        let one = Score::parse("p < C3:w f").unwrap();
        assert_eq!((one.notes[0].dynamic, one.notes[0].dynamic_end), (0.32, 0.72));
    }
}
