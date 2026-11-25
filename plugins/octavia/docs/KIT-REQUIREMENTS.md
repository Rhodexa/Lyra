# What Octavia needs from Lyra's plugin kit

A handoff for the Lyra session that builds the minimal plugin kit. Written
from Octavia's side on 2026-11-25. It says *what* is needed and why, not how
Lyra should build it; that's the Lyra session's call. Background (how Lyra
draws, how built-ins are split today): `docs/INTEGRATION.md`.

Octavia is a **built-in** for now (compiled into Lyra, living in its own
folder). Everything below is CLAP-shaped so that a CLAP build stays
possible, but nothing here requires CLAP.

## The goal

A plugin = **its own folder** (`../LyraRelated/Octavia`) with its sound, its
editor, its assets, docs and tests. Lyra only needs to *list* it. Today the
editors live in `lyra-app` and `plugin_look.rs` mixes the shared look with
synth-specific pieces; the kit should be the small shared part only.

## A. The sound side (`lyra-plugin`)

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

Updated by the audio thread at the end of each block, atomics only, the
UI only reads:

* **the note being played**: how it's played (from silence, new bow, slurred,
  staccato, bounced, tremolo, plucked), down/up bow, string, open or
  stopped, double stop + its second note, hand position and last shift;
* **the bow, now**: loudness, brightness, grit, noise (the frame values);
* **the player, now**: workload, composure;
* **the strings**: for each of the four, whether it's bowed, ringing on
  (crossing / finger lift) and how loud;
* **recent decisions**: a small ring buffer (the last ~16 notes with their
  decisions) for the LCD's note lane and event log;
* **pitch vs. target**: how far the current note is from the written pitch
  (intonation, vibrato), for a tuner-like readout.

## D. Order of work

1. Kit (this handoff): the shared minimum, synth-specific code moved out of
   `plugin_look.rs`, editors able to live in their plugin's crate.
2. Octavia adapter (Octavia session): parameters, presets, notes in and
   audio out; playable with Lyra's generic editor.
3. Upcoming-notes extension + latency (Lyra + Octavia).
4. Telemetry (Octavia).
5. Faceplate (from `docs/FACEPLATE.md` and Rhodexa's design).
