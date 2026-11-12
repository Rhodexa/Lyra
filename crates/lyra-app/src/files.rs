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

/// `$XDG_DATA_HOME/lyra`, else `~/.local/share/lyra`.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share"));
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

pub fn save(path: &Path, project: &Project) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("lyra.tmp");
    std::fs::write(&tmp, file::to_json(project)).map_err(|e| format!("can't write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("can't save {}: {e}", path.display()))
}

pub fn load(path: &Path) -> Result<Project, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("can't read {}: {e}", path.display()))?;
    file::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))
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

/// "My Song" for `/…/My Song.lyra`.
pub fn display_name(path: Option<&Path>) -> String {
    path.and_then(|p| p.file_stem()).map_or("Untitled".into(), |s| s.to_string_lossy().into_owned())
}
