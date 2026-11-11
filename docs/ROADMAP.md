# Roadmap

Every milestone ends with something you can *play*. Nothing on a later line gets
built before the line it depends on.

## M1: First beat (current)
- [x] Workspace, plugin interface, real-time engine with FIFOs
- [x] Synth drums: kick, snare, clap, hi-hat (with choke groups)
- [x] Channel rack: paint/erase steps, velocity and pitch on the wheel, mute/solo, vol/pan
- [x] Transport: play/stop, tempo, swing, position, beat lights, master meter
- [x] Typing keyboard as a piano, undo/redo
- [ ] **First build on a real connection** (`cargo run`), plus fix-ups
- [ ] Tune the drum sounds by ear

## M2: Patterns & melody
- Poly synth (saw/square/organ, unison, filter + envelopes)
- Piano roll (FL-style: click places the last-used length, drag edges, chord stamp, scale highlight)
- Multiple patterns + playlist arrangement, song mode

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
