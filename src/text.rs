use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use image::{Rgba, RgbaImage};

/// Bundled Liberation Sans (SIL OFL). See `assets/OFL-LiberationSans.txt`.
pub fn load_font() -> FontArc {
    FontArc::try_from_slice(include_bytes!("../assets/LiberationSans-Regular.ttf"))
        .expect("police embarquée invalide")
}

/// Rasterize `text` in straight alpha. Size is the font pixel size.
pub fn rasterize(font: &FontArc, text: &str, size: f32, color: [u8; 4]) -> RgbaImage {
    let size = size.clamp(8.0, 512.0);
    let scale = PxScale::from(size);
    let scaled = font.as_scaled(scale);
    let line_height = (scaled.ascent() - scaled.descent() + scaled.line_gap()).max(1.0);
    let pad = 2.0_f32;

    let lines: Vec<&str> = if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    };
    if lines.is_empty() {
        return RgbaImage::new(1, 1);
    }

    let mut glyphs = Vec::new();
    let mut max_x = pad;
    for (index, line) in lines.iter().enumerate() {
        let baseline = pad + scaled.ascent() + index as f32 * line_height;
        let mut x = pad;
        let mut previous = None;
        for ch in line.chars() {
            let id = font.glyph_id(ch);
            if let Some(previous) = previous {
                x += scaled.kern(previous, id);
            }
            glyphs.push(id.with_scale_and_position(scale, point(x, baseline)));
            x += scaled.h_advance(id);
            previous = Some(id);
        }
        max_x = max_x.max(x);
    }

    let width = (max_x + pad).ceil().clamp(1.0, 4096.0) as u32;
    let height = (pad + lines.len() as f32 * line_height + pad)
        .ceil()
        .clamp(1.0, 4096.0) as u32;
    let mut image = RgbaImage::new(width, height);

    for glyph in glyphs {
        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|px, py, coverage| {
            if coverage <= 0.0 {
                return;
            }
            let x = (bounds.min.x + px as f32).floor() as i32;
            let y = (bounds.min.y + py as f32).floor() as i32;
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                return;
            }
            let dest = image.get_pixel(x as u32, y as u32).0;
            let src_a = (coverage.clamp(0.0, 1.0) * color[3] as f32 / 255.0).clamp(0.0, 1.0);
            let dst_a = dest[3] as f32 / 255.0;
            let out_a = src_a + dst_a * (1.0 - src_a);
            if out_a <= 0.0 {
                return;
            }
            let mix = |src: u8, dst: u8| -> u8 {
                let value = (src as f32 * src_a + dst as f32 * dst_a * (1.0 - src_a)) / out_a;
                value.round().clamp(0.0, 255.0) as u8
            };
            image.put_pixel(
                x as u32,
                y as u32,
                Rgba([
                    mix(color[0], dest[0]),
                    mix(color[1], dest[1]),
                    mix(color[2], dest[2]),
                    (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
                ]),
            );
        });
    }

    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterize_french_text_has_coverage() {
        let font = load_font();
        let image = rasterize(&font, "Été", 32.0, [255, 255, 255, 255]);
        assert!(image.width() > 8 && image.height() > 8);
        assert!(image.pixels().any(|pixel| pixel.0[3] > 200));
    }
}
