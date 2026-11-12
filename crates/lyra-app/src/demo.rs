//! The project Lyra opens with: a 16-bar deep-house sketch in A minor (beat,
//! bass, chords, arranged in the playlist), so the very first thing you do is
//! press Space and nod your head.

use lyra_model::{ChannelId, Clip, EffectSlot, Note, Project, TICKS_PER_BAR, TICKS_PER_STEP};

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

    // Three patterns: drums, bass, chords — so the playlist can arrange them.
    p.patterns[0].name = "Beat".into();
    let bass_pat = p.add_pattern();
    p.patterns[bass_pat].name = "Bass".into();
    let chord_pat = p.add_pattern();
    p.patterns[chord_pat].name = "Chords".into();

    let beat = &mut p.patterns[0];
    beat.min_length = 32;
    for bar in [0u32, 16] {
        for s in [0, 4, 8, 12] {
            beat.set_step(kick, bar + s, true, 1.0);
        }
        for s in [4, 12] {
            beat.set_step(clap, bar + s, true, 0.9);
        }
        for s in 0..16 {
            if s % 4 == 2 {
                beat.set_step(open, bar + s, true, 0.7); // offbeat "tss", choked by the next closed hat
            } else {
                beat.set_step(hat, bar + s, true, if s % 2 == 0 { 0.62 } else { 0.4 });
            }
        }
        for s in [7, 15] {
            beat.set_step(snare, bar + s, true, 0.3); // ghost notes for groove
        }
    }

    // Bass: offbeat roots with an octave hop, staying out of the kick's way.
    // Bar 1 on A, bar 2 on F.
    let notes = p.patterns[bass_pat].notes_mut(bass);
    for (bar, root) in [(0.0, 45u8), (16.0, 41u8)] {
        for &(step, len, offset, vel) in
            &[(2.0, 1.5, 0, 0.9), (6.0, 1.5, 0, 0.8), (10.0, 1.0, 12, 0.85), (11.0, 1.0, 0, 0.7), (14.0, 1.5, 7, 0.8)]
        {
            notes.push(note(bar + step, len, root + offset, vel));
        }
    }
    // Chords: Am7 then Fmaj7, stabbed in a 3-3-2 rhythm.
    let notes = p.patterns[chord_pat].notes_mut(chords);
    for (bar, chord) in [(0.0, [57u8, 60, 64, 67]), (16.0, [53u8, 57, 60, 64])] {
        for &(step, len) in &[(0.0, 2.5), (3.0, 2.5), (6.0, 2.0), (10.0, 2.5), (13.0, 2.0)] {
            for &k in &chord {
                notes.push(note(bar + step, len, k, 0.75));
            }
        }
    }

    // Mixer: drums share inserts, bass and chords "breathe" with the kick (Pump),
    // the chords get an echo and a room.
    let route = |p: &mut Project, ch: ChannelId, t: usize, name: &str| {
        if let Some(c) = p.channel_mut(ch) {
            c.mixer = t;
        }
        p.mixer[t].name = name.into();
    };
    route(&mut p, kick, 1, "Kick");
    route(&mut p, clap, 2, "Clap & snare");
    route(&mut p, snare, 2, "Clap & snare");
    route(&mut p, hat, 3, "Hats");
    route(&mut p, open, 3, "Hats");
    route(&mut p, bass, 4, "Bass");
    route(&mut p, chords, 5, "Chords");
    let fx = |id: &str, tweaks: &[(usize, f32)]| {
        let mut params = lyra_dsp::effects::default_params(id);
        for &(i, v) in tweaks {
            params[i] = v;
        }
        EffectSlot { effect: id.into(), params, bypass: false }
    };
    p.mixer[2].effects = vec![fx("lyra.reverb", &[(0, 0.4), (3, 0.12)])];
    p.mixer[3].effects = vec![fx("lyra.eq", &[(0, 300.0)])];
    p.mixer[4].effects = vec![fx("lyra.eq", &[(1, 3.0), (4, -4.0)]), fx("lyra.pump", &[(0, 0.55)])];
    p.mixer[5].effects = vec![
        fx("lyra.pump", &[(0, 0.7), (1, 0.6)]),
        fx("lyra.delay", &[(0, 2.0), (1, 0.4), (4, 0.22)]),
        fx("lyra.reverb", &[(0, 0.8), (3, 0.28)]),
    ];
    p.mixer[0].effects = vec![fx("lyra.comp", &[(0, -10.0), (1, 2.5), (4, 1.5)])];

    // A 16-bar song: intro (beat), bass joins, breakdown (chords alone), drop (all).
    let bar = TICKS_PER_BAR;
    let ids = [p.patterns[0].id, p.patterns[bass_pat].id, p.patterns[chord_pat].id];
    let names = ["Drums", "Bass", "Chords"];
    for (t, name) in names.iter().enumerate() {
        p.tracks[t].name = name.to_string();
    }
    for &(track, start, bars) in &[(0, 0, 8), (0, 10, 6), (1, 4, 4), (1, 10, 6), (2, 8, 8)] {
        p.clips.push(Clip { pattern: ids[track], track, start: start * bar, len: bars * bar });
    }
    p
}
