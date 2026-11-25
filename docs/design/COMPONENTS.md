# Lyra's components: the widget set

What Lyra draws with, after the port (2026-11-25). The look of each piece is
settled in [catalog-next.html](catalog-next.html); this file says where its
code lives and where it's used. Use these. Don't hand-draw a new one.

Plugins (Fluttersine, Chrysalis) are not in this list on purpose: they keep
their own look, frozen in `plugin_look.rs`. **Never change them from here.**

## Foundations (`theme.rs`)

| Piece | Code | Notes |
|---|---|---|
| **Fonts** | `theme::read`, `read_bold` (Roboto, for names and sentences), `label`, `label_bold` (Roboto Condensed, caps legends) | bundled |
| **Colour roles** | `GROUND`, `SURFACE`, `PLATE`, `SUNKEN`, `INK`, `INK_DIM`, `LEGEND`, `INK_FAINT`, `INK_ON_LIGHT`, `LCD_DIM` | widgets ask for a role, never a colour |
| **Outline** | `outline_of(c)`, `outlined(p, r, radius, body)` | every control: 2 px (`OUTLINE_W`) in its own colour, half as bright |
| **Sizes** | `SIZE_S` 28 · `SIZE_M` 40 · `SIZE_L` 52; `Metrics` (`theme::m()`, live in F12) | 28 is the floor for anything clicked |
| **Lights** | `L_MINT` … eight, each hi / mid / low / edge / rim | "on" is backlit in a light |
| **Well, LCD panel** | `theme::well`, `lcd_panel` | |

## Controls (`widgets.rs`)

| Component | Code | Notes |
|---|---|---|
| **Button** | `button`, `button_sized`, `button_at` | a flat plate, caps legend; lit = backlit. egui's own buttons are styled to match |
| **Button, big reach** | `button_reach` | drawn small, reacts across a taller row (the toolbar's octave keys) |
| **Bare button** | `bare_button`, `bare_button_tall` | no plate until hovered: FILE, VIEW, a dialog's "Don't save" |
| **Wide key** | `wide_key` | a button across a panel ("+ Instrument") |
| **Mode switch** | `choice` | one choice among a few, a lit plate that slides; two options: any click switches (SONG / PAT, EDITOR / RECORDER) |
| **Folder tabs** | `tab_row`, `panel_header` | a tab row belongs to the panel under it; the seam above is its top edge |
| **Dropdown** | `dropdown`, `dropdown_compact` (mixer inserts) | a well with the value; opens the LCD drawer |
| **Menu** | `menu_drawer` + `DropRow::action`, `heading`, `toggle`, `.column()` | any key opens the same LCD drawer: actions with a shortcut, dim headings, toggles that keep it open, columns (FILE, VIEW, + Instrument, an empty mixer slot). Opens upward when there's no room below; always on screen |
| **Context drawer** | `context_drawer`, `open_drawer_at` + `drawer_at` | the same drawer at the mouse: right-click menus (rack rows, effect slots, browser folders, Patterns) and F3. Submenus are columns with headings |
| **Preset picker** | `user_presets::picker` | a dropdown of presets (yours under a heading) with Save as… / Delete in a second column; Save as turns it into a name field. Fluttersine and Chrysalis keep `user_presets::menu` |
| **Checkbox** | `checkbox` | a sunken well, backlit with a tick; the row is the target |
| **Slider** | `slider` | SL1: a recessed track lit up to a fader cap, a quiet readout; Shift = fine, wheel, double-click resets (F12) |
| **Text field** | `text_field`, `text_field_at`, `name_field` | `name_field`: one undo step per edit |
| **LCD number** | `lcd_number` | every character in its own cell; hover a digit, drag or wheel, double-click to type (BPM) |
| **LCD text** | `lcd_text` | a readout (song position) |
| **Knob** | `knob`, `knob_at`, `paint_knob` | kept as it was |
| **LED, sweep** | `led`, `sweep` | mutes; drag across a row to set them all |
| **Keyboard map** | `keyboard_map` | where the typing keys play |
| **Meter** | `meter`, `meter_bar`, `meter_fraction` | the one standard meter, vertical or horizontal |
| **Hint** | `.hint(text)` (`widgets::Hint`) | every control has one |

## Surfaces

| Piece | Code | Notes |
|---|---|---|
| **Module** | `module` | a toolbar section |
| **Plate** | `plate_tag`, `field_label` | a titled area; caps labels over fields |
| **Editor header** | `app.rs` (the areas' headers) | names and actions; ends in an engraved line, edge to edge |
| **Floating window** | `app::windows` + `widgets::window_title` | Lyra's frame and title row (caps name, close key). No new windows, ever: new tools become pages or tabs |
| **Short dialog** | `app::ask_to_save` | an LCD dialog ("Save changes?") |
| **Splitter grip** | `app::seam_grips` | shows on a resizable seam under the mouse |
| **Busy** | `app::status_bar` | exporting: a pulsing LED and a progress track in the context bar |
| **Drop ghost** | `playlist.rs` | a dragged sample shows as a ghost clip at the snapped spot |
| **Layout** | `flex::row`, `col`, `even_row`, `Px`, `Grow` | one row = one height, one column = one width |

## Still egui's

Nothing in Lyra's own UI (2026-11-25). The F12 panel's control library still
shows the silicone keys (Fluttersine's).
