//! `lyra --install-desktop`: tells the desktop who Lyra is.
//!
//! On Wayland the compositor finds a window's icon and name through a
//! `.desktop` file named after the app id (`lyra`), so without one KDE shows
//! a generic icon. This writes `~/.local/share/applications/lyra.desktop`
//! (starting this very binary) and the logo into the hicolor icon theme.
//! Run it again after moving the binary.

use std::path::Path;

const ICON_256: &[u8] = include_bytes!("../assets/icon_256.png");

/// Installs the desktop entry and icon under `data` (`$XDG_DATA_HOME`).
pub fn install(data: &Path, exe: &Path) -> std::io::Result<()> {
    let icons = data.join("icons/hicolor/256x256/apps");
    std::fs::create_dir_all(&icons)?;
    std::fs::write(icons.join("lyra.png"), ICON_256)?;
    let apps = data.join("applications");
    std::fs::create_dir_all(&apps)?;
    let exec = exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    std::fs::write(
        apps.join("lyra.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Lyra\nComment=Make music\nExec=\"{exec}\"\n\
             Icon=lyra\nTerminal=false\nCategories=AudioVideo;Audio;Music;\nStartupWMClass=lyra\n"
        ),
    )
}

/// The command line: install for this user, print where things went.
pub fn cli() -> i32 {
    let data = crate::files::xdg_data_home();
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("lyra: can't tell where I am: {e}");
            return 1;
        }
    };
    match install(&data, &exe) {
        Ok(()) => {
            println!("Installed {}", data.join("applications/lyra.desktop").display());
            println!("Lyra now has its icon (log out and in if the taskbar still shows the old one).");
            0
        }
        Err(e) => {
            eprintln!("lyra: couldn't install the desktop entry: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn writes_the_entry_and_the_icon() {
        let dir = std::env::temp_dir().join(format!("lyra-desktop-{}", std::process::id()));
        super::install(&dir, std::path::Path::new("/opt/my lyra/lyra")).unwrap();
        let entry = std::fs::read_to_string(dir.join("applications/lyra.desktop")).unwrap();
        assert!(entry.contains("Exec=\"/opt/my lyra/lyra\"\n"));
        assert!(entry.contains("Icon=lyra"));
        assert!(dir.join("icons/hicolor/256x256/apps/lyra.png").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
