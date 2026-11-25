//! Presets: each plugin's factory ones (from its crate) and yours, saved as
//! small files in Lyra's data folder:
//! `<data dir>/presets/<plugin id>/<name>.lyrapreset`, readable JSON
//! (`{ format, version, plugin, name, params }`). The parameters are stored
//! by id, like in a project; a preset from an older Lyra gets the
//! parameters added since at their defaults.
//!
//! [`picker`] is Lyra's own preset dropdown; plugin editors with their own
//! look use the kit's list (`lyra_kit::presets::menu`) through their `Host`.

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

const EXT: &str = "lyrapreset";

/// One preset: the kit's type, so plugin editors and Lyra share it.
pub use lyra_kit::PresetEntry as Entry;

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    plugin: String,
    name: String,
    params: Vec<f32>,
}

fn dir(plugin: &str) -> PathBuf {
    crate::files::data_dir().join("presets").join(plugin)
}

/// A name as a file name: no path separators or other troublemakers.
fn file_name(name: &str) -> String {
    let clean: String =
        name.chars().map(|c| if c.is_alphanumeric() || " -_.()'&+".contains(c) { c } else { '_' }).collect();
    format!("{}.{EXT}", clean.trim().trim_start_matches('.'))
}

/// Your presets, read from disk once and kept until one is saved or deleted.
static CACHE: Mutex<Option<HashMap<String, Vec<Entry>>>> = Mutex::new(None);

fn read_user(plugin: &str) -> Vec<Entry> {
    let defaults = lyra_catalog::default_params(plugin);
    let mut out: Vec<Entry> = std::fs::read_dir(dir(plugin))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == EXT))
        .filter_map(|e| serde_json::from_str::<File>(&std::fs::read_to_string(e.path()).ok()?).ok())
        .filter(|f| f.format == "lyra-preset" && f.plugin == plugin)
        .map(|f| {
            // Parameters added since the preset was saved keep their defaults.
            let mut params = defaults.clone();
            for (slot, v) in params.iter_mut().zip(f.params) {
                *slot = v;
            }
            Entry { name: f.name, params, user: true }
        })
        .collect();
    out.sort_by_key(|e| e.name.to_lowercase());
    out
}

fn user(plugin: &str) -> Vec<Entry> {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.get_or_insert_with(HashMap::new).entry(plugin.to_string()).or_insert_with(|| read_user(plugin)).clone()
}

fn forget(plugin: &str) {
    if let Some(c) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        c.remove(plugin);
    }
}

/// Every preset of a plugin: its own first, then yours.
pub fn entries(plugin: &str) -> Vec<Entry> {
    let mut v: Vec<Entry> = lyra_catalog::presets_for(plugin)
        .into_iter()
        .map(|p| Entry { name: p.name.to_string(), params: p.params(), user: false })
        .collect();
    v.extend(user(plugin));
    v
}

/// The preset these parameters are exactly, if any.
pub use lyra_kit::presets::current;

/// Save `params` as your preset `name` (replacing one of the same name).
pub fn save(plugin: &str, name: &str, params: &[f32]) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A preset needs a name.".into());
    }
    let d = dir(plugin);
    std::fs::create_dir_all(&d).map_err(|e| format!("Can't make {}: {e}", d.display()))?;
    let file = File {
        format: "lyra-preset".into(),
        version: 1,
        plugin: plugin.into(),
        name: name.into(),
        params: params.to_vec(),
    };
    let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    let path = d.join(file_name(name));
    // Crash-safe: write beside it, then swap it in.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)
        .and_then(|_| std::fs::rename(&tmp, &path))
        .map_err(|e| format!("Can't save the preset: {e}"))?;
    forget(plugin);
    Ok(())
}

/// Delete your preset `name`.
pub fn delete(plugin: &str, name: &str) {
    let _ = std::fs::remove_file(dir(plugin).join(file_name(name)));
    forget(plugin);
}

/// What [`picker`] asks for.
pub enum Picked {
    Load(Entry),
}

/// Lyra's own preset picker (the instrument window; Fluttersine and Chrysalis
/// use the kit's list in their own look): a dropdown of the plugin's presets and
/// yours, with a second column for "Save as…" and, when the sound is one of
/// yours, deleting it. "Save as…" turns the dropdown into a name field in
/// place: Enter saves, Esc gives up.
pub fn picker(ui: &mut Ui, plugin: &str, params: &[f32], width: f32) -> Option<Picked> {
    use crate::widgets::{self, DropRow};
    let id = egui::Id::new(("preset-picker", plugin));
    let saving: Option<String> = ui.ctx().data(|d| d.get_temp(id));
    if let Some(mut name) = saving {
        let resp = widgets::text_field(ui, &mut name, width, ("preset-name", plugin), "New preset name  ·  Enter saves");
        let first = ui.ctx().data(|d| d.get_temp::<bool>(id.with("focused"))).is_none();
        if first {
            resp.request_focus();
            ui.ctx().data_mut(|d| d.insert_temp(id.with("focused"), true));
        }
        let done = resp.lost_focus();
        let enter = done && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter && !name.trim().is_empty() {
            match save(plugin, name.trim(), params) {
                Ok(()) => ui.ctx().data_mut(|d| d.remove::<String>(id.with("error"))),
                Err(e) => ui.ctx().data_mut(|d| d.insert_temp(id.with("error"), e)),
            }
        }
        if done {
            ui.ctx().data_mut(|d| {
                d.remove::<String>(id);
                d.remove::<bool>(id.with("focused"));
            });
        } else {
            ui.ctx().data_mut(|d| d.insert_temp(id, name));
        }
        return None;
    }
    if let Some(err) = ui.ctx().data(|d| d.get_temp::<String>(id.with("error"))) {
        ui.label(RichText::new(err).small().color(crate::theme::ROSE));
    }
    let all = entries(plugin);
    let now = current(&all, params);
    // Rows: the presets (yours under a heading), then the actions column.
    let mut rows = Vec::new();
    let mut what: Vec<Option<usize>> = Vec::new(); // preset index per row
    let mut yours = false;
    for (k, e) in all.iter().enumerate() {
        if e.user && !yours {
            yours = true;
            rows.push(DropRow::heading("Yours"));
            what.push(None);
        }
        rows.push(DropRow::text(e.name.as_str()));
        what.push(Some(k));
    }
    let save_row = rows.len();
    rows.push(DropRow::text("Save as…").column());
    what.push(None);
    let delete_row = now.filter(|&k| all[k].user).map(|k| {
        rows.push(DropRow::text(format!("Delete \"{}\"", all[k].name)));
        what.push(None);
        (rows.len() - 1, k)
    });
    let chosen = now.and_then(|k| what.iter().position(|w| *w == Some(k))).unwrap_or(usize::MAX);
    let hint = "Presets: the instrument's own and yours";
    let picked = widgets::dropdown(ui, ("presets", plugin), width, &rows, chosen, true, hint)?;
    if picked == save_row {
        ui.ctx().data_mut(|d| d.insert_temp(id, String::new()));
        return None;
    }
    if let Some((row, k)) = delete_row {
        if picked == row {
            delete(plugin, &all[k].name);
            return None;
        }
    }
    what[picked].map(|k| Picked::Load(all[k].clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_preset_comes_back_and_goes_away() {
        let tmp = std::env::temp_dir().join(format!("lyra-presets-test-{}", std::process::id()));
        std::env::set_var("XDG_DATA_HOME", &tmp);
        let plugin = "lyra.chrysalis";
        let mut params = lyra_catalog::default_params(plugin);
        params[3] = 5.0; // osc 1 octave +2
        save(plugin, "My / weird: name", &params).unwrap();
        let all = entries(plugin);
        let mine = all.iter().find(|e| e.user).expect("listed");
        assert_eq!(mine.name, "My / weird: name");
        assert_eq!(mine.params, params);
        assert_eq!(current(&all, &params).map(|k| all[k].user), Some(true));
        // An older file with fewer parameters gets the rest at their defaults.
        let short = File {
            format: "lyra-preset".into(),
            version: 1,
            plugin: plugin.into(),
            name: "Old".into(),
            params: params[..10].to_vec(),
        };
        std::fs::write(dir(plugin).join("Old.lyrapreset"), serde_json::to_string(&short).unwrap()).unwrap();
        forget(plugin);
        let old = entries(plugin).into_iter().find(|e| e.name == "Old").unwrap();
        assert_eq!(old.params.len(), params.len());
        assert_eq!(old.params[3], 5.0);
        delete(plugin, "My / weird: name");
        delete(plugin, "Old");
        assert!(entries(plugin).iter().all(|e| !e.user));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
