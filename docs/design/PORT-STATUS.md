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

## To do (Rhodexa's notes, 2026-11-25)

- **Menus become LCD drawers** (File, View, + Instrument), like the dropdowns:
  "everything should be standardized". Then the song key's root / scale pickers
  (still egui ComboBoxes inside the View menu) can switch to `widgets::dropdown`.
- **The gap under editor headers** looks strange. Worst in the audio-clip editor:
  its EDITOR / RECORDER tabs sit under the header line with no top border, so
  they feel incomplete. Decide: tabs *inside* an editor may need their own top
  edge, or the header line and the tabs should merge.
- **Big targets**: the piano roll's octave and scale buttons are the right size
  drawn, but should react across the toolbar's whole height. Same for FILE:
  make it bigger (there's room), or move the toolbar's groups right and leave
  the space.
- **Alignment**: in the audio-clip editor, the transport row (play, stop, rec,
  WAIT…) doesn't line up with the tabs above it. In the Recorder plate, the
  labels, the (disabled) dropdown, PLAYS INTO and CLEAR THE TAPE don't share
  one left edge / width. Rule: one row = one height, one column = one width.
- Smaller: floating windows' frames, the F12 panel's sliders, update
  COMPONENTS.md to the new widget set; remove `#[allow(dead_code)]` tokens as
  they get used.

## Pass two (after the port)

Views from the settled widgets: transport, BPM control, master volume + meter,
oscilloscope / spectrum, the light picker (new: Lyra has no colour picking yet),
rounded docked panels (deferred). See UI-NEEDS.md, "Pass two".
