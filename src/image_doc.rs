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
    /// Filter shown as a live, reversible preview on top of `base`
    /// (like `adjustments`): not baked into the pixels until committed.
    pub active_filter: Option<PhotoFilter>,
    /// Cached preview = active filter applied over adjustments applied on base.
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
            active_filter: None,
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

    /// Live edits not yet baked into `base` (adjustments and/or filter preview).
    pub fn has_pending(&self) -> bool {
        !self.adjustments.is_identity() || self.active_filter.is_some()
    }

    /// Select a filter as a reversible preview, or `None` to go back to
    /// the image without any filter. Never touches `base`/history, so
    /// switching or clearing the filter always restores the previous look.
    pub fn set_filter(&mut self, filter: Option<PhotoFilter>) {
        if self.active_filter == filter {
            return;
        }
        self.active_filter = filter;
        self.recompute_preview();
    }

    /// Commit current preview as new base (e.g. after Apply adjustments/filter).
    /// Pushes history, so a committed edit can still be undone with Ctrl+Z.
    pub fn commit_preview(&mut self) {
        if !self.has_pending() || self.preview.as_raw() == self.base.as_raw() {
            // Nothing to bake; drop the pending state so callers stay clean.
            self.adjustments = Adjustments::default();
            self.active_filter = None;
            return;
        }
        self.push_history();
        self.base = self.preview.clone();
        self.adjustments = Adjustments::default();
        self.active_filter = None;
        self.dirty = true;
        self.mark_pixels_changed();
    }

    /// Apply a destructive transform on the committed base.
    pub fn apply_transform<F>(&mut self, f: F)
    where
        F: FnOnce(&RgbaImage) -> RgbaImage,
    {
        // Bake pending adjustments/filter first so they aren't lost.
        // A single history entry: undo returns to before both bake and transform.
        self.push_history();
        if self.has_pending() {
            self.base = self.preview.clone();
            self.adjustments = Adjustments::default();
            self.active_filter = None;
        }
        self.base = f(&self.base);
        self.preview = self.base.clone();
        self.dirty = true;
        self.mark_pixels_changed();
    }

    pub fn recompute_preview(&mut self) {
        let adjusted = filters::apply_adjustments(&self.base, &self.adjustments);
        self.preview = match self.active_filter {
            Some(filter) => filter.apply(&adjusted),
            None => adjusted,
        };
        self.mark_pixels_changed();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.redo_stack.push(self.base.clone());
            self.base = prev;
            self.adjustments = Adjustments::default();
            self.active_filter = None;
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
            self.active_filter = None;
            self.preview = self.base.clone();
            self.dirty = true;
            self.mark_pixels_changed();
        }
    }

    pub fn reset_to_original(&mut self) {
        if self.base.as_raw() == self.original.as_raw() && !self.has_pending() {
            return;
        }
        self.push_history();
        self.base = self.original.clone();
        self.adjustments = Adjustments::default();
        self.active_filter = None;
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

    /// Preview without the active filter, so thumbnails show the effect of
    /// each filter from the same starting point (never double-filtered).
    fn unfiltered_preview(&self) -> RgbaImage {
        filters::apply_adjustments(&self.base, &self.adjustments)
    }

    /// Texture IDs for the "Aucun" tile (`None`) plus each [`PhotoFilter::ALL`]
    /// entry, rebuilt when the underlying image changes.
    pub fn filter_thumb_ids(
        &mut self,
        ctx: &egui::Context,
    ) -> Vec<(Option<PhotoFilter>, egui::TextureId)> {
        let count = PhotoFilter::ALL.len() + 1;
        if self.filter_thumbs_dirty || self.filter_thumbs.len() != count {
            let thumb = make_square_thumb(&self.unfiltered_preview(), FILTER_THUMB_SIZE);
            let variants: Vec<RgbaImage> = std::iter::once(thumb.clone())
                .chain(PhotoFilter::ALL.iter().map(|f| f.apply(&thumb)))
                .collect();
            for (i, filtered) in variants.iter().enumerate() {
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
            self.filter_thumbs.truncate(count);
            self.filter_thumbs_dirty = false;
        }
        std::iter::once(None)
            .chain(PhotoFilter::ALL.iter().copied().map(Some))
            .zip(self.filter_thumbs.iter())
            .map(|(f, t)| (f, t.id()))
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
