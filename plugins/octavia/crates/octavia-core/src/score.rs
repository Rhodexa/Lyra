//! What the performer reads: notes in seconds, with optional *hints*.
//!
//! A note from a piano roll is just start, length, pitch and loudness. The
//! performer infers the rest (slurs, staccato, accents). Hints overrule it:
//! `articulation` other than `Auto`, or `slur: Some(..)`.
//!
//! Two text formats for testing. A **score**, with hints written in:
//!
//! ```text
//! # comment
//! tempo 66
//! mp
//! (G2:e D3 B3) A3 B3 D3 B3 D3     # ( ) = slur, durations are sticky
//! < ^C4:q 'D4 -E4 f               # < or > starts a hairpin, ending at the next dynamic
//! pizz G2:q r D3 arco
//! D3+A3:h                         # + joins a double stop (two strings at once)
//! ```
//!
//! Durations: `w h q e s t` (whole … 32nd), `3` after the letter for a
//! triplet, dots for dotted. Marks before a note: `^` accent, `'` staccato
//! (the note is shortened, as if drawn short), `*` spiccato (bounced), `-` tenuto. Dynamics:
//! `pp p mp mf f ff`. `r` is a rest. Unmarked notes are left to the performer.
//!
//! And a **roll**, exactly what a piano roll holds, nothing more:
//!
//! ```text
//! tempo 72
//! # beat  length  key  velocity(0..1)
//! 0       0.25    G2   0.7
//! 0.25    0.25    D3   0.65
//! ```

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Articulation {
    /// Let the performer decide (the default).
    Auto,
    /// Ordinary separate bow.
    Detache,
    Staccato,
    /// Bounced bow.
    Spiccato,
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
    /// Slurred into the next note (same bow, no re-attack)? `None` = let the
    /// performer decide (it slurs overlapping notes and fast runs).
    pub slur: Option<bool>,
    /// A second, lower note bowed together with this one on the next string
    /// down (a double stop). The performer drops it if no hand could play it.
    pub double: Option<f32>,
}

impl Note {
    pub fn end(&self) -> f64 {
        self.start + self.dur
    }
}

/// One line of melody, sorted by start. Overlapping notes mean "slur into
/// the next one" (Octavia plays one string at a time for now).
#[derive(Clone, Debug, Default)]
pub struct Score {
    pub notes: Vec<Note>,
}

impl Score {
    pub fn end(&self) -> f64 {
        self.notes.iter().map(Note::end).fold(0.0, f64::max)
    }

    pub fn tidy(&mut self) {
        self.notes.sort_by(|a, b| a.start.total_cmp(&b.start));
    }

    /// Reads a roll: `tempo N` lines, then `beat length key velocity` lines.
    pub fn parse_roll(text: &str) -> Result<Score, String> {
        let mut beat = 60.0 / 90.0;
        let mut notes = Vec::new();
        for (ln, line) in text.lines().enumerate() {
            let line = line.split(" #").next().unwrap_or("").trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let err = || format!("line {}: expected `beat length key velocity`, got {line:?}", ln + 1);
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols[0] == "tempo" {
                let bpm: f64 = cols.get(1).and_then(|v| v.parse().ok()).ok_or_else(err)?;
                beat = 60.0 / bpm.max(1.0);
                continue;
            }
            if cols.len() < 4 {
                return Err(err());
            }
            let at: f64 = cols[0].parse().map_err(|_| err())?;
            let len: f64 = cols[1].parse().map_err(|_| err())?;
            let pitch = parse_pitch(cols[2]).ok_or_else(err)?;
            let vel: f32 = cols[3].parse().map_err(|_| err())?;
            let dynamic = velocity_to_dynamic(vel);
            notes.push(Note {
                start: at * beat,
                dur: len * beat,
                pitch,
                dynamic,
                dynamic_end: dynamic,
                articulation: Articulation::Auto,
                slur: None,
                double: None,
            });
        }
        let mut s = Score { notes };
        s.tidy();
        s.merge_chords();
        Ok(s)
    }

    /// Notes starting together become one note: the highest, with the next
    /// one down as its double stop. Any others are dropped.
    pub fn merge_chords(&mut self) {
        let mut out: Vec<Note> = Vec::with_capacity(self.notes.len());
        let mut group: Vec<Note> = Vec::new();
        let flush = |group: &mut Vec<Note>, out: &mut Vec<Note>| {
            if group.is_empty() {
                return;
            }
            group.sort_by(|a, b| b.pitch.total_cmp(&a.pitch));
            let mut top = group[0].clone();
            let below = group[1..].iter().flat_map(|n| std::iter::once(n.pitch).chain(n.double)).chain(top.double);
            top.double = below.filter(|&p| p < top.pitch).reduce(f32::max);
            out.push(top);
            group.clear();
        };
        for n in self.notes.drain(..) {
            if group.first().is_some_and(|g| (n.start - g.start).abs() >= 0.01) {
                flush(&mut group, &mut out);
            }
            group.push(n);
        }
        flush(&mut group, &mut out);
        self.notes = out;
    }

    /// Writes the notes as a piano roll would hold them: timing, pitch and
    /// velocity only. Every hint is lost, which is the point: it shows what
    /// the performer infers on its own.
    pub fn to_roll(&self, tempo: f64) -> String {
        let beat = 60.0 / tempo;
        let mut s = format!("tempo {tempo}\n# beat  length  key  velocity\n");
        for n in &self.notes {
            let mut vel = dynamic_to_velocity(n.dynamic);
            if n.articulation == Articulation::Accent {
                vel = (vel + 0.2).min(1.0);
            }
            s += &format!("{:.4} {:.4} {} {:.2}\n", n.start / beat, n.dur / beat, pitch_name(n.pitch), vel);
            if let Some(d) = n.double {
                s += &format!("{:.4} {:.4} {} {:.2}\n", n.start / beat, n.dur / beat, pitch_name(d), vel);
            }
        }
        s
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
                let mut art = if pizz { Articulation::Pizzicato } else { Articulation::Auto };
                loop {
                    if let Some(r) = t.strip_prefix('^') {
                        art = Articulation::Accent;
                        t = r;
                    } else if let Some(r) = t.strip_prefix('\'') {
                        art = Articulation::Staccato;
                        t = r;
                    } else if let Some(r) = t.strip_prefix('*') {
                        art = Articulation::Spiccato;
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
                    let (pitch, double) = match name.split_once('+') {
                        Some((a, b)) => {
                            let a = parse_pitch(a).ok_or_else(|| err("bad note name"))?;
                            let b = parse_pitch(b).ok_or_else(|| err("bad note name"))?;
                            (a.max(b), Some(a.min(b)))
                        }
                        None => (parse_pitch(name).ok_or_else(|| err("bad note name"))?, None),
                    };
                    // Staccato is played short: write it as if drawn short.
                    let dur = if art == Articulation::Staccato { secs * 0.45 } else { secs };
                    notes.push(Note {
                        start: time,
                        dur,
                        pitch,
                        dynamic,
                        dynamic_end: dynamic,
                        articulation: art,
                        slur: if in_slur { Some(true) } else { None },
                        double,
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
        n.slur = Some(false);
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

/// Piano-roll velocity (0..1) to the performer's dynamic (pp ≈ 0.2 … ff ≈ 0.86).
pub fn velocity_to_dynamic(v: f32) -> f32 {
    0.12 + 0.78 * v.clamp(0.0, 1.0)
}

fn dynamic_to_velocity(d: f32) -> f32 {
    ((d - 0.12) / 0.78).clamp(0.0, 1.0)
}

/// 60 → `C4`.
pub fn pitch_name(p: f32) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let k = p.round() as i32;
    format!("{}{}", NAMES[k.rem_euclid(12) as usize], k.div_euclid(12) - 1)
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
        assert_eq!((n[0].slur, n[1].slur, n[2].slur), (Some(true), Some(true), Some(false)));
        assert_eq!(n[3].slur, None);
        assert_eq!(n[3].articulation, Articulation::Accent);
        assert_eq!(n[4].articulation, Articulation::Staccato);
        assert!((n[4].dur - 0.45).abs() < 1e-9, "staccato is written short");
        assert!((n[2].dur - 0.5).abs() < 1e-9);
        assert!((n[3].dur - 1.0).abs() < 1e-9);
        // The rest is a quarter: A3 starts at 0.5*3 + 1 + 1 + 1.
        assert!((n[5].start - 4.5).abs() < 1e-9);
    }

    #[test]
    fn rolls_round_trip_without_hints() {
        let s = Score::parse("tempo 60\nmf (C3:e D3) ^E3:q").unwrap();
        let roll = s.to_roll(60.0);
        let back = Score::parse_roll(&roll).unwrap();
        assert_eq!(back.notes.len(), 3);
        assert!((back.notes[2].start - 1.0).abs() < 1e-3);
        assert_eq!(back.notes[1].pitch, 50.0);
        assert!(back.notes.iter().all(|n| n.slur.is_none() && n.articulation == Articulation::Auto));
        assert!(back.notes[2].dynamic > back.notes[0].dynamic, "accent became velocity");
        assert_eq!(pitch_name(61.0), "C#4");
    }

    #[test]
    fn double_stops_from_plus_and_from_rolls() {
        let s = Score::parse("A3+D3:h").unwrap();
        assert_eq!((s.notes[0].pitch, s.notes[0].double), (57.0, Some(50.0)));
        let r = Score::parse_roll("tempo 60\n0 1 D3 0.5\n0 1 A3 0.5\n0 1 F3 0.5\n1 1 G3 0.5").unwrap();
        assert_eq!(r.notes.len(), 2);
        assert_eq!((r.notes[0].pitch, r.notes[0].double), (57.0, Some(53.0)), "top + the next one down");
        assert_eq!(r.notes[1].double, None);
        let back = Score::parse_roll(&s.to_roll(60.0)).unwrap();
        assert_eq!(back.notes[0].double, Some(50.0));
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
