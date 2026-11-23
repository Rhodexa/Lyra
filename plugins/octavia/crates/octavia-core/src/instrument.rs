//! What makes a cello a cello: its strings and its wooden body.
//!
//! Body modes are rough values from the acoustics literature (the A0 air
//! mode, the B1 corpus modes, the bridge region), not measurements of a
//! particular instrument. Above them, a seeded scatter of small modes gives
//! the body its "grain".

#[derive(Clone, Copy, Debug)]
pub struct Mode {
    pub hz: f32,
    pub q: f32,
    pub gain: f32,
}

#[derive(Clone, Debug)]
pub struct Instrument {
    pub name: &'static str,
    /// Open strings, lowest first, as MIDI notes.
    pub open_strings: [f32; 4],
    /// Brightness offset per string (low strings are darker).
    pub string_tone: [f32; 4],
    pub body_modes: Vec<Mode>,
    /// Frequency range (Hz) of the scattered small modes, and how many.
    pub body_grain: (f32, f32, usize),
    /// Dense detail of the body response applied per overtone: Hz range, count.
    pub body_detail: (f32, f32, usize),
    /// How much of the string goes straight out, unfiltered by the body.
    pub body_direct: f32,
    /// Below this the body radiates poorly (12 dB/octave roll-off).
    pub radiation_hz: f32,
    /// Seconds a low pizzicato rings (higher notes ring shorter).
    pub pluck_ring: f32,
    /// Semitones the left hand covers without moving: four fingers over
    /// about a major third on a cello (with a stretch); a whole fifth on a
    /// violin, whose strings are much shorter.
    pub hand_span: f32,
    /// Typical vibrato speed, Hz.
    pub vibrato_rate: f32,
}

impl Instrument {
    pub fn by_name(name: &str) -> Option<Instrument> {
        match name {
            "cello" => Some(cello()),
            "violin" => Some(violin()),
            _ => None,
        }
    }
}

const fn m(hz: f32, q: f32, gain: f32) -> Mode {
    Mode { hz, q, gain }
}

pub fn cello() -> Instrument {
    Instrument {
        name: "cello",
        open_strings: [36.0, 43.0, 50.0, 57.0], // C2 G2 D3 A3
        string_tone: [-0.08, -0.03, 0.02, 0.08],
        body_modes: vec![
            m(98.0, 18.0, 0.9),  // A0: air through the f-holes
            m(145.0, 22.0, 0.5), // T1
            m(196.0, 25.0, 0.9), // B1-
            m(230.0, 25.0, 1.0), // B1+
            m(290.0, 20.0, 0.6),
            m(360.0, 22.0, 0.6),
            m(420.0, 25.0, 0.45),
            m(520.0, 20.0, 0.55),
            m(640.0, 22.0, 0.4),
            m(780.0, 18.0, 0.35),
            m(1000.0, 14.0, 0.45),
            m(1250.0, 12.0, 0.35),
            m(1600.0, 10.0, 0.3),
            m(2100.0, 8.0, 0.25),
        ],
        body_grain: (150.0, 6000.0, 110),
        body_detail: (80.0, 9000.0, 320),
        body_direct: 0.22,
        radiation_hz: 70.0,
        pluck_ring: 1.6,
        hand_span: 4.0,
        vibrato_rate: 5.4,
    }
}

pub fn violin() -> Instrument {
    Instrument {
        name: "violin",
        open_strings: [55.0, 62.0, 69.0, 76.0], // G3 D4 A4 E5
        string_tone: [-0.08, -0.02, 0.03, 0.1],
        body_modes: vec![
            m(275.0, 18.0, 0.8), // A0
            m(460.0, 25.0, 0.9), // B1-
            m(540.0, 25.0, 1.0), // B1+
            m(700.0, 20.0, 0.5),
            m(850.0, 20.0, 0.5),
            m(1050.0, 18.0, 0.45),
            m(1300.0, 15.0, 0.4),
            m(1700.0, 12.0, 0.35),
            m(2300.0, 5.0, 0.4), // the "bridge hill"
            m(2800.0, 6.0, 0.3),
            m(3500.0, 8.0, 0.25),
        ],
        body_grain: (700.0, 7000.0, 28),
        body_detail: (150.0, 12000.0, 320),
        body_direct: 0.22,
        radiation_hz: 190.0,
        pluck_ring: 0.8,
        hand_span: 6.0,
        vibrato_rate: 5.8,
    }
}
