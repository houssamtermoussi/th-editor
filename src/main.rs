#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod filters;
mod image_doc;

fn main() -> eframe::Result {
    env_logger::init();

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/logo.png"))
        .expect("logo.png invalide");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("HT Editor — Éditeur photo local")
            .with_app_id("ht-editor")
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "HT Editor",
        options,
        Box::new(|cc| Ok(Box::new(app::HtEditorApp::new(cc)))),
    )
}
