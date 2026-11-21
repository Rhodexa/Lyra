//! Rendering to WAV: the song through the very bridge and engine that play
//! it live, only as fast as the CPU goes instead of in real time. Used by
//! Export WAV / Export stems in the project menu, and by the command line:
//!
//! ```text
//! lyra --render song.lyra [out.wav] [--stems <folder>]
//! ```
//!
//! The render runs to the end of the song and then on through the tails
//! (reverb, delay, releases) until half a second of silence, at most 15 s.

use crate::bridge::{self, Bridge, Playing};
use crate::samples::Samples;
use lyra_engine::Command;
use lyra_model::{Project, TICKS_PER_BAR};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub const SAMPLE_RATE: u32 = 48_000;
const BLOCK: usize = 256;
/// The tail ends after this long below `QUIET` (−90 dBFS), or at `MAX_TAIL_S`.
const QUIET_S: f64 = 0.5;
const QUIET: f32 = 3.2e-5;
const MAX_TAIL_S: f64 = 15.0;

/// Which part of the song a render plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Part {
    /// Everything, as you hear it.
    Mix,
    /// One channel alone (through its mixer track and the master), no audio clips.
    Channel(usize),
    /// The audio clips alone.
    Audio,
}

#[derive(Clone, Copy, Debug)]
pub struct Rendered {
    pub seconds: f64,
    /// Highest sample, 1.0 = 0 dBFS (above 1 was clipped in the file).
    pub peak: f32,
}

/// Render `part` of the song into a 24-bit WAV at `path`. `progress`, if
/// given, counts up to 1000 while the song itself renders.
pub fn render(
    project: &Project,
    samples: &mut Samples,
    path: &Path,
    part: Part,
    progress: Option<&AtomicU32>,
) -> Result<Rendered, String> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = hound::WavWriter::create(path, spec).map_err(|e| format!("can't write {}: {e}", path.display()))?;
    const FULL: f32 = 8_388_607.0; // 24-bit
    let r = render_frames(project, samples, part, progress, |frames| {
        for &s in frames {
            wav.write_sample((s.clamp(-1.0, 1.0) * FULL) as i32).map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    wav.finalize().map_err(|e| format!("can't finish {}: {e}", path.display()))?;
    Ok(r)
}

/// Render `part` of the song, handing each block of interleaved stereo frames
/// (unclipped, 1.0 = 0 dBFS) to `sink`: the core of [`render`], also used to
/// measure a mix without writing it ([`crate::report`]).
pub fn render_frames(
    project: &Project,
    samples: &mut Samples,
    part: Part,
    progress: Option<&AtomicU32>,
    mut sink: impl FnMut(&[f32]) -> Result<(), String>,
) -> Result<Rendered, String> {
    let mut p = project.clone();
    match part {
        Part::Mix => {}
        Part::Channel(i) => p.channels.iter_mut().enumerate().for_each(|(j, c)| c.mute = j != i),
        Part::Audio => p.channels.iter_mut().for_each(|c| c.mute = true),
    }
    let (mut engine, mut handle) = lyra_engine::create(SAMPLE_RATE as f32);
    Bridge::default().sync(&p, Playing::Song, &mut handle, samples);
    // The engine loops a song at its end; give it room for the tails instead.
    let mut seq = bridge::compile_song(&p);
    let song_ticks = seq.length;
    seq.length = song_ticks + (MAX_TAIL_S / p.seconds_per_tick()) as u32 + TICKS_PER_BAR;
    handle.send(Command::SetSequence(Box::new(seq)));
    if matches!(part, Part::Channel(_)) {
        handle.send(Command::SetAudio(Box::default()));
    }
    handle.send(Command::Seek(0.0));
    handle.send(Command::Play);

    let song_frames = (song_ticks as f64 * p.seconds_per_tick() * SAMPLE_RATE as f64).ceil() as usize;
    let max_frames = song_frames + (MAX_TAIL_S * SAMPLE_RATE as f64) as usize;
    let quiet_needed = (QUIET_S * SAMPLE_RATE as f64) as usize;
    let mut buf = vec![0.0f32; BLOCK * 2];
    let (mut done, mut quiet, mut peak) = (0usize, 0usize, 0.0f32);
    while done < max_frames {
        engine.process(&mut buf, 2);
        let n = BLOCK.min(max_frames - done);
        for frame in buf[..n * 2].chunks(2) {
            let level = frame[0].abs().max(frame[1].abs());
            peak = peak.max(level);
            quiet = if level < QUIET { quiet + 1 } else { 0 };
        }
        sink(&buf[..n * 2])?;
        done += n;
        handle.collect_garbage();
        if let Some(progress) = progress {
            progress.store((done.min(song_frames) * 1000 / song_frames.max(1)) as u32, Ordering::Relaxed);
        }
        if done >= song_frames && quiet >= quiet_needed {
            break;
        }
    }
    Ok(Rendered { seconds: done as f64 / SAMPLE_RATE as f64, peak })
}

/// One WAV per channel that plays in the song ("03 Bass.wav"), plus
/// "Audio clips.wav" when there are any. Returns the files written.
pub fn render_stems(
    project: &Project,
    samples: &mut Samples,
    dir: &Path,
    progress: Option<&AtomicU32>,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let seq = bridge::compile_song(project);
    let mut parts: Vec<(Part, String)> = project
        .channels
        .iter()
        .enumerate()
        .filter(|(i, _)| seq.notes.iter().any(|n| n.channel as usize == *i))
        .map(|(i, c)| (Part::Channel(i), format!("{:02} {}", i + 1, file_safe(&c.name))))
        .collect();
    if project.clips.iter().any(|c| c.audio().is_some()) {
        parts.push((Part::Audio, "Audio clips".into()));
    }
    let mut written = Vec::new();
    for (k, (part, name)) in parts.iter().enumerate() {
        let path = dir.join(format!("{name}.wav"));
        render(project, samples, &path, *part, None)?;
        if let Some(progress) = progress {
            progress.store(((k + 1) * 1000 / parts.len()) as u32, Ordering::Relaxed);
        }
        written.push(path);
    }
    Ok(written)
}

/// A channel name as a file name: no path separators or other troublemakers.
pub(crate) fn file_safe(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || " -_.()%".contains(c) { c } else { '_' }).collect();
    let s = s.trim().trim_start_matches('.').to_string();
    if s.is_empty() {
        "Channel".into()
    } else {
        s
    }
}

pub fn db(peak: f32) -> f32 {
    lyra_plugin::gain_to_db(peak)
}

/// `lyra --render <project.lyra> [<out.wav>] [--stems <folder>]`. Returns the
/// process exit code.
pub fn cli(args: &[String]) -> i32 {
    let usage = "usage: lyra --render <project.lyra> [<out.wav>] [--stems <folder>]";
    let mut stems: Option<PathBuf> = None;
    let mut files = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--stems" => match it.next() {
                Some(dir) => stems = Some(dir.into()),
                None => {
                    eprintln!("{usage}");
                    return 2;
                }
            },
            "-h" | "--help" => {
                println!("{usage}");
                return 0;
            }
            _ => files.push(PathBuf::from(a)),
        }
    }
    let Some(input) = files.first() else {
        eprintln!("{usage}");
        return 2;
    };
    let project = match crate::files::load(input) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("lyra: {e}");
            return 1;
        }
    };
    let mut samples = Samples::default();
    let started = std::time::Instant::now();
    if let Some(dir) = stems {
        return match render_stems(&project, &mut samples, &dir, None) {
            Ok(written) => {
                for f in &written {
                    println!("{}", f.display());
                }
                println!("{} stems in {:.1} s", written.len(), started.elapsed().as_secs_f64());
                0
            }
            Err(e) => {
                eprintln!("lyra: {e}");
                1
            }
        };
    }
    let out = files.get(1).cloned().unwrap_or_else(|| input.with_extension("wav"));
    match render(&project, &mut samples, &out, Part::Mix, None) {
        Ok(r) => {
            let clip = if r.peak > 1.0 { " (clipped!)" } else { "" };
            println!(
                "{}: {:.1} s, peak {:+.1} dBFS{clip}, rendered in {:.1} s",
                out.display(),
                r.seconds,
                db(r.peak),
                started.elapsed().as_secs_f64()
            );
            for e in samples.take_errors() {
                eprintln!("lyra: {e}");
            }
            0
        }
        Err(e) => {
            eprintln!("lyra: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyra_model::{Clip, Note};

    /// A one-bar kick pattern played twice, through a long reverb.
    fn song() -> Project {
        let mut p = Project::default();
        p.bpm = 120.0;
        let kick = p.add_channel("Kick", "lyra.kick", lyra_catalog::default_params("lyra.kick"));
        let hat = p.add_channel("Hat / open", "lyra.hihat", lyra_catalog::default_params("lyra.hihat"));
        p.patterns[0].notes_mut(kick).push(Note { start: 0, len: 24, key: 60, velocity: 1.0 });
        let id = p.patterns[0].id;
        p.add_clip(Clip::of_pattern(id, 0, 0, 2 * TICKS_PER_BAR));
        let _ = hat; // a channel with no notes: no stem for it
        p.mixer[1].effects = vec![lyra_model::EffectSlot::new("lyra.reverb", vec![0.95, 0.2, 1.0, 0.6])];
        p
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lyra-render-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_render_plays_the_song_once_and_keeps_its_tail() {
        let d = dir("mix");
        let path = d.join("song.wav");
        let r = render(&song(), &mut Samples::default(), &path, Part::Mix, None).unwrap();
        // Two bars at 120 = 4 s, then the reverb rings past the end, then stops.
        assert!(r.seconds > 4.2 && r.seconds < 4.0 + MAX_TAIL_S, "{} s", r.seconds);
        assert!(r.peak > 0.1 && r.peak <= 1.0, "peak {}", r.peak);
        let wav = hound::WavReader::open(&path).unwrap();
        assert_eq!((wav.spec().sample_rate, wav.spec().bits_per_sample), (SAMPLE_RATE, 24));
        // The song doesn't loop: nothing as loud as a kick in the tail.
        let samples: Vec<i32> = wav.into_samples::<i32>().map(Result::unwrap).collect();
        let tail = &samples[(4.5 * SAMPLE_RATE as f64) as usize * 2..];
        let tail_peak = tail.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0) as f32 / 8_388_607.0;
        assert!(tail_peak < r.peak * 0.5, "tail {tail_peak} vs peak {}", r.peak);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn stems_skip_silent_channels_and_have_safe_names() {
        let d = dir("stems");
        let written = render_stems(&song(), &mut Samples::default(), &d, None).unwrap();
        let names: Vec<String> = written.iter().map(|p| p.file_name().unwrap().to_string_lossy().into()).collect();
        assert_eq!(names, ["01 Kick.wav"]);
        assert_eq!(file_safe("Hat / open"), "Hat _ open");
        assert_eq!(file_safe("../.."), "_..");
        let _ = std::fs::remove_dir_all(&d);
    }
}
