//! The plugin rack: one row per instrument (mute light, pan, volume, name,
//! mixer insert). It lives in the right panel; the same rows *with* a step
//! grid are the "Step sequencer" editor, which any area can show.
//!
//! Mouse grammar ("it knows what you mean"):
//! * **click / drag** across steps — paint. The first step you touch decides
//!   whether you're painting *on* or *off*, so one gesture never flickers.
//!   Dragging works across rows too.
//! * **right-drag** — erase.
//! * **wheel** on a lit step — velocity. **Shift+wheel** — pitch.
//! * **lights** — click to mute/unmute, **drag across** several to set them all
//!   the same way · **right-click** — solo (mutes the others; click any dark
//!   light to bring it back; right-click the soloed one again to unmute all)
//! * **click a name** — select (the editor and piano roll follow) ·
//!   **double-click** — open the instrument editor ·
//!   **right-click** — piano roll, fill every 2/4/8 steps, clear, delete.
//! * a channel with real melodies shows a mini piano roll instead of steps —
//!   click it to open the piano roll.

use crate::app::{Focus, UiState};
use crate::areas::Editor;
use crate::theme::{self, cr, stroke};
use crate::widgets;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Response, Sense, StrokeKind, Ui};
use lyra_model::{History, Project, DEFAULT_KEY, TICKS_PER_STEP};
use lyra_plugin::{Curve, ParamInfo};

const STEP_GAP: f32 = 2.0;
const BEAT_GAP: f32 = 6.0;

pub static VOLUME: ParamInfo = ParamInfo::new(0, "Volume", 0.0, 1.25, 0.8, "%", Curve::Linear);
pub static PAN: ParamInfo = ParamInfo::new(1, "Pan", -1.0, 1.0, 0.0, "pan", Curve::Linear);

fn step_x(s: u32) -> f32 {
    s as f32 * (theme::m().step_w + STEP_GAP) + (s / 4) as f32 * (BEAT_GAP - STEP_GAP)
}

fn step_at(x: f32, len: u32) -> Option<u32> {
    // Each step's hit area includes the small gap after it, so a drag never "falls through".
    let w = theme::m().step_w;
    (0..len).find(|&s| x >= step_x(s) && x < step_x(s) + w + STEP_GAP)
}

/// Undo bookkeeping for a continuous control: one checkpoint per gesture.
/// Returns true when the value changed this frame.
pub fn tracked(r: &Response, h: &mut History, p: &Project) -> bool {
    if r.drag_started() || (r.changed() && !r.dragged()) {
        h.checkpoint(p);
    }
    r.changed()
}

/// Every instrument, once (presets live in each instrument's editor).
/// Returns (name, instrument id, params) when one is picked.
fn instrument_menu(ui: &mut Ui) -> Option<(&'static str, &'static str, Vec<f32>)> {
    let mut picked = None;
    for (title, melodic) in [("DRUMS", false), ("INSTRUMENTS", true)] {
        ui.label(egui::RichText::new(title).small().color(theme::TEXT_DIM));
        for inst in lyra_dsp::INSTRUMENTS.iter().filter(|i| i.melodic == melodic) {
            if ui.button(inst.name).clicked() {
                picked = Some((inst.name, inst.id, lyra_dsp::default_params(inst.id)));
            }
        }
        if !melodic {
            ui.separator();
        }
    }
    picked
}

fn add_channel_menu(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    if let Some((name, instrument, params)) = instrument_menu(ui) {
        h.checkpoint(p);
        p.add_channel(name, instrument, params);
        st.selected = p.channels.len() - 1;
        if lyra_dsp::info(instrument).is_some_and(|i| i.melodic) {
            st.show = Some(Editor::PianoRoll);
        }
        ui.close_menu();
    }
}

/// FL-style solo: mute everyone else. Soloing the channel that is already the
/// only one playing brings everybody back.
fn solo(p: &mut Project, ci: usize) {
    let alone = p.channels.iter().enumerate().all(|(i, c)| (i == ci) != c.mute);
    for (i, c) in p.channels.iter_mut().enumerate() {
        c.solo = false; // the old flag; mute flags are the whole truth now
        c.mute = !alone && i != ci;
    }
}

/// The plugin rack (right panel): the rows, without steps.
pub fn plugins(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState) {
    show(ui, p, h, st, None, false);
}

/// `steps`: also show each row's step grid (the Step sequencer editor).
pub fn show(ui: &mut Ui, p: &mut Project, h: &mut History, st: &mut UiState, play_step: Option<u32>, steps: bool) {
    let pat = st.pattern.min(p.patterns.len() - 1);

    // ── Header ──
    ui.horizontal(|ui| {
        if !steps {
            ui.menu_button("+ Instrument", |ui| add_channel_menu(ui, p, h, st));
            return;
        }
        let len = p.patterns[pat].min_length;
        let mut new_len = len;
        let bars = |n: u32| format!("{} bar{}", n / 16, if n > 16 { "s" } else { "" });
        egui::ComboBox::from_id_salt("pattern-length")
            .width(64.0)
            .selected_text(bars(len))
            .show_ui(ui, |ui| {
                for n in [16, 32, 48, 64] {
                    ui.selectable_value(&mut new_len, n, bars(n));
                }
            })
            .response
            .on_hover_text("Shortest length. The pattern grows by itself to fit its notes.");
        if new_len != len {
            h.checkpoint(p);
            p.patterns[pat].min_length = new_len;
        }
        ui.menu_button("+ Instrument", |ui| add_channel_menu(ui, p, h, st));
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
    let m = theme::m();
    let row_w = m.led
        + m.knob_small * 2.0
        + m.name_w
        + m.step_h
        + 8.0
        + 5.0 * ui.spacing().item_spacing.x
        + step_x(p.patterns[pat].play_length() - 1)
        + m.step_w;
    let input = (pointer, primary, secondary, wheel, shift);
    if steps {
        egui::ScrollArea::horizontal().min_scrolled_width(row_w.min(1000.0)).show(ui, |ui| {
            for ci in 0..p.channels.len() {
                row(ui, p, h, st, ci, pat, play_step, any_solo, input, true, &mut delete);
            }
        });
    } else {
        egui::ScrollArea::vertical().id_salt("plugin-rack").auto_shrink([false, true]).show(ui, |ui| {
            for ci in 0..p.channels.len() {
                row(ui, p, h, st, ci, pat, None, any_solo, input, false, &mut delete);
            }
        });
    }

    if let Some(ci) = delete {
        h.checkpoint(p);
        p.remove_channel(ci);
        st.selected = st.selected.min(p.channels.len().saturating_sub(1));
    }

    if steps {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "drag: paint · right-drag: erase · wheel: velocity · shift+wheel: pitch · double-click name: edit",
            )
            .small()
            .color(theme::TEXT_DIM),
        );
    }
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
    steps: bool,
    delete: &mut Option<usize>,
) {
    let id = p.channels[ci].id;
    let light = theme::channel_light(p.channels[ci].color);
    let color = light.mid;
    let m = theme::m();
    ui.horizontal(|ui| {
        // Mute lights. Press one to flip it; keep the button down and sweep over
        // others to set them the same way (one gesture, one undo step).
        let mute = p.channels[ci].mute;
        let audible = !mute && (!any_solo || p.channels[ci].solo);
        let r = widgets::led(ui, audible, light, "Click / drag: mute · Right-click: solo");
        match widgets::sweep(ui, r.rect, "rack-mute", audible) {
            Some(widgets::Sweep::Begin(on)) => {
                h.checkpoint(p);
                p.channels[ci].mute = !on;
                p.channels[ci].solo = false;
            }
            Some(widgets::Sweep::Continue(on)) => {
                p.channels[ci].mute = !on;
                p.channels[ci].solo = false;
            }
            None => {}
        }
        if r.secondary_clicked() {
            h.checkpoint(p);
            solo(p, ci);
        }

        // Pan & volume
        let mut pan = p.channels[ci].pan;
        if tracked(&widgets::knob(ui, &mut pan, &PAN, m.knob_small, color), h, p) {
            p.channels[ci].pan = pan;
        }
        let mut vol = p.channels[ci].volume;
        if tracked(&widgets::knob(ui, &mut vol, &VOLUME, m.knob_small, color), h, p) {
            p.channels[ci].volume = vol;
        }

        // Name button (flashes on every hit)
        let (rect, r) = ui.allocate_exact_size(vec2(m.name_w, m.step_h), Sense::click());
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
            FontId::proportional(m.text),
            if selected { theme::TEXT } else { theme::TEXT_DIM },
        );
        if r.clicked() {
            st.selected = ci;
        }
        if r.double_clicked() {
            st.selected = ci;
            st.show_editor = true;
            st.raise = Some(Focus::Editor);
        }
        r.context_menu(|ui| {
            if ui.button("Piano roll").clicked() {
                st.selected = ci;
                st.show = Some(Editor::PianoRoll);
                ui.close_menu();
            }
            if ui.button("Edit instrument").clicked() {
                st.selected = ci;
                st.show_editor = true;
                st.raise = Some(Focus::Editor);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Clone channel").on_hover_text("Same instrument and settings, no notes").clicked() {
                h.checkpoint(p);
                let mut copy = p.channels[ci].clone();
                copy.name = format!("{} 2", copy.name);
                let id = p.add_channel(&copy.name, &copy.instrument, copy.params.clone());
                let new = p.channels.pop().expect("just added");
                copy.id = id;
                copy.color = new.color;
                p.channels.insert(ci + 1, copy);
                st.selected = ci + 1;
                ui.close_menu();
            }
            ui.menu_button("Replace instrument", |ui| {
                if let Some((name, instrument, params)) = instrument_menu(ui) {
                    h.checkpoint(p);
                    let ch = &mut p.channels[ci];
                    let named_after_instrument = lyra_dsp::info(&ch.instrument).map(|i| i.name)
                        == Some(ch.name.as_str())
                        || lyra_dsp::PRESETS.iter().any(|pr| pr.name == ch.name);
                    if named_after_instrument {
                        ch.name = name.to_string(); // only rename channels still named after their instrument
                    }
                    ch.instrument = instrument.to_string();
                    ch.params = params;
                    ui.close_menu();
                }
            });
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

        // Which mixer insert this instrument plays into (M = master).
        let mut insert = p.channels[ci].mixer;
        let inserts = p.mixer.len().saturating_sub(1);
        let r = widgets::number_well(ui, &mut insert, 0..=inserts, ci + 1, vec2(m.step_h, m.step_h), |n| {
            if n == 0 {
                "M".into()
            } else {
                n.to_string()
            }
        });
        if tracked(&r, h, p) {
            p.channels[ci].mixer = insert;
        }
        r.on_hover_text("Mixer insert: drag or scroll to route (M = master)");
        if !steps {
            return;
        }

        ui.add_space(4.0);

        let len = p.patterns[pat].play_length();
        let (strip, strip_resp) =
            ui.allocate_exact_size(vec2(step_x(len - 1) + m.step_w, m.step_h), Sense::click_and_drag());

        // A melodic channel shows a mini piano roll; click it to open the real one.
        if !p.patterns[pat].is_step_channel(id) {
            preview(&painter, strip, p, pat, ci, color, play_step);
            if strip_resp.clicked() {
                st.selected = ci;
                st.show = Some(Editor::PianoRoll);
            }
            strip_resp.on_hover_text("Melody: click to open the piano roll");
            return;
        }

        let hover =
            pointer.filter(|_| ui.rect_contains_pointer(strip)).and_then(|pos| step_at(pos.x - strip.min.x, len));
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
            let r = Rect::from_min_size(pos2(strip.min.x + step_x(s), strip.min.y), vec2(m.step_w, m.step_h));
            let note = p.patterns[pat].step(id, s);
            let playing = play_step == Some(s);
            // Every other beat sits a shade lighter, so the bar reads in fours.
            let tone = if (s / 4) % 2 == 0 { 1.0 } else { 1.13 } * if playing { 1.25 } else { 1.0 };
            // Softer notes glow dimmer.
            let lit = note.map(|n| light.scaled(0.72 + 0.28 * n.velocity));
            theme::flat_key(&painter, r, lit, playing || hover == Some(s), tone);
            if let Some(n) = note {
                if n.key != DEFAULT_KEY {
                    painter.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        format!("{:+}", n.key as i32 - DEFAULT_KEY as i32),
                        FontId::monospace(9.0),
                        theme::key_ink(true),
                    );
                }
            }
        }
    });
}

/// Mini piano roll of a channel's notes, squeezed into one rack row.
fn preview(
    painter: &egui::Painter,
    r: Rect,
    p: &Project,
    pat: usize,
    ci: usize,
    color: Color32,
    play_step: Option<u32>,
) {
    painter.rect_filled(r, cr(3), theme::INSET);
    painter.rect_stroke(r, cr(3), stroke(1.0, theme::LINE), StrokeKind::Inside);
    let notes = p.patterns[pat].notes(p.channels[ci].id);
    let len_ticks = (p.patterns[pat].play_length() * TICKS_PER_STEP) as f32;
    let (lo, hi) = notes.iter().fold((127u8, 0u8), |(lo, hi), n| (lo.min(n.key), hi.max(n.key)));
    let span = (hi.saturating_sub(lo) as f32).max(12.0);
    let inner = r.shrink2(vec2(3.0, 3.0));
    // Beat dividers so the preview lines up with step rows above and below.
    for s in (4..p.patterns[pat].play_length()).step_by(4) {
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
