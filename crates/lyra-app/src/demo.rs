//! The project Lyra opens with: a two-bar deep-house groove in A minor, so the
//! very first thing you do is press Space and nod your head.

use lyra_model::{ChannelId, Note, Project, TICKS_PER_STEP};

fn add(p: &mut Project, name: &str, instrument: &str, tweaks: &[(usize, f32)], volume: f32, choke: u8) -> ChannelId {
    let mut params = lyra_dsp::default_params(instrument);
    for &(i, v) in tweaks {
        if let Some(slot) = params.get_mut(i) {
            *slot = v;
        }
    }
    add_with(p, name, instrument, params, volume, choke)
}

fn add_with(p: &mut Project, name: &str, instrument: &str, params: Vec<f32>, volume: f32, choke: u8) -> ChannelId {
    let id = p.add_channel(name, instrument, params);
    if let Some(ch) = p.channel_mut(id) {
        ch.volume = volume;
        ch.choke = choke;
    }
    id
}

fn preset(name: &str) -> Vec<f32> {
    lyra_dsp::PRESETS.iter().find(|p| p.name == name).map(|p| p.params()).unwrap_or_default()
}

/// A note `len` steps long starting at `step` (both may be fractional).
fn note(step: f32, len: f32, key: u8, velocity: f32) -> Note {
    let t = TICKS_PER_STEP as f32;
    Note { start: (step * t) as u32, len: (len * t) as u32, key, velocity }
}

pub fn project() -> Project {
    let mut p = Project::default(); // A minor, 124 bpm
    p.bpm = 124.0;
    p.swing = 0.15;

    // Hat params: 0 Tune, 1 Decay, 2 Tone, 3 Metal, 4 Level.
    let kick = add(&mut p, "Kick", "lyra.kick", &[], 0.9, 0);
    let clap = add(&mut p, "Clap", "lyra.clap", &[], 0.7, 0);
    let hat = add(&mut p, "Closed hat", "lyra.hat", &[(1, 0.045), (2, 8000.0)], 0.5, 1);
    let open = add(&mut p, "Open hat", "lyra.hat", &[(1, 0.32), (2, 6500.0)], 0.45, 1);
    let snare = add(&mut p, "Snare", "lyra.snare", &[(0, 220.0), (3, 0.12)], 0.45, 0);
    let bass = add_with(&mut p, "Bass", "lyra.synth", preset("Deep Bass"), 0.75, 0);
    let chords = add_with(&mut p, "Chords", "lyra.synth", preset("Chord Stab"), 0.5, 0);

    let pat = &mut p.patterns[0];
    pat.name = "Groove".into();
    pat.min_length = 32;
    for bar in [0u32, 16] {
        for s in [0, 4, 8, 12] {
            pat.set_step(kick, bar + s, true, 1.0);
        }
        for s in [4, 12] {
            pat.set_step(clap, bar + s, true, 0.9);
        }
        for s in 0..16 {
            if s % 4 == 2 {
                pat.set_step(open, bar + s, true, 0.7); // offbeat "tss", choked by the next closed hat
            } else {
                pat.set_step(hat, bar + s, true, if s % 2 == 0 { 0.62 } else { 0.4 });
            }
        }
        for s in [7, 15] {
            pat.set_step(snare, bar + s, true, 0.3); // ghost notes for groove
        }
    }

    // Bass: offbeat roots with an octave hop, staying out of the kick's way.
    // Bar 1 on A, bar 2 on F.
    for (bar, root) in [(0.0, 45u8), (16.0, 41u8)] {
        let notes = pat.notes_mut(bass);
        for &(step, len, offset, vel) in &[(2.0, 1.5, 0, 0.9), (6.0, 1.5, 0, 0.8), (10.0, 1.0, 12, 0.85), (11.0, 1.0, 0, 0.7), (14.0, 1.5, 7, 0.8)] {
            notes.push(note(bar + step, len, root + offset, vel));
        }
    }
    // Chords: Am7 then Fmaj7, stabbed in a 3-3-2 rhythm.
    for (bar, chord) in [(0.0, [57u8, 60, 64, 67]), (16.0, [53u8, 57, 60, 64])] {
        let notes = pat.notes_mut(chords);
        for &(step, len) in &[(0.0, 2.5), (3.0, 2.5), (6.0, 2.0), (10.0, 2.5), (13.0, 2.0)] {
            for &k in &chord {
                notes.push(note(bar + step, len, k, 0.75));
            }
        }
    }
    p
}
