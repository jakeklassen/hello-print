//! Printable reference sheet for checking true-size, borderless output.
//!
//! One design for every paper size, at 300 dpi:
//! - a light 3 mm band and a 0.5 mm black frame on the image edge, so a
//!   cropped edge (band or frame missing) or a shift (white strip) is obvious;
//! - outlined 20 mm squares in every corner;
//! - 1-10 mm rulers at the middle of each edge: lost ticks = mm cropped;
//! - a 50 mm outlined square in the centre to measure scale.
//!
//! Every dimension is rounded to whole pixels and outlines are drawn inside
//! their box, so outer edge to outer edge is exact. Ink is kept to thin lines.

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use image::{Rgb, RgbImage};
use std::path::Path;

pub const DPI: f64 = 300.0;

const FONT: &[u8] = include_bytes!("../assets/fonts/AtkinsonHyperlegible-Regular.ttf");

const WHITE: Rgb<u8> = Rgb([255, 255, 255]);
const BLACK: Rgb<u8> = Rgb([0, 0, 0]);
const BAND: Rgb<u8> = Rgb([205, 230, 248]);

/// Whole pixels at 300 dpi.
fn px(mm: f64) -> u32 {
    (mm * DPI / 25.4).round() as u32
}

/// Renders the sheet for a portrait page of `width_mm` x `height_mm`.
/// `title` names the size on the sheet (e.g. "Letter (8.5 x 11 in)").
pub fn render(width_mm: f64, height_mm: f64, title: &str) -> RgbImage {
    let (w, h) = (px(width_mm), px(height_mm));
    let mut img = RgbImage::from_pixel(w, h, WHITE);
    let font = FontRef::try_from_slice(FONT).expect("bundled font is valid");
    let stroke = px(0.5);

    // 3 mm band and frame on the image edge.
    let band = px(3.0);
    fill(&mut img, 0, 0, w, band, BAND);
    fill(&mut img, 0, h - band, w, band, BAND);
    fill(&mut img, 0, 0, band, h, BAND);
    fill(&mut img, w - band, 0, band, h, BAND);
    outline(&mut img, 0, 0, w, h, stroke);

    // 20 mm corner squares sharing both edges with the frame.
    let corner = px(20.0);
    for (x, y) in [
        (0, 0),
        (w - corner, 0),
        (0, h - corner),
        (w - corner, h - corner),
    ] {
        outline(&mut img, x, y, corner, corner, stroke);
    }

    // 1-10 mm rulers at the middle of each edge; long ticks at 5 and 10.
    let (mid_x, mid_y) = (w / 2, h / 2);
    for mm in 1..=10 {
        let p = px(mm as f64);
        let len = if mm % 5 == 0 { 80 } else { 45 };
        fill(&mut img, p - 1, mid_y - len / 2, 3, len, BLACK); // left
        fill(&mut img, w - p - 2, mid_y - len / 2, 3, len, BLACK); // right
        fill(&mut img, mid_x - len / 2, p - 1, len, 3, BLACK); // top
        fill(&mut img, mid_x - len / 2, h - p - 2, len, 3, BLACK); // bottom
    }
    let small = PxScale::from(26.0);
    let ruler_label = "1-10 mm";
    draw_text(
        &mut img,
        &font,
        small,
        20,
        mid_y + 75,
        &format!("L {ruler_label}"),
    );
    let right = format!("R {ruler_label}");
    let right_x = w - 20 - text_width(&font, small, &right).ceil() as u32;
    draw_text(&mut img, &font, small, right_x, mid_y + 75, &right);
    draw_text(
        &mut img,
        &font,
        small,
        mid_x + 55,
        px(10.0) + 10,
        &format!("T {ruler_label}"),
    );
    draw_text(
        &mut img,
        &font,
        small,
        mid_x + 55,
        h - px(10.0) - 20,
        &format!("B {ruler_label}"),
    );

    // Centre square; 25 mm on pages too small for 50 mm between the corners.
    let square_mm = if width_mm.min(height_mm) >= 100.0 {
        50
    } else {
        25
    };
    let square = px(square_mm as f64);
    let (x0, y0) = ((w - square) / 2, (h - square) / 2);
    outline(&mut img, x0, y0, square, square, stroke);

    let max_width = w - 2 * px(22.0);
    let normal = PxScale::from(30.0);
    let line = 40;

    // Title above the square.
    let heading = format!("Hello Print test sheet - {title} - {width_mm} x {height_mm} mm");
    centered(&mut img, &font, normal, max_width, y0 - px(6.0), &heading);

    // Measurements and how to read the sheet below it.
    let mut y = y0 + square + px(4.0) + 20;
    for text in [
        format!("Square: {square_mm} x {square_mm} mm, outer edge to outer edge"),
        "Corner squares: 20 mm".to_string(),
    ] {
        centered(&mut img, &font, small, max_width, y, &text);
        y += line;
    }
    y += line / 2;
    for text in [
        format!(
            "Size: the square should measure {square_mm} mm. Bigger means the printer enlarged it."
        ),
        "Edges: the blue band should show on every side. Missing ticks = mm cut off.".to_string(),
        "Centring: the band should be the same width on opposite sides.".to_string(),
    ] {
        centered(&mut img, &font, small, max_width, y, &text);
        y += line;
    }

    img
}

/// Renders the sheet and writes it as a PNG.
pub fn save(path: &Path, width_mm: f64, height_mm: f64, title: &str) -> Result<(), String> {
    render(width_mm, height_mm, title)
        .save(path)
        .map_err(|e| format!("Failed to write test sheet: {}", e))
}

fn fill(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, color: Rgb<u8>) {
    for yy in y..(y + h).min(img.height()) {
        for xx in x..(x + w).min(img.width()) {
            img.put_pixel(xx, yy, color);
        }
    }
}

/// Outline drawn inside the box, so outer edge to outer edge is exactly w x h.
fn outline(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, stroke: u32) {
    fill(img, x, y, w, stroke, BLACK);
    fill(img, x, y + h - stroke, w, stroke, BLACK);
    fill(img, x, y, stroke, h, BLACK);
    fill(img, x + w - stroke, y, stroke, h, BLACK);
}

fn text_width(font: &FontRef, scale: PxScale, text: &str) -> f32 {
    let scaled = font.as_scaled(scale);
    let mut width = 0.0;
    let mut prev = None;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(prev) = prev {
            width += scaled.kern(prev, id);
        }
        width += scaled.h_advance(id);
        prev = Some(id);
    }
    width
}

/// Draws `text` centred horizontally with its baseline at `baseline_y`,
/// shrinking it to fit `max_width` if needed.
fn centered(
    img: &mut RgbImage,
    font: &FontRef,
    scale: PxScale,
    max_width: u32,
    baseline_y: u32,
    text: &str,
) {
    let mut scale = scale;
    let width = text_width(font, scale, text);
    if width > max_width as f32 {
        let factor = max_width as f32 / width;
        scale = PxScale::from(scale.y * factor);
    }
    let width = text_width(font, scale, text);
    let x = ((img.width() as f32 - width) / 2.0).max(0.0) as u32;
    draw_text(img, font, scale, x, baseline_y, text);
}

/// Draws black text with its baseline at `baseline_y`, blending
/// anti-aliased edges into what's already there.
fn draw_text(
    img: &mut RgbImage,
    font: &FontRef,
    scale: PxScale,
    x: u32,
    baseline_y: u32,
    text: &str,
) {
    let scaled = font.as_scaled(scale);
    let mut caret = x as f32;
    let mut prev = None;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(prev) = prev {
            caret += scaled.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(scale, point(caret, baseline_y as f32));
        caret += scaled.h_advance(id);
        prev = Some(id);

        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|gx, gy, coverage| {
            let px = bounds.min.x as i64 + gx as i64;
            let py = bounds.min.y as i64 + gy as i64;
            if px < 0 || py < 0 || px >= img.width() as i64 || py >= img.height() as i64 {
                return;
            }
            let pixel = img.get_pixel_mut(px as u32, py as u32);
            for channel in pixel.0.iter_mut() {
                *channel = (*channel as f32 * (1.0 - coverage.clamp(0.0, 1.0))).round() as u8;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::PAPER_SIZES;
    use std::path::PathBuf;

    fn reference_path(slug: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("docs")
            .join("test-sheets")
            .join(format!("hello-print-test-sheet-{slug}.png"))
    }

    #[test]
    fn sheet_is_exact_size_with_frame_and_centred_square() {
        let img = render(101.6, 152.4, "4 x 6 in");
        assert_eq!((img.width(), img.height()), (1200, 1800));
        // Frame on every edge.
        assert_eq!(*img.get_pixel(0, 900), BLACK);
        assert_eq!(*img.get_pixel(1199, 900 + 200), BLACK);
        // Band just inside the frame.
        assert_eq!(*img.get_pixel(10, 300), BAND);
        // 50 mm (591 px) square, outer edges at 304 and 894 horizontally.
        assert_eq!(*img.get_pixel(304, 900), BLACK);
        assert_eq!(*img.get_pixel(303, 900), WHITE);
        assert_eq!(*img.get_pixel(304 + 591 - 1, 900), BLACK);
        assert_eq!(*img.get_pixel(304 + 591, 900), WHITE);
    }

    /// The PNGs in docs/test-sheets must match the generator. Regenerate them
    /// with `UPDATE_TEST_SHEETS=1 cargo test test_sheet`.
    #[test]
    fn reference_sheets_are_up_to_date() {
        let update = std::env::var_os("UPDATE_TEST_SHEETS").is_some();
        for size in PAPER_SIZES {
            let img = render(size.width_mm, size.height_mm, size.label);
            let path = reference_path(size.slug);
            if update {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                img.save(&path).unwrap();
                continue;
            }
            let saved = image::open(&path)
                .unwrap_or_else(|e| {
                    panic!("{}: {e} (run with UPDATE_TEST_SHEETS=1)", path.display())
                })
                .to_rgb8();
            assert!(
                saved == img,
                "{} is out of date; run with UPDATE_TEST_SHEETS=1",
                path.display()
            );
        }
    }
}
