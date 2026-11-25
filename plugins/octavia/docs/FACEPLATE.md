# Octavia's faceplate: the design brief

What the faceplate must show and control, derived from what the engine
actually has (2026-11-25). A brief for designing it (Rhodexa) and for
building it (a UI session). It isn't the design: layout, proportions and
drawing are the designer's. Earlier notes and the cheap mock-ups:
`docs/UI.md`, `docs/design/`. Palette: `docs/design/colors.css`.

## Identity

* Octavia is a character (coat, mane, bow tie, collar, violin) and the
  palette is hers: `--coat`/`--coat-outline` chassis, `--mane` controls,
  `--bow-tie` accent for "on" and what matters, `--collar` light text and
  LCD glass, violin wood for warmth (logo, small details).
* **A properly cheap LCD**: dot matrix for pictures, segment digits for
  numbers and short words, **four shades** (ghost / light / mid / dark),
  unlit dots and segments faintly visible. The engine's colours (in plots)
  become shade + pattern on the LCD (see "Decisions" below).
* No keyboard, no transport (Lyra has the piano roll and transport). No new
  windows: it's the instrument page inside Lyra.

## Controls (all are plugin parameters: saved, undoable, automatable)

Grouped the way the engine thinks about them. Names are suggestions.

### The player: musicianship (ability)

| control | range | default | what it does |
|---|---|---|---|
| Player | concert / student / virtuoso / session | concert | sets the four below at once |
| Intonation | 0–1 | 0.90 | landing in tune, how fast a miss is pulled in |
| Dexterity | 0–1 | 0.85 | how hard fast notes, shifts, string skips feel; bounce speed limit |
| Bowing | 0–1 | 0.88 | clean starts (grit, failed starts), steady strokes |
| Composure | 0–1 | 0.80 | staying calm under load, recovering in rests |

### Character: decorations (style, separate from ability)

| control | range | default | what it does |
|---|---|---|---|
| Expression | 0–2 | 1 | 0 = deadpan machine, 1 = normal, 2 = a lot |
| Vibrato | 0–2 | 1 | depth scale (0 = none) |
| Breath | 0–3 | 1 | steady rosin noise under the tone |
| Performer | on / off | on | off = deadpan (expression 0, vibrato 0): hear the rules vs. none |

### Voice

| control | range | default | what it does |
|---|---|---|---|
| Character (bright) | 0–1 | 0 | natural cello … bright and powerful ("boss fight") |
| Instrument | cello / violin (viola, bass later) | cello | strings, body, hand size |
| Patch | arco / pizz / tremolo | arco | how every note is played unless the note says otherwise |
| Double stops | on / off | on | play two-note chords on two strings when a hand can |
| Take | number | 1 | another performance of the same notes (the seed) |

### Not controls (on purpose)

Bow pressure, speed, position: the performer's job, decided per note.
Overriding them belongs to per-note hints and automation in Lyra later
("I choose / you choose", the conductor's hints). Rubato, ornaments,
hooked bowing: not in the engine yet.

## The screen (LCD): the performer thinking

Everything here comes from telemetry (`docs/KIT-REQUIREMENTS.md`, C).

### Page 1: Performer (the main view)

* **Note lane**: recent and current notes scrolling by, each marked by how
  it's being played.
* **Four strings**: which is bowed (dark), which are still ringing (light,
  fading through the shades as they die), open strings marked.
* **The hand**: a marker on the neck (position), sliding when it shifts.
* **The bow**: direction (↓ / ↑), and loudness / brightness as small moving
  traces; grit as a rough edge on the trace.
* **Workload and composure**: two small meters (segmented), the player's
  state.
* **Pitch vs. target**: a tuner-like needle or dot (intonation and vibrato).

### Page 2: Event log

The decisions as text, like `--explain`:
`D3  ↑Slurred  II  hand 3  load 45  calm 84`.

### Decisions as shade + pattern (four shades, no colour)

| decision | suggestion |
|---|---|
| from silence | dark, solid, with a small "start" tick |
| new bow stroke | dark, solid |
| slurred | mid, hollow (connected to the previous note) |
| staccato | dark, short, with a gap |
| bounced (spiccato) | dark dot that hops |
| tremolo | dark, zig-zag edge |
| plucked | single dot, light tail |
| double stop | two stacked |

Shade carries weight (dark = the bow is working, mid = flowing, light =
ringing on), pattern carries the kind.

## States

* **Idle**: last frame stays, dimmed; the event log keeps the last notes.
* **Playing**: everything moves, ~30–60 updates a second.
* **Deadpan** (performer off): the player meters read 0, a clear
  "PERFORMER OFF".

## Open questions for the designer

* One page with everything, or Performer / Event log tabs?
* How much of the character appears on the plate (logo only, or her)?
* Knobs or sliders (the GPT mock-up used vertical faders for skills)?
