//! Presets: each plugin's factory ones (from its crate) and yours, saved as
//! small files in Lyra's data folder:
//! `<data dir>/presets/<plugin id>/<name>.lyrapreset`, readable JSON
//! (`{ format, version, plugin, name, params }`). The parameters are stored
//! by id, like in a project; a preset from an older Lyra gets the
//! parameters added since at their defaults.
//!
//! [`menu`] draws the list every preset picker shows: the factory presets,
//! then yours (each with an × to delete it), then "Save as…".

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

const EXT: &str = "lyrapreset";

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub params: Vec<f32>,
    /// Yours (a file), not one of the plugin's own.
    pub user: bool,
}

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
pub fn current(entries: &[Entry], params: &[f32]) -> Option<usize> {
    entries.iter().position(|e| {
        e.params.len() == params.len()
            && e.params.iter().zip(params).all(|(a, b)| (a - b).abs() <= 1e-5 * a.abs().max(1.0))
    })
}

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

/// The preset list for a popup: the plugin's own, yours (× deletes), and a
/// "Save as" field for the current sound. Returns the preset picked, and
/// whether the popup should close.
pub fn menu(ui: &mut Ui, plugin: &str, params: &[f32], width: f32) -> (Option<Entry>, bool) {
    let all = entries(plugin);
    let now = current(&all, params);
    let mut pick = None;
    let mut close = false;
    ui.set_min_width(width);
    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        let mut shown_yours = false;
        for (k, e) in all.iter().enumerate() {
            if e.user && !shown_yours {
                shown_yours = true;
                ui.separator();
                ui.label(RichText::new("YOURS").small().color(crate::theme::TEXT_DIM));
            }
            ui.horizontal(|ui| {
                if ui.selectable_label(now == Some(k), &e.name).clicked() {
                    pick = Some(e.clone());
                    close = true;
                }
                if e.user {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let x = ui.small_button("×").on_hover_text("Delete this preset (its file goes too)");
                        if x.clicked() {
                            delete(plugin, &e.name);
                        }
                    });
                }
            });
        }
    });
    ui.separator();
    // Save as: the name is kept while the popup is open.
    let id = egui::Id::new(("preset-save-name", plugin));
    let mut name: String = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    let err_id = id.with("error");
    ui.horizontal(|ui| {
        let edit =
            ui.add(egui::TextEdit::singleline(&mut name).hint_text("New preset name").desired_width(width - 70.0));
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button("Save").clicked() || enter {
            match save(plugin, &name, params) {
                Ok(()) => {
                    name.clear();
                    ui.ctx().data_mut(|d| d.remove::<String>(err_id));
                }
                Err(e) => ui.ctx().data_mut(|d| d.insert_temp(err_id, e)),
            }
        }
    });
    if let Some(err) = ui.ctx().data(|d| d.get_temp::<String>(err_id)) {
        ui.label(RichText::new(err).small().color(crate::theme::ROSE));
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, name));
    (pick, close)
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
