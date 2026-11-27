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
| `NAME.curves.png` | lanes, top to bottom: **pitch** (white; green = a double stop's second note) over the written notes, **loudness** (orange), **brightness** (blue), **bow noise** (violet); one grid line per second |
| `NAME.spec.png` | spectrogram, 40 Hz – 16 kHz, dotted lines at each C |

The written notes are coloured by how Octavia chose to play them:
**blue** from silence · **teal** new bow stroke · **purple** slurred ·
**red** staccato · **orange** bounced (spiccato) · **pink** tremolo · **green** plucked.
`--explain` prints the same thing as text, note by note, with the bow
direction (↓ down, ↑ up), string, hand position, workload and composure.

The player: `--player concert|student|virtuoso|session`, or one skill at a
time with `--intonation`, `--dexterity`, `--bowing`, `--composure` (0..1).

Useful A/B switches: `--expression 0` (deadpan, rules off), `--vibrato 0`,
`--breath 2` (more steady bow noise), `--bright 1` (bright, powerful character),
`--seed 2` (another take).

To compare with a real recording (kept in `ext_sample/`, never committed):

```sh
cargo run --release -- analyze ext_sample/cello.wav   # same numbers + spectrogram
```

## Piano rolls

A `.roll` is just what a piano roll holds (start, length, key, velocity), and
Octavia infers every articulation from it:

```sh
cargo run --release -- roll scores/prelude.txt          # strip a score's marks into prelude.roll
cargo run --release -- scores/prelude.roll -o renders/inferred
cargo run --release -- scores/prelude.roll --pizz       # the pizzicato patch
```

## MIDI files

```sh
cargo run --release -- midi-info song.mid               # which track is what
cargo run --release -- song.mid --track 2               # one line from track 2
cargo run --release -- song.mid --bottom --transpose -12
```

Chords collapse to their top note (`--bottom` for the lowest), keeping the
next note down as a **double stop** when it's held and playable
(`--no-doubles` to turn that off). Small overlaps become slurs, and the line
is shifted by octaves into the instrument's range. The extracted line is
written as `NAME.roll`, so you can see and edit what Octavia plays.

## Rendering all the MIDIs

`scripts/render-midis.sh` renders every file in `midi/` (only `.mid` lives
there) into `renders/midi/`, with each piece's known settings. Extra options
apply to all of them: `scripts/render-midis.sh --player student`.

## Reference pieces

`scripts/refs.sh` re-renders the pieces that already sound right (with their
exact settings) into `renders/refs/` and lists any note now played
differently. `scripts/refs.sh --accept` takes the current run as the new
baseline. Add a line to the script when a new piece earns its place.

## Score format

```text
tempo 66
mp
(G2:e D3 B3) A3 B3      # ( ) slur; durations w h q e s t, 3 = triplet, dots; sticky
^C4:q 'D4 -E4           # ^ accent, ' staccato, - tenuto
p < C3:w f              # hairpins run to the next dynamic, even inside one note
pizz G2:q r D3 arco
*C4:s *C4 *C4           # * spiccato (bounced); fast separate notes bounce anyway
~D3:w                   # ~ tremolo: the bow shakes (or --tremolo for every long note)
D3+A3:h                 # + a double stop (two strings at once)
```

## License

MIT OR Apache-2.0, at your option.
