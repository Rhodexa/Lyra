# Spec: wavetable synth (working title, the director names it)

A built-in instrument for modern electronic sound design: growls, talking
basses, evolving pads, big supersaw-plus leads. Serum and Vital territory,
smaller.

Why it exists (from writing "Lyra Ascendant", 2026-11-19): Fluttersine's
growls, even with its new mod matrix (FM depth on synced LFOs), live in FM's
world. Modern bass design is wavetables morphing along *drawn* rhythmic
curves, run through per-voice distortion and formant filters. That needs
an instrument built around it, not more Fluttersine (it wasn't designed for that).

## Sound

* **2 wavetable oscillators.** A table is up to 256 frames × 2048 samples;
  *position* morphs between frames. Mip-mapped per octave (no aliasing).
  * **Warp per oscillator:** bend ±, sync, mirror, quantize, FM from the other
    oscillator, PWM-style phase warp.
  * **Unison** up to 16 voices with detune, blend and stereo spread.
* Sub (sine/square, −1/−2 oct) and noise (with colour) oscillators.
* **Tables:** a built-in set (basic shapes, vocal formants, "growl" sets
  generated additively), plus import a WAV as a table (cut into frames of
  2048). The import button needs a file dialog (rfd, like the browser).
* **2 filters** (serial/parallel): low/high/band SVF, **formant** (vowel
  morph A–E–I–O–U), **comb** (±, feedback), with drive.
* **Per-voice distortion** before the filter: tanh, hard, fold, bit-crush.
  (Lyra's Drive effect does this on the whole mix; per voice is different.)

## Modulation

* **4 LFOs with drawable shapes.** Multi-point curves with per-segment bend,
  plus shape presets. Tempo sync (the `SYNC_LABELS` set Fluttersine uses),
  song-locked unless retriggered, one-shot mode (an LFO that's really an
  envelope). Drawable rhythms are what make wobbles interesting instead of
  a sine going up and down.
* 4 ADSR envelopes (with curves).
* A **mod matrix**, 16 slots: any source → any destination, bipolar amount,
  and an optional *amount source* (e.g. velocity scales the LFO's depth).
* 4 **macros** (big knobs) that drive several slots at once. With automation
  (M5) they'll be the thing to automate.

## Real-time rules

Tables are built and mip-mapped on the UI thread (in `set_param` side tables
or a `set_sample`-like hook) and handed over in an `Arc`, never allocated in
`process()`. Unison × voices × 2 oscillators needs a CPU budget test like
Fluttersine's.

## Editor

A big hand-made editor: oscillator panels with a 3D-ish table view (frames
stacked), the LFO curve editor (points + bend handles), the matrix as rows
like Fluttersine's MOD page. The director's graphic pass, in the plate language.

## Done when

* Crate, catalog entry, `params.lock`; tests for table morphing (position 0/1
  gives the first/last frame exactly), no aliasing above Nyquist on a saw table
  at high notes, LFO curve evaluation, and CPU budget.
* Presets: growl, talking bass, reese, pluck, supersaw lead, evolving pad.
* A demo in `crates/lyra-app/examples/` (songkit) rendered with `lyra --render`.
