# What Lyra's UI needs

For Rhodexa. Written 2026-11-24, with every feature paused until the UI is
consistent again. It lists what to standardise or build, and the design
questions only you can answer. Each has my suggestion. The inventory of
what exists is in [COMPONENTS.md](COMPONENTS.md).

## What to call them

In egui, the library Lyra is drawn with, anything you can place and click is
a **widget**, so these are Lyra's **widgets**. A widget is one control (a
button, a knob, a dropdown). A group of widgets with a job (the Recorder's
plate, a mixer strip, a panel header) is a **view**. A whole area (the piano
roll, the mixer) is an **editor**. These are the words used from here on.

## Sizes: one grid for everything

The knobs already come in sizes that each have their own look. Everything else
should follow the same three sizes, so a row of mixed widgets lines up:

| Size | Drawn height | Knob of that size | For |
|---|---|---|---|
| **S** | ~20 px | Tiny | dense rows, the octave + / − |
| **M** | ~28 px | Rubber | most things: plates, headers, dropdowns |
| **L** | ~40 px | Polymer | the toolbar, the transport |

- Every widget is asked for by size (`S`, `M`, `L`), never by a number typed
  in the code. The numbers live in one place (Metrics, tunable in F12).
- **Hitbox ≠ drawing.** A small widget is drawn small but reacts in a bigger
  area (at least M), so it's easy to hit. You were thinking the right way.
- Layout stays the flexbox we have (`flex::row` / `col` with fixed and
  growing parts). The rule to add: one gap size between widgets, one inside
  plates.

## Widgets to standardise or build

1. **Button** in S / M / L, icon or text. States: idle, hover, pressed,
   disabled, and **on = bright body, dark ink** (like the lit keys and
   Chrysalis), never coloured ink on grey.
2. **Mode switch**: one choice among a few modes (SONG / PAT, the
   arrangement's tools, EDITOR / RECORDER if it's a mode). It must not look
   like tabs or like loose buttons: one recessed track with the chosen part
   lit, maybe sliding later.
3. **Tabs**, redesigned. They're hard because a tab belongs to the panel under
   it. Suggestion: the chosen tab is the *same surface* as its panel (no
   line between them), the others sit a step back and darker.
4. **Dropdown**: the editor picker, made general (any list, optional icons),
   in S / M / L. It replaces all 12 plain dropdowns and the mixer's number well.
5. **Number picker**: the BPM LCD's behaviour (hover a digit to choose how fine,
   drag or wheel, double-click to type) as a widget of its own, with an LCD
   look and a plain look, in S / M / L. For tempo, swing, lengths,
   inserts, anything numeric.
6. **Scrollbars**, two kinds sharing one look:
   - **viewport bar**: the overview bars we have (drag the ends to zoom, content
     drawn inside), for the editors;
   - **plain bar**: just scrolls, for the mixer, the browser, racks and lists.
     It replaces egui's scrollbars, which is what makes the mixer feel bad.
7. **Field row**: a label and its widget, with one rule for where the label
   goes (above, small caps, like the recorder's INPUT), so labels never float.
8. **Header row**: what every editor and plate shows at its top: the name on
   the left, its actions on the right.
9. **Plate**: a titled area of widgets (the audio-clip editor's "THIS CLIP"),
   made general.
10. **Meter**: the calm, eased one from the Recorder as a widget, beside the
    gradient meter.
11. **Text field**: our own look for names and search (one height: M).
12. **Checkbox / switch**: on/off for a setting (the piano roll's two
    checkboxes). The LED may already be this.
13. **Waveform**: one drawing for clips and the tape (the tape lost its middle
    line because it had its own).
14. **Menus and tooltips**: egui's, restyled to match (colours, corners,
    padding) rather than rebuilt.

## Questions for you

1. **Rounded corners on tiles** (clips, steps, rows)? My suggestion: a 2 px
   radius, enough to tell neighbours apart without looking soft.
2. **Tabs with rounded bottom corners where they meet the panel?** That's the
   classic "folder tab" shape and reads clearly as a tab. My suggestion: yes,
   rounded on the *top* of the chosen tab, flowing into the panel at the bottom.
3. **Is EDITOR / RECORDER a tab or a mode?** It switches what the plate shows,
   which is a mode. Tabs switch between pages. Your call.
4. **S / M / L numbers**: start at 20 / 28 / 40 and tune in F12?
5. **Studies**: do you want to draw the mode switch, tabs, button sizes and
   dropdown as HTML studies (like the knobs), or shall I draft one in your
   control-library style for you to edit?

## How we work on it

1. You settle the questions above (and studies, if you make them).
2. Each widget is built once, shown in F12 at every size and state, and named
   in COMPONENTS.md.
3. Every place in the app switches to it, one widget at a time, each with a
   before/after screenshot.
4. New UI from then on only uses catalog widgets. Anything missing gets added
   to the catalog first.
