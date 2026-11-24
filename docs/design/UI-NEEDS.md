# What Lyra's UI needs

For Rhodexa. Written 2026-11-24, with every feature paused until the UI is
consistent again. It lists what to standardise or build, and the design
questions only you can answer. Each has my suggestion. The inventory of
what exists is in [COMPONENTS.md](COMPONENTS.md).



> **Status 2026-11-25:** every widget and view on this list is designed and
> approved in [catalog-next.html](catalog-next.html); its "Settled" index at the
> top is the spec in one screen. Nothing is ported to the app yet (HTML first,
> the app last; decouple the plugins first).

> **Hands off the plugins.** Fluttersine (`fm_editor.rs`) and Chrysalis
> (`chrysalis_editor.rs`, `chrysalis_table.rs`) are independent projects with
> their own visual language. The cleanup and the widget catalog must not change
> them. **Before** any shared widget or theme value below is changed, first give
> the plugin editors their own copies of what they borrow, so they no longer
> depend on Lyra's widgets. Today they borrow:
>
> - **Fluttersine:** `widgets::paint_knob`, `knob_painted`, `paint_icon`, `Icon`;
>   `theme::led`, `lcd_panel`, `well`, `key_ink`, `mix`, `shade`, `m()` (Metrics),
>   the colours `TEXT`, `TEXT_DIM`, `PANEL`, `PANEL_HI`, `INSET`, `BG`, `SEAM`,
>   `LCD`, `LCD_INK`, `LCD_INK_DIM` and all eight `L_*` lights; `flex`, `rack::tracked`.
> - **Chrysalis:** `widgets::knob_painted` (behaviour only; it paints its own
>   knobs), `theme::mix`, `shade`, `hex`, `radial_circle`, `cr`, `stroke`;
>   `fm_editor::wheel_step`; `flex`, `rack::tracked`.
>
> (Listed 2026-11-24 from the `use` lines and calls; re-check before splitting.)

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
   *Settled in catalog-next (D3): a dark well closed, an LCD-coloured drawer open.
   DAW lists get crowded: drawers must never just grow downwards: they wrap
   into columns with category headings (big drawer, draft 1).*
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

## Also missing (found 2026-11-24, while building the catalog)

Foundations, decided once and used by every widget:

15. **Colour roles**, not colours: *ground* (BG), *surface* (PANEL), *raised*
    (buttons), *sunken* (wells), *ink*, *ink-dim*, *ink-on-light*, *edge*, and
    *the light* (a widget's colour). Today ~40 greys are typed inside widgets.
16. **One set of states** with one look each: idle, hover, pressed, **on**,
    **selected**, **disabled** (nothing has a disabled look yet), and *playing*
    (things that flash with the music). No focus rings or glows.
17. **Type scale**: three sizes (label caps ~9–10, small 11, body 14), one
    rule for caps labels, monospace only for numbers.
18. **Corner radii and gaps**: two or three of each, as tokens.
19. **Icons**: your set, at S / M / L sizes, one stroke weight, tinted with ink.

Widgets and views nobody has designed yet:

20. **List row**: the browser, files, presets, the Patterns tab: name, detail,
    hover, selected, dragging. Probably the most-used thing in Lyra after knobs.
21. **Splitter**: the edges you drag to resize panels (only egui's today).
22. **Slider**: a horizontal fader for settings and the F12 panel (egui's).
23. **Light picker**: choosing a channel's colour from the eight lights.
24. **Dialog**: "Save changes?" and friends, in place of egui's window.
25. **Busy / progress**: rendering, loading samples.
26. **Drop target**: how a place shows it will accept a dragged sample.
27. **Hint**: the tooltip look (14 says restyle egui's; this is its spec).

What I'd *drop* from Lyra's own set: the silicone key (now Fluttersine's),
the choice widget (the mode switch replaces it) and the number well (the
dropdown and number picker replace it).

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

Decided 2026-11-24: **HTML first, the app last.** We design every widget in
HTML until it's right, and that takes many rounds (the knobs took several,
across more than one AI model). No Rust changes until the catalog is stable.

1. Each widget is drafted in the catalog's "Next" page, at every size and
   state, live (drag, hover, click), on the foundations (colours, states, type,
   sizes) rather than its own numbers.
2. Rhodexa reacts; we iterate there until it's approved.
3. When the whole set is stable: first decouple Fluttersine and Chrysalis (see
   the top of this file), then port widget by widget, each with a before/after
   screenshot, and name it in COMPONENTS.md.
4. From then on, new UI only uses catalog widgets. Anything missing is
   designed in the catalog first.
