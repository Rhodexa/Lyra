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

- [ ] *(Rhodexa, 2026-11-23)* v4: the staccato scale (0:05–0:09 of articulations) sounds like a fast-attack synth. Chippiness grows on higher cello notes. The very first note of articulations sounds awesome. Violin still great. Breath up to 1.0 is okay, higher is gritty-bad. From a real cello recording (`ext_sample/cello.wav`): ours is far too bright; the bow sounds like a pencil moving on paper; the fingers on the fingerboard are obvious, like fingertips tapping a wooden table; heavy reverb (room?).
  - Claude: measured the recording with the new `octavia analyze`. Ours was 10–15 dB too bright above 1 kHz. v5: steeper overtone slope and lower high cut (now within about 3 dB of the recording, octave by octave); bow noise darker and grainy ("paper texture"); **finger taps**, a new curve event (`tap,time,strength`), knocked through the wooden body, hardest when a finger hammers down mid-slur; releases ring on and darken instead of gating off (that was the synthy staccato); more cello low end. The violin also got darker (centroid 2500 → 1860 Hz): compare `violin-air-v4` and `-v5`, and if v4 was better I'll give the violin its own tone. Files: `renders/ab/*-v5.wav`.

- [ ] *(Rhodexa, 2026-11-23)* v5 sounds much better. Found raw recordings (Iowa sample, anechoic Mozart cello): real cellos are much noisier than expected; the old breath-2.5 render was closer.
  - Claude: added a "noise floor" measure (energy between the overtones). Clean references sit at −21 to −23 dB; our held note at breath 1 was −30. Bow noise is now 8 dB louder at the voice, so breath 1 measures −24 on a held note. Attack scrapes rose with it. Files: `renders/ab/*-v6.wav`.
  - Licences (Rhodexa): Iowa MIS = free for anything (training candidate). Aalto anechoic = restrictive, so listening and measuring only.
- [ ] *(Rhodexa, 2026-11-23)* v6: "It definitely sounds like a cello now!" Loves v6 *and* v4; v4 isn't bad in retrospect. Loves v4's brightness and power, "boss fight" like a French horn.
  - Claude: `--bright 0..1` dial: 0 = v6 natural, 1 = v4-like bright and powerful (brighter overtones, lighter and smoother noise). Examples in `renders/bright/0.5` and `renders/bright/1`. In Lyra this becomes a knob.

- [ ] *(Rhodexa, 2026-11-23)* Prelude feels wrong: fast, wide legato leaps, as if sliding far up one string. A real player would change strings instead.
  - Claude: right, a real bug. The old rule stuck to one string within 9 semitones, so G2→D3 was a 7-semitone leap on the G string. Now there's a left-hand model: open strings and notes under the hand are free, crossings cheap, shifts expensive. Bar 1 plays open G, open D, A string with no shifts (tested). Shift slides scale with distance. And since one oscillator still sounds like one string leaping, **strings left behind now ring on** (open ones ~1.6 s, stopped ones ~0.5 s, choked when the bow returns), about 13 dB under the bowed line. A/B: `renders/ab/prelude-v6.wav` vs `prelude-v7.wav`.

- [ ] *(Rhodexa, 2026-11-23)* MIDIs in `midi/`: marmalade is the best of them. Monophonic of course, but it already makes some very interesting tests.
  - Claude: marmalade is a piano track (57% chords, E1–A6). The top-note line was grabbing left-hand notes under held melody notes and folding 53 notes by an octave one at a time, both heard as octave spikes. Now accompaniment under a held note is skipped, the octave shift is chosen to need the fewest folds (53 → 12), and the cello range goes up to A5. Re-rendered into `midi/marmalade.wav`.

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
- [ ] Streaming works (2026-11-23); the Lyra adapter is next and needs Lyra to hand over upcoming notes.
- [ ] Inferred accents only fire when a note is 0.12 louder than its neighbourhood average, so a run of accents in a row dilutes itself (only the first one in `articulations.roll` gets accented).
- [ ] No "open notes" yet (length unknown until note-off), so live keyboard playing isn't supported.
- [ ] About 20× real time per cello, untuned. Fine for now.

---

## Done
- 2026-11-23: Performer sees one second ahead, streaming engine, inference from bare piano rolls (`.roll`), pizzicato as a patch. The inferred Bach prelude bows exactly like the hand-marked one.
- 2026-11-23: First version: rules performer, recipe voice, plucks, sympathetic strings, body, renderer with plots and measurements.
