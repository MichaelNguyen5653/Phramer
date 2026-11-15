//! Image files and clipboard bitmaps from BGRA pixels.

use crate::error::OverlayError;

/// A file format the export can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
    /// Lossless WebP.
    WebP,
    /// One page holding the image at 96 DPI, losslessly compressed.
    Pdf,
}

impl ImageFormat {
    /// The format for a file extension, `None` for one this build cannot
    /// write.
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "bmp" => Some(Self::Bmp),
            "webp" => Some(Self::WebP),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    /// The extension a file of this format gets (`jpg` for JPEG).
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Bmp => "bmp",
            Self::WebP => "webp",
            Self::Pdf => "pdf",
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

    /// The image is opaque: RGB without the alpha bytes.
    fn rgb(&self) -> Vec<u8> {
        self.bgra
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[2], pixel[1], pixel[0]])
            .collect()
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
        ImageFormat::WebP => encode_webp(image),
        ImageFormat::Pdf => encode_pdf(image),
    }
}

/// Lossless WebP (VP8L). WebP holds at most 16384 pixels a side.
fn encode_webp(image: &CapturedImage) -> Result<Vec<u8>, OverlayError> {
    let mut bytes = Vec::new();
    image_webp::WebPEncoder::new(&mut bytes)
        .encode(
            &image.rgb(),
            image.width,
            image.height,
            image_webp::ColorType::Rgb8,
        )
        .map_err(|error| match error {
            image_webp::EncodingError::InvalidDimensions => OverlayError::Encode(format!(
                "{}x{} is too large for WebP",
                image.width, image.height
            )),
            other => OverlayError::Encode(other.to_string()),
        })?;
    Ok(bytes)
}

/// `pixels` as PDF points at 96 DPI (0.75 pt each), written exactly:
/// a quarter point is the finest step.
fn points(pixels: u32) -> String {
    let quarters = u64::from(pixels) * 3;
    let whole = quarters / 4;
    match quarters % 4 {
        1 => format!("{whole}.25"),
        2 => format!("{whole}.5"),
        3 => format!("{whole}.75"),
        _ => whole.to_string(),
    }
}

/// A PDF of one page the size of the image at 96 DPI, which shows the image
/// at full resolution: 8-bit RGB, Flate-compressed, so nothing is lost.
///
/// Written by hand: the document is always the same five objects, so a PDF
/// library would add a dependency for what fits in this function.
fn encode_pdf(image: &CapturedImage) -> Result<Vec<u8>, OverlayError> {
    use std::io::Write as _;

    let failed = |error: std::io::Error| OverlayError::Encode(error.to_string());
    // Fast, as for PNG: the user waits for the save
    let mut deflate = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    deflate.write_all(&image.rgb()).map_err(failed)?;
    let pixels = deflate.finish().map_err(failed)?;
    let (width, height) = (points(image.width), points(image.height));
    let content = format!("q {width} 0 0 {height} 0 0 cm /Im0 Do Q\n");

    // A binary comment after the header marks the file as binary
    let mut pdf = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    let mut object = |pdf: &mut Vec<u8>, head: String, stream: Option<&[u8]>| {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{head}\n", offsets.len()).as_bytes());
        if let Some(stream) = stream {
            pdf.extend_from_slice(b"stream\n");
            pdf.extend_from_slice(stream);
            pdf.extend_from_slice(b"\nendstream\n");
        }
        pdf.extend_from_slice(b"endobj\n");
    };
    object(&mut pdf, "<< /Type /Catalog /Pages 2 0 R >>".into(), None);
    object(
        &mut pdf,
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        None,
    );
    object(
        &mut pdf,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] \
             /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>"
        ),
        None,
    );
    object(
        &mut pdf,
        format!("<< /Length {} >>", content.len()),
        Some(content.as_bytes()),
    );
    object(
        &mut pdf,
        format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} \
             /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>",
            image.width,
            image.height,
            pixels.len()
        ),
        Some(&pixels),
    );
    let xref = pdf.len();
    let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
    for offset in &offsets {
        table.push_str(&format!("{offset:010} 00000 n \n"));
    }
    table.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        offsets.len() + 1
    ));
    pdf.extend_from_slice(table.as_bytes());
    Ok(pdf)
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

    /// 37x23 of varied colours (odd sizes catch row and stride mistakes).
    fn varied() -> CapturedImage {
        let (width, height) = (37u32, 23u32);
        let bgra = (0..width * height)
            .flat_map(|i| {
                let (x, y) = (i % width, i / width);
                [(x * 7) as u8, (y * 11) as u8, ((x ^ y) * 5) as u8, 255]
            })
            .collect();
        CapturedImage {
            width,
            height,
            bgra,
        }
    }

    #[test]
    fn extensions_name_each_format_and_back() {
        for format in [
            ImageFormat::Png,
            ImageFormat::Jpeg,
            ImageFormat::Bmp,
            ImageFormat::WebP,
            ImageFormat::Pdf,
        ] {
            assert_eq!(
                ImageFormat::from_extension(format.extension()),
                Some(format)
            );
        }
        assert_eq!(ImageFormat::from_extension("JPEG"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("WebP"), Some(ImageFormat::WebP));
        for other in ["tif", "tiff", "gif", "ico", ""] {
            assert_eq!(ImageFormat::from_extension(other), None, "{other}");
        }
    }

    #[test]
    fn webp_is_lossless() {
        let image = varied();
        let bytes = encode(&image, ImageFormat::WebP, 75).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WEBPVP8L");
        let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(decoder.dimensions(), (image.width, image.height));
        assert!(!decoder.has_alpha());
        let mut rgb = vec![0; decoder.output_buffer_size().unwrap()];
        decoder.read_image(&mut rgb).unwrap();
        assert_eq!(rgb, image.rgb());
    }

    #[test]
    fn webp_refuses_what_it_cannot_hold() {
        let image = CapturedImage {
            width: 16385,
            height: 1,
            bgra: vec![255; 16385 * 4],
        };
        let error = encode(&image, ImageFormat::WebP, 75).unwrap_err();
        assert!(
            error.to_string().contains("16385x1 is too large for WebP"),
            "{error}"
        );
    }

    #[test]
    fn bmp_is_lossless() {
        let image = varied();
        let bytes = encode(&image, ImageFormat::Bmp, 75).unwrap();
        let offset = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
        let stride = image.width as usize * 4;
        // Bottom-up BGRX rows: turned over, they are the image
        let rows: Vec<&[u8]> = bytes[offset..].chunks_exact(stride).rev().collect();
        assert_eq!(rows.len(), image.height as usize);
        for (y, row) in rows.iter().enumerate() {
            for x in 0..image.width as usize {
                let at = y * stride + x * 4;
                assert_eq!(&row[x * 4..x * 4 + 3], &image.bgra[at..at + 3], "{x},{y}");
            }
        }
    }

    #[test]
    fn jpeg_decodes_close_to_the_image() {
        let image = varied();
        let bytes = encode(&image, ImageFormat::Jpeg, 95).unwrap();
        let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
        let rgb = decoder.decode().unwrap();
        let info = decoder.info().unwrap();
        assert_eq!(
            (u32::from(info.width), u32::from(info.height)),
            (image.width, image.height)
        );
        let expected = image.rgb();
        assert_eq!(rgb.len(), expected.len());
        // Lossy: each channel within a tolerance, and close on average
        let differences: Vec<u32> = rgb
            .iter()
            .zip(&expected)
            .map(|(a, b)| u32::from(a.abs_diff(*b)))
            .collect();
        let worst = differences.iter().copied().max().unwrap_or(0);
        let mean = differences.iter().sum::<u32>() as f64 / differences.len() as f64;
        assert!(worst <= 48 && mean <= 4.0, "worst {worst}, mean {mean}");
    }

    #[test]
    fn pdf_pages_are_the_image_at_96_dpi() {
        assert_eq!(points(1920), "1440");
        assert_eq!(points(1), "0.75");
        assert_eq!(points(2), "1.5");
        assert_eq!(points(3), "2.25");
        let image = varied();
        let bytes = encode(&image, ImageFormat::Pdf, 75).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.4\n"));
        // Every cross-reference entry points at its object, and startxref
        // at the table (a reader may otherwise rebuild it and hide a fault)
        let marker = b"startxref\n";
        let at = bytes
            .windows(marker.len())
            .rposition(|window| window == marker)
            .unwrap();
        let tail = String::from_utf8(bytes[at + marker.len()..].to_vec()).unwrap();
        let xref: usize = tail.lines().next().unwrap().parse().unwrap();
        let table = String::from_utf8(bytes[xref..].to_vec()).unwrap();
        assert!(table.starts_with("xref\n0 6\n0000000000 65535 f \n"));
        for (number, entry) in table.split_inclusive('\n').skip(3).take(5).enumerate() {
            assert_eq!(entry.len(), 20, "{entry:?}");
            let offset: usize = entry[..10].parse().unwrap();
            let object = format!("{} 0 obj\n", number + 1);
            assert!(bytes[offset..].starts_with(object.as_bytes()), "{entry:?}");
        }
        let document = lopdf::Document::load_mem(&bytes).unwrap();
        let pages = document.get_pages();
        assert_eq!(pages.len(), 1);
        let page = document.get_dictionary(pages[&1]).unwrap();
        let media: Vec<f32> = page
            .get(b"MediaBox")
            .and_then(lopdf::Object::as_array)
            .unwrap()
            .iter()
            .map(|value| value.as_float().unwrap())
            .collect();
        // 37 x 23 pixels at 0.75 pt each
        assert_eq!(media, [0.0, 0.0, 27.75, 17.25]);
        let images = document.get_page_images(pages[&1]).unwrap();
        assert_eq!(images.len(), 1);
        let embedded = &images[0];
        assert_eq!((embedded.width, embedded.height), (37, 23));
        assert_eq!(embedded.bits_per_component, Some(8));
        assert_eq!(embedded.color_space.as_deref(), Some("DeviceRGB"));
        assert_eq!(
            embedded.filters.as_deref(),
            Some(&["FlateDecode".to_owned()][..])
        );
        // Full resolution and lossless: the stream inflates to the pixels
        let stream = document
            .get_object(embedded.id)
            .and_then(lopdf::Object::as_stream)
            .unwrap();
        assert_eq!(stream.decompressed_content().unwrap(), image.rgb());
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
