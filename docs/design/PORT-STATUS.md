# The UI port: where it stands

For whoever continues the port (a later session, or another agent). The spec is
[catalog-next.html](catalog-next.html) (its "Settled" index). The old look is in
[widget-catalog.html](widget-catalog.html). Rules: Fluttersine and Chrysalis are
frozen in `plugin_look.rs` and must not change; one commit per step, each with
before/after screenshots from the same saved state; no confirmation needed per
widget (Rhodexa reviews as it goes).

## Done (2026-11-25)

1. Synths decoupled (`plugin_look.rs`: frozen look, style, fonts).
2. Foundations in `theme.rs`: Roboto / Roboto Condensed bundled, tokens, S/M/L,
   `outline_of` / `outlined`.
3. Widgets (each its own commit, `Port step 3x`): button, mode switch, folder tabs,
   dropdown (+ compact for mixer inserts), list rows, text field, LCDs (cells,
   inverse aim), scrollbars (vertical = 34 px), hints (`widgets::Hint`), LCD
   dialog, bare button, egui buttons restyled, plates (`plate_tag`) and field
   labels, one standard meter, checkbox, splitter grip, busy (export progress,
   confirmed working by Rhodexa), drop ghost, header lines, mixer index tags.

4. Rhodexa's notes (2026-11-25), each its own commit:
   - **3s** Menus are LCD drawers (`widgets::menu_drawer`): FILE, the piano
     roll's VIEW (toggle rows), + Instrument (headings; opens upward when
     there's no room). The song key's root / scale are header dropdowns.
   - **3t** Big targets: the octave keys react across the toolbar's height
     (`button_reach`); FILE is as tall as the toolbar's keys.
   - **3u** The Recorder plate is one column, one width. (Its mode switch for
     EDITOR / RECORDER was wrong: they're tools, so they're tabs again.)
   - **Areas have no padding** (6119d98): the real cause of the gap under
     every editor's header. The header pads itself; editors run edge to edge.
   - **3v** Floating windows: Lyra's frame and title row. The slider (SL1) and
     the F12 panel on it, scrolling. COMPONENTS.md lists the new widget set.

## Still open

- The egui leftovers listed at the end of [COMPONENTS.md](COMPONENTS.md):
  right-click menus, the F3 menu, the Presets menu (needs a drawer that
  holds a text field).
- `SURFACE_SEL`, `SIZE_S`, `SIZE_L` are still unused (`#[allow(dead_code)]`).

## Pass two (after the port)

Views from the settled widgets: transport, BPM control, master volume + meter,
oscilloscope / spectrum, the light picker (new: Lyra has no colour picking yet),
rounded docked panels (deferred). See UI-NEEDS.md, "Pass two".
