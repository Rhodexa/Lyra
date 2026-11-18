//! The Sampler: plays one sample, pitched by the keys you play.
//!
//! The root key plays the sample at its own pitch; other keys shift it
//! (resampling by linear interpolation, so the pitch and length change
//! together, like a classic sampler). One-shot mode plays to the end no matter
//! how long the note is (drums); Hold mode stops on note-off (sustained sounds).
//!
//! The sample arrives through `set_sample` on the UI thread. Without one the
//! sampler is silent.

use crate::util::decay_coeff;
use lyra_plugin::{Curve, Event, ParamInfo, Plugin, ProcessContext, SampleData};
use std::sync::Arc;

pub mod p {
    pub const GAIN: usize = 0;
    pub const ROOT: usize = 1;
    pub const TUNE: usize = 2;
    pub const FINE: usize = 3;
    pub const ATTACK: usize = 4;
    pub const RELEASE: usize = 5;
    pub const START: usize = 6;
    pub const MODE: usize = 7;
}

pub(crate) static PARAMS: [ParamInfo; 8] = [
    ParamInfo::new(0, "Gain", -24.0, 12.0, 0.0, "dB", Curve::Linear),
    ParamInfo::new(1, "Root", 24.0, 96.0, 60.0, "note", Curve::Linear),
    ParamInfo::new(2, "Tune", -24.0, 24.0, 0.0, "st", Curve::Linear),
    ParamInfo::new(3, "Fine", -100.0, 100.0, 0.0, "ct", Curve::Linear),
    ParamInfo::new(4, "Attack", 0.0005, 2.0, 0.0005, "s", Curve::Exponential),
    ParamInfo::new(5, "Release", 0.005, 5.0, 0.05, "s", Curve::Exponential),
    ParamInfo::new(6, "Start", 0.0, 0.95, 0.0, "%", Curve::Linear),
    ParamInfo::choice(7, "Mode", &["One-shot", "Hold"], 0),
];

const VOICES: usize = 16;
/// A choke or note stolen for a new one fades this fast (seconds), never a click.
const QUICK_FADE: f32 = 0.004;

#[derive(Clone, Copy, Default)]
struct Voice {
    on: bool,
    key: u8,
    /// Read position in the sample, in its own frames.
    pos: f64,
    /// Frames to advance per output frame (pitch + rate conversion).
    step: f64,
    velocity: f32,
    env: f32,
    /// 0 while attacking/holding; otherwise the per-frame release multiplier.
    release: f32,
    /// Frames since it started (to steal the oldest).
    age: u64,
}

pub struct Sampler {
    p: [f32; 8],
    sr: f32,
    sample: Option<Arc<SampleData>>,
    voices: [Voice; VOICES],
    clock: u64,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        let mut p = [0.0; 8];
        for info in &PARAMS {
            p[info.id as usize] = info.default;
        }
        Self { p, sr: 48_000.0, sample: None, voices: [Voice::default(); VOICES], clock: 0 }
    }

    fn note_on(&mut self, key: u8, velocity: f32) {
        let Some(s) = &self.sample else { return };
        let len = s.len() as f64;
        if len < 2.0 {
            return;
        }
        let semis = key as f32 - self.p[p::ROOT].round() + self.p[p::TUNE] + self.p[p::FINE] / 100.0;
        let step = (s.sample_rate / self.sr) as f64 * 2f64.powf(semis as f64 / 12.0);
        // A free voice, else the oldest one (it fades out under the new note).
        let i = (0..VOICES)
            .find(|&i| !self.voices[i].on)
            .unwrap_or_else(|| (0..VOICES).max_by_key(|&i| self.voices[i].age).unwrap_or(0));
        self.voices[i] =
            Voice { on: true, key, pos: self.p[p::START] as f64 * len, step, velocity, env: 0.0, release: 0.0, age: 0 };
        self.clock += 1;
    }

    fn release(&mut self, key: Option<u8>, seconds: f32) {
        let coeff = decay_coeff(seconds, self.sr);
        for v in self.voices.iter_mut().filter(|v| v.on && key.is_none_or(|k| v.key == k)) {
            v.release = coeff;
        }
    }
}

impl Plugin for Sampler {
    fn name(&self) -> &'static str {
        "Sampler"
    }
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }
    fn set_param(&mut self, id: u32, value: f32) {
        if let Some(slot) = self.p.get_mut(id as usize) {
            *slot = value;
        }
    }
    fn activate(&mut self, sample_rate: f32) {
        self.sr = sample_rate;
        self.voices = [Voice::default(); VOICES];
    }
    fn set_sample(&mut self, sample: Arc<SampleData>) {
        self.sample = Some(sample);
    }

    fn process(&mut self, ctx: ProcessContext<'_>) {
        ctx.out_l.fill(0.0);
        ctx.out_r.fill(0.0);
        let Some(sample) = self.sample.clone() else { return };
        let (left, right) = (&sample.left, &sample.right);
        let len = left.len();
        let gain = lyra_plugin::db_to_gain(self.p[p::GAIN]);
        let attack_step = 1.0 / (self.p[p::ATTACK].max(0.0001) * self.sr);
        let hold = self.p[p::MODE].round() >= 1.0;
        let release_s = self.p[p::RELEASE];
        let mut next = 0;
        for i in 0..ctx.out_l.len() {
            while let Some(e) = ctx.events.get(next).filter(|e| e.offset as usize <= i) {
                match e.event {
                    Event::NoteOn { key, velocity } => self.note_on(key, velocity),
                    // One-shots ignore note-off: drums ring out.
                    Event::NoteOff { key } if hold => self.release(Some(key), release_s),
                    Event::AllNotesOff => self.release(None, QUICK_FADE),
                    _ => {}
                }
                next += 1;
            }
            let (mut l, mut r) = (0.0f32, 0.0f32);
            for v in self.voices.iter_mut().filter(|v| v.on) {
                let i0 = v.pos as usize;
                if i0 + 1 >= len {
                    v.on = false;
                    continue;
                }
                let frac = (v.pos - i0 as f64) as f32;
                let sl = left[i0] + (left[i0 + 1] - left[i0]) * frac;
                let sr = right[i0] + (right[i0 + 1] - right[i0]) * frac;
                if v.release > 0.0 {
                    v.env *= v.release;
                    if v.env < 1e-4 {
                        v.on = false;
                        continue;
                    }
                } else {
                    v.env = (v.env + attack_step).min(1.0);
                }
                let amp = v.env * v.velocity * gain;
                l += sl * amp;
                r += sr * amp;
                v.pos += v.step;
                v.age += 1;
            }
            ctx.out_l[i] = l;
            ctx.out_r[i] = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyra_plugin::TimedEvent;

    fn sine(seconds: f32, sr: f32) -> Arc<SampleData> {
        let n = (seconds * sr) as usize;
        let left: Vec<f32> = (0..n).map(|i| (i as f32 / sr * 440.0 * std::f32::consts::TAU).sin() * 0.5).collect();
        Arc::new(SampleData { sample_rate: sr, right: left.clone(), left })
    }

    fn run(s: &mut Sampler, events: &[TimedEvent], frames: usize) -> Vec<f32> {
        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        let mut out = Vec::new();
        let mut first = true;
        while out.len() < frames {
            let ev = if first { events } else { &[] };
            first = false;
            s.process(ProcessContext { sample_rate: 48_000.0, events: ev, out_l: &mut l, out_r: &mut r });
            out.extend_from_slice(&l);
        }
        out
    }

    fn on(key: u8) -> TimedEvent {
        TimedEvent { offset: 0, event: Event::NoteOn { key, velocity: 1.0 } }
    }

    /// How long the output keeps sounding, in frames.
    fn sounding(out: &[f32]) -> usize {
        out.iter().rposition(|x| x.abs() > 1e-3).map_or(0, |i| i + 1)
    }

    #[test]
    fn root_key_plays_the_sample_at_its_own_length_across_rates() {
        // A 0.1 s sample recorded at 44.1 kHz, played at 48 kHz: still 0.1 s.
        let mut s = Sampler::new();
        s.activate(48_000.0);
        s.set_sample(sine(0.1, 44_100.0));
        let out = run(&mut s, &[on(60)], 9_600);
        let len = sounding(&out) as f32 / 48_000.0;
        assert!((len - 0.1).abs() < 0.002, "root key length {len}");
        assert!(out.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn an_octave_up_is_twice_as_fast() {
        let mut s = Sampler::new();
        s.activate(48_000.0);
        s.set_sample(sine(0.2, 48_000.0));
        let out = run(&mut s, &[on(72)], 14_400);
        let len = sounding(&out) as f32 / 48_000.0;
        assert!((len - 0.1).abs() < 0.002, "octave up length {len}");
    }

    #[test]
    fn silent_without_a_sample_and_one_shots_ignore_note_off() {
        let mut s = Sampler::new();
        s.activate(48_000.0);
        assert!(run(&mut s, &[on(60)], 2_048).iter().all(|x| *x == 0.0));
        s.set_sample(sine(0.1, 48_000.0));
        let off = TimedEvent { offset: 10, event: Event::NoteOff { key: 60 } };
        let out = run(&mut s, &[on(60), off], 9_600);
        assert!(sounding(&out) > 4_000, "one-shot was cut by its note-off");
    }
}
