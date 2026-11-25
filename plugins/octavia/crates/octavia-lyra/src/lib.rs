//! # octavia-lyra
//!
//! Octavia as a Lyra instrument. Only glue: the parameter table
//! (`docs/FACEPLATE.md`, "The controls"), the player profiles as presets,
//! Lyra's notes into [`Engine`], its sound out, and a [`Telemetry`] the
//! faceplate reads. Everything musical lives in `octavia-core`.
//!
//! **Notes.** Lyra hands over the next second of the song each block (its
//! `lyra.note-lookahead` extension); that becomes the performer's forecast.
//! The note-ons and offs stay the truth: a forecast note that never comes is
//! dropped, a note nobody forecast (a key played live) is sight-read, a
//! note-off ends its note. See `Performer::forecast` in the core.

use lyra_plugin::{Curve, Event, ParamGroup, ParamInfo, Plugin, PluginInfo, Preset, ProcessContext, UpcomingNote};
use octavia_core::instrument::{cello, violin};
use octavia_core::{Engine, Note, Skill, Snapshot, Style, LOOKAHEAD};
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

pub const ID: &str = "lyra.octavia";

// Parameter ids: forever (see lyra-plugin). New ones go at the end.
pub const INTONATION: u32 = 0;
pub const DEXTERITY: u32 = 1;
pub const BOWING: u32 = 2;
pub const COMPOSURE: u32 = 3;
pub const EXPRESSION: u32 = 4;
pub const VIBRATO: u32 = 5;
pub const BREATH: u32 = 6;
pub const PERFORMER: u32 = 7;
pub const CHARACTER: u32 = 8;
pub const INSTRUMENT: u32 = 9;
pub const PATCH: u32 = 10;
pub const DOUBLE_STOPS: u32 = 11;
pub const TAKE: u32 = 12;

const LIN: Curve = Curve::Linear;
const S: Skill = Skill { intonation: 0.9, dexterity: 0.85, bowing: 0.88, composure: 0.8 };

pub static PARAMS: [ParamInfo; 13] = [
    ParamInfo::new(INTONATION, "Intonation", 0.0, 1.0, S.intonation, "%", LIN),
    ParamInfo::new(DEXTERITY, "Dexterity", 0.0, 1.0, S.dexterity, "%", LIN),
    ParamInfo::new(BOWING, "Bowing", 0.0, 1.0, S.bowing, "%", LIN),
    ParamInfo::new(COMPOSURE, "Composure", 0.0, 1.0, S.composure, "%", LIN),
    ParamInfo::new(EXPRESSION, "Expression", 0.0, 2.0, 1.0, "", LIN),
    ParamInfo::new(VIBRATO, "Vibrato", 0.0, 2.0, 1.0, "", LIN),
    ParamInfo::new(BREATH, "Breath", 0.0, 3.0, 1.0, "", LIN),
    ParamInfo::choice(PERFORMER, "Performer", &["Off", "On"], 1),
    ParamInfo::new(CHARACTER, "Character", 0.0, 1.0, 0.0, "%", LIN),
    ParamInfo::choice(INSTRUMENT, "Instrument", &["Cello", "Violin"], 0),
    ParamInfo::choice(PATCH, "Patch", &["Arco", "Pizz", "Trem"], 0),
    ParamInfo::choice(DOUBLE_STOPS, "Double stops", &["Off", "On"], 1),
    ParamInfo::new(TAKE, "Take", 1.0, 99.0, 1.0, "#", LIN),
];

static GROUPS: [ParamGroup; 3] = [
    ParamGroup { name: "Player", ids: 0..4 },
    ParamGroup { name: "Style", ids: 4..8 },
    ParamGroup { name: "Voice", ids: 8..13 },
];

/// The four player profiles (`Skill::profile`), as presets.
static PRESETS: [Preset; 4] = [
    ("Concert", &[(0, 0.9), (1, 0.85), (2, 0.88), (3, 0.8)]),
    ("Student", &[(0, 0.6), (1, 0.5), (2, 0.65), (3, 0.45)]),
    ("Virtuoso", &[(0, 0.98), (1, 0.98), (2, 0.96), (3, 0.92)]),
    ("Session", &[(0, 0.85), (1, 0.9), (2, 0.92), (3, 0.95)]),
];

pub static PLUGINS: &[PluginInfo] = &[PluginInfo {
    id: ID,
    name: "Octavia",
    melodic: true,
    params: &PARAMS,
    groups: &GROUPS,
    presets: &PRESETS,
    create: || Box::new(Octavia::new()),
}];

/// What the faceplate reads: the latest [`Snapshot`], and which instrument
/// plays (its strings, hand span and names come from `octavia-core`).
///
/// The audio thread never waits: it publishes with `try_lock` and skips a
/// block if the UI happens to be copying (one 60th of a second later it
/// tries again). The UI copies out with [`Telemetry::read`].
#[derive(Default)]
pub struct Telemetry {
    latest: Mutex<Snapshot>,
    instrument: AtomicU8,
}

impl Telemetry {
    pub fn read(&self) -> Snapshot {
        *self.latest.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 0 = cello, 1 = violin (the `Instrument` parameter).
    pub fn instrument(&self) -> u8 {
        self.instrument.load(Relaxed)
    }
}

pub struct Octavia {
    params: [f32; 13],
    /// One engine per instrument, both made in `activate` (switching must
    /// not allocate on the audio thread).
    engines: Vec<Engine>,
    active: usize,
    sample_rate: f64,
    /// The knobs changed since the engine last heard.
    dirty: bool,
    telemetry: Arc<Telemetry>,
}

impl Default for Octavia {
    fn default() -> Self {
        Self::new()
    }
}

impl Octavia {
    pub fn new() -> Self {
        Self {
            params: std::array::from_fn(|i| PARAMS[i].default),
            engines: Vec::new(),
            active: 0,
            sample_rate: 48_000.0,
            dirty: true,
            telemetry: Arc::default(),
        }
    }

    fn style(&self) -> Style {
        let p = |id: u32| self.params[id as usize];
        let on = p(PERFORMER) >= 0.5;
        let patch = p(PATCH).round() as u32;
        Style {
            expression: if on { p(EXPRESSION) } else { 0.0 },
            vibrato: if on { p(VIBRATO) } else { 0.0 },
            breath: p(BREATH),
            pizzicato: patch == 1,
            tremolo: patch == 2,
            skill: Skill {
                intonation: p(INTONATION),
                dexterity: p(DEXTERITY),
                bowing: p(BOWING),
                composure: p(COMPOSURE),
            },
            seed: p(TAKE).round().max(1.0) as u64,
        }
    }

    fn doubles(&self) -> bool {
        self.params[DOUBLE_STOPS as usize] >= 0.5
    }

    /// Hand changed knobs to the engine; switch instruments. REAL-TIME.
    fn apply(&mut self) {
        if !self.dirty || self.engines.is_empty() {
            return;
        }
        self.dirty = false;
        let want = (self.params[INSTRUMENT as usize].round() as usize).min(self.engines.len() - 1);
        if want != self.active {
            self.engines[self.active].all_off();
            self.active = want;
            self.telemetry.instrument.store(want as u8, Relaxed);
        }
        let (style, doubles) = (self.style(), self.doubles());
        let e = &mut self.engines[self.active];
        e.set_style(&style, doubles);
        e.set_bright(self.params[CHARACTER as usize]);
    }

    fn event(&mut self, event: Event) {
        let e = &mut self.engines[self.active];
        match event {
            Event::NoteOn { key, velocity, id } => e.note_on(id, key as f32, velocity),
            Event::NoteOff { key, id } => e.note_off(id, key as f32),
            Event::AllNotesOff => e.all_off(),
        }
    }
}

impl Plugin for Octavia {
    fn name(&self) -> &'static str {
        "Octavia"
    }

    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }

    fn groups(&self) -> &'static [ParamGroup] {
        &GROUPS
    }

    fn set_param(&mut self, id: u32, value: f32) {
        if let Some(p) = self.params.get_mut(id as usize) {
            if *p != value {
                *p = value;
                self.dirty = true;
            }
        }
    }

    fn activate(&mut self, sample_rate: f32) {
        let (style, bright) = (self.style(), self.params[CHARACTER as usize]);
        self.sample_rate = sample_rate as f64;
        self.engines =
            [cello(), violin()].iter().map(|inst| Engine::new(inst, &style, sample_rate, bright, false)).collect();
        self.active = (self.params[INSTRUMENT as usize].round() as usize).min(self.engines.len() - 1);
        self.telemetry.instrument.store(self.active as u8, Relaxed);
        self.dirty = true;
        self.apply();
    }

    /// A bit more than the performer's one second, so the edge of its
    /// window is never missing a note while a block plays.
    fn note_lookahead(&self) -> f64 {
        LOOKAHEAD + 0.1
    }

    fn upcoming(&mut self, notes: &[UpcomingNote]) {
        let doubles = self.doubles();
        let Some(e) = self.engines.get_mut(self.active) else {
            return;
        };
        let (now, sr) = (e.time(), self.sample_rate);
        e.forecast(
            notes.iter().map(|n| {
                let dynamic = octavia_core::score::velocity_to_dynamic(n.velocity);
                let note = Note {
                    // On its sample, as its note-on will be: exactly, so a
                    // note on a control frame isn't pushed past it by rounding.
                    start: ((now + n.start) * sr).round() / sr,
                    dur: n.len,
                    pitch: n.key as f32,
                    dynamic,
                    dynamic_end: dynamic,
                    articulation: octavia_core::Articulation::Auto,
                    slur: None,
                    double: None,
                };
                (n.id, note)
            }),
            doubles,
        );
    }

    fn process(&mut self, ctx: ProcessContext<'_>) {
        if self.engines.is_empty() {
            ctx.out_l.fill(0.0);
            ctx.out_r.fill(0.0);
            return;
        }
        self.apply();
        self.engines[self.active].settle();
        // Play up to each event, then let it happen: notes land on their sample.
        let n = ctx.out_l.len();
        let mut at = 0;
        for ev in ctx.events {
            let to = (ev.offset as usize).min(n);
            if to > at {
                self.engines[self.active].process(&mut ctx.out_l[at..to]);
                at = to;
            }
            self.event(ev.event);
        }
        if at < n {
            self.engines[self.active].process(&mut ctx.out_l[at..]);
        }
        ctx.out_r.copy_from_slice(ctx.out_l);
        if let Ok(mut latest) = self.telemetry.latest.try_lock() {
            *latest = self.engines[self.active].snapshot();
        }
    }

    fn telemetry(&self) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        Some(self.telemetry.clone())
    }
}

#[cfg(test)]
mod tests;
