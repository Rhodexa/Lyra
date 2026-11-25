//! The browser: the docked panel on the left (F4 shows / hides it), the
//! library: what you can add. Tabs:
//!
//! * **Files** — Lyra's Starter Kit, your library folder, and any folders
//!   you add (**+ Add folder…**; right-click one: open it in the file
//!   manager, or take it out of the browser). The search box looks through
//!   all of them. Each sample shows how long it is. Click a sample: hear it ·
//!   double-click: a sampler that plays it · drag: onto the plugin rack (a
//!   sampler), onto a rack row (swap it in), or onto an arrangement track (an
//!   audio clip)
//! * **Instruments** — click to add one to the instrument rack; open the
//!   arrow for its presets
//! * **Effects** — click to plug one into the selected mixer track
//!
//! What the *project* holds (patterns, audio clips, automation) is in the
//! right panel instead: see `pool.rs`.

use crate::app::{Focus, LeftTab, UiState};
use crate::widgets::Hint;
use crate::samples::{self, Folder};
use crate::theme::{self, cr};
use egui::{pos2, vec2, Align2, FontId, Rect, Response, Sense, Ui};
use lyra_model::{EffectSlot, History, Project};
use std::path::Path;

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    match st.left_tab {
        LeftTab::Files => files(ui, p, h, st),
        LeftTab::Instruments => instruments(ui, p, h, st),
        LeftTab::Effects => effects(ui, p, h, st),
    }
}

/// The Files tab: search, the folders, + Add folder.
fn files(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    let m = theme::m();
    // The search box and ↻, laid out in a fixed row: the panel grows to fit
    // its content, and a text field sized by number came out a little wider
    // than asked, widening the panel every frame.
    let (row_r, _) = ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::hover());
    let [field, again] = crate::flex::row(row_r, [crate::flex::Grow(1.0), crate::flex::Px(m.control_h)], 4.0);
    // (A text field's width doesn't count its inner margin: take it off.)
    crate::widgets::text_field_at(ui, field, &mut st.search, ui.id().with("search"), "Search samples", true);
    // ↻ as a button, drawn: the glyph from a font came out wider than its slot.
    let resp = ui.interact(again, ui.id().with("rescan"), Sense::click());
    let body = if resp.is_pointer_button_down_on() {
        theme::mix(theme::PLATE, egui::Color32::BLACK, 0.18)
    } else if resp.hovered() {
        theme::mix(theme::PLATE, egui::Color32::WHITE, 0.09)
    } else {
        theme::PLATE
    };
    theme::outlined(ui.painter(), again, 3.0, body);
    let ink = if resp.hovered() { egui::Color32::WHITE } else { theme::LEGEND };
    let (c, rad) = (again.center(), again.height() * 0.22);
    let arc: Vec<egui::Pos2> = (0..=20)
        .map(|i| {
            let a = -0.6 + i as f32 / 20.0 * 4.9;
            c + vec2(a.cos(), a.sin()) * rad
        })
        .collect();
    let tip = arc[arc.len() - 1];
    ui.painter().add(egui::Shape::line(arc, theme::stroke(1.5, ink)));
    let head = vec![tip + vec2(-3.5, 0.5), tip + vec2(1.5, -3.0), tip + vec2(2.0, 2.5)];
    ui.painter().add(egui::Shape::convex_polygon(head, ink, egui::Stroke::NONE));
    if resp.hint("Look again for new files").clicked() {
        st.rescan = true;
    }
    ui.add_space(4.0);
    let library = std::mem::take(&mut st.library);
    let query = st.search.trim().to_lowercase();
    let mut remove = None;
    egui::ScrollArea::vertical().id_salt("browser-files").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        if !query.is_empty() {
            // Everything that matches, from every folder, flat.
            let mut found = Vec::new();
            for root in &library {
                matches(root, &query, &mut found);
            }
            if found.is_empty() {
                ui.label(egui::RichText::new("Nothing matches.").small().color(theme::TEXT_DIM));
            }
            for path in found.iter().take(400) {
                file_row(ui, path, 12.0, p, h, st);
            }
        } else {
            let added = crate::files::browser_folders();
            for root in &library {
                let r = folder(ui, root, 0, p, h, st);
                let yours = added.contains(&root.path);
                // Right-click: the folder's menu, an LCD drawer at the mouse.
                let picked = crate::widgets::context_drawer(ui, &r, ("browser-root", root.path.clone()), 240.0, || {
                    let mut rows = vec![crate::widgets::DropRow::text("Open in the file manager")];
                    if yours {
                        rows.push(crate::widgets::DropRow::text("Take it out of the browser"));
                    }
                    rows
                });
                match picked {
                    Some(0) => {
                        let _ = std::fs::create_dir_all(&root.path);
                        let _ = std::process::Command::new("xdg-open").arg(&root.path).spawn();
                    }
                    Some(1) => remove = Some(root.path.clone()),
                    _ => {}
                }
            }
            ui.add_space(6.0);
            if crate::widgets::wide_key(ui, "+ Add folder…", "Show another folder of sounds here (Lyra remembers it)")
                .clicked()
            {
                if let Some(dir) = crate::files::pick_folder("Add a folder of sounds to the browser") {
                    let mut folders = crate::files::browser_folders();
                    if !folders.contains(&dir) {
                        folders.push(dir);
                        crate::files::set_browser_folders(&folders);
                    }
                    st.rescan = true;
                }
            }
        }
    });
    if let Some(dir) = remove {
        let folders: Vec<_> = crate::files::browser_folders().into_iter().filter(|f| *f != dir).collect();
        crate::files::set_browser_folders(&folders);
        st.rescan = true;
    }
    st.library = library;
}

/// Every file under `f` whose name contains `query` (lower case).
fn matches(f: &Folder, query: &str, out: &mut Vec<std::path::PathBuf>) {
    for sub in &f.folders {
        matches(sub, query, out);
    }
    for file in &f.files {
        if file.file_stem().is_some_and(|n| n.to_string_lossy().to_lowercase().contains(query)) {
            out.push(file.clone());
        }
    }
}

/// The Instruments tab.
fn instruments(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    egui::ScrollArea::vertical().id_salt("browser-instruments").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        for inst in lyra_catalog::instruments() {
            let presets = lyra_catalog::presets_for(inst.id);
            let open_id = ui.id().with(("presets", inst.id));
            let mut open: bool = ui.data(|d| d.get_temp(open_id)).unwrap_or(false);
            let (r, arrow) = row_with_arrow(ui, inst.name, !presets.is_empty(), open);
            if arrow {
                open ^= true;
                ui.data_mut(|d| d.insert_temp(open_id, open));
            } else if r.hint("Click: add to the instrument rack").clicked() {
                add_instrument(p, h, st, inst.name, inst.id, lyra_catalog::default_params(inst.id));
            }
            if open {
                for preset in &presets {
                    let r = row(ui, preset.name, None, false, 14.0, 0.0);
                    if r.hint("Click: add this preset to the instrument rack").clicked() {
                        add_instrument(p, h, st, preset.name, inst.id, preset.params());
                    }
                }
            }
        }
    });
}

/// The Effects tab.
fn effects(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    egui::ScrollArea::vertical().id_salt("browser-effects").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        let t = st.mixer_selected.min(p.mixer.len() - 1);
        let full = p.mixer[t].effects.len() >= lyra_engine::MAX_FX;
        for e in lyra_catalog::effects::all() {
            let hint = if full {
                format!("{}'s effect rack is full", p.mixer[t].name)
            } else {
                format!("Click: plug into {} (the selected mixer track)", p.mixer[t].name)
            };
            if row(ui, e.name, None, false, 0.0, 0.0).hint(hint).clicked() && !full {
                h.checkpoint(p);
                p.mixer[t].effects.push(EffectSlot::new(e.id, lyra_catalog::effects::default_params(e.id)));
                st.effect = Some((t, p.mixer[t].effects.len() - 1));
                st.raise = Some(Focus::Effect);
            }
        }
    });
}

fn count_files(f: &Folder) -> usize {
    f.files.len() + f.folders.iter().map(count_files).sum::<usize>()
}

/// A library folder: a foldable row, then its subfolders and samples.
/// Returns the folder row's response (for its right-click menu).
fn folder(ui: &mut Ui, f: &Folder, depth: usize, p: &mut Project, h: &mut History, st: &mut UiState) -> Response {
    let id = ui.id().with(("folder", &f.path));
    let mut open: bool = ui.data(|d| d.get_temp(id)).unwrap_or(depth == 0);
    let indent = depth as f32 * 12.0;
    let (rr, r) = ui.allocate_exact_size(vec2(ui.available_width(), theme::m().control_h), Sense::click());
    paint_row(ui, rr, &r, &f.name, None, false, indent + 12.0, 30.0, true);
    chevron(ui.painter(), pos2(r.rect.left() + indent + 10.0, r.rect.center().y), open, theme::TEXT_DIM);
    let count = count_files(f);
    ui.painter().text(
        r.rect.right_center() - vec2(6.0, 0.0),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::monospace(12.0),
        theme::INK_DIM,
    );
    if r.clicked() {
        open ^= true;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    if open {
        for sub in &f.folders {
            folder(ui, sub, depth + 1, p, h, st);
        }
        for file in &f.files {
            file_row(ui, file, indent + 24.0, p, h, st);
        }
    }
    r
}

/// One sample: a speaker mark, its name, how long it is.
fn file_row(ui: &mut Ui, file: &Path, indent: f32, p: &mut Project, h: &mut History, st: &mut UiState) {
    if crate::midi::is_midi(file) {
        return midi_row(ui, file, indent, st);
    }
    let m = theme::m();
    let name = samples::name_for(file);
    let shown = samples::display_name(&name);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click_and_drag());
    // The name starts after the speaker mark.
    paint_row(ui, r, &resp, &shown, None, st.previewing.as_deref() == Some(name.as_str()), indent + 10.0, 58.0, false);
    // A small speaker mark: "click to hear".
    let c = pos2(r.left() + indent + 6.0, r.center().y);
    let ink = if resp.hovered() { theme::INK } else { theme::INK_DIM };
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
    // Its length (read from the header once, then remembered).
    let secs = *st.lengths.entry(file.to_path_buf()).or_insert_with(|| samples::length(file));
    if let Some(secs) = secs {
        let text = if secs < 10.0 {
            format!("{secs:.2} s")
        } else if secs < 60.0 {
            format!("{secs:.1} s")
        } else {
            format!("{}:{:02}", (secs / 60.0) as u32, secs as u32 % 60)
        };
        ui.painter().text(
            r.right_center() - vec2(6.0, 0.0),
            Align2::RIGHT_CENTER,
            text,
            FontId::monospace(12.0),
            theme::INK_DIM,
        );
    }
    if resp.drag_started() {
        egui::DragAndDrop::set_payload(ui.ctx(), SampleDrag(name.clone()));
    }
    let resp = resp.hint(format!(
        "{}\nClick: hear it · double-click: new sampler · drag: onto the plugin rack, or onto a track",
        file.display()
    ));
    if resp.clicked() {
        if double_click(ui, "browser-last-sample-click", name.clone()) {
            add_sampler(p, h, st, &name);
        } else {
            st.preview = Some(name.clone());
        }
    }
}

/// A MIDI file in the browser: drag it onto the piano roll (into the
/// instrument you're editing) or the arrangement (every track, from there
/// down); double-click to bring the whole file in below the song.
fn midi_row(ui: &mut Ui, file: &Path, indent: f32, st: &mut UiState) {
    let m = theme::m();
    let shown = file.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click_and_drag());
    paint_row(ui, r, &resp, &shown, None, false, indent + 10.0, 58.0, false);
    // A note mark: "this is notes, not sound".
    let c = pos2(r.left() + indent + 6.0, r.center().y);
    let ink = if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
    ui.painter().circle_filled(c + vec2(-1.0, 3.0), 2.5, ink);
    ui.painter().vline(c.x + 1.5, egui::Rangef::new(c.y - 5.0, c.y + 3.0), theme::stroke(1.2, ink));
    ui.painter().text(
        r.right_center() - vec2(6.0, 0.0),
        Align2::RIGHT_CENTER,
        "MIDI",
        FontId::monospace(m.text_small),
        ink,
    );
    if resp.drag_started() {
        egui::DragAndDrop::set_payload(ui.ctx(), crate::midi::MidiDrag(file.to_path_buf()));
    }
    let resp = resp.hint(format!(
        "{}\nDrag: onto the piano roll (into this instrument) or a track (every part, from there down) · double-click: bring it all in",
        file.display()
    ));
    if resp.clicked() && double_click(ui, "browser-last-sample-click", file.display().to_string()) {
        st.import_midi = Some(file.to_path_buf());
    }
}

/// A sample being dragged out of the browser (its stored name).
#[derive(Clone, Debug)]
pub struct SampleDrag(pub String);

/// Our own double-click test (egui's doesn't fire on rows that can also be
/// dragged): true on a second click on the same `item` within 0.35 s.
pub fn double_click<T: Clone + PartialEq + Send + Sync + 'static>(ui: &Ui, key: &str, item: T) -> bool {
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
    p.add_channel(&samples::display_name(sample), "lyra.sampler", lyra_catalog::default_params("lyra.sampler"));
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

/// One item: an optional light, the name, a highlight when hovered or chosen.
fn row(ui: &mut Ui, name: &str, light: Option<theme::Light>, chosen: bool, indent: f32, reserve: f32) -> Response {
    let m = theme::m();
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click());
    paint_row(ui, r, &resp, name, light, chosen, indent, reserve, false);
    resp
}

/// An item with a fold arrow on the right (for instruments with presets).
/// Returns the row's response and whether the arrow was the part clicked.
fn row_with_arrow(ui: &mut Ui, name: &str, has_arrow: bool, open: bool) -> (Response, bool) {
    let m = theme::m();
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), m.control_h), Sense::click());
    paint_row(ui, r, &resp, name, None, false, 0.0, m.control_h, false);
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
    (resp.hint("Presets: the arrow"), clicked)
}

/// A list row (catalog-next.html, settled): flat, a soft lift under the
/// mouse; names in the reading font as written, cut with "…"; folder names
/// (`caps`) in the label font. `playing`: the sample being previewed, its name
/// lit and a small light pulsing. `reserve`: points kept free at the right (for
/// a count or a length).
#[allow(clippy::too_many_arguments)]
fn paint_row(
    ui: &Ui,
    r: Rect,
    resp: &Response,
    name: &str,
    light: Option<theme::Light>,
    playing: bool,
    indent: f32,
    reserve: f32,
    caps: bool,
) {
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(r, cr(2), ROW_HOVER);
    }
    let mut x = r.left() + 10.0 + indent;
    if let Some(l) = light {
        p.circle_filled(pos2(x + 3.0, r.center().y), 3.5, l.mid);
        x += 14.0;
    }
    let ink = if playing { theme::L_MINT.hi } else { theme::INK };
    let max_w = (r.right() - reserve - x - 4.0).max(10.0);
    let galley = if caps {
        let job = egui::text::LayoutJob::single_section(
            name.to_uppercase(),
            egui::TextFormat { font_id: theme::label(12.0), color: ink, extra_letter_spacing: 0.6, ..Default::default() },
        );
        ui.fonts(|f| f.layout_job(fit(job, max_w)))
    } else {
        let job = egui::text::LayoutJob::single_section(
            name.to_string(),
            egui::TextFormat { font_id: theme::read(13.0), color: ink, ..Default::default() },
        );
        ui.fonts(|f| f.layout_job(fit(job, max_w)))
    };
    p.galley(pos2(x, r.center().y - galley.size().y * 0.5), galley, ink);
    if playing {
        // A small light that pulses while the preview plays, left of the details.
        let t = ui.input(|i| i.time);
        let a = 0.35 + 0.65 * (0.5 + 0.5 * (t * std::f64::consts::TAU * 1.1).sin()) as f32;
        p.circle_filled(pos2(r.right() - reserve - 4.0, r.center().y), 3.0, theme::L_MINT.mid.gamma_multiply(a));
        ui.ctx().request_repaint();
    }
}

/// One line, cut with "…" at `max_w`.
fn fit(mut job: egui::text::LayoutJob, max_w: f32) -> egui::text::LayoutJob {
    job.wrap = egui::text::TextWrapping { max_width: max_w, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    job
}

/// A row under the mouse lifts a little (no fill otherwise: flat rows).
const ROW_HOVER: egui::Color32 = egui::Color32::from_rgb(0x32, 0x32, 0x32);

/// A small open/closed arrow.
fn chevron(p: &egui::Painter, c: egui::Pos2, open: bool, color: egui::Color32) {
    let pts = if open {
        vec![c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)]
    } else {
        vec![c + vec2(-1.5, -3.5), c + vec2(2.0, 0.0), c + vec2(-1.5, 3.5)]
    };
    p.add(egui::Shape::line(pts, theme::stroke(1.5, color)));
}
