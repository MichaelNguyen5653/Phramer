//! What the capture goes through before recognition: 15.0.0's
//! `ocrpreprocess.cpp`. A screenshot has none of what OCR engines are tuned
//! for (dark themes, glyphs a few pixels tall), so it is inverted, padded
//! and scaled first.

use crate::{OcrLine, RgbaImage};

/// The glyph height the engine reads best, pixels.
const IDEAL_GLYPH_HEIGHT: f64 = 40.0;
const MIN_SCALE: f64 = 0.5;
const MAX_SCALE: f64 = 6.0;
const HISTOGRAM_BUCKETS: usize = 64;
const DARK_MEDIAN_CEILING: usize = 110;
/// Smaller images are padded: engines miss text touching a tiny image's
/// edge.
const PAD_MIN_SIDE: u32 = 64;
const PAD_BORDER: u32 = 8;
/// The side of the sample the background is judged on.
const SAMPLE_SIDE: u32 = 64;

/// `qGray`: 11/16/5 weights over 32.
fn gray([r, g, b, _]: [u8; 4]) -> u8 {
    ((u32::from(r) * 11 + u32::from(g) * 16 + u32::from(b) * 5) / 32) as u8
}

/// The image on a border of its corner colour when it is smaller than
/// `PAD_MIN_SIDE` either way.
pub fn pad(image: &RgbaImage) -> RgbaImage {
    if image.is_empty() || (image.width >= PAD_MIN_SIDE && image.height >= PAD_MIN_SIDE) {
        return image.clone();
    }
    let width = (image.width + 2 * PAD_BORDER).max(PAD_MIN_SIDE + 2 * PAD_BORDER);
    let height = (image.height + 2 * PAD_BORDER).max(PAD_MIN_SIDE + 2 * PAD_BORDER);
    let fill = image.pixel(0, 0);
    let mut rgba = fill.repeat(width as usize * height as usize);
    let row = image.width as usize * 4;
    for y in 0..image.height as usize {
        let to = ((y + PAD_BORDER as usize) * width as usize + PAD_BORDER as usize) * 4;
        if let (Some(dest), Some(source)) = (
            rgba.get_mut(to..to + row),
            image.rgba.get(y * row..(y + 1) * row),
        ) {
            dest.copy_from_slice(source);
        }
    }
    RgbaImage {
        width,
        height,
        rgba,
    }
}

/// Whether the background is dark (`ocrIsDarkBackground`): the most common
/// shade is in the darkest third and the median is dark too, judged on a
/// 64 x 64 sample.
pub fn is_dark_background(image: &RgbaImage) -> bool {
    if image.is_empty() {
        return false;
    }
    let sample = resize(image, SAMPLE_SIDE, SAMPLE_SIDE);
    let mut buckets = [0usize; HISTOGRAM_BUCKETS];
    for pixel in sample.rgba.as_chunks::<4>().0 {
        buckets[usize::from(gray(*pixel)) * HISTOGRAM_BUCKETS / 256] += 1;
    }
    let total: usize = buckets.iter().sum();
    let mut dominant = 0;
    let mut seen = 0;
    let mut median = None;
    for (index, count) in buckets.iter().enumerate() {
        if *count > buckets[dominant] {
            dominant = index;
        }
        seen += count;
        if median.is_none() && seen * 2 >= total {
            median = Some(index * 256 / HISTOGRAM_BUCKETS);
        }
    }
    dominant < HISTOGRAM_BUCKETS / 3 && median.is_some_and(|median| median < DARK_MEDIAN_CEILING)
}

/// Inverted when the background is dark, then padded
/// (`ocrNormalizeImage`).
pub fn normalize(image: &RgbaImage) -> RgbaImage {
    let mut prepared = image.clone();
    if is_dark_background(&prepared) {
        for pixel in prepared.rgba.as_chunks_mut::<4>().0 {
            for channel in &mut pixel[..3] {
                *channel = 255 - *channel;
            }
        }
    }
    pad(&prepared)
}

/// Black and white at Otsu's threshold (`ocrBinarize`).
pub fn binarize(image: &RgbaImage) -> RgbaImage {
    let grays: Vec<u8> = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| gray(*p))
        .collect();
    if grays.is_empty() {
        return image.clone();
    }
    let mut histogram = [0u64; 256];
    for value in &grays {
        histogram[usize::from(*value)] += 1;
    }
    let total = grays.len() as u64;
    let sum: u64 = histogram
        .iter()
        .enumerate()
        .map(|(value, count)| value as u64 * count)
        .sum();
    let (mut below_weight, mut below_sum) = (0u64, 0u64);
    let mut best = -1.0;
    let mut threshold = 127;
    for (value, count) in histogram.iter().enumerate() {
        below_weight += count;
        if below_weight == 0 {
            continue;
        }
        let above_weight = total - below_weight;
        if above_weight == 0 {
            break;
        }
        below_sum += value as u64 * count;
        let below_mean = below_sum as f64 / below_weight as f64;
        let above_mean = (sum - below_sum) as f64 / above_weight as f64;
        let variance = below_weight as f64
            * above_weight as f64
            * (below_mean - above_mean)
            * (below_mean - above_mean);
        if variance > best {
            best = variance;
            threshold = value;
        }
    }
    let rgba = grays
        .iter()
        .flat_map(|value| {
            let flat = if usize::from(*value) > threshold {
                255
            } else {
                0
            };
            [flat, flat, flat, 255]
        })
        .collect();
    RgbaImage {
        width: image.width,
        height: image.height,
        rgba,
    }
}

/// The image at `scale`, its longer side kept within `max_dimension` when
/// that is not 0 (`ocrScaleImage`).
pub fn scale(image: &RgbaImage, scale: f64, max_dimension: u32) -> RgbaImage {
    if image.is_empty() {
        return image.clone();
    }
    let mut scale = scale;
    if max_dimension > 0 {
        let longer = f64::from(image.width.max(image.height));
        scale = scale.min(f64::from(max_dimension) / longer);
    }
    if (0.99..=1.01).contains(&scale) {
        return image.clone();
    }
    let side = |pixels: u32| ((f64::from(pixels) * scale).round() as u32).max(1);
    resize(image, side(image.width), side(image.height))
}

/// The scale that brings the median glyph to the engine's ideal height
/// (`ocrIdealScale`); 1 when nothing was measured.
pub fn ideal_scale(lines: &[OcrLine]) -> f64 {
    let mut heights: Vec<f64> = Vec::new();
    for line in lines {
        if line.words.is_empty() {
            heights.push(line.bounding_box.height);
        } else {
            heights.extend(line.words.iter().map(|word| word.bounding_box.height));
        }
    }
    heights.retain(|height| *height > 0.0);
    if heights.is_empty() {
        return 1.0;
    }
    heights.sort_by(f64::total_cmp);
    let median = heights[heights.len() / 2];
    (IDEAL_GLYPH_HEIGHT / median).clamp(MIN_SCALE, MAX_SCALE)
}

/// Smooth resampling: an area average when shrinking, bilinear when
/// growing (Qt's `SmoothTransformation` does the same).
pub fn resize(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let (width, height) = (width.max(1), height.max(1));
    if image.is_empty() || (width == image.width && height == image.height) {
        return image.clone();
    }
    let step_x = f64::from(image.width) / f64::from(width);
    let step_y = f64::from(image.height) / f64::from(height);
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let color = if step_x > 1.0 || step_y > 1.0 {
                area_average(image, x, y, step_x, step_y)
            } else {
                bilinear(
                    image,
                    (f64::from(x) + 0.5) * step_x - 0.5,
                    (f64::from(y) + 0.5) * step_y - 0.5,
                )
            };
            rgba.extend_from_slice(&color);
        }
    }
    RgbaImage {
        width,
        height,
        rgba,
    }
}

fn area_average(image: &RgbaImage, x: u32, y: u32, step_x: f64, step_y: f64) -> [u8; 4] {
    let x0 = (f64::from(x) * step_x) as u32;
    let y0 = (f64::from(y) * step_y) as u32;
    let x1 = ((f64::from(x + 1) * step_x).ceil() as u32).clamp(x0 + 1, image.width);
    let y1 = ((f64::from(y + 1) * step_y).ceil() as u32).clamp(y0 + 1, image.height);
    let mut sum = [0u64; 4];
    let mut count = 0u64;
    for sy in y0..y1 {
        for sx in x0..x1 {
            for (total, channel) in sum.iter_mut().zip(image.pixel(sx, sy)) {
                *total += u64::from(channel);
            }
            count += 1;
        }
    }
    let count = count.max(1);
    sum.map(|total| ((total + count / 2) / count) as u8)
}

fn bilinear(image: &RgbaImage, sx: f64, sy: f64) -> [u8; 4] {
    let (sx, sy) = (sx.max(0.0), sy.max(0.0));
    let (x0, y0) = (sx.floor() as u32, sy.floor() as u32);
    let (fx, fy) = (sx - f64::from(x0), sy - f64::from(y0));
    let clamp_x = |x: u32| x.min(image.width - 1);
    let clamp_y = |y: u32| y.min(image.height - 1);
    let a = image.pixel(clamp_x(x0), clamp_y(y0));
    let b = image.pixel(clamp_x(x0 + 1), clamp_y(y0));
    let c = image.pixel(clamp_x(x0), clamp_y(y0 + 1));
    let d = image.pixel(clamp_x(x0 + 1), clamp_y(y0 + 1));
    let mut out = [0u8; 4];
    for (channel, value) in out.iter_mut().enumerate() {
        let top = f64::from(a[channel]) + (f64::from(b[channel]) - f64::from(a[channel])) * fx;
        let lower = f64::from(c[channel]) + (f64::from(d[channel]) - f64::from(c[channel])) * fx;
        *value = (top + (lower - top) * fy).round() as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OcrWord, Rect};

    fn solid(width: u32, height: u32, color: [u8; 4]) -> RgbaImage {
        RgbaImage {
            width,
            height,
            rgba: color.repeat(width as usize * height as usize),
        }
    }

    #[test]
    fn a_dark_background_is_inverted_and_a_light_one_kept() {
        let dark = solid(100, 100, [20, 20, 30, 255]);
        assert!(is_dark_background(&dark));
        assert_eq!(normalize(&dark).pixel(0, 0), [235, 235, 225, 255]);
        let light = solid(100, 100, [240, 240, 240, 255]);
        assert!(!is_dark_background(&light));
        assert_eq!(normalize(&light).pixel(5, 5), [240, 240, 240, 255]);
    }

    #[test]
    fn small_images_are_padded_with_their_corner_colour() {
        let mut image = solid(10, 100, [255, 255, 255, 255]);
        image.rgba[4..8].copy_from_slice(&[1, 2, 3, 255]);
        let padded = pad(&image);
        assert_eq!((padded.width, padded.height), (80, 116));
        assert_eq!(padded.pixel(0, 0), [255, 255, 255, 255]);
        assert_eq!(padded.pixel(9, 8), [1, 2, 3, 255]);
        let big = solid(64, 64, [0, 0, 0, 255]);
        assert_eq!(pad(&big), big);
    }

    #[test]
    fn binarize_splits_at_otsus_threshold() {
        let mut image = solid(4, 1, [40, 40, 40, 255]);
        image.rgba[8..12].copy_from_slice(&[200, 200, 200, 255]);
        image.rgba[12..16].copy_from_slice(&[180, 180, 180, 255]);
        let flat = binarize(&image);
        let values: Vec<u8> = flat.rgba.chunks(4).map(|p| p[0]).collect();
        assert_eq!(values, [0, 0, 255, 255]);
    }

    #[test]
    fn scaling_respects_the_engine_limit() {
        let image = solid(1000, 500, [9, 9, 9, 255]);
        let scaled = scale(&image, 4.0, 2600);
        assert_eq!((scaled.width, scaled.height), (2600, 1300));
        let same = scale(&image, 1.0, 0);
        assert_eq!((same.width, same.height), (1000, 500));
        let half = scale(&image, 0.5, 0);
        assert_eq!((half.width, half.height), (500, 250));
        assert_eq!(half.pixel(10, 10), [9, 9, 9, 255]);
    }

    #[test]
    fn the_ideal_scale_brings_glyphs_to_forty_pixels() {
        let word = |height: f64| OcrWord {
            text: "w".into(),
            bounding_box: Rect::new(0.0, 0.0, 10.0, height),
        };
        let lines = vec![OcrLine::from_words(vec![
            word(10.0),
            word(10.0),
            word(30.0),
        ])];
        assert_eq!(ideal_scale(&lines), 4.0);
        let tiny = vec![OcrLine::from_words(vec![word(2.0)])];
        assert_eq!(ideal_scale(&tiny), MAX_SCALE);
        assert_eq!(ideal_scale(&[]), 1.0);
    }

    #[test]
    fn resizing_interpolates() {
        let mut image = solid(2, 1, [0, 0, 0, 255]);
        image.rgba[4..8].copy_from_slice(&[200, 200, 200, 255]);
        let grown = resize(&image, 4, 1);
        let values: Vec<u8> = grown.rgba.chunks(4).map(|p| p[0]).collect();
        assert_eq!(values, [0, 50, 150, 200]);
        let shrunk = resize(&image, 1, 1);
        assert_eq!(shrunk.pixel(0, 0), [100, 100, 100, 255]);
    }
}
