# Lyra architecture

Terse source of truth. The illustrated manual is generated from this and from the code.

## The crates

```text
lyra-app ─────► lyra-model      project data + undo            (no audio, no UI)
   │  │
   │  └───────► lyra-catalog    which plugins exist, by id
   │               │
   │               ├──► plugins/fluttersine, chrysalis, drums, synth, sampler, effects
   │               │        │  (each: its DSP, its params, its presets)
   │               │        └──► lyra-dsp   shared building blocks (SVF, FFT, fast sine, noise, sync lengths)
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

## Automation

`lyra_model::Automation` is a shared curve (points of knob *travel* 0..1,
each with the tension of the line to the next, and a `length` it repeats
at) plus `links`: the controls it moves, each over its own travel range.
`Clip::of_automation` places it, like any clip. Targets
(`lyra_model::Target`) name controls by stable ids: channel ids, parameter
ids, and `EffectId`s (effect slots carry their own id now, so reordering a
chain doesn't retarget anything).

`automation::compile` (app) turns the clips into one `AutoLane` per control
for the engine: the curve along the whole song (window, offset and repeats
applied, cut lines sampled), holding between clips, plus the control's
`ParamInfo` (its knob law) and its own value (`rest`). The engine evaluates
each lane at the song position every 64 samples while playing (chasing is
free: the value only depends on the position), sets the control directly,
and on Stop puts every control back at `rest`. Song mode only.

**Knobs show automation.** The engine moves automated controls itself and
leaves the project's values alone (they are where controls rest). Each frame
while the song plays, the app evaluates every lane at the playhead
(`Bridge::automated_at`, cheap: a curve depends only on the position) and
`automation::shown` serves those values to the editors: Chrysalis,
Fluttersine, the generic editor and the effect windows read their knobs
through it. Dragging an automated knob edits its resting value. It's the
host's job, as in CLAP; plugins aren't involved. (Mixer faders don't follow
yet.)

"Automate" automates the control changed last: the bridge compares the
project with what it sent last frame (`Project::changed_control`), so any
editor, and later any plugin window, feeds it.

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
`SetFxParam`. Every plugin, instrument or effect, gets `Plugin::transport()`
(tempo, playing, position) before each block: tempo-synced delays and pumps,
and Fluttersine's synced LFOs, which lock to the song position while it plays. Effects live in `crates/plugins/effects`, and
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

## Rendering

`lyra-app/src/render.rs` plays the song through the same `Bridge` and engine
as live playback, as fast as the CPU goes: the bridge sets up the engine, the
sequence gets extra length so the song doesn't loop, and the render runs on
through the tails until half a second of silence (15 s at most). 24-bit WAV.
Parts: the mix, one channel alone (stems), or the audio clips alone. The
project menu runs it on a thread (Export WAV / stems, Ctrl+E / Ctrl+Shift+E);
`lyra --render song.lyra [out.wav] [--stems dir]` runs it without a window or a
sound card, which is also how sound changes get checked: render a song before
and after, and compare.

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
  Opening a project gives every channel and effect the parameters its plugin
  gained since (`files::fill_new_params`, defaults at the end), so editors can
  index the whole table.
* **A plugin's whole state is its parameters** (CLAP-shaped: a CLAP host
  sees parameters plus an optional state blob, and we use only the first).
  Chrysalis even keeps its drawn LFO shapes as parameters (32 points × x, y,
  bend per LFO), so undo, saving, presets and later automation need nothing
  extra. The one thing that isn't a number, an imported WAV, travels as the
  channel's `sample` (`Plugin::set_sample`, built on the UI thread). If a
  plugin ever needs more than one such file, that is the moment for a real
  state blob.
* Built-ins use the `lyra.` namespace (no real reverse-DNS name starts with
  it). Hosted CLAP plugins will keep their own ids, stored as `clap:<id>`.
* **Instruments are stereo, and velocity is loudness.** A synth makes stereo
  by default (pan, unison spread, width); only drums and deliberately mono
  basses sit dead centre. Note velocity sets a voice's loudness, fully by
  default (an amount knob may soften it); per-voice velocity → timbre is extra.
* Volumes are linear gains with `Curve::Fader` and the `"gain"` unit: the knob
  feels like a console fader and reads in dB.
* Continuous: `ParamInfo::new(..)` with a linear or exponential knob curve.
* Choices: `ParamInfo::choice(id, name, labels, default)`. The value is an index.
* `Plugin::groups()` names runs of ids ("Filter": 6..11). The instrument
  editor (`editor.rs`) is generated from `params()` and `groups()` alone, so any
  future plugin gets an editor for free.
* Hand-made editors take over below the editor's header for one instrument id.
  Each lives in its plugin's folder and talks to Lyra only through the plugin
  kit (see "Plugin kit" below). Fluttersine (id `lyra.fluttersine`) has one:
  `crates/plugins/fluttersine/editor` (a header with the hover
  readout, preset, logo and Tone/Time/Volume/Glide; operator / filter / LFO / MOD
  tabs on the left; MATRIX / ALGORITHMS tabs on the right; draggable envelopes
  and filter curve; flat keys; laid out with `flex`). It still only
  edits `params`. Chrysalis (`lyra.chrysalis`) has `crates/plugins/chrysalis/editor`
  (the table studio is its `studio.rs`),
  with **its own look** (the user's call: the sound is aggressive, the editor
  shouldn't whisper): teal-black panels, dark screens with glowing curves,
  neon knobs (`lyra_kit::knob_painted`: Lyra's knob feel, its own drawing),
  green/cyan for the sound and violet for modulation, the emblem as a badge
  in the header. Everything is on one page except modulation (ENV / LFO /
  MATRIX tabs). It draws with the plugin's own functions (`Table::sample`,
  `warp_preview`, `drive_shape`, `curve::bend`), so the pictures are what
  plays. Its palette lives in its crate, not in `theme.rs`: it is
  Chrysalis's, not Lyra's.
* **Live state for editors:** `Plugin::telemetry()` (default none) returns an
  object the audio thread updates with atomics only, at the end of each
  block; the bridge keeps it per channel when it makes the plugin
  (`Bridge::live`) and the editor gets it from `Host::telemetry()` and downcasts it.
  Chrysalis shares its newest voice's playing positions, LFO phases,
  envelope stages, matrix sums and output level (`telemetry.rs`); its editor
  animates from that and repaints only while something moves.
* **Your presets** (`user_presets.rs`): one readable JSON file each in
  `<data dir>/presets/<plugin id>/<name>.lyrapreset` (`{ format, version,
  plugin, name, params }`, params by id; parameters added since get their
  defaults on load). Every preset picker (Chrysalis, Fluttersine, the
  generic editor's Presets menu) lists the factory presets, then yours with
  an × to delete, then "Save as".
* Plugin artwork (Fluttersine's wordmark, Chrysalis's emblem) lives in each
  editor's `assets/` folder and is live-reloaded from there while Lyra runs
  from the source tree (`lyra_kit::assets::Artwork`).

## Plugin kit

`crates/lyra-kit`: what a plugin's editor needs from Lyra, and nothing else,
so an editor can live in its plugin's own crate (even outside this repo, by
path). It holds **no colours, drawings, logos or layout of any plugin, and
nothing from Lyra's theme**: a plugin owns its look, and Lyra's redesigns
can't reach it.

* **`Host`** (trait): the channel as its editor sees it. Parameters as shown
  (automation) and as stored; changes in gestures like CLAP's: `begin_edit()`
  once per drag or click (one undo step), then `set_param` as often as needed
  (`lyra_kit::tracked` does this for knob-like widgets). Also the telemetry
  object, the channel's name, the tempo, presets (list / save / delete), the
  channel's sample slot, and Lyra's data folder. Lyra's implementation is
  `ChannelHost` in `lyra-app/src/plugin_host.rs`.
* **`Editor`**: `{ plugin, show, fonts }`. Registering one is one line in
  `plugin_host::EDITORS`; `fonts` adds the plugin's own font families at
  startup, before Lyra's.
* Behaviour, never painting: `knob_turn` / `knob_painted` (drag, Shift fine,
  wheel, double-click reset), `wheel_step`, `double_click`, `flex` (below), the
  preset list for a popup (`presets::menu`, ink passed in), and `assets::Artwork`
  (a PNG built in and live-reloaded from the plugin's folder).

Fluttersine and Chrysalis also share `crates/plugins/shared-look`: their look
as it was on 2026-11-25 (colours, lights, LED, LCD, knob painter, icons,
sizes, and the egui style written out as code). It belongs to them, not to
the kit. A new plugin (Octavia) brings its own look and needs only the kit.

## Chrysalis (wavetable synth)

`crates/plugins/chrysalis`. A table is up to 256 frames × 2048 samples, kept
at 20 band-limited levels half an octave apart (level `L` keeps harmonics up
to ⌊1024 / 2^(L/2)⌋, at 32 samples per harmonic, max 2048). An oscillator
picks the fullest level whose top harmonic stays below Nyquist, reads the
five fullest levels with cubic interpolation and the rest linearly: images
and aliasing stay at −74 dB or lower across the keyboard (tested). The nine
built-in tables are generated additively once (`table::builtin()`, ~37 MB,
~0.2 s; `lyra_chrysalis::warm_up()` starts it on a thread at launch).
Per voice: 2 oscillators (warp, unison ≤ 16), sub, noise, drive, 2 filters
(SVF / formant / comb), 4 envelopes, 4 per-note LFOs (free ones are shared
and song-locked when synced), 16 matrix slots, all at a 32-sample control
rate with per-sample ramps for position and levels. The inner loop runs one
sample at a time with the unison voices inside it (independent phases keep
the CPU busy); 16 notes × 2 oscillators × 8 unison costs ~12 % of a core.

## Music theory

`lyra-model/src/theory.rs` holds the scales and chords. The project stores a
key root and a scale. The piano roll uses them to highlight rows and to build
"in key" chords: stacked scale thirds from the clicked note, which snaps into
the key first.

## UI layout

* **The flexbox: `lyra-kit/src/flex.rs`** (in the kit, so plugin editors share it; `crate::flex` in Lyra). Layout by arithmetic. Take a
  rectangle and split it into a row or a column of parts. Each part is fixed
  (`Px(28.0)`) or a share of what's left (`Grow(1.0)`), with a gap between parts:

  ```rust
  let [tabs, bar, plates] = flex::col(left, [Px(32.0), Px(40.0), Grow(1.0)], 0.0);
  let [name, field] = flex::row(r, [Px(80.0), Grow(1.0)], 6.0);
  let cells = flex::even_row(r, 7, 4.0);   // seven equal columns (also even_col)
  let key = flex::center(r, vec2(28.0, 28.0));
  ```

  Padding is `Rect::shrink` / `shrink2` on the rectangle before splitting it.
  Hand-made views (the audio-clip editor, the automation editor, Chrysalis)
  are built this way: allocate one rectangle, split it, paint and interact in
  the parts. Pieces then line up and fill their space, with no pixel numbers
  scattered around. Prefer it to egui's `horizontal` / `vertical` for
  anything that must line up.
* **One row = one height, one column = one width.** Controls stacked in a
  column share its left edge and width (the Recorder plate). Keys in a row
  share its height. When something won't line up, look for a part sized by
  hand next to one sized by flex.
* **Areas have no padding.** The two centre areas (`app::area_ui`) are
  frames with no margin. The header row pads itself (8 px at the sides) and
  ends in an engraved line. The editor gets the rest of the area exactly, so
  timelines, rulers and bars run edge to edge. An editor that wants padding
  brings its own (the step sequencer wraps itself in an 8 px frame).
* **Tabs vs mode switch.** Folder tabs (`tab_row`) switch between tools or
  pages that sit under them, like EDITOR / RECORDER, two tools that work
  together. The mode switch (`choice`) is for one setting with a few values
  (SONG / PAT).

## Conventions

* Parameter `id` = its index in the plugin's `params()` list. Built-in instrument
  ids (`"lyra.kick"`) are saved in projects and are never renamed.
* Knobs everywhere: drag = change, Shift = fine, wheel = nudge, double-click = reset.
* **egui lock rule:** never call another `ctx`/`ui.ctx()` method inside a
  `ctx.input(|i| …)` (or `memory`, `data`…) closure. egui's context lock is not
  re-entrant, so the nested call deadlocks the UI thread. Copy the values out
  first, then call. (This froze the app on the first click in v0.1.)
* **Layout:** see "UI layout" below.
* Scroll bars everywhere are Lyra's (set once in `theme::apply`): solid,
  always visible, one width, well and thumb colours; egui's default thin
  floating bars are gone.
* Theme tokens and textures live only in `lyra-app/src/theme.rs`. Generated
  textures are placeholders for hand-made art.
