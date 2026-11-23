//! The instrument half of the pipeline, and the whole thing streaming.
//!
//! ```text
//!  frames ─▶ BowVoice ─┬─ overtones (body applied per overtone) ─────────┬─▶ out
//!                      └─ bow noise ─┐                                    │
//!  plucks ─▶ Plucks ─────────────────┤                                    │
//!  taps ───▶ Knocks ─────────────────┤                                    │
//!  rings ──▶ strings left ringing ───┴─▶ (+ sympathetic strings) ─▶ Body ─┘
//! ```
//!
//! * [`Sound`] plays curves, one frame-to-frame hop at a time.
//! * [`Engine`] is performer + sound, pulled block by block like any
//!   instrument in a host. This is what Lyra will wrap.
//! * [`render`] plays a whole [`Curves`] offline (for hand-edited curves).
//! * [`render_score`] performs and plays a whole score through the engine.

use crate::body::{Body, BodyResponse};
use crate::curves::{Curves, Frame, FRAME_RATE};
use crate::dsp::OnePole;
use crate::instrument::Instrument;
use crate::performer::{Event, Performer, Style};
use crate::pluck::Plucks;
use crate::ring::Rings;
use crate::rand::Rng;
use crate::score::{Note, Score};
use crate::sympathetic::Sympathetic;
use crate::voice::BowVoice;

/// First sample of frame `f`.
fn frame_sample(f: u64, sr: f32) -> u64 {
    (f as f64 * sr as f64 / FRAME_RATE).round() as u64
}

fn event_sample(e: &Event, sr: f32) -> u64 {
    let t = match e {
        Event::Pluck(p) => p.time,
        Event::Tap(k) => k.time,
        Event::Ring(r) => r.time,
    };
    (t.max(0.0) * sr as f64).round() as u64
}

/// The instrument: plays curves.
pub struct Sound {
    voice: BowVoice,
    /// The second string of a double stop: same body, its own vibration.
    voice2: BowVoice,
    voice2_live: bool,
    last_pitch2: f32,
    tone2: Vec<f32>,
    noise2: Vec<f32>,
    plucks: Plucks,
    knocks: Knocks,
    rings: Rings,
    sym: Sympathetic,
    body: Body,
    tone: Vec<f32>,
    noise: Vec<f32>,
}

impl Sound {
    /// `bright`: the character dial, 0 = natural (matched to recordings), 1 = bright and powerful.
    pub fn new(inst: &Instrument, sr: f32, seed: u64, bright: f32) -> Self {
        let mut voice = BowVoice::new(sr, BodyResponse::new(inst, seed), seed);
        voice.set_bright(bright);
        let mut voice2 = BowVoice::new(sr, BodyResponse::new(inst, seed), seed ^ 0x2D0B);
        voice2.set_bright(bright);
        let hop = (sr as f64 / FRAME_RATE).ceil() as usize + 1;
        Self {
            voice,
            voice2,
            voice2_live: false,
            last_pitch2: 48.0,
            tone2: vec![0.0; hop],
            noise2: vec![0.0; hop],
            plucks: Plucks::new(sr, inst.pluck_ring, seed),
            knocks: Knocks::new(sr, seed),
            rings: Rings::new(sr),
            sym: Sympathetic::new(&inst.open_strings, sr),
            body: Body::new(inst, sr, seed),
            tone: vec![0.0; hop],
            noise: vec![0.0; hop],
        }
    }

    pub fn set_bright(&mut self, bright: f32) {
        self.voice.set_bright(bright);
        self.voice2.set_bright(bright);
    }

    /// What the second string does in frame `f`. Two strings share the
    /// bow's weight, so each sounds a little softer.
    fn second(&mut self, f: &Frame) -> Frame {
        if f.pitch2 > 0.0 {
            self.last_pitch2 = f.pitch2;
        }
        Frame {
            pitch: self.last_pitch2,
            loudness: if f.pitch2 > 0.0 { f.loudness * 0.8 } else { 0.0 },
            brightness: f.brightness,
            noise: f.noise * 0.6,
            string: f.string2,
            ..Frame::default()
        }
    }

    /// One hop gliding from frame `a` to frame `b`. `events` are (offset in
    /// this hop, event), sorted by offset.
    pub fn hop(&mut self, a: &Frame, b: &Frame, out: &mut [f32], events: &[(usize, Event)]) {
        let n = out.len();
        if self.tone.len() < n {
            for v in [&mut self.tone, &mut self.noise, &mut self.tone2, &mut self.noise2] {
                v.resize(n, 0.0);
            }
        }
        self.voice.process(a, b, &mut self.tone[..n], &mut self.noise[..n]);
        let double = a.pitch2 > 0.0 || b.pitch2 > 0.0;
        if double || self.voice2_live {
            // Keeps running one hop after a double stop ends, to fade out.
            let (a2, b2) = (self.second(a), self.second(b));
            self.voice2.process(&a2, &b2, &mut self.tone2[..n], &mut self.noise2[..n]);
            for k in 0..n {
                self.tone[k] += self.tone2[k];
                self.noise[k] += self.noise2[k];
            }
            self.voice2_live = double;
        }
        if b.loudness > 0.05 {
            self.rings.choke(b.string);
            if b.pitch2 > 0.0 {
                self.rings.choke(b.string2);
            }
        }
        let mut next = 0;
        for (i, o) in out.iter_mut().enumerate() {
            while let Some((at, e)) = events.get(next) {
                if *at > i {
                    break;
                }
                match e {
                    Event::Pluck(p) => self.plucks.trigger(p),
                    Event::Tap(k) => self.knocks.trigger(k.strength),
                    Event::Ring(r) => self.rings.start(r),
                }
                next += 1;
            }
            let raw = self.noise[i] + self.plucks.tick() + self.knocks.tick() + self.rings.tick();
            // The open strings hear everything at the bridge, tone included.
            let ring = self.sym.tick(raw + self.tone[i]);
            *o = self.tone[i] + self.body.process(raw + ring);
        }
    }
}

/// Plays hand-made (or saved) curves, offline.
pub fn render(curves: &Curves, inst: &Instrument, sr: f32, seed: u64, bright: f32) -> Vec<f32> {
    let frames = &curves.frames;
    if frames.len() < 2 {
        return Vec::new();
    }
    let mut events: Vec<(u64, Event)> = curves
        .plucks
        .iter()
        .map(|p| Event::Pluck(*p))
        .chain(curves.taps.iter().map(|k| Event::Tap(*k)))
        .chain(curves.rings.iter().map(|r| Event::Ring(*r)))
        .map(|e| (event_sample(&e, sr), e))
        .collect();
    events.sort_by_key(|(s, _)| *s);
    let mut out = vec![0.0f32; frame_sample(frames.len() as u64 - 1, sr) as usize];
    let mut sound = Sound::new(inst, sr, seed, bright);
    let mut next = 0;
    let mut hop_events = Vec::new();
    for f in 0..frames.len() - 1 {
        let (s0, s1) = (frame_sample(f as u64, sr), frame_sample(f as u64 + 1, sr));
        hop_events.clear();
        while let Some(&(s, e)) = events.get(next) {
            if s >= s1 {
                break;
            }
            hop_events.push((s.saturating_sub(s0) as usize, e));
            next += 1;
        }
        sound.hop(&frames[f], &frames[f + 1], &mut out[s0 as usize..s1 as usize], &hop_events);
    }
    out
}

/// Performer + instrument, streaming. Add notes ahead of time (ideally at
/// least [`crate::performer::LOOKAHEAD`] before they start), pull audio in
/// blocks of any size. Time starts at 0 with the first sample.
pub struct Engine {
    perf: Performer,
    sound: Sound,
    sr: f32,
    /// Index of frame `a` (the start of the current hop).
    frame: u64,
    a: Frame,
    hop: Vec<f32>,
    hop_pos: usize,
    hop_len: usize,
    pending: Vec<(u64, Event)>,
    scratch: Vec<Event>,
    hop_events: Vec<(usize, Event)>,
    record: Option<Curves>,
}

impl Engine {
    /// `record`: keep the curves (for plots and saving); offline use only.
    pub fn new(inst: &Instrument, style: &Style, sr: f32, bright: f32, record: bool) -> Self {
        let e = Self {
            perf: Performer::new(inst, style),
            sound: Sound::new(inst, sr, style.seed, bright),
            sr,
            frame: 0,
            a: Frame::default(),
            hop: vec![0.0; (sr as f64 / FRAME_RATE).ceil() as usize + 1],
            hop_pos: 0,
            hop_len: 0,
            pending: Vec::with_capacity(64),
            scratch: Vec::with_capacity(16),
            hop_events: Vec::with_capacity(16),
            record: record.then(Curves::default),
        };
        // The first frame is pulled on the first `process`, so notes added
        // before then are seen from frame 0.
        e
    }

    pub fn add_note(&mut self, note: Note) {
        self.perf.add_note(note);
    }

    pub fn set_bright(&mut self, bright: f32) {
        self.sound.set_bright(bright);
    }

    /// Seconds, at the next sample `process` will write.
    pub fn time(&self) -> f64 {
        (frame_sample(self.frame, self.sr) + self.hop_pos as u64) as f64 / self.sr as f64
    }

    /// Past the end of every note added so far, tail included.
    pub fn finished(&self) -> bool {
        self.time() >= self.perf.end()
    }

    pub fn process(&mut self, out: &mut [f32]) {
        let mut k = 0;
        while k < out.len() {
            if self.hop_pos >= self.hop_len {
                self.next_hop();
            }
            let take = (self.hop_len - self.hop_pos).min(out.len() - k);
            out[k..k + take].copy_from_slice(&self.hop[self.hop_pos..self.hop_pos + take]);
            self.hop_pos += take;
            k += take;
        }
    }

    /// The recorded curves (if recording), up to now.
    pub fn curves(&self) -> Option<&Curves> {
        self.record.as_ref()
    }

    fn pull_frame(&mut self) -> Frame {
        let f = self.perf.next_frame(&mut self.scratch);
        if let Some(rec) = &mut self.record {
            rec.frames.push(f);
        }
        for e in self.scratch.drain(..) {
            if let Some(rec) = &mut self.record {
                match e {
                    Event::Pluck(p) => rec.plucks.push(p),
                    Event::Tap(k) => rec.taps.push(k),
                    Event::Ring(r) => rec.rings.push(r),
                }
            }
            self.pending.push((event_sample(&e, self.sr), e));
        }
        f
    }

    fn next_hop(&mut self) {
        if self.hop_len > 0 {
            self.frame += 1;
        } else {
            self.a = self.pull_frame();
        }
        let b = self.pull_frame();
        let (s0, s1) = (frame_sample(self.frame, self.sr), frame_sample(self.frame + 1, self.sr));
        self.hop_len = (s1 - s0) as usize;
        self.hop_pos = 0;
        self.hop_events.clear();
        let mut k = 0;
        while k < self.pending.len() {
            let (s, e) = self.pending[k];
            if s < s1 {
                self.hop_events.push((s.saturating_sub(s0) as usize, e));
                self.pending.swap_remove(k);
            } else {
                k += 1;
            }
        }
        self.hop_events.sort_by_key(|(o, _)| *o);
        let a = self.a;
        self.sound.hop(&a, &b, &mut self.hop[..self.hop_len], &self.hop_events);
        self.a = b;
    }
}

/// Performs and plays a whole score through the streaming engine.
/// Returns the audio and the curves it played.
pub fn render_score(score: &Score, inst: &Instrument, style: &Style, sr: f32, bright: f32) -> (Vec<f32>, Curves) {
    let mut engine = Engine::new(inst, style, sr, bright, true);
    for n in &score.notes {
        engine.add_note(n.clone());
    }
    let frames = (engine.perf.end() * FRAME_RATE).ceil() as u64;
    let mut out = vec![0.0f32; frame_sample(frames - 1, sr) as usize];
    for block in out.chunks_mut(512) {
        engine.process(block);
    }
    let curves = engine.record.take().unwrap_or_default();
    (out, curves)
}

/// Finger knocks: a click and a few milliseconds of dull noise. The body's
/// resonances turn them into "a fingertip on a wooden table".
struct Knocks {
    env: f32,
    click: f32,
    decay: f32,
    lp: OnePole,
    rng: Rng,
}

impl Knocks {
    fn new(sr: f32, seed: u64) -> Self {
        Self { env: 0.0, click: 0.0, decay: (-1.0 / (0.004 * sr)).exp(), lp: OnePole::new(sr, 1800.0), rng: Rng::new(seed ^ 0x7A9) }
    }
    fn trigger(&mut self, strength: f32) {
        self.env += 0.2 * strength;
        self.click += 0.45 * strength;
    }
    #[inline]
    fn tick(&mut self) -> f32 {
        let y = self.lp.lowpass(self.rng.bipolar() * self.env + self.click);
        self.env *= self.decay;
        self.click *= 0.5;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::cello;
    use crate::performer::{perform, LOOKAHEAD};

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn renders_clean_audible_audio() {
        let inst = cello();
        let s = Score::parse("tempo 80\n(G2:e D3 B3) A3 B3 D3 B3 D3 pizz G2:q D3 arco C3:h").unwrap();
        let (out, _) = render_score(&s, &inst, &Style::default(), 48_000.0, 0.0);
        assert!(out.iter().all(|v| v.is_finite()));
        let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.01 && peak < 4.0, "peak {peak}");
        // The pizzicato section is audible too (notes 9-10 at ~3.0-3.75 s).
        let pizz = &out[(3.05 * 48_000.0) as usize..(3.7 * 48_000.0) as usize];
        assert!(rms(pizz) > 0.005, "pizz rms {}", rms(pizz));
    }

    #[test]
    fn same_seed_same_sound() {
        let inst = cello();
        let s = Score::parse("C3:q E3 G3").unwrap();
        let c = perform(&s, &inst, &Style::default());
        assert_eq!(render(&c, &inst, 44_100.0, 7, 0.5), render(&c, &inst, 44_100.0, 7, 0.5));
    }

    #[test]
    fn engine_plays_exactly_what_the_curves_say() {
        let inst = cello();
        let s = Score::parse("tempo 90\n(G2:e D3 B3) A3 'B3 D3 pizz G2:q arco ^C3:h").unwrap();
        let style = Style::default();
        for sr in [48_000.0, 44_100.0] {
            let (streamed, curves) = render_score(&s, &inst, &style, sr, 0.3);
            assert_eq!(curves.frames, perform(&s, &inst, &style).frames);
            assert_eq!(streamed, render(&curves, &inst, sr, style.seed, 0.3), "at {sr} Hz");
        }
    }

    #[test]
    fn live_notes_arriving_one_second_early_change_nothing() {
        let inst = cello();
        let s = Score::parse("tempo 100\n(C3:e D3 E3 F3) G3:q A3 B3:h").unwrap();
        let style = Style::default();
        let (offline, _) = render_score(&s, &inst, &style, 48_000.0, 0.0);
        let mut e = Engine::new(&inst, &style, 48_000.0, 0.0, false);
        let mut live = vec![0.0f32; offline.len()];
        let mut next = 0;
        for block in live.chunks_mut(256) {
            // A host without look-ahead would deliver each note LOOKAHEAD
            // early by delaying everything; here notes arrive just in time.
            while next < s.notes.len() && s.notes[next].start - LOOKAHEAD <= e.time() + 0.004 {
                e.add_note(s.notes[next].clone());
                next += 1;
            }
            e.process(block);
        }
        assert_eq!(live, offline);
    }
}

#[cfg(test)]
mod probe {
    use crate::body::{Body, BodyResponse};
    use crate::curves::FRAME_RATE;
    use crate::instrument::cello;
    use crate::performer::{perform, Style};
    use crate::score::Score;
    use crate::sympathetic::Sympathetic;
    use crate::voice::BowVoice;

    #[test]
    #[ignore]
    fn how_loud_are_the_parts() {
        let inst = cello();
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../scores/prelude.txt")).unwrap();
        let c = perform(&Score::parse(&text).unwrap(), &inst, &Style::default());
        let sr = 48_000.0;
        let at = |f: usize| (f as f64 * sr as f64 / FRAME_RATE).round() as usize;
        let len = at(c.frames.len() - 1);
        let (mut tone, mut noise) = (vec![0.0; len], vec![0.0; len]);
        let mut v = BowVoice::new(sr, BodyResponse::new(&inst, 1), 1);
        for f in 0..c.frames.len() - 1 {
            let (a, b) = (at(f), at(f + 1));
            v.process(&c.frames[f], &c.frames[f + 1], &mut tone[a..b], &mut noise[a..b]);
        }
        let mut sym = Sympathetic::new(&inst.open_strings, sr);
        let mut body = Body::new(&inst, sr, 1);
        let (mut ring_out, mut noise_out) = (vec![0.0f32; len], vec![0.0f32; len]);
        for i in 0..len {
            let ring = sym.tick(noise[i] + tone[i]);
            ring_out[i] = body.process(ring);
        }
        let mut body2 = Body::new(&inst, sr, 1);
        for i in 0..len {
            noise_out[i] = body2.process(noise[i]);
        }
        let db = |x: &[f32]| 10.0 * (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).log10();
        println!("tone {:.1} dB | sympathetic {:.1} dB | bow noise {:.1} dB", db(&tone), db(&ring_out), db(&noise_out));
    }
}
