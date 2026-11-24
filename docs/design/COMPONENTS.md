# Lyra's components: what exists today

An inventory, taken from the code on 2026-11-24, before the cleanup. Every
piece of Lyra's own UI that draws something and reacts to the mouse is listed
here, with where its code lives, where it's used, and how it's doing.

**Status:** ✓ good, keep it · ~ works, needs the standard sizes or states ·
✗ one-off or ugly, to be replaced.

Plugins (Fluttersine, Chrysalis) are not in this list on purpose: they keep
their own look.


> **Hands off the plugins.** Fluttersine (`fm_editor.rs`) and Chrysalis
> (`chrysalis_editor.rs`, `chrysalis_table.rs`) are independent projects with
> their own visual language. The cleanup must not change them.
> **Done 2026-11-25:** everything visual they borrowed is frozen in
> `plugin_look.rs` (colours, lights, LED, LCD, well, the knob painter and its
> behaviour, icons, their logos, today's sizes, today's egui style, and their
> own font families), and they use only that, plus layout, undo and data
> helpers that draw nothing. Verified: before/after screenshots identical.
> Only their hover hints still follow Lyra's style.

---

## Controls (you press, turn or drag them)

| Component | What it is | Code | Used in | Status |
|---|---|---|---|---|
| **Knob** | Turn by dragging up/down, wheel, double-click resets. Four looks by size: *Tiny* (flat disc, < 34 px), *Rubber* (ringed cap, < 62 px), *Polymer* (lit body, larger), *Pan* (bipolar, a well with an arc) | `widgets::knob`, `knob_at`, `paint_knob` | everywhere | ✓ the model for everything else |
| **LED** | A round light that toggles (mute, on/off) | `widgets::led`, `theme::led` | rack, mixer, arrangement headers | ✓ |
| **Sweep** | Drag across a row of LEDs to set them all | `widgets::sweep` | track and channel mutes | ✓ behaviour, keep |
| **Flat button** (new) | A plate in the knob greys; on = coloured ink | `widgets::button`, `button_sized`, `button_at` | toolbar, arrangement tools, recorder, Reverse, + Link | ~ too small, one size only, "on" is hard to read |
| **Choice** (new) | Flat buttons side by side, one lit | `widgets::choice` | SONG / PAT, arrangement tools | ✗ looks like loose buttons; needs a real mode switch |
| **Wide button** | A flat button across a panel ("+ New pattern") | `widgets::wide_key` | browser, Patterns tab, plugin rack | ~ |
| **LCD number** | A backlit number; hover a digit to pick how fine, drag or wheel, double-click to type | `widgets::lcd_number` | BPM | ✓ behaviour, the model for a number picker |
| **LCD text** | A backlit readout | `widgets::lcd_text` | playhead position | ✓ |
| **Number well** | A small number in a well, drag to change | `widgets::number_well` | mixer insert per channel | ✗ ugly (your words) |
| **Name field** | Text field with one undo step per edit | `widgets::name_field` | clip, pattern, automation names | ~ egui's look |
| **Editor picker** | The dropdown at an area's top left: a well, icon, name, chevron; a popup of icon + name rows | `areas::picker_list` | every area | ✓ **the model dropdown**, but it only knows editors |
| **Plain dropdown** | egui's `ComboBox` | — | snap (arrangement, piano roll), chord stamp, instrument editor's mixer menu, automation editor, plugin rack, recorder | ✗ 12 places, all to become the model dropdown |
| **Plain button** | egui's `ui.button` | — | File menu, mixer, user presets, browser, plugin rack, design panel | ✗ 23 places |
| **Checkbox** | egui's | — | piano roll (2) | ✗ |
| **Text field** | egui's `TextEdit` | — | browser search, recorder name, user presets | ~ |
| **Keyboard map** | The tiny keyboard showing where the typing keys play | `widgets::keyboard_map` | toolbar | ✓ |
| **Rack name key** | A channel's or audio clip's name plate that flashes when it plays | `rack::name_key` | plugin rack, Audio tab | ✓ |
| **Editor tabs** | The tab row (now its own function) | `widgets::tab_row` | audio-clip editor | ✗ looks like an old X app |
| **Panel header** | A side panel's tabs plus the fold chevron | `widgets::panel_header` | left and right panels | ~ same tab look |
| **Fold strip** | A folded panel's thin strip with an arrow | `app::folded_strip`, `folded_bottom` | left, right, bottom | ✓ |

## Views of time (the editors' machinery)

| Component | What it is | Code | Used in | Status |
|---|---|---|---|---|
| **Overview bar** | Horizontal scrollbar that is also the zoom: drag its ends to zoom; draws the content inside it | `timeline::Overview` | arrangement, piano roll | ✓ the *viewport* scrollbar |
| **Vertical bar** | Its vertical twin (rows) | `timeline::VBar` | arrangement, piano roll | ✓ viewport scrollbar |
| **Plain scrollbar** | egui's `ScrollArea` | — | mixer, browser, plugin rack, presets, F12 | ✗ the mixer's scrolling feels bad; no Lyra scrollbar exists |
| **Bar ruler** | Bar numbers, the content's end, the playhead | `timeline::paint_ruler` | arrangement, piano roll | ✓ |
| **Seconds ruler** | The same in seconds | `audio_editor::paint_seconds` | audio-clip editor | ~ private to one editor |
| **Playhead, cursor line, loop, rubber band** | | `timeline::paint_playhead`, `paint_cursor`, `paint_loop`, `paint_rubber_band` | editors | ✓ |
| **Wheel scheme, snapping, edge-pan, double-click** | Behaviour shared by the editors | `timeline::wheel`, `snap`, `drag_to_grid`, `edge_pan_2d`, `Clicks` | editors | ✓ |
| **Waveform** | Peaks, fades, handles, the gain grip | `waveform::*` | arrangement clips, audio-clip editor | ✓ |
| **Tape waveform** | A second waveform drawing | `recorder::paint_tape` | Recorder | ✗ duplicate (no middle line, no handles); should be `waveform` |

## Readouts

| Component | Code | Used in | Status |
|---|---|---|---|
| **Meter bar** (fixed gradient) | `widgets::meter_bar`, `meter_stereo_h` | mixer, toolbar master | ✓ |
| **Calm meter** (eased, one colour) | inside `recorder::plate` | Recorder | ~ you like it; should become a component |
| **Toast** (status message) | `app::notify` | status bar | ✓ |
| **Context bar** | `app::status_bar` + editors' `context()` | status bar | ~ uses plain buttons |

## Surfaces (what things sit on)

| Component | Code | Notes |
|---|---|---|
| **Module** (toolbar section) | `widgets::module` | ✓ |
| **Plate** (a titled area of knobs) | `audio_editor::knob_plate` | ~ private to one editor |
| **Well** (recessed area) | `theme::well` | ✓ |
| **Silicone key, flat key** | `theme::key`, `flat_key` | the step grid; the silicone key is Fluttersine's now |
| **LCD panel** | `theme::lcd_panel` | ✓ |
| **Menus, tooltips, modal** | egui (File menu, hover hints, "Save changes?") | ~ egui's look |
| **Floating windows** | egui `Window` (instrument editor, effect windows) | no new ones, ever |

## Layout and sizes

| Piece | Code | Notes |
|---|---|---|
| **Flex** | `flex::row`, `col`, `even_row`, `even_col`, `center` with `Px` and `Grow` | ✓ works like a tiny flexbox; most older code doesn't use it yet |
| **Metrics** | `theme::Metrics` (text, control_h, tool_key, knob sizes, step sizes, ruler, rows…), live in F12 | ✓ but many sizes are still typed by hand in the code |
| **Light colours** | `theme::L_MINT` … eight lights, each with hi / mid / low / edge / rim | ✓ |

## The control library (F12)

F12 opens the Design panel: every Metrics size on a slider, and a swatch of the
silicone keys, steps, LEDs and LCD drawn by the real code. It's the start of a
live catalog, but it shows only a few components.
