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
- [x] Built-in FX: EQ, Filter, Compressor, Delay (tempo-synced, ping-pong), Reverb, Pump (beat-synced ducking), Limiter (look-ahead brickwall), Chorus, Drive (soft / hard / fold / bit-crush)
- [x] WAV export (the same bridge and engine, offline): project menu → Export WAV / Export stems (Ctrl+E / Ctrl+Shift+E), and `lyra --render song.lyra [out.wav] [--stems dir]`
- [ ] Sends / sidechain from a real kick
- [x] Fluttersine (FM synth, id `lyra.fluttersine`; port of the original: 6 operators, 6×6 matrix, filters, LFO, 18 presets) with a tabbed editor
- [x] Chrysalis (wavetable synth, id `lyra.chrysalis`): 2 table oscillators (9 built-in tables + WAV import, morphing position, 7 warps, unison ×16), sub, noise, per-voice drive, 2 filters (SVF / formant / comb), 4 envelopes with curves, 4 drawn LFOs, a 16-slot matrix with "via", 4 macros, 7 presets, a hand-made editor; demo songs `examples/chrysalis.rs` and `examples/glitchhop.rs` ("Wingbeat": velocity sets the talking bass's wobble speed)
- [x] Fluttersine, beyond the port: tempo-synced LFOs (locked to the song while it plays), a second LFO, two mod envelopes, six mod slots (source → pitch, cutoff, volume, FM depth, or any operator's level / FM in / pitch / warp); the MOD page. Stereo (a PAN per operator, supersaw Width), and velocity sets loudness
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
- The audio-clip editor as a small *destructive* recording editor (Rhodexa's idea; non-destructive editing is the arrangement's job): New, Save, Record (always appends to the end of the audio), select a range, then Silence it (clears the samples) or Collapse it (closes the gap: everything after moves up, so cutting from the start shifts the rest to the beginning); Invert selection. A few tools, on the sample data itself
- Sample browser: drag files in from the file manager (needs window drag-and-drop on Wayland)

## M5: Plugins & automation
- CLAP hosting with plugin windows (third-party synths/FX)
- Automation clips
- Docs for writing Lyra/CLAP plugins

## Later
- Pipsqueak, the chip synth (id `lyra.pipsqueak`; shelved, Fluttersine and Chrysalis cover chip sounds for now): tracker-style macros (volume / arp / pitch / duty), LFSR noise, chord-arp mode. Spec: [specs/chip-synth.md](specs/chip-synth.md)
- VST3 / LV2, MIDI hardware input, JACK
- Video / image-sequence lane synced to the timeline (for animation timing)
- Theme packs from hand-made assets
