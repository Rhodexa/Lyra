//! Pieces shared by the time editors (playlist, piano roll): the overview
//! bar (drag the thumb to pan, drag its ends to zoom, wheel over it to zoom),
//! its vertical twin for rows (`VBar`), and the bar ruler.

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
        if (pos.x - thumb.left()).abs() < 7.0 {
            OverviewDrag::Edge { left: true, fixed: v.scroll + visible }
        } else if (pos.x - thumb.right()).abs() < 7.0 {
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

    pub fn paint(&self, painter: &Painter, v: &TimeView, hot: bool) {
        theme::well(painter, self.rect);
        let thumb = self.thumb(v).shrink2(egui::vec2(0.0, 2.0));
        paint_thumb(painter, thumb, hot);
        // Grips on both ends: "drag me to zoom".
        for x in [thumb.left() + 6.0, thumb.right() - 6.0] {
            for dx in [-1.5, 1.5] {
                painter.vline(x + dx, thumb.shrink(6.0).y_range(), stroke(1.0, theme::TEXT_DIM));
            }
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
            let tri =
                vec![pos2(x - 5.0, ruler.top() + 2.0), pos2(x + 5.0, ruler.top() + 2.0), pos2(x, ruler.bottom() - 2.0)];
            painter.add(egui::Shape::convex_polygon(tri, theme::MINT, egui::Stroke::NONE));
            let line = if playing { theme::MINT } else { theme::MINT.gamma_multiply(0.35) };
            painter.vline(x, grid.y_range(), stroke(1.0, line));
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
/// over it to zoom (keeping the top row still). Thinner than the overview.
pub struct VBar {
    pub rect: Rect,
}

impl VBar {
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
        if (pos.y - thumb.top()).abs() < 6.0 {
            VDrag::Edge { top: true, fixed: v.start + v.visible }
        } else if (pos.y - thumb.bottom()).abs() < 6.0 {
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

    /// Wheel over the bar: zoom the rows, keeping the top row still.
    pub fn wheel_zoom(&self, v: &mut RowView, wheel_y: f32, min_rows: f32, max_rows: f32) {
        if wheel_y != 0.0 {
            v.visible = (v.visible * (-wheel_y * 0.004).exp()).clamp(min_rows, max_rows);
        }
    }

    pub fn paint(&self, painter: &Painter, v: &RowView, hot: bool) {
        theme::well(painter, self.rect);
        let thumb = self.thumb(v).shrink2(egui::vec2(2.0, 0.0));
        paint_thumb(painter, thumb, hot);
        // Grips on both ends: "drag me to zoom".
        for y in [thumb.top() + 6.0, thumb.bottom() - 6.0] {
            for dy in [-1.5, 1.5] {
                painter.hline(thumb.shrink(4.0).x_range(), y + dy, stroke(1.0, theme::TEXT_DIM));
            }
        }
    }
}

/// A scrollbar thumb: a flat plate in the well, with the keys' dark outline.
fn paint_thumb(painter: &Painter, thumb: Rect, hot: bool) {
    painter.rect_filled(thumb, cr(4), theme::KEY_RECESS);
    let fill = if hot { egui::Color32::from_rgb(0x4a, 0x4a, 0x4a) } else { egui::Color32::from_rgb(0x3a, 0x3a, 0x3a) };
    painter.rect_filled(thumb.shrink(1.0), cr(3), fill);
}
