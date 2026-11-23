use eframe::egui;

/// Root application state.
pub struct HtEditorApp {
    buffer: String,
    status: String,
}

impl HtEditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Slightly larger UI for an editor feel.
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::monospace(14.0),
        );
        cc.egui_ctx.set_style(style);

        Self {
            buffer: String::from("// Welcome to HT Editor\n\nfn main() {\n    println!(\"hello\");\n}\n"),
            status: String::from("Ready"),
        }
    }
}

impl eframe::App for HtEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        self.buffer.clear();
                        self.status = "New buffer".into();
                        ui.close();
                    }
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Clear").clicked() {
                        self.buffer.clear();
                        self.status = "Cleared".into();
                        ui.close();
                    }
                });
            });
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} chars", self.buffer.len()));
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let response = ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(&mut self.buffer)
                            .code_editor()
                            .desired_width(f32::INFINITY),
                    );
                    if response.changed() {
                        self.status = "Edited".into();
                    }
                });
        });
    }
}
