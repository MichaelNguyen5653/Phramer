//! Image files and clipboard bitmaps from BGRA pixels.

use crate::error::OverlayError;

/// A file format the export can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
}

impl ImageFormat {
    /// The format for a file extension, `None` for one this build cannot
    /// write.
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "bmp" => Some(Self::Bmp),
            _ => None,
        }
    }
}

/// A captured image: 8-bit BGRA, top row first, opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl CapturedImage {
    /// The part of a BGRA frame inside `x, y, width, height` (pixels), which
    /// must lie within the frame.
    pub fn crop(
        frame: &[u8],
        frame_width: u32,
        (x, y, width, height): (u32, u32, u32, u32),
    ) -> Self {
        let stride = frame_width as usize * 4;
        let mut bgra = Vec::with_capacity(width as usize * height as usize * 4);
        for row in y..y + height {
            let start = row as usize * stride + x as usize * 4;
            bgra.extend_from_slice(&frame[start..start + width as usize * 4]);
        }
        Self {
            width,
            height,
            bgra,
        }
    }

    fn rgba(&self) -> Vec<u8> {
        let mut rgba = self.bgra.clone();
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        rgba
    }
}

/// The image as a file of `format`. `jpeg_quality` is 0 to 100.
pub fn encode(
    image: &CapturedImage,
    format: ImageFormat,
    jpeg_quality: u8,
) -> Result<Vec<u8>, OverlayError> {
    match format {
        ImageFormat::Png => encode_png(image),
        ImageFormat::Jpeg => encode_jpeg(image, jpeg_quality),
        ImageFormat::Bmp => Ok(encode_bmp(image)),
    }
}

fn encode_png(image: &CapturedImage) -> Result<Vec<u8>, OverlayError> {
    let failed = |error: png::EncodingError| OverlayError::Encode(error.to_string());
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // A screenshot is saved while the user waits; fdeflate's fast mode is
    // several times quicker than the balanced level for a few percent size
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(failed)?;
    writer.write_image_data(&image.rgba()).map_err(failed)?;
    writer.finish().map_err(failed)?;
    Ok(bytes)
}

fn encode_jpeg(image: &CapturedImage, quality: u8) -> Result<Vec<u8>, OverlayError> {
    let too_large = || {
        OverlayError::Encode(format!(
            "{}x{} is too large for JPEG",
            image.width, image.height
        ))
    };
    let width = u16::try_from(image.width).map_err(|_| too_large())?;
    let height = u16::try_from(image.height).map_err(|_| too_large())?;
    let mut bytes = Vec::new();
    jpeg_encoder::Encoder::new(&mut bytes, quality.clamp(1, 100))
        .encode(&image.bgra, width, height, jpeg_encoder::ColorType::Bgra)
        .map_err(|error| OverlayError::Encode(error.to_string()))?;
    Ok(bytes)
}

/// A BMP file: the file header and a 32-bit `CF_DIB`.
fn encode_bmp(image: &CapturedImage) -> Vec<u8> {
    let dib = dib(image);
    let mut bytes = Vec::with_capacity(14 + dib.len());
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&u32_le(14 + dib.len()));
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&u32_le(14 + INFO_HEADER_SIZE));
    bytes.extend_from_slice(&dib);
    bytes
}

const INFO_HEADER_SIZE: usize = 40;
const V5_HEADER_SIZE: usize = 124;
const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;
/// `LCS_sRGB`: the colour space tag `'sRGB'`.
const LCS_SRGB: u32 = 0x7352_4742;
/// `LCS_GM_IMAGES`.
const INTENT_IMAGES: u32 = 4;
/// 96 DPI in pixels per metre, what Windows writes for screen images.
const PIXELS_PER_METRE: u32 = 3780;

fn u32_le(value: usize) -> [u8; 4] {
    u32::try_from(value).unwrap_or(u32::MAX).to_le_bytes()
}

/// Rows bottom-up, as a DIB with a positive height stores them.
fn bottom_up(image: &CapturedImage) -> impl Iterator<Item = &[u8]> {
    image
        .bgra
        .chunks_exact(image.width.max(1) as usize * 4)
        .rev()
}

/// `CF_DIB`: a `BITMAPINFOHEADER` and 32-bit BGRX rows, bottom-up.
pub fn dib(image: &CapturedImage) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(INFO_HEADER_SIZE + image.bgra.len());
    bytes.extend_from_slice(&u32_le(INFO_HEADER_SIZE));
    bytes.extend_from_slice(&image.width.to_le_bytes());
    bytes.extend_from_slice(&image.height.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&BI_RGB.to_le_bytes());
    bytes.extend_from_slice(&u32_le(image.bgra.len()));
    bytes.extend_from_slice(&PIXELS_PER_METRE.to_le_bytes());
    bytes.extend_from_slice(&PIXELS_PER_METRE.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    for row in bottom_up(image) {
        bytes.extend_from_slice(row);
    }
    bytes
}

/// `CF_DIBV5`: a `BITMAPV5HEADER` with explicit BGRA masks and sRGB, so
/// readers that honour alpha see an opaque image.
pub fn dib_v5(image: &CapturedImage) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(V5_HEADER_SIZE + image.bgra.len());
    bytes.extend_from_slice(&u32_le(V5_HEADER_SIZE));
    bytes.extend_from_slice(&image.width.to_le_bytes());
    bytes.extend_from_slice(&image.height.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&BI_BITFIELDS.to_le_bytes());
    bytes.extend_from_slice(&u32_le(image.bgra.len()));
    bytes.extend_from_slice(&PIXELS_PER_METRE.to_le_bytes());
    bytes.extend_from_slice(&PIXELS_PER_METRE.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    for mask in [0x00ff_0000u32, 0x0000_ff00, 0x0000_00ff, 0xff00_0000] {
        bytes.extend_from_slice(&mask.to_le_bytes());
    }
    bytes.extend_from_slice(&LCS_SRGB.to_le_bytes());
    // Endpoints (36 bytes) and gamma (12) are unused with sRGB
    bytes.extend_from_slice(&[0; 48]);
    bytes.extend_from_slice(&INTENT_IMAGES.to_le_bytes());
    // Profile data, profile size, reserved
    bytes.extend_from_slice(&[0; 12]);
    for row in bottom_up(image) {
        bytes.extend_from_slice(row);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> CapturedImage {
        // 2x2: blue, green / red, white
        CapturedImage {
            width: 2,
            height: 2,
            bgra: vec![
                255, 0, 0, 255, 0, 255, 0, 255, //
                0, 0, 255, 255, 255, 255, 255, 255,
            ],
        }
    }

    #[test]
    fn crops_rows_of_a_larger_frame() {
        let frame: Vec<u8> = (0..4 * 3 * 4).map(|i| i as u8).collect();
        let cropped = CapturedImage::crop(&frame, 4, (1, 1, 2, 2));
        assert_eq!((cropped.width, cropped.height), (2, 2));
        assert_eq!(&cropped.bgra[..4], &frame[20..24]);
        assert_eq!(&cropped.bgra[8..12], &frame[36..40]);
    }

    #[test]
    fn dibs_are_bottom_up_with_the_documented_header_sizes() {
        let dib = dib(&image());
        assert_eq!(dib.len(), 40 + 16);
        assert_eq!(&dib[..4], &40u32.to_le_bytes());
        // The bottom row (red, white) comes first
        assert_eq!(&dib[40..44], &[0, 0, 255, 255]);
        let v5 = dib_v5(&image());
        assert_eq!(v5.len(), 124 + 16);
        assert_eq!(&v5[..4], &124u32.to_le_bytes());
        assert_eq!(&v5[124..128], &[0, 0, 255, 255]);
    }

    #[test]
    fn png_round_trips() {
        let bytes = encode(&image(), ImageFormat::Png, 75).unwrap_or_default();
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder
            .read_info()
            .unwrap_or_else(|error| panic!("{error}"));
        let mut pixels = vec![0; reader.output_buffer_size().unwrap_or(0)];
        reader
            .next_frame(&mut pixels)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(&pixels[..4], &[0, 0, 255, 255]);
        assert_eq!(&pixels[8..12], &[255, 0, 0, 255]);
    }

    #[test]
    fn bmp_and_jpeg_have_their_signatures() {
        let bmp = encode(&image(), ImageFormat::Bmp, 75).unwrap_or_default();
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(bmp.len(), 14 + 40 + 16);
        let jpeg = encode(&image(), ImageFormat::Jpeg, 75).unwrap_or_default();
        assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
    }
}
