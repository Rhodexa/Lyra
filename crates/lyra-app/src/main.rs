//! Lyra — a pattern-based DAW that makes you want to play.

mod app;
mod audio;
mod bridge;
mod demo;
mod keys;
mod rack;
mod theme;
mod transport;
mod widgets;

// The model and engine must agree on what a "step" is.
const _: () = assert!(lyra_model::TICKS_PER_STEP == lyra_engine::TICKS_PER_STEP);

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Lyra")
            .with_app_id("lyra")
            .with_inner_size([1180.0, 720.0])
            .with_min_inner_size([760.0, 440.0])
            .with_icon(egui::IconData { rgba: theme::ICON_128.to_vec(), width: 128, height: 128 }),
        ..Default::default()
    };
    eframe::run_native("Lyra", options, Box::new(|cc| Ok(Box::new(app::LyraApp::new(cc)))))
}
