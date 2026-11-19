# Spec: chip synth (working title, the director names it)

A built-in instrument for chiptune: NES / Game Boy / C64 *flavour*, not
chip emulation. Fluttersine can imitate a pulse lead, but chiptune isn't
oscillators. It's the **per-note sequences** (tracker "macros") that move
volume, duty and pitch at a fixed tick rate. That's what this synth is for.

Why it exists (from writing "The Two-Week Reel" and "Lyra Ascendant" in
Lyra, 2026-11-19): chip arps had to be faked as a stream of 36 ms notes
(clicky, "iffy"); noise drums were white noise, not pitched LFSR noise;
duty-cycle movement and volume envelopes in steps had no way in.

## Sound

| Wave | Notes |
|---|---|
| Pulse | Duty 12.5 / 25 / 50 / 75 %, plus a duty macro (below). |
| Triangle | Optional 4-bit stepped (NES-style, 32 steps), on by default. |
| Saw | For C64-ish leads. |
| Noise | **LFSR**, 15-bit ("long") and 7-bit ("short", metallic) modes. Its clock follows the key through a period table, so noise is *pitched*: kicks, snares and hats from the noise channel, like the NES. |
| Wave | A 32-step, 4-bit wavetable (Game Boy wave channel). The table is 32 parameters; a step editor draws it. |

* **Band-limiting:** "Clean" (polyBLEP, like Lyra Synth) or "Raw" (aliasing,
  authentic), per instance.
* **Lo-fi:** volume quantized to 16 steps (4-bit), and an optional output
  sample-rate reduction.

## Macros (the heart of it)

Each macro is a short sequence of steps, played per note at the **tick rate**:
60 Hz by default, or tempo-synced (1/16, 1/32, …).

| Macro | Step values | Effect |
|---|---|---|
| Volume | 0..15 | the envelope, in steps |
| Arpeggio | semitones, −24..+24 | pitch offset per tick: `0 4 7` looped = the classic chord arp |
| Pitch | cents, ±100 per step, cumulative or absolute | vibrato, slides, laser "pew" |
| Duty | 0..3 (or wave index) | duty / waveform per tick |

* Each macro has a **length** (1..32 steps), a **loop point** (steps after it
  repeat while the note is held) and a **release point** (where it continues
  after note-off).
* Parameter layout: ids are forever. Allocate the full 32 steps per macro up
  front (4 macros × (32 steps + length + loop + release)), then the rest.

## Voices and pitch

* **Mono by default.** A chip channel is one voice. Poly is optional.
* **Chord mode:** while several keys are held, the one voice cycles through
  them at the tick rate, like tracker `0xy`, with **no envelope restart**.
  Poly off, chord on: play a chord in the piano roll, hear a chip arp.
* Legato glide (portamento, tracker `3xx`) and a pitch **sweep** (NES sweep
  unit: up/down by a shift amount per tick).

## Editor

* A generated editor works out of the box (params + groups), but macros want
  **step editors**: bar graphs you paint with a mouse sweep (like the rack's
  toggle sweep, `widgets::sweep`), loop/release markers you drag, a length
  handle. This needs the director's graphic pass, in the plate language
  (charcoal, silicone keys, the 8 lights).

## Presets (starting points)

Square lead 25 % · Thin lead 12.5 % with vibrato macro · Chord arp (major,
minor, sus) · Triangle bass · Wave bass · Noise kick (pitch macro down) ·
Noise snare · Noise hat (short mode) · Echo lead (volume macro with a quiet
repeat) · "Pew" (pitch macro down fast).

## Done when

* Crate `crates/plugins/<name>` with its `PLUGINS` entry, gathered by
  `lyra-catalog`; `params.lock` updated by its test.
* Tests: LFSR period (long mode repeats after 32767 steps, short after 127);
  a chord in chord mode makes one voice step through its keys with the
  envelope continuous; macro loop and release points; every preset plays and
  falls silent; CPU in the same ballpark as Lyra Synth.
* A demo song in `crates/lyra-app/examples/` (songkit) that uses it, rendered
  with `lyra --render`.
