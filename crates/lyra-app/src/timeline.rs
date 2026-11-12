//! Pieces shared by every horizontal time editor (playlist, piano roll):
//! the overview bar (drag the thumb to pan, drag its ends to zoom) and the
//! bar ruler.

use crate::theme::{self, cr, stroke};
use egui::{pos2, Align2, FontId, Painter, Pos2, Rect, StrokeKind};
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

    pub fn paint(&self, painter: &Painter, v: &TimeView, hot: bool) {
        painter.rect_filled(self.rect, cr(3), theme::INSET);
        let thumb = self.thumb(v);
        let fill = if hot { theme::mix(theme::RAISED, theme::MINT, 0.15) } else { theme::RAISED };
        painter.rect_filled(thumb, cr(4), fill);
        painter.rect_stroke(thumb, cr(4), stroke(1.0, theme::LINE), StrokeKind::Inside);
        // Grips on both ends: "drag me to zoom".
        for x in [thumb.left() + 5.0, thumb.right() - 5.0] {
            for dx in [-1.5, 1.5] {
                painter.vline(x + dx, thumb.shrink(5.0).y_range(), stroke(1.0, theme::MINT.gamma_multiply(0.6)));
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
