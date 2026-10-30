//! Image comparison for visual regression tests; slides are rendered with
//! [`SlideRenderer`] from `ppt-offscreen`.

use image::{Rgba, RgbaImage};
pub use ppt_offscreen::{FALLBACK_FAMILIES, SlideRenderer};

/// A channel difference above this counts as a mismatch.
pub const PIXEL_THRESHOLD: u8 = 48;

#[derive(Clone, Copy, Debug)]
pub struct Diff {
    /// Mismatched pixels, in percent of the page.
    pub score: f64,
}

fn distance(p: &Rgba<u8>, q: &Rgba<u8>) -> u8 {
    (0..3).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0)
}

/// Whether `p` has a close match within one pixel of (x, y) in `other`, so
/// antialiasing and sub-pixel glyph placement don't count as differences.
fn matched_nearby(p: &Rgba<u8>, other: &RgbaImage, x: u32, y: u32) -> bool {
    let (w, h) = other.dimensions();
    (y.saturating_sub(1)..=(y + 1).min(h - 1))
        .any(|sy| (x.saturating_sub(1)..=(x + 1).min(w - 1)).any(|sx| distance(p, other.get_pixel(sx, sy)) <= PIXEL_THRESHOLD))
}

/// Compares two same-sized images; returns the score and a visualization
/// (the actual image faded, mismatches in red).
pub fn compare(expected: &RgbaImage, actual: &RgbaImage) -> (Diff, RgbaImage) {
    assert_eq!(expected.dimensions(), actual.dimensions(), "compare needs equal sizes");
    let (w, h) = expected.dimensions();
    let mut bad = 0u64;
    let vis = RgbaImage::from_fn(w, h, |x, y| {
        let (p, q) = (expected.get_pixel(x, y), actual.get_pixel(x, y));
        let same = distance(p, q) <= PIXEL_THRESHOLD || (matched_nearby(p, actual, x, y) && matched_nearby(q, expected, x, y));
        if same {
            let fade = |v: u8| 255 - (255 - v) / 4;
            Rgba([fade(q[0]), fade(q[1]), fade(q[2]), 255])
        } else {
            bad += 1;
            Rgba([255, 0, 0, 255])
        }
    });
    (Diff { score: bad as f64 * 100.0 / (w as f64 * h as f64) }, vis)
}

/// Places images side by side with a gray gutter.
pub fn side_by_side(images: &[&RgbaImage]) -> RgbaImage {
    const GAP: u32 = 8;
    let h = images.iter().map(|i| i.height()).max().unwrap_or(0);
    let w = images.iter().map(|i| i.width()).sum::<u32>() + GAP * images.len().saturating_sub(1) as u32;
    let mut out = RgbaImage::from_pixel(w, h, Rgba([128, 128, 128, 255]));
    let mut x = 0;
    for img in images {
        image::imageops::overlay(&mut out, *img, x as i64, 0);
        x += img.width() + GAP;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_images_score_zero() {
        let img = RgbaImage::from_fn(20, 10, |x, _| Rgba([(x * 10) as u8, 0, 0, 255]));
        assert_eq!(compare(&img, &img).0.score, 0.0);
    }

    #[test]
    fn a_shifted_block_is_a_mismatch() {
        let block = |x0: u32| RgbaImage::from_fn(40, 40, move |x, y| if (x0..x0 + 10).contains(&x) && y < 10 { Rgba([0, 0, 0, 255]) } else { Rgba([255; 4]) });
        let (d, _) = compare(&block(0), &block(20));
        assert!(d.score > 5.0, "{}", d.score);
        // A one-pixel shift only flags the hard edges, while a missing thin line is still caught.
        assert!(compare(&block(0), &block(1)).0.score < 1.0);
        let line = |on: bool| RgbaImage::from_fn(40, 40, move |x, _| if on && x == 20 { Rgba([0, 0, 0, 255]) } else { Rgba([255; 4]) });
        assert!(compare(&line(true), &line(false)).0.score > 2.0);
    }
}
