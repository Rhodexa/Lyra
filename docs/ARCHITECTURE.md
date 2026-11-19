# Lyra architecture

Terse source of truth. The illustrated manual is generated from this and from the code.

## The crates

```text
lyra-app ─────► lyra-model      project data + undo            (no audio, no UI)
   │  │
   │  └───────► lyra-catalog    which plugins exist, by id
   │               │
   │               ├──► plugins/fluttersine, drums, synth, sampler, effects
   │               │        │  (each: its DSP, its params, its presets)
   │               │        └──► lyra-dsp   shared building blocks (filters, envelopes, noise)
   │               ▼
   └──► lyra-engine ──────► lyra-plugin     the ONE plugin interface (trait Plugin)
```

The engine only ever sees `Box<dyn Plugin>` (think `std::unique_ptr<IPlugin>`).
A plugin crate describes itself in a `PLUGINS` list of `PluginInfo` (id,
name, parameter table, presets, a constructor); `lyra-catalog` gathers those
lists. A plugin's knobs are declared in the plugin's own crate, next to the
code that reads them.

| Crate | Owns | Must never |
|---|---|---|
| `lyra-plugin` | `Plugin` trait, `Event`, `ParamInfo`, `PluginInfo` | depend on anything |
| `lyra-dsp` | small DSP pieces every plugin may use | know about any plugin |
| `plugins/*` | one plugin family each: its sound, parameters, presets | know about the engine, the UI or each other |
| `lyra-catalog` | the list of built-ins, lookups by id | contain DSP |
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

**Layout** (`app.rs` `docked`, `areas.rs`): browser left, plugin rack right
(both fold), and the centre split into two *areas*. Each `Area` holds one
editor kind plus its own state for every editor (`Playlist`, `PianoRoll`), so
areas remember their scroll/zoom and two can show the same editor. Editing
keys go to the area under the mouse (`key_target`); F5/F7/F9 open or fold the
bottom area on an editor. Instrument and effect editors stay floating windows.

**Samples** (`samples.rs`): WAV/FLAC are decoded on the UI thread into
`lyra_plugin::SampleData` (cached per file, shared by `Arc`). The bridge hands
a sampler its sample (`Plugin::set_sample`) before the plugin goes to the audio
thread. Projects store `bundled:<path>` for Lyra's own library, else a full
path. Browser previews use the engine's preview slot (`Command::Preview`),
mixed into the master before master volume. Waveforms draw from `Peaks`
(min/max per 64 and per 4096 frames, built once per file), so a long take
zoomed out never walks every sample.

**Audio clips** mirror patterns, on two levels. `Project.audio` holds the
shared source, `Audio` (sample, trim in seconds, a gain). A `Clip`'s `source`
is `Pattern(id)` or `Audio(id)`, and each placed clip has its own start,
length, `offset` (ticks into the audio: a clip is a window onto it), gain,
and fades at its own edges. The two gains add (in dB).

Clips have an `id` (`ClipId`) that lives only while Lyra runs: given by
`Project::add_clip`, renumbered when a file is read. `UiState::clip` is the
clip last clicked: the next click on empty arrangement places a copy of it
(settings and all), and the audio-clip editor shows its window and handles. In song mode `bridge::compile_audio` turns every
audio clip into an `AudioRegion` (song ticks, seconds into the file, gain,
fades, the track's insert) and sends the list as one `AudioLane`. The engine
renders regions *from the song position*, not from triggers, so seeking into
the middle of a line plays from there; nothing about it allocates. The fade
shape is one function, `lyra_engine::fade_curve`, used by the engine and by
the drawing.

Views **only edit the project**. `bridge.rs` is the only file that talks to the
engine about edits. Live playing (notes, play/stop) goes straight to the engine
as commands.

## Time

* `PPQ = 96` ticks per quarter note, and **1 step = 24 ticks = a 16th note**.
* A step *is* a note: the rack and the piano roll edit the same `Note`s.
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

## Song mode

Clips (`lyra-model`: what they play, track, start, length) live on playlist
tracks; audio clips are below, under *Audio clips*. Patterns have stable ids, so cloning or reordering patterns never
re-points a clip. In song mode, `bridge::compile_song` flattens every clip
into **one** `Sequence`: a clip repeats its pattern to fill its length, notes
are cut at the clip's end, and muted tracks are skipped. The engine plays it
exactly like a pattern. Song mode added no real-time code at all. The bridge
compiles the target (pattern or song) on every project change and re-sends it
only when the result differs. Switching mode seeks to the top.

## Mixer

Channels play into a **mixer track** (`Channel.mixer`; 0 = master, 1–16 =
inserts). New channels take the next free insert. Per block, the engine:

1. renders each channel and adds it into its track's buffer;
2. runs every insert's effect chain **in place**, then volume/pan/mute, and
   adds the result into the master;
3. runs the master's chain, then the master volume and the safety clamp.

Effect chains (`FxChain`) are built and activated on the UI thread and
swapped in whole with `SetFxChain`; the old chain goes back as trash. The bridge
rebuilds a chain only when its list of effects changes. Knob moves are plain
`SetFxParam`. Tempo-synced effects get `Plugin::transport()` (tempo, playing,
position) before each block. Effects live in `crates/plugins/effects`, and
their editors are generated from their parameter tables, like instruments.

## Project files

`lyra-model/src/file.rs` writes a `.lyra` file as readable JSON:
`{ format: "lyra-project", version, project }`. Missing fields get their
defaults, so old files keep opening. Files from a *newer* Lyra are refused
with a clear message. `lyra-app/src/files.rs` does the disk side: crash-safe
writes (temp file + rename), native dialogs through the desktop portal (`rfd`),
`~/.local/share/lyra/backup.lyra` (every 30 s while changed, on quit, and
before New/Open), and reopening the last project. Tests that run the app set
`XDG_DATA_HOME` to a scratch folder so they never touch the real backup.

## Held notes

Drums ignore note length, synths don't. When the engine plays a sequenced
note-on, it records `(key, ticks left)` in the channel's fixed `held` list and
sends the note-off itself when the time runs out. So deleting a sounding note,
swapping the whole sequence, or a note crossing the loop end can never leave a
note hanging. Stop clears the list and sends `AllNotesOff`.

## Plugin parameters

* **Ids are forever.** A plugin's id string (`lyra.fluttersine`) is how a
  project finds it; a parameter's id is its slot in the plugin's table, and
  projects save knob values in that order. So new parameters go at the end
  (groups decide where they show), retired ones keep their slot, and plugin
  ids never change. `crates/lyra-catalog/params.lock` records every id Lyra
  has shipped; a test fails if one changes meaning, and adds new ones itself.
* Built-ins use the `lyra.` namespace (no real reverse-DNS name starts with
  it). Hosted CLAP plugins will keep their own ids, stored as `clap:<id>`.
* Continuous: `ParamInfo::new(..)` with a linear or exponential knob curve.
* Choices: `ParamInfo::choice(id, name, labels, default)`. The value is an index.
* `Plugin::groups()` names runs of ids ("Filter": 6..11). The instrument
  editor (`editor.rs`) is generated from `params()` and `groups()` alone, so any
  future plugin gets an editor for free.
* Hand-made editors take over below the editor's header for one instrument id.
  Fluttersine (id `lyra.fluttersine`) has one: `fm_editor.rs` (operator tabs, always-visible
  matrix, draggable envelopes and filter curve). It still only edits `params`.

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
