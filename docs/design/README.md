# Design drop-box

How we design Lyra together without either of us having to describe pixels in words.

## The widget catalog

[widget-catalog.html](widget-catalog.html) is the design reference: every widget
Lyra draws, in today's colours (the palette is named in its `:root`, after the
Rust constants). Each family shows the current version first, drawn with the
app's own numbers, then every earlier study (from `HTMLSamples15112026/`) with
its version, source file and what it changed, recoloured but with its shape
untouched. Open it in a browser; every control is live.

## Sizes: the Design panel (F12)

Every size that shapes how Lyra feels lives in `theme::Metrics`: text, button height,
toolbar height and padding, knob, step and light sizes, piano-roll bars, resize zones.
Press **F12**, drag the sliders and the app changes live. When it feels right, press
**Copy as code** and paste the result to Claude, or into `JOURNAL.md`. It drops
straight into `Metrics::DEFAULT` in `crates/lyra-app/src/theme.rs`.

## Looks: sketch over a screenshot

1. Take a screenshot (Spectacle), or ask Claude for one (`LYRA_SCREENSHOT=…`).
2. Paint over it in Krita: arrows, circled problems, a redrawn module, colour swatches.
   Rough is fine.
3. Save it here as `YYYY-MM-DD-topic.png` (e.g. `2026-11-12-toolbar.png`).
4. Mention it in `JOURNAL.md`. Claude looks at the image and builds from it.

## Assets for the app

Finished art (logo, knob sprites, textures, icons) goes in `/assets`. Claude will
say exactly what each slot needs: size, format and states.
