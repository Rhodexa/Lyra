# Spec: the piano roll, where it goes next

**Status (2026-11-26):** steps 1 and 2 are built (see below); 3 to 6 are
ideas we've agreed on, not designs yet. Each one gets designed in its own
session, with Rhodexa, before any code.

Where this came from: the roll's mouse handling has been good since M2, but
it got stuck because a note only knows `start, len, key, velocity`, and the
roll gave no help with the music itself. Rhodexa writes by ear and by
intervals, not music theory ("A means nothing to me but the base of a very
clever exponent"), so anything that helps should speak in **semitone
distances and in sound**, never in chord names or scale degrees.

## Built

1. **Small things** (da47139)
   - The velocity lane's seam drags (the splitter grip shows on it).
   - Velocity strokes reach every note they cross; **Shift+drag** draws a
     straight line and the notes under it follow it.
   - **Un-double** (context bar / F3): notes at the same moment that are
     the same note or octaves apart, keep the lowest or the highest. For
     MIDI files that double everything in octaves.
   - Interval tags (a thread and `+5` while dragging) were tried and taken
     out again: "fun, but more noise than tool".
2. **Typing: the roll as a text editor** (an experiment, `piano_roll/typing.rs`;
   keys listed at the top of that file). Enter starts it, Esc ends it.
   Why: Rhodexa is fast with a mouse, but on a laptop the touchpad is "a pain
   for anything music"; and they type code all day (Home/End, Ctrl+arrows,
   Shift-select). So keyboard editing borrows a code editor's grammar.
   How it got here, the same day: a tracker-style step input (5e71292) →
   hold-to-grow (intuitive, but you over/undershoot and wait a long time for
   long notes) → this: a caret, the typed notes stay selected so
   Alt+← → fixes their length right away, and that length carries to the
   next notes (like clicking a note with the mouse), so a chord progression
   is chord, chord, chord. Holding still grows notes, for feel.
   **Verdict (2026-11-26): it works.** "Really fun to use", "definitely
   better than the touchpad", not perfect yet; Rhodexa will live with it and
   collect its little bugs. **Parked as is** (not for lack of interest: to
   try other experiments). Known rough edges: ↑↓ moves notes silently;
   Space plays from the playhead, not the caret.

   **Typing, next round (Rhodexa's ideas, for later):** grow typing into
   *full keyboard editing*, Blender-style.
   - **A 2D cursor**: a caret with a pitch too (a cell, not just a line), moved
     with the arrows through steps and keys. **Shift+arrows grow a box select**
     from it, so you pick notes in context, anywhere, without the mouse.
   - On what's selected: Alt+← → length (as now), ↑↓ / Ctrl+↑↓ pitch, another
     combination for **velocity**.
   - **Deselect all like Blender**: Alt+A, or A twice (A isn't a note key).
   - **F3 as a command palette**: type to filter (un-double, duplicate, quantize,
     toggles like a future harmony shadow), Enter runs it. Everything in the
     context bar reachable by name.
   - Rhodexa: "it's a lot of keyboard mapping, but I really like the
     concept." Design the keymap as a whole before building (the code-editor
     and Blender habits must not collide with the note keys).

## Next

3. **Links between notes: built as a prototype (2026-11-27), try it.**
   Hover a note: a dot past its right end; drag it onto a later note. The
   link is drawn as the pitch's own path (along the first row, then a curve
   into the second note); its **dot** sets when the glide starts (all the way
   right = no glide, legato), its **ring** bends it (the automation editor's
   curve, double-click = straight), right-click unlinks. Only **Chrysalis**
   glides so far (`Plugin::glides`, `Event::Glide`); other synths play linked
   notes as written. Code: `piano_roll/links.rs`, `bridge::chains`, the
   engine's `Legato` / `SeqGlide`, `NoteLink` in the model.
   *The original idea, for reference:* a grip on a note's right end; pull it onto the
   next note and the two are linked. A link has a **type**:
   - *slide*: the pitch glides (Chrysalis, Fluttersine);
   - *slur*: connected, no new attack (Octavia: same bow; synths: legato).

   A slide's **tension** bends its curve, using the automation editor's
   bend ring (drag it up or down), so there's nothing new to learn. Each synth
   reads a link its own way. Rhodexa's first picture was "something you pull
   to the next note and choose the tension"; they're not sure yet, so
   **prototype it and let them play with it before settling.**
   Needs: `Note` gets a link (the model can change freely, pre-stable),
   the engine sends it with the note (Octavia's KIT-REQUIREMENTS asks for
   per-note data: "tremolo here, accent there, slur these").
4. **Channel curves in the bottom lane**: mod wheel, expression, pitch bend,
   drawn like automation (points + bend rings), under the notes. The lane
   gets tabs (`tab_row`): VEL, then the curves. Octavia needs these the most
   ("one of the most demanding synths for mod wheels"); Fluttersine's mod
   wheel / bend / sustain aren't wired yet either (NOTES.md).
5. **Per-note curves** (Rhodexa's "notes as sub-splines"): a small curve
   inside a note, its time measured from the note's start, so it moves with
   the note. What CLAP calls note expressions. The most expressive option,
   but every synth must support it: after 3 and 4.
6. **Drum lanes**: a second view of the roll (a tab, not a window): one row
   per drum channel of the pattern, the roll's mouse and free lengths.
   Rhodexa writes beats in the roll, and each drum being its own channel
   means switching channels all the time.

## Smaller ideas, not scheduled

- Harmony shadow: tint the rows that blend with what the other channels play
  under the mouse. No names, just "these sound right here".
- Selection actions for the context bar: legato (stretch to the next note),
  chop, glue, strum, humanize, flip, fit to key, arpeggiate.
- A Paint tool tab (drag lays notes along the snap: hi-hats, basslines).
- Record from the typing keyboard or MIDI in real time, raw timing kept,
  quantised on output only.
