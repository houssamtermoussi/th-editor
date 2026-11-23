use std::path::PathBuf;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use image::ImageReader;
use rfd::FileDialog;

use crate::filters::{self, Adjustments};
use crate::image_doc::ImageDoc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    View,
    Crop,
}

/// Crop selection in image pixel coordinates.
#[derive(Debug, Clone, Copy)]
struct CropRect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl CropRect {
    fn normalized(self) -> (u32, u32, u32, u32) {
        let min_x = self.x0.min(self.x1).max(0.0);
        let min_y = self.y0.min(self.y1).max(0.0);
        let max_x = self.x0.max(self.x1);
        let max_y = self.y0.max(self.y1);
        let w = (max_x - min_x).max(1.0);
        let h = (max_y - min_y).max(1.0);
        (min_x as u32, min_y as u32, w as u32, h as u32)
    }

    fn is_valid(&self) -> bool {
        (self.x0 - self.x1).abs() >= 2.0 && (self.y0 - self.y1).abs() >= 2.0
    }
}

pub struct HtEditorApp {
    doc: Option<ImageDoc>,
    tool: Tool,
    status: String,
    zoom: f32,
    /// Pan offset in screen pixels.
    pan: Vec2,
    crop: Option<CropRect>,
    crop_dragging: bool,
}

impl HtEditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.window_fill = Color32::from_rgb(22, 24, 28);
        visuals.panel_fill = Color32::from_rgb(28, 30, 36);
        visuals.extreme_bg_color = Color32::from_rgb(18, 19, 22);
        cc.egui_ctx.set_visuals(visuals);

        Self {
            doc: None,
            tool: Tool::View,
            status: "Ouvrez une image pour commencer".into(),
            zoom: 1.0,
            pan: Vec2::ZERO,
            crop: None,
            crop_dragging: false,
        }
    }

    fn open_image(&mut self) {
        let Some(path) = FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
            .pick_file()
        else {
            return;
        };
        match load_image(&path) {
            Ok(doc) => {
                let (w, h) = (doc.width(), doc.height());
                self.status = format!("{} — {}×{}", path.display(), w, h);
                self.doc = Some(doc);
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
                self.crop = None;
                self.tool = Tool::View;
            }
            Err(e) => self.status = format!("Erreur: {e}"),
        }
    }

    fn save_image(&mut self) {
        let Some(doc) = self.doc.as_mut() else {
            self.status = "Aucune image ouverte".into();
            return;
        };
        // Bake pending adjustments before save.
        if !doc.adjustments.is_identity() {
            doc.commit_preview();
        }
        let default_name = doc
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("edited.png");

        let Some(path) = FileDialog::new()
            .add_filter("PNG", &["png"])
            .add_filter("JPEG", &["jpg", "jpeg"])
            .set_file_name(default_name)
            .save_file()
        else {
            return;
        };

        match save_image(doc.preview(), &path) {
            Ok(()) => {
                doc.path = Some(path.clone());
                doc.dirty = false;
                self.status = format!("Enregistré: {}", path.display());
            }
            Err(e) => self.status = format!("Erreur enregistrement: {e}"),
        }
    }

    fn apply_crop(&mut self) {
        let Some(crop) = self.crop.filter(|c| c.is_valid()) else {
            self.status = "Sélectionnez une zone à recadrer".into();
            return;
        };
        let (x, y, w, h) = crop.normalized();
        if let Some(doc) = self.doc.as_mut() {
            doc.apply_transform(|img| filters::crop(img, x, y, w, h));
            self.status = format!("Recadré {w}×{h}");
        }
        self.crop = None;
        self.tool = Tool::View;
    }

    fn fit_to_view(&mut self, available: Vec2, img_w: f32, img_h: f32) {
        if img_w <= 0.0 || img_h <= 0.0 {
            return;
        }
        let zx = available.x / img_w;
        let zy = available.y / img_h;
        self.zoom = (zx.min(zy) * 0.95).clamp(0.05, 8.0);
        self.pan = Vec2::ZERO;
    }
}

impl eframe::App for HtEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Keyboard shortcuts
        ctx.input(|i| {
            if i.modifiers.command && i.key_pressed(egui::Key::O) {
                // handled after borrow ends
            }
        });
        let open = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O));
        let save = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S));
        let undo = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z) && !i.modifiers.shift);
        let redo = ctx.input(|i| {
            (i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z))
                || (i.modifiers.command && i.key_pressed(egui::Key::Y))
        });
        if open {
            self.open_image();
        }
        if save {
            self.save_image();
        }
        if undo {
            if let Some(doc) = self.doc.as_mut() {
                doc.undo();
                self.status = "Annulé".into();
            }
        }
        if redo {
            if let Some(doc) = self.doc.as_mut() {
                doc.redo();
                self.status = "Refait".into();
            }
        }

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Fichier", |ui| {
                    if ui.button("Ouvrir…        Ctrl+O").clicked() {
                        ui.close();
                        self.open_image();
                    }
                    if ui
                        .add_enabled(self.doc.is_some(), egui::Button::new("Enregistrer…  Ctrl+S"))
                        .clicked()
                    {
                        ui.close();
                        self.save_image();
                    }
                    ui.separator();
                    if ui.button("Quitter").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Édition", |ui| {
                    let can_undo = self.doc.as_ref().is_some_and(|d| d.can_undo());
                    let can_redo = self.doc.as_ref().is_some_and(|d| d.can_redo());
                    if ui
                        .add_enabled(can_undo, egui::Button::new("Annuler        Ctrl+Z"))
                        .clicked()
                    {
                        if let Some(doc) = self.doc.as_mut() {
                            doc.undo();
                            self.status = "Annulé".into();
                        }
                        ui.close();
                    }
                    if ui
                        .add_enabled(can_redo, egui::Button::new("Refaire        Ctrl+Y"))
                        .clicked()
                    {
                        if let Some(doc) = self.doc.as_mut() {
                            doc.redo();
                            self.status = "Refait".into();
                        }
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(self.doc.is_some(), egui::Button::new("Réinitialiser l'original"))
                        .clicked()
                    {
                        if let Some(doc) = self.doc.as_mut() {
                            doc.reset_to_original();
                            self.status = "Image originale restaurée".into();
                        }
                        ui.close();
                    }
                });
                ui.menu_button("Affichage", |ui| {
                    if ui.button("Zoom +").clicked() {
                        self.zoom = (self.zoom * 1.2).min(8.0);
                        ui.close();
                    }
                    if ui.button("Zoom −").clicked() {
                        self.zoom = (self.zoom / 1.2).max(0.05);
                        ui.close();
                    }
                    if ui.button("Zoom 100%").clicked() {
                        self.zoom = 1.0;
                        self.pan = Vec2::ZERO;
                        ui.close();
                    }
                });
            });
        });

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tool, Tool::View, "👁 Vue");
                if ui
                    .selectable_value(&mut self.tool, Tool::Crop, "✂ Recadrer")
                    .clicked()
                {
                    self.crop = None;
                }
                ui.separator();

                let has = self.doc.is_some();
                if ui.add_enabled(has, egui::Button::new("↺ 90°")).clicked() {
                    if let Some(doc) = self.doc.as_mut() {
                        doc.apply_transform(filters::rotate270);
                        self.status = "Rotation −90°".into();
                    }
                }
                if ui.add_enabled(has, egui::Button::new("↻ 90°")).clicked() {
                    if let Some(doc) = self.doc.as_mut() {
                        doc.apply_transform(filters::rotate90);
                        self.status = "Rotation +90°".into();
                    }
                }
                if ui.add_enabled(has, egui::Button::new("⇄ Miroir H")).clicked() {
                    if let Some(doc) = self.doc.as_mut() {
                        doc.apply_transform(filters::flip_horizontal);
                        self.status = "Miroir horizontal".into();
                    }
                }
                if ui.add_enabled(has, egui::Button::new("⇅ Miroir V")).clicked() {
                    if let Some(doc) = self.doc.as_mut() {
                        doc.apply_transform(filters::flip_vertical);
                        self.status = "Miroir vertical".into();
                    }
                }

                if self.tool == Tool::Crop {
                    ui.separator();
                    if ui.button("Appliquer le recadrage").clicked() {
                        self.apply_crop();
                    }
                    if ui.button("Annuler sélection").clicked() {
                        self.crop = None;
                        self.tool = Tool::View;
                    }
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("Zoom {:.0}%", self.zoom * 100.0));
                    if let Some(doc) = &self.doc {
                        ui.label(format!("{}×{}", doc.width(), doc.height()));
                        if doc.dirty {
                            ui.colored_label(Color32::from_rgb(255, 180, 80), "● modifié");
                        }
                    }
                });
            });
        });

        // Side panel — filters & adjustments
        egui::SidePanel::right("filters")
            .default_width(260.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Ajustements");
                ui.add_space(6.0);

                let Some(doc) = self.doc.as_mut() else {
                    ui.label("Aucune image.");
                    return;
                };

                let mut adj = doc.adjustments;
                let mut changed = false;

                ui.label("Luminosité");
                if ui
                    .add(egui::Slider::new(&mut adj.brightness, -100.0..=100.0).suffix("%"))
                    .changed()
                {
                    changed = true;
                }
                ui.label("Contraste");
                if ui
                    .add(egui::Slider::new(&mut adj.contrast, -100.0..=100.0).suffix("%"))
                    .changed()
                {
                    changed = true;
                }
                ui.label("Saturation");
                if ui
                    .add(egui::Slider::new(&mut adj.saturation, -100.0..=100.0).suffix("%"))
                    .changed()
                {
                    changed = true;
                }

                if changed {
                    doc.adjustments = adj;
                    doc.recompute_preview();
                    self.status = "Ajustements en direct…".into();
                }

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!adj.is_identity(), egui::Button::new("Appliquer"))
                        .clicked()
                    {
                        doc.commit_preview();
                        self.status = "Ajustements appliqués".into();
                    }
                    if ui
                        .add_enabled(!adj.is_identity(), egui::Button::new("Réinitialiser"))
                        .clicked()
                    {
                        doc.adjustments = Adjustments::default();
                        doc.recompute_preview();
                        self.status = "Ajustements annulés".into();
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                ui.heading("Filtres");
                ui.add_space(6.0);

                if ui.button("Noir & blanc").clicked() {
                    doc.apply_transform(filters::grayscale);
                    self.status = "Filtre: N&B".into();
                }
                if ui.button("Sépia").clicked() {
                    doc.apply_transform(filters::sepia);
                    self.status = "Filtre: sépia".into();
                }
                if ui.button("Négatif").clicked() {
                    doc.apply_transform(filters::invert);
                    self.status = "Filtre: négatif".into();
                }
                if ui.button("Flou léger").clicked() {
                    doc.apply_transform(|img| filters::blur(img, 2.0));
                    self.status = "Filtre: flou".into();
                }
                if ui.button("Netteté").clicked() {
                    doc.apply_transform(filters::sharpen);
                    self.status = "Filtre: netteté".into();
                }

                ui.add_space(12.0);
                ui.separator();
                ui.heading("Transformations");
                ui.add_space(6.0);
                if ui.button("Rotation 180°").clicked() {
                    doc.apply_transform(filters::rotate180);
                    self.status = "Rotation 180°".into();
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let (response, painter) =
                ui.allocate_painter(available, Sense::click_and_drag());

            // Checkerboard background
            painter.rect_filled(
                response.rect,
                0.0,
                Color32::from_rgb(32, 34, 40),
            );

            let Some(doc) = &self.doc else {
                painter.text(
                    response.rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Fichier → Ouvrir…  (Ctrl+O)",
                    egui::FontId::proportional(18.0),
                    Color32::from_rgb(140, 145, 160),
                );
                return;
            };

            let img_w = doc.width() as f32;
            let img_h = doc.height() as f32;

            // Fit on double-click
            if response.double_clicked() {
                self.fit_to_view(available, img_w, img_h);
            }

            // Scroll zoom
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 && response.hovered() {
                let factor = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
                self.zoom = (self.zoom * factor).clamp(0.05, 8.0);
            }

            let display_size = Vec2::new(img_w * self.zoom, img_h * self.zoom);
            let center = response.rect.center() + self.pan;
            let img_rect = Rect::from_center_size(center, display_size);

            // Pan (middle mouse or left drag in View mode)
            if self.tool == Tool::View {
                if response.dragged_by(egui::PointerButton::Middle)
                    || response.dragged_by(egui::PointerButton::Primary)
                {
                    self.pan += response.drag_delta();
                }
            }

            // Texture needs &mut ImageDoc — take after pan/zoom updates
            let texture_id = {
                let doc = self.doc.as_mut().expect("doc checked above");
                doc.texture(ctx).id()
            };
            painter.image(
                texture_id,
                img_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );

            // Crop interaction
            if self.tool == Tool::Crop {
                if let Some(pos) = response.interact_pointer_pos() {
                    let img_pos = screen_to_image(pos, img_rect, img_w, img_h);
                    if response.drag_started_by(egui::PointerButton::Primary) {
                        self.crop = Some(CropRect {
                            x0: img_pos.x,
                            y0: img_pos.y,
                            x1: img_pos.x,
                            y1: img_pos.y,
                        });
                        self.crop_dragging = true;
                    } else if self.crop_dragging && response.dragged_by(egui::PointerButton::Primary)
                    {
                        if let Some(c) = self.crop.as_mut() {
                            c.x1 = img_pos.x.clamp(0.0, img_w);
                            c.y1 = img_pos.y.clamp(0.0, img_h);
                        }
                    }
                    if response.drag_stopped() {
                        self.crop_dragging = false;
                    }
                }

                if let Some(crop) = self.crop {
                    let (x, y, w, h) = crop.normalized();
                    let screen = image_rect_to_screen(
                        Rect::from_min_size(
                            Pos2::new(x as f32, y as f32),
                            Vec2::new(w as f32, h as f32),
                        ),
                        img_rect,
                        img_w,
                        img_h,
                    );
                    let full = img_rect;
                    let dim = Color32::from_black_alpha(140);
                    painter.rect_filled(
                        Rect::from_min_max(full.min, Pos2::new(full.max.x, screen.min.y)),
                        0.0,
                        dim,
                    );
                    painter.rect_filled(
                        Rect::from_min_max(Pos2::new(full.min.x, screen.max.y), full.max),
                        0.0,
                        dim,
                    );
                    painter.rect_filled(
                        Rect::from_min_max(
                            Pos2::new(full.min.x, screen.min.y),
                            Pos2::new(screen.min.x, screen.max.y),
                        ),
                        0.0,
                        dim,
                    );
                    painter.rect_filled(
                        Rect::from_min_max(
                            Pos2::new(screen.max.x, screen.min.y),
                            Pos2::new(full.max.x, screen.max.y),
                        ),
                        0.0,
                        dim,
                    );
                    painter.rect_stroke(
                        screen,
                        0.0,
                        Stroke::new(2.0_f32, Color32::from_rgb(80, 180, 255)),
                        egui::StrokeKind::Outside,
                    );
                } else {
                    painter.text(
                        response.rect.left_top() + Vec2::new(12.0, 12.0),
                        egui::Align2::LEFT_TOP,
                        "Glissez pour sélectionner la zone à recadrer",
                        egui::FontId::proportional(14.0),
                        Color32::from_rgb(200, 210, 230),
                    );
                }
            }

        });
    }
}

fn screen_to_image(pos: Pos2, img_rect: Rect, img_w: f32, img_h: f32) -> Pos2 {
    let u = ((pos.x - img_rect.min.x) / img_rect.width()).clamp(0.0, 1.0);
    let v = ((pos.y - img_rect.min.y) / img_rect.height()).clamp(0.0, 1.0);
    Pos2::new(u * img_w, v * img_h)
}

fn image_rect_to_screen(r: Rect, img_rect: Rect, img_w: f32, img_h: f32) -> Rect {
    let min = Pos2::new(
        img_rect.min.x + (r.min.x / img_w) * img_rect.width(),
        img_rect.min.y + (r.min.y / img_h) * img_rect.height(),
    );
    let max = Pos2::new(
        img_rect.min.x + (r.max.x / img_w) * img_rect.width(),
        img_rect.min.y + (r.max.y / img_h) * img_rect.height(),
    );
    Rect::from_min_max(min, max)
}

fn load_image(path: &PathBuf) -> Result<ImageDoc, String> {
    let reader = ImageReader::open(path).map_err(|e| e.to_string())?;
    let img = reader
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?
        .to_rgba8();
    Ok(ImageDoc::from_rgba(img, Some(path.clone())))
}

fn save_image(img: &image::RgbaImage, path: &PathBuf) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" => {
            let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
            rgb.save(path).map_err(|e| e.to_string())
        }
        _ => img.save(path).map_err(|e| e.to_string()),
    }
}
