//! The app shell: owns the project, runs one UI frame at a time.
//!
//! Each frame, in order:
//! 1. free engine garbage, read meters/playhead
//! 2. read the keyboard
//! 3. draw panels and windows (they edit `project` directly)
//! 4. apply actions (play, undo, live notes, editing keys → focused window)
//! 5. `bridge.sync` pushes whatever changed to the engine

use crate::areas::{self, Area, Editor};
use crate::bridge::{Bridge, Playing};
use crate::keys::{Action, Keyboard};
use crate::theme::{self, cr};
use crate::{audio, demo, editor, files, rack, transport};
use egui::{Id, Margin, RichText};
use lyra_engine::{Command, EngineHandle};
use lyra_model::{History, Project, TICKS_PER_STEP};
use lyra_plugin::Event;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

/// A floating window to bring to the front. (Editing keys don't follow
/// windows: they go to the area under the mouse, see `key_target`.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Editor,
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
    /// The plugin rack, docked on the right (F6).
    pub show_plugins: bool,
    pub show_editor: bool,
    /// Keys currently held on the typing keyboard (lights the piano roll's keys).
    pub live_keys: Vec<u8>,

    /// A window that asked to be brought to the front this frame.
    pub raise: Option<Focus>,
    pub show_design: bool,
    /// "Show this editor": a widget asks, the app makes sure an area shows it
    /// (opening the bottom area if neither does).
    pub show: Option<Editor>,
    /// The editors on screen this frame (lights the toolbar keys).
    pub visible: Vec<Editor>,
    /// "Open this pattern in the piano roll" (double-click in the browser or
    /// on a clip): the top area switches to the piano roll for it.
    pub open_pattern: Option<usize>,
    /// The sample library, as scanned (see `samples`); `rescan` asks for a new scan.
    pub library: Vec<crate::samples::Folder>,
    pub rescan: bool,
    /// "Play this sample now" (a click in the browser), and the last one played.
    pub preview: Option<String>,
    pub previewing: Option<String>,
    /// The browser panel on the left (F4).
    pub show_browser: bool,
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
    /// The current audio clip (what the audio-clip editor shows), and whether
    /// a click on empty arrangement places it (instead of the current pattern).
    pub audio: Option<lyra_model::AudioId>,
    pub place_audio: bool,
    /// The playlist clip (index) the audio-clip editor shows the window and
    /// own settings of: the one last clicked. None = just the source.
    pub audio_clip: Option<usize>,
    /// "Open this audio clip in the audio-clip editor" (a double-click).
    pub open_audio: Option<lyra_model::AudioId>,
    /// The pattern last frame: picking another one goes back to placing patterns.
    last_pattern: usize,
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
            show_plugins: true,
            show_editor: false,
            live_keys: Vec::new(),
            raise: None,
            show_design: false,
            show: None,
            visible: Vec::new(),
            open_pattern: None,
            library: Vec::new(),
            rescan: true,
            preview: None,
            previewing: None,
            show_browser: true,
            mixer_selected: 1,
            effect: None,
            fx_drag: None,
            track_meters: Vec::new(),
            song_mode: false,
            audio: None,
            place_audio: false,
            audio_clip: None,
            open_audio: None,
            last_pattern: 0,
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
    /// The top and bottom areas of the centre (see `areas`).
    areas: [Area; 2],
    /// The bottom area is shown (F5 / F7 / F9 open it on an editor).
    bottom_open: bool,
    /// Where each area was drawn last frame (for "keys go to the area under the mouse").
    area_rects: [Option<egui::Rect>; 2],
    /// The area editing keys go to while the mouse is outside both areas.
    key_area: usize,
    /// The playhead as of this frame (for seeks that depend on where we are).
    playhead: f64,
    /// Decoded samples (shared with samplers on the audio thread).
    samples: crate::samples::Samples,
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
    /// When the sound engine was restarted after a crash (for the crash-loop guard).
    engine_restarts: Vec<std::time::Instant>,
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
        let mut ui =
            UiState { show_editor: dev.wants("editor"), show_design: dev.wants("design"), ..UiState::default() };
        if dev.wants("piano") || dev.wants("editor") {
            ui.selected = 5; // the demo's bass: something melodic to look at
        }
        // LYRA_OPEN=piano / mixer / steps opens the bottom area on that editor.
        let bottom = [
            ("piano", Editor::PianoRoll),
            ("mixer", Editor::Mixer),
            ("steps", Editor::Steps),
            ("audio", Editor::Audio),
        ]
        .into_iter()
        .find(|(name, _)| dev.wants(name))
        .map(|(_, e)| e);
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
            areas: [Area::new(Editor::Arrangement), Area::new(bottom.unwrap_or(Editor::PianoRoll))],
            bottom_open: bottom.is_some(),
            area_rects: [None; 2],
            key_area: 0,
            playhead: 0.0,
            samples: crate::samples::Samples::default(),
            textures: theme::Textures::new(&cc.egui_ctx),
            frame_count: 0,
            raising: None,
            pending_seek: None,
            path,
            saved: project_for_saved,
            backed_up: project_for_backup,
            last_backup: std::time::Instant::now(),
            engine_restarts: Vec::new(),
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
        for a in &mut self.areas {
            a.framed = None;
        }
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

    /// The audio callback caught a panic (a bug in an instrument or effect) and
    /// went silent. Start a fresh engine and stream; the bridge then re-sends
    /// the whole project. At most three restarts a minute: a bug that strikes
    /// on every start shouldn't turn into an endless loop.
    fn recover_engine(&mut self) {
        if !self.audio.as_ref().is_some_and(|a| a.stats.crashed.load(Relaxed)) {
            return;
        }
        let now = std::time::Instant::now();
        self.engine_restarts.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        self.audio = None; // stops the old stream before a new one opens the device
        self.engine = None;
        if self.engine_restarts.len() >= 3 {
            self.audio_error = Some("sound engine stopped after repeated errors".into());
            self.notify(
                "The sound engine keeps failing, so it's off. Save, then restart Lyra (details in the terminal).",
                true,
            );
            return;
        }
        self.engine_restarts.push(now);
        match audio::start() {
            Ok((a, h)) => {
                self.audio = Some(a);
                self.engine = Some(h);
                self.bridge = Bridge::default();
                self.notify("The sound engine hit a bug and was restarted (details in the terminal).", true);
            }
            Err(e) => {
                self.audio_error = Some(format!("no audio: {e}"));
                self.notify("The sound engine hit a bug and couldn't restart.", true);
            }
        }
    }

    fn read_engine(&mut self) {
        self.recover_engine();
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

    /// Which area editing keys go to: the one under the mouse; while the mouse
    /// is elsewhere (browser, plugin rack, toolbar), the last one it was over.
    /// None while the mouse is over a floating window or a menu.
    fn key_target(&mut self, ctx: &egui::Context) -> Option<usize> {
        if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
            if ctx.layer_id_at(pos).is_some_and(|l| l.order != egui::Order::Background) {
                return None;
            }
            if let Some(i) = (0..2).find(|&i| self.area_rects[i].is_some_and(|r| r.contains(pos))) {
                self.key_area = i;
            }
        }
        if !self.bottom_open {
            self.key_area = 0;
        }
        Some(self.key_area)
    }

    /// Library scans, previews, the dragged-sample label, drops that nobody
    /// else took, and sample-loading errors.
    fn sample_housekeeping(&mut self, ctx: &egui::Context) {
        if std::mem::take(&mut self.ui.rescan) {
            use crate::samples::{bundled_root, scan, user_root};
            self.ui.library = [(bundled_root(), "Lyra"), (user_root(), "Your library")]
                .into_iter()
                .filter_map(|(path, name)| scan(&path, name))
                .collect();
        }
        if let Some(name) = self.ui.preview.take() {
            match self.samples.get(&name) {
                Ok(data) => {
                    if let Some(e) = &mut self.engine {
                        let mut sampler = lyra_sampler::Sampler::new();
                        lyra_plugin::Plugin::set_sample(&mut sampler, data);
                        let slot = lyra_engine::ChannelSlot::new(Box::new(sampler), e.sample_rate());
                        e.send(Command::Preview { slot, key: 60 });
                    }
                    self.ui.previewing = Some(name);
                }
                Err(err) => self.notify(&err, true),
            }
        }
        // While a sample is dragged, its name follows the mouse.
        if let Some(drag) = egui::DragAndDrop::payload::<crate::browser::SampleDrag>(ctx) {
            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                let layer = egui::LayerId::new(egui::Order::Tooltip, Id::new("sample-drag"));
                let painter = ctx.layer_painter(layer);
                let text = crate::samples::display_name(&drag.0);
                let galley = painter.layout_no_wrap(text, egui::FontId::proportional(theme::m().text), theme::TEXT);
                let r = egui::Rect::from_min_size(pos + egui::vec2(14.0, 8.0), galley.size() + egui::vec2(16.0, 8.0));
                painter.rect_filled(r, cr(4), theme::PANEL_HI);
                painter.rect_stroke(r, cr(4), theme::stroke(1.0, theme::LINE), egui::StrokeKind::Inside);
                painter.galley(r.min + egui::vec2(8.0, 4.0), galley, theme::TEXT);
            }
        }
        for err in self.samples.take_errors() {
            self.notify(&err, true);
        }
    }

    /// A seek inside the current pattern (the piano roll's ruler). In pattern
    /// mode, plain. In song mode it stays in song mode: it goes to that spot in
    /// the clip of this pattern that's playing now (or the first one); only a
    /// pattern with no clip at all switches to pattern mode.
    fn seek_pattern(&mut self, tick: f64) {
        if !self.ui.song_mode {
            self.pending_seek = Some(tick);
            return;
        }
        let pat = self.ui.pattern.min(self.project.patterns.len() - 1);
        let id = self.project.patterns[pat].id;
        let now = self.playhead;
        let clips = || self.project.clips.iter().filter(|c| c.pattern() == Some(id));
        let clip = clips()
            .find(|c| (c.start as f64..c.end() as f64).contains(&now))
            .or_else(|| clips().min_by_key(|c| c.start));
        match clip {
            Some(c) => {
                let len = self.project.patterns[pat].play_length() as f64 * TICKS_PER_STEP as f64;
                let into = (now - c.start as f64).max(0.0);
                // Same repeat of the pattern the playhead is in, clamped to the clip.
                let rep = (into / len).floor() * len;
                self.pending_seek = Some((c.start as f64 + rep + tick).min(c.end() as f64 - 1.0));
            }
            None => {
                self.ui.song_mode = false;
                self.pending_seek = Some(tick);
            }
        }
    }

    /// F5 / F7 / F9: open the bottom area on `editor`; again folds it away.
    fn toggle_bottom(&mut self, editor: Editor) {
        if self.bottom_open && self.areas[1].editor == editor {
            self.bottom_open = false;
        } else {
            self.areas[1].set(editor);
            self.bottom_open = true;
        }
    }

    /// An editing key, for the area `target`. Returns true when that area's
    /// editor used it.
    fn edit_in_area(&mut self, ctx: &egui::Context, a: Action, target: Option<usize>) -> bool {
        let Some(i) = target else { return false };
        let ch = self.ui.selected.min(self.project.channels.len().saturating_sub(1));
        let pat = self.ui.pattern.min(self.project.patterns.len() - 1);
        let area = &mut self.areas[i];
        match area.editor {
            Editor::Arrangement => area.playlist.action(a, &mut self.project, &mut self.history),
            Editor::PianoRoll if !self.project.channels.is_empty() => {
                let handled = area.piano.action(a, &mut self.project, &mut self.history, pat, ch);
                if handled && matches!(a, Action::Copy | Action::Cut) {
                    // egui only reports Ctrl+V when the *system* clipboard holds text, so
                    // leave a marker there; the notes themselves stay in Lyra's clipboard.
                    ctx.copy_text("lyra: notes".to_string());
                }
                handled
            }
            _ => false,
        }
    }

    fn apply(&mut self, ctx: &egui::Context, a: Action, playing: bool, target: Option<usize>) {
        if is_edit(a) && self.edit_in_area(ctx, a, target) {
            return;
        }
        match a {
            Action::TogglePlay => self.send(if playing { Command::Stop } else { Command::Play }),
            // Stop while stopped: back to the very start (of the song in song
            // mode, of the pattern in pattern mode).
            Action::Stop if !playing => self.pending_seek = Some(0.0),
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
            Action::SeekPattern(tick) => self.seek_pattern(tick),
            Action::SwapEditor => {
                let i = target.unwrap_or(0);
                self.areas[i].swap();
            }
            Action::SelectChannel(i) => self.ui.selected = i.min(self.project.channels.len().saturating_sub(1)),
            Action::Octave(d) => self.keyboard.octave = (self.keyboard.octave + d).clamp(-3, 3),
            Action::PrevPattern => self.ui.pattern = self.ui.pattern.saturating_sub(1),
            Action::NextPattern => self.ui.pattern = (self.ui.pattern + 1).min(self.project.patterns.len() - 1),
            Action::ToggleRack => self.ui.show_plugins ^= true,
            Action::TogglePiano => self.toggle_bottom(Editor::PianoRoll),
            Action::ToggleEditor => {
                self.ui.show_editor ^= true;
                self.ui.raise = Some(Focus::Editor);
            }
            Action::ToggleDesign => self.ui.show_design ^= true,
            Action::ToggleBrowser => self.ui.show_browser ^= true,
            Action::ToggleMixer => self.toggle_bottom(Editor::Mixer),
            Action::TogglePlaylist => self.toggle_bottom(Editor::Arrangement),
            Action::ToggleSongMode => self.ui.song_mode ^= true,
            Action::SongMode(on) => self.ui.song_mode = on,
            Action::NewProject => {
                files::backup(&self.project); // nothing is ever lost, so no "are you sure?"
                let mut p = Project::default();
                p.add_channel("Kick", "lyra.kick", lyra_catalog::default_params("lyra.kick"));
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
            | Action::MakeUnique
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
        .find(|c| c.pattern() == Some(pattern.id) && (c.start as f64..c.end() as f64).contains(&playhead))
        .map(|c| (playhead - c.start as f64) % loop_len)
}

/// The bottom area's panel id while it shows `editor` (one per editor, so each
/// keeps its own height).
fn bottom_panel_id(editor: Editor) -> Id {
    Id::new("area-bottom").with(editor.name())
}

/// A folded side panel: a thin strip with an arrow; click it to unfold.
fn folded_strip(ctx: &egui::Context, id: &'static str, left: bool, hint: &str, open: &mut bool) {
    let panel = if left { egui::SidePanel::left(id) } else { egui::SidePanel::right(id) };
    panel.resizable(false).exact_width(14.0).frame(egui::Frame::default().fill(theme::PANEL)).show(ctx, |ui| {
        let r = ui.max_rect();
        let resp = ui.interact(r, Id::new(id).with("unfold"), egui::Sense::click());
        let c = egui::pos2(r.center().x, r.top() + 20.0);
        let ink = if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
        let d = if left { 1.0 } else { -1.0 }; // the arrow points to where the panel unfolds
        let pts = vec![c + egui::vec2(-1.5 * d, -4.0), c + egui::vec2(2.5 * d, 0.0), c + egui::vec2(-1.5 * d, 4.0)];
        ui.painter().add(egui::Shape::line(pts, theme::stroke(1.5, ink)));
        if resp.on_hover_text(hint).clicked() {
            *open = true;
        }
    });
}

impl eframe::App for LyraApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        files::backup(&self.project); // quitting never loses anything
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.dev.inject(ctx, raw);
        self.keyboard.take_tabs(ctx, raw);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.read_engine();
        self.housekeeping(ctx);
        let (playing, playhead) =
            self.engine.as_ref().map_or((false, 0.0), |e| (e.shared().is_playing(), e.shared().playhead()));
        self.playhead = playhead;
        let mut actions = self.keyboard.poll(ctx);
        self.dev.frame(ctx);
        if std::mem::take(&mut self.dev.autoplay) && !playing {
            actions.push(Action::TogglePlay);
        }

        self.ui.visible = [Some(self.areas[0].editor), self.bottom_open.then_some(self.areas[1].editor)]
            .into_iter()
            .flatten()
            .collect();

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
        // ── Docked: browser (left), plugin rack (right), the two areas (centre) ──
        self.docked(ctx, playing, playhead, &mut actions);

        // ── Windows ──
        // The zoom factor set in `new()` only applies from the 2nd frame. Windows
        // placed before that get default sizes in un-zoomed units and end up too
        // big for the screen, so wait until the zoom has settled.
        self.frame_count += 1;
        if self.frame_count <= 2 {
            ctx.request_repaint();
        } else {
            self.windows(ctx);
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
                Focus::Editor => "editor",
                Focus::Effect => "effect",
            };
            ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, Id::new(id)));
            self.raising = (frames > 1).then_some((f, frames - 1));
            ctx.request_repaint();
        }

        self.sample_housekeeping(ctx);

        // ── Actions ──
        let target = self.key_target(ctx);
        for a in actions {
            self.apply(ctx, a, playing, target);
        }

        // ── Push changes to the engine ──
        if let Some(e) = &mut self.engine {
            let target = if self.ui.song_mode { Playing::Song } else { Playing::Pattern(self.ui.pattern) };
            self.bridge.sync(&self.project, target, e, &mut self.samples);
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
    /// The docked layout: browser on the left and plugin rack on the right
    /// (both fold to a thin strip), and the centre split into two areas, each
    /// showing the editor picked in its top-left corner (see `areas`). The
    /// bottom area opens and folds with F5 / F7 / F9.
    fn docked(&mut self, ctx: &egui::Context, playing: bool, playhead: f64, actions: &mut Vec<Action>) {
        let m = theme::m();
        let panel = egui::Frame::default().fill(theme::PANEL).inner_margin(Margin::same(8));

        // ── Browser ──
        if self.ui.show_browser {
            // (Clicking the browser leaves keyboard focus where it was.)
            egui::SidePanel::left("browser")
                .resizable(true)
                .default_width(230.0)
                .width_range(160.0..=480.0)
                .frame(panel)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("BROWSER").size(m.text_small).color(theme::TEXT_DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("<").on_hover_text("Fold the browser away  (F4)").clicked() {
                                self.ui.show_browser = false;
                            }
                        });
                    });
                    crate::browser::show(ui, &mut self.project, &mut self.history, &mut self.ui);
                });
        } else {
            folded_strip(ctx, "browser-folded", true, "Unfold the browser  (F4)", &mut self.ui.show_browser);
        }

        // ── Plugin rack (right) ──
        if self.ui.show_plugins {
            egui::SidePanel::right("plugins")
                .resizable(true)
                .default_width(300.0)
                .width_range(220.0..=520.0)
                .frame(panel)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("PLUGINS").size(m.text_small).color(theme::TEXT_DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(">").on_hover_text("Fold the plugin rack away  (F6)").clicked() {
                                self.ui.show_plugins = false;
                            }
                        });
                    });
                    rack::plugins(ui, &mut self.project, &mut self.history, &mut self.ui);
                    // A sample dropped anywhere else on the panel: a new sampler for it.
                    let released = ui.input(|i| i.pointer.any_released());
                    if released && ui.rect_contains_pointer(ui.max_rect()) {
                        if let Some(drag) = egui::DragAndDrop::take_payload::<crate::browser::SampleDrag>(ui.ctx()) {
                            crate::browser::add_sampler(&mut self.project, &mut self.history, &mut self.ui, &drag.0);
                        }
                    }
                });
        } else {
            folded_strip(ctx, "plugins-folded", false, "Unfold the plugin rack  (F6)", &mut self.ui.show_plugins);
        }

        // ── The two areas ──
        // Ask-to-show from widgets ("open the piano roll"): fine if an area
        // already shows it, otherwise the bottom area opens on it.
        if self.ui.pattern != self.ui.last_pattern {
            self.ui.place_audio = false;
            self.ui.last_pattern = self.ui.pattern;
        }
        // Double-clicked an audio clip: show it in the audio-clip editor.
        if let Some(id) = self.ui.open_audio.take() {
            self.ui.audio = Some(id);
            self.ui.show = Some(Editor::Audio);
        }
        if let Some(e) = self.ui.show.take() {
            let shown = self.areas[0].editor == e || (self.bottom_open && self.areas[1].editor == e);
            if !shown {
                self.areas[1].set(e);
                self.bottom_open = true;
            }
        }
        // Double-clicked a pattern: the top area edits it in the piano roll, on
        // the selected instrument if it plays in that pattern, otherwise on the
        // first one that does (melodic ones first).
        if let Some(i) = self.ui.open_pattern.take().filter(|&i| i < self.project.patterns.len()) {
            self.ui.pattern = i;
            let pattern = &self.project.patterns[i];
            let plays = |c: &lyra_model::Channel| !pattern.notes(c.id).is_empty();
            let chans = &self.project.channels;
            if !chans.get(self.ui.selected).is_some_and(plays) {
                let melodic = |c: &lyra_model::Channel| lyra_catalog::info(&c.instrument).is_some_and(|x| x.melodic);
                let pick = chans.iter().position(|c| plays(c) && melodic(c)).or_else(|| chans.iter().position(plays));
                if let Some(ch) = pick {
                    self.ui.selected = ch;
                }
            }
            self.areas[0].set(Editor::PianoRoll);
        }
        let avail = ctx.available_rect().height();
        // One panel id per editor: egui remembers each one's height, so the
        // mixer keeps its own size and the piano roll gets its own back.
        let editor = self.areas[1].editor;
        let default_h = match editor {
            Editor::Mixer => crate::mixer::STRIP_H + m.control_h + 4.0 + 2.0 * 8.0 + 4.0,
            Editor::Audio => avail * 0.36,
            _ => avail * 0.5,
        };
        let r = egui::TopBottomPanel::bottom(bottom_panel_id(editor))
            .resizable(true)
            .default_height(default_h)
            .height_range(160.0..=(avail - 160.0).max(160.0))
            .frame(panel)
            .show_animated(ctx, self.bottom_open, |ui| self.area_ui(1, ui, playing, playhead, actions));
        self.area_rects[1] = r.map(|r| r.response.rect).filter(|_| self.bottom_open);
        let r =
            egui::CentralPanel::default().frame(panel).show(ctx, |ui| self.area_ui(0, ui, playing, playhead, actions));
        self.area_rects[0] = Some(r.response.rect);
    }

    /// One area: its header (editor picker + the editor's own controls) and
    /// the editor, with this area's own memory for it.
    fn area_ui(&mut self, i: usize, ui: &mut egui::Ui, playing: bool, playhead: f64, actions: &mut Vec<Action>) {
        let pat = self.ui.pattern.min(self.project.patterns.len() - 1);
        self.ui.pattern = pat;
        let has_channel = !self.project.channels.is_empty();
        let ch = self.ui.selected.min(self.project.channels.len().saturating_sub(1));
        self.ui.selected = ch;
        // Where we are *inside the current pattern* (see `pattern_position`).
        let local = pattern_position(&self.project, pat, playhead, self.ui.song_mode);
        let live_keys = self.ui.live_keys.clone();
        let LyraApp { areas, project: p, history: h, ui: st, samples, .. } = self;
        let area = &mut areas[i];
        ui.push_id(("area", i), |ui| {
            ui.horizontal(|ui| {
                areas::picker(ui, area);
                ui.add_space(6.0);
                match area.editor {
                    Editor::Arrangement => area.playlist.toolbar(ui, p, st),
                    Editor::Audio => area.audio.toolbar(ui, p, h, st),
                    Editor::PianoRoll if has_channel => {
                        let color = theme::channel_color(p.channels[ch].color);
                        area.piano.toolbar(ui, p, h, color, ch);
                    }
                    _ => {}
                }
            });
            ui.add_space(4.0);
            match area.editor {
                Editor::Arrangement => {
                    let song_playhead = if st.song_mode { playhead } else { 0.0 };
                    let song_playing = playing && st.song_mode;
                    area.playlist.show(ui, p, h, st, song_playhead, song_playing, samples, actions);
                }
                Editor::Audio => {
                    let song_playhead = st.song_mode.then_some(playhead);
                    area.audio.show(ui, p, h, st, samples, song_playhead, actions);
                }
                Editor::PianoRoll if has_channel => {
                    let ch_id = p.channels[ch].id;
                    if area.framed != Some((pat, ch_id)) {
                        area.piano.frame_notes(p.patterns[pat].notes(ch_id));
                        area.framed = Some((pat, ch_id));
                    }
                    let (tick, sounding) = (local.unwrap_or(playhead), playing && local.is_some());
                    area.piano.show(ui, p, h, pat, ch, tick, sounding, &live_keys, actions);
                }
                Editor::PianoRoll => {
                    ui.label(RichText::new("Add an instrument in the plugin rack first.").color(theme::TEXT_DIM));
                }
                Editor::Mixer => {
                    // The mixer's strips sit on the background, like a rack frame.
                    ui.painter().rect_filled(ui.available_rect_before_wrap(), cr(0), theme::BG);
                    egui::ScrollArea::vertical().id_salt("mixer-v").show(ui, |ui| crate::mixer::show(ui, p, h, st));
                }
                Editor::Steps => {
                    let play_step = local.filter(|_| playing).map(|t| (t / TICKS_PER_STEP as f64) as u32);
                    egui::ScrollArea::vertical()
                        .id_salt("steps-v")
                        .show(ui, |ui| rack::show(ui, p, h, st, play_step, true));
                }
            }
        });
    }

    /// The floating windows: instrument editor, effect, design panel.
    fn windows(&mut self, ctx: &egui::Context) {
        let frame = egui::Frame::window(&ctx.style())
            .fill(theme::PANEL)
            .stroke(theme::stroke(1.0, theme::LINE))
            .corner_radius(cr(5))
            .inner_margin(Margin::same(8));

        // Default spots (first open only): instrument editor top-right, effect below it.
        let screen = ctx.screen_rect();
        let piano_top = (screen.height() * 0.42).max(300.0);

        let has_channel = !self.project.channels.is_empty();
        let ch = self.ui.selected.min(self.project.channels.len().saturating_sub(1));
        self.ui.selected = ch;

        if self.ui.show_editor && has_channel {
            let mut open = true;
            egui::Window::new(format!("Instrument · {}", self.project.channels[ch].name))
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
        }

        // The effect window: one effect from a mixer track's rack, like the
        // instrument window is one channel from the channel rack.
        let fx = self.ui.effect.filter(|&(t, i)| self.project.mixer.get(t).is_some_and(|m| i < m.effects.len()));
        self.ui.effect = fx;
        if let Some((t, i)) = fx {
            let mut open = true;
            let name = lyra_catalog::effects::info(&self.project.mixer[t].effects[i].effect)
                .map_or("Missing effect", |e| e.name);
            egui::Window::new(format!("{name} · {}", self.project.mixer[t].name))
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
                                "Z-M Q-P play · -/= octave {:+} · [ ] pattern · F5 F7 F9 bottom area · F4 F6 panels",
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

/// True while the pointer is on (or dragging) one of the docked panels'
/// resize edges. The arranger and piano roll read the mouse directly, so
/// without this a press on the edge also placed a clip or a note.
pub fn on_panel_edge(ctx: &egui::Context) -> bool {
    let pos = ctx.input(|i| i.pointer.interact_pos());
    let panels = ["browser", "plugins"].map(Id::new).into_iter().chain(Editor::ALL.map(bottom_panel_id));
    panels.into_iter().any(|panel| {
        ctx.read_response(panel.with("__resize")).is_some_and(|r| {
            r.hovered() || r.dragged() || r.is_pointer_button_down_on() || pos.is_some_and(|p| r.rect.contains(p))
        })
    })
}
