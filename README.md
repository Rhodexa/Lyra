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
| Play / stop | `Space` |
| Project | click the logo, or `Ctrl+N` new · `Ctrl+O` open · `Ctrl+S` save · `Ctrl+Shift+S` save as |
| Windows | `F5` playlist · `F6` channel rack · `F7` piano roll · `F8` instrument editor (also the WINDOWS module in the top bar) |
| Pattern / song | `L`, or PAT / SONG in the top bar |
| Design panel | `F12`: every size on a live slider; see [docs/design](docs/design/README.md) |
| Patterns | `[` / `]` or the picker in the top bar (new, clone) |
| Play notes | typing keyboard: `Z`–`M` and `Q`–`P` rows; `-` / `=` octave (shown in the top bar) |
| Undo / redo | `Ctrl+Z` / `Ctrl+Shift+Z` |
| Knobs | drag (Shift = fine), wheel, double-click to reset |
| UI size | `F12` Design panel, or `Ctrl+=` / `Ctrl+-` / `Ctrl+0` zoom (`LYRA_ZOOM=1.3` to start bigger) |

**Playlist** (`F5`; `L` or PAT/SONG switches what Space plays)

| Do | How |
|---|---|
| Place the current pattern | click an empty spot (the top bar's pattern is the one placed) |
| Move / resize / duplicate | drag a clip / drag its right end (longer = the pattern repeats) / Shift+drag |
| Pick a clip's pattern / open it | click a clip / double-click it |
| Delete | right-click or right-drag, or `Delete` |
| Select | Ctrl+drag, `Ctrl+A` · duplicate after: `Ctrl+D` / `Ctrl+B` |
| Play from here | click the ruler (switches to song mode) |
| Mute a track | click its light |
| Scroll / zoom / pan | wheel, Shift+wheel · Ctrl+wheel (time), Alt+wheel (track height) · middle-drag |

**Mixer** (`F9`)

| Do | How |
|---|---|
| Pick a track | click its strip; its effects appear on the right |
| Volume / pan / mute | fader (drag, wheel, double-click = reset) · knob · light |
| Effects | "+ add effect"; light = bypass; ^ v reorder; x remove |
| Route a channel | instrument editor (F8) → "mixer" menu |

**Channel rack**

| Do | How |
|---|---|
| Paint steps | click & drag across the rack. Right-drag erases |
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
| Scroll | wheel, Shift+wheel sideways, middle-drag, or drag the overview bar's thumb |
| Zoom | Ctrl+wheel (time) · Alt+wheel (pitch) · drag the overview thumb's ends |
| Velocity | wheel over a note, or drag in the lane at the bottom |
| Jump to another channel | double right-click one of its ghost notes |
| Move the playhead | click / drag in the ruler |
| Key, scale, shading, ghosts | the "View" menu · "full screen" button |

## Developer switches

`LYRA_BUFFER=512` (audio ring size) · `LYRA_SILENT=1` · `LYRA_PLAY=1` ·
`LYRA_OPEN=piano,editor,design` · `LYRA_ZOOM=1.3` · `LYRA_SCREENSHOT=out.png` (+ `LYRA_SCREENSHOT_FRAMES=n`) ·
`LYRA_SCRIPT="wait 30; move 71 134; down; move 71 110; up"` (synthetic input for UI tests). See `crates/lyra-app/src/devtools.rs`.

## Read more

- [docs/manual/index.html](docs/manual/index.html): the illustrated manual, all diagrams (open in a browser)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how it fits together (terse version)
- [docs/ROADMAP.md](docs/ROADMAP.md): what's next
- [JOURNAL.md](JOURNAL.md): wishes and friction from real use (fill it in!)

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

Icons: [Phosphor](https://phosphoricons.com) (MIT), via the `egui-phosphor` crate.
Contributions are accepted under the same terms.
