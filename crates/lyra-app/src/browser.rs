//! The browser: the docked panel on the left (F4 shows / hides it).
//!
//! * **Samples** — Lyra's Starter Kit and your library folder. Click a sample
//!   to hear it; double-click (or drag it onto the plugin rack) to make a
//!   sampler that plays it; drag it onto a rack row to swap that sample in
//! * **Patterns** — click to edit (and place) that pattern
//! * **Audio clips** — the project's audio clips: click to place it,
//!   double-click to open it, drag one onto a track for another copy
//! * **Instruments** — click to add one to the instrument rack; open the
//!   arrow for its presets
//! * **Effects** — click to plug one into the selected mixer track
//!
//! The library is scanned when Lyra starts; the ↻ on the Samples header
//! scans again (after you add files to your library folder).

use crate::app::{Focus, UiState};
use crate::samples::{self, Folder};
use crate::theme::{self, cr};
use egui::{pos2, vec2, Align2, FontId, Rect, Response, Sense, Ui};
use lyra_model::{EffectSlot, History, Project};

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    egui::ScrollArea::vertical().id_salt("browser").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;

        // ── Samples ──
        let library = std::mem::take(&mut st.library);
        let count = library.iter().map(count_files).sum();
        if section(ui, "Samples", count) {
            for root in &library {
                folder(ui, root, 0, p, h, st);
            }
            if library.iter().all(|f| f.files.is_empty() && f.folders.is_empty()) {
                ui.label(egui::RichText::new("No samples found.").small().color(theme::TEXT_DIM));
            }
            let lib = samples::user_root();
            ui.horizontal(|ui| {
                if ui.small_button("Rescan").on_hover_text("Look for new files in the library folders").clicked() {
                    st.rescan = true;
                }
                if ui
                    .small_button("Your folder…")
                    .on_hover_text(format!("Put your own samples in {}", lib.display()))
                    .clicked()
                {
                    let _ = std::fs::create_dir_all(&lib);
                    let _ = std::process::Command::new("xdg-open").arg(&lib).spawn();
                    st.rescan = true;
                }
            });
        }
        st.library = library;

        // ── Patterns ──
        if section(ui, "Patterns", p.patterns.len()) {
            for i in 0..p.patterns.len() {
                let light = theme::channel_light(i);
                let r = row(ui, &p.patterns[i].name, Some(light), st.pattern == i, 0.0);
                let r = r.on_hover_text("Click: make it the current pattern · double-click: open it in the piano roll");
                if r.double_clicked() {
                    st.open_pattern = Some(i);
                } else if r.clicked() {
                    st.pattern = i;
                    st.place_audio = false;
                }
            }
        }

        // ── Audio clips ──
        if section(ui, "Audio clips", p.audio.len()) {
            for i in 0..p.audio.len() {
                let a = &p.audio[i];
                let (id, light) = (a.id, theme::channel_light(a.color));
                let m = theme::m();
                let (r, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click_and_drag());
                let chosen = st.place_audio && st.audio == Some(id);
                paint_row(ui, r, &resp, &a.name, Some(light), chosen, 0.0);
                if resp.drag_started() {
                    egui::DragAndDrop::set_payload(ui.ctx(), AudioDrag(id));
                }
                let resp =
                    resp.on_hover_text("Click: place it with a click · double-click: open it · drag: onto a track");
                if resp.clicked() {
                    st.audio = Some(id);
                    st.audio_clip = None; // the source alone, not one clip of it
                    st.place_audio = true;
                    if double_click(ui, "browser-last-audio-click", id) {
                        st.open_audio = Some(id);
                    }
                }
            }
            if p.audio.is_empty() {
                ui.label(egui::RichText::new("Drop a sample on a track.").small().color(theme::TEXT_DIM));
            }
        }

        // ── Instruments ──
        if section(ui, "Instruments", lyra_dsp::INSTRUMENTS.len()) {
            for inst in lyra_dsp::INSTRUMENTS {
                let presets = lyra_dsp::presets_for(inst.id);
                let open_id = ui.id().with(("presets", inst.id));
                let mut open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
                let (r, arrow) = row_with_arrow(ui, inst.name, !presets.is_empty(), open);
                if arrow {
                    open ^= true;
                    ui.data_mut(|d| d.insert_temp(open_id, open));
                } else if r.on_hover_text("Click: add to the instrument rack").clicked() {
                    add_instrument(p, h, st, inst.name, inst.id, lyra_dsp::default_params(inst.id));
                }
                if open {
                    for preset in &presets {
                        let r = row(ui, preset.name, None, false, 14.0);
                        if r.on_hover_text("Click: add this preset to the instrument rack").clicked() {
                            add_instrument(p, h, st, preset.name, inst.id, preset.params());
                        }
                    }
                }
            }
        }

        // ── Effects ──
        if section(ui, "Effects", lyra_dsp::effects::EFFECTS.len()) {
            let t = st.mixer_selected.min(p.mixer.len() - 1);
            let full = p.mixer[t].effects.len() >= lyra_engine::MAX_FX;
            for e in lyra_dsp::effects::EFFECTS {
                let hint = if full {
                    format!("{}'s effect rack is full", p.mixer[t].name)
                } else {
                    format!("Click: plug into {} (the selected mixer track)", p.mixer[t].name)
                };
                if row(ui, e.name, None, false, 0.0).on_hover_text(hint).clicked() && !full {
                    h.checkpoint(p);
                    p.mixer[t].effects.push(EffectSlot {
                        effect: e.id.into(),
                        params: lyra_dsp::effects::default_params(e.id),
                        bypass: false,
                    });
                    st.effect = Some((t, p.mixer[t].effects.len() - 1));
                    st.raise = Some(Focus::Effect);
                }
            }
        }
    });
}

fn count_files(f: &Folder) -> usize {
    f.files.len() + f.folders.iter().map(count_files).sum::<usize>()
}

/// A library folder: a foldable row, then its subfolders and samples.
fn folder(ui: &mut Ui, f: &Folder, depth: usize, p: &mut Project, h: &mut History, st: &mut UiState) {
    let id = ui.id().with(("folder", &f.path));
    let mut open: bool = ui.data(|d| d.get_temp(id)).unwrap_or(depth == 0);
    let indent = depth as f32 * 12.0;
    let r = row(ui, &f.name, None, false, indent + 12.0);
    chevron(ui.painter(), pos2(r.rect.left() + indent + 10.0, r.rect.center().y), open, theme::TEXT_DIM);
    if r.clicked() {
        open ^= true;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    if !open {
        return;
    }
    for sub in &f.folders {
        folder(ui, sub, depth + 1, p, h, st);
    }
    for file in &f.files {
        let name = samples::name_for(file);
        let shown = samples::display_name(&name);
        let (r, resp) = {
            let m = theme::m();
            ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click_and_drag())
        };
        paint_row(ui, r, &resp, &shown, None, st.previewing.as_deref() == Some(name.as_str()), indent + 24.0);
        // A small speaker mark: "click to hear".
        let c = pos2(r.left() + indent + 16.0, r.center().y);
        let ink = if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                c + vec2(-3.0, -2.0),
                c + vec2(0.0, -2.0),
                c + vec2(3.0, -4.5),
                c + vec2(3.0, 4.5),
                c + vec2(0.0, 2.0),
                c + vec2(-3.0, 2.0),
            ],
            ink,
            egui::Stroke::NONE,
        ));
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), SampleDrag(name.clone()));
        }
        let resp = resp
            .on_hover_text("Click: hear it · double-click: new sampler · drag: onto the plugin rack, or onto a track");
        if resp.clicked() {
            if double_click(ui, "browser-last-sample-click", name.clone()) {
                add_sampler(p, h, st, &name);
            } else {
                st.preview = Some(name.clone());
            }
        }
    }
}

/// A sample being dragged out of the browser (its stored name).
#[derive(Clone, Debug)]
pub struct SampleDrag(pub String);

/// An audio clip being dragged out of the browser.
#[derive(Clone, Debug)]
pub struct AudioDrag(pub lyra_model::AudioId);

/// Our own double-click test (egui's doesn't fire on rows that can also be
/// dragged): true on a second click on the same `item` within 0.35 s.
fn double_click<T: Clone + PartialEq + Send + Sync + 'static>(ui: &Ui, key: &str, item: T) -> bool {
    let now = ui.input(|i| i.time);
    let key = egui::Id::new(key);
    let last: Option<(T, f64)> = ui.data(|d| d.get_temp(key));
    if last.is_some_and(|(n, t)| n == item && now - t < 0.35) {
        ui.data_mut(|d| d.remove::<(T, f64)>(key));
        true
    } else {
        ui.data_mut(|d| d.insert_temp(key, (item, now)));
        false
    }
}

/// A new sampler instrument playing `sample`, named after it.
pub fn add_sampler(p: &mut Project, h: &mut History, st: &mut UiState, sample: &str) {
    h.checkpoint(p);
    p.add_channel(&samples::display_name(sample), "lyra.sampler", lyra_dsp::default_params("lyra.sampler"));
    if let Some(ch) = p.channels.last_mut() {
        ch.sample = Some(sample.to_string());
    }
    st.selected = p.channels.len() - 1;
    st.show_plugins = true;
}

fn add_instrument(p: &mut Project, h: &mut History, st: &mut UiState, name: &str, id: &str, params: Vec<f32>) {
    h.checkpoint(p);
    p.add_channel(name, id, params);
    st.selected = p.channels.len() - 1;
    st.show_plugins = true;
}

/// A section header (click to fold). Returns whether it's open.
fn section(ui: &mut Ui, title: &str, count: usize) -> bool {
    let m = theme::m();
    let id = ui.id().with(("section", title));
    let mut open: bool = ui.data(|d| d.get_temp(id)).unwrap_or(true);
    ui.add_space(6.0);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click());
    if resp.clicked() {
        open ^= true;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    let p = ui.painter();
    chevron(p, pos2(r.left() + 8.0, r.center().y), open, theme::TEXT_DIM);
    p.text(
        r.left_center() + vec2(20.0, 0.0),
        Align2::LEFT_CENTER,
        title.to_uppercase(),
        FontId::proportional(m.text_small),
        if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM },
    );
    p.text(
        r.right_center() - vec2(6.0, 0.0),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::monospace(m.text_small),
        theme::TEXT_DIM,
    );
    open
}

/// One item: an optional light, the name, a highlight when hovered or chosen.
fn row(ui: &mut Ui, name: &str, light: Option<theme::Light>, chosen: bool, indent: f32) -> Response {
    let m = theme::m();
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click());
    paint_row(ui, r, &resp, name, light, chosen, indent);
    resp
}

/// An item with a fold arrow on the right (for instruments with presets).
/// Returns the row's response and whether the arrow was the part clicked.
fn row_with_arrow(ui: &mut Ui, name: &str, has_arrow: bool, open: bool) -> (Response, bool) {
    let m = theme::m();
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click());
    paint_row(ui, r, &resp, name, None, false, 0.0);
    if !has_arrow {
        return (resp, false);
    }
    let arrow = Rect::from_min_max(pos2(r.right() - m.control_h, r.top()), r.max);
    let over = resp.hover_pos().is_some_and(|p| arrow.contains(p));
    if over {
        ui.painter().rect_filled(arrow.shrink(3.0), cr(3), theme::RAISED);
    }
    chevron(ui.painter(), arrow.center(), open, if over { theme::TEXT } else { theme::TEXT_DIM });
    let clicked = resp.clicked() && over;
    (resp.on_hover_text("Presets: the arrow"), clicked)
}

fn paint_row(ui: &Ui, r: Rect, resp: &Response, name: &str, light: Option<theme::Light>, chosen: bool, indent: f32) {
    let m = theme::m();
    let p = ui.painter();
    if chosen || resp.hovered() {
        p.rect_filled(r, cr(3), if chosen { theme::PANEL_HI } else { theme::mix(theme::PANEL, theme::PANEL_HI, 0.6) });
    }
    let mut x = r.left() + 10.0 + indent;
    if let Some(l) = light {
        p.circle_filled(pos2(x + 3.0, r.center().y), 3.5, l.mid);
        x += 14.0;
    }
    p.text(
        pos2(x, r.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(m.text),
        if chosen { theme::TEXT } else { theme::mix(theme::TEXT, theme::TEXT_DIM, 0.35) },
    );
}

/// A small open/closed arrow.
fn chevron(p: &egui::Painter, c: egui::Pos2, open: bool, color: egui::Color32) {
    let pts = if open {
        vec![c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)]
    } else {
        vec![c + vec2(-1.5, -3.5), c + vec2(2.0, 0.0), c + vec2(-1.5, 3.5)]
    };
    p.add(egui::Shape::line(pts, theme::stroke(1.5, color)));
}
