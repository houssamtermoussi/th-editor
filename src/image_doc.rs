use std::path::PathBuf;

use egui::{ColorImage, TextureHandle, TextureOptions};
use image::{RgbaImage, imageops};

use crate::filters::{self, Adjustments, PhotoFilter};

const MAX_HISTORY: usize = 30;
const FILTER_THUMB_SIZE: u32 = 72;

/// Document holding the working image, history, and preview texture.
pub struct ImageDoc {
    pub path: Option<PathBuf>,
    /// Current committed pixels (after Apply / destructive ops).
    base: RgbaImage,
    /// Original as opened (for Reset).
    original: RgbaImage,
    history: Vec<RgbaImage>,
    redo_stack: Vec<RgbaImage>,
    pub adjustments: Adjustments,
    /// Cached preview = adjustments applied on base.
    preview: RgbaImage,
    texture: Option<TextureHandle>,
    texture_dirty: bool,
    filter_thumbs: Vec<TextureHandle>,
    filter_thumbs_dirty: bool,
    pub dirty: bool,
}

impl ImageDoc {
    pub fn from_rgba(img: RgbaImage, path: Option<PathBuf>) -> Self {
        let original = img.clone();
        let preview = img.clone();
        Self {
            path,
            base: img,
            original,
            history: Vec::new(),
            redo_stack: Vec::new(),
            adjustments: Adjustments::default(),
            preview,
            texture: None,
            texture_dirty: true,
            filter_thumbs: Vec::new(),
            filter_thumbs_dirty: true,
            dirty: false,
        }
    }

    fn mark_pixels_changed(&mut self) {
        self.texture_dirty = true;
        self.filter_thumbs_dirty = true;
    }

    pub fn width(&self) -> u32 {
        self.preview.width()
    }

    pub fn height(&self) -> u32 {
        self.preview.height()
    }

    pub fn preview(&self) -> &RgbaImage {
        &self.preview
    }

    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    fn push_history(&mut self) {
        self.history.push(self.base.clone());
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Commit current preview as new base (e.g. after Apply adjustments).
    pub fn commit_preview(&mut self) {
        if self.preview.as_raw() == self.base.as_raw() && self.adjustments.is_identity() {
            return;
        }
        self.push_history();
        self.base = self.preview.clone();
        self.adjustments = Adjustments::default();
        self.dirty = true;
        self.mark_pixels_changed();
    }

    /// Apply a destructive transform on the committed base.
    pub fn apply_transform<F>(&mut self, f: F)
    where
        F: FnOnce(&RgbaImage) -> RgbaImage,
    {
        // Bake pending adjustments first so they aren't lost.
        if !self.adjustments.is_identity() {
            self.preview = filters::apply_adjustments(&self.base, &self.adjustments);
            self.push_history();
            self.base = self.preview.clone();
            self.adjustments = Adjustments::default();
        } else {
            self.push_history();
        }
        self.base = f(&self.base);
        self.preview = self.base.clone();
        self.dirty = true;
        self.mark_pixels_changed();
    }

    pub fn recompute_preview(&mut self) {
        self.preview = filters::apply_adjustments(&self.base, &self.adjustments);
        self.mark_pixels_changed();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.redo_stack.push(self.base.clone());
            self.base = prev;
            self.adjustments = Adjustments::default();
            self.preview = self.base.clone();
            self.dirty = true;
            self.mark_pixels_changed();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.history.push(self.base.clone());
            self.base = next;
            self.adjustments = Adjustments::default();
            self.preview = self.base.clone();
            self.dirty = true;
            self.mark_pixels_changed();
        }
    }

    pub fn reset_to_original(&mut self) {
        if self.base.as_raw() == self.original.as_raw() && self.adjustments.is_identity() {
            return;
        }
        self.push_history();
        self.base = self.original.clone();
        self.adjustments = Adjustments::default();
        self.preview = self.base.clone();
        self.dirty = true;
        self.mark_pixels_changed();
    }

    pub fn texture(&mut self, ctx: &egui::Context) -> &TextureHandle {
        if self.texture_dirty || self.texture.is_none() {
            let color = ColorImage::from_rgba_unmultiplied(
                [self.preview.width() as usize, self.preview.height() as usize],
                self.preview.as_raw(),
            );
            match &mut self.texture {
                Some(tex) => {
                    tex.set(color, TextureOptions::LINEAR);
                }
                None => {
                    self.texture =
                        Some(ctx.load_texture("photo", color, TextureOptions::LINEAR));
                }
            }
            self.texture_dirty = false;
        }
        self.texture.as_ref().expect("texture just ensured")
    }

    /// Texture IDs for each [`PhotoFilter::ALL`] entry, rebuilt when the preview changes.
    pub fn filter_thumb_ids(&mut self, ctx: &egui::Context) -> Vec<(PhotoFilter, egui::TextureId)> {
        if self.filter_thumbs_dirty || self.filter_thumbs.len() != PhotoFilter::ALL.len() {
            let thumb = make_square_thumb(&self.preview, FILTER_THUMB_SIZE);
            for (i, filter) in PhotoFilter::ALL.iter().enumerate() {
                let filtered = filter.apply(&thumb);
                let color = ColorImage::from_rgba_unmultiplied(
                    [filtered.width() as usize, filtered.height() as usize],
                    filtered.as_raw(),
                );
                let name = format!("filter_thumb_{i}");
                if let Some(tex) = self.filter_thumbs.get_mut(i) {
                    tex.set(color, TextureOptions::LINEAR);
                } else {
                    self.filter_thumbs
                        .push(ctx.load_texture(name, color, TextureOptions::LINEAR));
                }
            }
            self.filter_thumbs.truncate(PhotoFilter::ALL.len());
            self.filter_thumbs_dirty = false;
        }
        PhotoFilter::ALL
            .iter()
            .zip(self.filter_thumbs.iter())
            .map(|(f, t)| (*f, t.id()))
            .collect()
    }
}

fn make_square_thumb(src: &RgbaImage, size: u32) -> RgbaImage {
    let (w, h) = src.dimensions();
    if w == 0 || h == 0 {
        return RgbaImage::new(size, size);
    }
    let side = w.min(h);
    let x = (w - side) / 2;
    let y = (h - side) / 2;
    let cropped = imageops::crop_imm(src, x, y, side, side).to_image();
    imageops::thumbnail(&cropped, size, size)
}
