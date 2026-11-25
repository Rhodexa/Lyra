# Octavia's faceplate: the approved design, and how to port it

**Status (2026-11-25): designed and approved by Rhodexa.** The spec is
[`docs/design/faceplate.html`](design/faceplate.html): open it in a browser,
press Play. Its LCD plays real renders ("tapes", `scripts/tape.py`), so what
it shows is what the engine does. This file says what's on the plate, why,
and how to build it with egui. When this file and the HTML disagree, the HTML
wins.

Palette: [`docs/design/colors.css`](design/colors.css), the only source of
colour. Art: `docs/design/logo.palette.png` (the logo, on the palette),
`bow.palette.png` (the bow alone, for later: a gift-bow "collar" on a corner
is one idea Rhodexa is exploring). Originals: `logo.png`, `bow.png`;
`scripts/recolor.py` puts new art on the palette.

## The vibe

A synth **made to be cheap**: blocky and flat, square corners, hard cartoon
shadows (an offset copy, no blur), thin dark outlines, few decorations.
Slightly off, never badly made, never polished. Octavia's own look: nothing
from Lyra's widgets or theme (Lyra's HTML catalog was only a model of the
*process*: HTML first, tokens, tuner, approve, port).

The LCD **cheats**: plain text with LCD ink shadows. The colours and the
shadow sell "LCD", not segment shapes (segment letters were tried and were
beautiful and hard to read). Segments survive only for the take number.
Unlit things are off or barely there (0.15), never half-lit.

## The plate (about 780×600 at scale 1)

```text
 ┌──────────────────────────────────────────────────────────┐
 │ [logo]                                  [CELLO|VIOLIN]   │
 │ string performer · model 01               instrument     │
 │ ┌─bezel──────────────────────┐  STYLE · decoration ───── │
 │ │ LCD, 640×336 units          │  (expr) (vib) (breath)    │
 │ │ shown at 0.68               │  [      PERFORMER      ]  │
 │ │                             │  SCREEN ──────────────── │
 │ └─────────────────────────────┘  [PERF|LOG]  take [−][+] │
 │ PLAYER · ability ──────────────  VOICE ───────────────── │
 │ [CONCERT ]  ┃   ┃   ┃   ┃        (character) [DOUBLE]    │
 │ [STUDENT ]  ┃   ┃   ┃   ┃         natural bright [STOPS]  │
 │ [VIRTUOSO]  ┃   ┃   ┃   ┃        [ARCO | PIZZ | TREM]     │
 │ [SESSION ]  intonation dexterity bowing composure        │
 │ octavia instruments            bowed by a player, not a robot │
 └──────────────────────────────────────────────────────────┘
```

### Settled values (the tuner's defaults in the HTML)

| token | value | notes |
|---|---|---|
| plate | `--chassis` fill, 2 px `--mane-outline` outline, corners 0, hard shadow 6 px `--mane-outline` | |
| purfling | inset 7 px: 3 px `--violin-wood` line, 2 px gap, 1.8 px `--violin-body` line | a violin's inlaid edge |
| outline | 2 px everywhere, always the darker tone (`--mane-outline`) | |
| buttons | `--control` body, `--text-inverse` caps text; **on** = `--accent` body, `--mane-outline` text; corners 0; lone buttons cast a 2 px hard shadow and drop into it when pressed | mode switches sit in a `--chassis-inset` track |
| knobs | 58 px, round. A wide dark groove (`--mane-outline`, 0.13×size) with the value arc inside it (`--accent`, 0.07×size); body r 0.31×size `--control`; pink pointer | contrast on the plate was the issue |
| faders | 128 px travel, 8 px `--control-track` slot, `--accent` fill below the cap, square cap with a 2 px hard shadow and a pink line | |
| gaps | 6 px between controls | |
| labels | IBM Plex Mono 700, 9–10 px, caps, tracked | |
| section heads | caps label + a 2 px rule to the right | |
| LCD | drawn in 640×336 units, shown at **0.68**; text never under 11.5 units; font **Roboto Condensed** 700 (Lyra already bundles it) | |
| LCD ink | 3 = `--lcd-3`, 2 = `--lcd-2`, 1 = `--lcd-1`, unlit = mix(`--lcd-0`, `--lcd-1`, **0.15**) | |
| LCD shadow | every piece of ink drawn twice: first at +2 units in mix(`--lcd-0`, `--lcd-2`, 0.275), then itself | |
| paused | dark ink drawn mid (the last frame stays, dimmed) | |

### Behaviour

* Knobs and faders: vertical drag, Shift = fine (×0.1), wheel nudges (5 %,
  Shift 1 %), double-click resets. One undo step per drag (begin/end
  gesture).
* Touching a control (hover or drag) shows it on the LCD for 1.6 s: four
  letters and the value, `VIBR 1.00`. Otherwise the LCD shows the player
  (`CONCERT`, or `CUSTOM` once a skill no longer matches a profile) or
  `PERF OFF`.
* Choosing a profile moves the four skill faders.

## The controls (all plugin parameters: saved, undoable, automatable)

### The player: musicianship (ability)

| control | range | default | what it does |
|---|---|---|---|
| Player | concert / student / virtuoso / session | concert | sets the four below at once (`Skill::profile`) |
| Intonation | 0–1 | 0.90 | landing in tune, how fast a miss is pulled in |
| Dexterity | 0–1 | 0.85 | how hard fast notes, shifts, string skips feel; bounce speed limit |
| Bowing | 0–1 | 0.88 | clean starts (grit, failed starts), steady strokes |
| Composure | 0–1 | 0.80 | staying calm under load, recovering in rests |

### Style: decorations (separate from ability)

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
| Patch | arco / pizz / trem | arco | how every note is played unless the note says otherwise |
| Double stops | on / off | on | play two-note chords on two strings when a hand can |
| Take | 1–99 | 1 | another performance of the same notes (the seed) |

### Not controls (on purpose)

Bow pressure, speed, position: the performer's job, decided per note.
Overriding them belongs to per-note hints and automation in Lyra later.

## The screen (LCD): the performer thinking

Coordinates are in the LCD's own units (640×336); the functions named are in
the HTML and port one to one.

### Always (`drawLcd`)

* **Message** (top left, 22 units, tracked): the player, or the touched
  control and its value, or `PERF OFF`.
* **Take** (top right): two 7-segment digits, unlit segments off, printed
  "TAKE" beside.
* **Printed words** (a row, lit when true): CELLO VIOLIN · ARCO PIZZ TREM ·
  DOUBLE · GRIT (the bow scratching now) · PERF LOG · ▶ PLAY. A printed rule
  under them.

### Page PERF (`perfPage`)

* **The note** (`nowPanel`): the note(s) being played, big (`A3+D3`), and
  how, in words (`SLURRED`, `FROM SILENCE`, …). No label, no bow words.
* **NEXT** (`nextPanel`): the next four planned notes (the look-ahead), each
  with its bow mark (⊓ down-bow, V up-bow, drawn as shapes so any font
  works), how in short (`SLUR`, `STAC`, `BOUNCE`…) and its string. Beside
  it, the bow's way **now**, big and vertical: ⊓↓ or V↑ (the bow is drawn
  vertical on the neck, so the arrows run along it).
* **Meters** (`meters`, BIOS style: outlined box, 32 blocks taller than
  wide, unlit blocks off): LOUDNESS, BRIGHTNESS, PITCH, WORKLOAD, CALM.
  PITCH is a tuner over ±50 cents in 41 blocks: the dark block is where the
  note sits (averaged over 0.15 s, the onset glide ignored), a mid span
  shows how far the vibrato swings, and the cents as a number. Under
  `PERFORMER OFF`, workload and calm read 0.
* **The neck** (`neckPanel`), the whole width along the bottom, seen from
  the side: nut left, highest string on top, two octaves of semitone marks
  (doubled at 8va, 15ma), bridge right.
  * Strings: bowed = dark and thick, ringing on = mid, idle = faint.
  * **Fingers** (12×12): **solid** while their string is played, **hollow**
    while only held (a note ringing into a rest until the finger lifts; a
    string crossed away from, still ringing at its stopped pitch). At the
    engine's real pitch, frame by frame: intonation, vibrato and slides
    move them. Open strings: a tick at the nut.
  * **Slides**: a dotted trail behind the finger, fading over 0.5 s, only
    where the engine slides (a legato note that shifts the hand on the same
    string).
  * **The hand**: a bracket above the strings covering `hand_span`
    semitones (cello 4, violin 6), gliding on shifts.
  * **The bow**: a vertical bar across the strings by the bridge while
    bowing; a small pluck mark for pizzicato.

### Page LOG (`logPage`)

The decisions as a table, newest at the bottom and darkest: TIME, NOTE, BOW
(⊓/V), HOW, STRING (`II open`), HAND, LOAD, CALM, SHIFT. Ten rows.

## Porting to egui

* **Everything is rect, convex polygon, line, text.** `rect_filled`,
  `convex_polygon`, galleys. No gradients, no blur, no images except the
  logo (a texture, loaded from Octavia's own folder).
* **The LCD** is one `Painter` region: transform its 640×336 units by the
  screen scale × Lyra's UI scale. Ink with a shadow = paint the same shape
  twice (offset, shadow colour; then ink). The 7-segment digits are six-point
  polygons (see `digits`).
* **Repaint** only while playing or while a message is showing; idle draws
  the last frame dimmed.
* **What the LCD needs from the engine** is listed in
  [`KIT-REQUIREMENTS.md`](KIT-REQUIREMENTS.md), section C. The mock derived
  some of it from events (lift moments, held fingers); the real telemetry
  should hand it over directly.
* **Fonts:** Roboto Condensed (Lyra has it) for the LCD; IBM Plex Mono (OFL,
  to bundle) for the plate's labels.
* **Check against the HTML** side by side, with the same tape moment: the
  page takes `?tape=showcase&t=11.15&live` to freeze a moment.
