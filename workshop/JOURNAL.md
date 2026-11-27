# Lyra journal

Your scratchpad for wishes and friction. Write it however it comes out:
half sentences, typos, "ugh" are all fine. Claude reads this before choosing
what to work on and folds the items into the plan when the time is right.

**Little marks (optional):**
`!` it really hurts · `?` not sure if it's me or Lyra · `~` someday, no rush
no marks would default to not urgent or 'coding teams judges'

---

## Friction
*Things that feel wrong, slow, confusing, ugly, or make you stop playing.*
// - [ ] ? bug or missing?
- [ ] bug a pattern not on the timeline or not playing yet still shows a staff playing from the start, patterns clippy on the arranger not from their 0 tick show the staff playing from 0 despite playing further ahead in the arranger. might be related. 
- [ ] No way to delete patterns fast and in mass
- [ ] Lyra always opens the arbitrary folder Music.
- [ ] ! Folder dialogue bolcks main Window, making the system believe it stopped working (resposing) when in reality it is awaiting for the file picker

## Wishes
*Things you wish Lyra did.*
- [ ] *(from chat, 2026-11-23)* Loop regions are drawn with Alt+drag, but Alt also means "off the grid", so a loop can't be fine-tuned. Maybe Shift+drag instead? And a "clear selection" button in the context bar?
- [ ] *(from chat, 2026-11-23, later)* Sit down and make reaching both areas (top and bottom) and their editors easy, now that the toolbar only opens panels.

- [ ] *(from chat, 2026-11-26)* The playhead in Lyra's eye colours, with a head triangle twice as wide (the arranger's and the roll's: they share `timeline`).

## Questions for Claude

- [ ]

---

## Noticed by Claude
*Only what needs you: a question or something to try. Claude's longer notes (known limits, leftovers) are in [NOTES.md](NOTES.md); finished items in [DONE.md](DONE.md).*

- [ ] *(from chat, 2026-11-16)* The instrument window's top bar (name field, "Lyra FM", mixer dropdown) looks out of place above the Fluttersine rack: restyle it in the plate language, later. Also still open: rename "Lyra FM" to Fluttersine (one word in `lyra-dsp`).
- [ ] Remember sizes / zoom between runs? You'd rather Lyra remembered as little as possible (2026-11-22), so this waits until a size really bothers you. "Copy as code" in F12 and I'll make your numbers the defaults.
- [ ] Wheel-over-note changes velocity only while *hovering* a note. With a selection but no hover, the wheel still scrolls, otherwise you couldn't scroll while anything is selected. Tell me if you want it the other way. Re: we'll tackle this later
- [ ] The synths' logos moved with their editors: export Fluttersine's wordmark over `plugins/fluttersine/editor/assets/logo.png` and Chrysalis's emblem over `plugins/chrysalis/editor/assets/emblem.png` (live reload works as before).
- [x] KDE shows a generic "W" icon for Lyra: **Rhodexa, run `cargo run --release -- --install-desktop` once** and tell me if the taskbar shows the logo. Then the logo can leave the toolbar (your wish above).
