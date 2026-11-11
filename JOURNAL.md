# Lyra journal

Your scratchpad for wishes and friction. Write it however it comes out:
half sentences, typos, "ugh" are all fine. Claude reads this before choosing
what to work on and folds the items into the plan when the time is right.

**Little marks (optional):**
`!` it really hurts · `?` not sure if it's me or Lyra · `~` someday, no rush

---

## Friction
*Things that feel wrong, slow, confusing, ugly, or make you stop playing.*

- [ ]

## Wishes
*Things you wish Lyra did.*

- [ ]

## Sound
*Ears-only notes: "kick too clicky", "pad needs more air", "this preset slaps".*

- [ ]

## Questions for Claude

- [ ]

---

## Noticed by Claude
*My own small to-dos, kept here so they don't get lost.*

- [ ] Remember the UI size you pick with Ctrl +/- between runs (needs settings saving, comes with project save/load).
- [ ] Wheel-over-note changes velocity only while *hovering* a note. With a selection but no hover, the wheel still scrolls, otherwise you couldn't scroll while anything is selected. Tell me if you want it the other way.

## Done ✓
*Claude moves finished items here, with the commit that fixed them.*

**Journal round 1 → `3e78962`**
- [x] ! ? Slightly bigger UI, more forgiving targets
  *→ 115% by default (Ctrl `=`/`-`/`0` adjusts, `LYRA_ZOOM` sets the start), bigger mute lights, wider note-edge grab.*
- [x] Moving hot keys
    Octave > Ctrl + Up/Down
    Semitone shift > Shift + Up/down
  *→ plain ↑↓ also does a semitone; Ctrl+←→ moves by a beat.*
- [x] ~ support piano roll full screen
  *→ "full screen" / "restore" button in the piano roll toolbar.*
- [x] ! Releasing Alt before note resize causes it to resnap. Keep snap off after risize beings regardless of alt, or at least give it more forgiving timing
  *→ Alt now latches for the whole drag (resize and move).*
- [x] Allow to toggle other channels back on after a Solo (Right click on channel)
  *→ FL-style: solo mutes the others, click any dark light to bring it back, right-click the soloed one again to unmute all.*
- [x] ~ Copypaste
  *→ Ctrl+C / X / V; pastes in place, selected, ready to drag.*
- [x] ~~ duplicate to end (Ctrl + B)
  *→ Ctrl+B = Ctrl+D = duplicate the selection right after itself.*
- [x] ~ Better canvas pan (generic) Top scrollbar for rolls/sequencers, grabbing thumb from edges zooms (viewport window style). Alt + wheel scroll > horizontal zoom, Ctrl + wheel vertical zoom
  *→ done in the piano roll; the playlist gets the same bar when it exists.*
- [x] ~~ Exception: If notes selected or hover on note: Change velocity/current per-note parameter instead
  *→ hover version, see "Noticed by Claude".*
- [x] Remove Lyra text next to logo, might even remove logo since window hearder already shows it (or should)
  *→ text removed, logo kept for now (KDE Wayland only shows our icon once Lyra has a .desktop file).*
- [x] ~~ Highlight in piano roll doesn't provide a huge effect. Should be moved to a menu in the piano roll preferences instead. (part of window)
  *→ a "View · A minor" menu: key, scale, shading, ghost notes.*
- [x] ! ~ Clicking timeline cursor in piano rool should reposition cursor, not resize patterns. Patterns should be dynamic, and by default play until the end of last filled bar (that is, rounding up)
  *→ ruler click/drag moves the playhead (Play starts there, Stop returns there); patterns grow and shrink with their notes, and the rack's bars menu is now just the minimum.*
- [x] Toolbar: Add piano scale — Octave base, mostly. Where does the keyboard start
  *→ "keys − Z = C3 +" in the top bar.*
- [x] ~~ Support Duplicate/Clone channel (Duplicates instrument, not its note sequnces)
  *→ right-click a channel name → Clone channel.*
- [x] ~~ Support Replace plugin
  *→ right-click → Replace instrument (keeps the notes).*
- [x] ~~ Recontextualizing a window should bring it back to front. i.e Edit instrument/double clicking channel should bring the plugin's window to front even if already open. Applies to piano roll too and other potential windows
- [x] ~~~ Click and drag to turn on multiple channels at once, similar to Blender's Click and slide over to hide/unhide multiple elements at once.
  *→ sweep across the lights; the first one you press decides on or off.*
- [x] *(Claude)* Drum channels default to 1-step notes in the piano roll.
- [x] *(Claude)* Space only ever means play/stop.

**Earlier**
- [x] App froze on the first click inside a window → `3ba6958`
- [x] Audio underruns / crackles → `1bb37a1`
