//! What Chrysalis is doing right now, for its editor: written by the audio
//! thread at the end of every block (relaxed atomics: no locks, nothing
//! allocated), read by the UI thread whenever it draws. It describes the
//! newest sounding voice, so a held chord shows its last note.

use crate::params::dest;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

#[derive(Default)]
struct F(AtomicU32);

impl F {
    fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Relaxed);
    }
    fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Relaxed))
    }
}

pub struct Telemetry {
    voices: AtomicU32,
    /// Counts processed blocks: the editor knows the engine is alive.
    blocks: AtomicU32,
    /// Each oscillator's position in its table (0..1), modulation included.
    pos: [F; 2],
    /// Each LFO's phase (0..1) and value (0..1).
    lfo_phase: [F; 4],
    lfo_value: [F; 4],
    /// Each envelope's stage (0 off, 1 attack, 2 decay, 3 sustain, 4 release),
    /// progress through it (0..1) and level.
    env_stage: [AtomicU32; 4],
    env_t: [F; 4],
    env_level: [F; 4],
    /// The matrix's sum for every destination (see `params::dest`).
    mods: [F; dest::COUNT],
    /// Output peak of the last block.
    level: F,
}

/// One moment of it, copied out for drawing.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub voices: u32,
    pub blocks: u32,
    pub pos: [f32; 2],
    pub lfo_phase: [f32; 4],
    pub lfo_value: [f32; 4],
    pub env_stage: [u8; 4],
    pub env_t: [f32; 4],
    pub env_level: [f32; 4],
    pub mods: [f32; dest::COUNT],
    pub level: f32,
}

impl Default for Telemetry {
    fn default() -> Self {
        Telemetry {
            voices: AtomicU32::new(0),
            blocks: AtomicU32::new(0),
            pos: Default::default(),
            lfo_phase: Default::default(),
            lfo_value: Default::default(),
            env_stage: Default::default(),
            env_t: Default::default(),
            env_level: Default::default(),
            mods: std::array::from_fn(|_| F::default()),
            level: F::default(),
        }
    }
}

impl Default for Snapshot {
    fn default() -> Self {
        Snapshot {
            voices: 0,
            blocks: 0,
            pos: [0.0; 2],
            lfo_phase: [0.0; 4],
            lfo_value: [0.0; 4],
            env_stage: [0; 4],
            env_t: [0.0; 4],
            env_level: [0.0; 4],
            mods: [0.0; dest::COUNT],
            level: 0.0,
        }
    }
}

/// What the engine reports for one block.
pub(crate) struct Report {
    pub voices: u32,
    pub pos: [f32; 2],
    pub lfo_phase: [f32; 4],
    pub lfo_value: [f32; 4],
    pub env: [(u8, f32, f32); 4],
    pub mods: [f32; dest::COUNT],
    pub level: f32,
}

impl Telemetry {
    pub(crate) fn write(&self, r: &Report) {
        self.voices.store(r.voices, Relaxed);
        self.blocks.fetch_add(1, Relaxed);
        for o in 0..2 {
            self.pos[o].set(r.pos[o]);
        }
        for l in 0..4 {
            self.lfo_phase[l].set(r.lfo_phase[l]);
            self.lfo_value[l].set(r.lfo_value[l]);
            let (stage, t, level) = r.env[l];
            self.env_stage[l].store(stage as u32, Relaxed);
            self.env_t[l].set(t);
            self.env_level[l].set(level);
        }
        for (slot, m) in self.mods.iter().zip(r.mods) {
            slot.set(m);
        }
        self.level.set(r.level);
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            voices: self.voices.load(Relaxed),
            blocks: self.blocks.load(Relaxed),
            pos: [self.pos[0].get(), self.pos[1].get()],
            lfo_phase: std::array::from_fn(|l| self.lfo_phase[l].get()),
            lfo_value: std::array::from_fn(|l| self.lfo_value[l].get()),
            env_stage: std::array::from_fn(|e| self.env_stage[e].load(Relaxed) as u8),
            env_t: std::array::from_fn(|e| self.env_t[e].get()),
            env_level: std::array::from_fn(|e| self.env_level[e].get()),
            mods: std::array::from_fn(|d| self.mods[d].get()),
            level: self.level.get(),
        }
    }
}
