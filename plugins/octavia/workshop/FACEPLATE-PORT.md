# Faceplate port: notes between the GUI and the engine

The faceplate session (session 3 of `HANDOFF.md`) writes here; the engine
session reads it, and answers under an item (or tells Rhodexa). Kept apart
from `JOURNAL.md` so the two sessions never edit the same file at once.

## Where the port is (2026-11-25)

First version done: `editor`, registered in Lyra
(`plugin_host::EDITORS`). Side by side with `faceplate.html` it matches the
layout, palette, fonts and every LCD panel. It reads only parameters (through
`lyra_kit::Host`) and `octavia_lyra::Telemetry`; it never touches the engine.

* `look.rs`: the palette (from `colors.css`), fonts, buttons, knobs, faders.
* `lcd.rs`: the screen, one function per function of the HTML.
* `lib.rs`: the plate's layout (the HTML's CSS arithmetic) and the wiring.
* Logo: read straight from `docs/design/logo.palette.png` (built in, and
  live-reloaded while Lyra runs from source: rerun `recolor.py`, it shows).
* Fonts bundled in `editor/assets/fonts` (OFL): IBM Plex Mono
  700/600 for the plate, Roboto Condensed 700/600 for the LCD.

## Where it differs from the mock, and why

* **NEXT, undecided notes.** A coming note has `decision: None` until 0.12 s
  before it starts, so NEXT shows its pitch and the string from
  `NoteInfo::intent`, without bow mark or HOW. Once decided, it shows all
  three like the mock. **For the engine:** when bow planning lands, adding the
  planned bow (`down`) and, if planned, how (`Played`) to `Intent` lights them
  up here with no other change: tell me the field names.
* **LOG's TIME** is `NoteInfo::start`, the engine's own clock (seconds since
  it was made), not the song position. Fine for reading gaps between notes.
  The kit gives editors no transport (see "For Lyra").
* **▶ PLAY** is lit while something sounds or is coming (a string not silent,
  loudness, or a note in `upcoming`): there's no transport in the kit. The
  same test dims the screen when idle, as the mock's pause did.
* **WORKLOAD** is full at `workload = 0.5`, the mock's `load / 50`. **For the
  engine:** if 0..1 is meant to be used end to end, say so and I'll drop the ×2.
* **Slide trail**: drawn from `sliding_from` to the finger while the engine
  says it slides, then fades over 0.5 s on the plate's side.
* **The hand bracket** glides toward `Snapshot::hand` (the mock's easing, 14/s).
* **Knobs** turn with Lyra's shared behaviour (`lyra_kit::knob_turn`: drag,
  Shift fine, wheel 2 %, double-click resets) so hands learn one knob in
  Lyra. **Faders** keep FACEPLATE.md's: the cap follows the mouse, wheel 5 %
  (Shift 1 %), double-click resets. One undo step per drag either way.
* **Player buttons** load the presets' four skills in one undo step; the
  LCD names the profile the faders match, else CUSTOM.

## Rhodexa's changes after the port (2026-11-25; these win over the mock)

* **The screen never dims.** Idle draws at full ink, like playing.
* **Solid meter bars**, not blocks: at 0.68 the blocks beat against the
  pixels. The tuner is a mid span (the vibrato) with a dark needle.
* **Fingers: one mark**, a filled square wherever a finger is down, played
  or only held. No hollow squares, no tick at the nut for open strings (a
  dark string with no finger is played open).
* **TOO LOW / TOO HIGH**: printed words with arrows under the neck, lit while
  a sounding note is below the lowest open string (either note of a double
  stop) or a finger/note is past the neck's two octaves.
* Sharper: rectangles land on whole pixels; the logo is decoded at about
  twice its size on the plate (it was scaled down 5× by the GPU and shimmered).

**For the engine:** a note below the instrument is common (cello parts on
the violin, MIDI bass lines). The screen now says so; what the engine plays
for it is yours to decide (today the violin drops D3 from an A3+D3 chord).

## For Lyra (not this crate's to change)

* An editor can't see the transport (playing, song position). With
  `Host::playing()` / `Host::position()` the LOG could print song time and
  ▶ PLAY could mean the transport.
* Lyra's generic preset dropdown is now hidden for every hand-made editor
  (it was hidden for Fluttersine and Chrysalis by name). For Octavia that
  means no "save your own preset" yet: the plate has no place for it.

## Housekeeping

* `octavia-editor` is a member of this workspace, so building here compiles
  egui (pinned `=0.31.1`, Lyra's version) and `Cargo.lock` lists it.
