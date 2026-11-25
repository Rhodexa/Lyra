use crate::curve::MAX_POINTS;
use crate::params::{dest, ft, p, warp, DESTS, GROUPS, PARAMS, SOURCES};
use crate::table::{builtin, Table, FRAME};
use crate::{Chrysalis, PRESETS};
use lyra_plugin::{Event, Plugin, ProcessContext, SampleData, TimedEvent, Transport};
use std::sync::Arc;

const SR: f32 = 48_000.0;

fn with(values: &[(usize, f32)]) -> Chrysalis {
    let mut c = Chrysalis::new();
    for &(i, v) in values {
        c.set_param(i as u32, v);
    }
    c.activate(SR);
    c
}

fn on(key: u8) -> TimedEvent {
    TimedEvent { offset: 0, event: Event::note_on(key, 0.9) }
}
fn off(key: u8) -> TimedEvent {
    TimedEvent { offset: 0, event: Event::note_off(key) }
}

/// Render both sides, 256-frame blocks; `events` go in the first block.
fn render(c: &mut Chrysalis, events: &[TimedEvent], seconds: f32) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
    let (mut ol, mut or) = (Vec::new(), Vec::new());
    let mut first = true;
    while (ol.len() as f32) < seconds * SR {
        let ev = if first { events } else { &[] };
        first = false;
        c.process(ProcessContext { sample_rate: SR, events: ev, out_l: &mut l, out_r: &mut r });
        ol.extend_from_slice(&l);
        or.extend_from_slice(&r);
    }
    (ol, or)
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

fn sync(label: &str) -> f32 {
    lyra_dsp::SYNC_LABELS.iter().position(|l| *l == label).unwrap() as f32
}

#[test]
fn params_and_groups_line_up() {
    assert_eq!(PARAMS.len(), p::COUNT);
    for (i, info) in PARAMS.iter().enumerate() {
        assert_eq!(info.id as usize, i);
        assert!(!info.name.is_empty());
        assert!(info.min < info.max && (info.min..=info.max).contains(&info.default), "{}", info.name);
    }
    let mut names: Vec<&str> = PARAMS.iter().map(|i| i.name).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), p::COUNT, "parameter names are unique");
    let mut next = 0;
    for g in GROUPS.iter() {
        assert_eq!(g.ids.start, next, "{} does not follow on", g.name);
        next = g.ids.end;
    }
    assert_eq!(next as usize, p::COUNT);
    assert_eq!(DESTS.len(), dest::COUNT);
    assert_eq!(DESTS[dest::osc(1, dest::O_BLEND)], "Osc 2 blend");
    assert_eq!(DESTS[dest::filter(1, dest::F_MIX)], "Filter 2 mix");
    assert_eq!(DESTS[dest::LFO_RATE + 3], "LFO 4 rate");
    assert_eq!(DESTS[dest::NOISE_COLOR], "Noise colour");
    for (name, values) in PRESETS.iter() {
        for &(i, v) in *values {
            let info = &PARAMS[i];
            assert!((info.min..=info.max).contains(&v), "{name}: {} = {v} out of range", info.name);
        }
    }
}

/// A two-frame table: frame 0 a sine, frame 1 a square (as an imported WAV).
fn two_frames() -> SampleData {
    let sine = (0..FRAME).map(|i| (std::f32::consts::TAU * i as f32 / FRAME as f32).sin());
    let square = (0..FRAME).map(|i| if i < FRAME / 2 { 0.8 } else { -0.8 });
    let left: Vec<f32> = sine.chain(square).collect();
    SampleData { sample_rate: SR, right: left.clone(), left }
}

/// One frame of `two_frames`, alone.
fn one_frame(k: usize) -> SampleData {
    let all = two_frames();
    let left = all.left[k * FRAME..(k + 1) * FRAME].to_vec();
    SampleData { sample_rate: SR, right: left.clone(), left }
}

fn plain_osc(pos: f32) -> Vec<(usize, f32)> {
    vec![
        (p::osc(0, p::TABLE), crate::table::IMPORTED as f32),
        (p::osc(0, p::POS), pos),
        (p::osc(0, p::UNISON), 0.0),
        (p::osc(0, p::RAND), 0.0),
    ]
}

#[test]
fn position_0_and_1_play_the_first_and_last_frame_exactly() {
    for (pos, frame) in [(0.0, 0), (1.0, 1)] {
        let mut both = with(&plain_osc(pos));
        both.set_sample(Arc::new(two_frames()));
        let mut alone = with(&plain_osc(0.0));
        alone.set_sample(Arc::new(one_frame(frame)));
        // Normalisation is per table (the louder frame sets it), so compare shapes.
        let a = render(&mut both, &[on(57)], 0.2).0;
        let b = render(&mut alone, &[on(57)], 0.2).0;
        let (pa, pb) = (peak(&a), peak(&b));
        assert!(pa > 0.05 && pb > 0.05);
        let err = a.iter().zip(&b).map(|(x, y)| (x / pa - y / pb).abs()).fold(0.0f32, f32::max);
        assert!(err < 1e-4, "position {pos}: frame {frame} differs by {err}");
    }
    // And the middle really is a mix of both.
    let mut mid = with(&plain_osc(0.5));
    mid.set_sample(Arc::new(two_frames()));
    let m = render(&mut mid, &[on(57)], 0.2).0;
    let mut s0 = with(&plain_osc(0.0));
    s0.set_sample(Arc::new(two_frames()));
    let a = render(&mut s0, &[on(57)], 0.2).0;
    assert!(m.iter().zip(&a).any(|(x, y)| (x - y).abs() > 0.05));
}

/// Energy away from the note's harmonics, relative to the whole, in dB.
fn inharmonic_db(x: &[f32], hz: f32) -> f32 {
    let n = 16384;
    let start = x.len() - n;
    let (mut re, mut im) = (vec![0.0f32; n], vec![0.0f32; n]);
    for i in 0..n {
        // Blackman-Harris: side lobes below −90 dB.
        let t = std::f32::consts::TAU * i as f32 / n as f32;
        let w = 0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos();
        re[i] = x[start + i] * w;
    }
    lyra_dsp::fft(&mut re, &mut im, false);
    let bin_hz = SR / n as f32;
    let (mut harm, mut other) = (0.0f64, 0.0f64);
    for k in 1..n / 2 {
        let e = (re[k] as f64).powi(2) + (im[k] as f64).powi(2);
        let f = k as f32 * bin_hz;
        let h = (f / hz).round();
        if h >= 1.0 && (f - h * hz).abs() < 6.0 * bin_hz {
            harm += e;
        } else {
            other += e;
        }
    }
    10.0 * (other / harm).log10() as f32
}

#[test]
fn a_saw_at_high_notes_does_not_alias() {
    for key in [36u8, 48, 57, 64, 72, 84, 96, 103, 110] {
        let mut c = with(&[(p::osc(0, p::UNISON), 0.0), (p::osc(0, p::RAND), 0.0), (p::AMP_VEL, 0.0)]);
        let (l, _) = render(&mut c, &[on(key)], 0.6);
        let hz = 440.0 * 2f32.powf((key as f32 - 69.0) / 12.0);
        let db = inharmonic_db(&l, hz);
        println!("key {key} ({hz:.0} Hz): everything off the harmonics at {db:.1} dB");
        assert!(db < -70.0, "key {key}: aliasing at {db:.1} dB");
    }
}

#[test]
fn a_synced_lfo_follows_the_tempo_and_locks_to_the_song() {
    let mut c = with(&[(p::lfo(0, p::L_SYNC), sync("1/4"))]);
    c.transport(Transport { tempo: 120.0, playing: false, position: 0.0 });
    render(&mut c, &[], 0.25); // a quarter at 120 is 0.5 s: half a cycle
    assert!((c.lfo_phase(0) - 0.5).abs() < 0.02, "phase {}", c.lfo_phase(0));
    let mut c = with(&[(p::lfo(1, p::L_SYNC), sync("1/8"))]);
    c.transport(Transport { tempo: 140.0, playing: true, position: 60.0 }); // 48 ticks a cycle
    c.process(ProcessContext { sample_rate: SR, events: &[], out_l: &mut [0.0; 1], out_r: &mut [0.0; 1] });
    assert!((c.lfo_phase(1) - 0.25).abs() < 0.01, "phase {}", c.lfo_phase(1));
}

#[test]
fn a_square_lfo_on_volume_gates_the_sound() {
    let mut v = vec![(p::lfo(0, p::L_SYNC), sync("1/8")), (p::lfo(0, p::L_POINTS), 4.0)];
    for (k, &(x, y)) in [(0.0, 1.0), (0.5, 1.0), (0.5, 0.0), (1.0, 0.0)].iter().enumerate() {
        v.push((p::point_x(0, k), x));
        v.push((p::point_y(0, k), y));
    }
    v.extend([(p::slot(0, p::S_SRC), 1.0), (p::slot(0, p::S_DEST), dest::VOLUME as f32), (p::slot(0, p::S_AMT), -1.0)]);
    let mut c = with(&v);
    c.transport(Transport { tempo: 120.0, playing: false, position: 0.0 });
    let (l, _) = render(&mut c, &[on(60)], 1.0);
    let q = (0.25 * SR) as usize / 2; // half a 1/8 cycle at 120
    let part = |k: usize| peak(&l[k * q + 300..(k + 1) * q - 300]);
    assert!(part(0) < 1e-3 && part(1) > 0.05, "first half {}, second half {}", part(0), part(1));
}

#[test]
fn velocity_sets_the_loudness() {
    let at = |velocity: f32| {
        let e = TimedEvent { offset: 0, event: Event::note_on(60, velocity) };
        peak(&render(&mut with(&[(p::osc(0, p::RAND), 0.0)]), &[e], 0.2).0)
    };
    let ratio = at(1.0) / at(0.25);
    assert!((ratio - 4.0).abs() < 0.2, "a quarter velocity is 12 dB down, got ×{ratio}");
}

#[test]
fn it_is_stereo_by_default_and_mono_without_spread() {
    let (l, r) = render(&mut with(&[]), &[on(60)], 0.3);
    let diff = l.iter().zip(&r).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(peak(&l) > 0.05 && diff > 0.02, "default unison is spread (max L−R {diff})");
    let (l, r) = render(&mut with(&[(p::osc(0, p::SPREAD), 0.0)]), &[on(60)], 0.3);
    assert!(l.iter().zip(&r).all(|(a, b)| (a - b).abs() < 1e-6));
}

#[test]
fn every_warp_and_filter_plays_finite_and_changes_the_sound() {
    let plain = render(&mut with(&[(p::osc(0, p::RAND), 0.0)]), &[on(48)], 0.3).0;
    for w in 1..8u8 {
        let mut v = vec![(p::osc(0, p::RAND), 0.0), (p::osc(0, p::WARP_MODE), w as f32), (p::osc(0, p::WARP), 0.6)];
        if w == warp::FM {
            v.push((p::osc(1, p::OSC_ON), 1.0));
            v.push((p::osc(1, p::LEVEL), 0.0));
        }
        let x = render(&mut with(&v), &[on(48)], 0.3).0;
        assert!(x.iter().all(|s| s.is_finite()), "warp {w}");
        assert!(x.iter().zip(&plain).any(|(a, b)| (a - b).abs() > 0.02), "warp {w} changes nothing");
    }
    for kind in 0..8u8 {
        let v = [
            (p::osc(0, p::RAND), 0.0),
            (p::filter(0, p::F_ON), 1.0),
            (p::filter(0, p::F_TYPE), kind as f32),
            (p::filter(0, p::F_CUTOFF), 600.0),
            (p::filter(0, p::F_RESO), 0.9),
            (p::filter(0, p::F_DRIVE), 0.5),
        ];
        let x = render(&mut with(&v), &[on(48)], 0.3).0;
        let pk = peak(&x);
        assert!(x.iter().all(|s| s.is_finite()) && pk > 0.01 && pk < 1.0, "filter {kind}: peak {pk}");
        assert!(x.iter().zip(&plain).any(|(a, b)| (a - b).abs() > 0.02), "filter {kind} changes nothing");
    }
    for mode in 1..5 {
        let v = [(p::osc(0, p::RAND), 0.0), (p::DIST_MODE, mode as f32), (p::DIST_DRIVE, 0.7)];
        let x = render(&mut with(&v), &[on(48)], 0.3).0;
        assert!(x.iter().all(|s| s.is_finite()), "drive {mode}");
        assert!(x.iter().zip(&plain).any(|(a, b)| (a - b).abs() > 0.02), "drive {mode} changes nothing");
    }
}

#[test]
fn the_formant_filter_moves_with_its_vowel() {
    let at = |vowel: f32| {
        let v = [
            (p::osc(0, p::RAND), 0.0),
            (p::filter(0, p::F_ON), 1.0),
            (p::filter(0, p::F_TYPE), ft::FORMANT as f32),
            (p::filter(0, p::F_CUTOFF), 1000.0),
            (p::filter(0, p::F_VOWEL), vowel),
        ];
        render(&mut with(&v), &[on(45)], 0.3).0
    };
    let (a, i) = (at(0.0), at(0.5));
    assert!(a.iter().zip(&i).any(|(x, y)| (x - y).abs() > 0.02));
}

#[test]
fn an_envelope_through_the_matrix_closes_the_filter() {
    let v = [
        (p::osc(0, p::RAND), 0.0),
        (p::filter(0, p::F_ON), 1.0),
        (p::filter(0, p::F_CUTOFF), 200.0),
        (p::env(1, p::E_DECAY), 0.1),
        (p::slot(0, p::S_SRC), (crate::params::src::ENV + 1) as f32),
        (p::slot(0, p::S_DEST), dest::filter(0, dest::F_CUTOFF) as f32),
        (p::slot(0, p::S_AMT), 0.8),
    ];
    let (l, _) = render(&mut with(&v), &[on(48)], 0.8);
    let early = peak(&l[100..2400]);
    let late = peak(&l[30_000..36_000]);
    assert!(early > 2.0 * late, "bright at first ({early}), then dark ({late})");
}

#[test]
fn via_scales_a_route_by_a_second_source() {
    // Macro 1 → pitch via velocity: velocity 0 means no bend at all.
    let v = [
        (p::osc(0, p::RAND), 0.0),
        (p::AMP_VEL, 0.0),
        (p::mac(0), 1.0),
        (p::slot(0, p::S_SRC), SOURCES.iter().position(|s| *s == "Macro 1").unwrap() as f32),
        (p::slot(0, p::S_DEST), dest::PITCH as f32),
        (p::slot(0, p::S_AMT), 0.5),
        (p::slot(0, p::S_VIA), crate::params::src::VELOCITY as f32),
    ];
    let soft = TimedEvent { offset: 0, event: Event::note_on(60, 0.0) };
    let a = render(&mut with(&v), &[soft], 0.2).0;
    let b = render(&mut with(&[(p::osc(0, p::RAND), 0.0), (p::AMP_VEL, 0.0)]), &[soft], 0.2).0;
    assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-5));
    let loud = TimedEvent { offset: 0, event: Event::note_on(60, 1.0) };
    let c = render(&mut with(&v), &[loud], 0.2).0;
    assert!(c.iter().zip(&b).any(|(x, y)| (x - y).abs() > 0.05));
}

#[test]
fn a_one_shot_lfo_holds_its_end() {
    // Ramp down over 1/16 as a one-shot on position: it ends (and stays) at the first frame.
    let mut v = vec![(p::lfo(0, p::L_SYNC), sync("1/16")), (p::lfo(0, p::L_MODE), 2.0), (p::lfo(0, p::L_POINTS), 2.0)];
    v.extend([(p::point_x(0, 0), 0.0), (p::point_y(0, 0), 1.0), (p::point_x(0, 1), 1.0), (p::point_y(0, 1), 0.0)]);
    let mut c = with(&v);
    c.transport(Transport { tempo: 120.0, playing: false, position: 0.0 });
    render(&mut c, &[on(60)], 0.5);
    assert!((c.voice_lfo(0, 0) - 0.0).abs() < 1e-3, "held at {}", c.voice_lfo(0, 0));
}

#[test]
fn velocity_can_set_a_note_lfos_speed() {
    // A one-shot ramp over 1/4 (0.5 s at 120), velocity (centred) → LFO 1 rate × 0.25:
    // velocity ½ plays it as drawn, velocity 1 twice as fast ("Wingbeat"'s talking bass).
    let mut v = vec![(p::lfo(0, p::L_SYNC), sync("1/4")), (p::lfo(0, p::L_MODE), 2.0), (p::lfo(0, p::L_POINTS), 2.0)];
    v.extend([(p::point_x(0, 0), 0.0), (p::point_y(0, 0), 0.0), (p::point_x(0, 1), 1.0), (p::point_y(0, 1), 1.0)]);
    let rate = DESTS.iter().position(|d| *d == "LFO 1 rate").unwrap() as f32;
    v.extend([
        (p::slot(0, p::S_SRC), crate::params::src::VELOCITY as f32),
        (p::slot(0, p::S_DEST), rate),
        (p::slot(0, p::S_AMT), 0.25),
        (p::slot(0, p::S_BIPOLAR), 1.0),
    ]);
    let after = |velocity: f32| {
        let mut c = with(&v);
        c.transport(Transport { tempo: 120.0, playing: false, position: 0.0 });
        let note = TimedEvent { offset: 0, event: Event::note_on(48, velocity) };
        render(&mut c, &[note], 0.125);
        c.voice_lfo(0, 0)
    };
    let (half, full) = (after(0.5), after(1.0));
    assert!((half - 0.25).abs() < 0.03, "velocity ½: {half}");
    assert!((full - 0.5).abs() < 0.04, "velocity 1: {full}");
}

#[test]
fn mono_legato_steps_pitch_without_restarting() {
    let mut c = with(&[(p::MODE, 1.0), (p::GLIDE, 0.0), (p::env(0, p::E_DECAY), 0.05), (p::env(0, p::E_SUSTAIN), 0.5)]);
    render(&mut c, &[on(60)], 0.3);
    let before = c.voice_env(0);
    render(&mut c, &[on(64), off(60)], 0.002);
    assert_eq!(c.voice_key(0), 64);
    assert!((c.voice_env(0) - before).abs() < 1e-3, "no restart");
}

#[test]
fn steals_when_full() {
    let mut c = with(&[]);
    let keys: Vec<TimedEvent> = (40..60).map(on).collect(); // 20 notes, 16 voices
    let (l, _) = render(&mut c, &keys, 0.2);
    assert!(l.iter().all(|x| x.is_finite()));
    assert_eq!(c.active_voices(), crate::MAX_VOICES);
}

#[test]
fn every_preset_plays_a_chord_and_falls_silent() {
    for (name, values) in PRESETS.iter() {
        let mut c = with(values);
        let release = (0..4).map(|e| c_param(values, p::env(e, p::E_RELEASE))).fold(0.0f32, f32::max);
        let chord = [45u8, 52, 57, 60];
        let (hl, hr) = render(&mut c, &chord.map(on), 0.8);
        let (tl, tr) = render(&mut c, &chord.map(off), 3.0 + 3.0 * release);
        let all: Vec<f32> = hl.iter().chain(&hr).chain(&tl).chain(&tr).copied().collect();
        assert!(all.iter().all(|x| x.is_finite()), "{name}: NaN/inf");
        let pk = peak(&all);
        assert!(pk > 0.05 && pk <= 1.0, "{name}: peak {pk}");
        let end = peak(&tl[tl.len() - 4800..]).max(peak(&tr[tr.len() - 4800..]));
        assert!(end < 1e-3, "{name}: still sounding after release ({end})");
        assert_eq!(c.active_voices(), 0, "{name}: voices never freed");
        let wide = hl.iter().zip(&hr).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        println!("{name:16} peak {pk:.2}  L−R {wide:.2}");
    }
}

fn c_param(values: &[(usize, f32)], id: usize) -> f32 {
    values.iter().rev().find(|(i, _)| *i == id).map_or(PARAMS[id].default, |&(_, v)| v)
}

#[test]
fn every_builtin_table_plays() {
    for t in 0..builtin().len() {
        let (l, r) = render(&mut with(&[(p::osc(0, p::TABLE), t as f32), (p::osc(0, p::POS), 0.5)]), &[on(48)], 0.2);
        assert!(peak(&l) > 0.05 && l.iter().chain(&r).all(|x| x.is_finite()), "table {t}");
    }
}

#[test]
fn level_choice_keeps_every_harmonic_below_nyquist() {
    for key in 0..128u8 {
        let hz = 440.0 * 2f32.powf((key as f32 - 69.0) / 12.0);
        if let Some(l) = Table::level_for(hz, SR) {
            assert!(crate::table::level_harmonics(l) as f32 * hz < SR / 2.0);
            if l > 0 {
                // …and it is the fullest such level.
                assert!(crate::table::level_harmonics(l - 1) as f32 * hz >= SR / 2.0);
            }
        }
    }
}

/// Seconds of CPU per second of audio for 16 held notes. This laptop's clock
/// swings more than 2× between runs, so: warm the CPU up first, then take
/// the best of five half-seconds.
fn cost_of(values: &[(usize, f32)]) -> f64 {
    let mut c = with(values);
    let keys: Vec<TimedEvent> = (48..64).map(on).collect();
    render(&mut c, &keys, 1.0);
    assert_eq!(c.active_voices(), crate::MAX_VOICES);
    (0..5)
        .map(|_| {
            let t = std::time::Instant::now();
            render(&mut c, &[], 0.5);
            2.0 * t.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn worst_case_cost() {
    let mut v = vec![
        (p::DIST_MODE, 1.0),
        (p::SUB_ON, 1.0),
        (p::NOISE_ON, 1.0),
        (p::filter(0, p::F_ON), 1.0),
        (p::filter(1, p::F_ON), 1.0),
        (p::filter(1, p::F_TYPE), ft::FORMANT as f32),
        (p::osc(1, p::OSC_ON), 1.0),
        (p::osc(0, p::TABLE), 4.0),
        (p::osc(1, p::TABLE), 3.0),
    ];
    // All 16 slots busy: LFOs on positions, cutoffs, warps.
    for s in 0..p::SLOT_COUNT {
        v.push((p::slot(s, p::S_SRC), (1 + s % 4) as f32));
        v.push((p::slot(s, p::S_DEST), (dest::osc(0, 0) + s % 14) as f32));
        v.push((p::slot(s, p::S_AMT), 0.1));
    }
    for (voices, warp_mode) in [(8, warp::OFF), (16, warp::OFF), (16, warp::FM)] {
        let mut v = v.clone();
        for o in 0..2 {
            v.push((p::osc(o, p::UNISON), (voices - 1) as f32));
            v.push((p::osc(o, p::WARP_MODE), warp_mode as f32));
            v.push((p::osc(o, p::WARP), 0.3));
        }
        let cost = cost_of(&v);
        println!(
            "16 notes × 2 oscs × {voices} unison, warp {warp_mode}, both filters, drive, all slots: {:.1}% of one core",
            cost * 100.0
        );
        if voices == 8 {
            // Really ~12 % (at full clock); the limit leaves room for a slow clock.
            let limit = if cfg!(debug_assertions) { 3.0 } else { 0.4 };
            assert!(cost < limit, "the 8-voice-unison worst case must stay under {:.0} %", limit * 100.0);
        }
    }
    let _ = MAX_POINTS;
}

/// How loud each preset is on one held note (prints; for levelling them).
#[test]
#[ignore]
fn preset_loudness() {
    for (name, values) in PRESETS.iter() {
        let key = if name.starts_with("Bass") { 41 } else { 60 };
        let (l, r) = render(&mut with(values), &[on(key)], 1.5);
        let part =
            |x: &[f32]| x[(0.2 * SR) as usize..].iter().map(|v| v * v).sum::<f32>() / (x.len() as f32 - 0.2 * SR);
        let rms = ((part(&l) + part(&r)) / 2.0).sqrt();
        println!(
            "{name:16} key {key}: rms {:6.1} dB, peak {:6.1} dB",
            20.0 * rms.log10(),
            20.0 * peak(&l).max(peak(&r)).log10()
        );
    }
}

#[test]
fn telemetry_follows_the_newest_voice() {
    use crate::telemetry::Telemetry;
    let mut c = with(&[
        (p::osc(0, p::POS), 0.0),
        (p::lfo(0, p::L_SYNC), 0.0),
        (p::lfo(0, p::L_RATE), 3.0),
        (p::slot(0, p::S_SRC), 1.0),
        (p::slot(0, p::S_DEST), dest::osc(0, dest::O_POS) as f32),
        (p::slot(0, p::S_AMT), 1.0),
    ]);
    let live = c.telemetry().expect("Chrysalis shares its state").downcast::<Telemetry>().expect("a Telemetry");
    render(&mut c, &[on(60)], 0.05);
    let a = live.snapshot();
    render(&mut c, &[], 0.1);
    let b = live.snapshot();
    assert_eq!(a.voices, 1);
    assert!(a.blocks > 0 && b.blocks > a.blocks);
    assert_ne!(a.env_stage[0], 0, "the amp envelope is running");
    assert!((a.pos[0] - b.pos[0]).abs() > 0.05, "the LFO moves the playing frame: {} → {}", a.pos[0], b.pos[0]);
    assert!(b.mods[dest::osc(0, dest::O_POS)] > 0.0);
    render(&mut c, &[off(60)], 1.5);
    assert_eq!(live.snapshot().voices, 0);
}

#[test]
fn a_phrase_lfo_waits_for_notes_and_restarts_after_silence() {
    use crate::params::lfo_mode::PHRASE;
    let mut c =
        with(&[(p::lfo(0, p::L_MODE), PHRASE as f32), (p::lfo(0, p::L_SYNC), 0.0), (p::lfo(0, p::L_RATE), 1.0)]);
    render(&mut c, &[], 0.3);
    assert_eq!(c.lfo_phase(0), 0.0, "no notes, no movement");
    render(&mut c, &[on(60)], 0.25);
    let a = c.lfo_phase(0);
    assert!((a - 0.25).abs() < 0.02, "runs with a note: {a}");
    render(&mut c, &[on(64)], 0.1); // overlapping: keeps going
    assert!(c.lfo_phase(0) > a);
    render(&mut c, &[off(60), off(64)], 2.0); // silence (the voices finish)
    let rest = c.lfo_phase(0);
    render(&mut c, &[], 0.3);
    assert_eq!(c.lfo_phase(0), rest, "stands still in the silence");
    render(&mut c, &[on(62)], 0.01);
    assert!(c.lfo_phase(0) < 0.02, "restarts on the next phrase: {}", c.lfo_phase(0));
}

#[test]
fn random_styles_step_and_glide() {
    // LFO 1 at 8 Hz on volume would be hard to read; read the LFO itself.
    for (style, name) in [(1.0, "steps"), (2.0, "smooth")] {
        let mut c = with(&[(p::lfo(0, p::L_SYNC), 0.0), (p::lfo(0, p::L_RATE), 8.0), (p::lfo_style(0), style)]);
        let mut values = Vec::new();
        for _ in 0..1500 {
            // 1500 blocks of 64 = 2 s.
            c.process(ProcessContext { sample_rate: SR, events: &[], out_l: &mut [0.0; 64], out_r: &mut [0.0; 64] });
            values.push(c.lfo_value(0));
        }
        assert!(values.iter().all(|v| (0.0..=1.0).contains(v)), "{name}: in range");
        let distinct = {
            let mut v: Vec<u32> = values.iter().map(|x| (x * 1e4) as u32).collect();
            v.dedup();
            v.len()
        };
        let biggest_jump = values.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        if style == 1.0 {
            // ~2 s at 8 Hz: about 16 steps, each held.
            assert!((8..40).contains(&distinct), "steps: {distinct} distinct values");
        } else {
            assert!(distinct > 100 && biggest_jump < 0.2, "smooth: {distinct} values, jump {biggest_jump}");
        }
    }
}
