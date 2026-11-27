use super::*;
use lyra_plugin::{TimedEvent, NO_ID};
use octavia_core::score::Score;
use octavia_core::{render_score, Played};

const SR: f32 = 48_000.0;
const BLOCK: usize = 256;

fn make() -> Octavia {
    let mut o = Octavia::new();
    o.activate(SR);
    o
}

/// Plays `s` (no double stops) the way Lyra does: before every block the
/// next second as `UpcomingNote`s, then the block with its note-ons and
/// note-offs at their samples. Returns the left channel.
fn host(o: &mut Octavia, s: &Score, len: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; len];
    let mut right = vec![0.0f32; BLOCK];
    let mut done = 0usize; // samples played so far
    for block in out.chunks_mut(BLOCK) {
        let n = block.len();
        let t0 = done as f64 / SR as f64; // the song's time at the block start
        let span = n as f64 / SR as f64;
        let up: Vec<UpcomingNote> = s
            .notes
            .iter()
            .enumerate()
            .filter(|(_, x)| x.start >= t0 && x.start < t0 + o.note_lookahead())
            .map(|(i, x)| UpcomingNote {
                start: x.start - t0,
                len: x.dur,
                key: x.pitch as u8,
                velocity: 0.5,
                id: i as u32,
            })
            .collect();
        o.upcoming(&up);
        let mut events = Vec::new();
        let offset = |t: f64| ((t - t0) * SR as f64).round() as u32;
        for (i, x) in s.notes.iter().enumerate() {
            if x.end() >= t0 && x.end() < t0 + span {
                events.push(TimedEvent {
                    offset: offset(x.end()),
                    event: Event::NoteOff { key: x.pitch as u8, id: i as u32 },
                });
            }
            if x.start >= t0 && x.start < t0 + span {
                events.push(TimedEvent {
                    offset: offset(x.start),
                    event: Event::NoteOn { key: x.pitch as u8, velocity: 0.5, id: i as u32 },
                });
            }
        }
        events.sort_by_key(|e| e.offset);
        o.process(ProcessContext { sample_rate: SR, events: &events, out_l: block, out_r: &mut right[..n] });
        done += n;
    }
    out
}

#[test]
fn playing_from_the_piano_roll_sounds_like_the_offline_render() {
    // As a piano roll holds it: timing, key and velocity, no hints.
    let mut s = Score::parse("tempo 100\nC3:e D3 E3 F3 G3:q A3 r:q B3:h").unwrap();
    for n in &mut s.notes {
        n.dynamic = octavia_core::score::velocity_to_dynamic(0.5);
        n.dynamic_end = n.dynamic;
        (n.slur, n.articulation) = (None, octavia_core::Articulation::Auto);
        // On a sample, as anything a host plays.
        n.start = (n.start * SR as f64).round() / SR as f64;
    }
    let mut o = make();
    let style = o.style();
    let offline = render_score(&s, &cello(), &style, SR, 0.0).audio;
    let live = host(&mut o, &s, offline.len());
    // Every decision as on the test bench…
    let decided: Vec<_> = o.telemetry.read().recent.iter().flatten().map(|n| n.decision.unwrap()).collect();
    assert_eq!(decided, render_score(&s, &cello(), &style, SR, 0.0).decisions);
    // …and every sample.
    let err = live.iter().zip(&offline).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert_eq!(err, 0.0, "Lyra plays what the test bench renders");
}

#[test]
fn a_key_played_live_sounds_and_stops() {
    let mut o = make();
    let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let mut energy = |o: &mut Octavia, blocks: usize, events: &[TimedEvent]| {
        let mut sum = 0.0;
        for k in 0..blocks {
            let ev = if k == 0 { events } else { &[] };
            o.upcoming(&[]); // stopped: nothing forecast
            o.process(ProcessContext { sample_rate: SR, events: ev, out_l: &mut l, out_r: &mut r });
            sum += l.iter().map(|v| v * v).sum::<f32>();
        }
        (sum / (blocks * BLOCK) as f32).sqrt()
    };
    let on = [TimedEvent { offset: 10, event: Event::note_on(50, 0.8) }];
    assert!(energy(&mut o, 150, &on) > 0.01, "the key sounds");
    let off = [TimedEvent { offset: 0, event: Event::NoteOff { key: 50, id: NO_ID } }];
    energy(&mut o, 200, &off);
    assert!(energy(&mut o, 50, &[]) < 1e-3, "and stops once let go");
}

#[test]
fn the_knobs_reach_the_player_and_the_telemetry_tells() {
    let mut o = make();
    o.set_param(PATCH, 1.0); // pizz
    let any = o.telemetry().expect("Octavia has telemetry");
    let tele = any.downcast::<Telemetry>().expect("of her own type");
    let s = Score::parse("tempo 120\nD3:q E3:q").unwrap();
    host(&mut o, &s, 12_000);
    let snap = tele.read();
    let now = snap.recent.iter().flatten().last().expect("a note played");
    assert_eq!(now.decision.map(|d| d.played), Some(Played::Plucked));
    assert!(snap.time > 0.2);
    // Switching to the violin: telemetry says so, and the violin plays.
    o.set_param(INSTRUMENT, 1.0);
    let s = Score::parse("tempo 120\nA4:q").unwrap();
    let out = host(&mut o, &s, 12_000);
    assert_eq!(tele.instrument(), 1);
    assert!(out.iter().any(|v| v.abs() > 1e-3));
}

#[test]
fn presets_are_the_player_profiles() {
    for (name, values) in PRESETS.iter() {
        let skill = Skill::profile(&name.to_lowercase()).expect("a profile");
        let got: Vec<f32> = values.iter().map(|v| v.1).collect();
        assert_eq!(got, vec![skill.intonation, skill.dexterity, skill.bowing, skill.composure], "{name}");
    }
    assert_eq!(Skill::default(), S, "the defaults are the concert player");
}
