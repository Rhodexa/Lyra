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
- [ ] ~ Docked layout (Blender / VS Code style areas that don't overlap) for the main views; it would also save the title bars (~5rem). Floating windows stay for plugins.
- [ ] ~ Once the desktop icon works, drop the logo from the toolbar.
- [ ] ~ Start building docked windows for essential panels. Eg:
Browser | Arranger/PainoRoll (Swappable or on top of eachother with easy buttons/hotkeys) | Inspector (Potentially Channel rack and other things might be dockable there). Plugins still maintain their floating nature due to rendering limitations.

## Sound
*Ears-only notes: "kick too clicky", "pad needs more air", "this preset slaps".*
// Director's note: These don't belong to Lyra. They would be incumbent to the plugins and synths dev, which could be also us, but not Lyra's team

- [ ]

## Questions for Claude

- [ ]

---

## Noticed by Claude
*My own small to-dos, kept here so they don't get lost.*

- [ ] Remember sizes / zoom between runs (needs settings saving, comes with project save/load). Until then, "Copy as code" in F12 and I'll make your numbers the defaults.
- [ ] Wheel-over-note changes velocity only while *hovering* a note. With a selection but no hover, the wheel still scrolls, otherwise you couldn't scroll while anything is selected. Tell me if you want it the other way.

- [ ] Drum clips in the playlist show their notes as one flat line (every drum sits on C4). Draw one lane per drum channel instead.

- [ ] KDE shows a generic "W" icon for Lyra (title bar and taskbar): Wayland takes the icon from a `.desktop` file, which Lyra doesn't have yet. Needs a `lyra.desktop` + icon install (app id `lyra`).
- [ ] *From the manual pass (verified, not urgent):*
  - Changing channels (add, remove, reorder, replace) rebuilds every engine channel, which cuts off any tails still ringing. Make it insert/remove just the one channel.
  - Notes played live from the typing keyboard don't trigger choke groups.
  - If the engine's command FIFO is ever full, a dropped `SetSequence` goes unnoticed (the bridge thinks it was sent).
  - Some undo steps do nothing: wheel-velocity checkpoints on every tick, and any press on the piano-roll grid checkpoints even when nothing changes.
  - The `solo` flag is dead since solo became FL-style mutes. Remove it.
  - `lyra_dsp::params()` builds a whole plugin just to read a static table; the editor does it every frame.

- [ ] *Lyra FM, notes from the port (behaviour kept from Fluttersine for now):*
  - Saw/pulse stop being band-limited while modulated, so some aliasing on bright FM patches.
  - An operator that becomes "used" while a note is held (raising a matrix cell) stays silent until the next note.
  - Mod wheel, pitch bend and sustain pedal aren't wired (Lyra's events don't have them yet). Chorus/delay from Fluttersine presets now belong in mixer effects.
  - The Ratio knob is continuous (FM Bell needs 5.19). The editor could snap to common ratios unless Alt is held.

## Done ✓
*Claude moves finished items here, with the commit that fixed them.*

**Playlist → (hash)**
- [x] Putting Arrenger/Playlist up to date with the visual language. 
  *→ clips are flat keys (dark outline, body tinted with the pattern's light, a flat title strip with dark ink); track headers are plates with round lights, and mute with press-and-sweep; the overview and vertical bars are a recessed well with a flat thumb (also in the piano roll).*

**Scrollbars → `19c4d0c`**
- [x] hover over horizontal scroll bar should zoom. That zoom is better to have its origin at the viewport's left edge rather than the cursor's location
  *→ wheel over the overview bar zooms, keeping the view's left edge still (piano roll and playlist). Ctrl+wheel over the grid still zooms around the mouse.*
- [x] Missing left virtical scroll bars, same style. These would immediatly make it also Touch friendly.
  *→ a vertical bar in the same style beside the grid in the piano roll and the playlist: drag the thumb to scroll, drag its ends to zoom the rows, click the track to jump, wheel over it to zoom (top row stays). I put it on the **right**, per your older wish below; if you meant the left, it's one line to move.*
- [x] Piano roll: a vertical scrollbar on the right (can be thinner than the top one), great on a touchpad.
  *→ same bar, 60 % of the overview's thickness.*
- [x] The piano roll still has its own copy of the overview bar and ruler; move it onto the shared `timeline` module the playlist uses.
  *→ done on the way: one overview, one vertical bar, one ruler for both editors.*

**Journal batch → `298b3d5`**
- [x] Swap UI PAT/SONG but not their colors. SONG/PAT, SONG is green. Clicking either toggles
  *→ SONG (green) | PAT (amber); clicking either face flips the mode.*
- [x] 1, 2, 3, 4 LEDs are unnecessary, could be a simple flashing LED with different colors, potentially also following the song's measure in the future. They use space right now.
  *→ one light: flashes on every beat, gold on the downbeat. Following the song's measure comes with time signatures.*
- [x] Slightly stronger inset shadow on the LCD views.
  *→ sorry: I claimed this last round, but that edit silently didn't apply. Now really in: a 3-px shadow, fading. My edit scripts now fail loudly when a change doesn't land.*
- [x] left edge sahdow on the current channel mixer editor so they not merge witht the mixer strips
  *→ the effect rack casts a soft shadow onto the strips.*
- [x] Dropdown text be soft white/light gray (text color) 
  *→ the pattern name is text-coloured now. (Other dropdowns already were.)*
- [x] Piano octave offset window be Text white too rather than green
  *→ the playable range is light grey now.*
- [x] Most if not all rack'ed toggles should support the mouse drag to turn on/off all on hover. This is a good time to generalize that architecture in the UI-building libraries.
  *→ `widgets::sweep`: press a toggle, keep the button down, sweep over its neighbours. One gesture = one undo step. Used by the rack's mute lights, the mixer's mute lights and the effect bypass lights.*
- [x] Channel module to mixer mapping. A simple number selector to send a synth to a mixer chan.
  *→ a number well after each instrument's name: drag or scroll (M = master, double-click resets).*
- [x] Channel rack should be called something else, Mixer has channels, the channel rack has plugins/synths/instruments...
  *→ "Instrument rack" (window title, F6 hint, "+ Instrument", README). The code still says `rack`.*
- [x] Small border in the bottom edge of the toolbar to make it more visible
  *→ a dark 2-px rule along the toolbar's bottom edge.*

**Mixer & toolbar round 3 → `a46fefa`**
- [x] *(chat)* Mixer window: resizing it shrank it, then it slowly grew back.
  *→ the strips asked for "the window's width minus 340 px", but the panel beside them was a few px wider than that, so every frame the window grew a little to fit. The widths now add up exactly; tested shrinking and growing with synthetic drags.*
- [x] *(chat)* Effects as a rack: predictable slots that open each effect, like channel rack → instrument.
  *→ each mixer track shows 8 fixed slots in signal order. Click: open the effect window · empty slot: plug one in · drag: move · right-click: replace / move / remove · light: bypass. The open effect follows its slot when you move it.*
- [x] *(chat)* PAT / SONG looked like two keys with bad spacing → one radio key, square where the faces meet.

**Control library → `0e5a746`, `65905c9`**
- [x] Less labelling, more implicit design: the TRANSPORT / TEMPO captions aren't needed (the transport already reads as a little cassette deck). Tempo as a bigger, clock-like number ("124.0 BPM" in big letters); no LED imitation, at least not at first.
  *→ all toolbar captions are gone (hover anything for its name). Tempo is the backlit LCD from your studies, since they came later than this wish.*
- [x] Number selectors you grab and drag up/down *and* wheel, LCD-style: hover a digit (tens, ones, the decimal) to choose how fine you tune.
  *→ the tempo LCD: the hovered digit gets underlined; drag or scroll changes that digit; double-click to type. Reusable as `widgets::lcd_number` for other numbers.*
- [x] Pattern selector: change pattern with the wheel. The separate ◀ ▶ arrows aren't that useful (far apart, bad for touch); keep them for now.
  *→ scroll over the pattern name; click it for the list. Arrows kept.*
- [x] *(your HTML studies)* Silicone keys in eight lights, round LEDs, LCDs, wells, three knob families, the channel strip → steps, toolbar, rack, mixer.

- [x] bug? Window won't actaully restore after fullscreen → `50dcf29`
  *→ yes, a bug: full screen overwrote the window's remembered size. Now each mode keeps its own.*

- [x] *(Claude)* No save/load: anything made was lost on quit → `dfe9b12`
- [x] *(manual pass)* Ruler click that switched PAT/SONG landed at bar 1 instead of where you clicked → `d704b68`
- [x] *(manual pass)* Copy/paste/quantize vanished while the playlist had focus → `d704b68`

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
