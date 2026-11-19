//! Drawing audio: waveforms and fade curves, shared by the arrangement's
//! audio clips and the audio-clip editor.

use crate::samples::Peaks;
use crate::theme::stroke;
use egui::{pos2, Color32, Mesh, Painter, Pos2, Rect, Shape};
use lyra_engine::fade_curve;

/// Draw a waveform into `rect`, one column per point. `at(x)` maps the
/// column starting at `x` to the frames it shows (from, to) and the gain to
/// draw them with, or None where there's nothing to draw. `zoom` scales the
/// height (1 = full scale fills the rect).
pub fn paint(
    painter: &Painter,
    rect: Rect,
    peaks: &Peaks,
    color: Color32,
    zoom: f32,
    mut at: impl FnMut(f32) -> Option<(f64, f64, f32)>,
) {
    let visible = rect.intersect(painter.clip_rect());
    if visible.width() <= 0.0 || visible.height() <= 0.0 {
        return;
    }
    let mid = rect.center().y;
    let half = rect.height() * 0.5;
    let mut mesh = Mesh::default();
    let mut x = visible.left().floor();
    while x < visible.right() {
        if let Some((a, b, gain)) = at(x) {
            if let Some((lo, hi)) = peaks.range(a, b) {
                let g = gain * zoom;
                let top = mid - (hi * g).clamp(-1.0, 1.0) * half;
                let bottom = mid - (lo * g).clamp(-1.0, 1.0) * half;
                // At least a hairline, so silence still shows where the audio is.
                let (top, bottom) = if bottom - top < 1.0 {
                    let c = (top + bottom) * 0.5;
                    (c - 0.5, c + 0.5)
                } else {
                    (top, bottom)
                };
                mesh.add_colored_rect(Rect::from_min_max(pos2(x, top), pos2(x + 1.0, bottom)), color);
            }
        }
        x += 1.0;
    }
    painter.add(Shape::mesh(mesh));
}

/// Which end of a clip a fade sits at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    In,
    Out,
}

/// The gain of a fade at column `x`, for a fade running from `x0` to `x1`.
pub fn fade_gain(end: End, x: f32, x0: f32, x1: f32, tension: f32) -> f32 {
    let w = (x1 - x0).max(1e-3);
    match end {
        End::In => fade_curve((x - x0) / w, tension),
        End::Out => fade_curve((x1 - x) / w, tension),
    }
}

/// Draw a fade's curve, and with `shade` darken the part of `rect` above it
/// (what the fade takes away). Returns the curve's midpoint: where its
/// tension handle sits.
#[allow(clippy::too_many_arguments)]
pub fn paint_fade(
    painter: &Painter,
    rect: Rect,
    end: End,
    x0: f32,
    x1: f32,
    tension: f32,
    ink: Color32,
    shade: bool,
) -> Pos2 {
    let y_of = |x: f32| rect.bottom() - fade_gain(end, x, x0, x1, tension) * rect.height();
    let steps = ((x1 - x0) / 3.0).ceil().clamp(2.0, 96.0) as usize;
    let xs: Vec<f32> = (0..=steps).map(|i| x0 + (x1 - x0) * i as f32 / steps as f32).collect();
    let line: Vec<Pos2> = xs.iter().map(|&x| pos2(x, y_of(x))).collect();
    let mx = (x0 + x1) * 0.5;
    if !shade {
        painter.add(Shape::line(line, stroke(1.0, ink)));
        return pos2(mx, y_of(mx));
    }
    let mut shade = Mesh::default();
    let dim = Color32::from_black_alpha(110);
    for w in xs.windows(2) {
        let (a, b) = (w[0], w[1]);
        let base = shade.vertices.len() as u32;
        for p in [pos2(a, rect.top()), pos2(b, rect.top()), pos2(b, y_of(b)), pos2(a, y_of(a))] {
            shade.colored_vertex(p, dim);
        }
        shade.add_triangle(base, base + 1, base + 2);
        shade.add_triangle(base, base + 2, base + 3);
    }
    painter.add(Shape::mesh(shade));
    painter.add(Shape::line(line, stroke(1.5, ink)));
    pos2(mx, y_of(mx))
}

/// A fade handle: a dot at a fade's end (drag sideways: length), in the
/// clip's own colour.
pub fn paint_handle(painter: &Painter, at: Pos2, color: Color32, hot: bool) {
    halo(painter, at, 6.0);
    painter.circle_filled(at, if hot { 4.5 } else { 3.5 }, color);
}

/// A soft dark disc behind a handle, so it reads on top of the waveform.
fn halo(painter: &Painter, at: Pos2, r: f32) {
    painter.circle_filled(at, r, Color32::from_black_alpha(110));
}

/// A tension handle: a ring on a fade's curve (drag up/down: bend it).
pub fn paint_tension(painter: &Painter, at: Pos2, color: Color32, hot: bool) {
    halo(painter, at, 6.5);
    painter.circle_stroke(at, if hot { 4.5 } else { 3.5 }, stroke(1.5, color));
}

/// The gain grip: a small up/down arrow (drag up/down: louder/quieter).
pub fn paint_gain(painter: &Painter, at: Pos2, color: Color32, hot: bool) {
    let s = if hot { 1.25 } else { 1.0 };
    halo(painter, at, 9.0 * s);
    let tri = |dir: f32| {
        let tip = at + egui::vec2(0.0, -6.0 * dir * s);
        let base = at + egui::vec2(0.0, -2.0 * dir * s);
        vec![tip, base + egui::vec2(3.5 * s, 0.0), base + egui::vec2(-3.5 * s, 0.0)]
    };
    for dir in [1.0, -1.0] {
        painter.add(Shape::convex_polygon(tri(dir), color, egui::Stroke::NONE));
    }
}

/// Tension after dragging a curve by `dy` points. Dragging up raises the
/// curve (louder sooner), which is lower tension, for both fades.
pub fn drag_tension(start: f32, dy: f32) -> f32 {
    (start + dy / 80.0).clamp(-1.0, 1.0)
}
