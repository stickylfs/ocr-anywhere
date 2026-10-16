use crate::capture::screen::CapturedRegion;

/// Blank border added around the selection: text touching the image edge is
/// often missed by the recognizer.
const PADDING: usize = 16;

/// Height the enhanced pass scales small selections to (up to 4x): enough for
/// ~10 px text, while larger factors only blur the glyphs.
const ENHANCED_TARGET_HEIGHT: f32 = 64.0;
const ENHANCED_MAX_SCALE: f32 = 4.0;

/// Prepares a captured region for `Windows.Media.Ocr`: adaptively rescales it and
/// adds a border filled with the surrounding background color.
///
/// The recognizer works best when characters are at least ~30 px tall, while
/// standard UI text at 100% scaling is only ~10 px, so small selections are
/// upscaled 2x (which also helps telling accents, colons and digits apart).
/// The result never exceeds `max_dimension` (`OcrEngine::MaxImageDimension`):
/// larger selections are scaled down instead of being rejected.
pub fn preprocess_region_for_ocr(region: &CapturedRegion, max_dimension: u32) -> CapturedRegion {
    let h = region.height;
    let scale = if h < 600 {
        2.0f32
    } else if h < 800 {
        1.5
    } else {
        1.0
    };
    resample_padded(region, scale, max_dimension)
}

/// Second attempt for selections where nothing was recognized: tiny or
/// low-contrast text such as desktop icon labels (white text with a shadow on
/// a wallpaper). Upscales small selections further, converts to grayscale and
/// stretches the contrast. (The recognizer reads both polarities, so an
/// inverted pass adds nothing.)
pub fn enhance_region_for_ocr(region: &CapturedRegion, max_dimension: u32) -> CapturedRegion {
    let h = region.height.max(1) as f32;
    let scale = (ENHANCED_TARGET_HEIGHT / h).clamp(2.0, ENHANCED_MAX_SCALE);
    let mut out = resample_padded(region, scale, max_dimension);
    stretch_contrast(&mut out);
    out
}

/// Scales `region` by `scale` (bilinear, clamped to `max_dimension`) and adds a
/// [`PADDING`] border of the background color. Upscaled images are mildly
/// sharpened.
fn resample_padded(region: &CapturedRegion, scale: f32, max_dimension: u32) -> CapturedRegion {
    let w = region.width as usize;
    let h = region.height as usize;
    if w == 0 || h == 0 || region.bgra_data.len() < w * h * 4 {
        return region.clone();
    }

    let mut scale = scale;
    let usable = (max_dimension as usize).saturating_sub(PADDING * 2).max(1) as f32;
    let longest = w.max(h) as f32;
    if longest * scale > usable {
        scale = usable / longest;
    }

    let scaled_w = ((w as f32 * scale).round() as usize).max(1);
    let scaled_h = ((h as f32 * scale).round() as usize).max(1);
    let out_w = scaled_w + PADDING * 2;
    let out_h = scaled_h + PADDING * 2;

    // Background color: per-channel median of the selection border, robust to
    // text or shadows touching the edges of a tight selection.
    let px = |x: usize, y: usize| &region.bgra_data[(y * w + x) * 4..(y * w + x) * 4 + 4];
    let border: Vec<&[u8]> = (0..w)
        .flat_map(|x| [px(x, 0), px(x, h - 1)])
        .chain((0..h).flat_map(|y| [px(0, y), px(w - 1, y)]))
        .collect();
    let mut background = [0u8, 0, 0, 255];
    for (c, slot) in background.iter_mut().enumerate().take(3) {
        let mut values: Vec<u8> = border.iter().map(|p| p[c]).collect();
        values.sort_unstable();
        *slot = values[values.len() / 2];
    }

    let mut out: Vec<u8> = background
        .iter()
        .copied()
        .cycle()
        .take(out_w * out_h * 4)
        .collect();

    // Bilinear resampling
    for y in 0..scaled_h {
        let src_y = y as f32 / scale;
        let y0 = (src_y.floor() as usize).min(h - 1);
        let y1 = (y0 + 1).min(h - 1);
        let dy = src_y - y0 as f32;
        let dst_row = ((y + PADDING) * out_w + PADDING) * 4;

        for x in 0..scaled_w {
            let src_x = x as f32 / scale;
            let x0 = (src_x.floor() as usize).min(w - 1);
            let x1 = (x0 + 1).min(w - 1);
            let dx = src_x - x0 as f32;
            let (p00, p10, p01, p11) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));

            for c in 0..4 {
                let val = (1.0 - dx) * (1.0 - dy) * p00[c] as f32
                    + dx * (1.0 - dy) * p10[c] as f32
                    + (1.0 - dx) * dy * p01[c] as f32
                    + dx * dy * p11[c] as f32;
                out[dst_row + x * 4 + c] = val.round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    // Mild sharpening after upscaling keeps small glyphs (colons, accents) separated.
    if scale > 1.0 {
        let source = out.clone();
        for y in 1..out_h - 1 {
            for x in 1..out_w - 1 {
                let idx = (y * out_w + x) * 4;
                for c in 0..3 {
                    let at = |xx: usize, yy: usize| source[(yy * out_w + xx) * 4 + c] as f32;
                    let sharp = at(x, y) * 1.25
                        - (at(x, y - 1) + at(x, y + 1) + at(x - 1, y) + at(x + 1, y)) * 0.0625;
                    out[idx + c] = sharp.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }

    CapturedRegion {
        width: out_w as u32,
        height: out_h as u32,
        bgra_data: out,
    }
}

/// Converts to grayscale and maps the 2nd–98th luminance percentiles to the
/// full range, so faint text stands out from its background.
fn stretch_contrast(region: &mut CapturedRegion) {
    let luma: Vec<u8> = region
        .bgra_data
        .chunks_exact(4)
        .map(|p| ((p[2] as u32 * 299 + p[1] as u32 * 587 + p[0] as u32 * 114) / 1000) as u8)
        .collect();
    if luma.is_empty() {
        return;
    }

    let mut histogram = [0usize; 256];
    for &l in &luma {
        histogram[l as usize] += 1;
    }
    let percentile = |fraction: f32| {
        let target = (luma.len() as f32 * fraction) as usize;
        let mut seen = 0;
        for (value, count) in histogram.iter().enumerate() {
            seen += count;
            if seen > target {
                return value as f32;
            }
        }
        255.0
    };
    let low = percentile(0.02);
    let high = percentile(0.98).max(low + 1.0);

    for (pixel, &l) in region.bgra_data.chunks_exact_mut(4).zip(&luma) {
        let v = ((l as f32 - low) * 255.0 / (high - low)).clamp(0.0, 255.0) as u8;
        pixel[0] = v;
        pixel[1] = v;
        pixel[2] = v;
        pixel[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32) -> CapturedRegion {
        CapturedRegion {
            width,
            height,
            bgra_data: vec![200; (width * height * 4) as usize],
        }
    }

    #[test]
    fn test_small_selection_is_upscaled_and_padded() {
        let out = preprocess_region_for_ocr(&solid(100, 20), 10_000);
        assert_eq!((out.width, out.height), (232, 72));
        // Padding uses the background color.
        assert_eq!(&out.bgra_data[..4], &[200, 200, 200, 255]);
    }

    #[test]
    fn test_large_selection_fits_engine_limit() {
        let out = preprocess_region_for_ocr(&solid(5000, 300), 2600);
        assert!(out.width <= 2600 && out.height <= 2600);
        assert_eq!(out.width, 2600);
    }

    #[test]
    fn test_degenerate_input_is_returned_unchanged() {
        let region = CapturedRegion {
            width: 10,
            height: 10,
            bgra_data: vec![0; 8],
        };
        assert_eq!(preprocess_region_for_ocr(&region, 2600).bgra_data.len(), 8);
    }

    #[test]
    fn test_enhanced_pass_upscales_tiny_text_and_stretches_contrast() {
        // A faint mid-gray line on a slightly lighter background.
        let (w, h) = (40u32, 20u32);
        let mut data = vec![180u8; (w * h * 4) as usize];
        for x in 0..w as usize {
            let i = (10 * w as usize + x) * 4;
            data[i..i + 3].copy_from_slice(&[150, 150, 150]);
        }
        let region = CapturedRegion {
            width: w,
            height: h,
            bgra_data: data,
        };

        let out = enhance_region_for_ocr(&region, 10_000);
        assert_eq!(
            out.height,
            64 + 32,
            "scaled to the target height, plus padding"
        );
        let values: Vec<u8> = out.bgra_data.chunks_exact(4).map(|p| p[0]).collect();
        assert_eq!(*values.iter().min().unwrap(), 0);
        assert_eq!(*values.iter().max().unwrap(), 255);
    }
}
