//! The contract between the performer and the instrument.
//!
//! A performance is a list of frames, [`FRAME_RATE`] per second, plus pluck
//! events (pizzicato). It saves as CSV so it can be plotted and hand-edited:
//!
//! ```text
//! pitch,loudness,brightness,noise,string
//! 43.000,0.000,0.420,0.050,1
//! ...
//! pluck,1.250,50.000,0.600,0.550,2
//! tap,2.000,0.700
//! ring,2.500,43.000,0.600,1,1
//! ```

pub const FRAME_RATE: f64 = 250.0;

/// What the bow and the left hand are doing at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    /// MIDI note number, fractional (vibrato, glides, intonation live here).
    pub pitch: f32,
    /// Bowed loudness, 0..1-ish (bow speed × force).
    pub loudness: f32,
    /// 0..1: dark/far from the bridge … bright/near the bridge.
    pub brightness: f32,
    /// 0..1: bow noise relative to the tone (rosin, scratch at attacks).
    pub noise: f32,
    /// Which string (0 = lowest), for tone and sympathetic resonance.
    pub string: u8,
}

/// A pizzicato pluck.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pluck {
    pub time: f64,
    pub pitch: f32,
    pub velocity: f32,
    pub brightness: f32,
    pub string: u8,
}

/// A left-hand finger hitting (or leaving) the fingerboard: a small
/// wooden knock, separate from the tone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tap {
    pub time: f64,
    /// 0..1
    pub strength: f32,
}

/// The bow has left a string, which rings on by itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub time: f64,
    pub pitch: f32,
    pub level: f32,
    pub string: u8,
    /// Open strings ring long; stopped ones are damped by the finger.
    pub open: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Curves {
    pub frames: Vec<Frame>,
    pub plucks: Vec<Pluck>,
    pub taps: Vec<Tap>,
    pub rings: Vec<Ring>,
}

impl Curves {
    pub fn duration(&self) -> f64 {
        self.frames.len() as f64 / FRAME_RATE
    }

    pub fn to_csv(&self) -> String {
        let mut s = String::from("pitch,loudness,brightness,noise,string\n");
        for f in &self.frames {
            s += &format!("{:.3},{:.4},{:.4},{:.4},{}\n", f.pitch, f.loudness, f.brightness, f.noise, f.string);
        }
        for p in &self.plucks {
            s += &format!("pluck,{:.4},{:.3},{:.4},{:.4},{}\n", p.time, p.pitch, p.velocity, p.brightness, p.string);
        }
        for k in &self.taps {
            s += &format!("tap,{:.4},{:.3}\n", k.time, k.strength);
        }
        for r in &self.rings {
            s += &format!("ring,{:.4},{:.3},{:.4},{},{}\n", r.time, r.pitch, r.level, r.string, r.open as u8);
        }
        s
    }

    pub fn from_csv(text: &str) -> Result<Curves, String> {
        let mut c = Curves::default();
        for (ln, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("pitch") {
                continue;
            }
            let err = || format!("line {}: can't read {line:?}", ln + 1);
            let cols: Vec<&str> = line.split(',').map(str::trim).collect();
            let num = |i: usize| cols.get(i).and_then(|v| v.parse::<f64>().ok()).ok_or_else(err);
            if cols[0] == "ring" {
                c.rings.push(Ring {
                    time: num(1)?,
                    pitch: num(2)? as f32,
                    level: num(3)? as f32,
                    string: num(4)? as u8,
                    open: num(5)? != 0.0,
                });
            } else if cols[0] == "tap" {
                c.taps.push(Tap { time: num(1)?, strength: num(2)? as f32 });
            } else if cols[0] == "pluck" {
                c.plucks.push(Pluck {
                    time: num(1)?,
                    pitch: num(2)? as f32,
                    velocity: num(3)? as f32,
                    brightness: num(4)? as f32,
                    string: num(5)? as u8,
                });
            } else {
                c.frames.push(Frame {
                    pitch: num(0)? as f32,
                    loudness: num(1)? as f32,
                    brightness: num(2)? as f32,
                    noise: num(3)? as f32,
                    string: num(4)? as u8,
                });
            }
        }
        c.plucks.sort_by(|a, b| a.time.total_cmp(&b.time));
        c.taps.sort_by(|a, b| a.time.total_cmp(&b.time));
        c.rings.sort_by(|a, b| a.time.total_cmp(&b.time));
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_round_trip() {
        let c = Curves {
            frames: vec![
                Frame { pitch: 43.0, loudness: 0.5, brightness: 0.4, noise: 0.05, string: 1 },
                Frame { pitch: 43.12, loudness: 0.55, brightness: 0.45, noise: 0.06, string: 1 },
            ],
            plucks: vec![Pluck { time: 0.5, pitch: 50.0, velocity: 0.6, brightness: 0.5, string: 2 }],
            taps: vec![Tap { time: 0.25, strength: 0.7 }],
            rings: vec![Ring { time: 0.75, pitch: 43.0, level: 0.6, string: 1, open: true }],
        };
        let back = Curves::from_csv(&c.to_csv()).unwrap();
        assert_eq!(back.frames.len(), 2);
        assert_eq!(back.plucks, c.plucks);
        assert_eq!(back.taps, c.taps);
        assert_eq!(back.rings, c.rings);
        assert!((back.frames[1].pitch - 43.12).abs() < 1e-3);
    }
}
