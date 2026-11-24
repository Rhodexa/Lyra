# Octavia's face (notes for when we build it)

Colour and mood: `design/vibe-2026-11-23.png`. Mauve-grey body, pink for
"on", a cream LCD, dark keys. It's the *vibe*, not the layout. Built from
Lyra's existing views, as a page inside the instrument window (no new
windows).

## What it should show: the performer thinking

The thing no other instrument has is a player making decisions, so the
screen shows those decisions, live, as the song plays:

- **Note lane**: the notes scrolling by, each coloured by how it's being
  played (from silence, new bow, slurred, staccato, bounced, plucked,
  double stop): the same colours as the curves plots and `--explain`.
- **Four strings**: which one is bowed, which ones are still ringing
  (crossings, the open-string ghost after a finger lifts).
- **The hand**: a marker on the neck, sliding when it shifts.
- **Bow**: loudness and brightness as two small moving traces (the
  concept's staircase graph is roughly this).

All of it is already in the engine (frames, events, decisions): the UI
only reads it, through Lyra's telemetry (atomics, no locks).

## Controls: only what's real

| control | engine | notes |
|---|---|---|
| Character | `bright` 0..1 | natural ↔ bright and powerful |
| Breath | `Style::breath` | rosin noise under the tone |
| Expression | `Style::expression` | 0 = deadpan … 2 = a lot |
| Vibrato | `Style::vibrato` | depth |
| Performer on/off | expression 0 + vibrato 0 | the concept's ON button: hear the rules vs none |
| Patch: Arco / Pizz | `Style::pizzicato` | a cellist doesn't decide to pluck |
| Instrument | cello / violin (viola, bass later) | |
| Double stops on/off | | |
| Take | `seed` | another performance of the same notes |

Not knobs: bow **pressure / speed / position**. Those are the performer's
job, frame by frame (the curves). Overriding them belongs to per-note data
and automation in Lyra, as the conductor's hints. **Rubato** and **tremolo**
are good ideas the engine doesn't do yet.

## Presets (the concept's "program 01 / 16")

A short bank of starting points, e.g. *Solo cellist* (defaults), *Bright
lead* (character 1), *Pizzicato*, *Deadpan* (performer off), *Violin solo*.

## Open questions

- Is a mini keyboard useful inside Lyra, which already has a piano roll?
  (It would need live playing, i.e. notes whose length isn't known yet.)
- Logo: integrated into the face, never a corner sticker (Lyra rule).
