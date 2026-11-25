# How Lyra draws, how plugins fit, and Octavia's plan

Written for Rhodexa (project director) on 2026-11-25, from Lyra's code as it
is today. In C/C++ terms where it helps.

## 1. How anything gets on screen

Lyra uses **egui**, an *immediate-mode* UI library, drawing through OpenGL
(the `glow` backend of `eframe`).

```text
 every frame (~60/s, only while something changes)
   main loop ──► app.rs calls show() functions, top to bottom
                   rack, piano roll, mixer, … the open instrument editor
                     each one: "rectangle here, this colour; line there; text…"
                 ──► egui collects those shapes into one list
                 ──► egui turns them into triangles ──► glow ──► GPU
```

* There's no persistent widget tree, no retained objects. Each frame, code
  simply *redraws everything* from the current data. A knob is a function:
  `knob(ui, &mut value, …)` paints the knob *and* reports the drag, in the
  same call. Think of a display list rebuilt every frame by the main loop.
* **Nobody but egui touches the GPU.** Not plugins, not editors. Animations
  (Chrysalis's moving wavetables) are just "draw it again next frame with new
  numbers".
* Lyra's own pieces live in `lyra-app`: `widgets.rs` (knob, LED, LCD number,
  buttons, meters…), `theme.rs` (colours, sizes), `flex.rs` (layout).

**From HTML to Lyra:** there's no converter. The HTML/CSS designs are the
*spec*; a session read them and rewrote each element as egui drawing calls
(`rect_filled`, `line`, text with a font) using the colours and sizes from
the design. That's also how a new Octavia design would be built.

## 2. How a built-in plugin is split today

A built-in plugin is **two separate pieces**:

| piece | where | knows about | never touches |
|---|---|---|---|
| the sound (DSP) | `crates/plugins/chrysalis` | `lyra-plugin` (the trait) and `lyra-dsp` (shared DSP parts) | UI, egui, the GPU, the engine |
| the editor (UI) | `lyra-app/src/chrysalis_editor.rs` | egui, the plugin's parameters, its telemetry | the audio thread |

They talk only through two narrow channels:

```text
 editor (UI thread)                          plugin (audio thread)
   edits PARAMETERS (numbers, with undo) ──► reads them each block
   reads TELEMETRY (atomics, read-only)  ◄── writes them each block
                                             (playing positions, LFO phases…)
```

* Lyra calls the editor's `show(ui, project, history, channel, live)`; the
  editor draws with egui and edits `channel.params`. It never calls into the
  running sound engine. For pictures it may call the plugin's *pure*
  functions (Chrysalis draws its tables with its own sampling code, so the
  picture is what plays).
* **Shared code, no duplication:** DSP pieces in `lyra-dsp`; UI pieces in
  `widgets.rs` and, since today, `plugin_look.rs`, the frozen look shared by
  Fluttersine and Chrysalis so Lyra's redesign can't change them.
* **Compiling:** Cargo builds everything (Lyra, every plugin, their editors)
  into **one program**. Each crate is like a static library; a crate can live
  in *any folder*. Lyra's `Cargo.toml` just points at it (by path, like
  CMake's `add_subdirectory(../elsewhere)`, or by git URL). Change one crate
  and only that crate and its dependants recompile. Nothing has to move.

## 3. Octavia's plan

```text
 Octavia repo (stays here)                 Lyra repo
 ─────────────────────────                 ─────────
 octavia-core   the engine, no UI           lyra-app/Cargo.toml: path = "../LyraRelated/…"
   Engine::add_note, process                crates/plugins/octavia (or in this repo)
   Decisions, telemetry data                   ~200 lines: implements Plugin by
 octavia-render CLI + refs + analyze           calling octavia-core; parameter
   = the test bench for the engine             table; presets; telemetry
                                            octavia_editor.rs: the faceplate
                                               (its own look, its own palette)
```

1. **Adapter** (small): parameters (character, breath, expression, vibrato,
   intonation/dexterity/bowing/composure, patch arco/pizz/tremolo,
   instrument, double stops), presets (concert, student, virtuoso,
   session), note events → `Engine::add_note`, audio ← `Engine::process`.
2. **Look-ahead**, the only new thing Lyra needs: a CLAP-style *extension*,
   "here are the next second of notes". CLAP allows custom extensions, so
   it stays CLAP-shaped; in a host without it, Octavia reports one second of
   latency (a standard CLAP feature) and works anyway.
3. **Telemetry** for the faceplate: the latest decision per note (string,
   bow direction, how it's played), hand position, workload, composure,
   which strings are ringing, all as atomics, like Chrysalis.
4. **The faceplate**: drawn from Rhodexa's design (palette, four-shade LCD),
   reading only parameters and telemetry.

## 4. Not losing track

This is how projects die, so the plan is built around it:

* **Each piece has one narrow contract**, written down: the engine's public
  API (`Engine`, `Style`, `Decision`), the parameter table, the telemetry
  struct. The UI never needs to know how bowing works; the engine never
  needs to know how anything is drawn. A UI session can build the
  faceplate knowing only the contract, without touching DSP, and the
  reverse.
* **The engine keeps its test bench** (tests, `refs.sh`, renders) in this
  repo, so it can be verified without Lyra.
* **One piece at a time**, each finished and committed before the next:
  adapter first (Octavia playable with Lyra's generic editor, which already
  builds an editor from any parameter table), then look-ahead, then
  telemetry, then the faceplate.

## 5. A plugin kit, later

The direction Lyra is already moving (`plugin_look.rs`: the synths' look
frozen and separate from Lyra's) leads naturally to moving each plugin's
editor out of `lyra-app` into its plugin's own crate, using a small shared
UI-kit crate (egui plus Lyra's knob/LCD/LED painters and the parameter+undo
context). Then Chrysalis, Fluttersine and Octavia could each live in their
own folder, with assets, docs and tests, and Lyra would only list them. Not
needed to start Octavia: worth doing when a second plugin wants it.
