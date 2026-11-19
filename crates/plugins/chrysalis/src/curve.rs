//! Drawn curves: the LFO shapes (points joined by bent segments) and the bend
//! every curved segment in Chrysalis uses (LFO segments, envelope stages).
//! The editor draws with these same functions, so what you see is what plays.

/// Points per LFO curve.
pub const MAX_POINTS: usize = 32;

/// How far a bend of 1 goes: `bend(½, 1)` = 16/17 ≈ 0.94.
const BEND_RANGE: f32 = 16.0;

/// The bend's shape constant for `b` in −1..1 (see [`bend_k`]).
pub fn bend_coeff(b: f32) -> f32 {
    BEND_RANGE.powf(-b.clamp(-1.0, 1.0))
}

/// A bent ramp from 0 to 1 over `t` in 0..1, with `k` from [`bend_coeff`]:
/// `k` = 1 is straight, `k` < 1 (bend > 0) rises fast then levels off,
/// `k` > 1 starts slow. Always passes through 0 and 1; `f(½) = 1/(1 + k)`.
#[inline]
pub fn bend_k(t: f32, k: f32) -> f32 {
    let d = t + k * (1.0 - t);
    if d <= 0.0 {
        return 0.0;
    }
    t / d
}

/// [`bend_k`] with the bend amount itself (−1..1).
pub fn bend(t: f32, b: f32) -> f32 {
    bend_k(t, bend_coeff(b))
}

/// The bend that makes a ramp pass through `f` at its middle (the inverse of
/// `bend(½, b)`), for dragging a bend handle. Clamped to −1..1.
pub fn bend_through(f: f32) -> f32 {
    let f = f.clamp(1e-4, 1.0 - 1e-4);
    let k = 1.0 / f - 1.0;
    (-k.ln() / BEND_RANGE.ln()).clamp(-1.0, 1.0)
}

/// One point of a drawn curve: where (0..1), its value (0..1), and how the
/// segment *leaving* it bends (−1..1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub bend: f32,
}

/// An LFO's shape, ready to evaluate: `n` points, the first at x = 0, the
/// x's never going backwards. The last segment runs from the last point to
/// (1, first point's y), so the loop closes by itself; two points at the same
/// x make a jump.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    n: usize,
    x: [f32; MAX_POINTS],
    y: [f32; MAX_POINTS],
    /// Bend constants (from [`bend_coeff`]) of each point's outgoing segment.
    k: [f32; MAX_POINTS],
}

impl Default for Shape {
    fn default() -> Self {
        Shape::new(&[Point { x: 0.0, y: 0.0, bend: 0.0 }, Point { x: 0.5, y: 1.0, bend: 0.0 }])
    }
}

impl Shape {
    /// From points as stored (they are tidied: clamped, first x = 0, sorted
    /// by x without reordering).
    pub fn new(points: &[Point]) -> Shape {
        let n = points.len().clamp(1, MAX_POINTS);
        let mut s = Shape { n, x: [1.0; MAX_POINTS], y: [0.0; MAX_POINTS], k: [1.0; MAX_POINTS] };
        let mut last = 0.0f32;
        for (i, p) in points.iter().take(n).enumerate() {
            let x = if i == 0 { 0.0 } else { p.x.clamp(last, 1.0) };
            last = x;
            s.x[i] = x;
            s.y[i] = p.y.clamp(0.0, 1.0);
            s.k[i] = bend_coeff(p.bend);
        }
        s
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// The curve's value (0..1) at phase `ph` (0..1).
    #[inline]
    pub fn eval(&self, ph: f32) -> f32 {
        let ph = ph.clamp(0.0, 1.0);
        let mut i = 0;
        while i + 1 < self.n && self.x[i + 1] <= ph {
            i += 1;
        }
        let (x0, y0) = (self.x[i], self.y[i]);
        let (x1, y1) = if i + 1 < self.n { (self.x[i + 1], self.y[i + 1]) } else { (1.0, self.y[0]) };
        let w = x1 - x0;
        if w <= 1e-7 {
            return y1;
        }
        let t = ((ph - x0) / w).clamp(0.0, 1.0);
        y0 + (y1 - y0) * bend_k(t, self.k[i])
    }
}

/// A shape preset: its name and points (x, y, bend).
pub type ShapePreset = (&'static str, &'static [(f32, f32, f32)]);

/// The bend that makes a quarter of a sine: `bend_through(sin 45°)`.
const Q: f32 = 0.317_9;

/// The shape presets the LFO editor offers.
pub const SHAPES: [ShapePreset; 8] = [
    // Four bent quarters: within 0.06 of a real sine.
    ("Sine", &[(0.0, 0.5, Q), (0.25, 1.0, -Q), (0.5, 0.5, Q), (0.75, 0.0, -Q)]),
    ("Triangle", &[(0.0, 0.0, 0.0), (0.5, 1.0, 0.0)]),
    ("Ramp up", &[(0.0, 0.0, 0.0), (1.0, 1.0, 0.0)]),
    ("Ramp down", &[(0.0, 1.0, 0.0), (1.0, 0.0, 0.0)]),
    ("Square", &[(0.0, 1.0, 0.0), (0.5, 1.0, 0.0), (0.5, 0.0, 0.0), (1.0, 0.0, 0.0)]),
    (
        "Stairs",
        &[
            (0.0, 0.0, 0.0),
            (0.25, 0.0, 0.0),
            (0.25, 0.333, 0.0),
            (0.5, 0.333, 0.0),
            (0.5, 0.667, 0.0),
            (0.75, 0.667, 0.0),
            (0.75, 1.0, 0.0),
            (1.0, 1.0, 0.0),
        ],
    ),
    // A pluck: up at once, then a curved fall (good as a one-shot).
    ("Decay", &[(0.0, 1.0, 0.7), (1.0, 0.0, 0.0)]),
    // Two quick humps and a long one: a wobble with a rhythm.
    (
        "Wub",
        &[
            (0.0, 0.0, 0.6),
            (0.125, 1.0, -0.5),
            (0.25, 0.0, 0.6),
            (0.375, 1.0, -0.5),
            (0.5, 0.0, 0.5),
            (0.75, 1.0, -0.4),
            (1.0, 0.0, 0.0),
        ],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(list: &[(f32, f32, f32)]) -> Shape {
        Shape::new(&list.iter().map(|&(x, y, bend)| Point { x, y, bend }).collect::<Vec<_>>())
    }

    #[test]
    fn bend_keeps_its_ends_and_bends_the_middle() {
        for b in [-1.0, -0.3, 0.0, 0.5, 1.0] {
            assert_eq!(bend(0.0, b), 0.0);
            assert!((bend(1.0, b) - 1.0).abs() < 1e-6);
            assert!((bend(0.5, bend_through(bend(0.5, b))) - bend(0.5, b)).abs() < 1e-4);
        }
        assert!((bend(0.5, 0.0) - 0.5).abs() < 1e-6);
        assert!(bend(0.5, 1.0) > 0.9 && bend(0.5, -1.0) < 0.1);
        // Monotonic.
        let mut last = 0.0;
        for i in 1..=100 {
            let v = bend(i as f32 / 100.0, 0.8);
            assert!(v >= last);
            last = v;
        }
    }

    #[test]
    fn a_curve_goes_through_its_points_and_closes_the_loop() {
        let s = pts(&[(0.0, 0.2, 0.0), (0.25, 1.0, 0.0), (0.5, 0.0, 0.7)]);
        assert!((s.eval(0.0) - 0.2).abs() < 1e-6);
        assert!((s.eval(0.25) - 1.0).abs() < 1e-6);
        assert!((s.eval(0.125) - 0.6).abs() < 1e-6, "straight segment: halfway");
        assert!((s.eval(0.5) - 0.0).abs() < 1e-6);
        // The last segment bends (0.7) back up to the first point's value.
        assert!((s.eval(0.75) - 0.2 * bend(0.5, 0.7)).abs() < 1e-5);
        assert!((s.eval(0.999_999) - 0.2).abs() < 1e-3);
    }

    #[test]
    fn two_points_at_one_x_jump() {
        let sq = pts(SHAPES[4].1);
        assert_eq!(sq.eval(0.49), 1.0);
        assert_eq!(sq.eval(0.5), 0.0);
        assert_eq!(sq.eval(0.9), 0.0);
    }

    #[test]
    fn bad_points_are_tidied() {
        // Out of order and out of range: never NaN, never outside 0..1.
        let s = pts(&[(0.4, 2.0, 5.0), (0.8, -1.0, 0.0), (0.2, 0.5, -3.0)]);
        for i in 0..=200 {
            let v = s.eval(i as f32 / 200.0);
            assert!(v.is_finite() && (0.0..=1.0).contains(&v));
        }
    }

    #[test]
    fn the_sine_preset_is_close_to_a_sine() {
        let s = pts(SHAPES[0].1);
        for i in 0..200 {
            let t = i as f32 / 200.0;
            let want = 0.5 + 0.5 * (std::f32::consts::TAU * t).sin();
            assert!((s.eval(t) - want).abs() < 0.06, "t {t}: {} vs {want}", s.eval(t));
        }
    }
}
