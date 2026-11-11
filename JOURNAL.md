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

- [ ] ! ? Slightly bigger UI, more forgiving targets
- [ ] Moving hot keys
    Octave > Ctrl + Up/Down 
    Semitone shift > Shift + Up/down

- [ ] ~ support piano roll full screen 
- [ ] ! Releasing Alt before note resize causes it to resnap. Keep snap off after risize beings regardless of alt, or at least give it more forgiving timing
- [ ] Allow to toggle other channels back on after a Solo (Right click on channel) 
- [ ] ~ Copypaste
- [ ] ~~ duplicate to end (Ctrl + B)
- [ ] ~ Better canvas pan (generic) Top scrollbar for rolls/sequencers, grabbing thumb from edges zooms (viewport window style). Alt + wheel scroll > horizontal zoom, Ctrl + wheel vertical zoom
- [ ] ~~ Exception: If notes selected or hover on note: Change velocity/current per-note parameter instead
- [ ] Remove Lyra text next to logo, might even remove logo since window hearder already shows it (or should)

- [ ] ~~ Highlight in piano roll doesn't provide a huge effect. Should be moved to a menu in the piano roll preferences instead. (part of window)

- [ ] ! ~ Clicking timeline cursor in piano rool should reposition cursor, not resize patterns. Patterns should be dynamic, and by default play until the end of last filled bar (that is, rounding up)

- [ ] Toolbar: Add piano scale — Octave base, mostly. Where does the keyboard start 

- [ ] ~~ Support Duplicate/Clone channel (Duplicates instrument, not its note sequnces)
- [ ] ~~ Support Replace plugin 
- [ ] ~~ Recontextualizing a window should bring it back to front. i.e Edit instrument/double clicking channel should bring the plugin's window to front even if already open. Applies to piano roll too and other potential windows

- [ ] ~~~ Click and drag to turn on multiple channels at once, similar to Blender's Click and slide over to hide/unhide multiple elements at once.


## Sound
*Ears-only notes: "kick too clicky", "pad needs more air", "this preset slaps".*

- [ ]

## Questions for Claude

- [ ]

---

## Noticed by Claude
*My own small to-dos, kept here so they don't get lost.*

- [ ] Piano roll's default note length is 2 steps even on drums, which turns a drum row into a melody preview. Use 1 step for drum channels.
- [ ] Space can also "press" a focused button or menu. Make sure Space only ever means play/stop.

## Done ✓
*Claude moves finished items here, with the commit that fixed them.*

- [x] App froze on the first click inside a window → `3ba6958`
- [x] Audio underruns / crackles → `1bb37a1`
