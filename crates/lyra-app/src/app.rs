//! The app shell: owns the project, runs one UI frame at a time.
//!
//! Each frame, in order:
//! 1. free engine garbage, read meters/playhead
//! 2. read the keyboard
//! 3. draw panels and windows (they edit `project` directly)
//! 4. apply actions (play, undo, live notes, editing keys → focused window)
//! 5. `bridge.sync` pushes whatever changed to the engine

use crate::bridge::{Bridge, Playing};
use crate::keys::{Action, Keyboard};
use crate::piano_roll::PianoRoll;
use crate::playlist::Playlist;
use crate::theme::{self, cr};
use crate::{audio, demo, editor, rack, transport};
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
    Playlist,
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
    /// While sweeping across mute lights: the mute state being painted.
    pub led_paint: Option<bool>,
    /// A window that asked to be brought to the front this frame.
    pub raise: Option<Focus>,
    pub show_design: bool,
    pub show_playlist: bool,
    /// Space plays the whole playlist (true) or just the current pattern.
    pub song_mode: bool,
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
            led_paint: None,
            raise: None,
            show_design: false,
            show_playlist: true,
            song_mode: false,
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
    playlist: Playlist,
    /// (pattern, channel id) the piano roll last framed its view on.
    piano_framed: Option<(usize, u32)>,
    textures: theme::Textures,
    frame_count: u64,
    /// A window being raised, and for how many more frames (see `windows`).
    raising: Option<(Focus, u8)>,
    /// A seek waiting until after this frame's sync: if the same click also
    /// switched pattern/song mode, the new sequence must reach the engine
    /// first, or the seek is clamped to the old length (and then reset to 0).
    pending_seek: Option<f64>,
}

impl LyraApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        // Sizes live in theme::Metrics (tunable live with F12). Zoom on top of that
        // stays available: Ctrl +/- at runtime, LYRA_ZOOM for the starting value.
        let zoom = std::env::var("LYRA_ZOOM").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0f32);
        cc.egui_ctx.set_zoom_factor(zoom.clamp(0.5, 3.0));
        let (audio, engine, audio_error) = match audio::start() {
            Ok((a, h)) => (Some(a), Some(h), None),
            Err(e) => (None, None, Some(format!("no audio: {e}"))),
        };
        let dev = crate::devtools::DevTools::from_env();
        let mut ui = UiState {
            show_piano: dev.wants("piano"),
            show_editor: dev.wants("editor"),
            show_design: dev.wants("design"),
            ..UiState::default()
        };
        if dev.wants("piano") || dev.wants("editor") {
            ui.selected = 5; // the demo's bass: something melodic to look at
            ui.focus = Focus::Piano;
        }
        let project = demo::project();
        ui.song_mode = !project.clips.is_empty(); // a project with an arrangement opens ready to play it
        Self {
            project,
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
            playlist: Playlist::default(),
            piano_framed: None,
            textures: theme::Textures::new(&cc.egui_ctx),
            frame_count: 0,
            raising: None,
            pending_seek: None,
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

    fn apply(&mut self, ctx: &egui::Context, a: Action, playing: bool) {
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
            Action::Seek(tick) => self.pending_seek = Some(tick),
            Action::SelectChannel(i) => self.ui.selected = i.min(self.project.channels.len().saturating_sub(1)),
            Action::Octave(d) => self.keyboard.octave = (self.keyboard.octave + d).clamp(-3, 3),
            Action::PrevPattern => self.ui.pattern = self.ui.pattern.saturating_sub(1),
            Action::NextPattern => self.ui.pattern = (self.ui.pattern + 1).min(self.project.patterns.len() - 1),
            Action::ToggleRack => self.ui.show_rack ^= true,
            Action::TogglePiano => {
                self.ui.show_piano ^= true;
                self.ui.raise = Some(Focus::Piano);
            }
            Action::ToggleEditor => {
                self.ui.show_editor ^= true;
                self.ui.raise = Some(Focus::Editor);
            }
            Action::ToggleDesign => self.ui.show_design ^= true,
            Action::TogglePlaylist => {
                self.ui.show_playlist ^= true;
                self.ui.raise = Some(Focus::Playlist);
            }
            Action::ToggleSongMode => self.ui.song_mode ^= true,
            Action::SongMode(on) => self.ui.song_mode = on,
            // Only keys the playlist actually handles stop here; the rest (copy,
            // paste, quantize…) fall through instead of vanishing.
            _ if self.ui.focus == Focus::Playlist
                && self.ui.show_playlist
                && is_edit(a)
                && self.playlist.action(a, &mut self.project, &mut self.history) => {}
            _ if piano_has_focus => {
                let handled = self.piano.action(a, &mut self.project, &mut self.history, pat, ch);
                if handled && matches!(a, Action::Copy | Action::Cut) {
                    // egui only reports Ctrl+V when the *system* clipboard holds text, so
                    // leave a marker there; the notes themselves stay in Lyra's clipboard.
                    ctx.copy_text("lyra: notes".to_string());
                }
            }
            Action::Arrow { dy, .. } if dy != 0 => {
                let last = self.project.channels.len().saturating_sub(1);
                self.ui.selected =
                    if dy > 0 { self.ui.selected.saturating_sub(1) } else { (self.ui.selected + 1).min(last) };
            }
            _ => {}
        }
    }
}

/// Editing keys (as opposed to transport/window keys) — routed to the focused editor.
fn is_edit(a: Action) -> bool {
    matches!(
        a,
        Action::Arrow { .. }
            | Action::Delete
            | Action::SelectAll
            | Action::Deselect
            | Action::Copy
            | Action::Cut
            | Action::Paste
            | Action::Duplicate
            | Action::Quantize
    )
}

/// The playhead expressed inside pattern `pat`.
fn pattern_position(p: &Project, pat: usize, playhead: f64, song_mode: bool) -> Option<f64> {
    if !song_mode {
        return Some(playhead);
    }
    let pattern = p.patterns.get(pat)?;
    let loop_len = (pattern.play_length() * TICKS_PER_STEP) as f64;
    p.clips
        .iter()
        .find(|c| c.pattern == pattern.id && (c.start as f64..c.end() as f64).contains(&playhead))
        .map(|c| (playhead - c.start as f64) % loop_len)
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
            .exact_height(theme::m().toolbar_h)
            .frame(egui::Frame::default().fill(theme::TOOLBAR).inner_margin(Margin::symmetric(6, 0)))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                let logo = self.textures.logo();
                let octave = self.keyboard.octave;
                transport::show(
                    ui,
                    &mut self.project,
                    &mut self.history,
                    &mut self.ui,
                    playing,
                    playhead,
                    logo,
                    octave,
                    &mut actions,
                );
                // Engraved double hairline: a small deco accent, dark not bright.
                let y = r.max.y;
                ui.painter().hline(r.x_range(), y - 0.5, theme::stroke(1.0, theme::LINE));
                ui.painter().hline(r.x_range(), y - 3.0, theme::stroke(1.0, theme::LINE.gamma_multiply(0.5)));
            });

        self.status_bar(ctx);
        let workspace = ctx.available_rect(); // what's left between the bars

        // ── Workspace ──
        egui::CentralPanel::default().frame(egui::Frame::default().fill(theme::BG)).show(ctx, |ui| {
            let r = ui.max_rect();
            self.textures.pattern(ui.painter(), r);
            self.textures.grain(ui.painter(), r);
        });

        // ── Windows ──
        // The zoom factor set in `new()` only applies from the 2nd frame. Windows
        // placed before that get default sizes in un-zoomed units and end up too
        // big for the screen, so wait until the zoom has settled.
        self.frame_count += 1;
        if self.frame_count <= 2 {
            ctx.request_repaint();
        } else {
            self.windows(ctx, workspace, playing, playhead, &mut actions);
        }

        // Bring a window forward when something asked for it ("open the piano
        // roll" on an already-open piano roll should show it, not do nothing).
        // Keep raising for a few frames: the click that asked (e.g. a double-click
        // in the rack) also makes egui raise the rack, and that would win otherwise.
        if let Some(f) = self.ui.raise.take() {
            self.raising = Some((f, 4));
        }
        if let Some((f, frames)) = self.raising {
            let id = match f {
                Focus::Rack => "rack",
                Focus::Piano if self.piano.maximized => "piano-full",
                Focus::Piano => "piano",
                Focus::Editor => "editor",
                Focus::Playlist => "playlist",
            };
            ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, Id::new(id)));
            self.ui.focus = f;
            self.raising = (frames > 1).then_some((f, frames - 1));
            ctx.request_repaint();
        }

        // ── Actions ──
        for a in actions {
            self.apply(ctx, a, playing);
        }

        // ── Push changes to the engine ──
        if let Some(e) = &mut self.engine {
            let target = if self.ui.song_mode { Playing::Song } else { Playing::Pattern(self.ui.pattern) };
            self.bridge.sync(&self.project, target, e);
            if let Some(tick) = self.pending_seek.take() {
                e.send(Command::Seek(tick));
            }
        }

        // Animate smoothly while sound is moving; idle gently otherwise.
        let busy = playing || self.ui.master_meter[0] > 1e-3 || self.ui.flash.iter().any(|f| *f > 0.01);
        ctx.request_repaint_after(Duration::from_millis(if busy { 8 } else { 50 }));
    }
}

impl LyraApp {
    /// The floating windows: channel rack, piano roll, instrument editor.
    fn windows(
        &mut self,
        ctx: &egui::Context,
        workspace: egui::Rect,
        playing: bool,
        playhead: f64,
        actions: &mut Vec<Action>,
    ) {
        let pat = self.ui.pattern.min(self.project.patterns.len() - 1);
        self.ui.pattern = pat;
        // Where we are *inside the current pattern*: the playhead itself in pattern
        // mode; in song mode, the position within a clip of this pattern (if one is playing).
        let local = pattern_position(&self.project, pat, playhead, self.ui.song_mode);
        let play_step = local.filter(|_| playing).map(|t| (t / TICKS_PER_STEP as f64) as u32);
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

        if self.ui.show_playlist {
            let mut open = true;
            let win = egui::Window::new("Playlist")
                .id(Id::new("playlist"))
                .open(&mut open)
                .default_pos([16.0, piano_top])
                .default_size(piano_size)
                .resizable(true)
                .frame(frame)
                .show(ctx, |ui| {
                    let song_playhead = if self.ui.song_mode { playhead } else { 0.0 };
                    let song_playing = playing && self.ui.song_mode;
                    self.playlist.show(
                        ui,
                        &mut self.project,
                        &mut self.history,
                        &mut self.ui,
                        song_playhead,
                        song_playing,
                        actions,
                    );
                });
            self.ui.show_playlist = open;
            if win.is_some_and(|w| clicked_inside(ctx, &w.response)) {
                self.ui.focus = Focus::Playlist;
            }
        }

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
            // Full screen is a *separate* window identity: egui remembers each one's
            // size and position, so "restore" returns to exactly where it was.
            let piano_id = if self.piano.maximized { "piano-full" } else { "piano" };
            let mut window = egui::Window::new(format!("Piano roll · {}", self.project.channels[ch].name))
                .id(Id::new(piano_id))
                .open(&mut open)
                .frame(frame);
            window = if self.piano.maximized {
                let r = workspace.shrink(4.0);
                window.fixed_pos(r.min).fixed_size(r.size() - egui::vec2(16.0, 44.0)).collapsible(false)
            } else {
                window.default_pos([16.0, piano_top]).default_size(piano_size).resizable(true)
            };
            let win = window.show(ctx, |ui| {
                self.piano.show(
                    ui,
                    &mut self.project,
                    &mut self.history,
                    pat,
                    ch,
                    local.unwrap_or(playhead),
                    playing && local.is_some(),
                    &live_keys,
                    actions,
                );
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

        if self.ui.show_design {
            let mut open = true;
            egui::Window::new("Design · sizes")
                .id(Id::new("design"))
                .open(&mut open)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .default_pos([screen.right() - 16.0, screen.bottom() - 40.0])
                .resizable(false)
                .frame(frame)
                .show(ctx, crate::design::show);
            self.ui.show_design = open;
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(theme::m().text_small + 12.0)
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
                                    .on_hover_text(
                                        "Callbacks that arrived too late: the sound card ran dry (clicks/crackles).",
                                    );
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
