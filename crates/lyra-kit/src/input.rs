//! Small input behaviours every editor shares, so they feel the same everywhere.

use egui::{Rect, Ui};

/// -1 / +1 when the wheel turns over `rect` this frame (wheel down = next).
pub fn wheel_step(ui: &Ui, rect: Rect) -> Option<i32> {
    if !ui.rect_contains_pointer(rect) {
        return None;
    }
    let d = ui.input(|i| i.raw_scroll_delta);
    let dy = if d.y != 0.0 { d.y } else { d.x };
    (dy != 0.0).then_some(if dy > 0.0 { -1 } else { 1 })
}

/// A second click on the same `item` within 0.35 s (egui's own double-click
/// needs one widget; this works across rows that redraw every frame).
pub fn double_click<T: Clone + PartialEq + Send + Sync + 'static>(ui: &Ui, key: &str, item: T) -> bool {
    let now = ui.input(|i| i.time);
    let key = egui::Id::new(key);
    let last: Option<(T, f64)> = ui.data(|d| d.get_temp(key));
    if last.is_some_and(|(n, t)| n == item && now - t < 0.35) {
        ui.data_mut(|d| d.remove::<(T, f64)>(key));
        true
    } else {
        ui.data_mut(|d| d.insert_temp(key, (item, now)));
        false
    }
}
