# Octavia: design

A string player and the instrument it plays, cheap enough for a CPU, open
enough to ship with Lyra (MIT OR Apache-2.0).

## The idea in one picture

```
notes + articulations ──▶ PERFORMER ──▶ CURVES ──▶ INSTRUMENT ──▶ audio
(from the piano roll)     rules now,    the         voice + plucks
                          maybe a tiny  contract    → sympathetic strings
                          net later                 → body
```

**Two halves, one contract.** The performer decides *what the hands do*. The
instrument decides *what that sounds like*. Between them sit **curves**:
pitch, loudness, brightness, noise and string, 250 times a second, plus
pluck events. Curves save as CSV (`*.curves.csv`), can be plotted and
edited by hand, and can be rendered without the performer. So we can always
tell which half is at fault.

## Performer (`performer.rs`)

Rules, each small and named, loosely after the KTH performance-rule research
(Sundberg, Friberg) and ordinary cello habits: phrase arches, high-loud,
short-soft, bow-change dips vs. seamless slurs, audible shifts on big
slurred leaps, delayed vibrato that follows loudness (none on short notes or
open strings), intonation that settles after the attack, leading tones that
lean up, string choice that stays put and avoids open strings on long notes.

**Rules before learning.** A trained performer only replaces a rule when it
beats it in a blind listen.

**It sees exactly one second ahead** (`LOOKAHEAD`). Every decision uses only
notes already started or starting within that window, so the result is the
same offline, in Lyra (which can hand over upcoming notes), and in any other
host (which would get one second of reported latency). Tests check this bit
for bit.

**It infers; hints overrule.** A note is start, length, pitch and dynamic.
`articulation` and `slur` default to "auto": overlapping notes slur, fast
touching runs slur four to a bow, short notes with gaps become staccato,
clearly louder notes get accents. Score marks, and later Lyra's per-note
data and automation, act as the conductor's hints. **Pizzicato is a patch**
(`Style::pizzicato`), not an inference: a cellist doesn't decide to pluck.

## Instrument

- **Voice** (`voice.rs`): up to 160 exactly-harmonic overtones sharing one
  phase (one multiply-add per overtone per sample, Chebyshev recurrence),
  shaped by brightness (slope, bow-position notches, damping), with slow
  per-overtone shimmer and tiny period jitter. Bow noise is pulsed once per
  period. *This recipe is where a small trained "timbre model" would slot
  in later: curves in, overtone and noise levels out.*
- **Plucks** (`pluck.rs`): string loops for pizzicato.
- **Sympathetic strings** (`sympathetic.rs`): the four open strings ring
  along faintly.
- **Body** (`body.rs`): about 40 resonant modes (the named low ones from the
  acoustics literature plus a seeded scatter for grain), and a radiation
  roll-off (a body that size barely radiates its lowest fundamentals).

Cost today: about 20× real time for one cello on one core, untuned.

## Hosting (Lyra, and maybe others)

`Engine` is the whole instrument, streaming: `add_note`, then `process(block)`
of any size. The offline renderer goes through the same engine.

**Lyra** would wrap it like Chrysalis or Fluttersine (`octavia-core` as a path
or git dependency, plus a small adapter for Lyra's `Plugin` trait). It needs
one thing Lyra's trait doesn't have yet: **upcoming notes**, at least one
second ahead (an optional trait method, "here is what's coming"). Without
it, the adapter would have to delay the sound by one second.

**Other DAWs** (CLAP/VST3, e.g. via nih-plug) only send notes as they happen:
the wrapper reports one second of latency, which hosts compensate for
automatically during playback. Notes whose length is unknown until note-off
("open notes", for live playing) aren't supported yet.

## Stages

1. ✅ Rules performer + recipe voice + body + renderer with plots and
   measurements.
2. ✅ Tuned by ear against real recordings (`octavia analyze`): spectrum,
   bow noise, finger taps, strings left ringing, finger lifts, spiccato,
   double stops, tremolo, fingering with a left hand, inference from bare
   piano rolls and MIDI.
3. ✅ Streaming engine with bounded look-ahead (identical offline and live,
   tested bit for bit).
4. ✅ The human layer (started 2026-11-24): `Skill` (intonation, dexterity,
   bowing, composure), workload and composure per note, landing errors and
   correction, slowly drifting errors, per-string tuning, selective vibrato,
   the bow arm (down/up strokes, stroke variation, late bite), grit and
   failed starts, physical speed limits. Still open: imperfect slides with
   finger noise, accidental open-string brushes, bow position as a decision,
   bow planning (hooked bowing), rubato.
5. **Lyra**: plugin kit (Lyra session, `KIT-REQUIREMENTS.md`), adapter,
   upcoming-notes extension, telemetry, faceplate (`FACEPLATE.md`).
6. Timbre model trained on clean-licensed solo recordings (Iowa MIS first).
   Small: curves in, overtone levels out. CPU inference written by hand.
7. Maybe a learned performer, if the rules plateau.

Where to start reading the engine: `crates/octavia-core/src/lib.rs` (the
pipeline), `performer.rs` (all the rules, each documented at the top),
`render.rs` (`Engine`, `Sound`), then `JOURNAL.md` for why each rule exists.

## Data and licensing

Code: MIT OR Apache-2.0, same as Lyra. Training audio must be clearly
licensed (CC0/CC-BY, or recorded for us under terms we write). Public-domain
*compositions* are fine to perform; most *recordings* of them are not fine to
train on. Known sources: **University of Iowa MIS** samples are free to use for
anything, so they're the first training candidate. The **Aalto anechoic orchestra**
recordings have restrictive terms: use them for listening and measuring only, never for
training or shipping. Anything else in `ext_sample/` is listening and measuring
material only.

If clean data can't be had, stage 4 stalls and the rules-plus-
recipe instrument is still the product.
