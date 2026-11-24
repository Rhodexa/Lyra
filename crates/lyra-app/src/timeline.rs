//! Pieces shared by the time editors (arrangement, piano roll, audio clip):
//! the overview bar (drag the thumb to pan, drag its ends to zoom, wheel over
//! it to zoom), its vertical twin for rows (`VBar`), the bar ruler, and the
//! input grammar: reading the mouse, the wheel scheme, double-clicks,
//! snapping, edge auto-pan, the rubber band and the playhead.

use crate::theme::{self, cr, stroke};
use egui::{pos2, Align2, FontId, Painter, Pos2, Rect};
use lyra_model::TICKS_PER_BAR;

/// The horizontal view of a time editor: where it starts and how zoomed it is.
#[derive(Clone, Copy, Debug)]
pub struct TimeView {
    /// Tick at the left edge.
    pub scroll: f32,
    /// Pixels per tick.
    pub ppt: f32,
}

impl TimeView {
    pub fn x(&self, area: Rect, tick: f32) -> f32 {
        area.left() + (tick - self.scroll) * self.ppt
    }
    pub fn tick(&self, area: Rect, x: f32) -> f32 {
        (x - area.left()) / self.ppt + self.scroll
    }
    pub fn visible(&self, area: Rect) -> f32 {
        area.width() / self.ppt
    }
    /// Zoom by `factor`, keeping the tick under screen-x `anchor_x` still.
    pub fn zoom_at(&mut self, area: Rect, factor: f32, anchor_x: f32, min: f32, max: f32) {
        let anchor = self.tick(area, anchor_x);
        self.ppt = (self.ppt * factor).clamp(min, max);
        self.scroll = (anchor - (anchor_x - area.left()) / self.ppt).max(0.0);
    }
}

/// An overview-bar drag in progress.
#[derive(Clone, Copy, Debug)]
pub enum OverviewDrag {
    /// Grabbed the thumb's middle, `grab` ticks right of its left end.
    Pan { grab: f32 },
    /// Grabbed one end; the other end stays where it was.
    Edge { left: bool, fixed: f32 },
}

/// The overview bar for content `content` ticks long.
pub struct Overview {
    pub rect: Rect,
    /// The area the view scrolls (same width as `rect`).
    pub area: Rect,
    pub content: f32,
}

impl Overview {
    /// Ticks the bar represents: the content plus room to grow, and always a
    /// bit more than is visible so the thumb never fills the bar (a full-width
    /// thumb doesn't look grabbable).
    pub fn span(&self, v: &TimeView) -> f32 {
        (self.content + 2.0 * TICKS_PER_BAR as f32).max(v.scroll + v.visible(self.area) * 1.2)
    }

    fn at(&self, v: &TimeView, x: f32) -> f32 {
        ((x - self.rect.left()) / self.rect.width() * self.span(v)).max(0.0)
    }

    pub fn thumb(&self, v: &TimeView) -> Rect {
        let span = self.span(v);
        let l = self.rect.left() + v.scroll / span * self.rect.width();
        let r = self.rect.left() + (v.scroll + v.visible(self.area)) / span * self.rect.width();
        Rect::from_min_max(pos2(l, self.rect.top() + 1.0), pos2(r.max(l + 14.0), self.rect.bottom() - 1.0))
    }

    /// Mouse pressed on the bar: returns the drag to start.
    pub fn press(&self, v: &mut TimeView, pos: Pos2) -> OverviewDrag {
        let thumb = self.thumb(v);
        let visible = v.visible(self.area);
        // The zoom handles: 14 px inside each end of the window (and a little past it).
        if pos.x > thumb.left() - 3.0 && pos.x < thumb.left() + HANDLE {
            OverviewDrag::Edge { left: true, fixed: v.scroll + visible }
        } else if pos.x < thumb.right() + 3.0 && pos.x > thumb.right() - HANDLE {
            OverviewDrag::Edge { left: false, fixed: v.scroll }
        } else if thumb.x_range().contains(pos.x) {
            OverviewDrag::Pan { grab: self.at(v, pos.x) - v.scroll }
        } else {
            // Clicked the track: centre the view there and keep panning.
            v.scroll = (self.at(v, pos.x) - visible * 0.5).max(0.0);
            OverviewDrag::Pan { grab: visible * 0.5 }
        }
    }

    pub fn drag(&self, v: &mut TimeView, drag: OverviewDrag, pos: Pos2, min_ticks: f32, max_ppt: f32) {
        let at = self.at(v, pos.x);
        match drag {
            OverviewDrag::Pan { grab } => v.scroll = (at - grab).max(0.0),
            OverviewDrag::Edge { left, fixed } => {
                let (a, b) = if left { (at.min(fixed - min_ticks), fixed) } else { (fixed, at.max(fixed + min_ticks)) };
                v.ppt = (self.area.width() / (b - a)).clamp(0.005, max_ppt);
                v.scroll = a.max(0.0);
            }
        }
    }

    /// Wheel over the bar: zoom, keeping the view's left edge where it is
    /// (zooming around the mouse made the content slide away under you).
    pub fn wheel_zoom(&self, v: &mut TimeView, wheel_y: f32, min_ppt: f32, max_ppt: f32) {
        if wheel_y != 0.0 {
            v.zoom_at(self.area, (wheel_y * 0.004).exp(), self.area.left(), min_ppt, max_ppt);
        }
    }

    /// The track, then `mini` (the content's miniature), then the window over
    /// it: the miniature never covers the thumb.
    pub fn paint(&self, painter: &Painter, v: &TimeView, hot: bool, mini: impl FnOnce()) {
        paint_track(painter, self.rect);
        mini();
        let thumb = self.thumb(v);
        paint_thumb(painter, thumb, hot);
        // The zoom handles: a grip line in the middle of each 14 px end.
        for x in [thumb.left() + HANDLE * 0.5, thumb.right() - HANDLE * 0.5] {
            let y = thumb.y_range();
            let h = y.span() * 0.22;
            painter.vline(x, egui::Rangef::new(y.min + h, y.max - h), stroke(2.0, if hot { theme::INK } else { theme::INK_DIM }));
        }
    }

    /// Where in the bar a tick lands (for drawing content silhouettes).
    pub fn x_of(&self, v: &TimeView, tick: f32) -> f32 {
        self.rect.left() + tick / self.span(v) * self.rect.width()
    }
}

/// Bar numbers, the content's end (gold) and the playhead (mint marker; a
/// bright line while playing, a dim one where Play will start when stopped).
pub fn paint_ruler(painter: &Painter, v: &TimeView, ruler: Rect, end: f32, playhead: Option<(f32, bool)>, grid: Rect) {
    painter.rect_filled(ruler, cr(0), theme::TOOLBAR);
    painter.hline(ruler.x_range(), ruler.bottom() - 0.5, stroke(1.0, theme::LINE));
    let bar_px = TICKS_PER_BAR as f32 * v.ppt;
    let every = [1, 2, 4, 8, 16, 32].into_iter().find(|n| bar_px * *n as f32 >= 34.0).unwrap_or(64);
    let first = (v.scroll / TICKS_PER_BAR as f32).floor() as u32;
    let last = (v.tick(ruler, ruler.right()) / TICKS_PER_BAR as f32).ceil() as u32;
    for bar in (first..=last).filter(|b| b % every == 0) {
        let x = v.x(ruler, (bar * TICKS_PER_BAR) as f32);
        painter.vline(x, ruler.y_range(), stroke(1.0, theme::LINE));
        painter.text(
            pos2(x + 3.0, ruler.center().y),
            Align2::LEFT_CENTER,
            format!("{}", bar + 1),
            FontId::proportional(theme::m().text_small),
            theme::TEXT_DIM,
        );
    }
    let x_end = v.x(ruler, end);
    if x_end >= ruler.left() {
        painter.vline(x_end, ruler.y_range(), stroke(2.0, theme::GOLD.gamma_multiply(0.8)));
    }
    if let Some((tick, playing)) = playhead {
        let x = v.x(ruler, tick);
        if x >= ruler.left() {
            paint_playhead(painter, x, ruler, grid.y_range(), playing);
        }
    }
}

/// The vertical view of a row editor (piano keys, playlist tracks), in rows:
/// `start` rows scrolled off the top, `visible` rows on screen, `total` rows.
#[derive(Clone, Copy, Debug)]
pub struct RowView {
    pub start: f32,
    pub visible: f32,
    pub total: f32,
}

/// A vertical-bar drag in progress.
#[derive(Clone, Copy, Debug)]
pub enum VDrag {
    /// Grabbed the thumb's middle, `grab` rows below its top.
    Pan { grab: f32 },
    /// Grabbed one end; the other end stays where it was.
    Edge { top: bool, fixed: f32 },
}

/// The overview bar's vertical twin, beside a row editor: drag the thumb to
/// scroll, drag an end to zoom the rows, click the track to jump there, wheel
/// over it to zoom (keeping the middle row still). Thinner than the overview.
pub struct VBar {
    pub rect: Rect,
}

impl VBar {
    /// How wide every row editor's bar is: exactly as wide as the overview
    /// bar is tall (consistency, predictability).
    pub fn width() -> f32 {
        theme::m().overview_h
    }

    fn span(&self, v: &RowView) -> f32 {
        v.total.max(v.start + v.visible).max(1.0)
    }

    fn at(&self, v: &RowView, y: f32) -> f32 {
        ((y - self.rect.top()) / self.rect.height() * self.span(v)).max(0.0)
    }

    pub fn thumb(&self, v: &RowView) -> Rect {
        let span = self.span(v);
        let t = self.rect.top() + v.start / span * self.rect.height();
        let b = self.rect.top() + (v.start + v.visible) / span * self.rect.height();
        Rect::from_min_max(
            pos2(self.rect.left() + 1.0, t),
            pos2(self.rect.right() - 1.0, b.max(t + 14.0).min(self.rect.bottom())),
        )
    }

    pub fn press(&self, v: &mut RowView, pos: Pos2) -> VDrag {
        let thumb = self.thumb(v);
        if pos.y > thumb.top() - 3.0 && pos.y < thumb.top() + HANDLE {
            VDrag::Edge { top: true, fixed: v.start + v.visible }
        } else if pos.y < thumb.bottom() + 3.0 && pos.y > thumb.bottom() - HANDLE {
            VDrag::Edge { top: false, fixed: v.start }
        } else if thumb.y_range().contains(pos.y) {
            VDrag::Pan { grab: self.at(v, pos.y) - v.start }
        } else {
            v.start = (self.at(v, pos.y) - v.visible * 0.5).clamp(0.0, (v.total - v.visible).max(0.0));
            VDrag::Pan { grab: v.visible * 0.5 }
        }
    }

    /// Follow a drag. Edge drags change `visible` (the caller turns that into
    /// a row height), between `min_rows` and `max_rows`.
    pub fn drag(&self, v: &mut RowView, drag: VDrag, pos: Pos2, min_rows: f32, max_rows: f32) {
        let at = self.at(v, pos.y);
        match drag {
            VDrag::Pan { grab } => v.start = (at - grab).clamp(0.0, (v.total - v.visible).max(0.0)),
            VDrag::Edge { top, fixed } => {
                let (a, b) = if top { (at.min(fixed - min_rows), fixed) } else { (fixed, at.max(fixed + min_rows)) };
                v.visible = (b - a).clamp(min_rows, max_rows);
                v.start = if top { (fixed - v.visible).max(0.0) } else { a.max(0.0) };
            }
        }
    }

    /// Wheel over the bar: zoom the rows around the middle of the view.
    pub fn wheel_zoom(&self, v: &mut RowView, wheel_y: f32, min_rows: f32, max_rows: f32) {
        if wheel_y != 0.0 {
            let middle = v.start + v.visible * 0.5;
            v.visible = (v.visible * (-wheel_y * 0.004).exp()).clamp(min_rows, max_rows);
            v.start = (middle - v.visible * 0.5).clamp(0.0, (v.total - v.visible).max(0.0));
        }
    }

    pub fn paint(&self, painter: &Painter, v: &RowView, hot: bool) {
        paint_track(painter, self.rect);
        let thumb = self.thumb(v);
        paint_thumb(painter, thumb, hot);
        for y in [thumb.top() + HANDLE * 0.5, thumb.bottom() - HANDLE * 0.5] {
            let x = thumb.x_range();
            let w = x.span() * 0.28;
            painter.hline(egui::Rangef::new(x.min + w, x.max - w), y, stroke(2.0, if hot { theme::INK } else { theme::INK_DIM }));
        }
    }
}

/// Width of a viewport bar's zoom handles (wide targets).
const HANDLE: f32 = 14.0;

/// A scrollbar's track: a sunken well with its outline.
fn paint_track(painter: &Painter, r: Rect) {
    theme::outlined(painter, r, theme::CORNER, theme::SUNKEN);
}

/// A scrollbar thumb (catalog-next, settled): a see-through window, the button
/// plate at 45 % with its dark outline, lighter while hot.
fn paint_thumb(painter: &Painter, thumb: Rect, hot: bool) {
    let a = if hot { 0.6 } else { 0.45 };
    // See-through: the fill, then the outline as a stroke (a filled outline
    // rect under it would make the window opaque).
    painter.rect_filled(thumb.shrink(theme::OUTLINE_W), cr(2), theme::PLATE.gamma_multiply(a));
    painter.rect_stroke(thumb, cr(3), stroke(theme::OUTLINE_W, theme::outline_of(theme::PLATE)), egui::StrokeKind::Inside);
}

/// Auto-scroll while a drag is held at or past an edge of the view: how many
/// points to scroll this frame along one axis (negative = towards `lo`).
/// Starts a few points inside the edge, so the edge itself works too, and
/// speeds up the further past it you hold the mouse. Time-based (`dt`), so it
/// feels the same at any frame rate.
pub fn edge_pan(pos: f32, lo: f32, hi: f32, dt: f32) -> f32 {
    const ZONE: f32 = 10.0;
    let over = if pos < lo + ZONE {
        pos - (lo + ZONE)
    } else if pos > hi - ZONE {
        pos - (hi - ZONE)
    } else {
        0.0
    };
    (over * 9.0).clamp(-1800.0, 1800.0) * dt.min(0.05)
}

/// The mouse as a time editor reads it, once per frame. Editors read the
/// pointer themselves (their canvases only sense hover, so they never win
/// egui's hit test over the docked panels' resize edges).
#[derive(Clone, Copy)]
pub struct Input {
    pub pos: Option<Pos2>,
    /// This frame's presses: left, right, middle.
    pub pressed: bool,
    pub pressed_right: bool,
    pub pressed_middle: bool,
    pub down: bool,
    pub mods: egui::Modifiers,
    pub wheel: egui::Vec2,
    pub time: f64,
    pub dt: f32,
    /// Over the editor's canvas, and not on a docked panel's resize edge.
    pub hovering: bool,
}

impl Input {
    pub fn read(ui: &egui::Ui, canvas: Rect) -> Self {
        let mut input = ui.input(|i| Self {
            pos: i.pointer.interact_pos(),
            pressed: i.pointer.primary_pressed(),
            pressed_right: i.pointer.secondary_pressed(),
            pressed_middle: i.pointer.button_pressed(egui::PointerButton::Middle),
            down: i.pointer.any_down(),
            mods: i.modifiers,
            wheel: i.raw_scroll_delta,
            time: i.time,
            dt: i.stable_dt,
            hovering: false,
        });
        input.hovering = ui.rect_contains_pointer(canvas) && !crate::app::on_panel_edge(ui.ctx());
        input
    }
}

/// What the wheel asks a time editor to do. The scheme lives here, once:
/// plain = up/down (a touchpad's sideways swipe: sideways), Ctrl = sideways,
/// Shift (or the wheel over the ruler) = zoom time around the mouse,
/// Alt = row height around the mouse. (Wheel over a scrollbar zooms that
/// axis: the editors check their bars first.)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wheel {
    None,
    /// Zoom time by `factor`, keeping screen-x `at` still.
    ZoomTime {
        factor: f32,
        at: f32,
    },
    /// Zoom rows by `factor`, keeping screen-y `at` still.
    ZoomRows {
        factor: f32,
        at: f32,
    },
    /// Move the view, in points: `x` > 0 = later, `y` > 0 = further down.
    Scroll {
        x: f32,
        y: f32,
    },
}

pub fn wheel(input: &Input, ruler: Rect) -> Wheel {
    let w = input.wheel;
    // egui turns Shift+wheel into sideways scroll, so with a modifier the
    // wheel's amount is whichever axis it arrived on.
    let amount = w.x + w.y;
    let over_ruler = input.pos.is_some_and(|p| ruler.contains(p));
    let at = input.pos.unwrap_or(ruler.left_top());
    if (input.mods.shift || over_ruler) && amount != 0.0 {
        Wheel::ZoomTime { factor: (amount * 0.004).exp(), at: at.x }
    } else if input.mods.command && amount != 0.0 {
        Wheel::Scroll { x: -amount, y: 0.0 }
    } else if input.mods.alt && w.y != 0.0 {
        Wheel::ZoomRows { factor: (w.y * 0.004).exp(), at: at.y }
    } else if w != egui::Vec2::ZERO {
        Wheel::Scroll { x: -w.x, y: -w.y * 0.5 }
    } else {
        Wheel::None
    }
}

/// Double-clicks decided on the second *press*. egui only reports a
/// double-click on the release, a frame or more after the press that starts
/// an editor's gestures, so asking egui worked only when a click fit in one
/// frame. A third press right after doesn't count again.
#[derive(Default)]
pub struct Clicks {
    last: Option<(f64, Pos2)>,
}

impl Clicks {
    /// Call every frame; true on the press that makes a double-click.
    pub fn double(&mut self, input: &Input) -> bool {
        if !input.pressed {
            return false;
        }
        let double =
            input.pos.is_some_and(|p| self.last.is_some_and(|(t, at)| input.time - t < 0.35 && at.distance(p) < 6.0));
        self.last = if double { None } else { input.pos.map(|p| (input.time, p)) };
        double
    }
}

/// Round `raw` to the nearest multiple of `step`.
pub fn snap(raw: f32, step: u32) -> f32 {
    (raw / step as f32).round() * step as f32
}

/// Where a dragged value (a start, a length) goes: moved by the mouse's
/// travel since the press, snapped to the grid. None while the mouse hasn't
/// really moved: a click must never pull an off-grid item onto the grid.
pub fn drag_to_grid(value: f32, anchor_tick: f32, tick: f32, ppt: f32, step: u32) -> Option<f32> {
    ((tick - anchor_tick).abs() * ppt >= 3.0).then(|| snap(value + tick - anchor_tick, step))
}

/// [`edge_pan`] on both axes of `area` (`vertical` false: sideways only).
pub fn edge_pan_2d(pos: Pos2, area: Rect, dt: f32, vertical: bool) -> egui::Vec2 {
    let dx = edge_pan(pos.x, area.left(), area.right(), dt);
    let dy = if vertical { edge_pan(pos.y, area.top(), area.bottom(), dt) } else { 0.0 };
    egui::vec2(dx, dy)
}

/// The box of a Ctrl+drag selection.
pub fn paint_rubber_band(painter: &Painter, from: Pos2, to: Pos2) {
    let r = Rect::from_two_pos(from, to);
    painter.rect_filled(r, cr(0), theme::MINT.gamma_multiply(0.07));
    painter.rect_stroke(r, cr(0), stroke(1.0, theme::MINT.gamma_multiply(0.7)), egui::StrokeKind::Inside);
}

/// The playhead: a marker in the ruler and a line down `lines` (dim while stopped).
pub fn paint_playhead(painter: &Painter, x: f32, ruler: Rect, lines: egui::Rangef, playing: bool) {
    let tri = vec![pos2(x - 5.0, ruler.top() + 2.0), pos2(x + 5.0, ruler.top() + 2.0), pos2(x, ruler.bottom() - 2.0)];
    painter.add(egui::Shape::convex_polygon(tri, theme::MINT, egui::Stroke::NONE));
    let line = if playing { theme::MINT } else { theme::MINT.gamma_multiply(0.35) };
    painter.vline(x, lines, stroke(1.0, line));
}

// ── Loop regions ──
//
// Drawn on purpose only: Alt+drag along a ruler, snapped to the grid. Alt+click
// on the ruler (or the context bar's "Clear loop") takes it away. A plain click
// on the ruler always means "play from here". While one is set, playback stays
// inside it. Not saved with the song.

/// The loop drawn from `anchor` to `at` (ticks), both ends on the grid, at
/// least one grid step long.
pub fn loop_span(anchor: f32, at: f32, step: u32) -> (u32, u32) {
    let step = step.max(1);
    let a = snap(anchor.min(at), step).max(0.0) as u32;
    let b = snap(anchor.max(at), step).max(0.0) as u32;
    (a, b.max(a + step))
}

/// A loop region: a band on the ruler, its edges down the grid.
pub fn paint_loop(painter: &Painter, v: &TimeView, ruler: Rect, grid: Rect, (a, b): (u32, u32)) {
    let (x0, x1) = (v.x(ruler, a as f32), v.x(ruler, b as f32));
    let band = Rect::from_min_max(pos2(x0, ruler.top() + 1.0), pos2(x1, ruler.bottom() - 1.0)).intersect(ruler);
    if band.width() > 0.0 {
        painter.rect_filled(band, cr(2), theme::LAVENDER.gamma_multiply(0.30));
    }
    let lines = grid.y_range();
    for x in [x0, x1] {
        if ruler.x_range().contains(x) {
            painter.vline(
                x,
                egui::Rangef::new(ruler.top(), lines.max),
                stroke(1.0, theme::LAVENDER.gamma_multiply(0.6)),
            );
        }
    }
    let inside = Rect::from_min_max(pos2(x0, grid.top()), pos2(x1, grid.bottom())).intersect(grid);
    if inside.width() > 0.0 {
        painter.rect_filled(inside, cr(0), theme::LAVENDER.gamma_multiply(0.04));
    }
}

/// A faint line where the mouse is, down the ruler and the grid, so you can
/// see what lines up before you click.
pub fn paint_cursor(painter: &Painter, x: f32, ruler: Rect, grid: Rect) {
    if grid.x_range().contains(x) {
        painter.vline(
            x,
            egui::Rangef::new(ruler.top(), grid.bottom()),
            stroke(1.0, egui::Color32::from_white_alpha(28)),
        );
    }
}

#[cfg(test)]
mod loop_tests {
    #[test]
    fn a_loop_lands_on_the_grid_and_is_never_empty() {
        assert_eq!(super::loop_span(390.0, 1150.0, 384), (384, 1152));
        assert_eq!(super::loop_span(1150.0, 390.0, 384), (384, 1152), "either direction");
        assert_eq!(super::loop_span(400.0, 410.0, 384), (384, 768), "at least a step");
    }
}
