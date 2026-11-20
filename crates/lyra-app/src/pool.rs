//! What's in the project, as racks in the right panel (the browser on the
//! left is the library: things you can add; this side is what you made):
//!
//! * **Clips** — the patterns and the audio clips. Click: make it current
//!   (the next click on the arrangement places it) · double-click: open it ·
//!   drag an audio clip onto a track: another copy. An audio clip's number is
//!   the mixer insert it plays into (M = master), like an instrument's.
//! * **Automation** — the automations: a little picture of each curve and
//!   how many controls it moves. Click, double-click, drag: the same.

use crate::app::UiState;
use crate::flex::{self, Grow, Px};
use crate::theme::{self, cr, stroke};
use crate::widgets;
use egui::{pos2, vec2, Align2, FontId, Rect, Response, Sense, Ui};
use lyra_model::{AudioId, AutomationId, History, Project};

/// An audio clip being dragged onto the arrangement.
#[derive(Clone, Debug)]
pub struct AudioDrag(pub AudioId);

/// An automation being dragged onto the arrangement.
#[derive(Clone, Debug)]
pub struct AutomationDrag(pub AutomationId);

/// A rack row: a plate with the item's light and name, highlighted when
/// it's the current one. Returns the row's response and the rect left over
/// on the right (for the row's own controls), `right` points wide.
fn plate(ui: &mut Ui, id: egui::Id, name: &str, light: theme::Light, chosen: bool, right: f32) -> (Response, Rect) {
    let m = theme::m();
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), m.step_h), Sense::hover());
    let [main, extra] = flex::row(r, [Grow(1.0), Px(right)], if right > 0.0 { theme::SEAM } else { 0.0 });
    let resp = ui.interact(main, id, Sense::click_and_drag());
    let p = ui.painter();
    let fill = if chosen {
        theme::PANEL_HI
    } else if resp.hovered() {
        theme::mix(theme::PANEL, theme::PANEL_HI, 0.6)
    } else {
        theme::PANEL
    };
    p.rect_filled(main, cr(0), fill);
    let [dot, text] = flex::row(main.shrink2(vec2(8.0, 0.0)), [Px(10.0), Grow(1.0)], 8.0);
    p.circle_filled(dot.center(), 4.0, light.mid);
    p.with_clip_rect(text).text(
        text.left_center(),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(m.text),
        if chosen { theme::TEXT } else { theme::mix(theme::TEXT, theme::TEXT_DIM, 0.3) },
    );
    (resp, extra)
}

fn caption(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text).size(theme::m().text_small).color(theme::TEXT_DIM));
}

/// The Clips tab: patterns, then audio clips.
pub fn clips(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    ui.spacing_mut().item_spacing.y = theme::SEAM;
    caption(ui, "PATTERNS");
    for i in 0..p.patterns.len() {
        let chosen = !st.place_audio && st.pattern == i;
        let (resp, _) =
            plate(ui, ui.id().with(("pool-pattern", i)), &p.patterns[i].name, theme::channel_light(i), chosen, 0.0);
        let resp = resp.on_hover_text("Click: the next click on the arrangement places it · double-click: open it");
        if resp.clicked() {
            if crate::browser::double_click(ui, "pool-last-pattern-click", i) {
                st.open_pattern = Some(i);
            }
            st.pattern = i;
            st.place_audio = false;
            st.place_automation = false;
            st.clip = None;
        }
    }
    caption(ui, "AUDIO");
    if p.audio.is_empty() {
        ui.label(egui::RichText::new("Drop a sample on a track.").small().color(theme::TEXT_DIM));
    }
    let m = theme::m();
    for i in 0..p.audio.len() {
        let (id, light) = (p.audio[i].id, theme::channel_light(p.audio[i].color));
        let chosen = st.place_audio && st.audio == Some(id) && st.clip.is_none();
        let (resp, well) = plate(ui, ui.id().with(("pool-audio", id)), &p.audio[i].name, light, chosen, m.step_h);
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), AudioDrag(id));
        }
        let resp = resp.on_hover_text(
            "Click: the next click on the arrangement places it · double-click: open it · drag: onto a track",
        );
        if resp.clicked() {
            if crate::browser::double_click(ui, "pool-last-audio-click", id) {
                st.open_audio = Some(id);
            }
            st.audio = Some(id);
            st.clip = None; // the audio clip itself, not one placed copy of it
            st.place_audio = true;
            st.place_automation = false;
        }
        // Its mixer insert, like an instrument's.
        let mut insert = p.audio[i].mixer;
        let inserts = p.mixer.len().saturating_sub(1);
        let r = ui.put(well, |ui: &mut Ui| {
            widgets::number_well(ui, &mut insert, 0..=inserts, 0, well.size(), widgets::insert_label)
        });
        let r = r.on_hover_text("The mixer insert it plays into (M = master)");
        if crate::rack::tracked(&r, h, p) {
            p.audio[i].mixer = insert;
        }
    }
}

/// The Automation tab: one row per automation.
pub fn automations(ui: &mut Ui, p: &mut Project, st: &mut UiState) {
    ui.spacing_mut().item_spacing.y = theme::SEAM;
    ui.add_space(6.0);
    if p.automations.is_empty() {
        ui.label(
            egui::RichText::new("Move a control, then the curve key in the arrangement's header.")
                .small()
                .color(theme::TEXT_DIM),
        );
    }
    let m = theme::m();
    for i in 0..p.automations.len() {
        let a = &p.automations[i];
        let (id, light) = (a.id, theme::channel_light(a.color));
        let chosen = st.automation == Some(id);
        let (resp, pic) = plate(ui, ui.id().with(("pool-auto", id)), &a.name, light, chosen, m.step_h * 2.4);
        // A little picture of the curve, and how many controls it moves.
        theme::well(ui.painter(), pic);
        let g = pic.shrink2(vec2(5.0, 4.0));
        let pts = crate::automation::points(a);
        let line: Vec<_> = (0..=24)
            .map(|k| {
                let u = k as f64 / 24.0 * a.length as f64;
                let v = crate::automation::value_at(a, &pts, u.min(a.length as f64 - 0.01));
                pos2(g.left() + g.width() * k as f32 / 24.0, g.bottom() - v * g.height())
            })
            .collect();
        ui.painter().add(egui::Shape::line(line, stroke(1.2, light.hi)));
        if a.links.len() > 1 {
            ui.painter().text(
                pic.right_top() + vec2(-3.0, 1.0),
                Align2::RIGHT_TOP,
                a.links.len().to_string(),
                FontId::monospace(m.text_small - 1.0),
                theme::TEXT_DIM,
            );
        }
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), AutomationDrag(id));
        }
        let what: Vec<String> =
            a.links.iter().filter_map(|l| crate::automation::control(p, l.target).map(|c| c.2)).collect();
        let resp = resp.on_hover_text(format!(
            "Moves {} · click: current · double-click: open · drag: onto a track",
            if what.is_empty() { "nothing".to_string() } else { what.join(", ") }
        ));
        if resp.clicked() {
            if crate::browser::double_click(ui, "pool-last-auto-click", id) {
                st.open_automation = Some(id);
            }
            st.automation = Some(id);
            st.clip = None;
            st.place_automation = true;
            st.place_audio = false;
        }
    }
}
