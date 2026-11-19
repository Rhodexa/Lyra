# Lyra

An open-source, pattern-based DAW that makes you want to play.
Written in Rust: real-time-safe audio engine, egui UI, built-in synths, and CLAP plugins later.

## Run

```bash
cargo run --release
```

On Fedora you need `alsa-lib-devel`, `clang` and `mesa-libGL-devel` installed.

## Play

| Do | How |
|---|---|
| Play / stop | `Space` · Stop while stopped goes back to the start |
| Project | click the logo, or `Ctrl+N` new · `Ctrl+O` open · `Ctrl+S` save · `Ctrl+Shift+S` save as |
| Layout | Browser on the left (`F4`), plugin rack on the right (`F6`); both fold to a strip. The centre has two **areas**: pick each one's editor (Arrangement, Piano roll, Audio clip, Mixer, Step sequencer) from the menu in its top-left corner; each area remembers its own scroll and zoom. `F5` / `F7` / `F9` open the bottom area on the arrangement / piano roll / mixer; the same key again folds it. Editing keys go to the area under the mouse. Floating: `F8` instrument editor, effect windows. |
| Pattern / song | `L`, or PAT / SONG in the top bar |
| Design panel | `F12`: every size on a live slider; see [docs/design](docs/design/README.md) |
| Patterns | `[` / `]` or the picker in the top bar (new, clone) |
| Play notes | typing keyboard: `Z`–`M` and `Q`–`P` rows; `-` / `=` octave (shown in the top bar) |
| Undo / redo | `Ctrl+Z` / `Ctrl+Shift+Z` |
| Knobs | drag (Shift = fine), wheel, double-click to reset |
| UI size | `F12` Design panel, or `Ctrl+=` / `Ctrl+-` / `Ctrl+0` zoom (`LYRA_ZOOM=1.3` to start bigger) |

**Arranger / playlist** (`F5`; `L` or SONG/PAT switches what Space plays)

| Do | How |
|---|---|
| Place a clip | click an empty spot: a copy of the clip you last clicked (its length, window, fades, gain), or the top bar's pattern if you picked one there |
| Move / resize / duplicate | drag a clip / drag its right end (longer = the pattern repeats) / Shift+drag |
| Pick a clip's pattern / open it | click a clip / double-click it |
| Delete | right-click or right-drag, or `Delete` |
| Select | Ctrl+drag, `Ctrl+A` · duplicate after: `Ctrl+D` / `Ctrl+B` |
| Audio clip | drop a sample from the browser on a track (or an audio clip from the browser for another copy) |
| Trim a clip | drag either end: a clip is a window onto its audio |
| A clip's own fades and gain | hover it: the dots on the title strip are the fades (drag sideways), the ring on a curve bends it (up/down), the ↕ in the middle is gain (up/down, Shift = fine, double-click = 0 dB). Each clip has its own |
| The *Audio clip* editor | double-click a clip. Its frame is the clip's window: drag it sideways to play another part of the audio (the clip stays put). On the frame: the clip's fade dots, curve rings and ↕ gain, like on the arrangement. The white trim lines and the *every copy* gain are shared by all copies; `Ctrl+U` gives the selected clips their own source |
| Where a track's audio plays | the number at the right of its header: mixer insert (M = master) |
| Play from here | click the ruler (switches to song mode) |
| Mute a track | click its light |
| Scroll / zoom / pan | wheel (up/down), Ctrl+wheel (sideways) · Shift+wheel or wheel over the ruler (time, around the mouse), wheel over a scrollbar (that axis), Alt+wheel (track height) · middle-drag |

**Mixer** (`F9`)

| Do | How |
|---|---|
| Pick a track | click its strip; its effects appear on the right |
| Volume / pan / mute | fader (drag, wheel, double-click = reset) · knob · light |
| Effects | "+ add effect"; light = bypass; ^ v reorder; x remove |
| Route a channel | instrument editor (F8) → "mixer" menu |

**Samples** (browser → Samples)

| Do | How |
|---|---|
| Hear a sample | click it |
| Make a sampler for it | double-click it, or drag it onto the plugin rack |
| Swap a sample in | drag it onto an instrument's name in the plugin rack (any instrument becomes a sampler) |
| Your own samples | put WAV/FLAC files (folders are fine) in `~/.local/share/lyra/library` ("Your folder…" opens it), then Rescan |

Lyra ships a **Starter Kit** of 65 CC0 one-shots (see `assets/samples/Starter Kit/CREDITS.md`).

**Plugin rack** (right panel; the step grid lives in the *Step sequencer* editor)

| Do | How |
|---|---|
| Paint steps (Step sequencer) | click & drag across the steps. Right-drag erases |
| Velocity / pitch of a step | mouse wheel / Shift + wheel |
| Select / edit a channel | click its name / double-click it |
| Piano roll, clone, replace instrument, fill, delete | right-click a channel name |
| Mute | click a light, or drag across several |
| Solo | right-click a light (click dark ones to bring them back; right-click again for all) |
| Pattern length | grows to fit your notes; the bars menu sets the minimum |
| Open a melody | click its mini piano roll |

**Piano roll**

| Do | How |
|---|---|
| Place a note (uses your last length) | click empty grid, keep holding to drag it (silent while playing) |
| Move / resize / duplicate | drag a note / drag its right edge / Shift+drag |
| Erase | right-click or right-drag |
| Select | Ctrl+drag (Shift adds), `Ctrl+A` |
| Stamp chords that fit the key | "stamp" menu, e.g. *Triad (in key)* |
| Transpose | `↑` `↓` or `Shift+↑↓` (semitone) / `Ctrl+↑↓` (octave) |
| Nudge | `←` `→` (snap) / `Ctrl+←→` (beat) |
| Copy / cut / paste | `Ctrl+C` / `Ctrl+X` / `Ctrl+V` (pastes under the mouse, or at the playhead) |
| Duplicate after / quantize / delete | `Ctrl+D` or `Ctrl+B` / `Ctrl+Q` / `Delete` |
| No snap | hold `Alt` (stays free for the rest of the drag) |
| Scroll | wheel (up/down), Ctrl+wheel (sideways), middle-drag, or the scrollbars |
| Zoom | Shift+wheel or wheel over the ruler (time) · Alt+wheel (pitch) · wheel over a scrollbar · drag a scrollbar thumb's ends |
| Velocity | wheel over a note, or drag in the lane at the bottom |
| Jump to another channel | double right-click one of its ghost notes |
| Move the playhead | click / drag in the ruler |
| Key, scale, shading, ghosts | the "View" menu · "full screen" button |

## Developer switches

`LYRA_BUFFER=512` (audio ring size) · `LYRA_SILENT=1` · `LYRA_PLAY=1` ·
`LYRA_OPEN=piano,mixer,steps,audio,editor,design` · `LYRA_ZOOM=1.3` · `LYRA_SCREENSHOT=out.png` (+ `LYRA_SCREENSHOT_FRAMES=n`) ·
`LYRA_SCRIPT="wait 30; move 71 134; down; move 71 110; up"` (synthetic input for UI tests). See `crates/lyra-app/src/devtools.rs`.

## Read more

- [docs/manual/index.html](docs/manual/index.html): the illustrated manual, all diagrams (open in a browser)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how it fits together (terse version)
- [docs/ROADMAP.md](docs/ROADMAP.md): what's next
- [JOURNAL.md](JOURNAL.md): wishes and friction from real use (fill it in!)

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Contributions are accepted under the same terms.
