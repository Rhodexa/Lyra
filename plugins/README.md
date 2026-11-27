# Plugins

Every folder here is one plugin, built into Lyra. Each one is its own small
project: its code, art, docs and (if it wants) its own journal live in its
folder, not in Lyra's `docs/`.

## The same parts in every folder

| part | what it is | may use |
|---|---|---|
| `plugin/` | **what Lyra loads**: the sound, parameters, presets. Its `PLUGINS` list is what `lyra-catalog` gathers | `lyra-plugin`, `lyra-dsp`, its own `core/` |
| `editor/` | the faceplate (optional: without one, Lyra shows plain knobs) | `lyra-kit`, `lyra-plugin`, its own `plugin/`, `shared-look`, `egui` |
| `core/` | optional: the engine with **no Lyra in it at all**, so it can become a standalone CLAP plugin or tool later | nothing from Lyra |
| `artwork/`, `docs/`, `scripts/`, `README.md`, `JOURNAL.md` | optional, as the plugin needs | |

Rules, so plugins never step on each other:

* A plugin never uses another plugin, nor `lyra-engine`, `lyra-model` or
  `lyra-app`. It only knows the plugin interface.
* Only `shared-look` is shared, and it is frozen (Fluttersine and Chrysalis's look).
* Plugins own their look; Lyra's theme never reaches into them.

## Today

| folder | what | parts |
|---|---|---|
| `chrysalis/` | wavetable synth | plugin, editor, artwork |
| `fluttersine/` | six-operator FM synth | plugin, editor |
| `octavia/` | modelled cello/violin and the player who plays it | core, plugin, editor, render (a command-line renderer: `cargo octavia`), docs, scripts, scores, journal |
| `synth/` | subtractive synth | plugin |
| `sampler/` | one sample across the keyboard | plugin |
| `drums/` | kick, snare, clap, hi-hat | plugin |
| `effects/` | EQ, filter, compressor, delay, reverb, pump | plugin |
| `shared-look/` | not a plugin: the frozen look Fluttersine and Chrysalis share | |

Only Octavia has a `core/`, because it was built outside Lyra from the
start. The others keep their sound in `plugin/`; a `core/` is worth adding
only when a plugin is about to leave Lyra.

## Adding one

1. `plugins/<name>/plugin/` with a `Cargo.toml` (`name = "lyra-<name>"`) and
   a `PLUGINS` list; `plugin/` and `editor/` folders are found automatically.
2. Add it to `[workspace.dependencies]` in the root `Cargo.toml` and chain
   its `PLUGINS` in `crates/lyra-catalog/src/lib.rs`.
3. An editor also goes in `crates/lyra-app/src/plugin_host.rs` (everything
   else gets the generic editor).
