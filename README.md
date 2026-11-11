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
| Play / stop | `Space` (or `Esc` to stop) |
| Paint steps | click & drag across the rack. Right-drag erases |
| Velocity / pitch of a step | mouse wheel / Shift + wheel |
| Fill every 2/4/8 steps | right-click a channel name |
| Play notes | typing keyboard: `Z`–`M` and `Q`–`P` rows; `-` / `=` octave |
| Choose channel | click its name, or `↑` / `↓` |
| Mute / solo | click / right-click the channel's light |
| Undo / redo | `Ctrl+Z` / `Ctrl+Shift+Z` |
| Knobs | drag (Shift = fine), wheel, double-click to reset |

## Read more

- [docs/manual/index.html](docs/manual/index.html): the illustrated manual, all diagrams (open in a browser)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how it fits together (terse version)
- [docs/ROADMAP.md](docs/ROADMAP.md): what's next

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Contributions are accepted under the same terms.
