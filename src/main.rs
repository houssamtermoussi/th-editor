#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

fn main() -> eframe::Result {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([640.0, 420.0])
            .with_title("HT Editor"),
        ..Default::default()
    };

    eframe::run_native(
        "HT Editor",
        options,
        Box::new(|cc| Ok(Box::new(app::HtEditorApp::new(cc)))),
    )
}
