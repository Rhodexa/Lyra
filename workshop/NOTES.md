# Notes: known limits and leftovers

Moved out of JOURNAL.md (2026-11-25) so the journal stays short. These are
things Claude noticed while building: what a feature leaves out on purpose,
what a song needed and didn't have. Nothing here is urgent. When one starts
to matter, it goes back into JOURNAL.md as friction or a wish.

- [ ] *(Sharing prep, 2026-11-27)* `LYRA_SCREENSHOT` writes uncompressed PNGs (8.6 MB at 1920×1126); the mockups in `workshop/design` came out of it. Compress before screenshots go in the README, or git keeps the heavy ones forever.

- [ ] *(Octavia in Lyra, 2026-11-25)* The generic instrument editor cuts choice labels short: Octavia's Instrument and Patch read "CE…" and "AR…" in their dropdowns (the value under them is whole). Octavia gets her own faceplate soon, but other plugins with longer choices will hit it.

- [ ] *(Octavia in Lyra, 2026-11-25)* Plugins can report latency (`Plugin::latency`), but Lyra doesn't compensate it. Nothing built in needs it today (Octavia gets look-ahead instead); hosted CLAP plugins with latency will.

- [ ] *(from writing "Lyra Ascendant", 2026-11-19)* No automation yet. Risers had to be faked (an 8-second attack plus a 10-second filter-envelope attack on noise, and a supersaw gliding between overlapping notes). Synced LFOs now cover the wobble speeds; automation (M5, after CLAP) is still the real fix for risers and swells.

- [ ] *Chrysalis, first version (2026-11-19): what's left out or rough, on purpose:*
  - One imported WAV per channel: both oscillators can play it, but they can't each have their own. (A second file needs a real plugin-state blob; see ARCHITECTURE "Plugin parameters".)
  - "LFO n rate" in the matrix only moves Retrig and One-shot LFOs. A Free LFO is shared by every voice, so one voice can't speed it up.
  - If both oscillators FM each other, oscillator 2 hears oscillator 1 one block (≤ 32 samples) late.
  - The drive and the Sync / Mirror / Quantize warps alias a little (no oversampling), like the Drive effect. The tables themselves don't.
  - While a note glides, the band-limited copy changes every half octave with no crossfade: in theory a faint step in brightness at the very top.
  - The built-in tables take ~37 MB of memory, shared by every Chrysalis.
  - Macros have numbers, not names (names aren't numbers, so they'd need that same state blob).

- [ ] *(from writing "Dawnbreaker", 2026-11-20: the first song built on automation clips. They did every build-up: shared curves, several links, offsets and "hold after the clip" all behaved.)* What was missing:
  - **Sends.** Nine inserts each carry their own hall reverb because nothing can share one. That costs CPU, and the room isn't one room. (Roadmap M3: "Sends / sidechain".)
  - **Tempo automation.** A heroic ending wants to broaden (a ritardando into the last chord); tempo isn't a control yet.
  - **Chrysalis's vowel knob runs A–E–I–O–U.** Morphing "oo" to "ah" passes through "ee". Ordering it U–O–A–E–I (back of the mouth to front) would make every neighbour a natural glide: oo, oh, ah, eh, ee. Presets would need their vowels remapped (no file compat needed yet).
  - **Stem export is slow.** 14 stems took 174 s against 17 s for the mix, because every stem runs every instrument and only mutes the others. Skipping muted channels, or one pass that taps every channel, would make it about as fast as one render.

- [ ] *Lyra FM, notes from the port (behaviour kept from Fluttersine for now):*
  - Saw/pulse stop being band-limited while modulated, so some aliasing on bright FM patches.
  - An operator that becomes "used" while a note is held (raising a matrix cell) stays silent until the next note.
  - Mod wheel, pitch bend and sustain pedal aren't wired (Lyra's events don't have them yet). Chorus/delay from Fluttersine presets now belong in mixer effects.
  - The Ratio knob is continuous (FM Bell needs 5.19). The editor could snap to common ratios unless Alt is held.

