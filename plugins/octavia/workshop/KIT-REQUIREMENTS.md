# What Octavia needs from Lyra's plugin kit

A handoff for the Lyra session that builds the minimal plugin kit. Written
from Octavia's side on 2026-11-25. It says *what* is needed and why, not how
Lyra should build it; that's the Lyra session's call. Background (how Lyra
draws, how built-ins are split today): `workshop/INTEGRATION.md`.

Octavia is a **built-in** for now (compiled into Lyra, living in its own
folder). Everything below is CLAP-shaped so that a CLAP build stays
possible, but nothing here requires CLAP.

## The goal

A plugin = **its own folder** (`../LyraRelated/Octavia`) with its sound, its
editor, its assets, docs and tests. Lyra only needs to *list* it. Today the
editors live in `lyra-app` and `plugin_look.rs` mixes the shared look with
synth-specific pieces; the kit should be the small shared part only.

## A. The sound side (`lyra-plugin`)

*Done 2026-11-25: note ids on note events (`NoteId`, `NO_ID` like CLAP's
-1), the `lyra.note-lookahead` extension (`Plugin::note_lookahead`,
`Plugin::upcoming`, `UpcomingNote`), `Plugin::latency`. Per-note data is
still to come. C is done too (`octavia_core::telemetry`).*

| need | today | why Octavia needs it | CLAP equivalent |
|---|---|---|---|
| parameter table, continuous + choice, groups | ✅ `ParamInfo`, `groups()` | character, breath, skills, patch… | `clap.params` |
| presets | ✅ `Preset` | player profiles (concert, student, virtuoso, session) | preset-load / factory presets |
| note on / off with velocity | ✅ `Event::NoteOn/Off` | the notes | `clap.note-ports`, note events |
| transport (tempo, position, playing) | ✅ `Plugin::transport` | knowing where the song is (see C) | transport events |
| telemetry (atomics, UI reads) | ✅ `Plugin::telemetry` | the faceplate's live view | the plugin GUI reading its own DSP |
| **upcoming notes, ~1 s ahead** | ❌ | the performer shapes phrases, slurs, bowing and fingering from what's coming. Without it Octavia must delay its output by 1 s | none: a **custom extension** (CLAP allows them, e.g. `lyra.note-lookahead`) |
| **report latency** | ❌ | fallback when a host can't give upcoming notes: Octavia delays itself 1 s and says so, so the host compensates | `clap.latency` |
| **note identity** (an id per note, so "on" and "off", and later per-note data, refer to one note) | ❌ (only key numbers) | double stops: two notes at once on related keys; later per-note hints | note ids in CLAP note events |
| **per-note data** (later, not needed to start) | ❌ | the conductor's hints from the piano roll: tremolo here, accent there, slur these | note expressions / custom per-note data |

**Upcoming notes, what exactly.** For each note: start (seconds or ticks),
length, key, velocity, note id. Delivered in start order, at least 1 s
before each starts, and updated if the user edits the clip while it plays.
Octavia already handles this internally (`Engine::add_note`); it's the host
side that's missing.

## B. The editor side (the kit proper)

What an Octavia editor must be able to do, and nothing more:

1. **Be its own crate**, in Octavia's folder, depending only on the kit and
   on `octavia-core` (for pure helpers), not on `lyra-app`.
2. **Read and change parameters with undo**, as gestures: begin, change…,
   end (one undo step per drag). Like CLAP's begin/end gesture.
3. **Read telemetry**: get the plugin's telemetry object (as Chrysalis does)
   and downcast it to Octavia's own type.
4. **Draw with egui** inside a rect Lyra gives it, at Lyra's UI scale, and ask
   for repaints while something moves (the performer view animates during
   playback).
5. **Own its look completely**: its palette, its fonts (a dot-matrix /
   segment LCD face, a UI font), its sizes. Lyra's redesigns must never
   reach it (the reason `plugin_look.rs` exists today).
6. **Load its own assets** from its own folder (logo, character art),
   ideally with the live-reload Lyra already has for artwork.
7. **Shared behaviour, optional**: the kit may offer Lyra's knob/fader
   *behaviour* (drag, Shift fine, wheel nudge, double-click reset) so hands
   learn it once, with the plugin doing its own *painting*.
8. **Registration**: one line telling Lyra "editor for `lyra.octavia` is
   this function".

What the kit must **not** contain: any plugin's colours, drawings, logos or
layout (they move into their plugins), nor anything from Lyra's own theme.

## C. Octavia's telemetry (so the kit can carry it)

Settled by the faceplate mock (2026-11-25), which plays real renders: this
is what its LCD actually reads (`docs/FACEPLATE.md`). Written by the audio
thread at the end of each block, read by the UI at up to 60 Hz; atomics or
a seqlock, never a lock the audio thread waits on. The mock had to
*reconstruct* some of it from events (finger lifts from taps, held fingers
from ring events); the engine knows these directly and should say them.

```rust
// a sketch, not an API: names and packing are the Octavia session's call
struct Telemetry {
    // now (latest frame)
    loudness: f32, brightness: f32, grit: f32,       // 0..1
    pitch: f32, pitch2: f32,                         // fractional MIDI (0 = none)
    string: u8, string2: u8,                         // 0 = lowest
    // the tuner, computed by the engine so the UI needn't sample 250 Hz frames:
    // where the note sits vs. written (cents, onset glide excluded, ~0.15 s
    // mean) and how far the vibrato swings
    cents: Option<f32>, vib_lo: f32, vib_hi: f32,
    // the left hand
    hand: f32,                                       // lowest semitone it covers
    strings: [StringState; 4],
    // the player
    workload: f32, composure: f32,                   // 0..1
    // decisions: the last ~10 played and the next 4 planned (the look-ahead)
    recent: Ring<NoteDecision, 10>, upcoming: [Option<NoteDecision>; 4],
    seq: u64,                                        // bumps when anything changes
}
struct StringState {
    sound: Silent | Bowed | Plucked | Ringing { level: f32 },
    finger: Option<f32>,     // stopped pitch (fractional MIDI) if a finger is down
    sliding_from: Option<f32>, // set while a legato shift slides along it
}
struct NoteDecision {
    start: f64, len: f64, pitch: u8, pitch2: Option<u8>,
    played: Played, down: bool, string: u8, open: bool,
    hand: f32, shift: f32, workload: f32, composure: f32,
}
```

Static, once per instrument: open-string pitches, `hand_span`, the names of
the strings (C G D A / G D A E).

## D. Order of work

1. Kit (this handoff): the shared minimum, synth-specific code moved out of
   `plugin_look.rs`, editors able to live in their plugin's crate.
2. Octavia adapter (Octavia session): parameters, presets, notes in and
   audio out; playable with Lyra's generic editor.
3. Upcoming-notes extension + latency (Lyra + Octavia).
4. Telemetry (Octavia).
5. Faceplate port (`docs/FACEPLATE.md`, approved; spec `docs/design/faceplate.html`).
