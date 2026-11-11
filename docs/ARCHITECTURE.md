# Lyra architecture

Terse source of truth. The illustrated manual is generated from this and from the code.

## The crates

```text
lyra-app ──────────► lyra-model     project data + undo        (no audio, no UI)
   │  │
   │  └──────────────► lyra-dsp     built-in instruments/effects
   │                      │
   └──► lyra-engine ◄─────┘ (via)  lyra-plugin   the ONE plugin interface
```

| Crate | Owns | Must never |
|---|---|---|
| `lyra-plugin` | `Plugin` trait, `Event`, `ParamInfo` | depend on anything |
| `lyra-dsp` | drums (M1), synths/effects (M2–M3) | know about the engine or UI |
| `lyra-engine` | audio thread: sequencer, channel mixing, FIFOs | allocate, lock or block in `process` |
| `lyra-model` | `Project`, `Pattern`, `Note`, `History` | know about audio or UI |
| `lyra-app` | window, views, theme, keyboard, sound card, bridge | send engine commands from views |

## Threads: two, and three one-way pipes

```text
 UI thread ("main loop", ~60-120 fps)        audio thread ("ISR", every ~5 ms)
 ───────────────────────────────────         ─────────────────────────────────
 views edit Project
 Bridge::sync diffs Project ── Commands ───► Engine::process
                         ◄──── Trash ────── old Boxes (UI thread frees them)
 meters, LEDs, playhead ◄──── atomics ───── peaks, trigger counters, position
```

* The audio callback is treated like an interrupt handler. Everything it will
  ever need is allocated beforehand on the UI thread and sent as a `Box`.
* Replacing data on the audio side sends the old `Box` back through the trash
  FIFO, so memory is always freed on the UI thread.
* There are no mutexes anywhere in the audio path.

## Data flow for one edit

1. You click a step. `rack.rs` calls `History::checkpoint`, then `Pattern::set_step`.
2. At the end of the frame, `Bridge::sync` sees that the pattern differs from the
   copy it last sent. It compiles a `Sequence` (a flat, sorted note list) and
   sends `Command::SetSequence`.
3. On its next callback, the engine swaps the sequence in and returns the old one as trash.

Views **only edit the project**. `bridge.rs` is the only file that talks to the
engine about edits. Live playing (notes, play/stop) goes straight to the engine
as commands.

## Time

* `PPQ = 96` ticks per quarter note, and **1 step = 24 ticks = a 16th note**.
* A step *is* a note: the rack and (later) the piano roll edit the same `Note`s.
* The engine schedules sample-accurately. Each block it finds the notes whose
  tick falls in `[pos, pos + block)`, handling the loop wrap, and converts them
  to sample offsets.
* Swing delays odd 16ths by up to ⅓ step.
* For the future video lane, wall-clock time is derived from ticks and tempo.
  The tempo map has to stay seconds-accurate.

## Pattern length

Patterns are dynamic. `Pattern::play_length()` runs to the end of the last bar
that has a note starting in it, and never below `min_length` (the rack's bars
menu). The engine gets that length each time the sequence is compiled.
`Command::Seek` moves the playhead and the start marker: Play starts there and
Stop returns there.

## Held notes

Drums ignore note length, synths don't. When the engine plays a sequenced
note-on, it records `(key, ticks left)` in the channel's fixed `held` list and
sends the note-off itself when the time runs out. So deleting a sounding note,
swapping the whole sequence, or a note crossing the loop end can never leave a
note hanging. Stop clears the list and sends `AllNotesOff`.

## Plugin parameters

* Continuous: `ParamInfo::new(..)` with a linear or exponential knob curve.
* Choices: `ParamInfo::choice(id, name, labels, default)`. The value is an index.
* `Plugin::groups()` names runs of ids ("Filter": 6..11). The instrument
  editor (`editor.rs`) is generated from `params()` and `groups()` alone, so any
  future plugin gets an editor for free.

## Music theory

`lyra-model/src/theory.rs` holds the scales and chords. The project stores a
key root and a scale. The piano roll uses them to highlight rows and to build
"in key" chords: stacked scale thirds from the clicked note, which snaps into
the key first.

## Conventions

* Parameter `id` = its index in the plugin's `params()` list. Built-in instrument
  ids (`"lyra.kick"`) are saved in projects and are never renamed.
* Knobs everywhere: drag = change, Shift = fine, wheel = nudge, double-click = reset.
* **egui lock rule:** never call another `ctx`/`ui.ctx()` method inside a
  `ctx.input(|i| …)` (or `memory`, `data`…) closure. egui's context lock is not
  re-entrant, so the nested call deadlocks the UI thread. Copy the values out
  first, then call. (This froze the app on the first click in v0.1.)
* Theme tokens and textures live only in `lyra-app/src/theme.rs`. Generated
  textures are placeholders for hand-made art.
