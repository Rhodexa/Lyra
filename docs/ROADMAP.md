# Roadmap

Every milestone ends with something you can *play*. Nothing on a later line gets
built before the line it depends on.

## M1: First beat ✓
- [x] Workspace, plugin interface, real-time engine with FIFOs
- [x] Synth drums: kick, snare, clap, hi-hat (with choke groups)
- [x] Channel rack: paint/erase steps, velocity and pitch on the wheel, mute/solo, vol/pan
- [x] Transport: play/stop, tempo, swing, position, beat lights, master meter
- [x] Typing keyboard as a piano, undo/redo
- [x] First build, plus fixing measured audio underruns (512-frame default, live underrun monitor)
- [ ] Tune the drum sounds by ear

## M2: Patterns & melody (current)
- [x] Lyra Synth: poly, unison ×7, sub, SVF filter + envelope, ADSR, 7 presets
- [x] Engine tracks held notes, so notes never hang when edited mid-play
- [x] Piano roll: FL-style mouse, chord stamp in key, scale highlight, velocity lane, ghosts
- [x] Instrument editor generated from plugin params (+ envelope view)
- [x] Multiple patterns (new / clone / switch)
- [x] Playlist arrangement + song mode (PAT/SONG, `L`, F5); clips repeat their pattern
- [ ] Organ waveform for M1-style organ bass
- [ ] Piano roll uses the shared `timeline` module (overview + ruler) like the playlist

## M3: Keep it, mix it
- [x] Save / open projects (`.lyra`, readable JSON, versioned), backup every 30 s + on quit, reopen the last project
- [x] Mixer (F9): master + 16 inserts, channel routing, 8 effect slots per track, meters
- [x] Built-in FX: EQ, Filter, Compressor, Delay (tempo-synced, ping-pong), Reverb, Pump (beat-synced ducking)
- [ ] WAV export (the same engine, offline)
- [ ] Sends / sidechain from a real kick
- [x] Fluttersine (FM synth, id `lyra.fluttersine`; port of the original: 6 operators, 6×6 matrix, filters, LFO, 18 presets) with a tabbed editor
- [ ] Library folder for samples/plugins + "collect into project" (with M4)
- Mixer inserts with effect slots; per-channel meters
- Built-in FX: EQ, compressor, delay, reverb, filter, sidechain pump
- Save/load projects; render to WAV (the same engine running offline)

## M4: Audio
- [x] Sampler instrument (one-shot / hold, root key, tune, attack/release, start), WAV + FLAC
- [x] Starter Kit: 65 CC0 one-shots (from Sonic Pi via Freesound, credits per file)
- [x] Browser: sample library (kit + your folder), click to hear, double-click / drag onto the plugin rack
- [x] Audio clips in the arrangement: drop a sample on a track; shared like patterns (Ctrl+U makes a copy unique); trim each copy's ends; fades with curves (on the clip or in the audio-clip editor), gain, trim; each track picks the insert its audio plays into
- [ ] Settings saving (layout, sizes, zoom); WAV export
- [ ] Add any folder to the browser (your animation project's voice folder), not only the library
- More waveform editing: split at the playhead, normalize, reverse; clips that follow tempo changes
- Sample browser: drag files in from the file manager (needs window drag-and-drop on Wayland)

## M5: Plugins & automation
- CLAP hosting with plugin windows (third-party synths/FX)
- Automation clips
- Docs for writing Lyra/CLAP plugins

## Later
- VST3 / LV2, MIDI hardware input, JACK
- Video / image-sequence lane synced to the timeline (for animation timing)
- Theme packs from hand-made assets
