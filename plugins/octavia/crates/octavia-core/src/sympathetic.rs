//! The open strings that aren't being played still ring along, weakly, when
//! the played note shares their overtones.

use crate::dsp::StringLoop;
use crate::midi_to_hz;

pub struct Sympathetic {
    strings: Vec<StringLoop>,
    coupling: f32,
    level: f32,
}

impl Sympathetic {
    pub fn new(open: &[f32; 4], sr: f32) -> Self {
        let strings = open
            .iter()
            .map(|&p| {
                let mut s = StringLoop::new();
                s.tune(sr, midi_to_hz(p), 2.5, 0.35);
                s
            })
            .collect();
        Self { strings, coupling: 0.004, level: 1.0 }
    }

    /// Takes the bridge signal, returns what the open strings add to it.
    #[inline]
    pub fn tick(&mut self, bridge: f32) -> f32 {
        let input = bridge * self.coupling;
        let mut y = 0.0;
        for s in &mut self.strings {
            y += s.tick(input);
        }
        y * self.level
    }
}
