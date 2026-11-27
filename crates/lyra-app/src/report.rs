//! Export data: everything about a song as plain files somebody (or some
//! language model) can read without Lyra. Used by Export data… in the
//! project menu and by the command line:
//!
//! ```text
//! lyra --export-data song.lyra [<folder>]
//! ```
//!
//! The folder (default: "<song> data" next to the song) gets:
//!
//! - `<song>.mid`: every channel as a MIDI track, with Lyra's swing applied.
//! - `song.md`: tempo, key, channels and their sounds (only the settings that
//!   differ from the instrument's defaults), the mixer, the arrangement, the
//!   automations, estimated chords per bar, and every pattern's notes.
//! - `mix.md`: the mix measured: level per stem per four bars, stereo width,
//!   and band balance (how much top end there is against the middle).
//! - `levels.csv`: the same measurements per bar, for spreadsheets.
//! - `spectrogram.png`: the mix over time, log frequency, colour = loudness.
//!
//! Everything comes from the project and from renders made the same way
//! Export WAV makes them. Nothing here changes the song.

use crate::render::{self, Part, SAMPLE_RATE};
use crate::samples::Samples;
use lyra_model::theory::note_name;
use lyra_model::{Clip, Project, Source, Target, TICKS_PER_BAR, TICKS_PER_STEP};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

const BEAT: u32 = TICKS_PER_STEP * 4;
/// Five bands, as (name, low edge, high edge) in Hz (0 = open).
const BANDS: [(&str, f32, f32); 5] = [
    ("low <150", 0.0, 150.0),
    ("low-mid 150-1k", 150.0, 1000.0),
    ("mid 1-4k", 1000.0, 4000.0),
    ("high 4-8k", 4000.0, 8000.0),
    ("air 8-16k", 8000.0, 16000.0),
];
const MID_BAND: usize = 2;
const AIR_BAND: usize = 4;

/// Write the whole data folder. `progress` counts to 1000 over the renders.
pub fn export(
    project: &Project,
    title: &str,
    samples: &mut Samples,
    dir: &Path,
    progress: Option<&AtomicU32>,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let write = |name: &str, bytes: &[u8]| -> Result<PathBuf, String> {
        let path = dir.join(name);
        std::fs::write(&path, bytes).map_err(|e| format!("can't write {}: {e}", path.display()))?;
        Ok(path)
    };
    let mut written = vec![
        write(&format!("{}.mid", render::file_safe(title)), &midi(project, title))?,
        write("song.md", sheet(project, title).as_bytes())?,
    ];
    let mut mono = Vec::new();
    let (md, csv) = mix_report(project, title, samples, progress, &mut mono)?;
    written.push(write("mix.md", md.as_bytes())?);
    written.push(write("levels.csv", csv.as_bytes())?);
    let frames_per_bar = TICKS_PER_BAR as f64 * project.seconds_per_tick() * SAMPLE_RATE as f64;
    let bars = (project.song_length() / TICKS_PER_BAR) as usize;
    let image = crate::spectrogram::png(&mono, SAMPLE_RATE as f32, frames_per_bar, bars)?;
    written.push(write("spectrogram.png", &image)?);
    Ok(written)
}

// ─────────────────────────────── the notes ───────────────────────────────

/// Every note the song plays, as (channel index, start, len, key, velocity),
/// with swing applied the way the engine applies it.
fn song_notes(p: &Project) -> Vec<(usize, u32, u32, u8, f32)> {
    let swing = (p.swing.clamp(0.0, 1.0) * TICKS_PER_STEP as f32 / 3.0).round() as u32;
    let mut out: Vec<_> = crate::bridge::compile_song(p)
        .notes
        .iter()
        .filter(|n| !p.channels[n.channel as usize].mute)
        .map(|n| {
            let late = if (n.start / TICKS_PER_STEP) % 2 == 1 { swing } else { 0 };
            (n.channel as usize, n.start + late, n.len.max(1), n.key, n.velocity)
        })
        .collect();
    out.sort_by_key(|n| (n.1, n.0, n.3));
    out
}

/// A channel that plays one-shots (drums, a dropped hit) rather than tunes.
fn percussive(p: &Project, channel: usize, notes: &[(usize, u32, u32, u8, f32)]) -> bool {
    let ch = &p.channels[channel];
    if lyra_catalog::info(&ch.instrument).is_some_and(|i| !i.melodic) {
        return true;
    }
    let mut keys: Vec<u8> = notes.iter().filter(|n| n.0 == channel).map(|n| n.3).collect();
    keys.sort_unstable();
    keys.dedup();
    ch.sample.is_some() && keys.len() <= 2
}

// ─────────────────────────────── MIDI ───────────────────────────────────

/// The song as a MIDI file: one track per channel that plays (drums on
/// General MIDI's channel 10, so other programs show them as drums).
fn midi(p: &Project, title: &str) -> Vec<u8> {
    let notes = song_notes(p);
    let mut tracks = Vec::new();
    let mut next_channel = 0u8;
    for i in 0..p.channels.len() {
        let mine: Vec<lyra_model::Note> = notes
            .iter()
            .filter(|n| n.0 == i)
            .map(|n| lyra_model::Note { start: n.1, len: n.2, key: n.3, velocity: n.4, link: None })
            .collect();
        if mine.is_empty() {
            continue;
        }
        let channel = if percussive(p, i, &notes) {
            9
        } else {
            let c = next_channel;
            next_channel = (next_channel + 1) % 15; // 0..14, skipping 9 below
            if c >= 9 {
                c + 1
            } else {
                c
            }
        };
        tracks.push(crate::midi::TrackOut { name: channel_label(p, i), channel, notes: mine });
    }
    crate::midi::write(title, p.bpm, p.song_length(), &tracks)
}

// ─────────────────────────────── song.md ────────────────────────────────

/// Bar.beat.sixteenth, 1-based, plus leftover ticks ("5.2.3", "5.2.3+12t").
fn when(tick: u32) -> String {
    let bar = tick / TICKS_PER_BAR + 1;
    let beat = tick % TICKS_PER_BAR / BEAT + 1;
    let six = tick % BEAT / TICKS_PER_STEP + 1;
    let rest = tick % TICKS_PER_STEP;
    if rest == 0 {
        format!("{bar}.{beat}.{six}")
    } else {
        format!("{bar}.{beat}.{six}+{rest}t")
    }
}

/// A length in sixteenths ("4", "1.5").
fn steps(len: u32) -> String {
    if len.is_multiple_of(TICKS_PER_STEP) {
        (len / TICKS_PER_STEP).to_string()
    } else {
        format!("{:.2}", len as f32 / TICKS_PER_STEP as f32).trim_end_matches('0').to_string()
    }
}

fn changed_params(id: &str, values: &[f32], effect: bool) -> String {
    let (infos, defaults, groups) = if effect {
        (lyra_catalog::effects::params(id), lyra_catalog::effects::default_params(id), &[][..])
    } else {
        (lyra_catalog::params(id), lyra_catalog::default_params(id), lyra_catalog::groups(id))
    };
    let mut parts = Vec::new();
    for (i, info) in infos.iter().enumerate() {
        let (Some(&v), Some(&d)) = (values.get(i), defaults.get(i)) else { continue };
        if (v - d).abs() > 1e-4 * (info.max - info.min).abs().max(1e-6) {
            // Names that repeat (every FM operator has an "Attack") get their group's name.
            let repeats = infos.iter().filter(|o| o.name == info.name).count() > 1;
            let group = groups.iter().find(|g| g.ids.contains(&info.id)).map(|g| g.name);
            match group {
                Some(g) if repeats => parts.push(format!("{g} {} {}", info.name, info.format(v))),
                _ => parts.push(format!("{} {}", info.name, info.format(v))),
            }
        }
    }
    if parts.is_empty() {
        "defaults".into()
    } else {
        parts.join(", ")
    }
}

fn effect_name(id: &str) -> &str {
    lyra_catalog::effects::info(id).map_or(id, |i| i.name)
}

fn target_name(p: &Project, t: &Target) -> String {
    let index = |id| p.channels.iter().position(|c| c.id == id);
    let label = |id| index(id).map_or("a deleted channel".to_string(), |i| channel_label(p, i));
    match *t {
        Target::Param { channel: id, param } => match index(id) {
            Some(i) => {
                let name = lyra_catalog::params(&p.channels[i].instrument).get(param as usize).map_or("?", |x| x.name);
                format!("{} › {name}", channel_label(p, i))
            }
            None => "a deleted channel".into(),
        },
        Target::Volume(id) => format!("{} › volume", label(id)),
        Target::Pan(id) => format!("{} › pan", label(id)),
        Target::MixerVolume(m) => format!("mixer {} › volume", mixer_name(p, m)),
        Target::MixerPan(m) => format!("mixer {} › pan", mixer_name(p, m)),
        Target::Effect { effect, param } => {
            for (m, track) in p.mixer.iter().enumerate() {
                if let Some(slot) = track.effects.iter().find(|s| s.id == effect) {
                    let name = lyra_catalog::effects::params(&slot.effect).get(param as usize).map_or("?", |i| i.name);
                    return format!("mixer {} › {} › {name}", mixer_name(p, m), effect_name(&slot.effect));
                }
            }
            "a deleted effect".into()
        }
        Target::MasterVolume => "master volume".into(),
    }
}

/// The value a link sets at curve value `v`, in the target's own units.
fn target_value(p: &Project, t: &Target, travel: f32) -> String {
    let info = match *t {
        Target::Param { channel, param } => p
            .channels
            .iter()
            .find(|c| c.id == channel)
            .and_then(|c| lyra_catalog::params(&c.instrument).get(param as usize)),
        Target::Effect { effect, param } => p
            .mixer
            .iter()
            .flat_map(|m| &m.effects)
            .find(|s| s.id == effect)
            .and_then(|s| lyra_catalog::effects::params(&s.effect).get(param as usize)),
        _ => None,
    };
    match info {
        Some(i) => i.format(i.from_normalized(travel)),
        None => format!("{:.0}% travel", travel * 100.0),
    }
}

/// A channel's name, with its number when another channel shares the name.
fn channel_label(p: &Project, index: usize) -> String {
    let name = &p.channels[index].name;
    if p.channels.iter().filter(|c| &c.name == name).count() > 1 {
        format!("{} {name}", index + 1)
    } else {
        name.clone()
    }
}

fn mixer_name(p: &Project, m: usize) -> String {
    match p.mixer.get(m) {
        Some(_) if m == 0 => "Master".into(),
        Some(t) if !t.name.is_empty() => format!("{m} ({})", t.name),
        _ => m.to_string(),
    }
}

fn sheet(p: &Project, title: &str) -> String {
    let notes = song_notes(p);
    let bars = p.song_length() / TICKS_PER_BAR;
    let seconds = p.song_length() as f64 * p.seconds_per_tick();
    let mut s = String::new();
    let _ = writeln!(s, "# {title}\n");
    let _ = writeln!(
        s,
        "Exported from Lyra. Positions are bar.beat.sixteenth (1-based); lengths are in sixteenths; \
         notes are scientific pitch (C4 = middle C = MIDI 60). 4/4 throughout.\n"
    );
    let _ = writeln!(
        s,
        "- Tempo: {} BPM, swing {:.0}% (every second sixteenth comes late by up to a third of a sixteenth)",
        fmt_bpm(p.bpm),
        p.swing * 100.0
    );
    let _ = writeln!(s, "- Length: {bars} bars, {}:{:02}", seconds as u32 / 60, seconds as u32 % 60);
    let _ = writeln!(
        s,
        "- Key set in the project: {} {}",
        lyra_model::theory::NOTE_NAMES[p.key_root as usize % 12],
        p.scale.name()
    );
    let pitched: Vec<_> = notes.iter().filter(|n| !percussive(p, n.0, &notes)).copied().collect();
    if let Some(k) = estimate_key(&pitched) {
        let _ = writeln!(s, "- Key heard in the notes (estimate): {k}");
    }
    let _ = writeln!(s, "- Master volume: {}", lyra_plugin::gain_to_db(p.master_volume).pipe_db());

    // Channels.
    let _ = writeln!(s, "\n## Channels\n");
    let _ = writeln!(s, "Sound settings list only what differs from the instrument's defaults.\n");
    for (i, ch) in p.channels.iter().enumerate() {
        let mine: Vec<_> = notes.iter().filter(|n| n.0 == i).collect();
        let instrument = lyra_catalog::info(&ch.instrument).map_or(ch.instrument.as_str(), |i| i.name);
        let _ = write!(s, "### {}. {}{}\n\n", i + 1, ch.name, if ch.mute { " (muted)" } else { "" });
        let _ = writeln!(
            s,
            "- Instrument: {instrument}{}",
            ch.sample.as_ref().map_or(String::new(), |x| format!(", sample `{}`", private(x)))
        );
        let _ = writeln!(
            s,
            "- Volume {}, pan {}, mixer insert {}",
            lyra_plugin::gain_to_db(ch.volume).pipe_db(),
            pan(ch.pan),
            mixer_name(p, ch.mixer)
        );
        if mine.is_empty() {
            let _ = writeln!(s, "- Plays no notes in the song");
        } else {
            let lo = mine.iter().map(|n| n.3).min().unwrap_or(0);
            let hi = mine.iter().map(|n| n.3).max().unwrap_or(0);
            let kind = if percussive(p, i, &notes) { ", one-shots" } else { "" };
            let _ = writeln!(s, "- {} notes, range {}–{}{kind}", mine.len(), note_name(lo), note_name(hi));
        }
        let _ = writeln!(s, "- Sound: {}\n", changed_params(&ch.instrument, &ch.params, false));
    }
    for a in &p.audio {
        let _ = writeln!(
            s,
            "### Audio: {}\n\n- Sample `{}`, volume {}, pan {}, mixer insert {}{}\n",
            a.name,
            private(&a.sample),
            lyra_plugin::gain_to_db(a.volume).pipe_db(),
            pan(a.pan),
            mixer_name(p, a.mixer),
            if a.mute { " (muted)" } else { "" }
        );
    }

    // Mixer.
    let _ = writeln!(s, "## Mixer\n");
    let _ = writeln!(s, "Every insert goes to the master. Effects run top to bottom.\n");
    for (m, t) in p.mixer.iter().enumerate() {
        let used = m == 0
            || !t.effects.is_empty()
            || p.channels.iter().any(|c| c.mixer == m)
            || p.audio.iter().any(|a| a.mixer == m);
        if !used {
            continue;
        }
        let _ = writeln!(
            s,
            "- **{}**: volume {}, pan {}{}",
            mixer_name(p, m),
            lyra_plugin::gain_to_db(t.volume).pipe_db(),
            pan(t.pan),
            if t.mute { ", muted" } else { "" }
        );
        for fx in &t.effects {
            let _ = writeln!(
                s,
                "  - {}{}: {}",
                effect_name(&fx.effect),
                if fx.bypass { " (bypassed)" } else { "" },
                changed_params(&fx.effect, &fx.params, true)
            );
        }
    }

    // Arrangement.
    let _ = writeln!(s, "\n## Arrangement\n");
    let _ = writeln!(s, "Playlist tracks and their clips, in bars (1-based, inclusive).\n");
    for (ti, track) in p.tracks.iter().enumerate() {
        let mut clips: Vec<&Clip> = p.clips.iter().filter(|c| c.track == ti).collect();
        if clips.is_empty() {
            continue;
        }
        clips.sort_by_key(|c| c.start);
        let name = if track.name.is_empty() { format!("Track {}", ti + 1) } else { track.name.clone() };
        let _ = writeln!(s, "- **{name}**{}", if track.mute { " (muted)" } else { "" });
        // Back-to-back copies of one clip read as one line ("×8").
        let mut runs: Vec<(&Clip, u32, usize)> = Vec::new(); // (first, end, count)
        for c in clips {
            match runs.last_mut() {
                Some((first, end, n))
                    if *end == c.start
                        && first.source == c.source
                        && first.len == c.len
                        && first.offset == c.offset =>
                {
                    *end = c.end();
                    *n += 1;
                }
                _ => runs.push((c, c.end(), 1)),
            }
        }
        for (c, end, count) in runs {
            let what = match c.source {
                Source::Pattern(id) => format!("pattern \"{}\"", p.pattern(id).map_or("?", |x| x.name.as_str())),
                Source::Audio(id) => {
                    format!("audio \"{}\"", p.audio.iter().find(|a| a.id == id).map_or("?", |a| a.name.as_str()))
                }
                Source::Automation(id) => format!(
                    "automation \"{}\"",
                    p.automations.iter().find(|a| a.id == id).map_or("?", |a| a.name.as_str())
                ),
            };
            let offset = if c.offset > 0 { format!(", starting {} steps in", steps(c.offset)) } else { String::new() };
            let times = if count > 1 { format!(" ×{count} back to back") } else { String::new() };
            let _ = writeln!(s, "  - {}–{}: {what}{offset}{times}", span_start(c.start), span_end(end));
        }
    }

    // Automations.
    if !p.automations.is_empty() {
        let _ = writeln!(s, "\n## Automations\n");
        let _ = writeln!(s, "Each curve runs 0..1 over its clip and moves every target between its two ends. Points are (position in the curve, value); a control keeps its last value after the clip ends.\n");
        for a in &p.automations {
            let _ = writeln!(s, "- **{}** ({} steps long)", a.name, steps(a.length));
            for l in &a.links {
                let _ = writeln!(
                    s,
                    "  - moves {}: {} at 0 → {} at 1",
                    target_name(p, &l.target),
                    target_value(p, &l.target, l.from),
                    target_value(p, &l.target, l.to)
                );
            }
            let pts: Vec<String> = a.points.iter().map(|pt| format!("({}, {:.2})", when(pt.tick), pt.value)).collect();
            let _ = writeln!(s, "  - points: {}", pts.join(" "));
        }
    }

    // Chords.
    let _ = writeln!(s, "\n## Harmony, half bar by half bar\n");
    let _ = writeln!(
        s,
        "Every pitched note sounding in each half bar (for at least a sixteenth), lowest first, chords and melody together: \
         the song may not use chords as such, so read these as notes, not as chord symbols. \
         \"Strongest\" are the pitch classes that sound longest. The guess is a chord name that fits those notes, \
         made by a program. It is not the composer's intent.\n"
    );
    let half = TICKS_PER_BAR / 2;
    for bar in 0..bars {
        for k in 0..2 {
            let from = bar * TICKS_PER_BAR + k * half;
            let keys = sounding(&pitched, from, half);
            let at = format!("{}.{}", bar + 1, 1 + k * 2);
            if keys.is_empty() {
                let _ = writeln!(s, "- {at}: (no pitched notes)");
                continue;
            }
            let names: Vec<String> = keys.iter().map(|&k| note_name(k)).collect();
            let _ = writeln!(
                s,
                "- {at}: {} · strongest {} · guess {}",
                names.join(" "),
                strongest(&pitched, from, half).join(" "),
                chord_at(&pitched, from, half)
            );
        }
    }

    // Patterns.
    let _ = writeln!(s, "\n## Patterns\n");
    let _ = writeln!(s, "Each pattern's notes as written (before swing), per channel and per bar of the pattern. `[C4 E4 G4]` = notes starting together. `name@pos×len` (len in sixteenths), `v` = velocity if not full.\n");
    // The patterns the song plays first, then the ones only lying around.
    let mut pats: Vec<_> = p.patterns.iter().collect();
    pats.sort_by_key(|pat| !p.clips.iter().any(|c| c.pattern() == Some(pat.id)));
    for pat in pats {
        if pat.notes.values().all(Vec::is_empty) {
            continue;
        }
        let used = p.clips.iter().filter(|c| c.pattern() == Some(pat.id)).count();
        let used = if used == 0 { "not in the song".to_string() } else { format!("{used} clips") };
        let _ = writeln!(s, "### Pattern \"{}\" ({} steps, {used})\n", pat.name, pat.play_length());
        for (ci, ch) in p.channels.iter().enumerate() {
            let list = pat.notes(ch.id);
            if list.is_empty() {
                continue;
            }
            let _ = writeln!(s, "- **{}**:", channel_label(p, ci));
            let mut by_bar: BTreeMap<u32, Chords> = BTreeMap::new();
            for n in list {
                by_bar
                    .entry(n.start / TICKS_PER_BAR)
                    .or_default()
                    .entry((n.start, n.len))
                    .or_default()
                    .push((n.key, n.velocity));
            }
            for (bar, groups) in by_bar {
                let words: Vec<String> = groups
                    .iter()
                    .map(|(&(start, len), keys)| {
                        let mut keys = keys.clone();
                        keys.sort_by_key(|k| k.0);
                        let names: Vec<String> = keys.iter().map(|k| note_name(k.0)).collect();
                        let pos = when(start % TICKS_PER_BAR);
                        let pos = &pos[pos.find('.').map_or(0, |i| i + 1)..];
                        let v = keys.iter().map(|k| k.1).fold(0.0f32, f32::max);
                        let vel = if v < 0.995 { format!(" v{:.0}", v * 100.0) } else { String::new() };
                        let n = if names.len() == 1 { names[0].clone() } else { format!("[{}]", names.join(" ")) };
                        format!("{n}@{pos}×{}{vel}", steps(len))
                    })
                    .collect();
                let _ = writeln!(s, "  - bar {}: {}", bar + 1, words.join("  "));
            }
        }
        let _ = writeln!(s);
    }
    s
}

/// A sample path without the home folder in it (the file may be shared).
fn private(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && path.starts_with(&home) => format!("~{}", &path[home.len()..]),
        _ => path.to_string(),
    }
}

/// Notes starting together: (start, len) → [(key, velocity)].
type Chords = BTreeMap<(u32, u32), Vec<(u8, f32)>>;

trait PipeDb {
    fn pipe_db(self) -> String;
}
impl PipeDb for f32 {
    fn pipe_db(self) -> String {
        if self < -120.0 {
            "−inf dB".into()
        } else {
            format!("{self:+.1} dB")
        }
    }
}

fn fmt_bpm(bpm: f64) -> String {
    if (bpm - bpm.round()).abs() < 1e-6 {
        format!("{bpm:.0}")
    } else {
        format!("{bpm:.2}")
    }
}

fn pan(v: f32) -> String {
    if v.abs() < 0.01 {
        "centre".into()
    } else if v < 0.0 {
        format!("L{:.0}", -v * 100.0)
    } else {
        format!("R{:.0}", v * 100.0)
    }
}

fn span_start(tick: u32) -> String {
    if tick.is_multiple_of(TICKS_PER_BAR) {
        (tick / TICKS_PER_BAR + 1).to_string()
    } else {
        when(tick)
    }
}

fn span_end(end: u32) -> String {
    if end.is_multiple_of(TICKS_PER_BAR) {
        (end / TICKS_PER_BAR).to_string()
    } else {
        format!("{} (exclusive)", when(end))
    }
}

// ─────────────────────────────── harmony ────────────────────────────────

/// (suffix, intervals). Order breaks ties: simpler chords first.
const CHORDS: &[(&str, &[u8])] = &[
    ("", &[0, 4, 7]),
    ("m", &[0, 3, 7]),
    ("5", &[0, 7]),
    ("sus4", &[0, 5, 7]),
    ("sus2", &[0, 2, 7]),
    ("dim", &[0, 3, 6]),
    ("aug", &[0, 4, 8]),
    ("7", &[0, 4, 7, 10]),
    ("maj7", &[0, 4, 7, 11]),
    ("m7", &[0, 3, 7, 10]),
    ("m7b5", &[0, 3, 6, 10]),
    ("6", &[0, 4, 7, 9]),
    ("m6", &[0, 3, 7, 9]),
    ("add9", &[0, 2, 4, 7]),
    ("madd9", &[0, 2, 3, 7]),
    ("9", &[0, 2, 4, 7, 10]),
    ("maj9", &[0, 2, 4, 7, 11]),
    ("m9", &[0, 2, 3, 7, 10]),
    ("7sus4", &[0, 5, 7, 10]),
    ("mMaj7", &[0, 3, 7, 11]),
];

/// The distinct keys sounding for at least a sixteenth in [from, from + len), low to high.
fn sounding(notes: &[(usize, u32, u32, u8, f32)], from: u32, len: u32) -> Vec<u8> {
    let mut keys: Vec<u8> = notes
        .iter()
        .filter(|n| (n.1 + n.2).min(from + len).saturating_sub(n.1.max(from)) >= TICKS_PER_STEP)
        .map(|n| n.3)
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// Up to four pitch classes that sound longest in [from, from + len).
fn strongest(notes: &[(usize, u32, u32, u8, f32)], from: u32, len: u32) -> Vec<&'static str> {
    let mut w = [0u32; 12];
    for n in notes {
        w[n.3 as usize % 12] += (n.1 + n.2).min(from + len).saturating_sub(n.1.max(from));
    }
    let mut pcs: Vec<usize> = (0..12).filter(|&i| w[i] > 0).collect();
    pcs.sort_by_key(|&i| std::cmp::Reverse(w[i]));
    pcs.into_iter().take(4).map(|i| lyra_model::theory::NOTE_NAMES[i]).collect()
}

/// The chord sounding in [from, from + len), or "—" for silence.
fn chord_at(notes: &[(usize, u32, u32, u8, f32)], from: u32, len: u32) -> String {
    let to = from + len;
    let mut w = [0.0f32; 12];
    let mut bass: Option<(u8, f32)> = None;
    for n in notes {
        let (a, b) = (n.1.max(from), (n.1 + n.2).min(to));
        if b <= a {
            continue;
        }
        let weight = (b - a) as f32 / len as f32 * (0.5 + 0.5 * n.4);
        w[n.3 as usize % 12] += weight;
        // The bass: the lowest note that sounds for at least a quarter of the window.
        if b - a >= len / 4 && bass.is_none_or(|(k, _)| n.3 < k) {
            bass = Some((n.3, weight));
        }
    }
    let total: f32 = w.iter().sum();
    if total < 1e-6 {
        return "—".into();
    }
    w.iter_mut().for_each(|x| *x /= total);
    let bass_pc = bass.map(|b| b.0 % 12);
    let mut best = (f32::MIN, 0usize, 0usize);
    for root in 0..12 {
        for (ci, (_, iv)) in CHORDS.iter().enumerate() {
            let tones: Vec<usize> = iv.iter().map(|i| (root + *i as usize) % 12).collect();
            let inside: f32 = tones.iter().map(|&t| w[t]).sum();
            let missing = tones.iter().filter(|&&t| w[t] < 0.02).count() as f32;
            let mut score = inside - (1.0 - inside) - 0.12 * missing - 0.015 * tones.len() as f32;
            if bass_pc == Some(root as u8) {
                score += 0.1;
            }
            if w[root] < 0.02 {
                score -= 0.3;
            }
            if score > best.0 + 1e-4 {
                best = (score, root, ci);
            }
        }
    }
    let (_, root, ci) = best;
    let names = lyra_model::theory::NOTE_NAMES;
    let mut name = format!("{}{}", names[root], CHORDS[ci].0);
    if let Some(b) = bass_pc.filter(|&b| b as usize != root) {
        let _ = write!(name, "/{}", names[b as usize]);
    }
    name
}

/// Krumhansl–Kessler key profiles against the song's pitch classes.
fn estimate_key(notes: &[(usize, u32, u32, u8, f32)]) -> Option<String> {
    const MAJOR: [f32; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
    const MINOR: [f32; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];
    let mut w = [0.0f32; 12];
    for n in notes {
        w[n.3 as usize % 12] += n.2 as f32;
    }
    if w.iter().sum::<f32>() <= 0.0 {
        return None;
    }
    let corr = |profile: &[f32; 12], root: usize| {
        let mx = w.iter().sum::<f32>() / 12.0;
        let mp = profile.iter().sum::<f32>() / 12.0;
        let (mut num, mut dx, mut dp) = (0.0, 0.0, 0.0);
        for i in 0..12 {
            let x = w[(root + i) % 12] - mx;
            let y = profile[i] - mp;
            num += x * y;
            dx += x * x;
            dp += y * y;
        }
        num / (dx * dp).sqrt().max(1e-9)
    };
    let mut ranked: Vec<(f32, String)> = (0..12)
        .flat_map(|r| {
            let n = lyra_model::theory::NOTE_NAMES[r];
            [(corr(&MAJOR, r), format!("{n} major")), (corr(&MINOR, r), format!("{n} minor"))]
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    Some(format!("{} (fit {:.2}; next: {} {:.2})", ranked[0].1, ranked[0].0, ranked[1].1, ranked[1].0))
}

// ─────────────────────────────── the mix ────────────────────────────────

/// A Butterworth (Q = 1/√2) biquad, low- or high-pass.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    fn new(freq: f32, high: bool) -> Self {
        let w = std::f32::consts::TAU * freq / SAMPLE_RATE as f32;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / std::f32::consts::SQRT_2;
        let a0 = 1.0 + alpha;
        let b = if high {
            [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0]
        } else {
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0]
        };
        Biquad { b: b.map(|x| x / a0), a: [-2.0 * cos / a0, (1.0 - alpha) / a0], z: [0.0; 2] }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// A band as two 2nd-order sections at each edge (24 dB/oct).
struct Band(Vec<Biquad>);

impl Band {
    fn new(lo: f32, hi: f32) -> Self {
        let mut f = Vec::new();
        if lo > 0.0 {
            f.extend([Biquad::new(lo, true), Biquad::new(lo, true)]);
        }
        if hi > 0.0 {
            f.extend([Biquad::new(hi, false), Biquad::new(hi, false)]);
        }
        Band(f)
    }

    fn run(&mut self, x: f32) -> f32 {
        self.0.iter_mut().fold(x, |x, f| f.run(x))
    }
}

/// Sums of squares over one bar.
#[derive(Clone, Copy, Default)]
struct Bar {
    frames: f64,
    mid: f64,
    side: f64,
    bands: [f64; 5],
    peak: f32,
}

impl Bar {
    fn add(&mut self, o: &Bar) {
        self.frames += o.frames;
        self.mid += o.mid;
        self.side += o.side;
        for i in 0..5 {
            self.bands[i] += o.bands[i];
        }
        self.peak = self.peak.max(o.peak);
    }

    /// RMS of the stereo signal, in dBFS.
    fn rms_db(&self) -> f32 {
        db(((self.mid + self.side) / self.frames.max(1.0)) as f32)
    }

    fn band_db(&self, i: usize) -> f32 {
        db((self.bands[i] / self.frames.max(1.0)) as f32)
    }

    /// Side against mid, in dB: −∞ = mono, 0 = as wide as it is centred.
    fn width_db(&self) -> f32 {
        db((self.side / self.mid.max(1e-12)) as f32)
    }

    fn width(&self) -> String {
        let w = self.width_db();
        if w < -60.0 {
            "mono".into()
        } else {
            format!("{w:.1}")
        }
    }
}

/// 10·log10 of a power.
fn db(power: f32) -> f32 {
    if power <= 1e-14 {
        -140.0
    } else {
        10.0 * power.log10()
    }
}

/// Render `part` and measure every bar of it (the tail is the last entry).
/// `keep`, if given, collects the mid (L+R)/2 signal for the spectrogram.
fn measure(
    p: &Project,
    samples: &mut Samples,
    part: Part,
    bars: usize,
    mut keep: Option<&mut Vec<f32>>,
) -> Result<Vec<Bar>, String> {
    let frames_per_bar = TICKS_PER_BAR as f64 * p.seconds_per_tick() * SAMPLE_RATE as f64;
    let mut out = vec![Bar::default(); bars + 1];
    let mut bands: Vec<Band> = BANDS.iter().map(|b| Band::new(b.1, b.2)).collect();
    let mut frame = 0usize;
    render::render_frames(p, samples, part, None, |buf| {
        for f in buf.chunks(2) {
            let bar = ((frame as f64 / frames_per_bar) as usize).min(bars);
            let (l, r) = (f[0], f[1]);
            let (m, s) = ((l + r) * 0.5, (l - r) * 0.5);
            let b = &mut out[bar];
            b.frames += 1.0;
            // mid² + side² = (l² + r²) / 2: the mean power of the two sides.
            b.mid += (m * m) as f64;
            b.side += (s * s) as f64;
            b.peak = b.peak.max(l.abs()).max(r.abs());
            for (i, band) in bands.iter_mut().enumerate() {
                let y = band.run(m);
                b.bands[i] += (y * y) as f64;
            }
            if let Some(keep) = keep.as_deref_mut() {
                keep.push(m);
            }
            frame += 1;
        }
        Ok(())
    })?;
    Ok(out)
}

fn mix_report(
    p: &Project,
    title: &str,
    samples: &mut Samples,
    progress: Option<&AtomicU32>,
    mono: &mut Vec<f32>,
) -> Result<(String, String), String> {
    let bars = (p.song_length() / TICKS_PER_BAR) as usize;
    let notes = song_notes(p);
    let mut parts: Vec<(Part, String)> = vec![(Part::Mix, "Mix".into())];
    for (i, c) in p.channels.iter().enumerate() {
        if notes.iter().any(|n| n.0 == i) {
            parts.push((Part::Channel(i), format!("{} {}", i + 1, c.name)));
        }
    }
    if p.clips.iter().any(|c| c.audio().is_some()) {
        parts.push((Part::Audio, "Audio clips".into()));
    }
    let mut measured = Vec::new();
    for (k, (part, name)) in parts.iter().enumerate() {
        let keep = (*part == Part::Mix).then_some(&mut *mono);
        measured.push((name.clone(), measure(p, samples, *part, bars, keep)?));
        if let Some(progress) = progress {
            progress.store(((k + 1) * 1000 / parts.len()) as u32, Ordering::Relaxed);
        }
    }
    let whole = |bars: &[Bar]| {
        let mut t = Bar::default();
        bars.iter().for_each(|b| t.add(b));
        t
    };
    let mix = whole(&measured[0].1);

    let mut s = String::new();
    let _ = writeln!(s, "# {title}: the mix, measured\n");
    let _ = writeln!(
        s,
        "Rendered at {} Hz the way Export WAV renders. Levels are RMS in dBFS (0 = full scale; music usually sits around −20 to −10). \
         Each stem is one channel played alone through its mixer insert and the master. \
         Width is side against mid in dB (−∞ = mono, around −10 = gently wide, 0 = as much side as centre). \
         Bands are the mid (L+R) signal through 24 dB/octave filters.\n",
        SAMPLE_RATE
    );
    let _ = writeln!(
        s,
        "`spectrogram.png` (next to this file) shows the mix: time left to right with a numbered line every four bars, \
         frequency bottom to top on a log scale from 20 Hz to 20 kHz (lines at 50, 100, 200, 500, 1k, 2k, 5k and 10k Hz), \
         loudness as colour from black (90 dB below the loudest moment) through purple, red and orange to pale yellow.\n"
    );
    let _ = writeln!(s, "## Whole song\n");
    let _ = writeln!(
        s,
        "- Peak: {:+.1} dBFS{}",
        20.0 * mix.peak.max(1e-9).log10(),
        if mix.peak > 1.0 { " (clips in a WAV export)" } else { "" }
    );
    let _ = writeln!(s, "- RMS: {:.1} dBFS", mix.rms_db());
    let _ = writeln!(s, "- Width: {} dB", mix.width());
    let _ = writeln!(s, "- Band balance, dB against the mid band (1–4 kHz):");
    for (i, b) in BANDS.iter().enumerate() {
        let _ = writeln!(s, "  - {}: {:+.1}", b.0, mix.band_db(i) - mix.band_db(MID_BAND));
    }
    let _ = writeln!(s, "- Brightness (air 8–16k minus mid 1–4k): {:+.1} dB. For reference, bright pop/EDM masters sit around −4 to 0, a dark or muffled mix below −10.\n", mix.band_db(AIR_BAND) - mix.band_db(MID_BAND));

    let _ = writeln!(s, "## Stems over the whole song\n");
    let _ = writeln!(s, "| Stem | RMS dB | vs mix | Peak dB | Width dB | Low | Low-mid | Mid | High | Air |");
    let _ = writeln!(s, "|---|---|---|---|---|---|---|---|---|---|");
    for (name, bars) in &measured {
        let t = whole(bars);
        let rel = |i| {
            let v = t.band_db(i) - mix.band_db(MID_BAND);
            if v < -90.0 {
                "·".into()
            } else {
                format!("{v:+.0}")
            }
        };
        let _ = writeln!(
            s,
            "| {name} | {:.1} | {:+.1} | {:+.1} | {} | {} | {} | {} | {} | {} |",
            t.rms_db(),
            t.rms_db() - mix.rms_db(),
            20.0 * t.peak.max(1e-9).log10(),
            t.width(),
            rel(0),
            rel(1),
            rel(2),
            rel(3),
            rel(4)
        );
    }
    let _ = writeln!(s, "\nBand columns: that stem's energy in each band, in dB against the *mix's* 1–4 kHz band, so you can see who fills which band.\n");

    let _ = writeln!(s, "## Level every four bars (RMS dB)\n");
    let _ = write!(s, "| Bars |");
    for (name, _) in &measured {
        let _ = write!(s, " {name} |");
    }
    let _ = write!(s, " Mix width | Mix brightness |\n|---|");
    for _ in &measured {
        let _ = write!(s, "---|");
    }
    let _ = writeln!(s, "---|---|");
    for start in (0..bars).step_by(4) {
        let end = (start + 4).min(bars);
        let _ = write!(s, "| {}–{} |", start + 1, end);
        let mut mix_part = Bar::default();
        for (k, (_, b)) in measured.iter().enumerate() {
            let t = whole(&b[start..end]);
            if k == 0 {
                mix_part = t;
            }
            let v = t.rms_db();
            if v < -90.0 {
                let _ = write!(s, " · |");
            } else {
                let _ = write!(s, " {v:.1} |");
            }
        }
        let bright = mix_part.band_db(AIR_BAND) - mix_part.band_db(MID_BAND);
        let _ = writeln!(s, " {} | {:+.1} |", mix_part.width(), bright);
    }
    let _ = writeln!(s, "\n`·` = silent there.");

    let mut csv = String::from("stem,bar,rms_db,peak_db,width_db");
    for b in BANDS {
        let _ = write!(csv, ",{}_db", b.0.split(' ').next().unwrap_or("band"));
    }
    csv.push('\n');
    for (name, bars) in &measured {
        for (i, b) in bars.iter().enumerate() {
            if b.frames == 0.0 {
                continue;
            }
            let bar = if i + 1 > bars.len() - 1 { "tail".to_string() } else { (i + 1).to_string() };
            let _ = write!(
                csv,
                "\"{}\",{bar},{:.2},{:.2},{:.2}",
                name.replace('"', "'"),
                b.rms_db(),
                20.0 * b.peak.max(1e-9).log10(),
                b.width_db()
            );
            for k in 0..5 {
                let _ = write!(csv, ",{:.2}", b.band_db(k));
            }
            csv.push('\n');
        }
    }
    Ok((s, csv))
}

// ─────────────────────────────── command line ───────────────────────────

/// `lyra --export-data <project.lyra> [<folder>]`. Returns the exit code.
pub fn cli(args: &[String]) -> i32 {
    let usage = "usage: lyra --export-data <project.lyra> [<folder>]";
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{usage}");
        return if args.is_empty() { 2 } else { 0 };
    }
    let input = PathBuf::from(&args[0]);
    let project = match crate::files::load(&input) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("lyra: {e}");
            return 1;
        }
    };
    let title = crate::files::display_name(Some(&input));
    let dir = args.get(1).map(PathBuf::from).unwrap_or_else(|| default_folder(&input, &title));
    let started = std::time::Instant::now();
    match export(&project, &title, &mut Samples::default(), &dir, None) {
        Ok(written) => {
            for f in &written {
                println!("{}", f.display());
            }
            println!("exported in {:.1} s", started.elapsed().as_secs_f64());
            0
        }
        Err(e) => {
            eprintln!("lyra: {e}");
            1
        }
    }
}

/// "<song> data", next to the song.
pub fn default_folder(song: &Path, title: &str) -> PathBuf {
    song.parent().unwrap_or(Path::new(".")).join(format!("{title} data"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyra_model::Note;

    #[test]
    fn midi_has_a_header_and_a_track_per_playing_channel() {
        let mut p = Project::default();
        p.bpm = 120.0;
        let keys = p.add_channel("Keys", "lyra.synth", lyra_catalog::default_params("lyra.synth"));
        let _silent = p.add_channel("Silent", "lyra.synth", lyra_catalog::default_params("lyra.synth"));
        for (i, k) in [60u8, 64, 67].iter().enumerate() {
            p.patterns[0].notes_mut(keys).push(Note { start: i as u32 * 96, len: 96, key: *k, velocity: 1.0, link: None });
        }
        let id = p.patterns[0].id;
        p.add_clip(Clip::of_pattern(id, 0, 0, TICKS_PER_BAR));
        let m = midi(&p, "t");
        assert_eq!(&m[..4], b"MThd");
        assert_eq!(u16::from_be_bytes([m[10], m[11]]), 2, "tempo track + one channel");
        assert_eq!(u16::from_be_bytes([m[12], m[13]]), 96);
        // 500 000 µs per quarter = 120 BPM.
        assert!(m.windows(6).any(|w| w == [0xFF, 0x51, 3, 0x07, 0xA1, 0x20]));
        // Three note-ons on channel 0.
        assert_eq!(m.windows(2).filter(|w| w[0] == 0x90 && [60, 64, 67].contains(&w[1])).count(), 3);
    }

    #[test]
    fn chords_are_named_with_their_bass() {
        let chord = |keys: &[u8]| {
            let notes: Vec<_> = keys.iter().map(|&k| (0, 0, TICKS_PER_BAR, k, 1.0)).collect();
            chord_at(&notes, 0, TICKS_PER_BAR)
        };
        assert_eq!(chord(&[48, 52, 55]), "C");
        assert_eq!(chord(&[57, 60, 64]), "Am");
        assert_eq!(chord(&[53, 57, 60, 64]), "Fmaj7");
        assert_eq!(chord(&[50, 53, 57, 60]), "Dm7");
        assert_eq!(chord(&[40, 53, 57, 60, 64]), "Fmaj7/E");
        assert_eq!(chord(&[]), "—");
    }

    #[test]
    fn positions_read_as_bar_beat_sixteenth() {
        assert_eq!(when(0), "1.1.1");
        assert_eq!(when(TICKS_PER_BAR + BEAT + TICKS_PER_STEP * 2), "2.2.3");
        assert_eq!(when(12), "1.1.1+12t");
        assert_eq!(steps(48), "2");
        assert_eq!(steps(36), "1.5");
    }
}
