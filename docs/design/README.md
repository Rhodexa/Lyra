# Design drop-box

How we design Lyra together without either of us having to describe pixels in words.

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
