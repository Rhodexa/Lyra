//! How a knob turns: Lyra's knob behaviour, with the painting left to the
//! plugin. Drag up/down (Shift = fine), wheel nudges (one notch = one step on
//! a choice), double-click resets to the default.

use egui::{vec2, Painter, Rect, Response, Sense, Ui};
use lyra_plugin::ParamInfo;

/// A knob `diameter` across that turns like Lyra's and is drawn by
/// `paint(painter, rect, value, hot)`. No tooltip: the plugin shows the value
/// its own way.
pub fn knob_painted(
    ui: &mut Ui,
    value: &mut f32,
    info: &ParamInfo,
    diameter: f32,
    paint: impl FnOnce(&Painter, Rect, f32, bool),
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(diameter, diameter), Sense::click_and_drag());
    let (resp, hot) = knob_turn(ui, resp, value, info);
    paint(ui.painter(), rect, *value, hot);
    resp
}

/// What every knob does with the mouse, given its response; returns the
/// response (marked changed when the value moved) and whether it is "hot"
/// (hovered or dragged).
pub fn knob_turn(ui: &mut Ui, mut resp: Response, value: &mut f32, info: &ParamInfo) -> (Response, bool) {
    let n0 = info.to_normalized(*value);
    let mut n = n0;
    let before = *value;

    // During a drag the knob keeps its own raw, continuous position and only
    // the value it outputs is quantised (choice knobs snap to steps). Rounding
    // the position itself every frame threw away any movement smaller than
    // half a step per frame, so stepped knobs barely moved.
    let raw_id = resp.id.with("raw");
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let raw: f32 = if resp.drag_started() { n0 } else { ui.data(|d| d.get_temp(raw_id)).unwrap_or(n0) };
        n = (raw - resp.drag_delta().y * if fine { 0.0015 } else { 0.006 }).clamp(0.0, 1.0);
        ui.data_mut(|d| d.insert_temp(raw_id, n));
    }
    if resp.hovered() {
        let wheel = ui.input(|i| i.raw_scroll_delta.y);
        if wheel != 0.0 {
            n = if info.is_choice() {
                // One notch = one step.
                info.to_normalized((value.round() + wheel.signum()).clamp(info.min, info.max))
            } else {
                n + wheel.signum() * 0.02
            };
        }
    }
    // Only convert back when the knob actually moved: the round trip through the
    // knob position isn't exact on exponential params (250 Hz came back as 250.00002),
    // which counted as an edit (and an undo step) just from being drawn.
    if n != n0 {
        *value = info.from_normalized(n);
    }
    if resp.double_clicked() {
        *value = info.default;
    }
    if *value != before {
        resp.mark_changed();
    }
    let hot = resp.hovered() || resp.dragged();
    (resp, hot)
}
