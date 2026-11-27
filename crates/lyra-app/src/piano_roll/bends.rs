//! Bends in the piano roll: a note that glides to another pitch by its end.
//! An articulation of the note itself ("be there by the end"); it knows
//! nothing of other notes.
//!
//! * hover a note: a **dot** past its right end. **Drag it up or down**: where
//!   the note arrives (in semitones; let go on its own row: no bend).
//! * the bend is drawn as the pitch's way: along the note's row, then a curve
//!   up or down to its end, with the pitch it arrives at written there.
//! * the **dot at the end** — drag up/down: where it arrives
//! * the **dot where the curve begins** — drag sideways: when the glide starts
//!   (at first, with the note). **Alt**: off the grid.
//! * the **ring** on the curve — drag up/down: bend it (starts slow and
//!   arrives fast, or the other way). **Double-click**: straight again.
//! * **right-click** the curve or a dot — no bend.
//!
//! Synths that glide (Chrysalis so far) play it; others play the note flat.

use super::{PianoRoll, View};
use crate::theme::stroke;
use crate::timeline;
use crate::waveform;
use egui::{pos2, Align2, Color32, FontId, Painter, Pos2, Rect};
use lyra_model::{theory, Note, NoteBend};

/// Grab distance for the dots, the ring and the curve.
const GRAB: f32 = 7.0;
/// How far past a note's right end its "pull me" dot sits (a bent note's end
/// dot sits there too, so the dot you pulled stays under the mouse).
const GRIP_OUT: f32 = 7.0;

/// Where a bend is on screen.
pub(super) struct Shape {
    /// The pitch's way: along the note's row, then the glide.
    pub way: Vec<Pos2>,
    /// The dot where the glide starts, the dot where it arrives, and the ring
    /// halfway along (none when the glide is too short or flat to bend).
    pub start: Pos2,
    pub end: Pos2,
    pub ring: Option<Pos2>,
    /// The glide's box (start pitch to end pitch), for bending.
    pub span: Rect,
    pub rising: bool,
    pub key: u8,
}

/// Note `i`'s bend, as drawn (None: no bend).
pub(super) fn shape(v: &View, notes: &[Note], i: usize) -> Option<Shape> {
    let n = notes.get(i)?;
    let (from, len, key) = n.glide()?;
    let tension = n.bend.map_or(0.0, |b| b.tension);
    let row = |key: u8| v.y(key) + v.row_h * 0.5;
    let (ya, yb) = (row(n.key), row(key));
    let (x0, x1) = (v.x(from as f32), v.x((from + len) as f32));
    let mut way = vec![pos2(v.x(n.start as f32), ya)];
    for k in 0..=24 {
        let t = k as f32 / 24.0;
        way.push(pos2(x0 + (x1 - x0) * t, ya + (yb - ya) * lyra_engine::fade_curve(t, tension)));
    }
    let end = pos2(x1 + GRIP_OUT, yb);
    way.push(end);
    let mid = lyra_engine::fade_curve(0.5, tension);
    let ring = (x1 - x0 >= 18.0 && (yb - ya).abs() >= v.row_h).then(|| pos2((x0 + x1) * 0.5, ya + (yb - ya) * mid));
    let span = Rect::from_two_pos(pos2(x0, ya), pos2(x1, yb));
    Some(Shape { way, start: pos2(x0, ya), end, ring, span, rising: yb < ya, key })
}

/// Where the "pull me" dot of a note without a bend sits.
fn grip(v: &View, n: &Note) -> Pos2 {
    let r = v.note_rect(n);
    pos2(r.right() + GRIP_OUT, r.center().y)
}

/// What part of a bend is under the mouse.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Part {
    Start(usize),
    End(usize),
    Ring(usize),
    Curve(usize),
}

/// The bend part under `pos`, if any (dots and rings before curves).
pub(super) fn hit(v: &View, notes: &[Note], pos: Pos2) -> Option<Part> {
    let shapes: Vec<(usize, Shape)> = (0..notes.len()).filter_map(|i| shape(v, notes, i).map(|s| (i, s))).collect();
    for (i, s) in shapes.iter().rev() {
        if s.end.distance(pos) <= GRAB {
            return Some(Part::End(*i));
        }
        if s.ring.is_some_and(|r| r.distance(pos) <= GRAB) {
            return Some(Part::Ring(*i));
        }
        if s.start.distance(pos) <= GRAB {
            return Some(Part::Start(*i));
        }
    }
    shapes.iter().rev().find(|(_, s)| near_line(&s.way, pos, GRAB * 0.6)).map(|(i, _)| Part::Curve(*i))
}

fn near_line(pts: &[Pos2], pos: Pos2, d: f32) -> bool {
    pts.windows(2).any(|w| {
        let (a, b) = (w[0], w[1]);
        let ab = b - a;
        let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((pos - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
        (a + ab * t).distance(pos) <= d
    })
}

/// The note (without a bend) whose "pull me" dot is under `pos`.
pub(super) fn grip_hit(v: &View, notes: &[Note], pos: Pos2) -> Option<usize> {
    (0..notes.len()).rev().find(|&i| notes[i].bend.is_none() && grip(v, &notes[i]).distance(pos) <= GRAB)
}

impl PianoRoll {
    /// The bends of the channel being edited, over its notes; and the dot of
    /// the note under the mouse.
    pub(super) fn paint_bends(&self, painter: &Painter, v: &View, notes: &[Note], color: Color32, hover: Option<Pos2>) {
        let hot = hover.and_then(|p| hit(v, notes, p));
        for i in 0..notes.len() {
            let Some(s) = shape(v, notes, i) else { continue };
            let lit = matches!(hot, Some(Part::Curve(j) | Part::Start(j) | Part::End(j) | Part::Ring(j)) if j == i);
            painter.add(egui::Shape::line(s.way.clone(), stroke(if lit { 3.0 } else { 2.0 }, color)));
            waveform::paint_handle(painter, s.start, color, hot == Some(Part::Start(i)), waveform::SMALL);
            waveform::paint_handle(painter, s.end, color, hot == Some(Part::End(i)), waveform::SMALL);
            if let Some(r) = s.ring {
                waveform::paint_tension(painter, r, color, hot == Some(Part::Ring(i)), waveform::SMALL);
            }
            // Where it arrives, like a note's own name.
            if v.row_h >= 10.0 {
                let at = s.end + egui::vec2(7.0, 0.0);
                painter.text(at, Align2::LEFT_CENTER, theory::note_name(s.key), FontId::proportional(9.0), color);
            }
        }
        // The dot to pull a bend from, on the note under the mouse.
        if let (Some(p), None, super::Drag::None) = (hover, hot, &self.drag) {
            let over = (0..notes.len()).rev().find(|&i| {
                notes[i].bend.is_none() && v.note_rect(&notes[i]).expand2(egui::vec2(GRIP_OUT + GRAB, 0.0)).contains(p)
            });
            if let Some(i) = over {
                let g = grip(v, &notes[i]);
                waveform::paint_handle(painter, g, color, g.distance(p) <= GRAB, waveform::SMALL);
            }
        }
    }

    /// Dragging the end dot: where the note arrives (the row under the mouse).
    pub(super) fn drag_bend_to(v: &View, notes: &mut [Note], i: usize, y: f32) {
        let Some(n) = notes.get_mut(i) else { return };
        let to = (v.key(y) - n.key as i32).clamp(-127, 127) as i8;
        let b = n.bend.get_or_insert(NoteBend { to: 0, from: 0, tension: 0.0 });
        b.to = to;
    }

    /// Dragging the start dot: when the glide starts (snapped unless `free`).
    pub(super) fn drag_bend_start(&self, v: &View, notes: &mut [Note], i: usize, x: f32, free: bool) {
        let Some(n) = notes.get_mut(i) else { return };
        let snap = if free { 1 } else { self.snap };
        let at = timeline::snap(v.tick(x), snap).clamp(n.start as f32, (n.start + n.len) as f32) as u32;
        if let Some(b) = &mut n.bend {
            b.from = at - n.start;
        }
    }

    /// Dragging the ring (grabbed at `anchor_y` with tension `orig`).
    pub(super) fn drag_bend_shape(v: &View, notes: &mut [Note], i: usize, orig: f32, anchor_y: f32, y: f32) {
        let Some(s) = shape(v, notes, i) else { return };
        // `drag_tension` thinks of a curve rising from the box's bottom to its
        // top; a falling glide is the same curve upside down.
        let flip = |y: f32| s.span.top() + s.span.bottom() - y;
        let (anchor_y, y) = if s.rising { (anchor_y, y) } else { (flip(anchor_y), flip(y)) };
        let tension = waveform::drag_tension(orig, anchor_y, y, s.span);
        if let Some(b) = &mut notes[i].bend {
            b.tension = tension;
        }
    }
}
