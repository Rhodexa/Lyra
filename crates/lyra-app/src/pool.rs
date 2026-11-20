//! What's in the project, as racks in the right panel (the browser on the
//! left is the library: things you can add; this side is what you made):
//!
//! * **Patterns** — each one an inset showing its notes, its name over them.
//!   Click: current (the next click on the arrangement places it) ·
//!   double-click: open it in the piano roll · drag: onto a track ·
//!   right-click: clone · + New pattern
//! * **Audio** — each audio clip as a rack row, like an instrument (they're
//!   used the same way): mute light, pan, volume, its name key (flashing
//!   as a clip of it starts), its mixer insert (M = master). Click,
//!   double-click, drag: the same.
//! * **Automation** — each automation, a little picture of its curve and how
//!   many controls it moves. Click, double-click, drag: the same.

use crate::app::UiState;
use crate::flex::{self, Grow, Px};
use crate::theme::{self, cr, stroke};
use crate::widgets;
use egui::{pos2, vec2, Align2, FontId, Rect, Response, Sense, Ui};
use lyra_model::{AudioId, AutomationId, History, Project};

/// An audio clip being dragged onto the arrangement.
#[derive(Clone, Debug)]
pub struct AudioDrag(pub AudioId);

/// A pattern being dragged onto the arrangement.
#[derive(Clone, Debug)]
pub struct PatternDrag(pub lyra_model::PatternId);

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

/// The Patterns tab: each pattern as an inset showing its notes (one loop,
/// every instrument in its colour), its name over them.
pub fn patterns(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    let m = theme::m();
    ui.spacing_mut().item_spacing.y = theme::SEAM;
    let mut clone = None;
    for i in 0..p.patterns.len() {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), m.step_h * 1.6), Sense::click_and_drag());
        let light = theme::channel_light(i);
        let chosen = !st.place_audio && !st.place_automation && st.pattern == i;
        let painter = ui.painter();
        theme::well(painter, r);
        if chosen || resp.hovered() {
            painter.rect_filled(r.shrink(1.0), cr(3), light.mid.gamma_multiply(if chosen { 0.12 } else { 0.06 }));
        }
        paint_notes(&painter.with_clip_rect(r.shrink(2.0)), r.shrink2(vec2(6.0, 5.0)), p, i);
        // The name, on a dark backing so busy notes never hide it.
        let ink = if chosen { theme::TEXT } else { theme::mix(theme::TEXT, theme::TEXT_DIM, 0.4) };
        let galley = painter.layout_no_wrap(p.patterns[i].name.clone(), FontId::proportional(m.text_small + 1.0), ink);
        let at = r.left_top() + vec2(6.0, 3.0);
        painter.rect_filled(
            Rect::from_min_size(at, galley.size() + vec2(6.0, 2.0)),
            cr(3),
            theme::WELL.gamma_multiply(0.85),
        );
        painter.galley(at + vec2(3.0, 1.0), galley, ink);
        if chosen {
            painter.rect_stroke(r, cr(4), stroke(1.0, light.mid.gamma_multiply(0.7)), egui::StrokeKind::Inside);
        }
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), PatternDrag(p.patterns[i].id));
        }
        let resp = resp.on_hover_text(
            "Click: the next click on the arrangement places it · double-click: open · drag: onto a track · right-click: clone",
        );
        if resp.clicked() {
            if crate::browser::double_click(ui, "pool-last-pattern-click", i) {
                st.open_pattern = Some(i);
            }
            st.pattern = i;
            st.place_audio = false;
            st.place_automation = false;
            st.clip = None;
        }
        resp.context_menu(|ui| {
            if ui.button("Clone").clicked() {
                clone = Some(i);
                ui.close_menu();
            }
        });
    }
    if let Some(i) = clone {
        h.checkpoint(p);
        st.pattern = p.clone_pattern(i);
    }
    ui.add_space(4.0);
    if ui.small_button("+ New pattern").clicked() {
        h.checkpoint(p);
        st.pattern = p.add_pattern();
        st.place_audio = false;
        st.place_automation = false;
        st.clip = None;
    }
}

/// One loop of pattern `i`'s notes in `r`, each instrument in its colour.
fn paint_notes(painter: &egui::Painter, r: Rect, p: &Project, i: usize) {
    let pat = &p.patterns[i];
    let len = (pat.play_length() * lyra_model::TICKS_PER_STEP).max(1) as f32;
    let (lo, hi) = pat.notes.values().flatten().fold((127u8, 0u8), |(lo, hi), n| (lo.min(n.key), hi.max(n.key)));
    let span = (hi.saturating_sub(lo) as f32).max(12.0);
    for ch in &p.channels {
        let ink = theme::channel_light(ch.color).mid.gamma_multiply(0.85);
        for n in pat.notes(ch.id) {
            let x0 = r.left() + n.start as f32 / len * r.width();
            let x1 = r.left() + (n.start + n.len) as f32 / len * r.width();
            let y = r.bottom() - (n.key - lo) as f32 / span * (r.height() - 2.0);
            painter.rect_filled(Rect::from_min_max(pos2(x0, y - 1.5), pos2(x1.max(x0 + 1.5), y)), cr(0), ink);
        }
    }
}

/// The Audio tab: each audio clip as a rack row, like an instrument (they're
/// used the same way): mute light, pan, volume, its name key (which flashes
/// as a clip of it starts playing), and its mixer insert.
pub fn audio(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState, playhead: Option<f64>) {
    let m = theme::m();
    ui.spacing_mut().item_spacing.y = theme::SEAM;
    if p.audio.is_empty() {
        ui.label(egui::RichText::new("Drop a sample on an arrangement track.").small().color(theme::TEXT_DIM));
    }
    for i in 0..p.audio.len() {
        let (id, light) = (p.audio[i].id, theme::channel_light(p.audio[i].color));
        let color = light.mid;
        ui.horizontal(|ui| {
            let audible = !p.audio[i].mute;
            let r = widgets::led(ui, audible, light, "Click / drag: mute");
            match widgets::sweep(ui, r.rect, "audio-mute", audible) {
                Some(widgets::Sweep::Begin(on)) => {
                    h.checkpoint(p);
                    p.audio[i].mute = !on;
                }
                Some(widgets::Sweep::Continue(on)) => p.audio[i].mute = !on,
                None => {}
            }
            let mut pan = p.audio[i].pan;
            if crate::rack::tracked(&widgets::knob(ui, &mut pan, &crate::rack::PAN, m.knob_small, color), h, p) {
                p.audio[i].pan = pan;
            }
            let mut vol = p.audio[i].volume;
            if crate::rack::tracked(&widgets::knob(ui, &mut vol, &crate::rack::VOLUME, m.knob_small, color), h, p) {
                p.audio[i].volume = vol;
            }
            // The name key: flashes as a clip of it starts, glows while one plays.
            let (rect, resp) = ui.allocate_exact_size(vec2(m.name_w, m.step_h), Sense::click_and_drag());
            let flash = playhead.map_or(0.0, |ph| {
                p.clips
                    .iter()
                    .filter(|c| c.audio() == Some(id) && (c.start as f64..c.end() as f64).contains(&ph))
                    .map(|c| (1.0 - (ph - c.start as f64) as f32 / (lyra_model::TICKS_PER_STEP * 4) as f32).max(0.3))
                    .fold(0.0f32, f32::max)
            });
            if flash > 0.0 {
                ui.ctx().request_repaint();
            }
            let chosen = st.place_audio && st.audio == Some(id) && st.clip.is_none();
            crate::rack::name_key(ui, rect, resp.hovered(), &p.audio[i].name, color, chosen, flash, false);
            if resp.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), AudioDrag(id));
            }
            let resp = resp.on_hover_text(
                "Click: the next click on the arrangement places it · double-click: open · drag: onto a track",
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
            let r =
                widgets::number_well(ui, &mut insert, 0..=inserts, 0, vec2(m.step_h, m.step_h), widgets::insert_label)
                    .on_hover_text("The mixer insert it plays into (M = master)");
            if crate::rack::tracked(&r, h, p) {
                p.audio[i].mixer = insert;
            }
        });
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
