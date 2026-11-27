# Hand-off: Octavia into Lyra (stage 5)

Written 2026-11-25, at the end of the faceplate design session. Three
pieces of work, each for its own session, each owning its own files. Read
this, then the files it points to.

**Status (2026-11-25, evening): sessions 1 and 2 are done; 3 has a first
version (`editor`, notes in `docs/FACEPLATE-PORT.md`),
waiting for Rhodexa's look.**
Octavia plays from Lyra's piano roll with the generic editor
(`plugin`). For the porter:

* Telemetry: `octavia_core::telemetry::Snapshot` (plain data). The editor
  gets it with `plugin.telemetry()` → downcast to `octavia_lyra::Telemetry`
  → `.read()`; `.instrument()` says cello or violin, and
  `octavia_core::instrument::{cello, violin}()` give open strings, hand
  span. Fingers solid = `StringSound::Bowed | Plucked`, hollow = `Ringing`.
* NEXT: coming notes have `decision: None` until 0.12 s before they start
  (see the journal's "Noticed").
* No Player parameter: profile buttons load the presets (journal).
* Lyra side: note ids, `Plugin::note_lookahead`/`upcoming`
  (`lyra.note-lookahead`) and `Plugin::latency` live in `lyra-plugin`.
  Lyra doesn't compensate latency; it gives look-ahead instead.

## Where things stand

* **The engine** (stages 1–4) is done and tested: performer, voice,
  streaming with look-ahead, the human layer. `docs/DESIGN.md`,
  `JOURNAL.md`.
* **The faceplate is designed and approved**: `docs/FACEPLATE.md` (what and
  how to port), `docs/design/faceplate.html` (the spec, playable).
* **What Lyra must provide** is written down: `docs/KIT-REQUIREMENTS.md`
  (A: sound side, B: the editor kit, C: Octavia's telemetry, D: order).
* How Lyra draws and how its plugins are split: `docs/INTEGRATION.md`.

## The three sessions

```text
 now, in parallel                         then
 ────────────────                         ────
 1. Lyra: the plugin kit      ─┐
    + upcoming notes, latency  ├──►  3. Octavia: the faceplate port
 2. Octavia: adapter          ─┘        (needs 1's kit and 2's telemetry)
    + telemetry
```

### 1. Lyra session: the plugin kit (repo `../../Lyra`)

Owns: `lyra-plugin`, the kit crate, Lyra's registry and host side. Never
touches Octavia's files.

* KIT-REQUIREMENTS **B** (the kit): editors in their own crates, parameter
  gestures with undo, telemetry downcast, own fonts and assets, repaint
  requests, one-line registration. Move synth-specific code out of
  `plugin_look.rs`.
* KIT-REQUIREMENTS **A**, the missing rows: upcoming notes ~1 s ahead
  (custom extension, `lyra.note-lookahead`), latency reporting, note ids.
* Done when: a plugin crate living in another folder can register an
  editor, draw with its own look, change parameters with undo and read its
  telemetry; and a plugin can receive upcoming notes or report latency.

> Opening prompt: *"Read `../LyraRelated/Octavia/docs/HANDOFF.md` and
> `../LyraRelated/Octavia/docs/KIT-REQUIREMENTS.md`. Build the plugin kit (B)
> and the missing sound-side pieces (A: upcoming notes, latency, note ids).
> Octavia is the first customer; Fluttersine and Chrysalis must look and
> behave exactly as before."*

### 2. Octavia session: the adapter and the telemetry (this repo)

Owns: `octavia-core` and a new adapter crate here (`plugin`,
depending on Lyra's `lyra-plugin` by path), plus the line in Lyra's
`Cargo.toml` that lists it.

* The adapter (`INTEGRATION.md` §3, step 1): the parameter table exactly as
  in `FACEPLATE.md` (ranges, defaults, groups), the four profiles as
  presets, notes in → `Engine::add_note`, audio out ← `Engine::process`.
  Playable with Lyra's generic editor before any faceplate exists.
* Look-ahead: use Lyra's upcoming notes once session 1 lands them; until
  then, report 1 s of latency and delay.
* The telemetry: KIT-REQUIREMENTS **C**, from the engine's own state (the
  mock had to reconstruct finger lifts and held fingers from events; the
  engine knows them).
* Done when: Octavia plays from Lyra's piano roll, its parameters automate,
  and a test reads telemetry that matches `--explain` for a score.

> Opening prompt: *"Read `docs/HANDOFF.md`. Build the Lyra adapter and the
> telemetry (session 2). Keep the engine's test bench green; telemetry must
> agree with `--explain` and the tapes."*

### 3. Faceplate port session (after 1 and 2)

Owns: Octavia's editor crate (here, e.g. `editor`) using the
kit. Draws only; reads parameters and telemetry, never the engine.

* Port `docs/design/faceplate.html` function by function, as
  `FACEPLATE.md` "Porting to egui" describes. The logo is
  `docs/design/logo.palette.png`.
* Done when: side by side with the HTML at the same tape moment
  (`?tape=showcase&t=11.15&live`), the two match; Rhodexa approves.

> Opening prompt: *"Read `docs/HANDOFF.md` and `docs/FACEPLATE.md`. Port the
> faceplate (session 3) with the kit, matching `docs/design/faceplate.html`."*

## Still open (Rhodexa's, no session needed)

* Whether Octavia herself appears on the plate; the gift-bow collar idea
  (`docs/design/bow.palette.png` is ready for it).
* A final logo (the current one is on the palette via `scripts/recolor.py`;
  rerun it on new art).

## Tools this session left behind

* `scripts/tape.py` / `scripts/tape-midis.sh`: renders → tapes for the mock
  (MIDI ones go to the git-ignored `docs/design/tapes/local/`).
* `scripts/recolor.py`: art onto the palette.
* The mock found one engine bug already (fixed in `68c1bfa`): it's a good
  way to *see* the performer. Keep it alive while the port happens.
