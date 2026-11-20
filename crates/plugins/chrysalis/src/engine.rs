//! The sound: per-voice oscillators, drive, filters, envelopes and
//! modulation, and the plugin that runs 16 of those voices.

use crate::curve::{bend_coeff, bend_k, Point, Shape, MAX_POINTS};
use crate::params::{dest, ft, lfo_mode, p, src, warp, PARAMS};
use crate::table::{builtin, Table, GUARD, IMPORTED};
use crate::telemetry::{Report, Telemetry};
use lyra_dsp::{poly_blep, round_fast, sin_turns, svf, svf_coeffs, tanh_fast, wrap, Rng, SYNC_BEATS};
use lyra_plugin::{Event, ParamGroup, ParamInfo, Plugin, ProcessContext, SampleData, Transport};
use std::f32::consts::SQRT_2;
use std::sync::Arc;

pub const MAX_VOICES: usize = 16;
pub const MAX_UNISON: usize = 16;
/// Modulation, pitch and filter coefficients update every this many samples.
pub const CR: usize = 32;
/// Notes remembered by mono mode.
const STACK: usize = 16;
/// Comb delay line (a power of two): low enough for an 12 Hz comb at 48 kHz.
const COMB: usize = 4096;

// ───────────────────────────── the patch, ready to play ─────────────────────────────

#[derive(Clone, Copy, Default)]
struct OscPrep {
    on: bool,
    table: usize,
    pos: f32,
    /// Octave + semitones + fine, in semitones.
    semis: f32,
    level: f32,
    pan: f32,
    warp_mode: u8,
    warp: f32,
    unison: usize,
    detune: f32,
    blend: f32,
    spread: f32,
    phase: f32,
    rand: f32,
}

#[derive(Clone, Copy, Default)]
struct FilterPrep {
    on: bool,
    kind: u8,
    cutoff: f32,
    reso: f32,
    drive: f32,
    key: f32,
    vowel: f32,
    mix: f32,
}

#[derive(Clone, Copy, Default)]
struct EnvPrep {
    /// Progress per sample of each stage (1 / its length in samples).
    a: f32,
    d: f32,
    s: f32,
    r: f32,
    /// Bend constants of the three curves.
    ka: f32,
    kd: f32,
    kr: f32,
}

#[derive(Clone, Copy, Default)]
struct LfoPrep {
    shape: Shape,
    /// See `params::lfo_mode`.
    mode: u8,
    /// 0 drawn, 1 random steps, 2 smooth random.
    style: u8,
    hz: f32,
    /// Synced length in quarter notes (0 = free rate).
    beats: f32,
}

#[derive(Clone, Copy, Default)]
struct Slot {
    src: usize,
    dest: usize,
    amt: f32,
    via: usize,
    bipolar: bool,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Prep {
    osc: [OscPrep; 2],
    sub_on: bool,
    sub_square: bool,
    sub_semis: f32,
    sub_level: f32,
    noise_on: bool,
    noise_color: f32,
    noise_level: f32,
    dist_mode: u8,
    dist_drive: f32,
    dist_mix: f32,
    filter: [FilterPrep; 2],
    parallel: bool,
    env: [EnvPrep; 4],
    lfo: [LfoPrep; 4],
    slots: [Slot; p::SLOT_COUNT],
    n_slots: usize,
    macros: [f32; 4],
    volume: f32,
    amp_vel: f32,
    mono: bool,
    glide_on: bool,
    glide_k: f32,
    glide_always: bool,
    /// Release progress per sample of a 5 ms "quick" release.
    quick: f32,
}

fn choice(v: f32) -> usize {
    v.round().max(0.0) as usize
}

impl Prep {
    fn compute(v: &[f32], sr: f32) -> Prep {
        use p::*;
        let mut q = Prep::default();
        for o in 0..OSCS {
            let at = |k: usize| v[osc(o, k)];
            q.osc[o] = OscPrep {
                on: at(OSC_ON) >= 0.5,
                table: choice(at(TABLE)),
                pos: at(POS),
                semis: (choice(at(OCTAVE)) as f32 - 3.0) * 12.0 + at(SEMI).round() + at(FINE) / 100.0,
                level: at(LEVEL),
                pan: at(PAN),
                warp_mode: choice(at(WARP_MODE)).min(7) as u8,
                warp: at(WARP),
                unison: (choice(at(UNISON)) + 1).min(MAX_UNISON),
                detune: at(DETUNE),
                blend: at(BLEND),
                spread: at(SPREAD),
                phase: at(PHASE),
                rand: at(RAND),
            };
        }
        q.sub_on = v[SUB_ON] >= 0.5;
        q.sub_square = v[SUB_SHAPE] >= 0.5;
        q.sub_semis = if v[SUB_OCT] >= 0.5 { -24.0 } else { -12.0 };
        q.sub_level = v[SUB_LEVEL];
        q.noise_on = v[NOISE_ON] >= 0.5;
        q.noise_color = v[NOISE_COLOR];
        q.noise_level = v[NOISE_LEVEL];
        q.dist_mode = choice(v[DIST_MODE]).min(4) as u8;
        q.dist_drive = v[DIST_DRIVE];
        q.dist_mix = v[DIST_MIX];
        for f in 0..2 {
            let at = |k: usize| v[filter(f, k)];
            q.filter[f] = FilterPrep {
                on: at(F_ON) >= 0.5,
                kind: choice(at(F_TYPE)).min(7) as u8,
                cutoff: at(F_CUTOFF),
                reso: at(F_RESO),
                drive: at(F_DRIVE),
                key: at(F_KEY),
                vowel: at(F_VOWEL),
                mix: at(F_MIX),
            };
        }
        q.parallel = v[ROUTING] >= 0.5;
        for e in 0..ENV_COUNT {
            let at = |k: usize| v[env(e, k)];
            let per = |s: f32| 1.0 / (s.max(1e-4) * sr);
            q.env[e] = EnvPrep {
                a: per(at(E_ATTACK)),
                d: per(at(E_DECAY)),
                s: at(E_SUSTAIN).clamp(0.0, 1.0),
                r: per(at(E_RELEASE)),
                ka: bend_coeff(at(E_A_CURVE)),
                kd: bend_coeff(at(E_D_CURVE)),
                kr: bend_coeff(at(E_R_CURVE)),
            };
        }
        for l in 0..LFO_COUNT {
            let n = (v[lfo(l, L_POINTS)].round() as usize).clamp(1, MAX_POINTS);
            let mut pts = [Point::default(); MAX_POINTS];
            for (k, pt) in pts.iter_mut().enumerate().take(n) {
                *pt = Point { x: v[point_x(l, k)], y: v[point_y(l, k)], bend: v[point_bend(l, k)] };
            }
            q.lfo[l] = LfoPrep {
                shape: Shape::new(&pts[..n]),
                mode: choice(v[lfo(l, L_MODE)]).min(3) as u8,
                style: choice(v[lfo_style(l)]).min(2) as u8,
                hz: v[lfo(l, L_RATE)],
                beats: SYNC_BEATS[choice(v[lfo(l, L_SYNC)]).min(SYNC_BEATS.len() - 1)],
            };
        }
        for s in 0..SLOT_COUNT {
            let at = |k: usize| v[slot(s, k)];
            let sl = Slot {
                src: choice(at(S_SRC)).min(15),
                dest: choice(at(S_DEST)).min(dest::COUNT - 1),
                amt: at(S_AMT),
                via: choice(at(S_VIA)).min(15),
                bipolar: at(S_BIPOLAR) >= 0.5,
            };
            if sl.src != 0 && sl.dest != 0 && sl.amt != 0.0 {
                q.slots[q.n_slots] = sl;
                q.n_slots += 1;
            }
        }
        for m in 0..4 {
            q.macros[m] = v[mac(m)];
        }
        q.volume = v[VOLUME];
        q.amp_vel = v[AMP_VEL];
        q.mono = v[MODE] >= 0.5;
        q.glide_on = v[GLIDE] > 0.0;
        let csr = sr / CR as f32;
        q.glide_k = if q.glide_on { 1.0 - (-3.0 / (v[GLIDE] * csr)).exp() } else { 1.0 };
        q.glide_always = v[GLIDE_MODE] >= 0.5;
        q.quick = 1.0 / (0.005 * sr);
        q
    }

    /// Render oscillator 2 first when oscillator 1 is frequency-modulated by it.
    fn osc_order(&self) -> [usize; 2] {
        if self.osc[0].warp_mode == warp::FM && self.osc[1].on {
            [1, 0]
        } else {
            [0, 1]
        }
    }
}

impl LfoPrep {
    /// The LFO's value (0..1) at phase `ph`; `(a, b)` are the random values
    /// of the previous and this cycle (the random styles use them).
    #[inline]
    fn value(&self, ph: f32, (a, b): (f32, f32)) -> f32 {
        match self.style {
            1 => b,
            // A cosine glide from the last random value to this one.
            2 => a + (b - a) * (0.5 - 0.5 * sin_turns(0.5 * ph + 0.25)),
            _ => self.shape.eval(ph),
        }
    }
}

/// A new phase for an LFO, and whether it wrapped into a new cycle (a jump
/// back of more than half a cycle: song-locking nudges it a little either way).
#[inline]
fn wrapped(old: f32, new: f32) -> bool {
    old - new > 0.5
}

// ───────────────────────────── envelopes ─────────────────────────────

const OFF: u8 = 0;
const ATTACK: u8 = 1;
const DECAY: u8 = 2;
const SUSTAIN: u8 = 3;
const RELEASE: u8 = 4;

#[derive(Clone, Copy, Default)]
struct Env {
    stage: u8,
    /// Progress through the stage, 0..1.
    t: f32,
    /// Where the stage started (attack and release start from wherever the level was).
    from: f32,
    level: f32,
}

impl Env {
    fn start(&mut self) {
        self.stage = ATTACK;
        self.t = 0.0;
        self.from = self.level;
    }
    fn release(&mut self) {
        if self.stage != OFF {
            self.stage = RELEASE;
            self.t = 0.0;
            self.from = self.level;
        }
    }
    /// Advance by `n` samples (`quick` = a 5 ms release, for stolen and stopped voices).
    #[inline]
    fn tick(&mut self, e: &EnvPrep, n: f32, quick: f32) -> f32 {
        match self.stage {
            ATTACK => {
                self.t += e.a * n;
                if self.t >= 1.0 {
                    self.stage = DECAY;
                    self.t = 0.0;
                    self.level = 1.0;
                } else {
                    self.level = self.from + (1.0 - self.from) * bend_k(self.t, e.ka);
                }
            }
            DECAY => {
                self.t += e.d * n;
                if self.t >= 1.0 {
                    self.stage = SUSTAIN;
                    self.level = e.s;
                } else {
                    self.level = 1.0 + (e.s - 1.0) * bend_k(self.t, e.kd);
                }
            }
            SUSTAIN => self.level = e.s,
            RELEASE => {
                self.t += if quick > 0.0 { quick } else { e.r } * n;
                if self.t >= 1.0 {
                    self.stage = OFF;
                    self.level = 0.0;
                } else {
                    self.level = self.from * (1.0 - bend_k(self.t, e.kr));
                }
            }
            _ => self.level = 0.0,
        }
        self.level
    }
}

// ───────────────────────────── voices ─────────────────────────────

/// One oscillator of one voice: its unison voices and where it is in the table.
#[derive(Clone, Copy)]
struct OscState {
    ph: [f32; MAX_UNISON],
    inc: [f32; MAX_UNISON],
    /// Each unison voice's gains to the left, the right, and to the FM signal.
    gl: [f32; MAX_UNISON],
    gr: [f32; MAX_UNISON],
    gm: [f32; MAX_UNISON],
    n: usize,
    /// Mip level for this block; `None` = too high to play.
    level: Option<usize>,
    /// Fractional frame, ramped per sample toward the control-rate target.
    fpos: f32,
    fpos_step: f32,
    gain: f32,
    gain_step: f32,
    /// Warp constants for this block (meaning depends on the mode).
    wa: f32,
    wb: f32,
    /// This block's raw output (all unison voices, before level): the FM signal.
    fm: [f32; CR],
}

impl Default for OscState {
    fn default() -> Self {
        OscState {
            ph: [0.0; MAX_UNISON],
            inc: [0.0; MAX_UNISON],
            gl: [0.0; MAX_UNISON],
            gr: [0.0; MAX_UNISON],
            gm: [0.0; MAX_UNISON],
            n: 1,
            level: None,
            fpos: 0.0,
            fpos_step: 0.0,
            gain: 0.0,
            gain_step: 0.0,
            wa: 0.0,
            wb: 0.0,
            fm: [0.0; CR],
        }
    }
}

/// Filter state for one filter of one voice, both sides.
struct FilterState {
    /// Two SVF stages (a 24 dB low-pass uses both), per side.
    s: [[(f32, f32); 2]; 2],
    /// The formant filter's three band-passes, per side.
    f: [[(f32, f32); 3]; 2],
    comb: Box<[[f32; COMB]; 2]>,
    w: usize,
    // This block's coefficients.
    c1: (f32, f32, f32),
    c2: (f32, f32, f32),
    k: f32,
    fc: [(f32, f32, f32); 3],
    fk: [f32; 3],
    fg: [f32; 3],
    delay: f32,
    fb: f32,
    norm: f32,
    drive: f32,
    mix: f32,
    kind: u8,
}

impl FilterState {
    fn new() -> Self {
        FilterState {
            s: [[(0.0, 0.0); 2]; 2],
            f: [[(0.0, 0.0); 3]; 2],
            comb: Box::new([[0.0; COMB]; 2]),
            w: 0,
            c1: (0.0, 0.0, 0.0),
            c2: (0.0, 0.0, 0.0),
            k: 2.0,
            fc: [(0.0, 0.0, 0.0); 3],
            fk: [1.0; 3],
            fg: [0.0; 3],
            delay: 1.0,
            fb: 0.0,
            norm: 1.0,
            drive: 0.0,
            mix: 1.0,
            kind: 0,
        }
    }

    fn reset(&mut self) {
        self.s = [[(0.0, 0.0); 2]; 2];
        self.f = [[(0.0, 0.0); 3]; 2];
        for side in self.comb.iter_mut() {
            side.fill(0.0);
        }
    }

    /// This block's coefficients: cutoff in Hz (already key-tracked and modulated).
    #[allow(clippy::too_many_arguments)]
    fn prepare(&mut self, kind: u8, cutoff: f32, reso: f32, vowel: f32, drive: f32, mix: f32, sr: f32) {
        self.kind = kind;
        self.drive = drive;
        self.mix = mix;
        let fc = cutoff.clamp(20.0, sr * 0.45);
        match kind {
            ft::FORMANT => {
                // Cutoff shifts the formants: 1 kHz leaves them alone.
                let shift = (cutoff / 1000.0).sqrt().clamp(0.5, 2.0);
                let narrow = 2f32.powf(-2.0 * reso);
                for (i, &(hz, gain, bw)) in crate::table::vowel_at(vowel).iter().enumerate() {
                    let f = (hz * shift).min(sr * 0.45);
                    let k = (bw * narrow * shift / f).clamp(0.01, 2.0);
                    self.fc[i] = svf_coeffs(f, sr, k);
                    self.fk[i] = k;
                    self.fg[i] = gain;
                }
            }
            ft::COMB_PLUS | ft::COMB_MINUS => {
                self.delay = (sr / fc).clamp(1.0, (COMB - 2) as f32);
                self.fb = 0.97 * reso.clamp(0.0, 1.0);
                self.norm = (1.0 - self.fb * self.fb).sqrt();
            }
            _ => {
                self.k = 2.0 * (1.0 - 0.985 * reso.clamp(0.0, 1.0));
                self.c1 = svf_coeffs(fc, sr, self.k);
                self.c2 = svf_coeffs(fc, sr, SQRT_2);
            }
        }
    }

    #[inline]
    fn run(&mut self, side: usize, x: f32) -> f32 {
        let dry = x;
        let x = if self.drive > 0.0 { tanh_fast(x * (1.0 + 7.0 * self.drive)) } else { x };
        let y = match self.kind {
            ft::LOW12 => svf(&mut self.s[side][0], x, self.c1).1,
            ft::LOW24 => {
                let a = svf(&mut self.s[side][0], x, self.c1).1;
                svf(&mut self.s[side][1], a, self.c2).1
            }
            ft::HIGH12 => {
                let (b, l) = svf(&mut self.s[side][0], x, self.c1);
                x - self.k * b - l
            }
            ft::BAND => self.k * svf(&mut self.s[side][0], x, self.c1).0,
            ft::NOTCH => x - self.k * svf(&mut self.s[side][0], x, self.c1).0,
            ft::FORMANT => {
                let mut y = 0.0;
                for i in 0..3 {
                    y += self.fg[i] * self.fk[i] * svf(&mut self.f[side][i], x, self.fc[i]).0;
                }
                2.0 * y
            }
            _ => {
                let buf = &mut self.comb[side];
                let rd = self.w as f32 - self.delay + COMB as f32;
                let j = rd as usize;
                let fr = rd - j as f32;
                let (a, b) = (buf[j & (COMB - 1)], buf[(j + 1) & (COMB - 1)]);
                let delayed = a + (b - a) * fr;
                let sign = if self.kind == ft::COMB_PLUS { 1.0 } else { -1.0 };
                let y = x + sign * self.fb * delayed;
                buf[self.w] = y;
                y * self.norm
            }
        };
        dry + (y - dry) * self.mix
    }

    /// The comb's write head moves once per sample, for both sides.
    #[inline]
    fn advance(&mut self) {
        self.w = (self.w + 1) & (COMB - 1);
    }
}

struct Voice {
    active: bool,
    gate: bool,
    /// Released quickly (stolen, or stop).
    quick: bool,
    key: u8,
    serial: u64,
    velocity: f32,
    /// This note's random number (0..1).
    rnd: f32,
    /// Current pitch (gliding toward `target`), in semitones.
    semi: f32,
    target: f32,
    osc: [OscState; 2],
    sub_ph: f32,
    sub_inc: f32,
    sub_gain: f32,
    sub_step: f32,
    rng: Rng,
    noise_lp: [f32; 2],
    noise_a: f32,
    noise_hp: bool,
    noise_gain: f32,
    noise_step: f32,
    dist_gain: f32,
    dist_mix: f32,
    crush_steps: f32,
    crush_hold: usize,
    crush_count: usize,
    crush_val: [f32; 2],
    filter: [FilterState; 2],
    env: [Env; 4],
    lfo_ph: [f32; 4],
    lfo: [f32; 4],
    /// Each per-note LFO's random values: the previous cycle's and this one's.
    lfo_r: [(f32, f32); 4],
    /// The mod matrix's sums, per destination (last control tick).
    m: [f32; dest::COUNT],
    /// Velocity's share of the loudness.
    amp: f32,
    out_l: f32,
    out_r: f32,
    step_l: f32,
    step_r: f32,
}

/// Equal-power pan, scaled so the centre is unity on both sides.
#[inline]
fn pan_gains(pan: f32) -> (f32, f32) {
    let x = (pan.clamp(-1.0, 1.0) + 1.0) * 0.125; // 0 .. ¼ turn
    (SQRT_2 * sin_turns(x + 0.25), SQRT_2 * sin_turns(x))
}

fn note_hz(semis: f32) -> f32 {
    440.0 * ((semis - 69.0) / 12.0).exp2()
}

impl Voice {
    fn new(seed: u32) -> Self {
        Voice {
            active: false,
            gate: false,
            quick: false,
            key: 60,
            serial: 0,
            velocity: 0.0,
            rnd: 0.0,
            semi: 60.0,
            target: 60.0,
            osc: [OscState::default(); 2],
            sub_ph: 0.0,
            sub_inc: 0.0,
            sub_gain: 0.0,
            sub_step: 0.0,
            rng: Rng::new(seed),
            noise_lp: [0.0; 2],
            noise_a: 1.0,
            noise_hp: false,
            noise_gain: 0.0,
            noise_step: 0.0,
            dist_gain: 1.0,
            dist_mix: 0.0,
            crush_steps: 1.0,
            crush_hold: 1,
            crush_count: 0,
            crush_val: [0.0; 2],
            filter: [FilterState::new(), FilterState::new()],
            env: [Env::default(); 4],
            lfo_ph: [0.0; 4],
            lfo: [0.0; 4],
            lfo_r: [(0.5, 0.5); 4],
            m: [0.0; dest::COUNT],
            amp: 1.0,
            out_l: 0.0,
            out_r: 0.0,
            step_l: 0.0,
            step_r: 0.0,
        }
    }

    fn release(&mut self) {
        self.gate = false;
        for e in self.env.iter_mut() {
            e.release();
        }
    }

    /// The matrix sources for this voice, 0..1.
    fn sources(&self, q: &Prep) -> [f32; 16] {
        let mut s = [0.0; 16];
        s[src::LFO..src::LFO + 4].copy_from_slice(&self.lfo);
        for e in 0..4 {
            s[src::ENV + e] = self.env[e].level;
        }
        s[src::VELOCITY] = self.velocity;
        s[src::KEY] = self.key as f32 / 127.0;
        s[src::RANDOM] = self.rnd;
        s[src::MACRO..src::MACRO + 4].copy_from_slice(&q.macros);
        s
    }

    /// The control tick: LFOs, mod envelopes, the matrix, and every derived
    /// value for the next block. `global` = the shared (free) LFOs' values;
    /// `snap` jumps the ramps straight to their targets (a fresh note).
    #[allow(clippy::needless_range_loop)] // four LFOs, indexed across several arrays
    fn ctrl(&mut self, q: &Prep, tables: [&Table; 2], global: [f32; 4], sr: f32, snap: bool) {
        // LFOs: shared ones come from the plugin; per-note ones run here.
        for l in 0..4 {
            let lp = &q.lfo[l];
            if lfo_mode::shared(lp.mode) {
                self.lfo[l] = global[l];
                continue;
            }
            // A one-shot holds the end of its curve.
            let ph = if lp.mode == lfo_mode::ONE_SHOT { self.lfo_ph[l].min(0.999_999) } else { self.lfo_ph[l] };
            self.lfo[l] = lp.value(ph, self.lfo_r[l]);
        }
        // Mod envelopes (2–4) at control rate; the amp envelope runs per sample.
        for e in 1..4 {
            self.env[e].tick(&q.env[e], CR as f32, 0.0);
        }
        // The matrix.
        let s = self.sources(q);
        self.m = [0.0; dest::COUNT];
        for sl in &q.slots[..q.n_slots] {
            let mut x = s[sl.src];
            if sl.bipolar {
                x = 2.0 * x - 1.0;
            }
            let via = if sl.via == 0 { 1.0 } else { s[sl.via] };
            self.m[sl.dest] += sl.amt * x * via;
        }
        let m = &self.m;

        // Pitch (with glide).
        if self.semi != self.target {
            self.semi += (self.target - self.semi) * q.glide_k;
            if (self.target - self.semi).abs() < 1e-4 {
                self.semi = self.target;
            }
        }
        let pitch = self.semi + 24.0 * m[dest::PITCH];

        for o in 0..2 {
            let op = &q.osc[o];
            let st = &mut self.osc[o];
            if !op.on {
                st.level = None;
                st.fm = [0.0; CR];
                continue;
            }
            let table = tables[o];
            let md = |k: usize| m[dest::osc(o, k)];
            let hz = note_hz(pitch + op.semis + 24.0 * md(dest::O_PITCH));
            let detune = (op.detune + md(dest::O_DETUNE)).clamp(0.0, 1.0);
            let cents = 100.0 * detune * detune;
            let n = op.unison;
            st.n = n;
            // Unison voices sit evenly from −cents to +cents: a geometric series of ratios.
            let (mut r, q_ratio) = if n > 1 {
                ((-cents / 1200.0).exp2(), (2.0 * cents / (1200.0 * (n - 1) as f32)).exp2())
            } else {
                (1.0, 1.0)
            };
            let blend = (op.blend + md(dest::O_BLEND)).clamp(0.0, 1.0);
            let pan = op.pan + md(dest::O_PAN);
            let mut norm = 0.0;
            let centre = |k: usize| n <= 2 || 2 * k + 1 == n || (n.is_multiple_of(2) && (k == n / 2 - 1 || k == n / 2));
            for k in 0..n {
                st.inc[k] = hz * r / sr;
                r *= q_ratio;
                let g = if centre(k) { 1.0 } else { blend };
                st.gm[k] = g;
                norm += g * g;
            }
            let norm = 1.0 / norm.sqrt().max(1e-6);
            for k in 0..n {
                st.gm[k] *= norm;
                // Detune rank k → alternate sides, so both sides get low and high detunes.
                let u = if n > 1 { -1.0 + 2.0 * k as f32 / (n - 1) as f32 } else { 0.0 };
                let side = if k % 2 == 0 { -u.abs() } else { u.abs() };
                let (gl, gr) = pan_gains(pan + op.spread * side);
                st.gl[k] = st.gm[k] * gl;
                st.gr[k] = st.gm[k] * gr;
            }
            // Warp.
            let w = (op.warp + md(dest::O_WARP)).clamp(0.0, 1.0);
            (st.wa, st.wb) = warp_consts(op.warp_mode, w);
            // Sync plays the table faster: pick the level for that pitch.
            let top = st.inc[n - 1] * sr * if op.warp_mode == warp::SYNC { st.wa } else { 1.0 };
            st.level = Table::level_for(top, sr);
            // Position and level, ramped across the block.
            let frames = (table.frames() - 1) as f32;
            let fpos = ((op.pos + md(dest::O_POS)).clamp(0.0, 1.0)) * frames;
            let gain = op.level * (1.0 + md(dest::O_LEVEL)).clamp(0.0, 2.0);
            if snap {
                st.fpos = fpos;
                st.gain = gain;
            }
            st.fpos_step = (fpos - st.fpos) / CR as f32;
            st.gain_step = (gain - st.gain) / CR as f32;
        }

        // Sub and noise.
        self.sub_inc = note_hz(pitch + q.sub_semis) / sr;
        let sub_gain = if q.sub_on { q.sub_level * (1.0 + m[dest::SUB_LEVEL]).clamp(0.0, 2.0) } else { 0.0 };
        let noise_gain = if q.noise_on { q.noise_level * (1.0 + m[dest::NOISE_LEVEL]).clamp(0.0, 2.0) } else { 0.0 };
        let color = (q.noise_color + 2.0 * m[dest::NOISE_COLOR]).clamp(-1.0, 1.0);
        let (fc, hp) =
            if color < 0.0 { (18_000.0 * (6.5 * color).exp2(), false) } else { (20.0 * (8.6 * color).exp2(), true) };
        let a = if !hp && color > -1e-3 { 1.0 } else { (1.0 - (-std::f32::consts::TAU * fc / sr).exp()).min(1.0) };
        self.noise_a = a;
        self.noise_hp = hp && color > 1e-3;
        // Keep the loudness of white noise whatever the colour.
        let var = if self.noise_hp { 1.0 - 2.0 * a + a / (2.0 - a) } else { a / (2.0 - a) };
        let comp = (1.0 / var.max(1e-4).sqrt()).min(8.0);
        let noise_gain = noise_gain * comp;
        if snap {
            self.sub_gain = sub_gain;
            self.noise_gain = noise_gain;
        }
        self.sub_step = (sub_gain - self.sub_gain) / CR as f32;
        self.noise_step = (noise_gain - self.noise_gain) / CR as f32;

        // Drive.
        let drive = (q.dist_drive + m[dest::DRIVE]).clamp(0.0, 1.0);
        self.dist_mix = if q.dist_mode == 0 { 0.0 } else { (q.dist_mix + m[dest::DRIVE_MIX]).clamp(0.0, 1.0) };
        (self.dist_gain, self.crush_steps, self.crush_hold) = drive_consts(drive);

        // Filters.
        for f in 0..2 {
            let fp = &q.filter[f];
            if !fp.on {
                continue;
            }
            let md = |k: usize| m[dest::filter(f, k)];
            let cutoff = fp.cutoff * (fp.key * (self.semi - 60.0) / 12.0 + 6.0 * md(dest::F_CUTOFF)).exp2();
            self.filter[f].prepare(
                fp.kind,
                cutoff,
                (fp.reso + md(dest::F_RESO)).clamp(0.0, 1.0),
                (fp.vowel + md(dest::F_VOWEL)).clamp(0.0, 1.0),
                (fp.drive + md(dest::F_DRIVE)).clamp(0.0, 1.0),
                (fp.mix + md(dest::F_MIX)).clamp(0.0, 1.0),
                sr,
            );
        }

        // Output: velocity, volume and pan.
        let gain = self.amp * (1.0 + m[dest::VOLUME]).max(0.0);
        let (pl, pr) = pan_gains(m[dest::PAN]);
        let (tl, tr) = (gain * pl, gain * pr);
        if snap {
            self.out_l = tl;
            self.out_r = tr;
        }
        self.step_l = (tl - self.out_l) / CR as f32;
        self.step_r = (tr - self.out_r) / CR as f32;
    }

    /// Advance the per-note LFOs by one control tick.
    #[allow(clippy::needless_range_loop)]
    fn lfo_tick(&mut self, q: &Prep, inc: [f32; 4]) {
        for l in 0..4 {
            let lp = &q.lfo[l];
            if lfo_mode::shared(lp.mode) {
                continue;
            }
            let ph = self.lfo_ph[l] + inc[l] * (4.0 * self.m[dest::LFO_RATE + l]).exp2();
            let new = if lp.mode == lfo_mode::ONE_SHOT { ph.min(1.0) } else { ph - ph.floor() };
            if wrapped(self.lfo_ph[l], new) {
                self.lfo_r[l] = (self.lfo_r[l].1, self.rng.bipolar() * 0.5 + 0.5);
            }
            self.lfo_ph[l] = new;
        }
    }

    /// Render `len` (≤ CR) samples and add them to `out_l` / `out_r`.
    fn render(&mut self, q: &Prep, tables: [&Table; 2], out_l: &mut [f32], out_r: &mut [f32]) {
        let len = out_l.len();
        let mut l = [0.0f32; CR];
        let mut r = [0.0f32; CR];
        let order = q.osc_order();
        for o in order {
            let (a, b) = self.osc.split_at_mut(1);
            let (me, other) = if o == 0 { (&mut a[0], &b[0]) } else { (&mut b[0], &a[0]) };
            if !q.osc[o].on {
                continue;
            }
            let Some(level) = me.level else {
                me.fm = [0.0; CR];
                continue;
            };
            let other_fm = other.fm;
            match q.osc[o].warp_mode {
                warp::BEND_UP | warp::BEND_DOWN => {
                    osc_block::<{ warp::BEND_UP }>(me, &other_fm, tables[o], level, len, &mut l, &mut r)
                }
                warp::SYNC => osc_block::<{ warp::SYNC }>(me, &other_fm, tables[o], level, len, &mut l, &mut r),
                warp::MIRROR => osc_block::<{ warp::MIRROR }>(me, &other_fm, tables[o], level, len, &mut l, &mut r),
                warp::QUANTIZE => osc_block::<{ warp::QUANTIZE }>(me, &other_fm, tables[o], level, len, &mut l, &mut r),
                warp::FM => osc_block::<{ warp::FM }>(me, &other_fm, tables[o], level, len, &mut l, &mut r),
                warp::PWM => osc_block::<{ warp::PWM }>(me, &other_fm, tables[o], level, len, &mut l, &mut r),
                _ => osc_block::<{ warp::OFF }>(me, &other_fm, tables[o], level, len, &mut l, &mut r),
            }
        }
        if q.sub_on {
            for i in 0..len {
                self.sub_gain += self.sub_step;
                let ph = self.sub_ph;
                let s = if q.sub_square {
                    let mut s = if ph < 0.5 { 1.0 } else { -1.0 };
                    s += poly_blep(ph, self.sub_inc);
                    s -= poly_blep(wrap(ph + 0.5), self.sub_inc);
                    s * 0.8
                } else {
                    sin_turns(ph)
                };
                let s = s * self.sub_gain;
                l[i] += s;
                r[i] += s;
                let p2 = ph + self.sub_inc;
                self.sub_ph = if p2 >= 1.0 { p2 - 1.0 } else { p2 };
            }
        }
        if q.noise_on {
            let a = self.noise_a;
            for i in 0..len {
                self.noise_gain += self.noise_step;
                for (side, buf) in [&mut l, &mut r].into_iter().enumerate() {
                    let x = self.rng.bipolar();
                    self.noise_lp[side] += a * (x - self.noise_lp[side]);
                    let y = if self.noise_hp { x - self.noise_lp[side] } else { self.noise_lp[side] };
                    buf[i] += y * self.noise_gain;
                }
            }
        }
        if self.dist_mix > 0.0 {
            let (g, mix) = (self.dist_gain, self.dist_mix);
            for i in 0..len {
                if q.dist_mode == 4 {
                    // Crush: fewer bits and a lower sample rate the harder you drive.
                    if self.crush_count == 0 {
                        for (side, x) in [l[i], r[i]].into_iter().enumerate() {
                            self.crush_val[side] = drive_shape(4, g, self.crush_steps, x);
                        }
                    }
                    self.crush_count = (self.crush_count + 1) % self.crush_hold;
                }
                for (side, x) in [&mut l[i], &mut r[i]].into_iter().enumerate() {
                    let y = if q.dist_mode == 4 { self.crush_val[side] } else { drive_shape(q.dist_mode, g, 0.0, *x) };
                    *x += (y - *x) * mix;
                }
            }
        }
        let [f1, f2] = &mut self.filter;
        let (on1, on2) = (q.filter[0].on, q.filter[1].on);
        if on1 || on2 {
            for i in 0..len {
                for (side, buf) in [&mut l, &mut r].into_iter().enumerate() {
                    let x = buf[i];
                    buf[i] = match (on1, on2, q.parallel) {
                        (true, true, true) => f1.run(side, x) + f2.run(side, x),
                        (true, true, false) => f2.run(side, f1.run(side, x)),
                        (true, false, _) => f1.run(side, x),
                        _ => f2.run(side, x),
                    };
                }
                f1.advance();
                f2.advance();
            }
        }
        let quick = if self.quick { q.quick } else { 0.0 };
        let e0 = q.env[0];
        for i in 0..len {
            let a = self.env[0].tick(&e0, 1.0, quick);
            self.out_l += self.step_l;
            self.out_r += self.step_r;
            out_l[i] += l[i] * a * self.out_l;
            out_r[i] += r[i] * a * self.out_r;
        }
        if self.env[0].stage == OFF {
            self.active = false;
        }
    }

    fn loudness(&self) -> f32 {
        self.env[0].level * self.amp
    }
}

/// A warp's two constants for amount `w` (0..1); what they mean depends on the mode.
pub fn warp_consts(mode: u8, w: f32) -> (f32, f32) {
    match mode {
        warp::BEND_UP => (1.0 / (1.0 + 8.0 * w), 0.0),
        warp::BEND_DOWN => (1.0 + 8.0 * w, 0.0),
        warp::SYNC => (1.0 + 7.0 * w, 0.0),
        // Forward up to c, then back: 1/c and 1/(1 − c); nothing at w = 0.
        warp::MIRROR => {
            let c = 1.0 - 0.5 * w;
            if w > 0.0 {
                (1.0 / c, 1.0 / (1.0 - c))
            } else {
                (1.0, 0.0)
            }
        }
        warp::QUANTIZE => {
            let steps = (11.0 - 9.5 * w).exp2().round();
            (steps, 1.0 / steps)
        }
        warp::FM => (1.5 * w, 0.0),
        warp::PWM => {
            let pw = 0.5 - 0.48 * w;
            (0.5 / pw, 0.5 / (1.0 - pw))
        }
        _ => (0.0, 0.0),
    }
}

/// Where warp `mode` at amount `w` reads the table for phase `p`, as the
/// oscillator does (FM, with no modulator here, leaves the phase alone). For pictures.
pub fn warp_preview(mode: u8, w: f32, p: f32) -> f32 {
    let (wa, wb) = warp_consts(mode, w);
    match mode {
        warp::BEND_UP | warp::BEND_DOWN => warp_phase::<{ warp::BEND_UP }>(p, wa, wb, 0.0),
        warp::SYNC => warp_phase::<{ warp::SYNC }>(p, wa, wb, 0.0),
        warp::MIRROR => warp_phase::<{ warp::MIRROR }>(p, wa, wb, 0.0),
        warp::QUANTIZE => warp_phase::<{ warp::QUANTIZE }>(p, wa, wb, 0.0),
        warp::PWM => warp_phase::<{ warp::PWM }>(p, wa, wb, 0.0),
        _ => p,
    }
}

/// Drive amount (0..1) → (input gain, crush levels, crush hold in samples).
pub fn drive_consts(drive: f32) -> (f32, f32, usize) {
    let d = drive.clamp(0.0, 1.0);
    (lyra_plugin::db_to_gain(36.0 * d), (16.0 - 13.0 * d).exp2() * 0.5, 1 + (d * d * 15.0) as usize)
}

/// The drive's curve: 1 tanh, 2 hard clip, 3 fold, 4 crush (levels only; the
/// voice also holds samples). `gain` and `steps` from [`drive_consts`].
#[inline]
pub fn drive_shape(mode: u8, gain: f32, steps: f32, x: f32) -> f32 {
    let d = x * gain;
    match mode {
        1 => tanh_fast(d),
        2 => d.clamp(-1.0, 1.0),
        3 => sin_turns(d * 0.25),
        4 => round_fast(x * steps) / steps,
        _ => x,
    }
}

/// The phase warp of one unison voice; `fm` is the other oscillator's signal.
#[inline(always)]
fn warp_phase<const W: u8>(p: f32, wa: f32, wb: f32, fm: f32) -> f32 {
    match W {
        warp::BEND_UP => p / (p + wa * (1.0 - p)),
        warp::SYNC => {
            let x = p * wa;
            x - (x as i32) as f32
        }
        warp::MIRROR => {
            let fwd = p * wa;
            if wb == 0.0 || fwd < 1.0 {
                fwd
            } else {
                (1.0 - p) * wb
            }
        }
        warp::QUANTIZE => (p * wa) as i32 as f32 * wb,
        warp::FM => wrap(p + wa * fm),
        warp::PWM => {
            let pw = 0.5 / wa;
            if p < pw {
                p * wa
            } else {
                0.5 + (p - pw) * wb
            }
        }
        _ => p,
    }
}

/// Levels this full (notes below ~190 Hz) read the table with cubic
/// interpolation: with that many harmonics, linear would leave images at
/// −56 to −73 dB. Above, linear is already near −80 dB and costs half.
pub const CUBIC_LEVELS: usize = 7;

/// 4-point Catmull-Rom between `x0` and `x1` at `t`.
#[inline(always)]
fn cubic(xm: f32, x0: f32, x1: f32, x2: f32, t: f32) -> f32 {
    let c1 = 0.5 * (x1 - xm);
    let c2 = xm - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
    let c3 = 0.5 * (x2 - xm) + 1.5 * (x0 - x1);
    ((c3 * t + c2) * t + c1) * t + x0
}

/// One oscillator's block: every unison voice, two frames, interpolated in
/// both. Sample by sample, with the unison voices inside: they are
/// independent, so the CPU works on several at once.
#[allow(clippy::too_many_arguments)]
fn osc_block<const W: u8>(
    st: &mut OscState,
    other_fm: &[f32; CR],
    table: &Table,
    level: usize,
    len: usize,
    l: &mut [f32; CR],
    r: &mut [f32; CR],
) {
    if level < CUBIC_LEVELS {
        osc_lookup::<W, true>(st, other_fm, table, level, len, l, r)
    } else {
        osc_lookup::<W, false>(st, other_fm, table, level, len, l, r)
    }
}

#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn osc_lookup<const W: u8, const CUBIC: bool>(
    st: &mut OscState,
    other_fm: &[f32; CR],
    table: &Table,
    level: usize,
    len: usize,
    l: &mut [f32; CR],
    r: &mut [f32; CR],
) {
    let size = table.level(level).size;
    let stride = size + GUARD;
    let data = table.level_data(level);
    let frames = table.frames();
    let last = (frames - 1) as f32;
    let fsize = size as f32;
    let n = st.n;
    let (wa, wb) = (st.wa, st.wb);
    let mut ph = st.ph;
    let inc = st.inc;
    let (gl, gr, gm) = (st.gl, st.gr, st.gm);
    for i in 0..len {
        st.fpos += st.fpos_step;
        st.gain += st.gain_step;
        let fp = st.fpos.clamp(0.0, last);
        let f0 = fp as usize;
        let f1 = (f0 + 1).min(frames - 1);
        let ff = fp - f0 as f32;
        // Each frame with its guards: sample m of the cycle is at m + 1.
        let a = &data[f0 * stride..(f0 + 1) * stride];
        let b = &data[f1 * stride..(f1 + 1) * stride];
        let fm = other_fm[i];
        let (mut sl, mut sr, mut sm) = (0.0f32, 0.0f32, 0.0f32);
        for k in 0..n {
            let mut x = ph[k] + inc[k];
            if x >= 1.0 {
                x -= 1.0;
            }
            ph[k] = x;
            let p = warp_phase::<W>(x, wa, wb, fm);
            let t = p * fsize;
            let j = (t as usize).min(size - 1);
            let fr = t - j as f32;
            let (sa, sb) = if CUBIC {
                let (ca, cb) = (&a[j..j + 4], &b[j..j + 4]);
                (cubic(ca[0], ca[1], ca[2], ca[3], fr), cubic(cb[0], cb[1], cb[2], cb[3], fr))
            } else {
                let (a0, a1, b0, b1) = (a[j + 1], a[j + 2], b[j + 1], b[j + 2]);
                (a0 + (a1 - a0) * fr, b0 + (b1 - b0) * fr)
            };
            let s = sa + (sb - sa) * ff;
            sl += s * gl[k];
            sr += s * gr[k];
            sm += s * gm[k];
        }
        l[i] += sl * st.gain;
        r[i] += sr * st.gain;
        st.fm[i] = sm;
    }
    st.ph = ph;
}

// ───────────────────────────── the plugin ─────────────────────────────

pub struct Chrysalis {
    p: Vec<f32>,
    q: Box<Prep>,
    dirty: bool,
    sr: f32,
    voices: Vec<Voice>,
    serial: u64,
    rng: Rng,
    stack: [u8; STACK],
    stack_len: usize,
    imported: Option<Arc<Table>>,
    /// Shared (free-mode) LFO phases and values.
    lfo_ph: [f32; 4],
    lfo: [f32; 4],
    /// Phase per control tick, this block.
    lfo_inc: [f32; 4],
    /// The shared LFOs' random values: the previous cycle's and this one's.
    lfo_r: [(f32, f32); 4],
    transport: Transport,
    ctrl_left: usize,
    dc: [(f32, f32); 2],
    /// What the editor sees (written at the end of every block).
    live: Arc<Telemetry>,
}

impl Default for Chrysalis {
    fn default() -> Self {
        Self::new()
    }
}

impl Chrysalis {
    pub fn new() -> Self {
        builtin(); // built here, on the UI thread, so the audio thread only reads it
        let p = lyra_plugin::default_params(&PARAMS);
        let sr = 48_000.0;
        Chrysalis {
            q: Box::new(Prep::compute(&p, sr)),
            p,
            dirty: false,
            sr,
            voices: (0..MAX_VOICES)
                .map(|i| Voice::new(0x5eed_1234 ^ (i as u32 + 1).wrapping_mul(0x9e37_79b9)))
                .collect(),
            serial: 0,
            rng: Rng::new(0xc4_7a5),
            stack: [0; STACK],
            stack_len: 0,
            imported: None,
            lfo_ph: [0.0; 4],
            lfo: [0.0; 4],
            lfo_inc: [0.0; 4],
            lfo_r: [(0.5, 0.5); 4],
            transport: Transport { tempo: 120.0, playing: false, position: 0.0 },
            ctrl_left: 0,
            dc: [(0.0, 0.0); 2],
            live: Arc::new(Telemetry::default()),
        }
    }

    fn pick_voice(&self) -> usize {
        if let Some(i) = self.voices.iter().position(|v| !v.active) {
            return i;
        }
        let quietest_released = (0..MAX_VOICES)
            .filter(|&i| !self.voices[i].gate)
            .min_by(|&a, &b| self.voices[a].loudness().total_cmp(&self.voices[b].loudness()));
        quietest_released.unwrap_or_else(|| (0..MAX_VOICES).min_by_key(|&i| self.voices[i].serial).unwrap_or(0))
    }

    fn start(&mut self, slot: usize, key: u8, velocity: f32, retrig: bool, glide: bool) {
        let q = &*self.q;
        let tables = tables(q, &self.imported);
        let v = &mut self.voices[slot];
        let fresh = !v.active;
        v.key = key;
        v.target = key as f32;
        if !glide {
            v.semi = v.target;
        }
        v.serial = self.serial;
        self.serial += 1;
        v.gate = true;
        v.quick = false;
        if fresh {
            for f in v.filter.iter_mut() {
                f.reset();
            }
            v.noise_lp = [0.0; 2];
            v.env = [Env::default(); 4];
            v.m = [0.0; dest::COUNT];
        }
        if retrig {
            for e in v.env.iter_mut() {
                e.start();
            }
            for o in 0..2 {
                let op = &q.osc[o];
                for k in 0..MAX_UNISON {
                    let r = self.rng.bipolar() * 0.5 + 0.5;
                    let ph = op.phase + op.rand * r;
                    v.osc[o].ph[k] = ph - ph.floor();
                }
            }
            v.sub_ph = 0.0;
            for l in 0..4 {
                if !lfo_mode::shared(q.lfo[l].mode) {
                    v.lfo_ph[l] = 0.0;
                    v.lfo_r[l] = (self.rng.bipolar() * 0.5 + 0.5, self.rng.bipolar() * 0.5 + 0.5);
                }
            }
            v.velocity = velocity;
            v.amp = 1.0 - q.amp_vel + q.amp_vel * velocity;
            v.rnd = self.rng.bipolar() * 0.5 + 0.5;
        }
        v.active = true;
        v.ctrl(q, tables, self.lfo, self.sr, fresh);
    }

    fn note_on(&mut self, key: u8, velocity: f32) {
        // A phrase LFO starts over on the first note after silence.
        if !self.voices.iter().any(|v| v.gate) {
            for l in 0..4 {
                if self.q.lfo[l].mode == lfo_mode::PHRASE {
                    self.lfo_ph[l] = 0.0;
                    self.lfo_r[l] = (self.lfo_r[l].1, self.rng.bipolar() * 0.5 + 0.5);
                }
            }
        }
        if self.q.mono {
            for v in self.voices[1..].iter_mut().filter(|v| v.gate) {
                v.release();
            }
            let legato = self.stack_len > 0 && self.voices[0].gate;
            self.stack_remove(key);
            if self.stack_len == STACK {
                self.stack.copy_within(1.., 0);
                self.stack_len -= 1;
            }
            self.stack[self.stack_len] = key;
            self.stack_len += 1;
            let glide = self.q.glide_on && self.voices[0].active && (legato || self.q.glide_always);
            self.start(0, key, velocity, !legato, glide);
        } else {
            let slot = self.pick_voice();
            self.start(slot, key, velocity, true, false);
        }
    }

    fn note_off(&mut self, key: u8) {
        if self.q.mono {
            let top = self.stack_len > 0 && self.stack[self.stack_len - 1] == key;
            self.stack_remove(key);
            let v = &mut self.voices[0];
            if self.stack_len == 0 {
                if v.gate {
                    v.release();
                }
            } else if top {
                v.key = self.stack[self.stack_len - 1];
                v.target = v.key as f32;
                if !self.q.glide_on {
                    v.semi = v.target;
                }
            }
        }
        let first = if self.q.mono { 1 } else { 0 };
        for v in self.voices[first..].iter_mut().filter(|v| v.active && v.gate && v.key == key) {
            v.release();
        }
    }

    fn stack_remove(&mut self, key: u8) {
        let mut w = 0;
        for r in 0..self.stack_len {
            if self.stack[r] != key {
                self.stack[w] = self.stack[r];
                w += 1;
            }
        }
        self.stack_len = w;
    }

    fn handle(&mut self, e: Event) {
        match e {
            Event::NoteOn { key, velocity } => self.note_on(key, velocity.clamp(0.0, 1.0)),
            Event::NoteOff { key } => self.note_off(key),
            Event::AllNotesOff => {
                self.stack_len = 0;
                for v in self.voices.iter_mut().filter(|v| v.active) {
                    v.release();
                    v.quick = true;
                }
            }
        }
    }

    /// This block's LFO speeds; a synced free LFO locks to the song while it plays.
    fn sync_lfos(&mut self) {
        let t = self.transport;
        let per_tick = CR as f32 / self.sr;
        for l in 0..4 {
            let lp = &self.q.lfo[l];
            let hz = if lp.beats > 0.0 { t.tempo.max(1.0) as f32 / 60.0 / lp.beats } else { lp.hz };
            self.lfo_inc[l] = hz * per_tick;
            if lp.beats > 0.0 && lp.mode == 0 && t.playing {
                let cycle = 96.0 * lp.beats as f64;
                self.set_lfo_phase(l, (t.position / cycle).fract() as f32);
            }
        }
    }

    fn lfo_tick(&mut self) {
        let sounding = self.voices.iter().any(|v| v.active);
        for l in 0..4 {
            let lp = self.q.lfo[l];
            // A phrase LFO only moves while notes sound.
            if !(lp.mode == lfo_mode::PHRASE && !sounding) {
                let ph = self.lfo_ph[l] + self.lfo_inc[l];
                self.set_lfo_phase(l, ph - ph.floor());
            }
            self.lfo[l] = lp.value(self.lfo_ph[l], self.lfo_r[l]);
        }
    }

    /// Move a shared LFO to `ph`, drawing a new random value when it wraps.
    fn set_lfo_phase(&mut self, l: usize, ph: f32) {
        if wrapped(self.lfo_ph[l], ph) {
            self.lfo_r[l] = (self.lfo_r[l].1, self.rng.bipolar() * 0.5 + 0.5);
        }
        self.lfo_ph[l] = ph;
    }

    /// Whether an imported WAV table arrived (through `set_sample`).
    pub fn has_import(&self) -> bool {
        self.imported.is_some()
    }

    #[cfg(test)]
    pub(crate) fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }

    #[cfg(test)]
    pub(crate) fn lfo_phase(&self, l: usize) -> f32 {
        self.lfo_ph[l]
    }
    #[cfg(test)]
    pub(crate) fn lfo_value(&self, l: usize) -> f32 {
        self.lfo[l]
    }
    #[cfg(test)]
    pub(crate) fn voice_lfo(&self, v: usize, l: usize) -> f32 {
        self.voices[v].lfo[l]
    }
    #[cfg(test)]
    pub(crate) fn voice_env(&self, v: usize) -> f32 {
        self.voices[v].env[0].level
    }
    #[cfg(test)]
    pub(crate) fn voice_key(&self, v: usize) -> u8 {
        self.voices[v].key
    }
}

/// Oscillator 1's and 2's tables: a built-in, or the import (Basic if there is none).
fn tables<'a>(q: &Prep, imported: &'a Option<Arc<Table>>) -> [&'a Table; 2] {
    let all = builtin();
    std::array::from_fn(|o| {
        let t = q.osc[o].table;
        if t == IMPORTED {
            imported.as_deref().unwrap_or(&all[0])
        } else {
            &all[t.min(all.len() - 1)]
        }
    })
}

impl Plugin for Chrysalis {
    fn name(&self) -> &'static str {
        "Chrysalis"
    }
    fn params(&self) -> &'static [ParamInfo] {
        &PARAMS
    }
    fn groups(&self) -> &'static [ParamGroup] {
        &crate::params::GROUPS
    }
    fn set_param(&mut self, id: u32, value: f32) {
        if let Some(p) = self.p.get_mut(id as usize) {
            if *p != value {
                *p = value;
                self.dirty = true;
            }
        }
    }
    fn transport(&mut self, transport: Transport) {
        self.transport = transport;
    }
    fn activate(&mut self, sample_rate: f32) {
        self.sr = sample_rate;
        *self.q = Prep::compute(&self.p, sample_rate);
        self.dirty = false;
    }
    fn set_sample(&mut self, sample: Arc<SampleData>) {
        self.imported = Table::from_sample(&sample).map(Arc::new);
    }
    fn process(&mut self, ctx: ProcessContext<'_>) {
        let ProcessContext { events, out_l, out_r, .. } = ctx;
        if self.dirty {
            *self.q = Prep::compute(&self.p, self.sr);
            self.dirty = false;
        }
        self.sync_lfos();
        let n = out_l.len().min(out_r.len());
        let mut peak = 0.0f32;
        let mut next = 0;
        let mut i = 0;
        let mut mix_l = [0.0f32; CR];
        let mut mix_r = [0.0f32; CR];
        while i < n {
            while next < events.len() && events[next].offset as usize <= i {
                self.handle(events[next].event);
                next += 1;
            }
            let tick = self.ctrl_left == 0;
            if tick {
                self.lfo_tick();
            }
            let q = &*self.q;
            let tables = tables(q, &self.imported);
            if tick {
                let (global, inc, sr) = (self.lfo, self.lfo_inc, self.sr);
                for v in self.voices.iter_mut().filter(|v| v.active) {
                    v.lfo_tick(q, inc);
                    v.ctrl(q, tables, global, sr, false);
                }
                self.ctrl_left = CR;
            }
            let mut end = (i + self.ctrl_left).min(n);
            if let Some(e) = events.get(next) {
                end = end.min((e.offset as usize).max(i + 1));
            }
            let len = end - i;
            let (bl, br) = (&mut mix_l[..len], &mut mix_r[..len]);
            bl.fill(0.0);
            br.fill(0.0);
            for v in self.voices.iter_mut().filter(|v| v.active) {
                v.render(q, tables, bl, br);
            }
            let vol = q.volume;
            for k in 0..len {
                for (side, (x, out)) in [(bl[k], &mut out_l[i + k]), (br[k], &mut out_r[i + k])].into_iter().enumerate()
                {
                    let x = x * vol;
                    let (px, py) = self.dc[side];
                    let y = x - px + 0.9995 * py;
                    self.dc[side] = (x, y);
                    *out = soft_clip(y);
                    peak = peak.max(out.abs());
                }
            }
            self.ctrl_left -= len;
            i = end;
        }
        for e in &events[next..] {
            self.handle(e.event);
        }
        self.report(peak);
    }

    fn telemetry(&self) -> Option<Arc<dyn std::any::Any + Send + Sync>> {
        Some(self.live.clone())
    }
}

impl Chrysalis {
    /// Tell the editor what the newest sounding voice is doing (allocation-free).
    fn report(&self, level: f32) {
        let q = &*self.q;
        let tables = tables(q, &self.imported);
        let newest = self.voices.iter().filter(|v| v.active).max_by_key(|v| v.serial);
        let voices = self.voices.iter().filter(|v| v.active).count() as u32;
        let mut r = Report {
            voices,
            pos: [0.0; 2],
            lfo_phase: self.lfo_ph,
            lfo_value: self.lfo,
            env: [(0, 0.0, 0.0); 4],
            mods: [0.0; dest::COUNT],
            level,
        };
        if let Some(v) = newest {
            for (o, table) in tables.iter().enumerate() {
                let last = (table.frames() - 1).max(1) as f32;
                r.pos[o] = (v.osc[o].fpos / last).clamp(0.0, 1.0);
            }
            for l in 0..4 {
                if !lfo_mode::shared(q.lfo[l].mode) {
                    r.lfo_phase[l] = v.lfo_ph[l];
                    r.lfo_value[l] = v.lfo[l];
                }
            }
            for e in 0..4 {
                r.env[e] = (v.env[e].stage, v.env[e].t, v.env[e].level);
            }
            r.mods = v.m;
        }
        self.live.write(&r);
    }
}

/// Transparent below −2 dB, then a tanh knee up to 1: a safety net, not a sound.
#[inline]
fn soft_clip(x: f32) -> f32 {
    const T: f32 = 0.8;
    let a = x.abs();
    if a <= T {
        x
    } else {
        (T + (1.0 - T) * tanh_fast((a - T) / (1.0 - T))).copysign(x)
    }
}
