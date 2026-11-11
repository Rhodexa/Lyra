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
| Windows | `F6` channel rack · `F7` piano roll · `F8` instrument editor |
| Patterns | `[` / `]` or the picker in the top bar (new, clone) |
| Play notes | typing keyboard: `Z`–`M` and `Q`–`P` rows; `-` / `=` octave |
| Undo / redo | `Ctrl+Z` / `Ctrl+Shift+Z` |
| Knobs | drag (Shift = fine), wheel, double-click to reset |

**Channel rack**

| Do | How |
|---|---|
| Paint steps | click & drag across the rack. Right-drag erases |
| Velocity / pitch of a step | mouse wheel / Shift + wheel |
| Select / edit a channel | click its name / double-click it |
| Piano roll, fill every 2/4/8, delete | right-click a channel name |
| Mute / solo | click / right-click the channel's light |
| Open a melody | click its mini piano roll |

**Piano roll**

| Do | How |
|---|---|
| Place a note (uses your last length) | click empty grid, keep holding to drag it |
| Move / resize / duplicate | drag a note / drag its right edge / Shift+drag |
| Erase | right-click or right-drag |
| Select | Ctrl+drag (Shift adds), `Ctrl+A` |
| Stamp chords that fit the key | "stamp" menu, e.g. *Triad (in key)* |
| Transpose / nudge | `↑` `↓` (Shift = octave) / `←` `→` |
| Duplicate after / quantize / delete | `Ctrl+D` / `Ctrl+Q` / `Delete` |
| No snap | hold `Alt` |
| Scroll / zoom | wheel, Shift+wheel / Ctrl+wheel |
| Velocity | drag in the lane at the bottom |
| Pattern length | drag the gold marker in the ruler |

## Developer switches

`LYRA_BUFFER=512` (audio ring size) · `LYRA_SILENT=1` · `LYRA_PLAY=1` ·
`LYRA_OPEN=piano,editor` · `LYRA_SCREENSHOT=out.png` ·
`LYRA_SCRIPT="wait 30; move 71 134; down; move 71 110; up"` (synthetic input for UI tests). See `crates/lyra-app/src/devtools.rs`.

## Read more

- [docs/manual/index.html](docs/manual/index.html): the illustrated manual, all diagrams (open in a browser)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how it fits together (terse version)
- [docs/ROADMAP.md](docs/ROADMAP.md): what's next

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Contributions are accepted under the same terms.
