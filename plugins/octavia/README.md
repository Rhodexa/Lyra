# Octavia

An expressive string player for the CPU: it reads notes and articulations,
decides how a cellist would play them, and plays them on a modelled cello
(or violin). A sibling of [Lyra](../../Lyra). Design: [docs/DESIGN.md](docs/DESIGN.md).

## Try it

```sh
cargo run --release -- scores/lament.txt scores/prelude.txt scores/articulations.txt
cargo run --release -- scores/violin-air.txt --instrument violin
```

Everything lands in `renders/`:

| file | what |
|---|---|
| `NAME.wav` | the sound (with a small room; `--dry` for none) |
| `NAME.curves.csv` | the performance: edit it and render the `.csv` directly |
| `NAME.curves.png` | lanes, top to bottom: **pitch** (white, over the written notes; purple = slurred), **loudness** (orange), **brightness** (blue), **bow noise** (violet); green ticks = plucks; one grid line per second |
| `NAME.spec.png` | spectrogram, 40 Hz – 16 kHz, dotted lines at each C |

Useful A/B switches: `--expression 0` (deadpan, rules off), `--vibrato 0`,
`--seed 2` (another take).

## Score format

```text
tempo 66
mp
(G2:e D3 B3) A3 B3      # ( ) slur; durations w h q e s t, 3 = triplet, dots; sticky
^C4:q 'D4 -E4           # ^ accent, ' staccato, - tenuto
p < C3:w f              # hairpins run to the next dynamic, even inside one note
pizz G2:q r D3 arco
```

## License

MIT OR Apache-2.0, at your option.
