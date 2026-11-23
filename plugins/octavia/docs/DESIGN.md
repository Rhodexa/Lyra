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

**It needs to see ahead.** Phrase shape needs the whole phrase. Offline
that's free. In Lyra, Octavia should get the clip's notes ahead of time,
not just live note events (see "Lyra" below). Live playing from a keyboard
gets a reduced, reactive rule set.

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

## Lyra

The core crate has no dependencies and no plugin format. Lyra would wrap it
like Chrysalis or Fluttersine: `octavia-core` as a path or git dependency, plus
a small adapter implementing Lyra's `Plugin` trait. Two things the adapter
needs that Lyra's trait doesn't have yet:

1. **Look-ahead**: the notes of the clip (or the next second or two of
   them), not only this block's events. Either a new optional trait method
   ("here is your upcoming score"), or Octavia reports latency and buffers.
2. **Block-wise rendering**: today `render()` is offline. The voice already
   works frame-pair by frame-pair, so a streaming engine is a refactor, not a
   rewrite.

## Stages

1. ✅ Rules performer + recipe voice + body + renderer with plots and
   measurements.
2. Listen, tune rules and tone with Rhodexa's ears. Attack/transition layer
   (bow scratch, finger-noise on shifts), spiccato, ties, double stops.
3. Streaming engine; Lyra adapter.
4. Timbre model trained on clean-licensed solo recordings (CC0/CC-BY, or a
   commissioned session). Small: curves in, overtone levels out. CPU inference
   written by hand.
5. Maybe a learned performer, if the rules plateau.

## Data and licensing

Code: MIT OR Apache-2.0, same as Lyra. Training audio must be clearly
licensed (CC0/CC-BY, or recorded for us under terms we write). Public-domain
*compositions* are fine to perform; most *recordings* of them are not fine to
train on. If clean data can't be had, stage 4 stalls and the rules-plus-
recipe instrument is still the product.
