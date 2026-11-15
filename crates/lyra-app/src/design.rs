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

    section(ui, "Control library", material_swatch);

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

/// The control library (from the user's HTML studies), painted by the real
/// theme code, so what you see here is what the app uses.
fn material_swatch(ui: &mut Ui) {
    let m = theme::m();
    let p = ui.painter().clone();
    let (s, gap) = (m.step_h * 1.7, 5.0);

    // Eight lights, then unlit, then a playing (hot) step.
    let (row, _) = ui.allocate_exact_size(egui::vec2(10.0 * (s + gap), s), egui::Sense::hover());
    for (i, l) in theme::LIGHTS.iter().enumerate() {
        let r = egui::Rect::from_min_size(row.min + egui::vec2(i as f32 * (s + gap), 0.0), egui::vec2(s, s));
        theme::key(&p, r, Some(*l), false, 1.0);
        let ink = theme::key_ink(true);
        p.text(r.center(), egui::Align2::CENTER_CENTER, format!("{:02}", i + 1), egui::FontId::monospace(11.0), ink);
    }
    for (k, hot) in [false, true].into_iter().enumerate() {
        let r = egui::Rect::from_min_size(row.min + egui::vec2((8 + k) as f32 * (s + gap), 0.0), egui::vec2(s, s));
        theme::key(&p, r, None, hot, 1.0);
        p.text(r.center(), egui::Align2::CENTER_CENTER, "00", egui::FontId::monospace(11.0), theme::key_ink(false));
    }
    ui.add_space(6.0);

    // Steps at real size: a bar with beats grouped, some lit.
    let (row, _) = ui.allocate_exact_size(egui::vec2(16.0 * (m.step_w + 2.0) + 12.0, m.step_h), egui::Sense::hover());
    for i in 0..16u32 {
        let x = i as f32 * (m.step_w + 2.0) + (i / 4) as f32 * 4.0;
        let r = egui::Rect::from_min_size(row.min + egui::vec2(x, 0.0), egui::vec2(m.step_w, m.step_h));
        let lit = [0, 4, 8, 10, 12].contains(&i).then_some(theme::L_MINT);
        theme::key(&p, r, lit, i == 10, if (i / 4) % 2 == 0 { 1.0 } else { 1.13 });
    }
    ui.add_space(6.0);

    // Big toggle keys with the piano glyph, LEDs, LCD.
    let big = m.control_h * 2.4;
    let (row, _) = ui.allocate_exact_size(egui::vec2(big * 2.0 + 260.0, big), egui::Sense::hover());
    for (k, lit) in [false, true].into_iter().enumerate() {
        let r = egui::Rect::from_min_size(row.min + egui::vec2(k as f32 * (big + 8.0), 0.0), egui::vec2(big, big));
        theme::key(&p, r, lit.then_some(theme::L_MINT), false, 1.0);
        piano_glyph(&p, theme::key_face(r).shrink(big * 0.2), theme::key_ink(lit));
    }
    let x = row.min.x + 2.0 * (big + 8.0) + 6.0;
    for (k, on) in [false, true].into_iter().enumerate() {
        theme::led(
            &p,
            egui::pos2(x + 14.0 + k as f32 * 34.0, row.center().y),
            28.0,
            on.then_some(theme::L_MINT),
            false,
        );
    }
    let lcd = egui::Rect::from_min_size(egui::pos2(x + 80.0, row.center().y - 31.0), egui::vec2(125.0, 62.0));
    theme::lcd_panel(&p, lcd);
    p.text(
        lcd.center() - egui::vec2(0.0, 6.0),
        egui::Align2::CENTER_CENTER,
        "110.0",
        egui::FontId::monospace(22.0),
        theme::LCD_INK,
    );
    p.text(
        lcd.center() + egui::vec2(0.0, 16.0),
        egui::Align2::CENTER_CENTER,
        "BPM",
        egui::FontId::proportional(9.0),
        theme::LCD_INK_DIM,
    );
    ui.add_space(6.0);

    // Knobs: each family, continuous / bipolar / stepped.
    use lyra_plugin::{Curve, ParamInfo};
    static CONT: ParamInfo = ParamInfo::new(0, "Level", 0.0, 1.0, 0.5, "%", Curve::Linear);
    static PAN: ParamInfo = ParamInfo::new(0, "Pan", -1.0, 1.0, 0.0, "pan", Curve::Linear);
    static STEPS: ParamInfo = ParamInfo::choice(0, "Mode", &["1", "2", "3", "4", "5", "6", "7", "8", "9"], 4);
    let sizes = [
        (m.knob_small, &PAN, -0.4),
        (m.knob_small, &CONT, 0.45),
        (45.0, &CONT, 0.45),
        (55.0, &CONT, 0.55),
        (55.0, &STEPS, 4.0),
        (72.0, &CONT, 0.72),
        (72.0, &STEPS, 4.0),
    ];
    let w: f32 = sizes.iter().map(|(d, ..)| d + 14.0).sum();
    let (row, _) = ui.allocate_exact_size(egui::vec2(w, 72.0), egui::Sense::hover());
    let mut x = row.min.x;
    for (d, info, v) in sizes {
        let r = egui::Rect::from_center_size(egui::pos2(x + d * 0.5, row.center().y), egui::vec2(d, d));
        crate::widgets::paint_knob(&p, r, info, v, theme::MINT, false);
        x += d + 14.0;
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
