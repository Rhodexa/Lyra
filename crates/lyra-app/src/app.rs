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
use crate::{audio, demo, editor, files, rack, transport};
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
    Mixer,
    Effect,
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

    /// A window that asked to be brought to the front this frame.
    pub raise: Option<Focus>,
    pub show_design: bool,
    pub show_playlist: bool,
    pub show_mixer: bool,
    /// The mixer track whose effects are shown.
    pub mixer_selected: usize,
    /// The effect open in the effect window: (mixer track, slot).
    pub effect: Option<(usize, usize)>,
    /// An effect slot being dragged to a new place in the rack.
    pub fx_drag: Option<usize>,
    /// Per mixer track [left, right], decaying (for the strip meters).
    pub track_meters: Vec<[f32; 2]>,
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
            raise: None,
            show_design: false,
            show_playlist: true,
            show_mixer: false,
            mixer_selected: 1,
            effect: None,
            fx_drag: None,
            track_meters: Vec::new(),
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
    /// Where the project lives on disk (None = never saved).
    path: Option<std::path::PathBuf>,
    /// The project as last saved/opened: differs from `project` = unsaved changes.
    saved: Project,
    /// The project as last backed up, and when.
    backed_up: Project,
    last_backup: std::time::Instant,
    title: String,
    /// A short status message (saved, opened, errors) and when it appeared.
    toast: Option<(String, bool, std::time::Instant)>,
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
            show_mixer: dev.wants("mixer"),
            ..UiState::default()
        };
        if dev.wants("piano") || dev.wants("editor") {
            ui.selected = 5; // the demo's bass: something melodic to look at
            ui.focus = Focus::Piano;
        }
        // Reopen the last project; first run (or a missing file) gets the demo.
        let (project, path, toast) = match files::last_project().map(|p| (files::load(&p), p)) {
            Some((Ok(project), path)) => (project, Some(path), None),
            Some((Err(e), _)) => (demo::project(), None, Some((e, true, std::time::Instant::now()))),
            None => (demo::project(), None, None),
        };
        let project_for_saved = if path.is_some() { project.clone() } else { Project::default() };
        let project_for_backup = project.clone();
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
            path,
            saved: project_for_saved,
            backed_up: project_for_backup,
            last_backup: std::time::Instant::now(),
            title: String::new(),
            toast,
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

    fn notify(&mut self, text: &str, error: bool) {
        if error {
            eprintln!("lyra: {text}");
        }
        self.toast = Some((text.to_string(), error, std::time::Instant::now()));
    }

    /// Swap in another project: fresh undo history, selection back to the start.
    fn replace_project(&mut self, project: Project, path: Option<std::path::PathBuf>) {
        self.saved = if path.is_some() { project.clone() } else { Project::default() };
        self.project = project;
        self.path = path;
        self.history = History::default();
        self.ui.selected = 0;
        self.ui.pattern = 0;
        self.ui.song_mode = !self.project.clips.is_empty();
        self.piano_framed = None;
        self.pending_seek = Some(0.0);
    }

    fn open(&mut self, path: std::path::PathBuf) {
        match files::load(&path) {
            Ok(p) => {
                files::backup(&self.project);
                files::remember(&path);
                let name = files::display_name(Some(&path));
                self.replace_project(p, Some(path));
                self.notify(&format!("Opened {name}"), false);
            }
            Err(e) => self.notify(&e, true),
        }
    }

    fn save(&mut self, choose: bool) {
        let path = match (&self.path, choose) {
            (Some(p), false) => Some(p.clone()),
            _ => files::pick_save(&files::display_name(self.path.as_deref())),
        };
        let Some(path) = path else { return }; // dialog cancelled
        match files::save(&path, &self.project) {
            Ok(()) => {
                files::remember(&path);
                self.saved = self.project.clone();
                self.notify(&format!("Saved {}", files::display_name(Some(&path))), false);
                self.path = Some(path);
            }
            Err(e) => self.notify(&e, true),
        }
    }

    /// Window title ("My Song • — Lyra" with unsaved changes) and the periodic backup.
    fn housekeeping(&mut self, ctx: &egui::Context) {
        let dirty = self.project != self.saved;
        let title = format!("{}{} — Lyra", files::display_name(self.path.as_deref()), if dirty { " •" } else { "" });
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        if self.last_backup.elapsed() > Duration::from_secs(30) {
            self.last_backup = std::time::Instant::now();
            if self.project != self.backed_up {
                files::backup(&self.project);
                self.backed_up = self.project.clone();
            }
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
        self.ui.track_meters.resize(lyra_engine::MIXER_TRACKS, [0.0; 2]);
        for (t, shown) in self.ui.track_meters.iter_mut().enumerate() {
            let peak = shared.take_track_peak(t);
            for c in 0..2 {
                shown[c] = peak[c].max(shown[c] * 0.86);
            }
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
            Action::ToggleMixer => {
                self.ui.show_mixer ^= true;
                self.ui.raise = Some(Focus::Mixer);
            }
            Action::TogglePlaylist => {
                self.ui.show_playlist ^= true;
                self.ui.raise = Some(Focus::Playlist);
            }
            Action::ToggleSongMode => self.ui.song_mode ^= true,
            Action::SongMode(on) => self.ui.song_mode = on,
            Action::NewProject => {
                files::backup(&self.project); // nothing is ever lost, so no "are you sure?"
                let mut p = Project::default();
                p.add_channel("Kick", "lyra.kick", lyra_dsp::default_params("lyra.kick"));
                self.replace_project(p, None);
                self.notify("New project · the previous one is in Open backup", false);
            }
            Action::OpenProject => {
                if let Some(path) = files::pick_open() {
                    self.open(path);
                }
            }
            Action::OpenBackup => {
                let path = files::backup_path();
                match files::load(&path) {
                    Ok(p) => {
                        self.replace_project(p, None);
                        self.notify("Opened the backup · Save as… to keep it", false);
                    }
                    Err(e) => self.notify(&e, true),
                }
            }
            Action::Save => self.save(false),
            Action::SaveAs => self.save(true),
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
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        files::backup(&self.project); // quitting never loses anything
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.dev.inject(ctx, raw);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.read_engine();
        self.housekeeping(ctx);
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
            .frame(egui::Frame::default().fill(theme::TOOLBAR).inner_margin(Margin::ZERO))
            .show(ctx, |ui| {
                let bar = ui.max_rect();
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
                // A dark rule along the bottom edge sets the toolbar off from the workspace.
                let rule = egui::Rect::from_min_max(egui::pos2(bar.left(), bar.bottom() - 2.0), bar.max);
                ui.painter().rect_filled(rule, cr(0), egui::Color32::from_rgb(0x0b, 0x0b, 0x0b));
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
                Focus::Mixer => "mixer",
                Focus::Effect => "effect",
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
        let rack_win = egui::Window::new("Instrument rack")
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

        if self.ui.show_mixer {
            let mut open = true;
            let win = egui::Window::new("Mixer")
                .id(Id::new("mixer"))
                .open(&mut open)
                .default_pos([16.0, piano_top])
                .default_size([piano_size[0], piano_size[1]])
                .resizable(true)
                // The mixer's background is the rack frame its strips sit in.
                .frame(frame.fill(theme::BG))
                .show(ctx, |ui| crate::mixer::show(ui, &mut self.project, &mut self.history, &mut self.ui));
            self.ui.show_mixer = open;
            if win.is_some_and(|w| clicked_inside(ctx, &w.response)) {
                self.ui.focus = Focus::Mixer;
            }
        }

        // The effect window: one effect from a mixer track's rack, like the
        // instrument window is one channel from the channel rack.
        let fx = self.ui.effect.filter(|&(t, i)| self.project.mixer.get(t).is_some_and(|m| i < m.effects.len()));
        self.ui.effect = fx;
        if let Some((t, i)) = fx {
            let mut open = true;
            let name =
                lyra_dsp::effects::info(&self.project.mixer[t].effects[i].effect).map_or("Missing effect", |e| e.name);
            let win = egui::Window::new(format!("{name} · {}", self.project.mixer[t].name))
                .id(Id::new("effect"))
                .open(&mut open)
                .pivot(egui::Align2::RIGHT_TOP)
                .default_pos([screen.right() - 16.0, piano_top])
                .resizable(false)
                .frame(frame)
                .show(ctx, |ui| crate::mixer::effect_editor(ui, &mut self.project, &mut self.history, t, i));
            if !open {
                self.ui.effect = None;
            }
            if win.is_some_and(|w| clicked_inside(ctx, &w.response)) {
                self.ui.focus = Focus::Effect;
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
                    // A short message (saved, opened, errors) for a few seconds.
                    if let Some((text, error, at)) = &self.toast {
                        if at.elapsed() < Duration::from_secs(if *error { 8 } else { 4 }) {
                            ui.label(RichText::new(text).small().color(if *error { theme::ROSE } else { theme::TEXT }));
                            ui.add_space(12.0);
                            ctx.request_repaint_after(Duration::from_millis(250));
                        }
                    }
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
