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

- [ ] *(Rhodexa, 2026-11-23)* marmalade ~31–32 s: a note repeated fast smears out (the bow just goes back and forth). It should bounce the bow (spiccato) instead.
  - Claude: the four repeated C5s were slurred by the fast-run rule (a slur between identical notes = one long note), and the MIDI import read a small same-key overlap as a slur. Now repeated notes are never slurred, same-key overlaps are cut, and **spiccato** is inferred for fast notes bowed separately: a drop, a tick of hair, then the string rings free and darkens. Score mark `*`. Re-rendered `midi/marmalade.wav`.

- [ ] *(Rhodexa, 2026-11-23)* Double stops, please.
  - Claude: done. A note can carry a second, lower pitch (`D3+A3` in scores; two notes starting together in a roll; a held chord note in MIDI). The performer only plays it if one hand can reach both notes on neighbouring strings and the note is at least 0.25 s long; otherwise it plays the top note alone. A second bowed voice shares the body and the vibrato. Pizzicato double stops pluck both notes, rolled by 12 ms. In marmalade, 21.75–28.5 s is now a run of double stops; octaves get refused (too wide for the hand). `--no-doubles` turns it off for MIDI.
- [ ] Octave double stops need a stretched hand (or thumb position), which isn't modelled; for now they're played as single notes.

- [ ] *(Rhodexa, 2026-11-23)* Bug in what it does after a spiccato (`midi/angel.mid --track 8 --bright 0.5`). Would love to see it side by side with the MIDI.
  - Claude: three bugs. (1) Long notes ending a fast run were bounced (A#4 0.78 s, D#5 2.6 s: a drop, then two seconds of free ringing), so now only short notes bounce. (2) The note after a bounce started "from silence" with a slow, soft attack; now it's a new stroke. (3) Any note after a short lift (bounce or staccato) got the slow start; now only real rests do. For side by side: the curves plot colours each written note by how it was played, and `--explain` lists it.

- [ ] *(Rhodexa, 2026-11-23)* crystalize.mid (Lindsey Stirling): great test material, lots of string switching.
  - Claude: rendered on violin (`midi/crystalize.wav`, shifted up an octave to fit) and cello (`midi/cello/crystalize.wav`). `--explain` now also shows the string (IV lowest … I highest), open strings, hand position and shifts. Found that the hand was cello-sized on the violin too, giving 61 tiny one-semitone shifts. Hand span is now per instrument (cello 4, violin 6 semitones): 8 shifts left, all real climbs to D6/G6 and back.

- [ ] *(Rhodexa, 2026-11-23)* crystalize: way too much legato. Taps sound uncanny right at the note after silence: a player places the finger quietly beforehand. Spiccato needs a stronger bow *landing*. (Sounds better overall.)
  - Claude: all 129 slurs came from "fast runs four to a bow" firing on any quick pair. Now only real runs (four or more fast notes) slur: very fast (<0.15 s) in fours, moderately fast in pairs (129 → 59; quick figures bounce instead). No finger tap after a rest. Bounces land with a knock through the body plus a stronger, longer scrape. Note: the inferred Bach prelude is now bowed in pairs, not fours.

- [ ] *(Rhodexa, 2026-11-23)* beethoven2.mid (Beethoven Virus) track 5, 0:51–1:04: lots of string crossing, yet it slurs.
  - Claude: 0.09 s sixteenths hit "very fast runs four to a bow", but those are zig-zag figures rocking across strings (D2–A2–D2–G2…, a pedal note against a moving one), which players bow note by note. Now fast runs slur only where the line flows (steps, or leaps continuing the same direction); a leap reversing against a neighbouring interval is bowed separately, and short notes bounce. 0:51–1:04 now: sweeping arpeggios slurred, rocking figures bounced. Rendered as `midi/beethoven2.wav`.

- [ ] *(Rhodexa, 2026-11-23)* What happens when the player releases a stopped (non-open) string while it's still decaying?
  - Claude: the finger lifting turns the string back into the open string: the stopped note is choked, the leftover energy rings faintly at the open pitch, plus a tiny pluck from the finger leaving. Modelled now: before a rest, once a stopped note has died to about a quarter (as players wait), the finger lifts. Not for open strings, plucks, double stops, or when the next note is close. Demo: `scores/lifts.txt` → `renders/lifts.wav`.

- [x] *(Rhodexa, 2026-11-23)* "You have no idea how good Rainbow Tylenol sounds with this." Ran: `midi/rainbow.mid --instrument violin --bright 0.3 --transpose -12`.
  - Claude: now a reference piece. `scripts/refs.sh` re-renders every piece Rhodexa liked with its exact settings and lists the notes that are now played differently; `--accept` makes a run the new baseline. I run it after every performer change.

- [ ] *(Rhodexa, 2026-11-23)* Megalovania: "too too DAH": the DAH isn't properly attacked. Bug or the track?
  - Claude: a bug, two stacked. (1) The second "too" overlapped the DAH by 4 ms (the file's rounding) and the importer read it as a slur, so the DAH had no attack at all. Overlaps now count only when meant: at least 30 ms or an eighth of the step to the next note. (2) The whole file is velocity 25/127, so everything was pianissimo with slow, soft attacks. MIDI velocities are now centred on mf per line (`--raw-velocity` to keep them). Plus a new gentle rule: a leap up of a fifth or more out of a quick pickup gets an accent. Opening is now bounce, bounce, crisp accented DAH.
  - Reference check: Rainbow Tylenol has one new bow change at 1:17.8 (a borderline overlap). Accepted as the new baseline; tell me if you hear it.
- [ ] ~ Lyra can't open MIDI files yet: that would make A/B testing of tracks much easier (for Lyra's journal).

- [ ] *(Rhodexa, 2026-11-24)* After a day of listening (Lindsey Stirling's Roundtable etc.): way too perfect. Ideas: a performer **workload**, **composure**, **skill** sliders; grit and failed bow starts; tap variation; accidental open-string plucks when frantic; bow position as a decision; selective, varied vibrato (also on double stops); overshoot/undershoot and correction; imperfect slides with finger noise; strings not perfectly tuned. Context from outside: `docs/design/checklist-gpt.txt`, `docs/design/faceplate-gpt.html`.
  - Claude, step 1 done: `Skill` (intonation, dexterity, bowing, composure; `--player concert|student|virtuoso|session` or one flag each), workload per note (speed, hurried shifts, string skips, leaps, double stops, bounces; eased by dexterity), composure (drains under load, recovers in rests). They now drive intonation (landing off, overshoot after hurried shifts, corrected by ear at a speed set by skill; errors drift slowly via a "hand bias" instead of re-rolling), per-string tuning (open strings a couple of cents off), vibrato (straight notes sometimes, varied depth/onset/speed, nerves make it quicker and narrower; double stops narrower, lower string less) and tap loudness (busier = harder, never identical). `--explain` shows load and calm per note. Next: physical speed limits, then bow-start grit, slides, accidental brushes, bow position.

- [ ] *(Rhodexa, 2026-11-24)* Physical speed limits: how fast can fingers move, can a bow really bounce that fast? beethoven.mid's opening back and forth sounds synthetic, way too perfect. (Dropping notes is the composer's job, not Octavia's.)
  - Claude: the opening is A2 repeated at 6 a second: not hard, but every stroke was identical, like a drum machine. Now **the bow arm**: strokes alternate down/up (down-bows heavier, more bite), each stroke varies with a slow drift (more for less bowing skill or more load), and the bow bites a few ms late. Deadpan stays mechanical. **Limits**: a bow can't bounce beyond ~12 strokes a second (counting strokes, not notes: a slurred pickup is one); beyond that it stays on the string; faster strokes dip less. A/B: `renders/ab/beethoven-{cello,violin}-{before,after}.wav` and `beethoven-cello-student.wav`. Reference check: 5 notes in marmalade/Rainbow now on-string instead of bounced (14–21 strokes a second: physically right).

- [ ] *(Rhodexa, 2026-11-24)* The student plays plain bad now ("I could play as bad"). A student doesn't hear worse: they land worse when it's fast, correct slower, and on fast notes there's no time to fix it; long important notes still end in tune. (Skipping notes stays the composer's call.) Next: bow-start grit. Also: files everywhere.
  - Claude: landing error grows with load (curve gentler); correction speed is the skill; what stays off once settled is tiny for everyone (tested: a student's long notes settle within ~5 cents). **Grit**: until the bow catches the string, the period wanders and the tone crunches with extra noise, for 30–100 ms: more for hard starts (accents, staccato), little preparation time, a weaker or busier arm; now and then a start fails (crunch plus a stall before it catches); slurred crossings catch the new string with a small scrape. It's a new curve column, `grit`. **Files**: `midi/` holds only .mid; renders go to `renders/midi/` (`scripts/render-midis.sh` renders them all, extra options pass through, e.g. `--player student`). A/B: `renders/ab/beethoven-cello-{after,grit,student}.wav`, `beethoven-violin-grit.wav`.

- [ ] *(Rhodexa, 2026-11-24)* Flowering Night (flower.mid): the plucks at the start when strings are released are always the same. (Loves them, just not identical.) Grit is quite noticeable; we'll tune it slowly.
  - Claude: those are strings left ringing on crossings; the phrase repeats, so the rings were identical. Each ring now varies in strength, brightness and ring time with how the bow left the string; the quick plucked start is kept (I briefly softened it, then put it back). Finger lifts vary too: sometimes no ghost at all, the lift tap varies. New ring column `bright`.
- [ ] ~ Grit amount: noticeable at the moment; tune slowly by ear (`--bowing` moves it).

- [ ] *(Rhodexa, 2026-11-24)* How do we make the player really shake the bow instead of spiccato on short notes? (And: dying for a UI.)
  - Claude: that's bowed tremolo, which didn't exist. Added like pizzicato (asked for, not inferred): `~` in scores, `--tremolo` patch (every note of 1/4 s or more; shorter notes stay normal), later per-note data in Lyra. 13–16 strokes a second, slightly uneven, each reversal a small dip and scrape, down-strokes heavier; faster and rougher under load; works on double stops. Demo: `scores/tremolo.txt` → `renders/tremolo.wav`; `renders/ab/flower-tremolo-patch.wav`. Pink in plots.

- [ ] *(Rhodexa, 2026-11-25)* The HTML UI shows a bug in Flowering Night: the cool weird sounds correlate with a finger left on a string after the hand has moved out of reach. A real player can't do that.
  - Claude: right. A string left ringing at a *stopped* pitch only ended when the bow came back or it died out, even if the hand had shifted away from the finger holding it. Now the performer tracks which fingers are holding ringing strings; when the hand moves out of reach (or a crossing shifts at once), the finger lifts: the string rings open (faintly, sometimes not at all) with a tiny tap. Flowering Night: 64 of 238 stopped rings were ghost fingers; they now become open-string ghosts.

## Wishes

- [ ] ! *(Rhodexa, 2026-11-25)* The performer should plan ahead: look at what's coming and say "I think I know what I'm going to do next", then show it on the screen and play it out (changing its mind when the music changes). Planned decisions are the core of the engine; without them it's just another violin synth. Sound first, a nice screen second.
  - Claude (step 1, the left hand): she now fingers the whole visible passage (up to 12 notes) and plays the first step of the cheapest plan; a shift costs more when there's no time for it, so shifts move into long notes and rests. On your 15 MIDI pieces: 12% fewer shifts, 16% less hurried shifting (flower −24%, hatsune −23%). The faceplate gets her intention for every coming note (`NoteInfo::intent`: string, hand). **Listen:** `renders/ab/plan/{flower,beethoven,hatsune}-{before,after}.wav`; it should sound calmer in fast runs (fewer slides and missed landings). Next steps of planning: the bow (down-bows where the music leans, saving bow for long notes), then how each note is played.
- [ ] *(Rhodexa, 2026-11-25)* Live playing feels responsive and sounds like a violin, but there's no way to control the attack when playing live.

## Questions for Claude

- [ ]

---

## Noticed by Claude
*Known gaps, on purpose for the first version.*

- [ ] The tone is a hand-written recipe, not trained. Untuned by ears so far: I only matched numbers (cello centroid about 900 Hz, presence −10 dB).
- [ ] Attacks are only curves (a noise puff plus a brightness bite). There's no real bow-scratch transient or finger thump on shifts yet. This is the next big realism step.
- [ ] One line only: no double stops, no ties (two slurred notes of the same pitch don't re-articulate, but there's no `~` tie mark), no spiccato or sul ponticello yet.
- [ ] The violin body is a rough guess; the cello got more care.
- [ ] Inferred accents only fire when a note is 0.12 louder than its neighbourhood average, so a run of accents in a row dilutes itself (only the first one in `articulations.roll` gets accented).
- [ ] About 20× real time per cello, untuned. Fine for now.
- [ ] *(found by the faceplate, 2026-11-25)* A string crossing that comes with a shift still rings the old string at its **stopped** pitch, though the hand has moved away. `showcase` at 0.70 s: D3 on the G string (finger at 7), then the hand shifts to 2 (covers 2–6) for E3, and the G string rings on at D3. The finger should lift (ring open, or damped). Changes the sound, so it needs a listening A/B.

- [ ] *(2026-11-25, for the faceplate)* NEXT can only show *how* a note is played once the performer commits to it, 0.12 s before it starts (`PLAN_LEAD`); earlier, a coming note has its pitch and time but `decision: None`. The mock knew every decision in advance from the renders. Either NEXT shows undecided notes plainly (pitch only), or the performer learns to pre-decide (a bigger change: decisions would then have to stay revisable).
- [ ] *(2026-11-25)* There is no "Player" parameter: the four profiles are presets that set the skill faders, and the LCD can name the profile by matching the faders (or show CUSTOM). The faceplate's profile buttons load presets. If a saved, automatable Player is wanted after all, it goes at the end of the table.
- [ ] *(2026-11-25)* Switching Cello ↔ Violin mid-song cuts what's sounding (two engines, one plays). Fine for an instrument switch; say if it should crossfade.
- [ ] *(2026-11-25)* Two notes pressed live within ~10 ms become a double stop; slower "rolled" chords become two notes in a row (a cello can't hold a third anyway). From the piano roll, chords always land together.

---

## Done
- 2026-11-25: **In Lyra.** `crates/octavia-lyra`: the parameter table, the four profiles as presets, notes in, audio out, telemetry. Lyra hands over the next second of notes (`lyra.note-lookahead`) as a *forecast*; note-ons and offs stay the truth: a deleted note is dropped (or cut if the hand had committed), a key played live is sight-read and held until its note-off ("open notes"), chords become double stops. Played through Lyra's API the result is sample-identical to `render_score`. Telemetry (`octavia_core::telemetry::Snapshot`) comes from the engine's own state and matches `--explain` note for note.
- 2026-11-23: Performer sees one second ahead, streaming engine, inference from bare piano rolls (`.roll`), pizzicato as a patch. The inferred Bach prelude bows exactly like the hand-marked one.
- 2026-11-23: First version: rules performer, recipe voice, plucks, sympathetic strings, body, renderer with plots and measurements.
