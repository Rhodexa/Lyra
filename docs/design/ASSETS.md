# Asset list

Everything Lyra could use hand-made art for, in priority order. Nothing here
blocks development: each slot has a drawn-in-code placeholder today, and your
art replaces it without touching any logic.

**General rules**
- **Vector first:** SVG on a 24×24 grid, with strokes about 1.5 px at 24 px size.
  Lyra will rasterise at whatever size the UI needs.
- **Icons are single-colour** (pure white on transparent). Lyra tints them per
  state (dim, hover, active mint), so one drawing covers all states.
- **Textures** are tileable PNGs, seamless on all four edges.
- File names in `kebab-case`, dropped in `/assets`. Tell me in `JOURNAL.md`
  when a batch lands.

## 1. Must-have soon

| Asset | What it is today | Spec |
|---|---|---|
| **UI font** | egui's built-in font (the biggest single cause of the "engineering tool" look) | Pick one or two open-licensed families (OFL). Ideas: *M PLUS Rounded 1c*, *Zen Maru Gothic*, *Nunito*, *Outfit*. One for UI text, optionally one for big numbers (tempo, position). |
| **Transport icons** | vector shapes in `widgets.rs` | play, stop, record (coming), loop (coming), metronome (coming) |
| **Window / view icons** | same | playlist, channel rack, piano roll, instrument editor, mixer (coming), browser (coming), design panel |
| **Edit icons** | same | plus (new), minus, clone/duplicate, previous, next, delete, snap/magnet, chord stamp, scale/key |

## 2. Make it feel like Lyra

| Asset | Spec |
|---|---|
| **Knob** | Either a 64×64 PNG *base* plus a separate *pointer* layer (Lyra rotates the pointer), or a 128-frame vertical strip (64×64 per frame). Two sizes are enough: small (rack) and large (editor). The value arc stays drawn in code, in the channel's colour. |
| **Step button** | 3 states: off (two shades for alternating beats), on (tinted by channel colour, so draw it light/neutral), playing (highlight). Nine-slice-friendly PNG, ~24×30 at 1×. |
| **Mute light** | on / off, ~16×16. Tinted by channel colour, so draw it neutral. |
| **Workspace texture** | Replaces the generated grain + seigaiha. 256×256 tileable PNG, very low contrast and dark. |
| **App logo / icon** | You already have `assets/icon.png`. Needed sizes for the desktop icon: 512, 256, 128, 64, 48, 32, 16 (PNG), or one SVG. |

## 3. Later

| Asset | Spec |
|---|---|
| Cursors | resize-note, pencil (place), eraser, hand (pan): 32×32 PNG with the hotspot marked |
| Instrument portraits | a small badge per built-in instrument (kick, snare, clap, hat, synth) for the browser, ~48×48 |
| Splash / empty states | "no notes yet", "drop a sample here" illustrations, any size |
| Theme variants | if we ever do a light "washi" theme |

## Wishes from the journal that touch art

- **Less labelling, more implicit design:** drop the "TRANSPORT" and "TEMPO"
  captions. The tempo becomes a big number you can play with, not a labelled box.
- **Numbers you tune by hovering a digit** (LCD-style, but not LED-imitation):
  a font with clear tabular digits matters here.
