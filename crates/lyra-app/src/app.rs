//! The app shell: owns the project, runs one UI frame at a time.
//!
//! Each frame, in order:
//! 1. free engine garbage, read meters/playhead
//! 2. read the keyboard
//! 3. draw the views (they edit `project` directly)
//! 4. apply actions (play, undo, live notes)
//! 5. `bridge.sync` pushes whatever changed to the engine

use crate::keys::{Action, Keyboard};
use crate::theme::{self, cr};
use crate::{audio, bridge::Bridge, demo, rack, transport};
use egui::{Margin, RichText};
use lyra_engine::{Command, EngineHandle};
use lyra_model::{History, Project, TICKS_PER_STEP};
use lyra_plugin::Event;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

/// View state that isn't part of the project (not saved, not undoable).
#[derive(Default)]
pub struct UiState {
    pub selected: usize,
    pub pattern: usize,
    /// While dragging across steps: Some(true) = painting on, Some(false) = erasing.
    pub paint: Option<bool>,
    /// Per-channel hit flash, 1.0 = just triggered, fades to 0.
    pub flash: Vec<f32>,
    last_triggers: Vec<u32>,
    pub master_meter: [f32; 2],
}

pub struct LyraApp {
    project: Project,
    history: History,
    bridge: Bridge,
    engine: Option<EngineHandle>,
    audio: Option<audio::Audio>,
    audio_error: Option<String>,
    /// Worst recent DSP load (0..1), held briefly so the readout is legible.
    dsp_load: f32,
    keyboard: Keyboard,
    dev: crate::devtools::DevTools,
    ui: UiState,
    textures: theme::Textures,
}

impl LyraApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let (audio, engine, audio_error) = match audio::start() {
            Ok((a, h)) => (Some(a), Some(h), None),
            Err(e) => (None, None, Some(format!("no audio: {e}"))),
        };
        Self {
            project: demo::project(),
            history: History::default(),
            bridge: Bridge::default(),
            engine,
            audio,
            audio_error,
            dsp_load: 0.0,
            keyboard: Keyboard::default(),
            dev: crate::devtools::DevTools::from_env(),
            ui: UiState::default(),
            textures: theme::Textures::new(&cc.egui_ctx),
        }
    }

    fn send(&mut self, cmd: Command) {
        if let Some(e) = &mut self.engine {
            e.send(cmd);
        }
    }

    /// Play a note on the selected channel right now.
    fn live(&mut self, event: Event) {
        if self.ui.selected < self.project.channels.len() {
            let channel = self.ui.selected;
            self.send(Command::Live { channel, event });
        }
    }

    fn read_engine(&mut self) {
        let n = self.project.channels.len();
        self.ui.flash.resize(n, 0.0);
        self.ui.last_triggers.resize(n, 0);
        for f in &mut self.ui.flash {
            *f *= 0.82;
        }
        let Some(e) = &mut self.engine else { return };
        e.collect_garbage();
        let shared = e.shared();
        for i in 0..n {
            let t = shared.trigger_count(i);
            if t != self.ui.last_triggers[i] {
                self.ui.last_triggers[i] = t;
                self.ui.flash[i] = 1.0;
            }
            let _ = shared.take_peak(i); // per-channel meters arrive with the mixer (M3)
        }
        let peak = shared.take_master_peak();
        for (shown, new) in self.ui.master_meter.iter_mut().zip(peak) {
            *shown = new.max(*shown * 0.86);
        }
        if let Some(a) = &self.audio {
            self.dsp_load = a.stats.take_max_load().max(self.dsp_load * 0.97);
        }
    }
}

impl eframe::App for LyraApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.read_engine();
        let (playing, playhead) =
            self.engine.as_ref().map_or((false, 0.0), |e| (e.shared().is_playing(), e.shared().playhead()));
        let mut actions = self.keyboard.poll(ctx);
        self.dev.frame(ctx);
        if std::mem::take(&mut self.dev.autoplay) && !playing {
            actions.push(Action::TogglePlay);
        }

        // ── Top bar ──
        egui::TopBottomPanel::top("transport")
            .exact_height(38.0)
            .frame(egui::Frame::default().fill(theme::TOOLBAR).inner_margin(Margin::symmetric(6, 0)))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                let logo = self.textures.logo();
                transport::show(ui, &mut self.project, &mut self.history, &self.ui, playing, playhead, logo, &mut actions);
                // Engraved double hairline: a small deco accent, dark not bright.
                let y = r.max.y;
                ui.painter().hline(r.x_range(), y - 0.5, theme::stroke(1.0, theme::LINE));
                ui.painter().hline(r.x_range(), y - 3.0, theme::stroke(1.0, theme::LINE.gamma_multiply(0.5)));
            });

        // ── Status bar ──
        egui::TopBottomPanel::bottom("status")
            .exact_height(20.0)
            .frame(egui::Frame::default().fill(theme::TOOLBAR).inner_margin(Margin::symmetric(8, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let late = self.audio.as_ref().map_or(0, |a| a.stats.late.load(Relaxed));
                    let healthy = self.audio.is_some() && late == 0;
                    let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot.center(), 3.0, if healthy { theme::MINT } else { theme::ROSE });
                    match &self.audio {
                        Some(a) => {
                            let frames = a.stats.frames.load(Relaxed);
                            let ms = frames as f32 / a.sample_rate as f32 * 1000.0;
                            let asked = a.requested.map_or("system default".to_string(), |n| format!("ring {n}"));
                            ui.label(
                                RichText::new(format!(
                                    "{} · {} · {} Hz · {frames} frames ({ms:.1} ms, {asked}) · DSP {:.0}%",
                                    a.host,
                                    a.device_name,
                                    a.sample_rate,
                                    self.dsp_load * 100.0
                                ))
                                .small()
                                .color(theme::TEXT_DIM),
                            );
                            if late > 0 {
                                ui.label(RichText::new(format!("· {late} underruns")).small().color(theme::ROSE))
                                    .on_hover_text("Callbacks that arrived too late: the sound card ran dry (clicks/crackles).");
                            }
                        }
                        None => {
                            let msg = self.audio_error.as_deref().unwrap_or("no audio");
                            ui.label(RichText::new(msg).small().color(theme::ROSE));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "space play · Z-M / Q-P notes · -/= octave ({:+}) · up/down channel · ctrl+Z undo",
                                self.keyboard.octave
                            ))
                            .small()
                            .color(theme::TEXT_DIM),
                        );
                    });
                });
            });

        // ── Workspace ──
        egui::CentralPanel::default().frame(egui::Frame::default().fill(theme::BG)).show(ctx, |ui| {
            let r = ui.max_rect();
            self.textures.pattern(ui.painter(), r);
            self.textures.grain(ui.painter(), r);
        });

        // ── Windows ──
        let play_step = playing.then(|| (playhead / TICKS_PER_STEP as f64) as u32);
        let frame = egui::Frame::window(&ctx.style())
            .fill(theme::PANEL)
            .stroke(theme::stroke(1.0, theme::LINE))
            .corner_radius(cr(5))
            .inner_margin(Margin::same(8));
        egui::Window::new("Channel rack")
            .default_pos([24.0, 62.0])
            .resizable(false)
            .frame(frame)
            .show(ctx, |ui| {
                rack::show(ui, &mut self.project, &mut self.history, &mut self.ui, play_step);
            });

        // ── Actions ──
        for a in actions {
            match a {
                Action::TogglePlay => self.send(if playing { Command::Stop } else { Command::Play }),
                Action::Stop => self.send(Command::Stop),
                Action::Undo => {
                    self.history.undo(&mut self.project);
                }
                Action::Redo => {
                    self.history.redo(&mut self.project);
                }
                Action::NoteOn(key) => self.live(Event::NoteOn { key, velocity: 0.8 }),
                Action::NoteOff(key) => self.live(Event::NoteOff { key }),
                Action::SelectPrev => self.ui.selected = self.ui.selected.saturating_sub(1),
                Action::SelectNext => {
                    self.ui.selected = (self.ui.selected + 1).min(self.project.channels.len().saturating_sub(1))
                }
            }
        }

        // ── Push changes to the engine ──
        if let Some(e) = &mut self.engine {
            self.bridge.sync(&self.project, self.ui.pattern, e);
        }

        // Animate smoothly while sound is moving; idle gently otherwise.
        let busy = playing || self.ui.master_meter[0] > 1e-3 || self.ui.flash.iter().any(|f| *f > 0.01);
        ctx.request_repaint_after(Duration::from_millis(if busy { 8 } else { 50 }));
    }
}
