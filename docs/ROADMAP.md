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
- [ ] Playlist arrangement + song mode
- [ ] Organ waveform for M1-style organ bass

## M3: Mixer
- Mixer inserts with effect slots; per-channel meters
- Built-in FX: EQ, compressor, delay, reverb, filter, sidechain pump
- Save/load projects; render to WAV (the same engine running offline)

## M4: Audio
- Sampler, sample browser (drag from disk), audio clips
- Waveform editing (cut, fade, normalize): the voice-acting workflow

## M5: Plugins & automation
- CLAP hosting with plugin windows (third-party synths/FX)
- Automation clips
- Docs for writing Lyra/CLAP plugins

## Later
- VST3 / LV2, MIDI hardware input, JACK
- Video / image-sequence lane synced to the timeline (for animation timing)
- Theme packs from hand-made assets
