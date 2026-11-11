//! The app shell: owns the project, runs one UI frame at a time.
//!
//! Each frame, in order:
//! 1. free engine garbage, read meters/playhead
//! 2. read the keyboard
//! 3. draw panels and windows (they edit `project` directly)
//! 4. apply actions (play, undo, live notes, editing keys → focused window)
//! 5. `bridge.sync` pushes whatever changed to the engine

use crate::keys::{Action, Keyboard};
use crate::piano_roll::PianoRoll;
use crate::theme::{self, cr};
use crate::{audio, bridge::Bridge, demo, editor, rack, transport};
use egui::{Id, Margin, RichText};
use lyra_engine::{Command, EngineHandle};
use lyra_model::{History, Project, TICKS_PER_STEP};
use lyra_plugin::Event;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

/// Which window gets editing keys (arrows, Delete, Ctrl+A/D/Q).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Rack,
    Piano,
    Editor,
}

/// View state that isn't part of the project (not saved, not undoable).
pub struct UiState {
    pub selected: usize,
    pub pattern: usize,
    /// While dragging across steps: Some(true) = painting on, Some(false) = erasing.
    pub paint: Option<bool>,
    /// Per-channel hit flash, 1.0 = just triggered, fades to 0.
    pub flash: Vec<f32>,
    last_triggers: Vec<u32>,
    pub master_meter: [f32; 2],
    pub show_rack: bool,
    pub show_piano: bool,
    pub show_editor: bool,
    pub focus: Focus,
    /// Keys currently held on the typing keyboard (lights the piano roll's keys).
    pub live_keys: Vec<u8>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected: 0,
            pattern: 0,
            paint: None,
            flash: Vec::new(),
            last_triggers: Vec::new(),
            master_meter: [0.0; 2],
            show_rack: true,
            show_piano: false,
            show_editor: false,
            focus: Focus::Rack,
            live_keys: Vec::new(),
        }
    }
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
    piano: PianoRoll,
    /// (pattern, channel id) the piano roll last framed its view on.
    piano_framed: Option<(usize, u32)>,
    textures: theme::Textures,
}

impl LyraApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let (audio, engine, audio_error) = match audio::start() {
            Ok((a, h)) => (Some(a), Some(h), None),
            Err(e) => (None, None, Some(format!("no audio: {e}"))),
        };
        let dev = crate::devtools::DevTools::from_env();
        let mut ui = UiState { show_piano: dev.wants("piano"), show_editor: dev.wants("editor"), ..UiState::default() };
        if dev.wants("piano") || dev.wants("editor") {
            ui.selected = 5; // the demo's bass: something melodic to look at
            ui.focus = Focus::Piano;
        }
        Self {
            project: demo::project(),
            history: History::default(),
            bridge: Bridge::default(),
            engine,
            audio,
            audio_error,
            dsp_load: 0.0,
            keyboard: Keyboard::default(),
            dev,
            ui,
            piano: PianoRoll::default(),
            piano_framed: None,
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

    fn apply(&mut self, a: Action, playing: bool) {
        let ch = self.ui.selected.min(self.project.channels.len().saturating_sub(1));
        let pat = self.ui.pattern.min(self.project.patterns.len() - 1);
        let piano_has_focus = self.ui.focus == Focus::Piano && self.ui.show_piano && !self.project.channels.is_empty();
        match a {
            Action::TogglePlay => self.send(if playing { Command::Stop } else { Command::Play }),
            Action::Stop => self.send(Command::Stop),
            Action::Undo => {
                self.history.undo(&mut self.project);
            }
            Action::Redo => {
                self.history.redo(&mut self.project);
            }
            Action::NoteOn(key) => {
                self.ui.live_keys.push(key);
                self.live(Event::NoteOn { key, velocity: 0.8 });
            }
            Action::NoteOff(key) => {
                self.ui.live_keys.retain(|&k| k != key);
                self.live(Event::NoteOff { key });
            }
            Action::PrevPattern => self.ui.pattern = self.ui.pattern.saturating_sub(1),
            Action::NextPattern => self.ui.pattern = (self.ui.pattern + 1).min(self.project.patterns.len() - 1),
            Action::ToggleRack => self.ui.show_rack ^= true,
            Action::TogglePiano => self.ui.show_piano ^= true,
            Action::ToggleEditor => self.ui.show_editor ^= true,
            _ if piano_has_focus => {
                self.piano.action(a, &mut self.project, &mut self.history, pat, ch);
            }
            Action::Arrow { dy, .. } if dy != 0 => {
                let last = self.project.channels.len().saturating_sub(1);
                self.ui.selected = if dy > 0 { self.ui.selected.saturating_sub(1) } else { (self.ui.selected + 1).min(last) };
            }
            _ => {}
        }
    }
}

/// Did this click land on this window (and not on something drawn above it)?
///
/// ⚠ Never call other `ctx` methods *inside* a `ctx.input(|i| …)` closure: egui
/// holds its (non-re-entrant) context lock for the whole closure, so a nested
/// `ctx.layer_id_at()` waits on itself forever. Read first, then ask.
fn clicked_inside(ctx: &egui::Context, resp: &egui::Response) -> bool {
    let press = ctx.input(|i| {
        (i.pointer.primary_pressed() || i.pointer.secondary_pressed()).then(|| i.pointer.interact_pos()).flatten()
    });
    press.is_some_and(|pos| ctx.layer_id_at(pos) == Some(resp.layer_id))
}

impl eframe::App for LyraApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.dev.inject(ctx, raw);
    }

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
                transport::show(ui, &mut self.project, &mut self.history, &mut self.ui, playing, playhead, logo, &mut actions);
                // Engraved double hairline: a small deco accent, dark not bright.
                let y = r.max.y;
                ui.painter().hline(r.x_range(), y - 0.5, theme::stroke(1.0, theme::LINE));
                ui.painter().hline(r.x_range(), y - 3.0, theme::stroke(1.0, theme::LINE.gamma_multiply(0.5)));
            });

        self.status_bar(ctx);

        // ── Workspace ──
        egui::CentralPanel::default().frame(egui::Frame::default().fill(theme::BG)).show(ctx, |ui| {
            let r = ui.max_rect();
            self.textures.pattern(ui.painter(), r);
            self.textures.grain(ui.painter(), r);
        });

        // ── Windows ──
        let pat = self.ui.pattern.min(self.project.patterns.len() - 1);
        self.ui.pattern = pat;
        let play_tick = playing.then_some(playhead);
        let play_step = play_tick.map(|t| (t / TICKS_PER_STEP as f64) as u32);
        let frame = egui::Frame::window(&ctx.style())
            .fill(theme::PANEL)
            .stroke(theme::stroke(1.0, theme::LINE))
            .corner_radius(cr(5))
            .inner_margin(Margin::same(8));

        // Default layout (first open only): rack top-left, editor top-right,
        // piano roll filling the lower part of the screen.
        let screen = ctx.screen_rect();
        let piano_top = (screen.height() * 0.42).max(300.0);
        let piano_size = [screen.width() - 32.0, (screen.height() - piano_top - 28.0).max(240.0)];

        let mut open = self.ui.show_rack;
        let rack_win = egui::Window::new("Channel rack")
            .id(Id::new("rack"))
            .open(&mut open)
            .default_pos([16.0, 54.0])
            .resizable(false)
            .frame(frame)
            .show(ctx, |ui| {
                rack::show(ui, &mut self.project, &mut self.history, &mut self.ui, play_step);
            });
        self.ui.show_rack = open;
        if rack_win.is_some_and(|w| clicked_inside(ctx, &w.response)) {
            self.ui.focus = Focus::Rack;
        }

        let has_channel = !self.project.channels.is_empty();
        let ch = self.ui.selected.min(self.project.channels.len().saturating_sub(1));
        self.ui.selected = ch;

        if self.ui.show_piano && has_channel {
            let ch_id = self.project.channels[ch].id;
            if self.piano_framed != Some((pat, ch_id)) {
                self.piano.frame_notes(self.project.patterns[pat].notes(ch_id));
                self.piano_framed = Some((pat, ch_id));
            }
            let mut open = true;
            let live_keys = self.ui.live_keys.clone();
            let win = egui::Window::new(format!("Piano roll · {}", self.project.channels[ch].name))
                .id(Id::new("piano"))
                .open(&mut open)
                .default_pos([16.0, piano_top])
                .default_size(piano_size)
                .resizable(true)
                .frame(frame)
                .show(ctx, |ui| {
                    self.piano.show(ui, &mut self.project, &mut self.history, pat, ch, play_tick, &live_keys, &mut actions);
                });
            self.ui.show_piano = open;
            if win.is_some_and(|w| clicked_inside(ctx, &w.response)) {
                self.ui.focus = Focus::Piano;
            }
        } else {
            self.piano_framed = None;
        }

        if self.ui.show_editor && has_channel {
            let mut open = true;
            let win = egui::Window::new(format!("Instrument · {}", self.project.channels[ch].name))
                .id(Id::new("editor"))
                .open(&mut open)
                .pivot(egui::Align2::RIGHT_TOP)
                .default_pos([screen.right() - 16.0, 54.0])
                .resizable(false)
                .frame(frame)
                .show(ctx, |ui| {
                    editor::show(ui, &mut self.project, &mut self.history, ch);
                });
            self.ui.show_editor = open;
            if win.is_some_and(|w| clicked_inside(ctx, &w.response)) {
                self.ui.focus = Focus::Editor;
            }
        }

        // ── Actions ──
        for a in actions {
            self.apply(a, playing);
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

impl LyraApp {
    fn status_bar(&mut self, ctx: &egui::Context) {
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
                                    "{} · {} Hz · {frames} frames ({ms:.1} ms, {asked}) · DSP {:.0}%",
                                    a.host,
                                    a.sample_rate,
                                    self.dsp_load * 100.0
                                ))
                                .small()
                                .color(theme::TEXT_DIM),
                            )
                            .on_hover_text(format!("Output device: {}", a.device_name));
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
                                "Z-M Q-P play · -/= octave {:+} · [ ] pattern · F6 F7 F8 windows",
                                self.keyboard.octave
                            ))
                            .small()
                            .color(theme::TEXT_DIM),
                        );
                    });
                });
            });
    }
}
