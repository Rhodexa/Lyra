//! Just enough music theory for the piano roll to be helpful:
//! scales (for highlighting and snapping) and chords (for one-click stamping).

pub const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// "C4" for MIDI 60.
pub fn note_name(key: u8) -> String {
    format!("{}{}", NOTE_NAMES[key as usize % 12], key as i32 / 12 - 1)
}

pub fn is_black(key: u8) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Major,
    Minor,
    Dorian,
    Mixolydian,
    HarmonicMinor,
    MinorPentatonic,
    MajorPentatonic,
    Chromatic,
}

impl Scale {
    pub const ALL: [Scale; 8] = [
        Scale::Major,
        Scale::Minor,
        Scale::Dorian,
        Scale::Mixolydian,
        Scale::HarmonicMinor,
        Scale::MinorPentatonic,
        Scale::MajorPentatonic,
        Scale::Chromatic,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Scale::Major => "Major",
            Scale::Minor => "Minor",
            Scale::Dorian => "Dorian",
            Scale::Mixolydian => "Mixolydian",
            Scale::HarmonicMinor => "Harmonic minor",
            Scale::MinorPentatonic => "Minor pentatonic",
            Scale::MajorPentatonic => "Major pentatonic",
            Scale::Chromatic => "Chromatic",
        }
    }

    /// Semitones above the root.
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Scale::MinorPentatonic => &[0, 3, 5, 7, 10],
            Scale::MajorPentatonic => &[0, 2, 4, 7, 9],
            Scale::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        }
    }

    pub fn contains(self, root: u8, key: u8) -> bool {
        self.intervals().contains(&((key + 12 - root % 12) % 12))
    }

    /// Nearest in-scale key (ties go down).
    pub fn snap(self, root: u8, key: u8) -> u8 {
        (0..12u8)
            .flat_map(|d| [key.saturating_sub(d), key.saturating_add(d).min(127)])
            .find(|&k| self.contains(root, k))
            .unwrap_or(key)
    }

    /// Move `steps` scale degrees from `key` (which is snapped into the scale first).
    pub fn degree_offset(self, root: u8, key: u8, steps: i32) -> u8 {
        let iv = self.intervals();
        let n = iv.len() as i32;
        let key = self.snap(root, key) as i32;
        let rel = key - (root % 12) as i32;
        let octave = rel.div_euclid(12);
        let degree = iv.iter().position(|&s| s as i32 == rel.rem_euclid(12)).unwrap_or(0) as i32;
        let target = degree + steps;
        let k = (root % 12) as i32 + (octave + target.div_euclid(n)) * 12 + iv[target.rem_euclid(n) as usize] as i32;
        k.clamp(0, 127) as u8
    }
}

/// What one click in the piano roll places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chord {
    Single,
    /// Built from the project's scale: always "fits".
    ScaleTriad,
    ScaleSeventh,
    ScaleNinth,
    Major,
    Minor,
    Maj7,
    Min7,
    Dom7,
    Min9,
    Sus2,
    Sus4,
    Power,
    Octave,
}

impl Chord {
    pub const ALL: [Chord; 14] = [
        Chord::Single,
        Chord::ScaleTriad,
        Chord::ScaleSeventh,
        Chord::ScaleNinth,
        Chord::Major,
        Chord::Minor,
        Chord::Maj7,
        Chord::Min7,
        Chord::Dom7,
        Chord::Min9,
        Chord::Sus2,
        Chord::Sus4,
        Chord::Power,
        Chord::Octave,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Chord::Single => "Single note",
            Chord::ScaleTriad => "Triad (in key)",
            Chord::ScaleSeventh => "7th (in key)",
            Chord::ScaleNinth => "9th (in key)",
            Chord::Major => "Major",
            Chord::Minor => "Minor",
            Chord::Maj7 => "Maj7",
            Chord::Min7 => "Min7",
            Chord::Dom7 => "Dom7",
            Chord::Min9 => "Min9",
            Chord::Sus2 => "Sus2",
            Chord::Sus4 => "Sus4",
            Chord::Power => "Power (5th)",
            Chord::Octave => "Octave",
        }
    }

    /// Keys to place when the user clicks `key`. The first is the clicked (root) note.
    pub fn keys(self, key: u8, scale: Scale, root: u8) -> Vec<u8> {
        let stack = |n: i32| -> Vec<u8> {
            // Chords stacked in thirds need a 7-note scale; others fall back to minor.
            let s = if scale.intervals().len() == 7 { scale } else { Scale::Minor };
            let base = s.snap(root, key);
            (0..n).map(|i| s.degree_offset(root, base, i * 2)).collect()
        };
        let fixed = |iv: &[u8]| iv.iter().map(|&i| key.saturating_add(i).min(127)).collect();
        match self {
            Chord::Single => vec![key],
            Chord::ScaleTriad => stack(3),
            Chord::ScaleSeventh => stack(4),
            Chord::ScaleNinth => stack(5),
            Chord::Major => fixed(&[0, 4, 7]),
            Chord::Minor => fixed(&[0, 3, 7]),
            Chord::Maj7 => fixed(&[0, 4, 7, 11]),
            Chord::Min7 => fixed(&[0, 3, 7, 10]),
            Chord::Dom7 => fixed(&[0, 4, 7, 10]),
            Chord::Min9 => fixed(&[0, 3, 7, 10, 14]),
            Chord::Sus2 => fixed(&[0, 2, 7]),
            Chord::Sus4 => fixed(&[0, 5, 7]),
            Chord::Power => fixed(&[0, 7]),
            Chord::Octave => fixed(&[0, 12]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minor_scale_chords() {
        let a = 9; // A
        // In A minor, the triad on A is A C E; on F it's F A C (major); on E, E G B.
        assert_eq!(Chord::ScaleTriad.keys(57, Scale::Minor, a), vec![57, 60, 64]);
        assert_eq!(Chord::ScaleTriad.keys(53, Scale::Minor, a), vec![53, 57, 60]);
        assert_eq!(Chord::ScaleSeventh.keys(57, Scale::Minor, a), vec![57, 60, 64, 67]); // Am7
        // An out-of-key click (A#) snaps into the key first.
        assert_eq!(Chord::ScaleTriad.keys(58, Scale::Minor, a)[0], 57);
    }

    #[test]
    fn names_and_snapping() {
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(57), "A3");
        assert!(Scale::Minor.contains(9, 60)); // C is in A minor
        assert!(!Scale::Minor.contains(9, 61));
        assert_eq!(Scale::Minor.snap(9, 61), 60);
    }
}
