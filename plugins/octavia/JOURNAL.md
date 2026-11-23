# Octavia journal

Your scratchpad for what you hear. Half sentences and "ugh" are fine.
Claude reads this before choosing what to work on.

**Little marks (optional):**
`!` it really hurts · `?` not sure · `~` someday, no rush

---

## What I hear
*Listening notes on the renders: what sounds fake, what sounds alive.*

- [ ] *(Rhodexa, 2026-11-23)* lament / prelude / articulations: tries to be a string instrument, but there's a synth behind it, like a ~12% duty pulse wave, very chiptune-y. (Nice songs though.) violin-air: genuinely convincing, couldn't tell if it was a recording.
  - Claude: the diagnosis was literal. The bow-position notches `sin(nπβ)` are exactly a 6–13% pulse spectrum, and on cello they land at 0.8–1.6 kHz. First fix: notches blurred over the bow's width, bow position wanders, cello body 28 → 110 small modes. A/B: `renders/ab/lament-before.wav` vs `-after.wav` (same for prelude). If it's still chippy, the next suspects are the overtones' perfectly locked phases (a too-clean pulse shape) and a body that is still too sparse.

- [ ] *(Rhodexa, 2026-11-23)* After the notch fix: better, but still "a Game Boy playing from inside someone's mouth". Vowel-like filtering on a pulse. The first second is convincing, then it falls apart.
  - Claude: two causes. (1) A few broad body resonances = formants = vowels. (2) Overtones perfectly in phase and every cycle identical = a frozen pulse in the sustain. Fix v3: a dense, jagged body response (about 340 interfering modes) applied *per overtone* as gain plus phase, so overtones no longer line up and each flickers under vibrato; faster and deeper jitter and shimmer. Held low G crest factor went from 12.9 to 9.8 dB (less pulse-like). Files: `renders/ab/*-v2.wav` / `*-before.wav` vs `*-v3.wav`.

- [ ] *(Rhodexa, 2026-11-23)* v3: starting to sound convincing (prelude, lament): a cello, but in perfect unison with a Game Boy. Fine in attacks and decays; in sustains the perfect waveform slowly comes to the front. Prelude legato slides sound very fake. Violin barely changed, still good, but lacks power/low end.
  - Claude: measured the parts. Sympathetic strings −48 dB (not it). Bow noise was 40 dB under the tone in sustains, so the sustain was a pure additive synth. v4: steady bow noise raised about 15 dB, half of it combed around the overtones; slurs are now a finger landing (8 ms) or a string crossing (4 ms), and only big expressive shifts slide; violin radiation roll-off 260 → 190 Hz and bridge hill lowered. New knob `--breath X`. Files: `renders/ab/*-v4.wav`, plus `prelude-v4-breath2.5.wav`. Diagnostic: `cargo test --release -p octavia-core how_loud -- --ignored --nocapture`.

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
