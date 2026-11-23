# Octavia journal

Your scratchpad for what you hear. Half sentences and "ugh" are fine.
Claude reads this before choosing what to work on.

**Little marks (optional):**
`!` it really hurts · `?` not sure · `~` someday, no rush

---

## What I hear
*Listening notes on the renders: what sounds fake, what sounds alive.*

- [ ]

## Wishes

- [ ]

## Questions for Claude

- [ ]

---

## Noticed by Claude
*Known gaps, on purpose for the first version.*

- [ ] The tone is a hand-written recipe, not trained. Untuned by ears so far: I only matched numbers (cello centroid about 900 Hz, presence −10 dB).
- [ ] Attacks are only curves (a noise puff plus a brightness bite). There's no real bow-scratch transient or finger thump on shifts yet. This is the next big realism step.
- [ ] One line only: no double stops, no ties (two slurred notes of the same pitch don't re-articulate, but there's no `~` tie mark), no spiccato or sul ponticello yet.
- [ ] The violin body is a rough guess; the cello got more care.
- [ ] Offline only. Streaming and the Lyra adapter are stage 3 (docs/DESIGN.md).
- [ ] About 20× real time per cello, untuned. Fine for now.

---

## Done
- 2026-11-23: First version: rules performer, recipe voice, plucks, sympathetic strings, body, renderer with plots and measurements.
