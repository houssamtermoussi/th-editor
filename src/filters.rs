use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage, imageops};

/// Adjustments applied live via sliders (non-destructive until "Apply").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adjustments {
    pub brightness: f32, // -100 ..= 100
    pub contrast: f32,   // -100 ..= 100
    pub saturation: f32, // -100 ..= 100
}

impl Default for Adjustments {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
        }
    }
}

impl Adjustments {
    pub fn is_identity(&self) -> bool {
        self.brightness.abs() < f32::EPSILON
            && self.contrast.abs() < f32::EPSILON
            && self.saturation.abs() < f32::EPSILON
    }
}

pub fn apply_adjustments(src: &RgbaImage, adj: &Adjustments) -> RgbaImage {
    if adj.is_identity() {
        return src.clone();
    }

    let brightness = adj.brightness / 100.0;
    let contrast = 1.0 + adj.contrast / 100.0;
    let saturation = 1.0 + adj.saturation / 100.0;

    let (w, h) = src.dimensions();
    let mut out = ImageBuffer::new(w, h);

    for (x, y, pixel) in src.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        let (mut rf, mut gf, mut bf) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);

        // Contrast around mid-gray
        rf = ((rf - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
        gf = ((gf - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
        bf = ((bf - 0.5) * contrast + 0.5).clamp(0.0, 1.0);

        // Brightness
        rf = (rf + brightness).clamp(0.0, 1.0);
        gf = (gf + brightness).clamp(0.0, 1.0);
        bf = (bf + brightness).clamp(0.0, 1.0);

        // Saturation (luma-preserving)
        let luma = 0.2126 * rf + 0.7152 * gf + 0.0722 * bf;
        rf = (luma + (rf - luma) * saturation).clamp(0.0, 1.0);
        gf = (luma + (gf - luma) * saturation).clamp(0.0, 1.0);
        bf = (luma + (bf - luma) * saturation).clamp(0.0, 1.0);

        out.put_pixel(
            x,
            y,
            Rgba([
                (rf * 255.0).round() as u8,
                (gf * 255.0).round() as u8,
                (bf * 255.0).round() as u8,
                a,
            ]),
        );
    }

    out
}

pub fn grayscale(src: &RgbaImage) -> RgbaImage {
    DynamicImage::ImageRgba8(src.clone())
        .grayscale()
        .to_rgba8()
}

pub fn sepia(src: &RgbaImage) -> RgbaImage {
    let (w, h) = src.dimensions();
    let mut out = ImageBuffer::new(w, h);
    for (x, y, pixel) in src.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        let rf = r as f32;
        let gf = g as f32;
        let bf = b as f32;
        let tr = (0.393 * rf + 0.769 * gf + 0.189 * bf).min(255.0);
        let tg = (0.349 * rf + 0.686 * gf + 0.168 * bf).min(255.0);
        let tb = (0.272 * rf + 0.534 * gf + 0.131 * bf).min(255.0);
        out.put_pixel(x, y, Rgba([tr as u8, tg as u8, tb as u8, a]));
    }
    out
}

pub fn invert(src: &RgbaImage) -> RgbaImage {
    let (w, h) = src.dimensions();
    let mut out = ImageBuffer::new(w, h);
    for (x, y, pixel) in src.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        out.put_pixel(x, y, Rgba([255 - r, 255 - g, 255 - b, a]));
    }
    out
}

pub fn blur(src: &RgbaImage, sigma: f32) -> RgbaImage {
    imageops::blur(src, sigma)
}

pub fn sharpen(src: &RgbaImage) -> RgbaImage {
    imageops::unsharpen(src, 1.5, 3)
}

pub fn rotate90(src: &RgbaImage) -> RgbaImage {
    imageops::rotate90(src)
}

pub fn rotate180(src: &RgbaImage) -> RgbaImage {
    imageops::rotate180(src)
}

pub fn rotate270(src: &RgbaImage) -> RgbaImage {
    imageops::rotate270(src)
}

pub fn flip_horizontal(src: &RgbaImage) -> RgbaImage {
    imageops::flip_horizontal(src)
}

pub fn flip_vertical(src: &RgbaImage) -> RgbaImage {
    imageops::flip_vertical(src)
}

pub fn crop(src: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
    let (iw, ih) = src.dimensions();
    let x = x.min(iw.saturating_sub(1));
    let y = y.min(ih.saturating_sub(1));
    let w = w.min(iw - x).max(1);
    let h = h.min(ih - y).max(1);
    imageops::crop_imm(src, x, y, w, h).to_image()
}
