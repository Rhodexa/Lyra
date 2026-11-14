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

    section(ui, "Material: plasma (now) vs bezel buttons", material_swatch);

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

/// Side by side: today's flat "plasma" look and backlit membrane keys, on the
/// same things (steps off / on / playing, a light, a button with an icon).
fn material_swatch(ui: &mut Ui) {
    let m = theme::m();
    let (w, h) = (m.step_w * 1.4, m.step_h * 1.2);
    for keys in [false, true] {
        ui.horizontal(|ui| {
            ui.label(RichText::new(if keys { "bezel" } else { "plasma" }).small().color(theme::TEXT_DIM));
            let (rect, _) = ui.allocate_exact_size(egui::vec2(w * 7.5 + 70.0, h + 14.0), egui::Sense::hover());
            let p = ui.painter();
            let colors = [None, Some(theme::MINT), Some(theme::ROSE), Some(theme::SKY), Some(theme::BUTTER)];
            for (i, c) in colors.iter().enumerate() {
                let r = egui::Rect::from_min_size(rect.min + egui::vec2(i as f32 * (w + 8.0), 4.0), egui::vec2(w, h));
                let playing = i == 3;
                if keys {
                    match c {
                        Some(c) => theme::key(p, r, *c, true, playing),
                        None => theme::key(p, r, theme::STEP_OFF_B, false, false),
                    }
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
            let x = rect.min.x + 5.0 * (w + 8.0) + 8.0;
            for (k, lit) in [false, true].into_iter().enumerate() {
                let b = egui::Rect::from_min_size(
                    egui::pos2(x + k as f32 * (h + 10.0), rect.min.y + 4.0),
                    egui::vec2(h, h),
                );
                if keys {
                    theme::key(p, b, if lit { theme::MINT } else { theme::RAISED }, lit, false);
                    piano_glyph(p, b.shrink(h * 0.25), theme::key_ink(lit));
                } else {
                    p.rect_filled(
                        b,
                        theme::cr(4),
                        if lit { theme::mix(theme::RAISED, theme::MINT, 0.22) } else { theme::RAISED },
                    );
                    piano_glyph(p, b.shrink(h * 0.25), if lit { theme::MINT } else { theme::TEXT_DIM });
                }
            }
        });
    }
}

/// The piano icon from the reference, drawn simply (until the real SVG lands).
fn piano_glyph(p: &egui::Painter, r: egui::Rect, ink: egui::Color32) {
    let s = theme::stroke((r.height() * 0.1).max(1.5), ink);
    p.rect_stroke(r, theme::cr(2), s, egui::StrokeKind::Inside);
    for i in 1..4 {
        let x = r.left() + r.width() * i as f32 / 4.0;
        p.vline(x, (r.center().y)..=r.bottom(), theme::stroke(s.width * 0.8, ink));
    }
    for i in [1.0, 2.0, 3.0] {
        let x = r.left() + r.width() * i / 4.0;
        let key = egui::Rect::from_center_size(
            egui::pos2(x, r.top() + r.height() * 0.32),
            egui::vec2(r.width() * 0.13, r.height() * 0.55),
        );
        p.rect_filled(key, theme::cr(1), ink);
    }
}
