//! A tiny flexbox: split a rectangle into a row or a column of parts, each a
//! fixed size (`Px`) or a share of what's left (`Grow`), with gaps between.
//! Layout by arithmetic, so a panel's pieces line up and fill it exactly
//! instead of being placed by hand.
//!
//! ```ignore
//! let [name, field] = flex::row(r, [Px(80.0), Grow(1.0)], 6.0);
//! let cells = flex::even_row(r, 7, 4.0); // seven equal columns
//! ```

use egui::{pos2, vec2, Rect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Part {
    /// This many points.
    Px(f32),
    /// A share of the room the fixed parts and gaps leave.
    Grow(f32),
}
pub use Part::{Grow, Px};

/// Where each part starts and how long it is, along one axis.
fn split<const N: usize>(start: f32, len: f32, parts: [Part; N], gap: f32) -> [(f32, f32); N] {
    let fixed: f32 = parts.iter().map(|p| if let Px(x) = p { *x } else { 0.0 }).sum();
    let weight: f32 = parts.iter().map(|p| if let Grow(w) = p { *w } else { 0.0 }).sum();
    let gaps = gap * N.saturating_sub(1) as f32;
    let free = (len - fixed - gaps).max(0.0);
    let mut at = start;
    parts.map(|p| {
        let size = match p {
            Px(x) => x,
            Grow(w) if weight > 0.0 => free * w / weight,
            Grow(_) => 0.0,
        };
        let out = (at, size);
        at += size + gap;
        out
    })
}

/// Parts side by side, left to right, each the full height of `r`.
pub fn row<const N: usize>(r: Rect, parts: [Part; N], gap: f32) -> [Rect; N] {
    split(r.left(), r.width(), parts, gap).map(|(x, w)| Rect::from_min_size(pos2(x, r.top()), vec2(w, r.height())))
}

/// Parts stacked, top to bottom, each the full width of `r`.
pub fn col<const N: usize>(r: Rect, parts: [Part; N], gap: f32) -> [Rect; N] {
    split(r.top(), r.height(), parts, gap).map(|(y, h)| Rect::from_min_size(pos2(r.left(), y), vec2(r.width(), h)))
}

/// `n` equal columns.
pub fn even_row(r: Rect, n: usize, gap: f32) -> Vec<Rect> {
    let w = ((r.width() - gap * n.saturating_sub(1) as f32) / n.max(1) as f32).max(0.0);
    (0..n).map(|i| Rect::from_min_size(pos2(r.left() + i as f32 * (w + gap), r.top()), vec2(w, r.height()))).collect()
}

/// `n` equal rows.
pub fn even_col(r: Rect, n: usize, gap: f32) -> Vec<Rect> {
    let h = ((r.height() - gap * n.saturating_sub(1) as f32) / n.max(1) as f32).max(0.0);
    (0..n).map(|i| Rect::from_min_size(pos2(r.left(), r.top() + i as f32 * (h + gap)), vec2(r.width(), h))).collect()
}

/// A `size` box centred in `r`.
pub fn center(r: Rect, size: egui::Vec2) -> Rect {
    Rect::from_center_size(r.center(), size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_and_growing_parts_fill_the_row_exactly() {
        let r = Rect::from_min_size(pos2(10.0, 0.0), vec2(300.0, 20.0));
        let [a, b, c] = row(r, [Px(50.0), Grow(1.0), Grow(3.0)], 10.0);
        assert_eq!((a.left(), a.width()), (10.0, 50.0));
        assert_eq!((b.left(), b.width()), (70.0, 57.5));
        assert_eq!((c.left(), c.width()), (137.5, 172.5));
        assert_eq!(c.right(), r.right());
        assert!([a, b, c].iter().all(|x| x.height() == 20.0));
    }

    #[test]
    fn even_splits_and_columns() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let cells = even_row(r, 4, 4.0);
        assert_eq!(cells.len(), 4);
        assert!((cells[3].right() - 100.0).abs() < 1e-4);
        let [top, rest] = col(r, [Px(10.0), Grow(1.0)], 5.0);
        assert_eq!((top.height(), rest.top(), rest.bottom()), (10.0, 15.0, 50.0));
        assert!((even_col(r, 2, 0.0)[1].top() - 25.0).abs() < 1e-4);
    }

    #[test]
    fn too_little_room_never_goes_negative() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(20.0, 20.0));
        let [_, g] = row(r, [Px(50.0), Grow(1.0)], 4.0);
        assert_eq!(g.width(), 0.0);
    }
}
