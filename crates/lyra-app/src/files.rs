//! Saving, opening and backing up projects.
//!
//! * Writes are crash-safe: write to `name.lyra.tmp`, then rename over the
//!   real file. A crash mid-save can never leave a half-written project.
//! * A backup copy (`~/.local/share/lyra/backup.lyra`) is written every half
//!   minute while there are changes, on quit, and right before New/Open
//!   replace the project. So "I lost my song" can't happen, and Lyra needs no
//!   "are you sure?" pop-ups.
//! * Lyra reopens the last project you saved or opened.

use lyra_model::{file, Project};
use std::path::{Path, PathBuf};

/// `$XDG_DATA_HOME`, else `~/.local/share`.
pub fn xdg_data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share"))
}

/// `$XDG_DATA_HOME/lyra`, else `~/.local/share/lyra`.
pub fn data_dir() -> PathBuf {
    let base = xdg_data_home();
    base.join("lyra")
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn backup_path() -> PathBuf {
    data_dir().join("backup.lyra")
}

fn last_project_file() -> PathBuf {
    data_dir().join("last-project")
}

/// Where dialogs start: ~/Music/Lyra if it can exist, else home.
fn default_dir() -> PathBuf {
    let music = home().join("Music");
    let dir = music.join("Lyra");
    if music.is_dir() && std::fs::create_dir_all(&dir).is_ok() {
        dir
    } else {
        home()
    }
}

/// The folder a song file sits in, as a full path.
fn folder_of(song: &Path) -> PathBuf {
    let dir = song.parent().unwrap_or(Path::new("."));
    std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf())
}

/// Collect: copies every file the song uses from outside its folder into the
/// folder's `audio/`, and points the song at the copies, so the folder holds
/// everything. Lyra's own Starter Kit stays where it is (it ships with Lyra).
/// Returns (files copied, files missing).
pub fn collect(project: &mut Project, song: &Path) -> Result<(usize, usize), String> {
    let dir = folder_of(song);
    let audio = dir.join("audio");
    let mut moved: Vec<(String, String)> = Vec::new();
    let (mut copied, mut missing) = (0, 0);
    for name in project.file_names_mut() {
        if crate::samples::is_bundled(name) || Path::new(name.as_str()).starts_with(&dir) {
            continue;
        }
        if let Some((_, new)) = moved.iter().find(|(old, _)| old == name) {
            *name = new.clone();
            continue;
        }
        let from = PathBuf::from(name.as_str());
        if !from.is_file() {
            missing += 1;
            continue;
        }
        std::fs::create_dir_all(&audio).map_err(|e| format!("can't create {}: {e}", audio.display()))?;
        let to = free_name(&audio, &from);
        if !to.exists() {
            std::fs::copy(&from, &to).map_err(|e| format!("can't copy {}: {e}", from.display()))?;
            copied += 1;
        }
        let new = to.to_string_lossy().into_owned();
        moved.push((name.clone(), new.clone()));
        *name = new;
    }
    Ok((copied, missing))
}

/// Where `file` goes in `folder`: its own name, or "name 2", "name 3"… if a
/// different file already has it (the same file again is reused).
fn free_name(folder: &Path, file: &Path) -> PathBuf {
    let stem = file.file_stem().map_or("sound".into(), |s| s.to_string_lossy().into_owned());
    let ext = file.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let same = |a: &Path| std::fs::read(a).ok() == std::fs::read(file).ok();
    for n in 1.. {
        let name = if n == 1 { format!("{stem}{ext}") } else { format!("{stem} {n}{ext}") };
        let at = folder.join(name);
        if !at.exists() || same(&at) {
            return at;
        }
    }
    unreachable!()
}

pub fn save(path: &Path, project: &Project) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    }
    // File names relative to the song where they can be (see `samples`).
    let mut stored = project.clone();
    let dir = folder_of(path);
    for name in stored.file_names_mut() {
        *name = crate::samples::for_saving(name, &dir);
    }
    let tmp = path.with_extension("lyra.tmp");
    std::fs::write(&tmp, file::to_json(&stored)).map_err(|e| format!("can't write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("can't save {}: {e}", path.display()))
}

pub fn load(path: &Path) -> Result<Project, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    let mut project = file::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = folder_of(path);
    for name in project.file_names_mut() {
        *name = crate::samples::from_saved(name, &dir);
    }
    fill_new_params(&mut project);
    Ok(project)
}

/// Parameters a plugin gained after the project was saved get their
/// defaults. Ids are forever and new ones go at the end, so the saved values
/// still line up; this only adds the missing tail (and editors can index
/// every parameter the plugin has).
pub fn fill_new_params(project: &mut Project) {
    fn fill(params: &mut Vec<f32>, defaults: Vec<f32>) {
        if params.len() < defaults.len() {
            let n = params.len();
            params.extend_from_slice(&defaults[n..]);
        }
    }
    for ch in &mut project.channels {
        fill(&mut ch.params, lyra_catalog::default_params(&ch.instrument));
    }
    for slot in project.mixer.iter_mut().flat_map(|t| &mut t.effects) {
        fill(&mut slot.params, lyra_catalog::effects::default_params(&slot.effect));
    }
}

pub fn remember(path: &Path) {
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(last_project_file(), path.to_string_lossy().as_bytes());
}

pub fn last_project() -> Option<PathBuf> {
    let p = PathBuf::from(std::fs::read_to_string(last_project_file()).ok()?.trim());
    p.is_file().then_some(p)
}

pub fn backup(project: &Project) {
    if let Err(e) = save(&backup_path(), project) {
        eprintln!("lyra: backup failed: {e}");
    }
}

/// Native "Open" dialog (through the desktop portal). Blocks the UI while
/// open; the audio thread keeps playing.
pub fn pick_open() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Open project")
        .add_filter("Lyra project", &[file::EXTENSION])
        .set_directory(default_dir())
        .pick_file()
}

pub fn pick_save(suggested: &str) -> Option<PathBuf> {
    let path = rfd::FileDialog::new()
        .set_title("Save project")
        .add_filter("Lyra project", &[file::EXTENSION])
        .set_directory(default_dir())
        .set_file_name(format!("{suggested}.{}", file::EXTENSION))
        .save_file()?;
    Some(if path.extension().is_some_and(|e| e == file::EXTENSION) {
        path
    } else {
        path.with_extension(file::EXTENSION)
    })
}

/// "Export WAV" dialog: where the render goes.
pub fn pick_export(suggested: &str) -> Option<PathBuf> {
    let path = rfd::FileDialog::new()
        .set_title("Export WAV")
        .add_filter("WAV audio", &["wav"])
        .set_directory(default_dir())
        .set_file_name(format!("{suggested}.wav"))
        .save_file()?;
    Some(if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) {
        path
    } else {
        path.with_extension("wav")
    })
}

/// "Export stems" dialog: the folder the stems go into.
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).set_directory(default_dir()).pick_folder()
}

/// The folders you added to the browser (one path per line in
/// `~/.local/share/lyra/folders`).
pub fn browser_folders() -> Vec<PathBuf> {
    std::fs::read_to_string(data_dir().join("folders"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

pub fn set_browser_folders(folders: &[PathBuf]) {
    let text: String = folders.iter().map(|f| format!("{}\n", f.display())).collect();
    let _ = std::fs::create_dir_all(data_dir());
    let _ = std::fs::write(data_dir().join("folders"), text);
}

/// "My Song" for `/…/My Song.lyra`.
pub fn display_name(path: Option<&Path>) -> String {
    path.and_then(|p| p.file_stem()).map_or("Untitled".into(), |s| s.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_copies_outside_files_in_and_the_song_moves_with_them() {
        let root = std::env::temp_dir().join(format!("lyra-collect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (song_dir, voices) = (root.join("Songs/My Song"), root.join("Voices"));
        std::fs::create_dir_all(&song_dir).unwrap();
        std::fs::create_dir_all(voices.join("b")).unwrap();
        std::fs::write(voices.join("line.wav"), b"one").unwrap();
        std::fs::write(voices.join("b/line.wav"), b"two").unwrap(); // same name, other sound
        let mut p = Project::default();
        let a = voices.join("line.wav").to_string_lossy().into_owned();
        p.add_audio("a", &a, 1.0);
        p.add_audio("a again", &a, 1.0);
        p.add_audio("b", &voices.join("b/line.wav").to_string_lossy(), 1.0);
        p.add_audio("gone", "/nowhere/lost.wav", 1.0);
        p.add_audio("kit", "bundled:Starter Kit/Kicks/bd_haus.flac", 1.0);
        let song = song_dir.join("My Song.lyra");
        assert_eq!(collect(&mut p, &song).unwrap(), (2, 1));
        let names: Vec<_> = p.audio.iter().map(|a| a.sample.clone()).collect();
        let audio = song_dir.join("audio");
        assert_eq!(names[0], audio.join("line.wav").to_string_lossy());
        assert_eq!(names[1], names[0], "one copy for both");
        assert_eq!(names[2], audio.join("line 2.wav").to_string_lossy());
        assert_eq!(
            (names[3].as_str(), names[4].as_str()),
            ("/nowhere/lost.wav", "bundled:Starter Kit/Kicks/bd_haus.flac")
        );
        assert_eq!(collect(&mut p, &song).unwrap(), (0, 1), "a second time, nothing to do");
        // Saved, the song folder moves somewhere else and still finds its sounds.
        save(&song, &p).unwrap();
        let text = std::fs::read_to_string(&song).unwrap();
        assert!(text.contains("project:audio/line 2.wav"), "stored relative");
        let moved = root.join("Elsewhere");
        std::fs::rename(&song_dir, &moved).unwrap();
        let back = load(&moved.join("My Song.lyra")).unwrap();
        assert_eq!(back.audio[2].sample, moved.join("audio/line 2.wav").to_string_lossy());
        assert_eq!(std::fs::read(&back.audio[2].sample).unwrap(), b"two");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn older_projects_get_new_parameters_at_their_defaults() {
        let mut p = Project::default();
        let full = lyra_catalog::default_params("lyra.fluttersine");
        let mut old = full[..154].to_vec(); // Fluttersine as first ported
        old[0] = 8.0; // a saved value
        p.add_channel("Lead", "lyra.fluttersine", old);
        p.mixer[1].effects.push(lyra_model::EffectSlot::new("lyra.delay", vec![1.0]));
        fill_new_params(&mut p);
        let params = &p.channels[0].params;
        assert_eq!(params.len(), full.len());
        assert_eq!(params[0], 8.0, "saved values stay");
        assert_eq!(params[154..], full[154..]);
        let delay = &p.mixer[1].effects[0].params;
        assert_eq!(delay.len(), lyra_catalog::effects::default_params("lyra.delay").len());
        assert_eq!(delay[0], 1.0);
    }
}
