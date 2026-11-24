//! The app shell: owns the project, runs one UI frame at a time.
//!
//! Each frame, in order:
//! 1. free engine garbage, read meters/playhead
//! 2. read the keyboard
//! 3. draw panels and windows (they edit `project` directly)
//! 4. apply actions (play, undo, live notes, editing keys → focused window)
//! 5. `bridge.sync` pushes whatever changed to the engine

use crate::areas::{self, Area, Editor};
use crate::widgets::{self, Hint};
use crate::bridge::{Bridge, Playing};
use crate::keys::{Action, Keyboard};
use crate::theme::{self, cr};
use crate::{audio, demo, editor, files, rack, transport};
use egui::{Id, Margin, RichText};
use lyra_engine::{Command, EngineHandle};
use lyra_model::{Clip, History, Project, TICKS_PER_STEP};
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
    /// Whether the bottom area is open this frame (lights its toolbar key).
    pub bottom_open: bool,
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
    /// The clip last clicked on the arrangement: the next click on empty
    /// space places a copy of it, and the audio-clip editor shows its window
    /// and own settings. None = nothing picked (or a pattern / audio clip
    /// picked in the top bar or the browser).
    pub clip: Option<lyra_model::ClipId>,
    /// "Open this audio clip in the audio-clip editor" (a double-click).
    pub open_audio: Option<lyra_model::AudioId>,
    /// The current automation (the automation editor shows it), and "open
    /// this one" (a double-click on its clip).
    pub automation: Option<lyra_model::AutomationId>,
    pub open_automation: Option<lyra_model::AutomationId>,
    /// A click on empty arrangement places the current automation (picked in
    /// the Automation tab).
    pub place_automation: bool,
    /// Which rack the right panel shows.
    pub right_tab: RightTab,
    /// Which tab the browser shows, and what the Files tab searches for.
    pub left_tab: LeftTab,
    pub search: String,
    /// A MIDI file to bring in whole (double-clicked in the browser).
    pub import_midi: Option<std::path::PathBuf>,
    /// What plays on its own right now (see `Action::ListenClip`), and where
    /// it is (seconds into the sound): the editors draw a playhead on it.
    pub listening: Option<(Listening, f64)>,
    /// Sample lengths in seconds, read from file headers as rows show up.
    pub lengths: std::collections::HashMap<std::path::PathBuf, Option<f64>>,
    /// The control changed last, anywhere: what "Automate" automates.
    pub last_tweaked: Option<lyra_model::Target>,
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
            bottom_open: false,
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
            clip: None,
            open_audio: None,
            automation: None,
            open_automation: None,
            place_automation: false,
            right_tab: RightTab::Plugins,
            left_tab: LeftTab::Files,
            search: String::new(),
            import_midi: None,
            lengths: std::collections::HashMap::new(),
            listening: None,
            last_tweaked: None,
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
    frame_count: u64,
    /// A window being raised, and for how many more frames (see `windows`).
    raising: Option<(Focus, u8)>,
    /// A seek waiting until after this frame's sync: if the same click also
    /// switched pattern/song mode, the new sequence must reach the engine
    /// first, or the seek is clamped to the old length (and then reset to 0).
    pending_seek: Option<f64>,
    /// The loop region the engine has (song or pattern ticks).
    sent_loop: Option<(u32, u32)>,
    /// The scratch tape (the audio-clip editor's Recorder tab).
    recorder: crate::recorder::Recorder,
    /// What was last sent to play on its own (the engine says where it is).
    listening_to: Option<Listening>,
    /// Where the context menu (F3) is open, if it is.
    context_menu: Option<egui::Pos2>,
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
    /// A WAV export running on its own thread.
    export: Option<Export>,
    /// Leaving this project (new, open, quit) is waiting on "save changes?".
    leaving: Option<Leave>,
    /// The window may close now (the question was answered).
    may_close: bool,
}

/// The browser's tabs: the library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeftTab {
    Files,
    Instruments,
    Effects,
}

/// The right panel's racks: what's in the project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RightTab {
    Plugins,
    Patterns,
    Audio,
    Automation,
}

/// Ways to leave the current project. With unsaved changes, each one asks first.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Leave {
    New,
    Open,
    OpenBackup,
    Quit,
}

/// An export in progress: the render runs on a thread (it takes seconds),
/// the UI keeps going and shows how far it got.
struct Export {
    progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    thread: std::thread::JoinHandle<Result<String, String>>,
}

impl LyraApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        // The synths keep today's look whatever Lyra's style becomes (plugin_look.rs).
        crate::plugin_look::capture_style(&cc.egui_ctx);
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
        if let Some(n) = std::env::var("LYRA_SELECT").ok().and_then(|v| v.trim().parse().ok()) {
            ui.selected = n;
        }
        // LYRA_OPEN=piano / mixer / steps opens the bottom area on that editor.
        let bottom = [
            ("piano", Editor::PianoRoll),
            ("mixer", Editor::Mixer),
            ("steps", Editor::Steps),
            ("audio", Editor::Audio),
            ("automation", Editor::Automation),
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
        let mut app = Self {
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
            frame_count: 0,
            raising: None,
            pending_seek: None,
            sent_loop: None,
            context_menu: None,
            listening_to: None,
            recorder: crate::recorder::Recorder::default(),
            path,
            saved: project_for_saved,
            backed_up: project_for_backup,
            last_backup: std::time::Instant::now(),
            engine_restarts: Vec::new(),
            title: String::new(),
            toast,
            export: None,
            leaving: None,
            may_close: false,
        };
        // A tape Lyra didn't get to clear last time (it stopped mid-take).
        if let Some(secs) = app.recorder.recover() {
            app.notify(&format!("The recorder kept your tape from last time ({secs:.0} s)"), false);
        }
        app
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

    /// "Automate": an automation for the control changed last, 4 bars, flat
    /// at its value, on a free track where the song is; the editor opens on it.
    fn automate(&mut self) {
        let Some(target) = self.ui.last_tweaked else {
            self.notify("Move a knob first: Automate makes a curve for the last control you changed", false);
            return;
        };
        let Some((info, value, name)) = crate::automation::control(&self.project, target) else { return };
        let len = 4 * lyra_model::TICKS_PER_BAR;
        let start = if self.ui.song_mode {
            (self.playhead as u32 / lyra_model::TICKS_PER_BAR) * lyra_model::TICKS_PER_BAR
        } else {
            0
        };
        let p = &mut self.project;
        // An empty track first (automation reads best on its own row), else
        // any track free for those bars.
        let empty = |t: usize| !p.clips.iter().any(|c| c.track == t);
        let free = |t: usize| !p.clips.iter().any(|c| c.track == t && c.start < start + len && c.end() > start);
        let tracks = 0..p.tracks.len();
        let Some(track) = tracks.clone().find(|&t| empty(t)).or_else(|| tracks.clone().find(|&t| free(t))) else {
            self.notify("No free track there for the automation", true);
            return;
        };
        self.history.checkpoint(p);
        let i = p.add_automation(&name, target, info.to_normalized(value), len);
        let id = p.automations[i].id;
        let c = p.add_clip(lyra_model::Clip::of_automation(id, track, start, len));
        self.ui.clip = Some(p.clips[c].id);
        self.ui.automation = Some(id);
        self.ui.open_automation = Some(id);
        self.notify(&format!("Automating {name}"), false);
    }

    /// "Link": the current automation moves the control changed last too.
    fn link_tweaked(&mut self) {
        let (Some(target), Some(i)) =
            (self.ui.last_tweaked, self.ui.automation.and_then(|id| self.project.automation_index(id)))
        else {
            return;
        };
        let name = crate::automation::control(&self.project, target).map_or_else(String::new, |c| c.2);
        if self.project.automations[i].links.iter().any(|l| l.target == target) {
            self.notify(&format!("{name} is already on this curve"), false);
            return;
        }
        self.history.checkpoint(&self.project);
        self.project.automations[i].links.push(lyra_model::Link::full(target));
        self.notify(&format!("This curve moves {name} too"), false);
    }

    fn dirty(&self) -> bool {
        self.project != self.saved
    }

    /// Leave this project: right away if everything is saved, else after
    /// asking (see `ask_to_save`).
    fn leave(&mut self, ctx: &egui::Context, how: Leave) {
        if self.dirty() {
            self.leaving = Some(how);
        } else {
            self.left(ctx, how);
        }
    }

    /// Actually leave (the question, if any, was answered). The project is
    /// backed up first either way: nothing is ever lost.
    fn left(&mut self, ctx: &egui::Context, how: Leave) {
        match how {
            Leave::New => {
                files::backup(&self.project);
                let mut p = Project::default();
                p.add_channel("Kick", "lyra.kick", lyra_catalog::default_params("lyra.kick"));
                self.replace_project(p, None);
                self.notify("New project", false);
            }
            Leave::Open => {
                if let Some(path) = files::pick_open() {
                    self.open(path);
                }
            }
            Leave::OpenBackup => match files::load(&files::backup_path()) {
                Ok(p) => {
                    self.replace_project(p, None);
                    self.notify("Opened the backup · Save as… to keep it", false);
                }
                Err(e) => self.notify(&e, true),
            },
            Leave::Quit => {
                self.may_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// "Save changes?" while leaving a project that has unsaved changes.
    fn ask_to_save(&mut self, ctx: &egui::Context) {
        let Some(how) = self.leaving else { return };
        let name = files::display_name(self.path.as_deref());
        let mut answer = None;
        // A short question: the LCD dialog (catalog-next, settled), drawn by Lyra.
        let modal = egui::Modal::new(Id::new("save-changes")).frame(egui::Frame::NONE).show(ctx, |ui| {
            let bg = ui.painter().add(egui::Shape::Noop);
            let pad = 16.0;
            let inner = egui::Frame::NONE.inner_margin(pad).show(ui, |ui| {
                ui.set_width(360.0);
                let title = ui.fonts(|f| f.layout_no_wrap("SAVE CHANGES?".into(), theme::label_bold(13.0), theme::LCD_INK));
                let (tr, _) = ui.allocate_exact_size(title.size(), egui::Sense::hover());
                ui.painter().galley(tr.min, title, theme::LCD_INK);
                ui.add_space(10.0);
                ui.label(RichText::new(format!("“{name}” has changes that aren't saved.")).font(theme::read(14.0)).color(theme::LCD_INK));
                ui.add_space(2.0);
                ui.label(
                    RichText::new("If you don't save, they're still in Open backup until the next backup replaces them.")
                        .font(theme::read(12.0))
                        .color(theme::LCD_INK_DIM),
                );
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if widgets::bare_button(ui, "Don't save", true, "").clicked() {
                        answer = Some(false);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let save = widgets::button(ui, widgets::Legend::Text("Save"), Some(theme::L_MINT), "Save  (Enter)");
                        if save.clicked() {
                            answer = Some(true);
                        }
                        if widgets::button(ui, widgets::Legend::Text("Cancel"), None, "Stay  (Esc)").clicked() {
                            self.leaving = None;
                        }
                    });
                });
            });
            let r = inner.response.rect;
            ui.painter().set(
                bg,
                egui::Shape::Vec(vec![
                    egui::Shape::rect_filled(r.translate(egui::vec2(0.0, 8.0)).expand(4.0), theme::cr(10), egui::Color32::from_black_alpha(110)),
                    egui::Shape::rect_filled(r, theme::cr(7), theme::mix(theme::LCD_INK, egui::Color32::BLACK, 0.5)),
                    egui::Shape::rect_filled(r.shrink(theme::OUTLINE_W), theme::cr(5), theme::LCD),
                ]),
            );
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                answer = Some(true);
            }
        });
        if modal.should_close() {
            self.leaving = None; // Esc or a click outside: stay
        }
        if let Some(save) = answer {
            self.leaving = None;
            if save {
                self.save(false);
                if self.dirty() {
                    return; // the save dialog was cancelled (or failed): stay
                }
            }
            self.left(ctx, how);
        }
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

    /// Export WAV (or stems): pick where, then render on a thread.
    fn export(&mut self, stems: bool) {
        if self.export.is_some() {
            self.notify("Already exporting", false);
            return;
        }
        let name = files::display_name(self.path.as_deref());
        let target = if stems {
            files::pick_folder("Export stems: pick a folder").map(|d| d.join(format!("{name} stems")))
        } else {
            files::pick_export(&name)
        };
        let Some(target) = target else { return }; // dialog cancelled
        let project = self.project.clone();
        let progress = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = progress.clone();
        let thread = std::thread::spawn(move || {
            let mut samples = crate::samples::Samples::default();
            let shown = files::display_name(Some(&target));
            if stems {
                let written = crate::render::render_stems(&project, &mut samples, &target, Some(&counter))?;
                Ok(format!("Exported {} stems to {shown}", written.len()))
            } else {
                let r =
                    crate::render::render(&project, &mut samples, &target, crate::render::Part::Mix, Some(&counter))?;
                let clip = if r.peak > 1.0 { " · it clipped: turn the master down" } else { "" };
                Ok(format!(
                    "Exported {shown}.wav · {:.0} s, peak {:+.1} dB{clip}",
                    r.seconds,
                    crate::render::db(r.peak)
                ))
            }
        });
        self.export = Some(Export { progress, thread });
    }

    /// What the context bar and the context menu (F3) offer: the editing
    /// area's actions for what's selected there.
    fn context_items(&self) -> Vec<(Action, String)> {
        let area = &self.areas[self.key_area];
        match area.editor {
            Editor::PianoRoll if !self.project.channels.is_empty() => {
                let ch = self.ui.selected.min(self.project.channels.len() - 1);
                let pat = self.ui.pattern.min(self.project.patterns.len() - 1);
                area.piano.context(&self.project, pat, ch)
            }
            Editor::Arrangement => area.playlist.context(),
            _ => Vec::new(),
        }
    }

    /// Play `data` from `from` to `to` seconds into mixer `track`, on its own.
    fn listen(
        &mut self,
        what: Listening,
        data: std::sync::Arc<lyra_plugin::SampleData>,
        from: f64,
        to: f64,
        track: usize,
        gain: f32,
    ) {
        let listen = lyra_engine::Listen { data, from, to, track, gain };
        self.send(Command::Listen(Some(Box::new(listen))));
        self.listening_to = Some(what);
    }

    /// Hear the open audio clip: the window of the clip last clicked (with
    /// its gain), or the whole trimmed sound, through its mixer insert.
    fn listen_clip(&mut self) {
        let Some(ai) = crate::audio_editor::current(&self.project, &mut self.ui) else { return };
        let a = &self.project.audio[ai];
        let Ok(data) = self.samples.get(&a.sample) else { return };
        let spt = self.project.seconds_per_tick();
        let clip = self
            .ui
            .clip
            .and_then(|c| self.project.clip_index(c))
            .filter(|&c| self.project.clips[c].audio() == Some(a.id));
        let (from, to, gain) = match clip {
            Some(c) => {
                let c = &self.project.clips[c];
                let from = a.start + c.offset as f64 * spt;
                ((from), (from + c.len as f64 * spt).min(a.end), a.volume * lyra_plugin::db_to_gain(c.gain))
            }
            None => (a.start, a.end, a.volume),
        };
        let (id, track) = (a.id, a.mixer);
        self.listen(Listening::Clip(id), data, from, to, track, gain);
    }

    /// Where kept recordings go: the song's `audio/`, or (a song never saved)
    /// Lyra's own recordings folder, until the first save moves them in.
    fn recordings_dir(&self) -> std::path::PathBuf {
        match &self.path {
            Some(song) => song.parent().unwrap_or(std::path::Path::new(".")).join("audio"),
            None => files::temp_recordings(),
        }
    }

    /// The Recorder asked to keep the tape: send it to the arranger, or
    /// append it to the open audio clip.
    fn recorder_ask(&mut self) {
        use crate::recorder::{write_wav, Ask};
        let Some(ask) = self.recorder.ask.take() else { return };
        let tape = {
            let s = self.recorder.shared.lock().unwrap_or_else(|e| e.into_inner());
            if s.tape.frames() == 0 {
                return;
            }
            s.tape.sample()
        };
        let dir = self.recordings_dir();
        match ask {
            Ask::SendToArranger => {
                let prefix = self.recorder.prefix.trim();
                let base = format!("{} {}", if prefix.is_empty() { "Recording" } else { prefix }, files::now_stamp());
                let path = files::free_path(&dir, &base, "wav");
                if let Err(e) = write_wav(&path, &tape) {
                    return self.notify(&e, true);
                }
                self.history.checkpoint(&self.project);
                let name = path.file_stem().map_or_else(|| base.clone(), |s| s.to_string_lossy().into_owned());
                let i = self.project.add_audio(&name, &path.to_string_lossy(), tape.seconds() as f64);
                self.project.audio[i].mixer = self.recorder.track; // it plays where you heard it
                let id = self.project.audio[i].id;
                // At the playhead, on the open clip's track (or the first empty one).
                let track = self
                    .ui
                    .clip
                    .and_then(|c| self.project.clip_index(c))
                    .map_or_else(|| crate::midi::free_track(&self.project), |c| self.project.clips[c].track);
                while self.project.tracks.len() <= track {
                    let n = self.project.tracks.len() + 1;
                    self.project.tracks.push(lyra_model::Track { name: format!("Track {n}"), mute: false });
                }
                let at = if self.ui.song_mode { self.playhead.max(0.0) as u32 } else { 0 };
                let len = ((tape.seconds() as f64 / self.project.seconds_per_tick()).ceil() as u32).max(1);
                let c = self.project.add_clip(Clip::of_audio(id, track, at, len));
                self.ui.audio = Some(id);
                self.ui.clip = Some(self.project.clips[c].id);
                self.recorder.shared.lock().unwrap_or_else(|e| e.into_inner()).clear(); // moved, not copied
                self.notify(&format!("Sent \"{name}\" to the arranger"), false);
            }
            Ask::Append => {
                let Some(ai) = crate::audio_editor::current(&self.project, &mut self.ui) else { return };
                let old_name = self.project.audio[ai].sample.clone();
                let old = match self.samples.get(&old_name) {
                    Ok(d) => d,
                    Err(e) => return self.notify(&e, true),
                };
                let joined = crate::recorder::append(&old, &tape);
                let stem = crate::samples::path_for(&old_name)
                    .file_stem()
                    .map_or_else(|| "audio".to_string(), |s| s.to_string_lossy().into_owned());
                let path = files::next_version(&dir, &stem, "wav");
                if let Err(e) = write_wav(&path, &joined) {
                    return self.notify(&e, true);
                }
                self.history.checkpoint(&self.project);
                let a = &mut self.project.audio[ai];
                // A trim that reached the old end now reaches the new one.
                let was_whole = a.end >= old.seconds() as f64 - 1e-3;
                a.sample = path.to_string_lossy().into_owned();
                if was_whole {
                    a.end = joined.seconds() as f64;
                }
                self.recorder.shared.lock().unwrap_or_else(|e| e.into_inner()).clear(); // moved, not copied
                let shown = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                self.notify(&format!("Appended {:.1} s: the clip now plays {shown}", tape.seconds()), false);
            }
        }
    }

    /// Bring a MIDI file in: as a song part (every track an instrument with a
    /// pattern, from `track` down) or as notes for the instrument being edited.
    fn import_midi(&mut self, path: &std::path::Path, to: Midi) {
        let song = match crate::midi::load(path) {
            Ok(s) if s.tracks.is_empty() => return self.notify("That MIDI file has no notes", true),
            Ok(s) => s,
            Err(e) => return self.notify(&e, true),
        };
        self.history.checkpoint(&self.project);
        let name = files::display_name(Some(path));
        match to {
            Midi::Song { at, track } => {
                let n = crate::midi::import(&mut self.project, &song, at, track);
                self.notify(&format!("{name}: {n} parts, each a new instrument and pattern"), false);
            }
            Midi::Notes { at } => {
                let Some(ch) = self.project.channels.get(self.ui.selected).map(|c| c.id) else { return };
                let pat = self.ui.pattern.min(self.project.patterns.len() - 1);
                let notes = crate::midi::merged(&song, at);
                let n = notes.len();
                self.project.patterns[pat].notes_mut(ch).extend(notes);
                self.notify(&format!("{name}: {n} notes"), false);
            }
        }
    }

    /// Export the selected notes, a span of bars, or the pattern as a MIDI file.
    fn export_midi(&mut self, a: Action) {
        let area = &self.areas[self.key_area];
        let export = match area.editor {
            Editor::PianoRoll if !self.project.channels.is_empty() => {
                let ch = self.ui.selected.min(self.project.channels.len() - 1);
                let pat = self.ui.pattern.min(self.project.patterns.len() - 1);
                area.piano.midi_export(a, &self.project, pat, ch)
            }
            Editor::Arrangement => {
                area.playlist.midi_export(a, &self.project, &files::display_name(self.path.as_deref()))
            }
            _ => None,
        };
        let Some(export) = export else {
            self.notify("No notes there to export", true);
            return;
        };
        let Some(path) = files::pick_save_midi(&crate::render::file_safe(&export.name)) else { return };
        let bytes = crate::midi::write(&export.name, self.project.bpm, export.length, &export.tracks);
        match std::fs::write(&path, bytes) {
            Ok(()) => {
                let notes: usize = export.tracks.iter().map(|t| t.notes.len()).sum();
                self.notify(&format!("Exported {notes} notes to {}", path.display()), false);
            }
            Err(e) => self.notify(&format!("Couldn't write {}: {e}", path.display()), true),
        }
    }

    /// The context menu (F3): the context bar's actions, at the mouse.
    fn context_menu(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(at) = self.context_menu else { return };
        let items = self.context_items();
        let mut chosen = None;
        let area =
            egui::Area::new(Id::new("context-menu")).order(egui::Order::Foreground).fixed_pos(at).show(ctx, |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(180.0);
                    if items.is_empty() {
                        ui.label(RichText::new("Nothing to do here").small().color(theme::TEXT_DIM));
                    }
                    for (action, label) in &items {
                        if ui.button(label).clicked() {
                            chosen = Some(*action);
                        }
                    }
                });
            });
        let away = ctx.input(|i| i.pointer.any_pressed() || i.key_pressed(egui::Key::Escape))
            && !ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| area.response.rect.contains(p));
        if let Some(a) = chosen {
            actions.push(a);
            self.context_menu = None;
        } else if away {
            self.context_menu = None;
        }
    }

    /// Export data: a saved song gets "<song> data" next to it, an unsaved
    /// one asks where. Renders every stem to measure it, so it runs on a thread.
    fn export_data(&mut self) {
        if self.export.is_some() {
            self.notify("Already exporting", false);
            return;
        }
        let title = files::display_name(self.path.as_deref());
        let dir = match &self.path {
            Some(path) => crate::report::default_folder(path, &title),
            None => match files::pick_folder("Export data: pick a folder") {
                Some(d) => d.join(format!("{title} data")),
                None => return,
            },
        };
        let project = self.project.clone();
        let progress = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = progress.clone();
        let thread = std::thread::spawn(move || {
            let mut samples = crate::samples::Samples::default();
            let written = crate::report::export(&project, &title, &mut samples, &dir, Some(&counter))?;
            Ok(format!("Exported {} files to {}", written.len(), dir.display()))
        });
        self.export = Some(Export { progress, thread });
    }

    fn poll_export(&mut self, ctx: &egui::Context) {
        let Some(export) = &self.export else { return };
        if !export.thread.is_finished() {
            // The status bar draws the progress (see `status_bar`).
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        }
        let export = self.export.take().expect("checked above");
        match export.thread.join() {
            Ok(Ok(text)) => self.notify(&text, false),
            Ok(Err(e)) => self.notify(&format!("Export failed: {e}"), true),
            Err(_) => self.notify("Export failed: the renderer crashed", true),
        }
    }

    fn save(&mut self, choose: bool) {
        let path = match (&self.path, choose) {
            (Some(p), false) => Some(p.clone()),
            _ => files::pick_save(&files::display_name(self.path.as_deref())),
        };
        let Some(path) = path else { return }; // dialog cancelled
                                               // Recordings kept before the song had a folder move into its audio/ now.
        if let Err(e) = files::adopt_recordings(&mut self.project, &path) {
            self.notify(&e, true);
        }
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

    /// Collect: the song's folder gets a copy of every file it uses from
    /// elsewhere, and the song is saved pointing at them. An unsaved song is
    /// saved first (it needs a folder).
    fn collect(&mut self) {
        if self.path.is_none() {
            self.save(true);
        }
        let Some(path) = self.path.clone() else { return };
        let before = self.project.clone();
        match files::collect(&mut self.project, &path) {
            Ok((_, 0)) if self.project == before => self.notify("Every file is already in the song's folder", false),
            Ok((copied, missing)) => {
                if self.project != before {
                    self.save(false);
                }
                let lost =
                    if missing > 0 { format!(" · {missing} missing, left as they were") } else { String::new() };
                self.notify(&format!("Copied {copied} files into audio/{lost}"), missing > 0);
            }
            Err(e) => self.notify(&format!("Collect failed: {e}"), true),
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
        // What plays on its own, and where (its end clears it).
        let at = shared.listening();
        if at.is_none() {
            self.listening_to = None;
        }
        self.ui.listening = self.listening_to.zip(at);
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
            // Lyra's own kit, your library folder, then the folders you added.
            let added = files::browser_folders().into_iter().map(|path| {
                let name =
                    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
                (path, name)
            });
            self.ui.library = [(bundled_root(), "Lyra".to_string()), (user_root(), "Your library".to_string())]
                .into_iter()
                .chain(added)
                .filter_map(|(path, name)| scan(&path, &name))
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
                let galley = painter.layout_no_wrap(text, theme::read(13.0), theme::INK);
                let r = egui::Rect::from_min_size(pos + egui::vec2(14.0, 8.0), galley.size() + egui::vec2(20.0, 12.0));
                // The settled drag ghost: a plate with its outline.
                theme::outlined(&painter, r, theme::CORNER, theme::PLATE);
                painter.galley(r.min + egui::vec2(10.0, 6.0), galley, theme::INK);
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
            Action::ToggleBottom => self.bottom_open ^= true,
            Action::ToggleSongMode => self.ui.song_mode ^= true,
            Action::SongMode(on) => self.ui.song_mode = on,
            Action::Automate => self.automate(),
            Action::LinkTweaked => self.link_tweaked(),
            Action::NewProject => self.leave(ctx, Leave::New),
            Action::OpenProject => self.leave(ctx, Leave::Open),
            Action::OpenBackup => self.leave(ctx, Leave::OpenBackup),
            Action::Save => self.save(false),
            Action::SaveAs => self.save(true),
            Action::Collect => self.collect(),
            Action::ExportWav | Action::ExportStems => self.export(a == Action::ExportStems),
            Action::ExportData => self.export_data(),
            Action::ExportNotesMidi | Action::ExportBarsMidi | Action::ExportPatternMidi => self.export_midi(a),
            Action::ListenClip => self.listen_clip(),
            Action::ListenTape => {
                let (data, secs) = {
                    let s = self.recorder.shared.lock().unwrap_or_else(|e| e.into_inner());
                    (std::sync::Arc::new(s.tape.sample()), s.tape.seconds())
                };
                if secs > 0.0 {
                    let track = self.recorder.track;
                    self.listen(Listening::Tape, data, 0.0, secs, track, 1.0);
                }
            }
            Action::StopListening => {
                self.send(Command::Listen(None));
                self.listening_to = None;
            }
            Action::ImportMidi => {
                if let Some(path) = files::pick_open_midi() {
                    let row = crate::midi::free_track(&self.project);
                    self.import_midi(&path, Midi::Song { at: 0, track: row });
                }
            }
            Action::ContextMenu => self.context_menu = ctx.input(|i| i.pointer.hover_pos()),
            Action::ClearLoop => {
                let area = &mut self.areas[self.key_area];
                (area.piano.looped, area.playlist.looped) = (None, None);
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
/// The folded bottom area: a thin strip along the bottom with an arrow; click it to unfold.
fn folded_bottom(ctx: &egui::Context, open: &mut bool) {
    let frame = egui::Frame::default().fill(theme::PANEL);
    egui::TopBottomPanel::bottom("bottom-folded").resizable(false).exact_height(14.0).frame(frame).show(ctx, |ui| {
        let r = ui.max_rect();
        let resp = ui.interact(r, Id::new("bottom-folded").with("unfold"), egui::Sense::click());
        let c = r.center();
        let ink = if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
        let pts = vec![c + egui::vec2(-4.0, 1.5), c + egui::vec2(0.0, -2.5), c + egui::vec2(4.0, 1.5)];
        ui.painter().add(egui::Shape::line(pts, theme::stroke(1.5, ink)));
        if resp.hint("Unfold the bottom area  (F5)").clicked() {
            *open = true;
        }
    });
}

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
        if resp.hint(hint).clicked() {
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
        self.poll_export(ctx);
        let (playing, playhead) =
            self.engine.as_ref().map_or((false, 0.0), |e| (e.shared().is_playing(), e.shared().playhead()));
        self.playhead = playhead;
        // Editors show automation moving their knobs while the song plays.
        let shown = if playing && self.ui.song_mode { self.bridge.automated_at(playhead) } else { Vec::new() };
        crate::automation::set_shown(shown);
        // Closing the window with unsaved changes asks first, like New and Open.
        let closing = ctx.input(|i| i.viewport().close_requested());
        if closing && !self.may_close && !self.dev.closing && self.dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.leaving = Some(Leave::Quit);
        }
        self.ask_to_save(ctx);
        let mut actions = self.keyboard.poll(ctx);
        self.dev.frame(ctx);
        if std::mem::take(&mut self.dev.autoplay) && !playing {
            actions.push(Action::TogglePlay);
        }

        self.ui.bottom_open = self.bottom_open;

        // ── Top bar ──
        egui::TopBottomPanel::top("transport")
            .exact_height(theme::m().toolbar_h)
            .frame(egui::Frame::default().fill(theme::TOOLBAR).inner_margin(Margin::ZERO))
            .show(ctx, |ui| {
                let bar = ui.max_rect();
                let octave = self.keyboard.octave;
                transport::show(
                    ui,
                    &mut self.project,
                    &mut self.history,
                    &mut self.ui,
                    playing,
                    playhead,
                    octave,
                    &mut actions,
                );
                // A dark rule along the bottom edge sets the toolbar off from the workspace.
                let rule = egui::Rect::from_min_max(egui::pos2(bar.left(), bar.bottom() - 2.0), bar.max);
                ui.painter().rect_filled(rule, cr(0), egui::Color32::from_rgb(0x0b, 0x0b, 0x0b));
            });

        self.status_bar(ctx, &mut actions);
        // ── Docked: browser (left), plugin rack (right), the two areas (centre) ──
        self.docked(ctx, playing, playhead, &mut actions);
        self.context_menu(ctx, &mut actions);
        // The recorder listens while its tab is on screen (or its tape runs).
        let shown = if self.bottom_open { 2 } else { 1 };
        let tape_tab = self.areas[..shown]
            .iter()
            .any(|a| a.editor == Editor::Audio && a.audio.tab == crate::audio_editor::AudioTab::Recorder);
        self.recorder.poll(tape_tab);
        self.recorder_ask();
        // MIDI files brought in this frame: double-clicked in the browser, or dropped on an editor.
        if let Some(path) = self.ui.import_midi.take() {
            let track = crate::midi::free_track(&self.project);
            self.import_midi(&path, Midi::Song { at: 0, track });
        }
        for i in 0..self.areas.len() {
            if let Some((path, at)) = self.areas[i].piano.dropped_midi.take() {
                self.import_midi(&path, Midi::Notes { at });
            }
            if let Some((path, at, track)) = self.areas[i].playlist.dropped_midi.take() {
                self.import_midi(&path, Midi::Song { at, track });
            }
        }

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

        // New effects get their ids (automation finds them by it).
        if self.project.mixer.iter().any(|t| t.effects.iter().any(|e| e.id == 0)) {
            self.project.number_effects();
        }
        // ── Push changes to the engine ──
        // Sample files rewritten this frame are read again, and their players remade.
        for name in crate::samples::take_reloads() {
            self.samples.forget(&name);
            self.bridge.force_rebuild();
        }
        if let Some(e) = &mut self.engine {
            let target = if self.ui.song_mode { Playing::Song } else { Playing::Pattern(self.ui.pattern) };
            self.bridge.sync(&self.project, target, e, &mut self.samples);
            if let Some(t) = self.bridge.tweaked.take() {
                self.ui.last_tweaked = Some(t);
            }
            if let Some(tick) = self.pending_seek.take() {
                e.send(Command::Seek(tick));
            }
            // The loop region: the arrangement's in song mode, the piano roll's in pattern mode.
            // (Only from areas on screen: a loop you can't see mustn't hold playback.)
            let shown = if self.bottom_open { 2 } else { 1 };
            let looped = self.areas[..shown].iter().find_map(|a| match a.editor {
                Editor::Arrangement if self.ui.song_mode => a.playlist.looped,
                Editor::PianoRoll if !self.ui.song_mode => a.piano.looped,
                _ => None,
            });
            if looped != self.sent_loop {
                e.send(Command::SetLoop(looped.map(|(a, b)| (a as f64, b as f64))));
                self.sent_loop = looped;
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
        // The areas have no padding: the header pads itself, and each editor
        // brings its own if it wants any (timelines run edge to edge).
        let panel = egui::Frame::default().fill(theme::PANEL).inner_margin(Margin::ZERO);
        // Side panels: the header runs edge to edge, the content keeps a margin.
        let side = egui::Frame::default().fill(theme::PANEL);
        let inside = egui::Frame::default().inner_margin(Margin { left: 8, right: 8, top: 6, bottom: 8 });

        // ── Browser ──
        if self.ui.show_browser {
            // (Clicking the browser leaves keyboard focus where it was.)
            let r = egui::SidePanel::left("browser")
                .resizable(true)
                .default_width(230.0)
                .width_range(160.0..=480.0)
                .frame(side)
                .show(ctx, |ui| {
                    let tabs = [LeftTab::Files, LeftTab::Instruments, LeftTab::Effects];
                    let chosen = tabs.iter().position(|&t| t == self.ui.left_tab).unwrap_or(0);
                    let (tab, fold) = crate::widgets::panel_header(
                        ui,
                        &["Files", "Instruments", "Effects"],
                        chosen,
                        false,
                        "Fold the browser away  (F4)",
                    );
                    if let Some(i) = tab {
                        self.ui.left_tab = tabs[i];
                    }
                    if fold {
                        self.ui.show_browser = false;
                    }
                    inside.show(ui, |ui| crate::browser::show(ui, &mut self.project, &mut self.history, &mut self.ui));
                });
            ctx.data_mut(|d| d.insert_temp(Id::new("seam-left"), r.response.rect));
        } else {
            ctx.data_mut(|d| d.remove::<egui::Rect>(Id::new("seam-left")));
            folded_strip(ctx, "browser-folded", true, "Unfold the browser  (F4)", &mut self.ui.show_browser);
        }

        // ── Plugin rack (right) ──
        if self.ui.show_plugins {
            let r = egui::SidePanel::right("plugins")
                .resizable(true)
                .default_width(300.0)
                .width_range(220.0..=520.0)
                .frame(side)
                .show(ctx, |ui| {
                    // Tabs: the project's racks. (The browser on the left is the library.)
                    let tabs = [RightTab::Patterns, RightTab::Plugins, RightTab::Audio, RightTab::Automation];
                    let chosen = tabs.iter().position(|&t| t == self.ui.right_tab).unwrap_or(0);
                    let (tab, fold) = crate::widgets::panel_header(
                        ui,
                        &["Patterns", "Plugins", "Audio", "Automation"],
                        chosen,
                        true,
                        "Fold this panel away  (F6)",
                    );
                    if let Some(i) = tab {
                        self.ui.right_tab = tabs[i];
                    }
                    if fold {
                        self.ui.show_plugins = false;
                    }
                    let song_playhead = (playing && self.ui.song_mode).then_some(playhead);
                    inside.show(ui, |ui| match self.ui.right_tab {
                        RightTab::Plugins => rack::plugins(ui, &mut self.project, &mut self.history, &mut self.ui),
                        RightTab::Patterns => {
                            egui::ScrollArea::vertical().id_salt("pool-patterns").show(ui, |ui| {
                                crate::pool::patterns(ui, &mut self.project, &mut self.history, &mut self.ui)
                            });
                        }
                        RightTab::Audio => {
                            egui::ScrollArea::vertical().id_salt("pool-audio").show(ui, |ui| {
                                crate::pool::audio(
                                    ui,
                                    &mut self.project,
                                    &mut self.history,
                                    &mut self.ui,
                                    song_playhead,
                                )
                            });
                        }
                        RightTab::Automation => {
                            egui::ScrollArea::vertical()
                                .id_salt("pool-auto")
                                .show(ui, |ui| crate::pool::automations(ui, &mut self.project, &mut self.ui));
                        }
                    });
                    // A sample dropped anywhere else on the plugin rack: a new sampler for it.
                    let released = ui.input(|i| i.pointer.any_released());
                    if self.ui.right_tab == RightTab::Plugins && released && ui.rect_contains_pointer(ui.max_rect()) {
                        if let Some(drag) = egui::DragAndDrop::take_payload::<crate::browser::SampleDrag>(ui.ctx()) {
                            crate::browser::add_sampler(&mut self.project, &mut self.history, &mut self.ui, &drag.0);
                        }
                    }
                });
            ctx.data_mut(|d| d.insert_temp(Id::new("seam-right"), r.response.rect));
        } else {
            ctx.data_mut(|d| d.remove::<egui::Rect>(Id::new("seam-right")));
            folded_strip(ctx, "plugins-folded", false, "Unfold the plugin rack  (F6)", &mut self.ui.show_plugins);
        }

        // ── The two areas ──
        // Ask-to-show from widgets ("open the piano roll"): fine if an area
        // already shows it, otherwise the bottom area opens on it.
        if self.ui.pattern != self.ui.last_pattern {
            self.ui.place_audio = false;
            self.ui.place_automation = false;
            // Picked another pattern (top bar, browser, [ ]): the next click
            // places that one, not a copy of the last clip.
            if self.ui.clip.and_then(|id| self.project.clip_index(id)).is_some_and(|i| {
                self.project.clips[i].pattern() != self.project.patterns.get(self.ui.pattern).map(|p| p.id)
            }) {
                self.ui.clip = None;
            }
            self.ui.last_pattern = self.ui.pattern;
        }
        // Double-clicked an automation clip (or just made one): its editor.
        if let Some(id) = self.ui.open_automation.take() {
            self.ui.automation = Some(id);
            self.ui.show = Some(Editor::Automation);
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
            Editor::Audio | Editor::Automation => avail * 0.36,
            _ => avail * 0.5,
        };
        if !self.bottom_open {
            folded_bottom(ctx, &mut self.bottom_open);
        }
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
        // The splitters' grips: on the seam under the mouse (or being dragged).
        let left = ctx.data(|d| d.get_temp::<egui::Rect>(Id::new("seam-left")));
        let right = ctx.data(|d| d.get_temp::<egui::Rect>(Id::new("seam-right")));
        let bottom = self.area_rects[1];
        let seams = [
            left.map(|r| (r.right(), r.y_range(), true)),
            right.map(|r| (r.left(), r.y_range(), true)),
            bottom.map(|r| (r.top(), r.x_range(), false)),
        ];
        seam_grips(ctx, seams);
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
        let LyraApp { areas, project: p, history: h, ui: st, samples, recorder, .. } = self;
        let area = &mut areas[i];
        ui.push_id(("area", i), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0; // header, line and editor touch
            let header = egui::Frame::NONE.inner_margin(Margin { left: 8, right: 8, top: 6, bottom: 6 });
            header.show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.horizontal(|ui| {
                    areas::picker(ui, area);
                    ui.add_space(6.0);
                    match area.editor {
                        Editor::Arrangement => area.playlist.toolbar(ui, p, st, actions),
                        Editor::Audio => area.audio.toolbar(ui, p, h, st, recorder),
                        Editor::Automation => area.automation.toolbar(ui, p, h, st),
                        Editor::PianoRoll if has_channel => {
                            let color = theme::channel_color(p.channels[ch].color);
                            area.piano.toolbar(ui, p, h, color, ch);
                        }
                        _ => {}
                    }
                });
            });
            // The header's bottom edge: an engraved line in the ground colour,
            // edge to edge; the editor starts right under it.
            let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), theme::OUTLINE_W), egui::Sense::hover());
            ui.painter().rect_filled(line, theme::cr(0), theme::GROUND);
            // The editor gets the rest of the area exactly (no item spacing above it).
            let rest = ui.available_rect_before_wrap();
            ui.allocate_rect(rest, egui::Sense::hover());
            let ui = &mut ui.new_child(egui::UiBuilder::new().max_rect(rest).layout(*ui.layout()));
            ui.spacing_mut().item_spacing.y = 4.0;
            match area.editor {
                Editor::Arrangement => {
                    let song_playhead = if st.song_mode { playhead } else { 0.0 };
                    let song_playing = playing && st.song_mode;
                    area.playlist.show(ui, p, h, st, song_playhead, song_playing, samples, actions);
                }
                Editor::Audio => {
                    let song_playhead = st.song_mode.then_some(playhead);
                    area.audio.show(ui, p, h, st, samples, song_playhead, actions, recorder);
                }
                Editor::Automation => {
                    let song_playhead = st.song_mode.then_some(playhead);
                    area.automation.show(ui, p, h, st, song_playhead, actions);
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
                    egui::Frame::NONE.inner_margin(Margin::same(8)).show(ui, |ui| {
                        ui.label(RichText::new("Add an instrument in the plugin rack first.").color(theme::TEXT_DIM));
                    });
                }
                Editor::Mixer => {
                    // The mixer's strips sit on the background, like a rack frame.
                    ui.painter().rect_filled(ui.available_rect_before_wrap(), cr(0), theme::BG);
                    egui::ScrollArea::vertical().id_salt("mixer-v").show(ui, |ui| crate::mixer::show(ui, p, h, st));
                }
                Editor::Steps => {
                    let play_step = local.filter(|_| playing).map(|t| (t / TICKS_PER_STEP as f64) as u32);
                    egui::ScrollArea::vertical().id_salt("steps-v").show(ui, |ui| {
                        egui::Frame::NONE
                            .inner_margin(Margin::same(8))
                            .show(ui, |ui| rack::show(ui, p, h, st, play_step, true));
                    });
                }
            }
        });
    }

    /// The floating windows: instrument editor, effect, design panel.
    fn windows(&mut self, ctx: &egui::Context) {
        // Lyra's frame: the panel with an outline in its own colour, lifted
        // off the app by the drawers' shadow; the title row is Lyra's own.
        let frame = egui::Frame::window(&ctx.style())
            .fill(theme::PANEL)
            .stroke(theme::stroke(theme::OUTLINE_W, theme::outline_of(theme::PANEL)))
            .corner_radius(cr(5))
            .shadow(egui::epaint::Shadow { offset: [0, 4], blur: 10, spread: 1, color: egui::Color32::from_black_alpha(90) })
            .inner_margin(Margin::same(8));

        // Default spots (first open only): instrument editor top-right, effect below it.
        let screen = ctx.screen_rect();
        let piano_top = (screen.height() * 0.42).max(300.0);

        let has_channel = !self.project.channels.is_empty();
        let ch = self.ui.selected.min(self.project.channels.len().saturating_sub(1));
        self.ui.selected = ch;

        if self.ui.show_editor && has_channel {
            let mut open = true;
            let live = self.bridge.live(self.project.channels[ch].id);
            let title = format!("Instrument · {}", self.project.channels[ch].name);
            egui::Window::new(&title)
                .id(Id::new("editor"))
                .title_bar(false)
                .pivot(egui::Align2::RIGHT_TOP)
                .default_pos([screen.right() - 16.0, 54.0])
                .resizable(false)
                .frame(frame)
                .show(ctx, |ui| {
                    widgets::window_title(ui, &title, &mut open);
                    editor::show(ui, &mut self.project, &mut self.history, ch, live.clone());
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
            let title = format!("{name} · {}", self.project.mixer[t].name);
            egui::Window::new(&title)
                .id(Id::new("effect"))
                .title_bar(false)
                .pivot(egui::Align2::RIGHT_TOP)
                .default_pos([screen.right() - 16.0, piano_top])
                .resizable(false)
                .frame(frame)
                .show(ctx, |ui| {
                    widgets::window_title(ui, &title, &mut open);
                    crate::mixer::effect_editor(ui, &mut self.project, &mut self.history, t, i)
                });
            if !open {
                self.ui.effect = None;
            }
        }

        if self.ui.show_design {
            let mut open = true;
            egui::Window::new("Design · sizes")
                .id(Id::new("design"))
                .title_bar(false)
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .default_pos([screen.right() - 16.0, screen.bottom() - 40.0])
                .resizable(false)
                .frame(frame)
                .show(ctx, |ui| {
                    widgets::window_title(ui, "Design · sizes", &mut open);
                    // Taller than most screens: it scrolls instead of running off the top.
                    egui::ScrollArea::vertical().max_height(screen.height() - 160.0).show(ui, crate::design::show);
                });
            self.ui.show_design = open;
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let items = self.context_items();
        egui::TopBottomPanel::bottom("status")
            .exact_height(theme::m().text_small + 12.0)
            .frame(egui::Frame::default().fill(theme::TOOLBAR).inner_margin(Margin::symmetric(8, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // Busy (catalog-next, settled): never block, never spin. A
                    // pulsing activity light, the task, a progress track lit to
                    // where it is, and a quiet percentage.
                    if let Some(export) = self.export.as_ref().filter(|e| !e.thread.is_finished()) {
                        let f = export.progress.load(Relaxed) as f32 / 1000.0;
                        let t = ui.input(|i| i.time);
                        let pulse = 0.4 + 0.6 * (0.5 + 0.5 * (t * std::f64::consts::TAU * 1.1).sin()) as f32;
                        let (dot, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot.center(), 5.0, theme::outline_of(theme::L_MINT.mid));
                        ui.painter().circle_filled(dot.center(), 3.5, theme::L_MINT.mid.gamma_multiply(pulse));
                        ui.label(RichText::new("Exporting…").font(theme::read(12.0)).color(theme::INK_DIM));
                        let (track, _) = ui.allocate_exact_size(egui::vec2(220.0, 8.0), egui::Sense::hover());
                        ui.painter().rect_filled(track, theme::cr(4), theme::GROUND);
                        let bed = track.shrink(2.0);
                        ui.painter().rect_filled(bed, theme::cr(2), egui::Color32::from_rgb(0x2b, 0x2b, 0x2b));
                        let lit = egui::Rect::from_min_size(bed.min, egui::vec2(bed.width() * f.clamp(0.0, 1.0), bed.height()));
                        ui.painter().rect_filled(lit, theme::cr(2), theme::L_MINT.mid);
                        ui.label(RichText::new(format!("{:>3.0} %", f * 100.0)).font(egui::FontId::monospace(12.0)).color(theme::INK_DIM));
                        ui.add_space(12.0);
                    }
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
                            .hint(format!("Output device: {}", a.device_name));
                            if late > 0 {
                                ui.label(RichText::new(format!("· {late} underruns")).small().color(theme::ROSE))
                                    .hint(
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
                        // The context bar: what you can do with what's selected (F3 shows the same, at the mouse).
                        for (action, label) in items.iter().rev() {
                            let text = RichText::new(label).small().color(theme::TEXT);
                            if ui.add(egui::Button::new(text).corner_radius(3.0)).clicked() {
                                actions.push(*action);
                            }
                        }
                        if !items.is_empty() {
                            ui.add_space(8.0);
                        }
                        ui.label(
                            RichText::new(format!(
                                "Z-M Q-P play · -/= octave {:+} · [ ] pattern · F4 F5 F6 panels · F7 piano roll · F9 mixer · F3 actions",
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

/// What's being listened to on its own.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Listening {
    Clip(lyra_model::AudioId),
    Tape,
}

/// Where an imported MIDI file goes.
#[derive(Clone, Copy)]
enum Midi {
    /// Every track an instrument and a pattern, placed at `at` from arrangement `track` down.
    Song { at: u32, track: usize },
    /// All its notes into the instrument being edited, from tick `at`.
    Notes { at: u32 },
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

/// The splitter grip (catalog-next, settled): resizing works as egui does it;
/// over a resizable seam (or while dragging it) a small grip plate shows on
/// it, centred along the seam. Each seam: (position, extent along it, vertical).
fn seam_grips(ctx: &egui::Context, seams: impl IntoIterator<Item = Option<(f32, egui::Rangef, bool)>>) {
    let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) else { return };
    let dragging = ctx.input(|i| i.pointer.primary_down());
    let resizing = matches!(
        ctx.output(|o| o.cursor_icon),
        egui::CursorIcon::ResizeHorizontal | egui::CursorIcon::ResizeColumn | egui::CursorIcon::ResizeVertical | egui::CursorIcon::ResizeRow
    );
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("seam-grips")));
    for (at, span, vertical) in seams.into_iter().flatten() {
        let (d, along) = if vertical { (pos.x - at, pos.y) } else { (pos.y - at, pos.x) };
        let near = d.abs() <= 8.0 && span.contains(along);
        if !(near && (resizing || !dragging)) {
            continue;
        }
        let c = if vertical { egui::pos2(at, span.center()) } else { egui::pos2(span.center(), at) };
        let size = if vertical { egui::vec2(8.0, 36.0) } else { egui::vec2(36.0, 8.0) };
        let r = egui::Rect::from_center_size(c, size);
        theme::outlined(&painter, r, 3.0, theme::PLATE);
    }
}

