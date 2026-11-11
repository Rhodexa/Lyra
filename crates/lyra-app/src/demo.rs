//! The project Lyra opens with: a one-bar house groove, so the very first
//! thing you do is press Space and nod your head.

use lyra_model::{ChannelId, Project};

fn add(p: &mut Project, name: &str, instrument: &str, tweaks: &[(usize, f32)], volume: f32, choke: u8) -> ChannelId {
    let mut params = lyra_dsp::default_params(instrument);
    for &(i, v) in tweaks {
        if let Some(slot) = params.get_mut(i) {
            *slot = v;
        }
    }
    let id = p.add_channel(name, instrument, params);
    if let Some(ch) = p.channel_mut(id) {
        ch.volume = volume;
        ch.choke = choke;
    }
    id
}

pub fn project() -> Project {
    let mut p = Project::default();
    p.bpm = 124.0;
    p.swing = 0.15;

    // Hat params: 0 Tune, 1 Decay, 2 Tone, 3 Metal, 4 Level.
    let kick = add(&mut p, "Kick", "lyra.kick", &[], 0.9, 0);
    let clap = add(&mut p, "Clap", "lyra.clap", &[], 0.7, 0);
    let hat = add(&mut p, "Closed hat", "lyra.hat", &[(1, 0.045), (2, 8000.0)], 0.5, 1);
    let open = add(&mut p, "Open hat", "lyra.hat", &[(1, 0.32), (2, 6500.0)], 0.45, 1);
    let snare = add(&mut p, "Snare", "lyra.snare", &[(0, 220.0), (3, 0.12)], 0.45, 0);

    let pat = &mut p.patterns[0];
    pat.name = "Groove".into();
    for s in [0, 4, 8, 12] {
        pat.set_step(kick, s, true, 1.0);
    }
    for s in [4, 12] {
        pat.set_step(clap, s, true, 0.9);
    }
    for s in 0..16 {
        if s % 4 == 2 {
            pat.set_step(open, s, true, 0.7); // the offbeat "tss" — choked by the next closed hat
        } else {
            pat.set_step(hat, s, true, if s % 2 == 0 { 0.62 } else { 0.4 });
        }
    }
    for s in [7, 15] {
        pat.set_step(snare, s, true, 0.3); // ghost notes for groove
    }
    p
}
