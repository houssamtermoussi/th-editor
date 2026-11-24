use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Layout, Margin, Mesh, Pos2,
    Rect, RichText, Sense, Shadow, Stroke, StrokeKind, TextureHandle, TextureId, TextureOptions,
    UiBuilder, Vec2, epaint::Vertex,
};
use image::{ImageReader, RgbaImage};
use rfd::FileDialog;

use crate::filters::{self, Adjustments, PhotoFilter};
use crate::image_doc::ImageDoc;
use crate::text;
use crate::theme;

const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    View,
    Crop,
    Draw,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrawMode {
    Brush,
    Eraser,
}

struct TextPlacement {
    content: String,
    /// Top-left of the raster, in image pixels. `None` until the user places it.
    pos: Option<Pos2>,
    size: f32,
    color: Color32,
}

impl Default for TextPlacement {
    fn default() -> Self {
        Self {
            content: String::new(),
            pos: None,
            size: 64.0,
            color: Color32::WHITE,
        }
    }
}

struct JpegExport {
    path: PathBuf,
    quality: u8,
    size_label: String,
    /// Re-encode the size estimate after the quality slider is released.
    dirty: bool,
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

type Transform = fn(&RgbaImage) -> RgbaImage;

/// User intents collected while drawing the UI, applied once the frame's borrows end.
enum Action {
    Open,
    OpenPath(PathBuf),
    Save,
    Quit,
    Undo,
    Redo,
    ResetOriginal,
    SetTool(Tool),
    Transform(Transform, &'static str),
    Filter(PhotoFilter),
    ClearFilter,
    CommitFilter,
    SetAdjustments(Adjustments),
    CommitAdjustments,
    ResetAdjustments,
    ApplyCrop,
    CancelCrop,
    ApplyStraighten,
    ResetStraighten,
    ApplyText,
    ClearText,
    CenterText,
    ToggleCompare,
    ExportJpeg,
    SetJpegQuality { quality: u8, recompute: bool },
    ConfirmJpegExport,
    CancelJpegExport,
    ZoomBy(f32),
    Zoom100,
    Fit,
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
    fit_pending: bool,
    draw_mode: DrawMode,
    brush_color: Color32,
    brush_radius: f32,
    stroke_from: Option<Pos2>,
    text: TextPlacement,
    text_dragging: bool,
    text_drag_last: Option<Pos2>,
    text_raster: Option<RgbaImage>,
    text_texture: Option<TextureHandle>,
    text_key: u64,
    /// Live straighten preview, in degrees. Baked only on Apply.
    straighten: f32,
    compare: bool,
    compare_split: f32,
    compare_drag: bool,
    peek_original: bool,
    jpeg_export: Option<JpegExport>,
    font: ab_glyph::FontArc,
    logo: Option<TextureHandle>,
    checker: TextureHandle,
}

impl HtEditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);

        let mut app = Self {
            doc: None,
            tool: Tool::View,
            status: "Ouvrez une image pour commencer".into(),
            zoom: 1.0,
            pan: Vec2::ZERO,
            crop: None,
            crop_dragging: false,
            fit_pending: false,
            draw_mode: DrawMode::Brush,
            brush_color: Color32::WHITE,
            brush_radius: 18.0,
            stroke_from: None,
            text: TextPlacement::default(),
            text_dragging: false,
            text_drag_last: None,
            text_raster: None,
            text_texture: None,
            text_key: 0,
            straighten: 0.0,
            compare: false,
            compare_split: 0.5,
            compare_drag: false,
            peek_original: false,
            jpeg_export: None,
            font: text::load_font(),
            logo: load_logo(&cc.egui_ctx),
            checker: make_checker(&cc.egui_ctx),
        };
        if let Some(path) = std::env::args_os().nth(1) {
            app.open_path(PathBuf::from(path));
        }
        app
    }

    fn open_image(&mut self) {
        if let Some(path) = FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
            .pick_file()
        {
            self.open_path(path);
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        match load_image(&path) {
            Ok(doc) => {
                self.status = format!("Ouvert : {}", file_name(&path));
                self.doc = Some(doc);
                self.reset_edit_state();
                self.pan = Vec2::ZERO;
                self.fit_pending = true;
            }
            Err(e) => self.status = format!("Erreur : {e}"),
        }
    }

    fn reset_edit_state(&mut self) {
        self.crop = None;
        self.crop_dragging = false;
        self.tool = Tool::View;
        self.stroke_from = None;
        self.text = TextPlacement::default();
        self.text_dragging = false;
        self.text_drag_last = None;
        self.text_raster = None;
        self.text_texture = None;
        self.text_key = 0;
        self.straighten = 0.0;
        self.compare = false;
        self.compare_split = 0.5;
        self.compare_drag = false;
        self.peek_original = false;
        self.jpeg_export = None;
    }

    /// Bake text, straighten and live adjustments so the file matches the screen.
    /// Returns false when a text draft still has no position.
    fn prepare_for_save(&mut self) -> bool {
        self.rebuild_text_raster();
        if !self.text.content.trim().is_empty() && self.text.pos.is_none() {
            self.status = "Placez le texte sur l'image avant d'enregistrer".into();
            return false;
        }
        if self.text.pos.is_some() && !self.text.content.trim().is_empty() {
            self.apply_text();
        }
        if self.straighten.abs() >= 0.05 {
            self.apply_straighten();
        }
        if let Some(doc) = self.doc.as_mut() {
            doc.end_stroke();
            if doc.has_pending() {
                doc.commit_preview();
            }
        }
        true
    }

    fn save_image(&mut self) {
        if self.doc.is_none() {
            self.status = "Aucune image ouverte".into();
            return;
        }
        if !self.prepare_for_save() {
            return;
        }
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
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

        if is_jpeg(&path) {
            self.open_jpeg_dialog(path);
            return;
        }

        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        match doc.preview().save(&path) {
            Ok(()) => {
                self.status = format!("Enregistré : {}", file_name(&path));
                doc.path = Some(path);
                doc.dirty = false;
            }
            Err(e) => self.status = format!("Erreur d'enregistrement : {e}"),
        }
    }

    fn start_jpeg_export(&mut self) {
        if self.doc.is_none() {
            self.status = "Aucune image ouverte".into();
            return;
        }
        if !self.prepare_for_save() {
            return;
        }
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
        let default_name = doc
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|n| n.to_str())
            .map(|n| format!("{n}.jpg"))
            .unwrap_or_else(|| "edited.jpg".into());

        let Some(path) = FileDialog::new()
            .add_filter("JPEG", &["jpg", "jpeg"])
            .set_file_name(default_name)
            .save_file()
        else {
            return;
        };
        let path = if is_jpeg(&path) {
            path
        } else {
            path.with_extension("jpg")
        };
        self.open_jpeg_dialog(path);
    }

    fn open_jpeg_dialog(&mut self, path: PathBuf) {
        self.jpeg_export = Some(JpegExport {
            path,
            quality: 90,
            size_label: "…".into(),
            dirty: true,
        });
        self.status = "Choisissez la qualité JPEG".into();
    }

    fn refresh_jpeg_estimate(&mut self) {
        let quality = match &self.jpeg_export {
            Some(export) if export.dirty => export.quality,
            _ => return,
        };
        let label = if let Some(doc) = self.doc.as_ref() {
            match filters::encode_jpeg(doc.preview(), quality) {
                Ok(bytes) => format_bytes(bytes.len()),
                Err(e) => e,
            }
        } else {
            "Aucune image".into()
        };
        if let Some(export) = &mut self.jpeg_export {
            export.size_label = label;
            export.dirty = false;
        }
    }

    fn confirm_jpeg_export(&mut self) {
        let Some(export) = self.jpeg_export.take() else {
            return;
        };
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        match filters::encode_jpeg(doc.preview(), export.quality) {
            Ok(bytes) => match std::fs::write(&export.path, &bytes) {
                Ok(()) => {
                    self.status = format!(
                        "JPEG {} % ({}) : {}",
                        export.quality,
                        format_bytes(bytes.len()),
                        file_name(&export.path)
                    );
                    doc.path = Some(export.path);
                    doc.dirty = false;
                }
                Err(e) => self.status = format!("Erreur d'enregistrement : {e}"),
            },
            Err(e) => self.status = format!("Erreur JPEG : {e}"),
        }
    }

    fn apply_straighten(&mut self) {
        let angle = self.straighten;
        if angle.abs() < 0.05 {
            self.straighten = 0.0;
            return;
        }
        let baked_text = self.bake_placed_text();
        if let Some(doc) = self.doc.as_mut() {
            doc.apply_transform(|img| filters::rotate_degrees(img, angle));
        }
        self.straighten = 0.0;
        self.fit_pending = true;
        self.status = if baked_text {
            format!("Texte appliqué, redressé de {angle:+.1}°")
        } else {
            format!("Redressé de {angle:+.1}°")
        };
    }

    fn apply_text(&mut self) {
        let Some(pos) = self.text.pos else {
            self.status = "Cliquez sur l'image pour placer le texte".into();
            return;
        };
        if self.text.content.trim().is_empty() {
            self.status = "Saisissez un texte à appliquer".into();
            return;
        }
        self.rebuild_text_raster();
        let Some(raster) = self.text_raster.clone() else {
            self.status = "Impossible de dessiner le texte".into();
            return;
        };
        let (ox, oy) = (pos.x.round() as i32, pos.y.round() as i32);
        if let Some(doc) = self.doc.as_mut() {
            doc.apply_transform(move |img| {
                let mut out = img.clone();
                filters::blit(&mut out, &raster, ox, oy);
                out
            });
            self.status = "Texte appliqué".into();
        }
        self.text.content.clear();
        self.text.pos = None;
        self.text_raster = None;
        self.text_texture = None;
        self.text_key = 0;
    }

    fn center_text(&mut self) {
        self.rebuild_text_raster();
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
        let (tw, th) = self
            .text_raster
            .as_ref()
            .map(|img| (img.width() as f32, img.height() as f32))
            .unwrap_or((0.0, 0.0));
        self.text.pos = Some(Pos2::new(
            (doc.width() as f32 - tw) * 0.5,
            (doc.height() as f32 - th) * 0.5,
        ));
        self.status = "Texte centré — Appliquer pour l'écrire dans l'image".into();
    }

    fn rebuild_text_raster(&mut self) {
        if self.text.content.trim().is_empty() {
            self.text_raster = None;
            self.text_texture = None;
            self.text_key = 0;
            return;
        }
        let key = text_cache_key(&self.text);
        if key == self.text_key && self.text_raster.is_some() {
            return;
        }
        self.text_raster = Some(text::rasterize(
            &self.font,
            &self.text.content,
            self.text.size,
            color_rgba(self.text.color),
        ));
        self.text_texture = None;
        self.text_key = key;
    }

    fn sync_text_texture(&mut self, ctx: &egui::Context) {
        self.rebuild_text_raster();
        if self.text_raster.is_none() {
            self.text_texture = None;
            return;
        }
        if self.text_texture.is_some() {
            return;
        }
        let Some(img) = self.text_raster.as_ref() else {
            return;
        };
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [img.width() as usize, img.height() as usize],
            img.as_raw(),
        );
        self.text_texture = Some(ctx.load_texture("text_stamp", color, TextureOptions::LINEAR));
    }

    fn bake_placed_text(&mut self) -> bool {
        if self.text.pos.is_some() && !self.text.content.trim().is_empty() {
            self.apply_text();
            true
        } else {
            false
        }
    }

    fn apply_crop(&mut self) {
        if self.straighten.abs() >= 0.05 {
            self.status = "Appliquez le redressement avant de recadrer".into();
            return;
        }
        let Some(crop) = self.crop.filter(|c| c.is_valid()) else {
            self.status = "Sélectionnez une zone à recadrer".into();
            return;
        };
        let baked_text = self.bake_placed_text();
        let (x, y, w, h) = crop.normalized();
        if let Some(doc) = self.doc.as_mut() {
            doc.apply_transform(|img| filters::crop(img, x, y, w, h));
            self.status = if baked_text {
                format!("Recadré en {w}×{h} (texte appliqué)")
            } else {
                format!("Recadré en {w}×{h}")
            };
        }
        self.crop = None;
        self.tool = Tool::View;
        self.fit_pending = true;
    }

    fn fit_to_view(&mut self, available: Vec2, img_w: f32, img_h: f32) {
        if img_w <= 0.0 || img_h <= 0.0 {
            return;
        }
        let zx = available.x / img_w;
        let zy = available.y / img_h;
        self.zoom = (zx.min(zy) * 0.9).clamp(MIN_ZOOM, MAX_ZOOM);
        self.pan = Vec2::ZERO;
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Open => self.open_image(),
            Action::OpenPath(path) => self.open_path(path),
            Action::Save => self.save_image(),
            Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::Undo => {
                self.stroke_from = None;
                if let Some(doc) = self.doc.as_mut().filter(|d| d.can_undo()) {
                    doc.undo();
                    self.status = "Annulé".into();
                }
                if let Some(export) = &mut self.jpeg_export {
                    export.dirty = true;
                }
            }
            Action::Redo => {
                self.stroke_from = None;
                if let Some(doc) = self.doc.as_mut().filter(|d| d.can_redo()) {
                    doc.redo();
                    self.status = "Rétabli".into();
                }
                if let Some(export) = &mut self.jpeg_export {
                    export.dirty = true;
                }
            }
            Action::ResetOriginal => {
                self.stroke_from = None;
                self.straighten = 0.0;
                self.crop = None;
                self.text = TextPlacement::default();
                self.text_raster = None;
                self.text_texture = None;
                self.text_key = 0;
                if let Some(doc) = self.doc.as_mut() {
                    doc.reset_to_original();
                    self.status = "Image originale restaurée".into();
                    self.fit_pending = true;
                }
            }
            Action::SetTool(tool) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.end_stroke();
                }
                self.stroke_from = None;
                if self.doc.is_some() {
                    self.tool = tool;
                    self.crop = None;
                    self.crop_dragging = false;
                    self.status = match tool {
                        Tool::View => "Déplacer et zoomer".into(),
                        Tool::Crop => "Glissez pour sélectionner la zone à recadrer".into(),
                        Tool::Draw => {
                            "Pinceau : glissez pour dessiner, Ctrl+Z annule le trait".into()
                        }
                        Tool::Text => "Cliquez sur l'image pour placer le texte".into(),
                    };
                }
            }
            Action::Transform(f, label) => {
                self.stroke_from = None;
                let baked_text = self.bake_placed_text();
                if self.straighten.abs() >= 0.05 {
                    self.apply_straighten();
                }
                if let Some(doc) = self.doc.as_mut() {
                    doc.apply_transform(f);
                    self.status = if baked_text {
                        format!("{label} (texte appliqué)")
                    } else {
                        label.into()
                    };
                    self.fit_pending = true;
                }
            }
            Action::Filter(filter) => {
                if let Some(doc) = self.doc.as_mut() {
                    // Reversible preview: clicking the active filter again removes it.
                    if doc.active_filter == Some(filter) {
                        doc.set_filter(None);
                        self.status = "Filtre retiré".into();
                    } else {
                        doc.set_filter(Some(filter));
                        self.status = format!(
                            "Aperçu : {} — recliquez pour revenir en arrière",
                            filter.label()
                        );
                    }
                }
            }
            Action::ClearFilter => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.set_filter(None);
                    self.status = "Filtre retiré".into();
                }
            }
            Action::CommitFilter => {
                if let Some(doc) = self.doc.as_mut() {
                    let label = doc.active_filter.map(|f| f.label());
                    doc.commit_preview();
                    self.status = match label {
                        Some(label) => {
                            format!("Filtre appliqué : {label} (Ctrl+Z pour annuler)")
                        }
                        None => "Aucun filtre à appliquer".into(),
                    };
                }
            }
            Action::SetAdjustments(adj) => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.adjustments = adj;
                    doc.recompute_preview();
                    self.status = "Ajustements en direct…".into();
                }
            }
            Action::CommitAdjustments => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.commit_preview();
                    self.status = "Ajustements appliqués".into();
                }
            }
            Action::ResetAdjustments => {
                if let Some(doc) = self.doc.as_mut() {
                    doc.adjustments = Adjustments::default();
                    doc.recompute_preview();
                    self.status = "Ajustements annulés".into();
                }
            }
            Action::ApplyCrop => self.apply_crop(),
            Action::CancelCrop => {
                self.crop = None;
                self.tool = Tool::View;
            }
            Action::ApplyStraighten => self.apply_straighten(),
            Action::ResetStraighten => {
                self.straighten = 0.0;
                self.status = "Redressement annulé".into();
            }
            Action::ApplyText => self.apply_text(),
            Action::ClearText => {
                self.text.content.clear();
                self.text.pos = None;
                self.text_raster = None;
                self.text_texture = None;
                self.text_key = 0;
                self.status = "Texte effacé".into();
            }
            Action::CenterText => self.center_text(),
            Action::ToggleCompare => {
                if self.doc.is_some() {
                    self.compare = !self.compare;
                    self.compare_drag = false;
                    self.status = if self.compare {
                        "Comparaison : glissez la barre. Maintenez \\ pour l'original.".into()
                    } else {
                        "Comparaison fermée".into()
                    };
                }
            }
            Action::ExportJpeg => self.start_jpeg_export(),
            Action::SetJpegQuality { quality, recompute } => {
                if let Some(export) = &mut self.jpeg_export {
                    export.quality = quality.clamp(1, 100);
                    export.dirty = recompute;
                }
            }
            Action::ConfirmJpegExport => self.confirm_jpeg_export(),
            Action::CancelJpegExport => {
                if self.jpeg_export.take().is_some() {
                    self.status = "Export annulé".into();
                }
            }
            Action::ZoomBy(factor) => self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM),
            Action::Zoom100 => {
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
            Action::Fit => self.fit_pending = true,
        }
    }

    fn shortcuts(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let typing = ctx.wants_keyboard_input();
        ctx.input(|i| {
            let cmd = i.modifiers.command;
            if self.jpeg_export.is_some() && !typing {
                if i.key_pressed(egui::Key::Escape) {
                    actions.push(Action::CancelJpegExport);
                }
                if i.key_pressed(egui::Key::Enter) {
                    actions.push(Action::ConfirmJpegExport);
                }
            }
            if cmd && i.key_pressed(egui::Key::O) {
                actions.push(Action::Open);
            }
            if cmd && i.key_pressed(egui::Key::S) {
                actions.push(Action::Save);
            }
            if cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z) {
                actions.push(Action::Undo);
            }
            if (cmd && i.modifiers.shift && i.key_pressed(egui::Key::Z))
                || (cmd && i.key_pressed(egui::Key::Y))
            {
                actions.push(Action::Redo);
            }
            if cmd && i.key_pressed(egui::Key::Num0) {
                actions.push(Action::Fit);
            }
            if !typing && !cmd && self.jpeg_export.is_none() {
                if i.key_pressed(egui::Key::V) {
                    actions.push(Action::SetTool(Tool::View));
                }
                if i.key_pressed(egui::Key::C) {
                    actions.push(Action::SetTool(Tool::Crop));
                }
                if i.key_pressed(egui::Key::B) {
                    actions.push(Action::SetTool(Tool::Draw));
                }
                if i.key_pressed(egui::Key::T) {
                    actions.push(Action::SetTool(Tool::Text));
                }
                if self.straighten.abs() >= 0.05 {
                    if i.key_pressed(egui::Key::Enter) {
                        actions.push(Action::ApplyStraighten);
                    }
                    if i.key_pressed(egui::Key::Escape) {
                        actions.push(Action::ResetStraighten);
                    }
                } else if self.tool == Tool::Crop {
                    if i.key_pressed(egui::Key::Enter) {
                        actions.push(Action::ApplyCrop);
                    }
                    if i.key_pressed(egui::Key::Escape) {
                        actions.push(Action::CancelCrop);
                    }
                }
            }
            if let Some(path) = i.raw.dropped_files.iter().find_map(|f| f.path.clone()) {
                actions.push(Action::OpenPath(path));
            }
        });
    }

    fn header(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let has_doc = self.doc.is_some();
        let can_undo = self.doc.as_ref().is_some_and(|d| d.can_undo());
        let can_redo = self.doc.as_ref().is_some_and(|d| d.can_redo());

        egui::TopBottomPanel::top("header")
            .exact_height(48.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_PANEL)
                    .inner_margin(Margin::symmetric(12, 0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(logo) = &self.logo {
                        ui.add(egui::Image::new((logo.id(), Vec2::splat(24.0))).corner_radius(5));
                    }
                    ui.label(
                        RichText::new("HT Editor")
                            .size(15.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.add_space(6.0);

                    // MenuBar claims all remaining width, so give it a bounded slot.
                    let menu_size = Vec2::new(180.0, ui.available_height());
                    ui.allocate_ui_with_layout(
                        menu_size,
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            egui::MenuBar::new().ui(ui, |ui| {
                                self.menus(ui, has_doc, can_undo, can_redo, actions)
                            });
                        },
                    );

                    vertical_divider(ui);

                    segmented(ui, |ui| {
                        let tools = [
                            (Tool::View, "Main", "Déplacer et zoomer (V)"),
                            (Tool::Crop, "Recadrer", "Recadrer l'image (C)"),
                            (Tool::Draw, "Dessin", "Pinceau et gomme (B)"),
                            (Tool::Text, "Texte", "Écrire sur l'image (T)"),
                        ];
                        for (tool, label, hint) in tools {
                            let selected = self.tool == tool;
                            let button = egui::Button::selectable(selected, label)
                                .min_size(Vec2::new(64.0, 26.0));
                            if ui
                                .add_enabled(has_doc, button)
                                .on_hover_text(hint)
                                .clicked()
                            {
                                actions.push(Action::SetTool(tool));
                            }
                        }
                    });

                    let compare =
                        egui::Button::selectable(self.compare, "◐").min_size(Vec2::new(34.0, 26.0));
                    if ui
                        .add_enabled(has_doc, compare)
                        .on_hover_text("Avant / après — glissez la barre, ou maintenez \\")
                        .clicked()
                    {
                        actions.push(Action::ToggleCompare);
                    }

                    vertical_divider(ui);

                    if icon_button(ui, "↩", "Annuler (Ctrl+Z)", can_undo).clicked() {
                        actions.push(Action::Undo);
                    }
                    if icon_button(ui, "↪", "Rétablir (Ctrl+Y)", can_redo).clicked() {
                        actions.push(Action::Redo);
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let save = theme::primary_button("💾  Enregistrer");
                        if ui
                            .add_enabled(has_doc, save)
                            .on_hover_text("Ctrl+S")
                            .clicked()
                        {
                            actions.push(Action::Save);
                        }
                        if ui.button("Ouvrir…").on_hover_text("Ctrl+O").clicked() {
                            actions.push(Action::Open);
                        }
                    });
                });
            });
    }

    fn menus(
        &self,
        ui: &mut egui::Ui,
        has_doc: bool,
        can_undo: bool,
        can_redo: bool,
        actions: &mut Vec<Action>,
    ) {
        ui.menu_button("Fichier", |ui| {
            ui.set_min_width(220.0);
            if ui
                .add(egui::Button::new("Ouvrir…").shortcut_text("Ctrl+O"))
                .clicked()
            {
                actions.push(Action::Open);
            }
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Enregistrer…").shortcut_text("Ctrl+S"),
                )
                .clicked()
            {
                actions.push(Action::Save);
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("Exporter JPEG…"))
                .clicked()
            {
                actions.push(Action::ExportJpeg);
            }
            ui.separator();
            if ui.button("Quitter").clicked() {
                actions.push(Action::Quit);
            }
        });
        ui.menu_button("Édition", |ui| {
            ui.set_min_width(220.0);
            if ui
                .add_enabled(
                    can_undo,
                    egui::Button::new("Annuler").shortcut_text("Ctrl+Z"),
                )
                .clicked()
            {
                actions.push(Action::Undo);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new("Rétablir").shortcut_text("Ctrl+Y"),
                )
                .clicked()
            {
                actions.push(Action::Redo);
            }
            ui.separator();
            if ui
                .add_enabled(has_doc, egui::Button::new("Restaurer l'original"))
                .clicked()
            {
                actions.push(Action::ResetOriginal);
            }
            ui.separator();
            if ui
                .add_enabled(
                    has_doc,
                    egui::Button::new("Écrire du texte").shortcut_text("T"),
                )
                .clicked()
            {
                actions.push(Action::SetTool(Tool::Text));
            }
        });
        ui.menu_button("Affichage", |ui| {
            ui.set_min_width(220.0);
            ui.add_enabled_ui(has_doc, |ui| {
                if ui.button("Zoom avant").clicked() {
                    actions.push(Action::ZoomBy(1.2));
                }
                if ui.button("Zoom arrière").clicked() {
                    actions.push(Action::ZoomBy(1.0 / 1.2));
                }
                if ui.button("Taille réelle (100 %)").clicked() {
                    actions.push(Action::Zoom100);
                }
                if ui
                    .add(egui::Button::new("Ajuster à la fenêtre").shortcut_text("Ctrl+0"))
                    .clicked()
                {
                    actions.push(Action::Fit);
                }
                ui.separator();
                if ui.button("Avant / après").clicked() {
                    actions.push(Action::ToggleCompare);
                }
            });
        });
    }

    fn status_bar(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(30.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_PANEL)
                    .inner_margin(Margin::symmetric(12, 0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let dirty = self.doc.as_ref().is_some_and(|d| d.dirty);
                    let dot = if dirty {
                        theme::WARNING
                    } else {
                        theme::SUCCESS
                    };
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                    ui.painter().circle_filled(rect.center(), 3.5, dot);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&self.status).small().color(theme::TEXT_DIM),
                        )
                        .truncate(),
                    );

                    let Some(doc) = &self.doc else { return };
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if small_button(ui, "Ajuster", "Ajuster à la fenêtre (Ctrl+0)").clicked()
                        {
                            actions.push(Action::Fit);
                        }
                        if small_button(ui, "+", "Zoom avant").clicked() {
                            actions.push(Action::ZoomBy(1.2));
                        }
                        let zoom = format!("{:.0} %", self.zoom * 100.0);
                        if small_button(ui, &zoom, "Taille réelle").clicked() {
                            actions.push(Action::Zoom100);
                        }
                        if small_button(ui, "−", "Zoom arrière").clicked() {
                            actions.push(Action::ZoomBy(1.0 / 1.2));
                        }

                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(format!("{} × {} px", doc.width(), doc.height()))
                                .small()
                                .color(theme::TEXT_DIM),
                        );
                        if dirty {
                            ui.add_space(8.0);
                            badge(ui, "Non enregistré", theme::WARNING);
                        }
                    });
                });
            });
    }

    fn inspector(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        egui::SidePanel::right("inspector")
            .default_width(300.0)
            .width_range(260.0..=440.0)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_PANEL)
                    .inner_margin(Margin::same(12)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.doc.is_none() {
                            theme::card().show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                theme::section_title(ui, "Réglages", "");
                                ui.label(
                                    RichText::new(
                                        "Ouvrez une image pour accéder aux ajustements, filtres et transformations.",
                                    )
                                    .color(theme::TEXT_DIM),
                                );
                            });
                            return;
                        }

                        if let Some(doc) = self.doc.as_ref() {
                            file_card(ui, doc);
                        }
                        ui.add_space(10.0);
                        text_card(
                            ui,
                            &mut self.text,
                            self.tool == Tool::Text,
                            self.straighten.abs() >= 0.05,
                            actions,
                        );
                        ui.add_space(10.0);
                        if self.tool == Tool::Draw {
                            draw_card(
                                ui,
                                &mut self.draw_mode,
                                &mut self.brush_color,
                                &mut self.brush_radius,
                            );
                            ui.add_space(10.0);
                        }
                        if let Some(doc) = self.doc.as_ref() {
                            adjustments_card(ui, doc, actions);
                        }
                        ui.add_space(10.0);
                        if let Some(doc) = self.doc.as_mut() {
                            filters_card(ui, ctx, doc, actions);
                        }
                        ui.add_space(10.0);
                        transforms_card(ui, &mut self.straighten, actions);
                    });
            });
    }

    fn canvas(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG_CANVAS))
            .show(ctx, |ui| {
                let available = ui.available_size();
                let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());
                let canvas = response.rect;

                if self.doc.is_none() {
                    self.empty_state(ui, canvas, actions);
                    drop_overlay(ui, canvas);
                    return;
                }

                self.sync_text_texture(ctx);

                let (img_w, img_h, orig_w, orig_h) = {
                    let doc = self.doc.as_ref().expect("doc checked");
                    let (ow, oh) = doc.original_size();
                    (
                        doc.width() as f32,
                        doc.height() as f32,
                        ow as f32,
                        oh as f32,
                    )
                };

                if self.fit_pending {
                    self.fit_pending = false;
                    self.fit_to_view(available, img_w, img_h);
                }
                if response.double_clicked()
                    && self.tool == Tool::View
                    && self.straighten.abs() < 0.05
                {
                    self.fit_to_view(available, img_w, img_h);
                }

                // Zoom towards the pointer so the pixel under it stays put.
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0
                    && let Some(pointer) = response.hover_pos()
                {
                    let factor = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
                    let new_zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
                    let center = canvas.center() + self.pan;
                    let rel = (pointer - center) / self.zoom;
                    self.pan = pointer - rel * new_zoom - canvas.center();
                    self.zoom = new_zoom;
                }

                let display_size = Vec2::new(img_w * self.zoom, img_h * self.zoom);
                let mut img_rect = Rect::from_center_size(canvas.center() + self.pan, display_size);
                let compare = self.compare && !self.peek_original;
                let straighten_locked = self.straighten.abs() >= 0.05;
                if straighten_locked {
                    self.crop = None;
                    self.crop_dragging = false;
                }
                let on_handle = compare
                    && response
                        .interact_pointer_pos()
                        .or(response.hover_pos())
                        .is_some_and(|p| split_hit(img_rect, self.compare_split).contains(p));

                if compare && response.drag_started_by(egui::PointerButton::Primary) && on_handle {
                    self.compare_drag = true;
                }
                if self.compare_drag {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        let t = (pointer.x - img_rect.min.x) / img_rect.width().max(1.0);
                        self.compare_split = t.clamp(0.02, 0.98);
                    }
                } else if response.dragged_by(egui::PointerButton::Middle)
                    || (self.tool == Tool::View
                        && !self.peek_original
                        && response.dragged_by(egui::PointerButton::Primary))
                {
                    self.pan += response.drag_delta();
                    img_rect = Rect::from_center_size(canvas.center() + self.pan, display_size);
                }

                if !self.compare_drag && !self.peek_original && self.tool == Tool::Draw {
                    if straighten_locked {
                        if response.drag_started_by(egui::PointerButton::Primary) {
                            self.status = "Appliquez le redressement avant de dessiner".into();
                        }
                    } else {
                        let radius = self.brush_radius;
                        let color = color_rgba(self.brush_color);
                        let erase = self.draw_mode == DrawMode::Eraser;
                        let doc = self.doc.as_mut().expect("doc checked");
                        draw_interaction(
                            &mut self.stroke_from,
                            doc,
                            &response,
                            img_rect,
                            img_w,
                            img_h,
                            radius,
                            color,
                            erase,
                        );
                    }
                }

                if !self.compare_drag && !self.peek_original && self.tool == Tool::Text {
                    if straighten_locked {
                        if response.drag_started_by(egui::PointerButton::Primary) {
                            self.status =
                                "Appliquez le redressement avant de placer le texte".into();
                        }
                    } else {
                        let raster = self
                            .text_raster
                            .as_ref()
                            .map(|img| Vec2::new(img.width() as f32, img.height() as f32));
                        text_interaction(
                            &mut self.text,
                            &mut self.text_dragging,
                            &mut self.text_drag_last,
                            &response,
                            img_rect,
                            img_w,
                            img_h,
                            raster,
                        );
                    }
                }

                if response.drag_stopped() {
                    self.compare_drag = false;
                }

                let (texture_id, orig_id) = {
                    let doc = self.doc.as_mut().expect("doc checked");
                    let texture_id = doc.texture(ctx).id();
                    let orig_id = doc.original_texture(ctx).id();
                    (texture_id, orig_id)
                };
                let checker_id = self.checker.id();
                let angle = self.straighten;
                let text_tex = self.text_texture.as_ref().map(|tex| tex.id());
                let text_pos = self.text.pos;
                let text_px = self
                    .text_raster
                    .as_ref()
                    .map(|img| Vec2::new(img.width() as f32, img.height() as f32));
                let pivot = img_rect.center();
                let photo_uv = full_uv();
                let checker_uv = Rect::from_min_max(
                    Pos2::ZERO,
                    Pos2::new(display_size.x / 16.0, display_size.y / 16.0),
                );

                if angle.abs() < 0.05 && !self.peek_original {
                    let shadow = Shadow {
                        offset: [0, 10],
                        blur: 32,
                        spread: 2,
                        color: Color32::from_black_alpha(150),
                    };
                    painter.add(shadow.as_shape(img_rect, CornerRadius::ZERO));
                }

                if self.peek_original {
                    let fitted = contain_rect(img_rect, orig_w, orig_h);
                    painter.image(checker_id, fitted, checker_uv, Color32::WHITE);
                    painter.image(orig_id, fitted, photo_uv, Color32::WHITE);
                    paint_caption(
                        &painter,
                        fitted.left_top() + Vec2::new(12.0, 10.0),
                        "Original",
                    );
                } else if compare {
                    paint_textured_quad(&painter, checker_id, img_rect, checker_uv, pivot, 0.0);
                    let split_x = img_rect.min.x + img_rect.width() * self.compare_split;
                    let left = Rect::from_min_max(img_rect.min, Pos2::new(split_x, img_rect.max.y));
                    let right =
                        Rect::from_min_max(Pos2::new(split_x, img_rect.min.y), img_rect.max);
                    let fitted = contain_rect(img_rect, orig_w, orig_h);
                    painter
                        .with_clip_rect(left)
                        .image(orig_id, fitted, photo_uv, Color32::WHITE);
                    let after = painter.with_clip_rect(right);
                    paint_textured_quad(&after, texture_id, img_rect, photo_uv, pivot, angle);
                    paint_text_overlay(
                        &after, text_tex, text_pos, text_px, img_rect, img_w, img_h, pivot, angle,
                    );
                    painter.line_segment(
                        [
                            Pos2::new(split_x, img_rect.min.y),
                            Pos2::new(split_x, img_rect.max.y),
                        ],
                        Stroke::new(2.0_f32, Color32::WHITE),
                    );
                    let knob = Rect::from_center_size(
                        Pos2::new(split_x, img_rect.center().y),
                        Vec2::splat(28.0),
                    );
                    painter.rect_filled(knob, 8.0, theme::ACCENT);
                    painter.text(
                        knob.center(),
                        Align2::CENTER_CENTER,
                        "↔",
                        FontId::proportional(14.0),
                        Color32::WHITE,
                    );
                    paint_caption(
                        &painter,
                        img_rect.left_top() + Vec2::new(12.0, 10.0),
                        "Avant",
                    );
                    paint_caption(
                        &painter,
                        Pos2::new(split_x + 12.0, img_rect.min.y + 10.0),
                        "Après",
                    );
                } else {
                    paint_textured_quad(&painter, checker_id, img_rect, checker_uv, pivot, angle);
                    paint_textured_quad(&painter, texture_id, img_rect, photo_uv, pivot, angle);
                    paint_text_overlay(
                        &painter, text_tex, text_pos, text_px, img_rect, img_w, img_h, pivot, angle,
                    );
                    if angle.abs() < 0.05 {
                        painter.rect_stroke(
                            img_rect,
                            0.0,
                            Stroke::new(1.0_f32, Color32::from_white_alpha(18)),
                            StrokeKind::Outside,
                        );
                    }
                }

                if response.hovered() {
                    let icon = if self.compare_drag || on_handle {
                        CursorIcon::ResizeHorizontal
                    } else {
                        match self.tool {
                            Tool::Crop if !straighten_locked => CursorIcon::Crosshair,
                            Tool::Draw if !straighten_locked && !self.peek_original => {
                                CursorIcon::None
                            }
                            Tool::View if response.dragged() => CursorIcon::Grabbing,
                            Tool::View => CursorIcon::Grab,
                            Tool::Text => CursorIcon::PointingHand,
                            _ => CursorIcon::Default,
                        }
                    };
                    ctx.set_cursor_icon(icon);
                }

                if self.tool == Tool::Draw && !straighten_locked && !self.peek_original {
                    if let Some(pos) = response.hover_pos().filter(|pos| img_rect.contains(*pos)) {
                        let radius = self.brush_radius * self.zoom;
                        let stroke = if self.draw_mode == DrawMode::Eraser {
                            theme::WARNING
                        } else {
                            Color32::WHITE
                        };
                        painter.circle_stroke(pos, radius, Stroke::new(1.5_f32, stroke));
                    }
                }

                if straighten_locked {
                    self.straighten_bar(ctx, canvas, actions);
                } else if self.tool == Tool::Crop {
                    self.crop_interaction(&response, img_rect, img_w, img_h);
                    if let Some(crop) = self.crop {
                        paint_crop(&painter, crop, img_rect, img_w, img_h);
                    }
                    self.crop_bar(ctx, canvas, actions);
                } else if self.tool == Tool::Text {
                    self.text_bar(ctx, canvas, actions);
                }

                drop_overlay(ui, canvas);
            });
    }

    fn crop_interaction(
        &mut self,
        response: &egui::Response,
        img_rect: Rect,
        img_w: f32,
        img_h: f32,
    ) {
        let Some(pos) = response.interact_pointer_pos() else {
            return;
        };
        let img_pos = screen_to_image(pos, img_rect, img_w, img_h);
        if response.drag_started_by(egui::PointerButton::Primary) {
            self.crop = Some(CropRect {
                x0: img_pos.x,
                y0: img_pos.y,
                x1: img_pos.x,
                y1: img_pos.y,
            });
            self.crop_dragging = true;
        } else if self.crop_dragging && response.dragged_by(egui::PointerButton::Primary) {
            if let Some(c) = self.crop.as_mut() {
                c.x1 = img_pos.x.clamp(0.0, img_w);
                c.y1 = img_pos.y.clamp(0.0, img_h);
            }
        }
        if response.drag_stopped() {
            self.crop_dragging = false;
        }
    }

    fn crop_bar(&self, ctx: &egui::Context, canvas: Rect, actions: &mut Vec<Action>) {
        egui::Area::new(egui::Id::new("crop_bar"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(canvas.center_top() + Vec2::new(0.0, 14.0))
            .show(ctx, |ui| {
                floating_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let valid = self.crop.filter(|c| c.is_valid());
                        let text = match valid {
                            Some(c) => {
                                let (_, _, w, h) = c.normalized();
                                format!("✂  Sélection {w} × {h} px")
                            }
                            None => "✂  Glissez sur l'image pour sélectionner la zone".into(),
                        };
                        ui.label(RichText::new(text).color(theme::TEXT));
                        ui.add_space(6.0);
                        if ui.button("Annuler").on_hover_text("Échap").clicked() {
                            actions.push(Action::CancelCrop);
                        }
                        if ui
                            .add_enabled(valid.is_some(), theme::primary_button("Appliquer"))
                            .on_hover_text("Entrée")
                            .clicked()
                        {
                            actions.push(Action::ApplyCrop);
                        }
                    });
                });
            });
    }

    fn straighten_bar(&self, ctx: &egui::Context, canvas: Rect, actions: &mut Vec<Action>) {
        egui::Area::new(egui::Id::new("straighten_bar"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(canvas.center_top() + Vec2::new(0.0, 14.0))
            .show(ctx, |ui| {
                floating_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("↻  Redressement {:+.1}°", self.straighten))
                                .color(theme::TEXT),
                        );
                        ui.add_space(6.0);
                        if ui.button("Annuler").on_hover_text("Échap").clicked() {
                            actions.push(Action::ResetStraighten);
                        }
                        if ui
                            .add(theme::primary_button("Appliquer"))
                            .on_hover_text("Entrée")
                            .clicked()
                        {
                            actions.push(Action::ApplyStraighten);
                        }
                    });
                });
            });
    }

    fn text_bar(&self, ctx: &egui::Context, canvas: Rect, actions: &mut Vec<Action>) {
        let ready = self.text.pos.is_some() && !self.text.content.trim().is_empty();
        egui::Area::new(egui::Id::new("text_bar"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(canvas.center_top() + Vec2::new(0.0, 14.0))
            .show(ctx, |ui| {
                floating_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let hint = if self.text.content.trim().is_empty() {
                            "T  Saisissez le texte dans le panneau"
                        } else if self.text.pos.is_none() {
                            "T  Cliquez sur l'image pour placer le texte"
                        } else {
                            "T  Glissez le texte, puis appliquez"
                        };
                        ui.label(RichText::new(hint).color(theme::TEXT));
                        ui.add_space(6.0);
                        if ui.button("Centrer").clicked() {
                            actions.push(Action::CenterText);
                        }
                        if ui.button("Effacer").clicked() {
                            actions.push(Action::ClearText);
                        }
                        if ui
                            .add_enabled(ready, theme::primary_button("Appliquer"))
                            .clicked()
                        {
                            actions.push(Action::ApplyText);
                        }
                    });
                });
            });
    }

    fn jpeg_window(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some(export) = self.jpeg_export.as_ref() else {
            return;
        };
        let mut open = true;
        let quality = export.quality;
        let size_label = export.size_label.clone();
        let path_label = file_name(&export.path);
        egui::Window::new("Exporter en JPEG")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(300.0);
                ui.label(RichText::new(path_label).strong().color(theme::TEXT));
                ui.add_space(4.0);
                ui.label(RichText::new("Qualité").color(theme::TEXT));
                let mut q = quality;
                let slider = ui.add(egui::Slider::new(&mut q, 1..=100).suffix(" %"));
                if slider.drag_stopped() {
                    actions.push(Action::SetJpegQuality {
                        quality: q,
                        recompute: true,
                    });
                } else if slider.changed() {
                    actions.push(Action::SetJpegQuality {
                        quality: q,
                        recompute: false,
                    });
                }
                ui.label(
                    RichText::new(format!("Taille estimée : {size_label}"))
                        .small()
                        .color(theme::TEXT_DIM),
                );
                ui.label(
                    RichText::new("Les pixels transparents sont aplatis sur du blanc.")
                        .small()
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Annuler").on_hover_text("Échap").clicked() {
                        actions.push(Action::CancelJpegExport);
                    }
                    if ui
                        .add(theme::primary_button("Enregistrer"))
                        .on_hover_text("Entrée")
                        .clicked()
                    {
                        actions.push(Action::ConfirmJpegExport);
                    }
                });
            });
        if !open {
            actions.push(Action::CancelJpegExport);
        }
    }

    fn empty_state(&self, ui: &mut egui::Ui, canvas: Rect, actions: &mut Vec<Action>) {
        let card = Rect::from_center_size(canvas.center(), Vec2::new(380.0, 270.0));
        ui.painter().rect(
            card,
            CornerRadius::same(16),
            theme::BG_PANEL,
            Stroke::new(1.0_f32, theme::BORDER),
            StrokeKind::Inside,
        );
        let builder = UiBuilder::new()
            .max_rect(card.shrink(24.0))
            .layout(Layout::top_down(Align::Center));
        ui.scope_builder(builder, |ui| {
            ui.add_space(8.0);
            if let Some(logo) = &self.logo {
                ui.add(egui::Image::new((logo.id(), Vec2::splat(72.0))).corner_radius(14));
            }
            ui.add_space(10.0);
            ui.label(
                RichText::new("Aucune image ouverte")
                    .size(18.0)
                    .strong()
                    .color(theme::TEXT),
            );
            ui.label(RichText::new("Glissez-déposez une image ici, ou").color(theme::TEXT_DIM));
            ui.add_space(8.0);
            let open = theme::primary_button("Ouvrir une image").min_size(Vec2::new(200.0, 36.0));
            if ui.add(open).clicked() {
                actions.push(Action::Open);
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new("PNG · JPEG · GIF · BMP · WebP   —   Ctrl+O")
                    .small()
                    .color(theme::TEXT_DIM),
            );
        });
    }
}

impl eframe::App for HtEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let typing = ctx.wants_keyboard_input();
        self.peek_original = !typing && ctx.input(|i| i.key_down(egui::Key::Backslash));
        self.refresh_jpeg_estimate();
        let mut actions = Vec::new();
        self.shortcuts(ctx, &mut actions);
        self.header(ctx, &mut actions);
        self.status_bar(ctx, &mut actions);
        self.inspector(ctx, &mut actions);
        self.canvas(ctx, &mut actions);
        self.jpeg_window(ctx, &mut actions);
        for action in actions {
            self.apply(ctx, action);
        }
    }
}

fn file_card(ui: &mut egui::Ui, doc: &ImageDoc) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let name = doc
            .path
            .as_deref()
            .map(file_name)
            .unwrap_or_else(|| "Sans titre".into());
        ui.add(egui::Label::new(RichText::new(name).strong().color(theme::TEXT)).truncate());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} × {} px", doc.width(), doc.height()))
                    .small()
                    .color(theme::TEXT_DIM),
            );
            if doc.dirty {
                badge(ui, "Modifié", theme::WARNING);
            }
        });
    });
}

fn adjustments_card(ui: &mut egui::Ui, doc: &ImageDoc, actions: &mut Vec<Action>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        theme::section_title(
            ui,
            "Ajustements",
            "Aperçu en direct · double-clic pour remettre à zéro",
        );

        let mut adj = doc.adjustments;
        let mut changed = false;
        changed |= adjust_slider(ui, "Luminosité", &mut adj.brightness, -100.0..=100.0, true);
        changed |= adjust_slider(ui, "Contraste", &mut adj.contrast, -100.0..=100.0, true);
        changed |= adjust_slider(ui, "Saturation", &mut adj.saturation, -100.0..=100.0, true);
        changed |= adjust_slider(ui, "Vignette", &mut adj.vignette, 0.0..=100.0, false);
        changed |= adjust_slider(ui, "Grain", &mut adj.grain, 0.0..=100.0, false);
        if changed {
            actions.push(Action::SetAdjustments(adj));
        }

        ui.add_space(6.0);
        let pending = !adj.is_identity();
        let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
        let size = Vec2::new(width, 32.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(pending, egui::Button::new("Réinitialiser").min_size(size))
                .clicked()
            {
                actions.push(Action::ResetAdjustments);
            }
            if ui
                .add_enabled(pending, theme::primary_button("Appliquer").min_size(size))
                .clicked()
            {
                actions.push(Action::CommitAdjustments);
            }
        });
    });
}

fn adjust_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    signed: bool,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::TEXT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let modified = value.abs() >= 0.5;
            let color = if modified {
                theme::ACCENT
            } else {
                theme::TEXT_DIM
            };
            let digits = if signed {
                format!("{:+.0}", *value)
            } else {
                format!("{:.0}", *value)
            };
            ui.label(RichText::new(digits).monospace().color(color));
        });
    });
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = ui.available_width();
        ui.spacing_mut().interact_size.y = 18.0;
        let response = ui.add(egui::Slider::new(value, range).show_value(false));
        if response.double_clicked() {
            *value = 0.0;
            changed = true;
        }
        changed |= response.changed();
    });
    ui.add_space(2.0);
    changed
}

fn draw_card(ui: &mut egui::Ui, mode: &mut DrawMode, color: &mut Color32, radius: &mut f32) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        theme::section_title(ui, "Dessin", "Glissez sur l'image. Ctrl+Z annule le trait.");
        ui.horizontal(|ui| {
            if ui
                .selectable_label(*mode == DrawMode::Brush, "Pinceau")
                .clicked()
            {
                *mode = DrawMode::Brush;
            }
            if ui
                .selectable_label(*mode == DrawMode::Eraser, "Gomme")
                .clicked()
            {
                *mode = DrawMode::Eraser;
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Taille").color(theme::TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{radius:.0} px"))
                        .monospace()
                        .color(theme::ACCENT),
                );
            });
        });
        ui.add(egui::Slider::new(radius, 1.0..=120.0).show_value(false));
        if *mode == DrawMode::Brush {
            color_row(ui, color);
        } else {
            ui.label(
                RichText::new("La gomme rend les pixels transparents.")
                    .small()
                    .color(theme::TEXT_DIM),
            );
        }
    });
}

fn text_card(
    ui: &mut egui::Ui,
    text: &mut TextPlacement,
    active: bool,
    straighten_locked: bool,
    actions: &mut Vec<Action>,
) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        theme::section_title(
            ui,
            "Texte",
            "Écrivez ici, cliquez sur l'image pour placer, puis Appliquer.",
        );
        let editor = ui.add(
            egui::TextEdit::multiline(&mut text.content)
                .desired_rows(3)
                .hint_text("Votre texte"),
        );
        if !active && (editor.changed() || editor.has_focus()) {
            actions.push(Action::SetTool(Tool::Text));
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Taille").color(theme::TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.0} px", text.size))
                        .monospace()
                        .color(theme::ACCENT),
                );
            });
        });
        ui.add(egui::Slider::new(&mut text.size, 12.0..=320.0).show_value(false));
        color_row(ui, &mut text.color);
        if straighten_locked {
            ui.label(
                RichText::new("Appliquez le redressement pour déplacer le texte.")
                    .small()
                    .color(theme::WARNING),
            );
        }
        let ready = text.pos.is_some() && !text.content.trim().is_empty();
        ui.add_space(4.0);
        if ui.button("Centrer").clicked() {
            actions.push(Action::SetTool(Tool::Text));
            actions.push(Action::CenterText);
        }
        ui.add_space(4.0);
        let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
        let size = Vec2::new(width, 32.0);
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("Effacer").min_size(size))
                .clicked()
            {
                actions.push(Action::ClearText);
            }
            if ui
                .add_enabled(ready, theme::primary_button("Appliquer").min_size(size))
                .clicked()
            {
                actions.push(Action::ApplyText);
            }
        });
    });
}

fn color_row(ui: &mut egui::Ui, color: &mut Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Couleur").color(theme::TEXT));
        ui.color_edit_button_srgba(color);
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for swatch in [
            Color32::WHITE,
            Color32::BLACK,
            Color32::from_rgb(220, 50, 50),
            Color32::from_rgb(240, 180, 40),
            Color32::from_rgb(70, 130, 230),
            Color32::from_rgb(40, 180, 120),
        ] {
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
            ui.painter().rect_filled(rect, 4.0, swatch);
            if *color == swatch {
                ui.painter().rect_stroke(
                    rect,
                    4.0,
                    Stroke::new(2.0_f32, theme::ACCENT),
                    StrokeKind::Inside,
                );
            }
            if response.clicked() {
                *color = swatch;
            }
        }
    });
}

fn filters_card(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    doc: &mut ImageDoc,
    actions: &mut Vec<Action>,
) {
    let thumbs = doc.filter_thumb_ids(ctx);
    let active = doc.active_filter;
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        theme::section_title(
            ui,
            "Filtres",
            "Aperçu réversible · recliquez sur le filtre actif ou sur « Aucun » pour revenir en arrière",
        );

        const COLUMNS: usize = 3;
        let gap = 8.0;
        let tile = ((ui.available_width() - gap * (COLUMNS as f32 - 1.0)) / COLUMNS as f32).floor();
        for row in thumbs.chunks(COLUMNS) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (filter, tex_id) in row {
                    let label = filter.map_or("Aucun", |f| f.label());
                    let selected = active == *filter;
                    if filter_tile(ui, *tex_id, label, tile, selected).clicked() {
                        match filter {
                            Some(f) => actions.push(Action::Filter(*f)),
                            None => actions.push(Action::ClearFilter),
                        }
                    }
                }
            });
        }

        if let Some(filter) = active {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("En aperçu : {} (non appliqué)", filter.label()))
                    .small()
                    .color(theme::ACCENT),
            );
            ui.add_space(4.0);
            let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            let size = Vec2::new(width, 32.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new("↩  Retirer").min_size(size))
                    .on_hover_text("Revenir à l'image sans filtre")
                    .clicked()
                {
                    actions.push(Action::ClearFilter);
                }
                if ui
                    .add(theme::primary_button("Appliquer").min_size(size))
                    .on_hover_text("Valider le filtre (annulable ensuite avec Ctrl+Z)")
                    .clicked()
                {
                    actions.push(Action::CommitFilter);
                }
            });
        }
    });
}

fn filter_tile(
    ui: &mut egui::Ui,
    tex_id: egui::TextureId,
    label: &str,
    size: f32,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(size, size + 20.0), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let img_rect = Rect::from_min_size(rect.min, Vec2::splat(size));
    let hovered = response.hovered();

    egui::Image::new((tex_id, img_rect.size()))
        .corner_radius(theme::RADIUS)
        .paint_at(ui, img_rect);
    let stroke = if selected || hovered {
        Stroke::new(2.0_f32, theme::ACCENT)
    } else {
        Stroke::new(1.0_f32, theme::BORDER)
    };
    ui.painter()
        .rect_stroke(img_rect, theme::RADIUS, stroke, StrokeKind::Inside);
    if selected {
        // Check badge so the active filter is visible at a glance.
        let badge_rect = Rect::from_min_size(img_rect.min + Vec2::new(4.0, 4.0), Vec2::splat(18.0));
        ui.painter()
            .circle_filled(badge_rect.center(), 9.0, theme::ACCENT);
        ui.painter().text(
            badge_rect.center(),
            Align2::CENTER_CENTER,
            "✓",
            FontId::proportional(12.0),
            Color32::WHITE,
        );
    }
    let label = if selected {
        format!("✓ {label}")
    } else {
        label.to_owned()
    };
    ui.painter().text(
        Pos2::new(rect.center().x, img_rect.max.y + 10.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.5),
        if selected || hovered {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        },
    );
    response
}

fn transforms_card(ui: &mut egui::Ui, straighten: &mut f32, actions: &mut Vec<Action>) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        theme::section_title(
            ui,
            "Transformations",
            "Le redressement reste un aperçu tant qu'il n'est pas appliqué.",
        );

        ui.horizontal(|ui| {
            ui.label(RichText::new("Redresser").color(theme::TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let active = straighten.abs() >= 0.05;
                let color = if active {
                    theme::ACCENT
                } else {
                    theme::TEXT_DIM
                };
                ui.label(
                    RichText::new(format!("{straighten:+.1}°"))
                        .monospace()
                        .color(color),
                );
            });
        });
        ui.scope(|ui| {
            ui.spacing_mut().slider_width = ui.available_width();
            let response = ui.add(egui::Slider::new(straighten, -45.0..=45.0).show_value(false));
            if response.double_clicked() {
                *straighten = 0.0;
            }
        });
        let pending = straighten.abs() >= 0.05;
        if pending {
            let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            let size = Vec2::new(width, 32.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new("Réinitialiser").min_size(size))
                    .clicked()
                {
                    actions.push(Action::ResetStraighten);
                }
                if ui
                    .add(theme::primary_button("Appliquer").min_size(size))
                    .on_hover_text("Entrée")
                    .clicked()
                {
                    actions.push(Action::ApplyStraighten);
                }
            });
        }
        ui.add_space(4.0);

        let items: [(&str, Transform, &'static str); 5] = [
            ("↺  −90°", filters::rotate270, "Rotation −90°"),
            ("↻  +90°", filters::rotate90, "Rotation +90°"),
            ("↔  Miroir H", filters::flip_horizontal, "Miroir horizontal"),
            ("↕  Miroir V", filters::flip_vertical, "Miroir vertical"),
            ("↻  180°", filters::rotate180, "Rotation 180°"),
        ];
        let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
        for row in items.chunks(2) {
            ui.horizontal(|ui| {
                for (label, f, status) in row {
                    let button = egui::Button::new(*label).min_size(Vec2::new(width, 32.0));
                    if ui.add(button).on_hover_text(*status).clicked() {
                        actions.push(Action::Transform(*f, status));
                    }
                }
            });
        }
    });
}

fn paint_crop(painter: &egui::Painter, crop: CropRect, img_rect: Rect, img_w: f32, img_h: f32) {
    let (x, y, w, h) = crop.normalized();
    let sel = image_rect_to_screen(
        Rect::from_min_size(Pos2::new(x as f32, y as f32), Vec2::new(w as f32, h as f32)),
        img_rect,
        img_w,
        img_h,
    );

    let dim = Color32::from_black_alpha(160);
    let full = img_rect;
    for r in [
        Rect::from_min_max(full.min, Pos2::new(full.max.x, sel.min.y)),
        Rect::from_min_max(Pos2::new(full.min.x, sel.max.y), full.max),
        Rect::from_min_max(
            Pos2::new(full.min.x, sel.min.y),
            Pos2::new(sel.min.x, sel.max.y),
        ),
        Rect::from_min_max(
            Pos2::new(sel.max.x, sel.min.y),
            Pos2::new(full.max.x, sel.max.y),
        ),
    ] {
        painter.rect_filled(r, 0.0, dim);
    }

    let guide = Stroke::new(1.0_f32, Color32::from_white_alpha(70));
    for t in [1.0 / 3.0, 2.0 / 3.0] {
        let gx = sel.min.x + sel.width() * t;
        let gy = sel.min.y + sel.height() * t;
        painter.line_segment([Pos2::new(gx, sel.min.y), Pos2::new(gx, sel.max.y)], guide);
        painter.line_segment([Pos2::new(sel.min.x, gy), Pos2::new(sel.max.x, gy)], guide);
    }
    painter.rect_stroke(
        sel,
        0.0,
        Stroke::new(1.0_f32, Color32::WHITE),
        StrokeKind::Outside,
    );

    let handle = Stroke::new(3.0_f32, theme::ACCENT);
    let len = 14.0_f32.min(sel.width() / 2.0).min(sel.height() / 2.0);
    for (corner, dx, dy) in [
        (sel.left_top(), 1.0, 1.0),
        (sel.right_top(), -1.0, 1.0),
        (sel.left_bottom(), 1.0, -1.0),
        (sel.right_bottom(), -1.0, -1.0),
    ] {
        painter.line_segment([corner, corner + Vec2::new(dx * len, 0.0)], handle);
        painter.line_segment([corner, corner + Vec2::new(0.0, dy * len)], handle);
    }
}

fn drop_overlay(ui: &egui::Ui, canvas: Rect) {
    if !ui.ctx().input(|i| !i.raw.hovered_files.is_empty()) {
        return;
    }
    let painter = ui.painter();
    let area = canvas.shrink(16.0);
    painter.rect(
        area,
        CornerRadius::same(16),
        Color32::from_rgba_unmultiplied(104, 132, 255, 30),
        Stroke::new(2.0_f32, theme::ACCENT),
        StrokeKind::Inside,
    );
    painter.text(
        area.center(),
        Align2::CENTER_CENTER,
        "Déposez l'image pour l'ouvrir",
        FontId::proportional(20.0),
        theme::TEXT,
    );
}

fn floating_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::BG_PANEL)
        .stroke(Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(theme::RADIUS_LG)
        .inner_margin(Margin::symmetric(12, 8))
        .shadow(Shadow {
            offset: [0, 6],
            blur: 20,
            spread: 0,
            color: Color32::from_black_alpha(120),
        })
}

/// Groups selectable buttons inside a recessed pill, like a segmented control.
fn segmented(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::BG_CANVAS)
        .corner_radius(theme::RADIUS + 2)
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.spacing_mut().interact_size.y = 26.0;
                add_contents(ui);
            });
        });
}

fn vertical_divider(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 22.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::BORDER);
    ui.add_space(4.0);
}

fn icon_button(ui: &mut egui::Ui, icon: &str, hint: &str, enabled: bool) -> egui::Response {
    let button = egui::Button::new(RichText::new(icon).size(16.0))
        .frame_when_inactive(false)
        .min_size(Vec2::splat(30.0));
    ui.add_enabled(enabled, button).on_hover_text(hint)
}

fn small_button(ui: &mut egui::Ui, text: &str, hint: &str) -> egui::Response {
    let button = egui::Button::new(RichText::new(text).small().color(theme::TEXT))
        .frame_when_inactive(false)
        .min_size(Vec2::new(22.0, 20.0));
    ui.add(button).on_hover_text(hint)
}

fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(11.0), color);
    let padding = Vec2::new(7.0, 2.0);
    let (rect, _) = ui.allocate_exact_size(galley.size() + padding * 2.0, Sense::hover());
    ui.painter()
        .rect_filled(rect, 8.0, color.gamma_multiply(0.15));
    ui.painter().galley(rect.min + padding, galley, color);
}

fn load_logo(ctx: &egui::Context) -> Option<TextureHandle> {
    let img = image::load_from_memory(include_bytes!("../assets/logo.png")).ok()?;
    let img = img.thumbnail(144, 144).to_rgba8();
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [img.width() as usize, img.height() as usize],
        img.as_raw(),
    );
    Some(ctx.load_texture("logo", color, TextureOptions::LINEAR))
}

/// 2×2 checkerboard tiled with repeat sampling to show transparency behind the photo.
fn make_checker(ctx: &egui::Context) -> TextureHandle {
    let a = Color32::from_rgb(52, 55, 64);
    let b = Color32::from_rgb(40, 42, 50);
    let color = egui::ColorImage::new([2, 2], vec![a, b, b, a]);
    ctx.load_texture("checker", color, TextureOptions::NEAREST_REPEAT)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn screen_to_image(pos: Pos2, img_rect: Rect, img_w: f32, img_h: f32) -> Pos2 {
    let u = ((pos.x - img_rect.min.x) / img_rect.width()).clamp(0.0, 1.0);
    let v = ((pos.y - img_rect.min.y) / img_rect.height()).clamp(0.0, 1.0);
    Pos2::new(u * img_w, v * img_h)
}

fn screen_to_image_unclamped(pos: Pos2, img_rect: Rect, img_w: f32, img_h: f32) -> Pos2 {
    let u = (pos.x - img_rect.min.x) / img_rect.width().max(1.0);
    let v = (pos.y - img_rect.min.y) / img_rect.height().max(1.0);
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

fn is_jpeg(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg")
    )
}

fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} o")
    } else if bytes < 1024 * 1024 {
        format!("{:.0} Ko", bytes as f32 / 1024.0)
    } else {
        format!("{:.1} Mo", bytes as f32 / (1024.0 * 1024.0))
    }
}

fn color_rgba(color: Color32) -> [u8; 4] {
    let [r, g, b, a] = color.to_array();
    [r, g, b, a]
}

fn text_cache_key(text: &TextPlacement) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in text.content.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash ^= text.size.to_bits() as u64;
    hash = hash.wrapping_mul(0x100000001b3);
    hash ^= u32::from_le_bytes(color_rgba(text.color)) as u64;
    hash
}

fn full_uv() -> Rect {
    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
}

fn contain_rect(dst: Rect, src_w: f32, src_h: f32) -> Rect {
    if src_w <= 0.0 || src_h <= 0.0 {
        return dst;
    }
    let scale = (dst.width() / src_w).min(dst.height() / src_h);
    Rect::from_center_size(dst.center(), Vec2::new(src_w * scale, src_h * scale))
}

fn split_hit(img_rect: Rect, t: f32) -> Rect {
    let x = img_rect.min.x + img_rect.width() * t;
    Rect::from_center_size(
        Pos2::new(x, img_rect.center().y),
        Vec2::new(18.0, img_rect.height()),
    )
}

fn rotate_point(point: Pos2, pivot: Pos2, degrees: f32) -> Pos2 {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let x = point.x - pivot.x;
    let y = point.y - pivot.y;
    Pos2::new(pivot.x + x * cos - y * sin, pivot.y + x * sin + y * cos)
}

fn paint_textured_quad(
    painter: &egui::Painter,
    texture: TextureId,
    rect: Rect,
    uv: Rect,
    pivot: Pos2,
    angle_deg: f32,
) {
    if !rect.is_positive() {
        return;
    }
    if angle_deg.abs() < 0.05 {
        painter.image(texture, rect, uv, Color32::WHITE);
        return;
    }
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    let uvs = [
        uv.left_top(),
        uv.right_top(),
        uv.right_bottom(),
        uv.left_bottom(),
    ];
    let mut mesh = Mesh::with_texture(texture);
    for (pos, uv) in corners.into_iter().zip(uvs) {
        mesh.vertices.push(Vertex {
            pos: rotate_point(pos, pivot, angle_deg),
            uv,
            color: Color32::WHITE,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

fn paint_text_overlay(
    painter: &egui::Painter,
    texture: Option<TextureId>,
    pos: Option<Pos2>,
    size: Option<Vec2>,
    img_rect: Rect,
    img_w: f32,
    img_h: f32,
    pivot: Pos2,
    angle_deg: f32,
) {
    let (Some(texture), Some(pos), Some(size)) = (texture, pos, size) else {
        return;
    };
    if size.x < 1.0 || size.y < 1.0 {
        return;
    }
    let screen = image_rect_to_screen(Rect::from_min_size(pos, size), img_rect, img_w, img_h);
    paint_textured_quad(painter, texture, screen, full_uv(), pivot, angle_deg);
}

fn paint_caption(painter: &egui::Painter, pos: Pos2, text: &str) {
    let font = FontId::proportional(13.0);
    painter.text(
        pos + Vec2::new(1.0, 1.0),
        Align2::LEFT_TOP,
        text,
        font.clone(),
        Color32::from_black_alpha(180),
    );
    painter.text(pos, Align2::LEFT_TOP, text, font, Color32::WHITE);
}

fn stroke_hits(pos: Pos2, img_w: f32, img_h: f32, radius: f32) -> bool {
    pos.x >= -radius && pos.y >= -radius && pos.x <= img_w + radius && pos.y <= img_h + radius
}

fn draw_interaction(
    stroke_from: &mut Option<Pos2>,
    doc: &mut ImageDoc,
    response: &egui::Response,
    img_rect: Rect,
    img_w: f32,
    img_h: f32,
    radius: f32,
    color: [u8; 4],
    erase: bool,
) {
    let pointer = response.interact_pointer_pos();
    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(pos) = pointer
    {
        let img_pos = screen_to_image_unclamped(pos, img_rect, img_w, img_h);
        if stroke_hits(img_pos, img_w, img_h, radius) {
            doc.begin_stroke();
            doc.paint_stroke(img_pos, img_pos, radius, color, erase);
            *stroke_from = Some(img_pos);
        }
    } else if response.dragged_by(egui::PointerButton::Primary)
        && stroke_from.is_some()
        && let Some(pos) = pointer
    {
        let img_pos = screen_to_image_unclamped(pos, img_rect, img_w, img_h);
        let from = stroke_from.unwrap_or(img_pos);
        doc.paint_stroke(from, img_pos, radius, color, erase);
        *stroke_from = Some(img_pos);
    }
    if response.drag_stopped() {
        doc.end_stroke();
        *stroke_from = None;
    }
}

fn text_interaction(
    text: &mut TextPlacement,
    dragging: &mut bool,
    drag_last: &mut Option<Pos2>,
    response: &egui::Response,
    img_rect: Rect,
    img_w: f32,
    img_h: f32,
    raster: Option<Vec2>,
) {
    let Some(pos) = response.interact_pointer_pos() else {
        if response.drag_stopped() {
            *dragging = false;
            *drag_last = None;
        }
        return;
    };
    let img_pos = screen_to_image_unclamped(pos, img_rect, img_w, img_h);
    if response.drag_started_by(egui::PointerButton::Primary) {
        let size = raster.unwrap_or(Vec2::ZERO);
        let inside = text
            .pos
            .is_some_and(|origin| Rect::from_min_size(origin, size).contains(img_pos));
        *dragging = inside;
        if !inside {
            text.pos = Some(Pos2::new(
                img_pos.x - size.x * 0.5,
                img_pos.y - size.y * 0.5,
            ));
        }
        *drag_last = Some(img_pos);
    } else if *dragging && response.dragged_by(egui::PointerButton::Primary) {
        if let (Some(last), Some(origin)) = (*drag_last, text.pos.as_mut()) {
            *origin += img_pos - last;
        }
        *drag_last = Some(img_pos);
    }
    if response.drag_stopped() {
        *dragging = false;
        *drag_last = None;
    }
}
