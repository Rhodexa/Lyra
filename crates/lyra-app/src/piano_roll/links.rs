//! Links in the piano roll: a note that glides into a later one.
//!
//! * hover a note: a **dot** past its right end. **Drag it onto a later note**
//!   and the two are linked: one sound that glides between them (on synths
//!   that glide; others play the notes as written).
//! * the link is drawn as the pitch's way: along the first note's row, then
//!   a curve into the second note's start.
//! * the **dot** where the curve begins — drag sideways: when the glide starts
//!   (all the way to the second note's start: no glide, the pitch just
//!   changes, legato). **Alt**: off the grid.
//! * the **ring** on the curve — drag up/down: bend it (starts slow and
//!   arrives fast, or the other way). **Double-click**: straight again.
//! * **right-click** the curve, the dot or the ring — unlink.
//!
//! Moving one end of a link up or down keeps it (the link follows), and
//! deleting either end removes it.

use super::{PianoRoll, View};
use crate::theme::stroke;
use crate::timeline;
use crate::waveform;
use egui::{pos2, Color32, Painter, Pos2, Rect};
use lyra_model::{Note, NoteLink, TICKS_PER_STEP};

/// Grab distance for the dots, the ring and the curve.
const GRAB: f32 = 7.0;
/// How far past a note's right end its link dot sits.
const GRIP_OUT: f32 = 7.0;

/// Where a link is on screen.
pub(super) struct Shape {
    /// The pitch's way: along the first note's row, then the glide.
    pub way: Vec<Pos2>,
    /// The dot where the glide starts, and the ring halfway along it (none
    /// when the glide is too short or flat to bend).
    pub dot: Pos2,
    pub ring: Option<Pos2>,
    /// The glide's box (its corners: start pitch, end pitch), for bending.
    pub span: Rect,
    pub rising: bool,
}

/// The link of note `i`, as drawn (None: no link, or nothing to link to).
pub(super) fn shape(v: &View, notes: &[Note], i: usize) -> Option<Shape> {
    let a = notes.get(i)?;
    let link = a.link?;
    let b = &notes[a.link_target(notes)?];
    let (arrive, glide) = (b.start, link.glide.min(b.start - a.start));
    let from = arrive - glide;
    let row = |key: u8| v.y(key) + v.row_h * 0.5;
    let (ya, yb) = (row(a.key), row(b.key));
    let (x0, x1) = (v.x(from as f32), v.x(arrive as f32));
    let mut way = vec![pos2(v.x((a.start + a.len).min(from) as f32), ya)];
    for k in 0..=24 {
        let t = k as f32 / 24.0;
        way.push(pos2(x0 + (x1 - x0) * t, ya + (yb - ya) * lyra_engine::fade_curve(t, link.tension)));
    }
    let mid = lyra_engine::fade_curve(0.5, link.tension);
    let ring = (x1 - x0 >= 18.0 && (yb - ya).abs() >= v.row_h).then(|| pos2((x0 + x1) * 0.5, ya + (yb - ya) * mid));
    let span = Rect::from_two_pos(pos2(x0, ya), pos2(x1, yb));
    Some(Shape { way, dot: pos2(x0, ya), ring, span, rising: yb < ya })
}

/// Where the "drag me onto a note" dot of note `i` sits.
pub(super) fn grip(v: &View, n: &Note) -> Pos2 {
    let r = v.note_rect(n);
    pos2(r.right() + GRIP_OUT, r.center().y)
}

/// What part of a link is under the mouse.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Part {
    Dot(usize),
    Ring(usize),
    Curve(usize),
}

/// The link part under `pos`, if any (dots and rings before curves).
pub(super) fn hit(v: &View, notes: &[Note], pos: Pos2) -> Option<Part> {
    let shapes: Vec<(usize, Shape)> = (0..notes.len()).filter_map(|i| shape(v, notes, i).map(|s| (i, s))).collect();
    for (i, s) in &shapes {
        if s.ring.is_some_and(|r| r.distance(pos) <= GRAB) {
            return Some(Part::Ring(*i));
        }
        if s.dot.distance(pos) <= GRAB {
            return Some(Part::Dot(*i));
        }
    }
    shapes.iter().find(|(_, s)| near_line(&s.way, pos, GRAB * 0.6)).map(|(i, _)| Part::Curve(*i))
}

fn near_line(pts: &[Pos2], pos: Pos2, d: f32) -> bool {
    pts.windows(2).any(|w| {
        let (a, b) = (w[0], w[1]);
        let ab = b - a;
        let t = if ab.length_sq() < 1e-6 { 0.0 } else { ((pos - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) };
        (a + ab * t).distance(pos) <= d
    })
}

/// A new link from `a` to `b` (a later note): the glide spans the gap
/// between them, or a step when they touch.
pub(super) fn new_link(a: &Note, b: &Note) -> NoteLink {
    let gap = b.start.saturating_sub(a.start + a.len);
    let glide = if gap > 0 { gap } else { TICKS_PER_STEP.min(b.start - a.start) };
    NoteLink { to: (b.key as i32 - a.key as i32).clamp(-127, 127) as i8, glide, tension: 0.0 }
}

/// Remove notes (indices, any order), and the links that point at them.
pub(super) fn remove(notes: &mut Vec<Note>, mut gone: Vec<usize>) {
    gone.sort_unstable_by(|a, b| b.cmp(a));
    gone.dedup();
    for k in gone {
        unlink_target(notes, k);
        notes.remove(k);
    }
}

/// Links that point at note `k` (about to be removed) are removed with it.
pub(super) fn unlink_target(notes: &mut [Note], k: usize) {
    let sources: Vec<usize> = (0..notes.len()).filter(|&i| notes[i].link_target(notes) == Some(k)).collect();
    for i in sources {
        notes[i].link = None;
    }
}

/// Links with exactly one end among `moving`: (source note, its `to`, +1 if
/// the target moves, -1 if the source does). A move by `dk` semitones then
/// keeps them: `to` = orig + sign × dk.
pub(super) fn stretched(notes: &[Note], moving: &[usize]) -> Vec<(usize, i8, i32)> {
    (0..notes.len())
        .filter_map(|i| {
            let (link, t) = (notes[i].link?, notes[i].link_target(notes)?);
            match (moving.contains(&i), moving.contains(&t)) {
                (false, true) => Some((i, link.to, 1)),
                (true, false) => Some((i, link.to, -1)),
                _ => None,
            }
        })
        .collect()
}

impl PianoRoll {
    /// The links of the channel being edited, over its notes; the dot of the
    /// note under the mouse; a link being drawn.
    pub(super) fn paint_links(&self, painter: &Painter, v: &View, notes: &[Note], color: Color32, hover: Option<Pos2>) {
        let hot = hover.and_then(|p| hit(v, notes, p));
        for i in 0..notes.len() {
            let Some(s) = shape(v, notes, i) else { continue };
            let lit = matches!(hot, Some(Part::Curve(j) | Part::Dot(j) | Part::Ring(j)) if j == i);
            painter.add(egui::Shape::line(s.way.clone(), stroke(if lit { 3.0 } else { 2.0 }, color)));
            waveform::paint_handle(painter, s.dot, color, hot == Some(Part::Dot(i)), waveform::SMALL);
            if let Some(r) = s.ring {
                waveform::paint_tension(painter, r, color, hot == Some(Part::Ring(i)), waveform::SMALL);
            }
        }
        // The dot to start a link from, on the note under the mouse.
        if let (Some(p), None, super::Drag::None) = (hover, hot, &self.drag) {
            let over = (0..notes.len()).rev().find(|&i| {
                let r = v.note_rect(&notes[i]).expand2(egui::vec2(GRIP_OUT + GRAB, 0.0));
                r.contains(p)
            });
            if let Some(i) = over.filter(|&i| notes[i].link.is_none() || shape(v, notes, i).is_none()) {
                let g = grip(v, &notes[i]);
                waveform::paint_handle(painter, g, color, g.distance(p) <= GRAB, waveform::SMALL);
            }
        }
        if let (super::Drag::LinkNew { from }, Some(p)) = (&self.drag, hover) {
            if let Some(a) = notes.get(*from) {
                let g = grip(v, a);
                painter.line_segment([g, p], stroke(2.0, color.gamma_multiply(0.7)));
                waveform::paint_handle(painter, g, color, true, waveform::SMALL);
            }
        }
    }

    /// The note whose link dot (the one to start a new link) is under `pos`.
    pub(super) fn grip_hit(&self, v: &View, notes: &[Note], pos: Pos2) -> Option<usize> {
        (0..notes.len()).rev().find(|&i| {
            (notes[i].link.is_none() || shape(v, notes, i).is_none()) && grip(v, &notes[i]).distance(pos) <= GRAB
        })
    }

    /// Dragging a link's dot: when the glide starts (snapped unless `free`).
    pub(super) fn drag_link_start(&self, v: &View, notes: &mut [Note], i: usize, x: f32, free: bool) {
        let Some(t) = notes[i].link_target(notes) else { return };
        let (start, arrive) = (notes[i].start, notes[t].start);
        let snap = if free { 1 } else { self.snap };
        let at = timeline::snap(v.tick(x), snap).clamp(start as f32, arrive as f32) as u32;
        if let Some(l) = &mut notes[i].link {
            l.glide = arrive - at;
        }
    }

    /// Dragging a link's ring (grabbed at `anchor_y` with tension `orig`).
    pub(super) fn drag_link_bend(v: &View, notes: &mut [Note], i: usize, orig: f32, anchor_y: f32, y: f32) {
        let Some(s) = shape(v, notes, i) else { return };
        // `drag_tension` thinks of a curve rising from the box's bottom to its
        // top; a falling glide is the same curve upside down.
        let (anchor_y, y) = if s.rising {
            (anchor_y, y)
        } else {
            (s.span.top() + s.span.bottom() - anchor_y, s.span.top() + s.span.bottom() - y)
        };
        let tension = waveform::drag_tension(orig, anchor_y, y, s.span);
        if let Some(l) = &mut notes[i].link {
            l.tension = tension;
        }
    }
}
