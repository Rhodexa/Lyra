//! The Design panel (F12): every size in `theme::Metrics` on a slider, applied
//! live. Tune until it feels right, press "Copy as code", and paste the result
//! to Claude or into JOURNAL.md — it drops straight into `Metrics::DEFAULT`.

use crate::theme::{self, Metrics};
use egui::{RichText, Slider, Ui};

pub fn show(ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let mut m = theme::m();
    let before = m;

    ui.label(
        RichText::new(
            "Drag the sliders and watch the app change. When it feels right, copy and paste the numbers to Claude.",
        )
        .small()
        .color(theme::TEXT_DIM),
    );
    ui.add_space(4.0);

    let mut zoom = ctx.zoom_factor();
    section(ui, "Whole UI", |ui| {
        ui.add(Slider::new(&mut zoom, 0.6..=2.0).text("zoom (also Ctrl +/-)"));
    });
    if (zoom - ctx.zoom_factor()).abs() > 1e-3 {
        ctx.set_zoom_factor(zoom);
    }

    section(ui, "Text & controls", |ui| {
        ui.add(Slider::new(&mut m.text, 9.0..=22.0).text("text"));
        ui.add(Slider::new(&mut m.text_small, 7.0..=18.0).text("small text"));
        ui.add(Slider::new(&mut m.control_h, 16.0..=44.0).text("button / menu height"));
        ui.add(Slider::new(&mut m.corner, 0.0..=12.0).text("corner radius"));
    });
    section(ui, "Toolbar", |ui| {
        ui.add(Slider::new(&mut m.toolbar_h, 30.0..=110.0).text("height"));
        ui.add(Slider::new(&mut m.module_pad, 0.0..=16.0).text("module padding"));
        ui.add(Slider::new(&mut m.module_gap, 0.0..=24.0).text("space between modules"));
    });
    section(ui, "Channel rack", |ui| {
        ui.add(Slider::new(&mut m.step_w, 12.0..=40.0).text("step width"));
        ui.add(Slider::new(&mut m.step_h, 14.0..=48.0).text("step height"));
        ui.add(Slider::new(&mut m.knob_small, 14.0..=40.0).text("knob size"));
        ui.add(Slider::new(&mut m.led, 10.0..=28.0).text("mute light size"));
        ui.add(Slider::new(&mut m.name_w, 70.0..=240.0).text("name width"));
    });
    section(ui, "Instrument editor", |ui| {
        ui.add(Slider::new(&mut m.knob_large, 24.0..=72.0).text("knob size"));
    });
    section(ui, "Piano roll", |ui| {
        ui.add(Slider::new(&mut m.overview_h, 6.0..=60.0).text("overview bar height"));
        ui.add(Slider::new(&mut m.ruler_h, 12.0..=40.0).text("ruler height"));
        ui.add(Slider::new(&mut m.keys_w, 30.0..=120.0).text("keyboard width"));
        ui.add(Slider::new(&mut m.row_h, 8.0..=32.0).text("row height (new windows)"));
        ui.add(Slider::new(&mut m.edge_grab, 2.0..=24.0).text("resize zone inside note"));
        ui.add(Slider::new(&mut m.edge_reach, 0.0..=24.0).text("resize zone past note end"));
    });

    section(ui, "Material: plasma (now) vs matte rubber", material_swatch);

    if m != before {
        theme::set_metrics(&ctx, m);
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Copy as code").on_hover_text("Puts the numbers on the clipboard").clicked() {
            let zoom_line = format!("// UI zoom: {:.2}\n", ctx.zoom_factor());
            ctx.copy_text(zoom_line + &theme::m().as_code());
        }
        if ui.button("Reset").clicked() {
            theme::set_metrics(&ctx, Metrics::DEFAULT);
            ctx.set_zoom_factor(1.0);
        }
    });
}

fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.label(RichText::new(title.to_uppercase()).small().color(theme::TEXT_DIM));
    ui.indent(title, add);
    ui.add_space(4.0);
}

/// Side by side: today's glowing look and the proposed matte-rubber material,
/// on the same things (steps off/on/playing, a mute light, a button).
fn material_swatch(ui: &mut Ui) {
    let m = theme::m();
    let (w, h) = (m.step_w * 1.4, m.step_h * 1.2);
    for rubber in [false, true] {
        ui.horizontal(|ui| {
            ui.label(RichText::new(if rubber { "rubber" } else { "plasma" }).small().color(theme::TEXT_DIM));
            let (rect, _) = ui.allocate_exact_size(egui::vec2(w * 7.0 + 60.0, h + 10.0), egui::Sense::hover());
            let p = ui.painter();
            let colors = [None, Some(theme::MINT), Some(theme::ROSE), Some(theme::SKY), Some(theme::BUTTER)];
            for (i, c) in colors.iter().enumerate() {
                let r = egui::Rect::from_min_size(rect.min + egui::vec2(i as f32 * (w + 6.0), 4.0), egui::vec2(w, h));
                let playing = i == 3;
                if rubber {
                    let base = c.map_or(theme::STEP_OFF_B, theme::matte);
                    theme::rubber_pad(p, r, base, playing);
                } else {
                    let fill = c.map_or(theme::STEP_OFF_B, |c| theme::mix(theme::PANEL_HI, c, 0.95));
                    p.rect_filled(r, theme::cr(2), fill);
                    if playing {
                        p.rect_filled(r, theme::cr(2), egui::Color32::WHITE.gamma_multiply(0.12));
                        p.rect_stroke(
                            r.expand(1.0),
                            theme::cr(3),
                            theme::stroke(1.0, theme::MINT),
                            egui::StrokeKind::Outside,
                        );
                    }
                }
            }
            let c = rect.min + egui::vec2(5.0 * (w + 6.0) + 16.0, 4.0 + h / 2.0);
            if rubber {
                theme::rubber_dot(p, c, m.led * 0.32, theme::MINT, true);
                let b = egui::Rect::from_min_size(c + egui::vec2(18.0, -h / 2.0), egui::vec2(w * 1.3, h));
                theme::rubber_pad(p, b, theme::RAISED, false);
            } else {
                p.circle_filled(c, m.led * 0.43, theme::MINT.gamma_multiply(0.18));
                p.circle_filled(c, m.led * 0.27, theme::MINT);
                let b = egui::Rect::from_min_size(c + egui::vec2(18.0, -h / 2.0), egui::vec2(w * 1.3, h));
                p.rect_filled(b, theme::cr(4), theme::mix(theme::RAISED, theme::MINT, 0.22));
            }
        });
    }
}
