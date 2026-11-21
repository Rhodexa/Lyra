//! Samples: the library on disk, and decoded audio ready to play.
//!
//! **Library.** Two roots, shown in the browser:
//! * Lyra's own *Starter Kit* (CC0, see its CREDITS.md), shipped in
//!   `assets/samples` (override the location with `LYRA_ASSETS`);
//! * your library folder, `$XDG_DATA_HOME/lyra/library` (usually
//!   `~/.local/share/lyra/library`): drop folders of samples there.
//!
//! **Names in projects.** A sample from Lyra's own library is saved as
//! `bundled:<path inside assets/samples>`, so projects don't break when Lyra
//! is installed somewhere else; anything else is saved as its full path.
//!
//! **Decoding** (WAV via `hound`, FLAC via `claxon`) happens on the UI thread,
//! once per file; the result is shared with the audio thread through an `Arc`.

use lyra_plugin::SampleData;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const BUNDLED: &str = "bundled:";

/// Where Lyra's own samples live.
pub fn bundled_root() -> PathBuf {
    std::env::var_os("LYRA_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets")))
        .join("samples")
}

/// Your own library folder.
pub fn user_root() -> PathBuf {
    crate::files::data_dir().join("library")
}

/// The name a project stores for a sample file.
pub fn name_for(path: &Path) -> String {
    match path.strip_prefix(bundled_root()) {
        Ok(rel) => format!("{BUNDLED}{}", rel.to_string_lossy()),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// The file a stored sample name points to.
pub fn path_for(name: &str) -> PathBuf {
    match name.strip_prefix(BUNDLED) {
        Some(rel) => bundled_root().join(rel),
        None => PathBuf::from(name),
    }
}

/// A sample's display name: its file name without the extension.
pub fn display_name(name: &str) -> String {
    Path::new(name).file_stem().map_or_else(|| name.to_string(), |s| s.to_string_lossy().into_owned())
}

pub fn is_audio(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "wav" | "flac"))
}

/// Decoded samples, by stored name. Failed loads are remembered too, so a
/// missing file is reported once instead of every frame.
#[derive(Default)]
pub struct Samples {
    cache: HashMap<String, Result<Arc<SampleData>, String>>,
    peaks: HashMap<String, Arc<Peaks>>,
    /// Failures not shown to the user yet.
    new_errors: Vec<String>,
}

impl Samples {
    pub fn get(&mut self, name: &str) -> Result<Arc<SampleData>, String> {
        if let Some(found) = self.cache.get(name) {
            return found.clone();
        }
        let result = decode(&path_for(name)).map(Arc::new);
        if let Err(e) = &result {
            self.new_errors.push(e.clone());
        }
        self.cache.insert(name.to_string(), result.clone());
        result
    }

    /// Drop a sample from the cache, so the next `get` reads the file again.
    pub fn forget(&mut self, name: &str) {
        self.cache.remove(name);
        self.peaks.remove(name);
    }

    /// The sample's waveform summary (built on first use, then kept).
    pub fn peaks(&mut self, name: &str) -> Option<Arc<Peaks>> {
        if let Some(p) = self.peaks.get(name) {
            return Some(p.clone());
        }
        let peaks = Arc::new(Peaks::new(self.get(name).ok()?));
        self.peaks.insert(name.to_string(), peaks.clone());
        Some(peaks)
    }

    /// Loading errors since the last call (each file reported once).
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.new_errors)
    }
}

/// A waveform summary for drawing: the lowest and highest sample (of both
/// channels) in every block of frames, at two block sizes, so drawing a
/// long voice take zoomed out never walks millions of samples.
pub struct Peaks {
    pub data: Arc<SampleData>,
    fine: Vec<(f32, f32)>,
    coarse: Vec<(f32, f32)>,
}

const FINE: usize = 64;
const COARSE: usize = 4096;

impl Peaks {
    pub fn new(data: Arc<SampleData>) -> Self {
        let frames = data.left.len().min(data.right.len());
        let fine: Vec<(f32, f32)> = (0..frames.div_ceil(FINE))
            .map(|b| {
                let r = b * FINE..((b + 1) * FINE).min(frames);
                data.left[r.clone()]
                    .iter()
                    .chain(&data.right[r])
                    .fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)))
            })
            .collect();
        let per = COARSE / FINE;
        let coarse = fine
            .chunks(per)
            .map(|c| c.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &(a, b)| (lo.min(a), hi.max(b))))
            .collect();
        Self { data, fine, coarse }
    }

    pub fn frames(&self) -> usize {
        self.data.left.len().min(self.data.right.len())
    }

    pub fn sample_rate(&self) -> f32 {
        self.data.sample_rate
    }

    /// Lowest and highest sample between frames `a` and `b` (fractional), or
    /// None outside the file.
    pub fn range(&self, a: f64, b: f64) -> Option<(f32, f32)> {
        let frames = self.frames();
        let (a, b) = (a.max(0.0), b.min(frames as f64));
        if a >= frames as f64 || b <= 0.0 || b < a {
            return None;
        }
        let (i0, i1) = (a as usize, (b.ceil() as usize).max(a as usize + 1).min(frames));
        let span = i1 - i0;
        let fold = |v: &[(f32, f32)]| v.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &(x, y)| (lo.min(x), hi.max(y)));
        let (lo, hi) = if span >= COARSE * 2 {
            let (c0, c1) = (i0.div_ceil(COARSE), i1 / COARSE);
            let mid = fold(&self.coarse[c0..c1]);
            let head = fold(&self.fine[i0 / FINE..(c0 * COARSE / FINE).max(i0 / FINE)]);
            let tail =
                fold(&self.fine[(c1 * COARSE / FINE).min(self.fine.len())..i1.div_ceil(FINE).min(self.fine.len())]);
            (mid.0.min(head.0).min(tail.0), mid.1.max(head.1).max(tail.1))
        } else if span >= FINE * 2 {
            fold(&self.fine[i0 / FINE..i1.div_ceil(FINE).min(self.fine.len())])
        } else {
            let d = &self.data;
            (i0..i1).fold((f32::MAX, f32::MIN), |(lo, hi), i| {
                (lo.min(d.left[i]).min(d.right[i]), hi.max(d.left[i]).max(d.right[i]))
            })
        };
        Some((lo, hi))
    }
}

thread_local! {
    static RELOAD: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Ask for a sample to be read again (its file was just rewritten, e.g. by
/// Chrysalis's table studio): the app forgets it and remakes the channels.
pub fn reload(name: &str) {
    RELOAD.with(|r| r.borrow_mut().push(name.to_string()));
}

/// The samples asked to be read again since the last call. UI thread.
pub fn take_reloads() -> Vec<String> {
    RELOAD.with(|r| std::mem::take(&mut *r.borrow_mut()))
}

/// Read a WAV or FLAC file into float samples (-1..1), left and right.
pub fn decode(path: &Path) -> Result<SampleData, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let shown = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let (rate, channels, interleaved) = match ext.as_str() {
        "wav" => decode_wav(path),
        "flac" => decode_flac(path),
        _ => Err("not a WAV or FLAC file".to_string()),
    }
    .map_err(|e| format!("{shown}: {e}"))?;
    let ch = channels.max(1);
    let frames = interleaved.len() / ch;
    let left: Vec<f32> = (0..frames).map(|i| interleaved[i * ch]).collect();
    let right: Vec<f32> = if ch > 1 { (0..frames).map(|i| interleaved[i * ch + 1]).collect() } else { left.clone() };
    Ok(SampleData { sample_rate: rate as f32, left, right })
}

fn decode_wav(path: &Path) -> Result<(u32, usize, Vec<f32>), String> {
    let mut r = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = r.spec();
    let data: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>().map_err(|e| e.to_string())?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        }
    };
    Ok((spec.sample_rate, spec.channels as usize, data))
}

fn decode_flac(path: &Path) -> Result<(u32, usize, Vec<f32>), String> {
    let mut r = claxon::FlacReader::open(path).map_err(|e| e.to_string())?;
    let info = r.streaminfo();
    let scale = 1.0 / (1i64 << (info.bits_per_sample - 1)) as f32;
    let data = r.samples().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    Ok((info.sample_rate, info.channels as usize, data))
}

/// A folder in the library, as the browser shows it: subfolders first, then
/// audio files, both sorted. Hidden entries are skipped.
pub struct Folder {
    pub name: String,
    pub path: PathBuf,
    pub folders: Vec<Folder>,
    pub files: Vec<PathBuf>,
}

pub fn scan(path: &Path, name: &str) -> Option<Folder> {
    let entries = std::fs::read_dir(path).ok()?;
    let (mut folders, mut files) = (Vec::new(), Vec::new());
    for e in entries.flatten() {
        let p = e.path();
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if let Some(f) = scan(&p, &e.file_name().to_string_lossy()) {
                folders.push(f);
            }
        } else if is_audio(&p) {
            files.push(p);
        }
    }
    folders.sort_by_key(|f| f.name.to_lowercase());
    files.sort();
    Some(Folder { name: name.to_string(), path: path.to_path_buf(), folders, files })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_starter_kit_decodes() {
        let kit = scan(&bundled_root().join("Starter Kit"), "Starter Kit").expect("kit folder");
        let mut n = 0;
        let mut all = vec![&kit];
        while let Some(f) = all.pop() {
            all.extend(f.folders.iter());
            for file in &f.files {
                let s = decode(file).unwrap_or_else(|e| panic!("{e}"));
                assert!(s.len() > 100 && s.sample_rate >= 8_000.0, "{}", file.display());
                assert!(s.left.iter().chain(&s.right).all(|x| x.is_finite() && x.abs() <= 1.0001));
                n += 1;
            }
        }
        assert_eq!(n, 65, "every one-shot in the kit");
    }

    #[test]
    fn peaks_find_the_loudest_sample_at_any_zoom() {
        let mut left = vec![0.0f32; 100_000];
        left[70_123] = 0.9;
        left[70_200] = -0.7;
        let right = left.clone();
        let p = Peaks::new(Arc::new(SampleData { sample_rate: 48_000.0, left, right }));
        assert_eq!(p.range(0.0, 100_000.0), Some((-0.7, 0.9)), "zoomed all the way out");
        assert_eq!(p.range(70_000.0, 70_300.0), Some((-0.7, 0.9)), "in the fine blocks");
        assert_eq!(p.range(70_123.0, 70_124.0), Some((0.9, 0.9)), "one sample");
        assert_eq!(p.range(0.0, 1_000.0), Some((0.0, 0.0)));
        assert_eq!(p.range(200_000.0, 300_000.0), None);
    }

    #[test]
    fn bundled_names_round_trip() {
        let p = bundled_root().join("Starter Kit/Kicks/bd_haus.flac");
        let name = name_for(&p);
        assert_eq!(name, "bundled:Starter Kit/Kicks/bd_haus.flac");
        assert_eq!(path_for(&name), p);
        assert_eq!(display_name(&name), "bd_haus");
    }
}

#[cfg(test)]
mod sampler_tests {
    use super::*;
    use lyra_plugin::{Event, ProcessContext, TimedEvent};

    /// The real kick through the real sampler, the way the app builds it.
    #[test]
    fn a_kit_kick_through_the_sampler_is_loud() {
        let data = Arc::new(decode(&path_for("bundled:Starter Kit/Kicks/bd_boom.flac")).unwrap());
        let peak_in = data.left.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        let mut s = lyra_catalog::create("lyra.sampler").unwrap();
        for (i, v) in lyra_catalog::default_params("lyra.sampler").into_iter().enumerate() {
            s.set_param(i as u32, v);
        }
        s.set_sample(data);
        s.activate(48_000.0);
        let (mut l, mut r) = (vec![0.0; 256], vec![0.0; 256]);
        let on = [TimedEvent { offset: 0, event: Event::NoteOn { key: 48, velocity: 0.8 } }];
        let mut peak = 0.0f32;
        for block in 0..40 {
            let ev: &[TimedEvent] = if block == 0 { &on } else { &[] };
            s.process(ProcessContext { sample_rate: 48_000.0, events: ev, out_l: &mut l, out_r: &mut r });
            peak = l.iter().fold(peak, |m, x| m.max(x.abs()));
        }
        println!("file peak {peak_in}, sampler peak {peak}");
        assert!(peak > 0.1 * peak_in, "file peak {peak_in}, sampler peak {peak}");
    }
}
