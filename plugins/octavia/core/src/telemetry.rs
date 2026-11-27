//! What the player is doing right now, for a faceplate to draw.
//!
//! A [`Snapshot`] is plain data (`Copy`, no heap): the engine fills one at
//! the end of a block ([`crate::Engine::snapshot`]), a host hands it to the
//! UI thread however it likes. Everything here is read from the engine's
//! own state, never reconstructed: where the fingers are, which strings
//! ring, what was decided for each note. The contract is
//! `workshop/KIT-REQUIREMENTS.md` section C; `docs/FACEPLATE.md` says how the
//! screen uses it.

use crate::curves::Frame;
use crate::performer::Decision;

/// How many past notes a snapshot carries (the LOG page's ten rows).
pub const RECENT: usize = 10;
/// How many coming notes it carries (the NEXT panel).
pub const UPCOMING: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// Engine time, seconds.
    pub time: f64,
    /// The sound now: pitch, loudness, brightness, grit, strings.
    pub frame: Frame,
    /// Where the note sits against the written pitch, if one is bowed and
    /// past its onset glide (see [`Tuner`]).
    pub tuner: Option<Tuner>,
    /// The left hand: the lowest semitone above the open strings it covers.
    pub hand: f32,
    pub strings: [StringState; 4],
    /// The player, 0..1.
    pub workload: f32,
    pub composure: f32,
    /// The notes played so far, oldest first; the last one is the current
    /// note (or the last one played).
    pub recent: [Option<NoteInfo>; RECENT],
    /// The notes coming next, in order. The performer decides a note only
    /// just before it starts, so the later ones have no decision yet.
    pub upcoming: [Option<NoteInfo>; UPCOMING],
}

/// The tuner, in cents against the written note, over the last 0.15 s: the
/// average (where the note sits) and how far the vibrato swings.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tuner {
    pub cents: f32,
    pub lo: f32,
    pub hi: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StringState {
    pub sound: StringSound,
    /// Where a finger stops it (fractional MIDI pitch), if one does: the
    /// engine's real pitch while it's played (intonation, vibrato, slides).
    pub finger: Option<f32>,
    /// While a legato shift slides along this string: the pitch it left.
    pub sliding_from: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum StringSound {
    #[default]
    Silent,
    /// The bow is on it.
    Bowed,
    /// Plucked a moment ago.
    Plucked,
    /// Ringing on its own (a note into a rest, a string crossed away from),
    /// roughly this loud (0..1).
    Ringing(f32),
}

/// The left hand's plan for a coming note.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intent {
    /// String (0 = lowest).
    pub string: u8,
    /// Hand position (lowest semitone it covers) and how far it moves there.
    pub hand: f32,
    pub shift: f32,
}

/// One note as the performer sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteInfo {
    pub start: f64,
    pub len: f64,
    pub pitch: f32,
    /// A double stop's lower note.
    pub pitch2: Option<f32>,
    /// How it is played; `None` while not decided yet (the performer
    /// commits just before a note starts).
    pub decision: Option<Decision>,
    /// Before that: where the left hand means to play it, from the plan
    /// over everything in sight. It can change as more music comes into
    /// view; the decision is what was actually done.
    pub intent: Option<Intent>,
    /// Played on an open string.
    pub open: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::cello;
    use crate::performer::{Played, Style};
    use crate::render::{render_score, Engine};
    use crate::score::Score;

    /// Streams a score block by block and yields a snapshot after each.
    fn snapshots(s: &Score) -> Vec<Snapshot> {
        let mut e = Engine::new(&cello(), &Style::default(), 48_000.0, 0.0, false);
        for n in &s.notes {
            e.add_note(n.clone());
        }
        let mut out = vec![0.0f32; 256];
        let end = s.notes.last().map_or(0.0, |n| n.end()) + 1.0;
        let mut snaps = Vec::new();
        while e.time() < end {
            e.process(&mut out);
            snaps.push(e.snapshot());
        }
        snaps
    }

    #[test]
    fn every_decision_shown_is_the_one_explain_prints() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../scores/showcase.txt")).unwrap();
        let s = Score::parse(&text).unwrap();
        let explain = render_score(&s, &cello(), &Style::default(), 48_000.0, 0.0).decisions;
        let snaps = snapshots(&s);
        for (i, n) in s.notes.iter().enumerate() {
            // Just after it starts, it is the newest note in the snapshot.
            let snap = snaps.iter().find(|x| x.time >= n.start + 0.01).expect("reached");
            let now = snap.recent.iter().flatten().last().expect("a note");
            assert_eq!(now.start, n.start, "note {i}");
            assert_eq!(now.decision, Some(explain[i]), "note {i} at {:.2}s", n.start);
            // A note shows in NEXT before it starts, decided once committed.
            let before = snaps.iter().rev().find(|x| x.time < n.start - 0.13).filter(|_| i > 0);
            if let Some(b) = before {
                if let Some(u) = b.upcoming.iter().flatten().find(|u| u.start == n.start) {
                    assert_eq!(u.decision, None, "note {i} isn't decided this early");
                }
            }
        }
    }

    #[test]
    fn coming_notes_show_the_hands_plan_and_it_is_kept() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../scores/showcase.txt")).unwrap();
        let s = Score::parse(&text).unwrap();
        let explain = render_score(&s, &cello(), &Style::default(), 48_000.0, 0.0).decisions;
        let snaps = snapshots(&s);
        let (mut shown, mut kept) = (0, 0);
        for (i, n) in s.notes.iter().enumerate().filter(|(_, n)| n.double.is_none()) {
            // Half a second before it starts, NEXT knows where it'll be played…
            let Some(b) = snaps.iter().rev().find(|x| x.time < n.start - 0.5) else { continue };
            let Some(u) = b.upcoming.iter().flatten().find(|u| u.start == n.start) else { continue };
            let intent = u.intent.expect("a coming note has the hand's plan");
            shown += 1;
            // …and usually that's what happens (more music can change it).
            if intent.string == explain[i].string {
                kept += 1;
            }
        }
        assert!(shown >= 10, "{shown}");
        assert!(kept * 10 >= shown * 8, "the plan mostly holds: {kept} of {shown}");
    }

    #[test]
    fn the_bowed_string_has_a_finger_at_the_real_pitch_and_the_tuner_reads() {
        // A long stopped note (E3: no open string has it).
        let s = Score::parse("tempo 60\nE3:w").unwrap();
        let snaps = snapshots(&s);
        let mid = snaps.iter().find(|x| x.time >= 2.0).unwrap();
        let d = mid.recent.iter().flatten().last().unwrap().decision.unwrap();
        let st = mid.strings[d.string as usize];
        assert_eq!(st.sound, StringSound::Bowed);
        let f = st.finger.expect("stopped, not open");
        assert!((f - mid.frame.pitch).abs() < 1e-4, "the finger is where the sound is");
        let t = mid.tuner.expect("bowed and settled");
        assert!(t.lo <= t.cents && t.cents <= t.hi && t.cents.abs() < 50.0, "{t:?}");
        assert!(t.hi - t.lo > 2.0, "the vibrato swings: {t:?}");
        // Afterwards the bow is off and nothing is bowed.
        let after = snaps.last().unwrap();
        assert!(after.strings.iter().all(|s| s.sound != StringSound::Bowed));
        assert_eq!(after.tuner, None);
    }

    #[test]
    fn plucks_show_as_plucks() {
        let s = Score::parse("tempo 60\npizz D3:q").unwrap();
        let snaps = snapshots(&s);
        let at = snaps.iter().find(|x| x.time >= 0.05).unwrap();
        let d = at.recent.iter().flatten().last().unwrap().decision.unwrap();
        assert_eq!(d.played, Played::Plucked);
        assert_eq!(at.strings[d.string as usize].sound, StringSound::Plucked);
        assert_eq!(at.tuner, None);
    }
}
