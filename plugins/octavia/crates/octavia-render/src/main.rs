//! `octavia` — render a score (or hand-edited curves) to sound and pictures.

mod analyze;
mod plot;
mod reverb;

use octavia_core::{perform, render, Curves, Instrument, Score, Style};
use std::path::{Path, PathBuf};
use std::time::Instant;

const USAGE: &str = "\
usage: octavia <score.txt | curves.csv>... [options]
       octavia analyze <file.wav>...      measure any recording the same way

  -o DIR             where to write (default: renders/)
  --instrument NAME  cello (default) or violin
  --expression X     0 = deadpan, 1 = normal, 2 = a lot
  --vibrato X        vibrato depth scale (default 1)
  --breath X         steady bow noise under the tone (default 1; try 0.5 .. 3)
  --seed N           a different take of the same performance
  --dry              no room reverb
  --sr N             sample rate (default 48000)

writes NAME.wav, NAME.curves.csv, NAME.curves.png, NAME.spec.png
a .csv input skips the performer and plays the curves as written";

struct Args {
    inputs: Vec<PathBuf>,
    out: PathBuf,
    instrument: String,
    style: Style,
    dry: bool,
    sr: f32,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        inputs: Vec::new(),
        out: PathBuf::from("renders"),
        instrument: "cello".into(),
        style: Style::default(),
        dry: false,
        sr: 48_000.0,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => return Err(String::new()),
            "-o" => a.out = val("-o")?.into(),
            "--instrument" => a.instrument = val("--instrument")?,
            "--expression" => a.style.expression = val("--expression")?.parse().map_err(|_| "bad --expression")?,
            "--vibrato" => a.style.vibrato = val("--vibrato")?.parse().map_err(|_| "bad --vibrato")?,
            "--breath" => a.style.breath = val("--breath")?.parse().map_err(|_| "bad --breath")?,
            "--seed" => a.style.seed = val("--seed")?.parse().map_err(|_| "bad --seed")?,
            "--sr" => a.sr = val("--sr")?.parse().map_err(|_| "bad --sr")?,
            "--dry" => a.dry = true,
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            s => a.inputs.push(s.into()),
        }
    }
    if a.inputs.is_empty() {
        return Err(String::new());
    }
    Ok(a)
}

fn main() {
    let all: Vec<String> = std::env::args().collect();
    if all.get(1).is_some_and(|a| a == "analyze") {
        let mut failed = false;
        for p in &all[2..] {
            if let Err(e) = analyze_file(Path::new(p)) {
                eprintln!("{p}: {e}");
                failed = true;
            }
        }
        std::process::exit(failed as i32);
    }
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}\n");
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    let Some(inst) = Instrument::by_name(&args.instrument) else {
        eprintln!("no instrument called {:?} (try cello or violin)", args.instrument);
        std::process::exit(2);
    };
    if let Err(e) = std::fs::create_dir_all(&args.out) {
        eprintln!("can't create {}: {e}", args.out.display());
        std::process::exit(1);
    }
    let mut failed = false;
    for input in &args.inputs {
        if let Err(e) = run(input, &args, &inst) {
            eprintln!("{}: {e}", input.display());
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}

fn run(input: &Path, args: &Args, inst: &Instrument) -> Result<(), String> {
    let text = std::fs::read_to_string(input).map_err(|e| e.to_string())?;
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("out").trim_end_matches(".curves").to_string();
    let is_curves = input.extension().is_some_and(|e| e == "csv");

    let t0 = Instant::now();
    let (score, curves) = if is_curves {
        (None, Curves::from_csv(&text)?)
    } else {
        let score = Score::parse(&text)?;
        let curves = perform(&score, inst, &args.style);
        (Some(score), curves)
    };
    let t_perform = t0.elapsed();
    let t1 = Instant::now();
    let mono = render(&curves, inst, args.sr, args.style.seed);
    let t_render = t1.elapsed();

    // Room, then a fixed gain so loudness differences between renders are real.
    let mut left = Vec::with_capacity(mono.len());
    let mut right = Vec::with_capacity(mono.len());
    let mut room = reverb::Reverb::new(args.sr, 1.9);
    for &x in &mono {
        let (wl, wr) = if args.dry { (0.0, 0.0) } else { room.tick(x) };
        left.push(0.9 * x + 0.16 * wl);
        right.push(0.9 * x + 0.16 * wr);
    }
    let peak = left.iter().chain(&right).fold(0.0f32, |m, v| m.max(v.abs()));
    let gain = if peak > 0.89 { 0.89 / peak } else { 1.0 };

    let base = args.out.join(&stem);
    let wav = base.with_extension("wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: args.sr as u32,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&wav, spec).map_err(|e| e.to_string())?;
    for (l, r) in left.iter().zip(&right) {
        for v in [l, r] {
            w.write_sample(((v * gain).clamp(-1.0, 1.0) * 8_388_607.0) as i32).map_err(|e| e.to_string())?;
        }
    }
    w.finalize().map_err(|e| e.to_string())?;
    if !is_curves {
        std::fs::write(with_suffix(&base, ".curves.csv"), curves.to_csv()).map_err(|e| e.to_string())?;
    }
    plot::curves_png(&curves, score.as_ref(), &with_suffix(&base, ".curves.png")).map_err(|e| e.to_string())?;
    plot::spectrogram_png(&mono, args.sr, &with_suffix(&base, ".spec.png")).map_err(|e| e.to_string())?;

    let secs = mono.len() as f64 / args.sr as f64;
    let r = analyze::report(&mono, args.sr);
    println!("{}", octave_line(&r));
    println!(
        "{stem}: {secs:.1} s, {} notes | perform {:.1} ms, sound {:.0} ms = {:.0}x real time | \
         dry peak {:+.1} dBFS, rms {:+.1} dB{} | centroid {:.0} Hz, presence(2-5k/0.1-1k) {:+.1} dB, air(8-16k/1-4k) {:+.1} dB",
        score.as_ref().map_or(0, |s| s.notes.len()),
        t_perform.as_secs_f64() * 1e3,
        t_render.as_secs_f64() * 1e3,
        secs / t_render.as_secs_f64().max(1e-9),
        r.peak_db,
        r.rms_db,
        if gain < 1.0 { format!(", wav trimmed {:+.1} dB", 20.0 * gain.log10()) } else { String::new() },
        r.centroid_hz,
        r.presence_db,
        r.air_db,
    );
    Ok(())
}

fn with_suffix(base: &Path, suffix: &str) -> PathBuf {
    let mut s = base.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn octave_line(r: &analyze::Report) -> String {
    let names = ["63", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];
    let cells: Vec<String> = names.iter().zip(r.octaves).map(|(n, v)| format!("{n}:{v:+.0}")).collect();
    format!("  octaves (dB) {} | noise floor {:+.1} dB", cells.join(" "), r.floor_db)
}

/// Measures a recording the same way renders are measured, and draws its spectrogram.
fn analyze_file(path: &Path) -> Result<(), String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let ch = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 * scale).collect()
        }
    };
    let mono: Vec<f32> = interleaved.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect();
    let sr = spec.sample_rate as f32;
    let r = analyze::report(&mono, sr);
    let out = with_suffix(&path.with_extension(""), ".spec.png");
    plot::spectrogram_png(&mono, sr, &out).map_err(|e| e.to_string())?;
    println!(
        "{}: {:.1} s | peak {:+.1} dBFS, rms {:+.1} dB | centroid {:.0} Hz, presence(2-5k/0.1-1k) {:+.1} dB, air(8-16k/1-4k) {:+.1} dB\n{}\n  spectrogram: {}",
        path.display(),
        mono.len() as f32 / sr,
        r.peak_db,
        r.rms_db,
        r.centroid_hz,
        r.presence_db,
        r.air_db,
        octave_line(&r),
        out.display()
    );
    Ok(())
}
