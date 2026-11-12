# Lyra journal

Your scratchpad for wishes and friction. Write it however it comes out:
half sentences, typos, "ugh" are all fine. Claude reads this before choosing
what to work on and folds the items into the plan when the time is right.

**Little marks (optional):**
`!` it really hurts · `?` not sure if it's me or Lyra · `~` someday, no rush

---

## Friction
*Things that feel wrong, slow, confusing, ugly, or make you stop playing.*
// - [ ] ? bug or missing?
- [ ]

## Wishes
*Things you wish Lyra did.*
- [ ] 
- [ ]

## Sound
*Ears-only notes: "kick too clicky", "pad needs more air", "this preset slaps".*

- [ ]

## Questions for Claude

- [ ]

---

## Noticed by Claude
*My own small to-dos, kept here so they don't get lost.*

- [ ] Remember sizes / zoom between runs (needs settings saving, comes with project save/load). Until then, "Copy as code" in F12 and I'll make your numbers the defaults.
- [ ] Wheel-over-note changes velocity only while *hovering* a note. With a selection but no hover, the wheel still scrolls, otherwise you couldn't scroll while anything is selected. Tell me if you want it the other way.

- [ ] **No save/load yet.** Anything you make is lost when Lyra closes. First thing in M3.
- [ ] Drum clips in the playlist show their notes as one flat line (every drum sits on C4). Draw one lane per drum channel instead.
- [ ] The piano roll still has its own copy of the overview bar and ruler; move it onto the shared `timeline` module the playlist uses.

## Done ✓
*Claude moves finished items here, with the commit that fixed them.*

- [x] bug? Window won't actaully restore after fullscreen → `50dcf29`
  *→ yes, a bug: full screen overwrote the window's remembered size. Now each mode keeps its own.*

**Journal round 2 → `5ca6d87` (+ `c739b39`)**
*From the journal:*
- [x] Placing or moving notes while playing back plays those notes, which is distracting. Notes should only play previews when the playhead is stopped.
- [x] MMB to pan around.
  *→ piano roll; the playlist will get it too.*
- [x] Notes, more forgiving resize zone, maybe extending a bit beyond/outside the note end. It's too small and hard to hit
  *→ 10 pt inside + 8 pt past the end; both tunable in the Design panel (F12).*
- [x] Button on toolbar to bring cahnnel rack back into context
  *→ WINDOWS module: rack, piano roll, instrument.*
- [x] Double right-click on ghost note to got to that channel → `c739b39`

*From chat (noted here by Claude):*
- [x] Overview scrollbar 3–4× taller → 34 pt (was 11)
- [x] Toolbar 1.5–2× taller → 60 pt (was 38)
- [x] Main buttons and sliders 25–50% bigger; "15% zoom too small to notice"
  *→ real sizes instead of zoom: controls 26 pt, text 14, rack knobs 24, editor knobs 40, steps 24×30. All on sliders in F12.*
- [x] `[- Z = C3 +]` visually inconsistent and too technical
  *→ "TYPING KEYS" module with a mini keyboard, playable range lit in mint.*
- [x] BPM selector too little padding; pattern `< >` uncontained and different heights
  *→ every toolbar group is a captioned module; all buttons are the same height.*
- [x] "We need a way for both of us to design that sort of thing better"
  *→ F12 Design panel (live sliders + "Copy as code") and `docs/design/` for sketches over screenshots.*
- [x] Double-click on a channel raises the editor, but the rack fights back and wins
- [x] Alt+wheel / Ctrl+wheel backwards → Ctrl+wheel = time zoom, Alt+wheel = pitch zoom
- [x] Copy/paste not working
  *→ egui sends Ctrl+C/V as clipboard events, not keys, and only sends paste when the system clipboard has text. Fixed both; paste now lands under the mouse.*
- [x] "+ Channel" shouldn't list 4 configs of the same synth → just "Lyra Synth"; presets live in its editor

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
