# Done

Everything finished from JOURNAL.md, with the commit that did it, newest
first. Moved out of the journal (2026-11-25) so it stays short.

**One repo (2026-11-27)**
- [x] *(Rhodexa)* Should Octavia and the plugins share one repo, and how should they be laid out? *→ yes: Octavia joined Lyra with its whole history (e567673); every plugin is `plugins/<name>/` with `plugin/` (what Lyra loads), optional `editor/`, and Octavia's extra `core/` + `render/`; layout and rules in `plugins/README.md`; Lyra now builds from a fresh clone; `cargo octavia …` runs the renderer (71e8e2e, 6fa97c5). Octavia's refs baselines were already stale before the move (6 of 8 differ from the pre-move build too; the move itself renders byte-identical).*

**Octavia in Lyra (2026-11-25)**
- [x] *(Octavia's handoff, session 3)* The faceplate port. *→ `octavia-editor` (in Octavia's repo, commit 30c7bd7) is her editor: the plate, its controls and the live performer screen from her telemetry; Lyra lists it in one line, and the generic preset dropdown hides for every hand-made editor (251f15f). Differences from the mock: Octavia's `docs/FACEPLATE-PORT.md`.*
- [x] *(Octavia's handoff, docs/KIT-REQUIREMENTS.md part A)* Upcoming notes, latency, note ids. *→ note events carry an id (CLAP's `note_id`; live notes `NO_ID`); the `lyra.note-lookahead` extension hands a plugin the next N seconds of the song every block, ids matching the note-ons, round the loop, empty while stopped; `Plugin::latency` (`clap.latency`, not compensated yet). Octavia is listed by path from `../LyraRelated/Octavia` (7f89a15).*

**Plugin kit (2026-11-25)**
- [x] *(Octavia's handoff, docs/KIT-REQUIREMENTS.md part B)* A minimal plugin kit; synth-specific code out of `plugin_look.rs`; editors able to live in their own crates. *→ `lyra-kit` (Host, Editor, behaviour only); Fluttersine and Chrysalis editors in `crates/plugins/*/editor` with their logos; their shared frozen look in `crates/plugins/shared-look`, its style now code (d5cc71b).*

**UI port (2026-11-25)**
- [x] Stretching viewport bar beyond used space enters a stretching loop that grows way too fast. *→ the bar's scale is frozen while you drag; it settles on release. The vertical bar had the same loop (5665a2e).*
- [x] Menus as LCD drawers, and the key root/scale pickers. *→ FILE, VIEW and + Instrument open the dropdowns' drawer; root and scale are header dropdowns (6e18428).*
- [x] Octave buttons and FILE should react across the toolbar's full height. *→ the octave keys do; FILE is as tall as the toolbar's keys (707a4ef).*
- [x] The gap under editor headers; the audio-clip editor's tabs had no top edge. *→ the areas' 8 px padding is gone, editors run edge to edge, EDITOR / RECORDER are tabs again (6119d98).*
- [x] Alignment in the audio-clip editor's transport row and the Recorder plate. *→ one left edge, one width (73f38b9).*
- [x] Tooltip prevents drawers from opening. *→ a menu key's hint sat on top of its own drawer; no hints while a drawer is open now (f82834a). If a drawer still won't open for you, tell me which one.*
- [x] Viewport bar's content miniature overshadows the thumb. *→ the miniature is drawn under the window, and the window is see-through (9d08a6c).*
- [x] Number pickers were missing their digit padding (the big click targets rule). *→ every LCD character in its own cell, the tuned digit a full-height inverse cell (39198f3).*
- [x] All editors were missing the bottom border on their headers. *→ an engraved line in the ground colour, edge to edge (d8f98c0).*
- [x] Mixer inserts could have their index on top in the middle or as a tag. *→ a tag hanging from each strip's top centre (d8f98c0).*
- [x] The mixer's vertical names were cut off at the bottom ("ASTER", "NSERT 1"). *→ the strips fit the height they're given; names and fx counts whole again (92dc64a).*
- [x] ! The vertical viewport bar must be **exactly as wide as the horizontal overview bar is tall**. *→ both 34 px now, the thumb a see-through window with 14 px zoom handles (02a2f6c).*

**Toolbar and panels (2026-11-23)**
- [x] ~ Once the desktop icon works, drop the logo from the toolbar. *→ gone; a File menu sits there (b577fe3).*
- [x] ~ The top bar's buttons for opening sections came from FL and are now unneeded and confusing… *→ three layout keys (left panel, bottom area, right panel) like a code editor's, `F4` `F5` `F6`; the folded bottom area leaves a strip with an arrow, like the side panels (1bdfe9b).*

**Small visible things (2026-11-22)**
- [x] vertical scroll bar could be slightly wider and zoom on the center
  *→ a little wider in the arrangement and the piano roll (one shared width now); the wheel over it zooms around the middle (645ba6a).*
- [x] Drum clips showed their notes as one flat line. *→ an instrument that only plays one key in a pattern gets its own lane, in rack order, under the melody; the Patterns tab draws the same way (645ba6a).*
- [x] The ↕ gain grip could be off screen on a wide clip. *→ it sits in the middle of the clip's visible part, in the arrangement and the audio-clip editor (645ba6a).*

**Engine robustness (2026-11-22)**
- [x] Changing channels (add, remove, reorder, replace) rebuilt every engine channel, cutting off tails.
  *→ the engine keeps the channels it still has and only makes new ones for new or swapped instruments; the rest ring on (7cd5c85).*
- [x] Notes played live from the typing keyboard didn't trigger choke groups. *→ they do (7cd5c85).*
- [x] A full command FIFO dropped commands silently. *→ they wait in a backlog and go out in order (7cd5c85).*

**Files tab (2026-11-22)**
- [x] bug, pattern dropdown outgrows the screen and later patterns can be reached.
  *→ the dropdown is gone: patterns live in their own tab of the right panel, which scrolls (a82f65a).*

**Chrysalis's table studio (2026-11-21)**
- [x] *(chat)* "It's very obviously missing its waveform editor"
  *→ a TABLE page inside Chrysalis (the WAVE EDITOR key at the right of the header, which turns into BACK TO SYNTH; never another window, per your plea), opened by EDIT on either oscillator with a copy of the table it plays. PLAY ON picks the oscillator that gets it; the other can choose it too ("Imported"). In it: draw a frame freehand, set its first 64 harmonics as bars, add / delete frames, MORPH everything between the first and the last frame (crossfade or spectral), resize to 8–256 frames, normalize, smooth, start over from a sine / saw / square / what's playing, and its own undo. Your table is a WAV in `~/.local/share/lyra/tables/`; with LIVE on, every stroke saves and the oscillator plays it at once (through the import, so a channel still has one imported table, shared by both oscillators).*

**Automation on the knobs, turquoise, your presets, 17 new sounds (2026-11-20)**
- [x] *(chat)* "Are the knobs moving with automation clips the synthesizer's concern?"
  *→ no: Lyra's (the host knows the curves and the playhead, the same way a CLAP host tells a plugin's GUI). While the song plays, every editor's knobs follow their automation: Chrysalis (its pictures too), Fluttersine, the plain editor, effect windows. The mixer faders don't yet.* (`d64ab09`)
- [x] *(chat)* "Her mane is turquoise… lighter field, softer colours, more turquoise than green", and the wing holes somewhere
  *→ turquoise is Chrysalis's hero colour now (osc A, sub/noise, the voice, the nameplate); the banner's green, softened, is osc B; everything a little lighter and softer. The readout shows three of her wing holes while you aren't touching anything.* (`f9429e1`)
- [x] *(chat)* "A way to store new presets"
  *→ every instrument's preset list has YOURS and a Save field. Each is a small file in `~/.local/share/lyra/presets/<plugin>/`; × deletes it.* (`e254720`)
- [x] *(chat)* Stronger growls, glitch hop, piano, pads, MDK / TheFatRat-style leads
  *→ 17 new Chrysalis presets, all presets now named by kind: Bass · Monster growl, Metal growl, Yoi, Wub, Glitch, Neuro reese; Lead · Hero, Chip, Mega saw; Pluck · Arp, Glitch stab; Keys · Electric, Synth piano, Bells; Pad · Warm, Glass; FX · Riser. Levelled by measurement; please listen.*

**Arrangement tools → `8821c56`**
- [x] Slide pattern clip window
  *→ the Slide tool (a key in the arrangement header): drag what plays inside a clip, it stays put. Patterns wrap around; audio stops at its ends. Works on several selected clips at once.*
- [x] Trim pattern start per clip
  *→ drag a pattern clip's left end, like audio clips: what plays stays where it is.*
- [x] Cutter/Slide tool, Cut pattern at cursor possition.
  *→ the Cut tool: click a clip to split it at the mouse (snapped; Alt = free), with a preview line. Patterns or audio; the selected clips too if you click one of them.*
- [x] Per audio clip reversal toggle (Audio clip editor) Reverses the window playback
  *→ Reverse, under the clip's knobs in the audio-clip editor. The fades stay at the clip's edges on the arrangement (so in the editor the fade-in sits at the frame's right end, where reversed playback starts).*

**Chrysalis: random LFOs, phrase mode, warp you can see (2026-11-19)**
- [x] *(chat)* "A free mode for the LFO that only runs when triggered", and "random/stepped noise on the LFO"
  *→ a new LFO mode, PHRASE: one LFO for all notes that only runs while notes sound and starts over on the first note after silence. And a style for every LFO: DRAWN (your shape), STEPS (a new random value each cycle) or SMOOTH (random values glided between); the random ones show a scrolling trace of what they're doing.*
- [x] *(chat)* "The warp doesn't have animations"
  *→ the table view bends with the warp as the matrix moves it, and FM finally has a picture: oscillator A's wave bent by B's (at B's pitch).*
- [x] *(chat)* An overlap at the preset label
  *→ the readout's text is clipped to its box (a long name ran into the preset's ‹ button) and the preset name stops before its chevron. Tell me if it was somewhere else.*
- [x] *(chat)* The screenshots look lighter and softer than the real window, and that's nicer
  *→ Chrysalis's darkest tones lifted a touch. (A screenshot is Lyra's raw image; your screen shows it through its colour handling, so the real window can look a bit darker.)*

**Chrysalis comes alive (2026-11-19)**
- [x] *(chat)* "It's only missing animations": the playing frame, LFO playheads, envelope dots, modulation on the knobs, screens that breathe
  *→ all of them. A plugin can now share what it's doing with its editor (`Plugin::telemetry`: atomics the audio thread writes at the end of each block; the bridge hands it to the editor). While notes sound, the table view's bold curve is the frame that really plays (your setting stays as a faint line), each LFO has a glowing playhead and a dot on its curve, a dot rides the envelope, modulated knobs show a lighter arc and dot where the matrix has them right now (the pointer stays where you set it), and the screens' backlight rises with the output. The editor only redraws continuously while something moves.*
- [x] *(chat)* Readout and preset picker side by side looked odd; the bottom keys didn't feel like tabs ("not flat on the bottom and round on top!"); drag the amount bars up/down too
  *→ readout on the left, the picker on the right with "PRESET" inside it. Each tab group is one recess (a view switcher), the open tab solid with a small notch pointing down into what it shows. Amount bars follow up/down drags as well as sideways.*

**Chrysalis, third pass: layout that fits (2026-11-19)**
- [x] *(chat)* Overlap in the filters, too much text under the knobs, tiny header knobs too high, empty space in SUB·NOISE, mod tabs that look like text, thin bars, too much space between routes, and "is there a flexbox?"
  *→ there wasn't: now `flex.rs` lays out a panel by arithmetic (fixed and growing parts), and every Chrysalis panel uses it. Knobs show only their names; hovered or turned, the name becomes the value, and a readout beside the preset shows the full name and value. SUB and NOISE split their panel in half with full-width keys, the filter rows can't overlap, the macros are smaller, the mod tabs are keys, the matrix rows are taller with thick amount bars and tight gaps.*
- [x] *(chat)* "That scroll bar is awful, too thin, changes size, a different colour"
  *→ egui's default style everywhere in Lyra: replaced once, app-wide, by solid always-visible bars in Lyra's well-and-thumb colours.*

**Chrysalis, second pass on its face (2026-11-19)**
- [x] *(chat)* Too many light, Android/Qt-style borders; the matrix crowded and tiny; toggles as dots; empty space in SUB·NOISE and the LFO side; header knobs off-centre
  *→ no light outlines anywhere: edges are darker than what they edge (ink, not black). Anything chosen or on is solid colour with darker, bolder letters (tabs, switches, keys). The panels' names are their on/off keys (OSC A, SUB, NOISE, FILTER 1/2). Dropdowns are dark fields with no border. The matrix lists only the routes in use, in bigger rows with a wide glowing amount bar (drag sideways, double-click for 0), an × to remove, and "+ route" to add.*

**Chrysalis gets its own face (2026-11-19)**
- [x] *(chat)* "The logo is way too cool while the UI is way too quiet and the sound is super aggressive"; the emblem "looks like a sticker"
  *→ a new editor with Chrysalis's own look, from your banner: teal-black, dark screens with glowing curves, neon knobs; green and cyan for the sound, violet for everything that moves it. One page instead of seven tabs: both oscillators side by side, then sub/noise, drive and filters, then modulation (ENV / LFO / MATRIX, all 16 slots at once) beside the macros. The emblem is a badge in the header that breaks out of its bar, with its glow trailing along it.*

**Chrysalis, the wavetable synth (2026-11-19)**
- [x] *(spec)* `docs/specs/chrysalis.md`
  *→ built. Two wavetable oscillators (nine built-in tables, or load your own WAV), each with position, seven warps and up to 16 unison voices; a sub and a noise; per-voice drive; two filters (low/high/band/notch, a formant filter that sings A-E-I-O-U, combs); four envelopes with bendable curves; four LFOs you draw yourself; 16 mod slots (any source → anything, scaled "via" a second source); four macros. Seven presets: Growl, Talking bass, Reese, Pluck, Supersaw lead, Evolving pad, and Init. Its editor has the table as a stack of frames (drag it to move through it), and your emblem lives in the corner. Demo: `cargo run --release -p lyra-app --example chrysalis -- <dir>`, then `lyra --render "<dir>/Metamorphosis.lyra"`.*
  *Please listen: the presets and the demo were levelled by measurement, not by ear.*

**Fluttersine in stereo, and velocity you can hear (2026-11-19)**
- [x] *(chat)* "Shouldn't it just generate stereo? The OUT column should have left/right knobs"
  *→ yes, it was mono to the end (one signal copied to both sides). Now there's a PAN column beside OUT in the matrix, a Width knob that spreads a supersaw's seven saws left and right, and Pan as a MOD destination (auto-pan). A patch that stays centred still renders as mono, so it costs nothing extra.*
- [x] *(chat)* "The volume of the notes doesn't affect the synth's volume at all"
  *→ in Fluttersine it barely did: each operator's Velocity defaulted to 0.5, so the softest note was only 6 dB down. Now the voice plate has a Velocity knob (how much velocity sets the loudness, full by default, like Lyra Synth and the drums): a quarter velocity is 12 dB quieter. The per-operator Velocity stays as extra (on a modulator it's brightness) and defaults to 0.*

**Faders that feel like faders (2026-11-19)**
- [x] *(chat)* "The mixer doesn't behave too well", and Fluttersine's Volume said "100%" for −4.4 dB
  *→ it wasn't you: the mixer faders, the rack's volume knobs and the master knob moved linearly in gain, so everything you mix with (−20 to 0 dB) was squeezed into the top third. Now all of them use one console-style taper (`Curve::Fader`): unity at about 79 % of the travel, +6 dB at the top, and they read in dB. Fluttersine's Volume is a plain gain now, shown in dB (same sound: the reel renders bit-identical).*

**After writing two test songs in Lyra (2026-11-19) → `18381f6`, `5fdf189`, `13fc580`**
- [x] *(Noticed by Claude)* Lyra couldn't export audio; my song scripts copied the bridge to get a WAV
  *→ Export WAV / Export stems in the project menu (Ctrl+E / Ctrl+Shift+E), and `lyra --render song.lyra` from a terminal. Same bridge and engine as playback, through the tails.* (`13fc580`)
- [x] *(Noticed by Claude)* No limiter: a couple of clipped samples meant turning the whole master down
  *→ Limiter (look-ahead brickwall), plus Chorus and Drive (soft / hard / fold / bit-crush).* (`5fdf189`)
- [x] *(chat)* "The wobble is too early-dubstep", "Fluttersine's matrix isn't complete"
  *→ Fluttersine's new MOD page: two mod envelopes and six slots (LFOs, envelopes, velocity, key, random → pitch, cutoff, volume, FM depth, or any operator's level / FM in / pitch / warp), a second LFO, and tempo sync for both (locked to the song while it plays). An envelope on Pitch is a pitch drop; an LFO on "Op 1 FM in" is a growl that changes vowel, not just brightness.* (`18381f6`)
- [x] *(chat)* "The arps are iffy"
  *→ They restarted the envelope on every 36 ms note. In the test song they're now one mono legato voice stepping through the chord (no restart). The real home for this is the chip synth's chord mode: see `docs/specs/chip-synth.md`.*

**Cleanup pass → `d742c6f`**
- [x] *(Noticed by Claude)* Some undo steps do nothing
  *→ a checkpoint identical to the last one is skipped, and undo skips past a snapshot equal to the project.*
- [x] *(Noticed by Claude)* The `solo` flag is dead
  *→ removed; an old file with a soloed channel opens as mutes on the others.*
- [x] *(Noticed by Claude)* `lyra_dsp::params()` builds a whole plugin
  *→ each plugin's table is listed in the catalog (tested against the plugin).*

**Audio clips + journal → `0c60fc3`**
- [x] *(chat)* Audio clips, instanced like patterns, non-destructive, with fade in/out you can see and bend (tension), for the voice-acting work
  *→ drop a sample on a track. Gain, fades and trim are shared by every copy (Ctrl+U gives the selected copies their own); each copy's own ends trim it. Fades: the little squares on the clip's title strip (sideways) and the dot on the curve (up/down), or the knobs in the new Audio clip editor (double-click a clip). Each track's number at the right of its header picks the mixer insert its audio plays into.*
- [x] *(chat)* Clips a bit too see-through, more contrast
  *→ clip bodies are more solid now (the current one a bit more).*
- [x] bug? ? I couldn't get double-clicking patterns to work reliably. Occasionally it opens, but its like 30% of the time. Tried with touch pad, mouse and touchpad button.
  *→ a real bug: Lyra asked egui "was this a double-click?" on the frame the button went down, but egui only knows on the frame it comes back up. So it worked only when your click was quick enough to fit in one frame. Now Lyra times the presses itself. (Same fix covers audio clips.)*
- [x] bug: Tab works but only when the window is in focus, which is the correct behaviour, however after tab switches editor the area looses focus and tab doesnt work anymore until you click on it. Focus shoudln't require interation, instead areas alone should make sure they are in focus through hover.
  *→ egui uses Tab to move keyboard focus between widgets, and it did that before Lyra saw the key: focus landed on some widget, and once that was a text field, every key went there. Lyra now takes Tab away before egui sees it. Tested: four Tabs in a row flip back and forth every time.*
- [x] ~ tapping STOP when already stop to return playhead to beginning. Beginning of song in song mode, beginning of pattern in pat mode.
  *→ yes: Stop while stopped goes to the start (song or pattern, whichever mode you're in).*

**Journal batch → `3496244`**
- [x] Draging a note or group of notes or patterns or anything outside the viewport should slowly pan the viewport, that way dragging say a note to an octave higher would be just about holding the note at the top edge and letting the viewport pan
  *→ in the piano roll and the arrangement, for moving, resizing and rubber-band selecting. It starts a few points inside the edge and speeds up the further past it you hold. Tested: A2 held at the top edge ended up on A#4.*
- [x] ~ Now: Mixer panel oresizes the bottom split, it would be nice if either the mixer _was_ resizable _or_ just as usefully: Keep track of a desired height by the area, so when switching back to a piano roll or other editors they would take their size back
  *→ the second option: each editor remembers its own height in the bottom area. The mixer opens at its natural height; the piano roll gets its size back.*
- [x] Double-click a pattern should open its piano roll on the main panel
  *→ in the browser or on a clip: the top area switches to the piano roll for that pattern (on the selected instrument if it plays in it, otherwise the first one that does). Tab takes you back.*
- [x] Tab switches the editor between the last two editors used? (within an area)
  *→ yes, the area under the mouse.*
- [x] ~ Changing cursor position _within piano roll_ when in _song mode_ should _not_ switch to pattern mode.
  *→ it stays in song mode and jumps to that spot in the clip of this pattern that's playing (or its first clip). Only a pattern with no clip in the song still switches to pattern mode.*

**Crash + journal → `e8f2956`**
- [x] *(chat)* The sound engine crashed while adding a reverb: `index out of bounds: the len is 192000 but the index is 192000` (effects.rs:370)
  *→ the Delay, not the reverb: a float rounding wrapped its read position to one past the end of its 4-second buffer. Fixed, with a test that reproduces it. And now any bug inside the audio thread is caught: Lyra restarts the sound engine by itself and tells you.*
- [x] ? bug. Notes that start before the end of the pattern but end after it don't extend the pattern's length
  *→ a real bug, or rather a choice of mine that was wrong: the length counted where notes start, not where they end. Now a note crossing the bar line adds the bar (ending exactly on the line doesn't).*
- [x] Note strips be slightly trasnlucent to see when they step on eachother, or hide themselves. Other that,
  *→ notes are slightly see-through now. (Your note ends with "Other that," — I think the rest of the sentence got lost.)*

**Docked layout → `0cbc1e5`**
- [x] ~ Docked layout (Blender / VS Code style areas that don't overlap) for the main views; it would also save the title bars (~5rem). Floating windows stay for plugins.
- [x] ~ Start building docked windows for essential panels. Eg:
Browser | Arranger/PainoRoll (Swappable or on top of eachother with easy buttons/hotkeys) | Inspector (Potentially Channel rack and other things might be dockable there). Plugins still maintain their floating nature due to rendering limitations.
  *→ browser docked left (F4, folds to a strip), arranger docked in the centre, piano roll docked under it on a draggable split (or alone via its full-screen button). Rack, mixer, instrument and effect windows stay floating. The right-hand panel waits until something earns it (ideas: the mixer's effect rack, the selected instrument's main controls, a sample editor for voice lines).*

**Playlist → `e16fe3a`**
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
