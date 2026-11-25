//! Presets as an editor sees them: the entries the [`Host`] lists, which one
//! the sound is now, and the plain list a plugin can put in its own popup.

use crate::Host;
use egui::{Color32, RichText, Ui};

/// One preset: the plugin's own or the user's.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetEntry {
    pub name: String,
    pub params: Vec<f32>,
    /// The user's (a file), not one of the plugin's own.
    pub user: bool,
}

/// The preset these parameters are exactly, if any.
pub fn current(entries: &[PresetEntry], params: &[f32]) -> Option<usize> {
    entries.iter().position(|e| {
        e.params.len() == params.len()
            && e.params.iter().zip(params).all(|(a, b)| (a - b).abs() <= 1e-5 * a.abs().max(1.0))
    })
}

/// The colours [`menu`] prints its notes in (the plugin's, not Lyra's).
#[derive(Clone, Copy)]
pub struct MenuInk {
    /// The "YOURS" heading.
    pub dim: Color32,
    /// A failed save.
    pub error: Color32,
}

/// The preset list for a popup, in egui's widgets under the plugin's style:
/// the plugin's own, the user's (× deletes), and a "Save as" field for the
/// current sound. Returns the preset picked, and whether the popup should close.
pub fn menu(ui: &mut Ui, host: &mut dyn Host, width: f32, ink: MenuInk) -> (Option<PresetEntry>, bool) {
    let all = host.presets();
    let params = host.params().to_vec();
    let now = current(&all, &params);
    let id = egui::Id::new(("preset-save-name", host.instance()));
    let mut pick = None;
    let mut close = false;
    let mut delete = None;
    ui.set_min_width(width);
    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        let mut shown_yours = false;
        for (k, e) in all.iter().enumerate() {
            if e.user && !shown_yours {
                shown_yours = true;
                ui.separator();
                ui.label(RichText::new("YOURS").small().color(ink.dim));
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
                            delete = Some(e.name.clone());
                        }
                    });
                }
            });
        }
    });
    if let Some(name) = delete {
        host.delete_preset(&name);
    }
    ui.separator();
    // Save as: the name is kept while the popup is open.
    let mut name: String = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    let err_id = id.with("error");
    ui.horizontal(|ui| {
        let edit =
            ui.add(egui::TextEdit::singleline(&mut name).hint_text("New preset name").desired_width(width - 70.0));
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button("Save").clicked() || enter {
            match host.save_preset(&name, &params) {
                Ok(()) => {
                    name.clear();
                    ui.ctx().data_mut(|d| d.remove::<String>(err_id));
                }
                Err(e) => ui.ctx().data_mut(|d| d.insert_temp(err_id, e)),
            }
        }
    });
    if let Some(err) = ui.ctx().data(|d| d.get_temp::<String>(err_id)) {
        ui.label(RichText::new(err).small().color(ink.error));
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, name));
    (pick, close)
}
