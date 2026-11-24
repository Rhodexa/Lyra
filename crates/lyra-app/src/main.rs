//! Lyra — a pattern-based DAW that makes you want to play.

mod app;
mod areas;
mod audio;
mod audio_editor;
mod automation;
mod automation_editor;
mod bridge;
mod browser;
mod chrysalis_editor;
mod demo;
mod design;
mod desktop;
mod devtools;
mod editor;
mod files;
mod flex;
mod fm_editor;
mod keys;
mod midi;
mod mixer;
mod piano_roll;
mod playlist;
mod pool;
mod rack;
mod recorder;
mod render;
mod report;
mod samples;
mod spectrogram;
mod theme;
mod timeline;
mod transport;
mod user_presets;
mod waveform;
mod widgets;

// The model and engine must agree on what a "step" is.
const _: () = assert!(lyra_model::TICKS_PER_STEP == lyra_engine::TICKS_PER_STEP);

fn main() -> eframe::Result {
    // `lyra --render song.lyra [out.wav] [--stems dir]`: no window, no sound card.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--render") {
        std::process::exit(render::cli(&args[1..]));
    }
    // `lyra --export-data song.lyra [folder]`: MIDI, notes, chords and mix measurements.
    if args.first().is_some_and(|a| a == "--export-data") {
        std::process::exit(report::cli(&args[1..]));
    }
    // `lyra --install-desktop`: the desktop entry and icon (so Wayland shows Lyra's logo).
    if args.first().is_some_and(|a| a == "--install-desktop") {
        std::process::exit(desktop::cli());
    }
    // Chrysalis's built-in wavetables take a moment to build: start now, off the UI thread.
    lyra_chrysalis::warm_up();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Lyra")
            .with_app_id("lyra")
            .with_inner_size([1280.0, 800.0])
            .with_maximized(true)
            .with_min_inner_size([760.0, 440.0])
            .with_icon(egui::IconData { rgba: theme::ICON_128.to_vec(), width: 128, height: 128 }),
        ..Default::default()
    };
    eframe::run_native("Lyra", options, Box::new(|cc| Ok(Box::new(app::LyraApp::new(cc)))))
}
