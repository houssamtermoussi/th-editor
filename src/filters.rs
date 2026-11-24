use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ExtendedColorType, ImageBuffer, RgbImage, Rgba, RgbaImage, imageops};

/// Named photo filters shown as clickable previews in the side panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoFilter {
    Grayscale,
    Sepia,
    Invert,
    SoftBlur,
    Sharpen,
}

impl PhotoFilter {
    pub const ALL: &'static [Self] = &[
        Self::Grayscale,
        Self::Sepia,
        Self::Invert,
        Self::SoftBlur,
        Self::Sharpen,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Grayscale => "Noir & blanc",
            Self::Sepia => "Sépia",
            Self::Invert => "Négatif",
            Self::SoftBlur => "Flou léger",
            Self::Sharpen => "Netteté",
        }
    }

    pub fn apply(self, src: &RgbaImage) -> RgbaImage {
        match self {
            Self::Grayscale => grayscale(src),
            Self::Sepia => sepia(src),
            Self::Invert => invert(src),
            Self::SoftBlur => blur(src, 2.0),
            Self::Sharpen => sharpen(src),
        }
    }
}

/// Adjustments applied live via sliders (non-destructive until "Apply").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adjustments {
    pub brightness: f32, // -100 ..= 100
    pub contrast: f32,   // -100 ..= 100
    pub saturation: f32, // -100 ..= 100
    pub vignette: f32,   // 0 ..= 100
    pub grain: f32,      // 0 ..= 100
}

impl Default for Adjustments {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            vignette: 0.0,
            grain: 0.0,
        }
    }
}

impl Adjustments {
    pub fn is_identity(&self) -> bool {
        self.brightness.abs() < f32::EPSILON
            && self.contrast.abs() < f32::EPSILON
            && self.saturation.abs() < f32::EPSILON
            && self.vignette.abs() < f32::EPSILON
            && self.grain.abs() < f32::EPSILON
    }
}

pub fn apply_adjustments(src: &RgbaImage, adj: &Adjustments) -> RgbaImage {
    if adj.is_identity() {
        return src.clone();
    }

    let brightness = adj.brightness / 100.0;
    let contrast = 1.0 + adj.contrast / 100.0;
    let saturation = 1.0 + adj.saturation / 100.0;
    let vignette = (adj.vignette / 100.0).clamp(0.0, 1.0);
    let grain = (adj.grain / 100.0).clamp(0.0, 1.0);

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

        if vignette > 0.0 {
            let nx = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
            let ny = (y as f32 + 0.5) / h as f32 * 2.0 - 1.0;
            let radius = (nx * nx + ny * ny).sqrt();
            let t = ((radius - 0.35) / 0.95).clamp(0.0, 1.0);
            let t = t * t * (3.0 - 2.0 * t);
            let shade = 1.0 - t * vignette;
            rf *= shade;
            gf *= shade;
            bf *= shade;
        }

        if grain > 0.0 {
            let n = grain_noise(x, y) * grain * 0.45;
            rf = (rf + n).clamp(0.0, 1.0);
            gf = (gf + n).clamp(0.0, 1.0);
            bf = (bf + n).clamp(0.0, 1.0);
        }

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
    DynamicImage::ImageRgba8(src.clone()).grayscale().to_rgba8()
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

/// Stable grain in −1..=1 so a slider drag does not flicker.
fn grain_noise(x: u32, y: u32) -> f32 {
    let mut n = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263));
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n ^= n >> 16;
    (n & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0
}

/// Rotate around the image center. Positive degrees turn clockwise on screen.
/// Near-zero angles return the source unchanged. Empty corners stay transparent.
pub fn rotate_degrees(src: &RgbaImage, degrees: f32) -> RgbaImage {
    if degrees.abs() < 0.05 {
        return src.clone();
    }
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (w, h) = src.dimensions();
    let wf = w as f32;
    let hf = h as f32;
    let new_w = ((wf * cos.abs() + hf * sin.abs()).ceil() as u32).max(1);
    let new_h = ((wf * sin.abs() + hf * cos.abs()).ceil() as u32).max(1);

    let cx_s = (wf - 1.0) * 0.5;
    let cy_s = (hf - 1.0) * 0.5;
    let cx_d = (new_w as f32 - 1.0) * 0.5;
    let cy_d = (new_h as f32 - 1.0) * 0.5;

    let mut out = ImageBuffer::new(new_w, new_h);
    for y in 0..new_h {
        for x in 0..new_w {
            let dx = x as f32 - cx_d;
            let dy = y as f32 - cy_d;
            // Inverse of the clockwise-on-screen mapping used by the canvas preview.
            let sx = cx_s + cos * dx + sin * dy;
            let sy = cy_s - sin * dx + cos * dy;
            out.put_pixel(x, y, Rgba(sample_bilinear(src, sx, sy)));
        }
    }
    out
}

fn sample_bilinear(src: &RgbaImage, sx: f32, sy: f32) -> [u8; 4] {
    let w = src.width() as i32;
    let h = src.height() as i32;
    let x0 = sx.floor() as i32;
    let y0 = sy.floor() as i32;
    let tx = sx - x0 as f32;
    let ty = sy - y0 as f32;
    let mut acc = [0.0_f32; 4];
    for dy in 0..2 {
        for dx in 0..2 {
            let xx = x0 + dx;
            let yy = y0 + dy;
            let weight =
                (if dx == 0 { 1.0 - tx } else { tx }) * (if dy == 0 { 1.0 - ty } else { ty });
            let sample = if xx >= 0 && yy >= 0 && xx < w && yy < h {
                let px = src.get_pixel(xx as u32, yy as u32).0;
                [px[0] as f32, px[1] as f32, px[2] as f32, px[3] as f32]
            } else {
                [0.0; 4]
            };
            for channel in 0..4 {
                acc[channel] += sample[channel] * weight;
            }
        }
    }
    acc.map(|v| v.round().clamp(0.0, 255.0) as u8)
}

/// Soft round stamp. `erase` fades alpha instead of painting `color`.
pub fn stamp(img: &mut RgbaImage, cx: f32, cy: f32, radius: f32, color: [u8; 4], erase: bool) {
    let radius = radius.max(0.5);
    let (w, h) = img.dimensions();
    let x0 = (cx - radius).floor().max(0.0) as u32;
    let y0 = (cy - radius).floor().max(0.0) as u32;
    let x1 = ((cx + radius).ceil() as u32).min(w);
    let y1 = ((cy + radius).ceil() as u32).min(h);
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > radius {
                continue;
            }
            let t = 1.0 - dist / radius;
            let coverage = t * t * (3.0 - 2.0 * t);
            let pixel = img.get_pixel_mut(x, y);
            if erase {
                let alpha = pixel.0[3] as f32 * (1.0 - coverage);
                pixel.0[3] = alpha.round().clamp(0.0, 255.0) as u8;
            } else {
                blend_over(&mut pixel.0, color, coverage);
            }
        }
    }
}

/// Interpolated stroke so a fast drag does not leave gaps.
pub fn stroke_line(
    img: &mut RgbaImage,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    color: [u8; 4],
    erase: bool,
) {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let dist = (dx * dx + dy * dy).sqrt();
    let step = (radius * 0.35).max(0.75);
    let steps = (dist / step).ceil() as u32;
    for i in 0..=steps {
        let t = if steps == 0 {
            0.0
        } else {
            i as f32 / steps as f32
        };
        stamp(img, from.0 + dx * t, from.1 + dy * t, radius, color, erase);
    }
}

fn blend_over(dst: &mut [u8; 4], src: [u8; 4], coverage: f32) {
    let src_a = (src[3] as f32 / 255.0) * coverage.clamp(0.0, 1.0);
    let dst_a = dst[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.001 {
        *dst = [0, 0, 0, 0];
        return;
    }
    for i in 0..3 {
        let blended = (src[i] as f32 * src_a + dst[i] as f32 * dst_a * (1.0 - src_a)) / out_a;
        dst[i] = blended.round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

pub fn blit(dst: &mut RgbaImage, src: &RgbaImage, x: i32, y: i32) {
    imageops::overlay(dst, src, x as i64, y as i64);
}

/// JPEG with transparent pixels flattened onto white.
pub fn encode_jpeg(img: &RgbaImage, quality: u8) -> Result<Vec<u8>, String> {
    let quality = quality.clamp(1, 100);
    let rgb = flatten_on_white(img);
    let mut cursor = Cursor::new(Vec::new());
    let mut encoder = JpegEncoder::new_with_quality(&mut cursor, quality);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

fn flatten_on_white(img: &RgbaImage) -> RgbImage {
    let mut rgb = RgbImage::new(img.width(), img.height());
    for (x, y, pixel) in img.enumerate_pixels() {
        let alpha = pixel.0[3] as f32 / 255.0;
        let mix = |channel: u8| -> u8 {
            (channel as f32 * alpha + 255.0 * (1.0 - alpha))
                .round()
                .clamp(0.0, 255.0) as u8
        };
        rgb.put_pixel(
            x,
            y,
            image::Rgb([mix(pixel.0[0]), mix(pixel.0[1]), mix(pixel.0[2])]),
        );
    }
    rgb
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, color: [u8; 4]) -> RgbaImage {
        ImageBuffer::from_pixel(w, h, Rgba(color))
    }

    #[test]
    fn identity_adjustments_keep_pixels() {
        let src = solid(4, 4, [10, 20, 30, 255]);
        let out = apply_adjustments(&src, &Adjustments::default());
        assert_eq!(out.as_raw(), src.as_raw());
    }

    #[test]
    fn vignette_darkens_corners_more_than_the_center() {
        let src = solid(32, 32, [180, 180, 180, 255]);
        let out = apply_adjustments(
            &src,
            &Adjustments {
                vignette: 100.0,
                ..Adjustments::default()
            },
        );
        let center = out.get_pixel(16, 16).0[0];
        let corner = out.get_pixel(0, 0).0[0];
        assert!(
            center > corner,
            "center {center} should stay brighter than corner {corner}"
        );
        assert!(center > 150);
    }

    #[test]
    fn grain_is_stable_and_visible() {
        let src = solid(16, 16, [120, 120, 120, 255]);
        let adj = Adjustments {
            grain: 80.0,
            ..Adjustments::default()
        };
        let a = apply_adjustments(&src, &adj);
        let b = apply_adjustments(&src, &adj);
        assert_eq!(a.as_raw(), b.as_raw());
        assert_ne!(a.as_raw(), src.as_raw());
    }

    #[test]
    fn zero_rotation_is_a_copy() {
        let src = solid(6, 4, [1, 2, 3, 255]);
        assert_eq!(rotate_degrees(&src, 0.0).as_raw(), src.as_raw());
        assert_eq!(rotate_degrees(&src, 0.01).as_raw(), src.as_raw());
    }

    #[test]
    fn quarter_turn_swaps_the_long_side() {
        let mut src = solid(30, 10, [0, 0, 0, 0]);
        src.put_pixel(2, 2, Rgba([255, 0, 0, 255]));
        let out = rotate_degrees(&src, 90.0);
        assert!(
            out.height() > out.width(),
            "{}x{}",
            out.width(),
            out.height()
        );
        let red: u32 = out.pixels().map(|pixel| pixel.0[0] as u32).sum();
        assert!(red > 200, "red energy {red}");
    }

    #[test]
    fn brush_paints_the_center_and_leaves_the_corner() {
        let mut img = solid(21, 21, [0, 0, 0, 0]);
        stamp(&mut img, 10.0, 10.0, 3.0, [255, 0, 0, 255], false);
        let center = img.get_pixel(10, 10).0;
        assert!(center[0] > 200 && center[3] > 200);
        assert_eq!(img.get_pixel(0, 0).0[3], 0);
    }

    #[test]
    fn eraser_clears_alpha() {
        let mut img = solid(15, 15, [0, 80, 0, 255]);
        stamp(&mut img, 7.5, 7.5, 2.0, [0, 0, 0, 255], true);
        assert!(img.get_pixel(7, 7).0[3] < 10);
        assert_eq!(img.get_pixel(0, 0).0[3], 255);
    }

    #[test]
    fn jpeg_bytes_roundtrip() {
        let img = solid(8, 8, [10, 20, 30, 255]);
        let bytes = encode_jpeg(&img, 90).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (8, 8));
    }
}
