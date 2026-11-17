//! Images over the engine's size limit, split into overlapping tiles and
//! the tiles' lines merged back (`ocrtiling.cpp`). Pure.
//!
//! Clamping the scale instead means a capture wider than the limit can only
//! ever be shrunk, exactly when its glyphs are already too small to read.
//! With tiles the whole image can be as large as the glyphs need, because
//! the engine never sees it whole.

use crate::{OcrLine, Rect, RgbaImage};

/// Most tiles one pass may be split into: each is an engine round trip.
pub const MAX_TILES: usize = 12;
/// Tiles advance by less than their width, leaving a 12% overlap, so a
/// line crossing a seam is whole in one tile.
const TILE_STEP: f64 = 0.88;
/// Boxes sharing more than this fraction of the smaller one's area are the
/// same line seen through two tiles.
const SAME_LINE_OVERLAP: f64 = 0.6;
/// Guards the reduction loop; 0.8^24 is small enough that any image fits.
const MAX_BUDGET_ATTEMPTS: usize = 24;

/// A tile in whole-image pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Pieces of a `width` x `height` image the engine will accept
/// (`ocrPlanTiles`). One tile covering everything when the image fits or
/// the engine has no limit (`max_dimension` 0); none for an empty image.
pub fn plan_tiles(width: u32, height: u32, max_dimension: u32) -> Vec<Tile> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    if max_dimension == 0 || (width <= max_dimension && height <= max_dimension) {
        return vec![Tile {
            x: 0,
            y: 0,
            width,
            height,
        }];
    }
    let tile_width = max_dimension.min(width);
    let tile_height = max_dimension.min(height);
    let step = ((f64::from(max_dimension) * TILE_STEP).round() as u32).max(1);
    let mut tiles: Vec<Tile> = Vec::new();
    let mut y = 0;
    while y < height {
        // The last row is pulled back against the edge rather than left as
        // a sliver that would cut every glyph in it in half
        let top = y.min(height - tile_height);
        let mut x = 0;
        while x < width {
            let left = x.min(width - tile_width);
            let tile = Tile {
                x: left,
                y: top,
                width: tile_width,
                height: tile_height,
            };
            if !tiles.contains(&tile) {
                tiles.push(tile);
            }
            if left + tile_width >= width {
                break;
            }
            x += step;
        }
        if top + tile_height >= height {
            break;
        }
        y += step;
    }
    tiles
}

/// The largest scale no greater than `scale` whose image stays within
/// [`MAX_TILES`] (`ocrScaleWithinTileBudget`): less magnified, but whole.
pub fn scale_within_tile_budget(width: u32, height: u32, scale: f64, max_dimension: u32) -> f64 {
    if width == 0 || height == 0 || max_dimension == 0 || scale <= 0.0 {
        return scale;
    }
    let mut scale = scale;
    for _ in 0..MAX_BUDGET_ATTEMPTS {
        let side = |pixels: u32| ((f64::from(pixels) * scale).round() as u32).max(1);
        if plan_tiles(side(width), side(height), max_dimension).len() <= MAX_TILES {
            return scale;
        }
        scale *= 0.8;
    }
    scale
}

/// The pixels of `tile`.
pub fn crop(image: &RgbaImage, tile: Tile) -> RgbaImage {
    let x = tile.x.min(image.width);
    let y = tile.y.min(image.height);
    let width = tile.width.min(image.width - x);
    let height = tile.height.min(image.height - y);
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for row in y..y + height {
        let start = (row as usize * image.width as usize + x as usize) * 4;
        rgba.extend_from_slice(&image.rgba[start..start + width as usize * 4]);
    }
    RgbaImage {
        width,
        height,
        rgba,
    }
}

/// Tile-local boxes moved into whole-image coordinates (`ocrRemapLines`).
pub fn remap_lines(lines: &[OcrLine], dx: f64, dy: f64) -> Vec<OcrLine> {
    let shift = |rect: &mut Rect| {
        rect.x += dx;
        rect.y += dy;
    };
    lines
        .iter()
        .cloned()
        .map(|mut line| {
            shift(&mut line.bounding_box);
            for word in &mut line.words {
                shift(&mut word.bounding_box);
            }
            line
        })
        .collect()
}

/// Lines two overlapping tiles both reported, collapsed to the longer
/// reading (`ocrMergeTiledLines`).
pub fn merge_tiled_lines(lines: &[OcrLine]) -> Vec<OcrLine> {
    let mut merged: Vec<OcrLine> = Vec::new();
    for line in lines {
        let same = merged.iter_mut().find(|kept| {
            let smaller = kept.bounding_box.area().min(line.bounding_box.area());
            smaller > 0.0
                && kept.bounding_box.intersected(&line.bounding_box).area() / smaller
                    >= SAME_LINE_OVERLAP
        });
        match same {
            Some(kept) => {
                // QString::size(): UTF-16 units
                if line.text.encode_utf16().count() > kept.text.encode_utf16().count() {
                    *kept = line.clone();
                }
            }
            None => merged.push(line.clone()),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OcrWord;

    /// Whether every pixel of the image falls inside a tile, sampled on a
    /// grid as 15.0.0's test did, plus the last row and column.
    fn covers_everything(width: u32, height: u32, tiles: &[Tile]) -> bool {
        let xs = (0..width).step_by(7).chain(Some(width - 1));
        xs.flat_map(|x| {
            (0..height)
                .step_by(7)
                .chain(Some(height - 1))
                .map(move |y| (x, y))
        })
        .all(|(x, y)| {
            tiles.iter().any(|tile| {
                x >= tile.x && x < tile.x + tile.width && y >= tile.y && y < tile.y + tile.height
            })
        })
    }

    fn line(text: &str, rect: Rect) -> OcrLine {
        OcrLine {
            text: text.into(),
            bounding_box: rect,
            words: Vec::new(),
        }
    }

    #[test]
    fn small_images_are_a_single_tile() {
        let tile = Tile {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        };
        assert_eq!(plan_tiles(800, 600, 2600), [tile]);
    }

    #[test]
    fn an_unlimited_engine_is_a_single_tile() {
        assert_eq!(plan_tiles(9000, 9000, 0).len(), 1);
        assert_eq!(plan_tiles(9000, 9000, 0)[0].width, 9000);
        assert!(plan_tiles(0, 10, 100).is_empty());
    }

    #[test]
    fn oversized_images_are_split_without_gaps() {
        let tiles = plan_tiles(5000, 3000, 2000);
        assert!(tiles.len() > 1);
        for tile in &tiles {
            assert!(tile.width <= 2000 && tile.height <= 2000);
            assert!(tile.x + tile.width <= 5000 && tile.y + tile.height <= 3000);
        }
        assert!(covers_everything(5000, 3000, &tiles));
        // 0, 1760, 3000 across; 0, 1000 down
        assert_eq!(tiles.len(), 6);
        assert_eq!(tiles.last().unwrap().x, 3000);
        assert_eq!(tiles.last().unwrap().y, 1000);
    }

    #[test]
    fn adjacent_tiles_overlap() {
        let tiles = plan_tiles(4000, 500, 2000);
        assert!(tiles.len() >= 2);
        assert!(tiles[0].x + tiles[0].width > tiles[1].x);
        // Never taller than the image
        assert!(tiles.iter().all(|tile| tile.height == 500));
    }

    #[test]
    fn a_scale_that_already_fits_is_untouched() {
        assert_eq!(scale_within_tile_budget(400, 300, 2.0, 2600), 2.0);
    }

    #[test]
    fn an_extreme_scale_is_reduced_to_the_tile_budget() {
        let scale = scale_within_tile_budget(3840, 2160, 6.0, 2600);
        assert!(scale < 6.0);
        let side = |pixels: u32| (f64::from(pixels) * scale).round() as u32;
        assert!(plan_tiles(side(3840), side(2160), 2600).len() <= MAX_TILES);
    }

    #[test]
    fn crops_take_the_tiles_pixels() {
        let image = RgbaImage {
            width: 3,
            height: 2,
            rgba: (0..24).collect(),
        };
        let piece = crop(
            &image,
            Tile {
                x: 1,
                y: 1,
                width: 2,
                height: 1,
            },
        );
        assert_eq!((piece.width, piece.height), (2, 1));
        assert_eq!(piece.rgba, (16..24).collect::<Vec<u8>>());
    }

    #[test]
    fn remapping_translates_line_and_word_boxes() {
        let words = vec![OcrWord {
            text: "abc".into(),
            bounding_box: Rect::new(0.0, 5.0, 30.0, 10.0),
        }];
        let remapped = remap_lines(&[OcrLine::from_words(words)], 100.0, 200.0);
        assert_eq!(
            remapped[0].bounding_box,
            Rect::new(100.0, 205.0, 30.0, 10.0)
        );
        assert_eq!(
            remapped[0].words[0].bounding_box,
            Rect::new(100.0, 205.0, 30.0, 10.0)
        );
    }

    #[test]
    fn seam_duplicates_collapse_to_the_longer_reading() {
        let merged = merge_tiled_lines(&[
            line("full sentence here", Rect::new(0.0, 0.0, 200.0, 20.0)),
            line("full sen", Rect::new(0.0, 0.0, 190.0, 20.0)),
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].text, "full sentence here");

        // The longer one arriving second replaces the shorter
        let merged = merge_tiled_lines(&[
            line("ull sentence", Rect::new(10.0, 0.0, 180.0, 20.0)),
            line("full sentence here", Rect::new(0.0, 0.0, 200.0, 20.0)),
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].text, "full sentence here");
    }

    #[test]
    fn distinct_neighbouring_lines_are_not_merged() {
        let merged = merge_tiled_lines(&[
            line("first", Rect::new(0.0, 0.0, 100.0, 20.0)),
            line("second", Rect::new(0.0, 24.0, 100.0, 20.0)),
            line("beside", Rect::new(120.0, 0.0, 100.0, 20.0)),
        ]);
        assert_eq!(merged.len(), 3);
    }
}
