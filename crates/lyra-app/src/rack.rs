//! The channel rack: one row per instrument, one button per step.
//!
//! Mouse grammar ("it knows what you mean"):
//! * **click / drag** across steps — paint. The first step you touch decides
//!   whether you're painting *on* or *off*, so one gesture never flickers.
//!   Dragging works across rows too.
//! * **right-drag** — erase.
//! * **wheel** on a lit step — velocity. **Shift+wheel** — pitch.
//! * **click a name** — select (the editor and piano roll follow) ·
//!   **double-click** — open the instrument editor ·
//!   **right-click** — piano roll, fill every 2/4/8 steps, clear, delete.
//! * a channel with real melodies shows a mini piano roll instead of steps —
//!   click it to open the piano roll.

use crate::app::UiState;
use crate::theme::{self, cr, stroke};
use crate::widgets;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Response, Sense, StrokeKind, Ui};
use lyra_model::{History, Project, DEFAULT_KEY, TICKS_PER_STEP};
use lyra_plugin::{Curve, ParamInfo};

const STEP_W: f32 = 18.0;
const STEP_H: f32 = 22.0;
const STEP_GAP: f32 = 2.0;
const BEAT_GAP: f32 = 6.0;
const NAME_W: f32 = 108.0;

pub static VOLUME: ParamInfo = ParamInfo::new(0, "Volume", 0.0, 1.25, 0.8, "%", Curve::Linear);
pub static PAN: ParamInfo = ParamInfo::new(1, "Pan", -1.0, 1.0, 0.0, "pan", Curve::Linear);

fn step_x(s: u32) -> f32 {
    s as f32 * (STEP_W + STEP_GAP) + (s / 4) as f32 * (BEAT_GAP - STEP_GAP)
}

fn step_at(x: f32, len: u32) -> Option<u32> {
    // Each step's hit area includes the small gap after it, so a drag never "falls through".
    (0..len).find(|&s| x >= step_x(s) && x < step_x(s) + STEP_W + STEP_GAP)
}

/// Undo bookkeeping for a continuous control: one checkpoint per gesture.
/// Returns true when the value changed this frame.
pub fn tracked(r: &Response, h: &mut History, p: &Project) -> bool {
    if r.drag_started() || (r.changed() && !r.dragged()) {
        h.checkpoint(p);
    }
    r.changed()
}

/// "+ Channel" menu contents: drums, then synth presets. Shared with the editor.
fn add_channel_menu(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    ui.label(egui::RichText::new("DRUMS").small().color(theme::TEXT_DIM));
    for inst in lyra_dsp::INSTRUMENTS.iter().filter(|i| !i.melodic) {
        if ui.button(inst.name).clicked() {
            h.checkpoint(p);
            p.add_channel(inst.name, inst.id, lyra_dsp::default_params(inst.id));
            st.selected = p.channels.len() - 1;
            ui.close_menu();
        }
    }
    ui.separator();
    ui.label(egui::RichText::new("SYNTH").small().color(theme::TEXT_DIM));
    for preset in lyra_dsp::PRESETS {
        if ui.button(preset.name).clicked() {
            h.checkpoint(p);
            p.add_channel(preset.name, preset.instrument, preset.params());
            st.selected = p.channels.len() - 1;
            st.show_piano = true;
            ui.close_menu();
        }
    }
}

pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState, play_step: Option<u32>) {
    let pat = st.pattern.min(p.patterns.len() - 1);

    // ── Header ──
    ui.horizontal(|ui| {
        let len = p.patterns[pat].length;
        let mut new_len = len;
        let bars = |n: u32| format!("{} bar{}", n / 16, if n > 16 { "s" } else { "" });
        egui::ComboBox::from_id_salt("pattern-length").width(64.0).selected_text(bars(len)).show_ui(ui, |ui| {
            for n in [16, 32, 48, 64] {
                ui.selectable_value(&mut new_len, n, bars(n));
            }
        });
        if new_len != len {
            h.checkpoint(p);
            p.patterns[pat].length = new_len;
        }
        ui.menu_button("+ Channel", |ui| add_channel_menu(ui, p, h, st));
    });
    ui.add_space(4.0);

    let (pointer, primary, secondary, any_down, wheel, shift) = ui.input(|i| {
        let d = i.raw_scroll_delta;
        (
            i.pointer.interact_pos(),
            i.pointer.primary_pressed(),
            i.pointer.secondary_pressed(),
            i.pointer.any_down(),
            if d.y != 0.0 { d.y } else { d.x }, // Shift+wheel arrives as horizontal scroll
            i.modifiers.shift,
        )
    });
    if !any_down {
        st.paint = None;
    }
    let any_solo = p.any_solo();
    let mut delete = None;

    // Long patterns scroll sideways instead of growing the window forever.
    // Row = light, two knobs, name, gap, steps (+ item spacing). Up to ~1000px, then scroll.
    let row_w = 12.0 + 18.0 * 2.0 + NAME_W + 8.0 + 5.0 * ui.spacing().item_spacing.x
        + step_x(p.patterns[pat].length - 1)
        + STEP_W;
    egui::ScrollArea::horizontal().min_scrolled_width(row_w.min(1000.0)).max_width(1000.0).show(ui, |ui| {
        for ci in 0..p.channels.len() {
            row(ui, p, h, st, ci, pat, play_step, any_solo, (pointer, primary, secondary, wheel, shift), &mut delete);
        }
    });

    if let Some(ci) = delete {
        h.checkpoint(p);
        p.remove_channel(ci);
        st.selected = st.selected.min(p.channels.len().saturating_sub(1));
    }

    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("drag: paint · right-drag: erase · wheel: velocity · shift+wheel: pitch · double-click name: edit")
            .small()
            .color(theme::TEXT_DIM),
    );
}

type Input = (Option<egui::Pos2>, bool, bool, f32, bool);

#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut Ui,
    p: &mut Project,
    h: &mut History,
    st: &mut UiState,
    ci: usize,
    pat: usize,
    play_step: Option<u32>,
    any_solo: bool,
    (pointer, primary, secondary, wheel, shift): Input,
    delete: &mut Option<usize>,
) {
    let id = p.channels[ci].id;
    let color = theme::channel_color(p.channels[ci].color);
    ui.horizontal(|ui| {
        // Mute / solo
        let (mute, solo) = (p.channels[ci].mute, p.channels[ci].solo);
        let audible = !mute && (!any_solo || solo);
        let r = widgets::led(ui, audible, if solo { theme::GOLD } else { color }, "Click: mute · Right-click: solo");
        if r.clicked() {
            h.checkpoint(p);
            p.channels[ci].mute = !mute;
        }
        if r.secondary_clicked() {
            h.checkpoint(p);
            for c in &mut p.channels {
                c.solo = false;
            }
            p.channels[ci].solo = !solo;
        }

        // Pan & volume
        let mut pan = p.channels[ci].pan;
        if tracked(&widgets::knob(ui, &mut pan, &PAN, 18.0, color), h, p) {
            p.channels[ci].pan = pan;
        }
        let mut vol = p.channels[ci].volume;
        if tracked(&widgets::knob(ui, &mut vol, &VOLUME, 18.0, color), h, p) {
            p.channels[ci].volume = vol;
        }

        // Name button (flashes on every hit)
        let (rect, r) = ui.allocate_exact_size(vec2(NAME_W, STEP_H), Sense::click());
        let selected = st.selected == ci;
        let flash = st.flash.get(ci).copied().unwrap_or(0.0);
        let painter = ui.painter().clone();
        let bg = if selected {
            theme::mix(theme::RAISED, color, 0.16)
        } else if r.hovered() {
            theme::mix(theme::PANEL_HI, color, 0.07)
        } else {
            theme::PANEL_HI
        };
        painter.rect_filled(rect, cr(3), bg);
        if flash > 0.01 {
            painter.rect_filled(rect, cr(3), color.gamma_multiply(0.12 * flash));
        }
        let tab = Rect::from_min_size(rect.min, vec2(3.0, rect.height()));
        painter.rect_filled(tab, cr(1), theme::mix(theme::mix(theme::PANEL_HI, color, 0.5), color, flash));
        painter.rect_stroke(
            rect,
            cr(3),
            if selected { stroke(1.0, color.gamma_multiply(0.6)) } else { stroke(1.0, theme::LINE) },
            StrokeKind::Inside,
        );
        painter.text(
            rect.left_center() + vec2(9.0, 0.0),
            Align2::LEFT_CENTER,
            &p.channels[ci].name,
            FontId::proportional(11.5),
            if selected { theme::TEXT } else { theme::TEXT_DIM },
        );
        if r.clicked() {
            st.selected = ci;
        }
        if r.double_clicked() {
            st.selected = ci;
            st.show_editor = true;
        }
        r.context_menu(|ui| {
            if ui.button("Piano roll").clicked() {
                st.selected = ci;
                st.show_piano = true;
                ui.close_menu();
            }
            if ui.button("Edit instrument").clicked() {
                st.selected = ci;
                st.show_editor = true;
                ui.close_menu();
            }
            ui.separator();
            for every in [2, 4, 8] {
                if ui.button(format!("Fill every {every} steps")).clicked() {
                    h.checkpoint(p);
                    p.patterns[pat].fill_every(id, every);
                    ui.close_menu();
                }
            }
            if ui.button("Clear notes").clicked() {
                h.checkpoint(p);
                p.patterns[pat].notes.remove(&id);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Delete channel").clicked() {
                *delete = Some(ci);
                ui.close_menu();
            }
        });

        ui.add_space(4.0);

        let len = p.patterns[pat].length;
        let (strip, strip_resp) = ui.allocate_exact_size(vec2(step_x(len - 1) + STEP_W, STEP_H), Sense::click_and_drag());

        // A melodic channel shows a mini piano roll; click it to open the real one.
        if !p.patterns[pat].is_step_channel(id) {
            preview(&painter, strip, p, pat, ci, color, play_step);
            if strip_resp.clicked() {
                st.selected = ci;
                st.show_piano = true;
            }
            strip_resp.on_hover_text("Melody: click to open the piano roll");
            return;
        }

        let hover = pointer.filter(|_| ui.rect_contains_pointer(strip)).and_then(|pos| step_at(pos.x - strip.min.x, len));
        if let Some(s) = hover {
            let lit = p.patterns[pat].step(id, s).is_some();
            if primary {
                h.checkpoint(p);
                st.paint = Some(!lit);
                st.selected = ci;
            } else if secondary {
                h.checkpoint(p);
                st.paint = Some(false);
            }
            if let Some(on) = st.paint {
                p.patterns[pat].set_step(id, s, on, 0.8);
            }
            if wheel != 0.0 && lit {
                h.checkpoint(p);
                if let Some(n) = p.patterns[pat].step_mut(id, s) {
                    if shift {
                        n.key = (n.key as i32 + wheel.signum() as i32).clamp(0, 127) as u8;
                    } else {
                        n.velocity = (n.velocity + wheel.signum() * 0.05).clamp(0.05, 1.0);
                    }
                }
            }
        }

        for s in 0..len {
            let r = Rect::from_min_size(pos2(strip.min.x + step_x(s), strip.min.y), vec2(STEP_W, STEP_H));
            let off = if (s / 4) % 2 == 0 { theme::STEP_OFF_A } else { theme::STEP_OFF_B };
            let note = p.patterns[pat].step(id, s);
            let fill = match note {
                Some(n) => theme::mix(theme::PANEL_HI, color, 0.3 + 0.7 * n.velocity),
                None if hover == Some(s) => theme::mix(off, color, 0.18),
                None => off,
            };
            painter.rect_filled(r, cr(2), fill);
            painter.rect_stroke(r, cr(2), stroke(1.0, theme::LINE), StrokeKind::Inside);
            if let Some(n) = note {
                if n.key != DEFAULT_KEY {
                    painter.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        format!("{:+}", n.key as i32 - DEFAULT_KEY as i32),
                        FontId::proportional(8.5),
                        theme::INSET,
                    );
                }
            }
            if play_step == Some(s) {
                painter.rect_stroke(r.expand(1.0), cr(3), stroke(1.0, theme::MINT), StrokeKind::Outside);
                if note.is_some() {
                    painter.rect_filled(r, cr(2), Color32::WHITE.gamma_multiply(0.12));
                }
            }
        }
    });
}

/// Mini piano roll of a channel's notes, squeezed into one rack row.
fn preview(painter: &egui::Painter, r: Rect, p: &Project, pat: usize, ci: usize, color: Color32, play_step: Option<u32>) {
    painter.rect_filled(r, cr(3), theme::INSET);
    painter.rect_stroke(r, cr(3), stroke(1.0, theme::LINE), StrokeKind::Inside);
    let notes = p.patterns[pat].notes(p.channels[ci].id);
    let len_ticks = (p.patterns[pat].length * TICKS_PER_STEP) as f32;
    let (lo, hi) = notes.iter().fold((127u8, 0u8), |(lo, hi), n| (lo.min(n.key), hi.max(n.key)));
    let span = (hi.saturating_sub(lo) as f32).max(12.0);
    let inner = r.shrink2(vec2(3.0, 3.0));
    // Beat dividers so the preview lines up with step rows above and below.
    for s in (4..p.patterns[pat].length).step_by(4) {
        let x = r.min.x + step_x(s) - (BEAT_GAP - STEP_GAP) * 0.5;
        painter.vline(x, inner.y_range(), stroke(1.0, theme::LINE));
    }
    for n in notes {
        let x0 = inner.left() + n.start as f32 / len_ticks * inner.width();
        let x1 = inner.left() + (n.start + n.len) as f32 / len_ticks * inner.width();
        let y = inner.bottom() - (n.key - lo) as f32 / span * (inner.height() - 2.0);
        painter.rect_filled(Rect::from_min_max(pos2(x0, y - 2.0), pos2(x1.max(x0 + 2.0), y)), cr(1), color);
    }
    if let Some(s) = play_step {
        let x = inner.left() + (s * TICKS_PER_STEP) as f32 / len_ticks * inner.width();
        painter.vline(x, r.y_range(), stroke(1.0, theme::MINT));
    }
}
