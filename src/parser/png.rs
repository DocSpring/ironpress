//! Minimal PNG parser that extracts enough information for PDF embedding.
//!
//! PNG files contain IDAT chunks with zlib-compressed pixel data including
//! PNG row filters. PDF's FlateDecode filter with PNG predictors can handle
//! this natively, so we pass the raw IDAT data through.

/// PNG file signature (8 bytes).
pub const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// Maximum total size of accumulated IDAT data (50 MB).
pub const MAX_IDAT_SIZE: usize = 50 * 1024 * 1024;

/// Parsed information from a PNG file needed for PDF embedding.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PngInfo {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: u8,
    /// Number of color channels (1=Gray, 2=GrayAlpha, 3=RGB, 4=RGBA).
    pub channels: u8,
    /// Concatenated raw IDAT chunk data (zlib-compressed).
    pub idat_data: Vec<u8>,
}

impl PngInfo {
    pub const fn has_alpha(&self) -> bool {
        matches!(self.channels, 2 | 4)
    }
}

/// Check whether a byte slice starts with the PNG signature.
pub fn is_png(data: &[u8]) -> bool {
    data.len() >= 8 && data[..8] == PNG_SIGNATURE
}

/// Parse a PNG file and extract the information needed for PDF embedding.
///
/// Returns `None` if the data is not a valid PNG or cannot be parsed.
/// Accumulated IDAT data is limited to [`MAX_IDAT_SIZE`] bytes.
pub fn parse_png(data: &[u8]) -> Option<PngInfo> {
    parse_png_with_limit(data, MAX_IDAT_SIZE)
}

/// Internal parser with a configurable IDAT size limit (used for testing).
fn parse_png_with_limit(data: &[u8], max_idat: usize) -> Option<PngInfo> {
    if !is_png(data) {
        return None;
    }

    let mut pos = 8; // skip signature
    let mut width = 0u32;
    let mut height = 0u32;
    let mut bit_depth = 0u8;
    let mut color_type = 0u8;
    let mut ihdr_found = false;
    let mut idat_data = Vec::new();

    while pos + 8 <= data.len() {
        // Each chunk: 4 bytes length, 4 bytes type, `length` bytes data, 4 bytes CRC
        let chunk_len = read_u32_be(data, pos) as usize;
        let chunk_type = &data[pos + 4..pos + 8];

        // Ensure we have enough data for the full chunk (length + type + data + CRC)
        if pos + 12 + chunk_len > data.len() {
            break;
        }

        let chunk_data_start = pos + 8;

        match chunk_type {
            b"IHDR" => {
                if chunk_len < 13 {
                    return None;
                }
                width = read_u32_be(data, chunk_data_start);
                height = read_u32_be(data, chunk_data_start + 4);
                bit_depth = data[chunk_data_start + 8];
                color_type = data[chunk_data_start + 9];
                ihdr_found = true;
            }
            b"IDAT" => {
                if idat_data.len() + chunk_len > max_idat {
                    return None; // IDAT data exceeds safety limit
                }
                idat_data.extend_from_slice(&data[chunk_data_start..chunk_data_start + chunk_len]);
            }
            b"IEND" => {
                break;
            }
            _ => {}
        }

        // Advance past: length(4) + type(4) + data(chunk_len) + CRC(4)
        pos += 12 + chunk_len;
    }

    if !ihdr_found || idat_data.is_empty() || width == 0 || height == 0 {
        return None;
    }

    let channels = match color_type {
        0 => 1,           // Grayscale
        2 => 3,           // RGB
        4 => 2,           // Grayscale + Alpha
        6 => 4,           // RGBA
        _ => return None, // Unsupported (e.g., indexed color type 3)
    };

    Some(PngInfo {
        width,
        height,
        bit_depth,
        color_type,
        channels,
        idat_data,
    })
}

/// Re-encode an image that the PDF embedding paths can't take as it is, as an
/// 8-bit RGB or RGBA PNG. That's a PNG with palette colours (which
/// [`parse_png`] rejects, so a CSS background drew nothing), a `tRNS`
/// transparency chunk (which the raw IDAT pass-through ignored) or 16-bit
/// samples with alpha (which the alpha decoder read as twice as many 8-bit
/// pixels: a garbled image at double size), and the other formats Chromium
/// draws that have a decoder in Rust ([`other_raster_format`]), which were
/// left out. Other bytes, including every other PNG, come back unchanged, and
/// so does an image that doesn't decode.
pub(crate) fn normalize_for_pdf(bytes: Vec<u8>) -> Vec<u8> {
    let format = if needs_normalizing(&bytes) {
        image::ImageFormat::Png
    } else if let Some(format) = other_raster_format(&bytes) {
        format
    } else {
        return bytes;
    };
    // An animated GIF or WebP decodes to its first frame, which Chromium
    // prints when the page has just loaded
    let Ok(image) = image::load_from_memory_with_format(&bytes, format) else {
        return bytes;
    };
    let image = if image.color().has_alpha() {
        image::DynamicImage::ImageRgba8(image.to_rgba8())
    } else {
        image::DynamicImage::ImageRgb8(image.to_rgb8())
    };
    let mut encoded = Vec::new();
    match image.write_to(
        &mut std::io::Cursor::new(&mut encoded),
        image::ImageFormat::Png,
    ) {
        Ok(()) => encoded,
        Err(_) => bytes,
    }
}

/// GIF, WebP, BMP and ICO, by their first bytes. The PDF paths embed only PNG
/// and JPEG; AVIF, which Chromium draws too, needs a C decoder.
fn other_raster_format(data: &[u8]) -> Option<image::ImageFormat> {
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some(image::ImageFormat::Gif)
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some(image::ImageFormat::WebP)
    } else if data.starts_with(b"BM") {
        Some(image::ImageFormat::Bmp)
    } else if data.starts_with(&[0, 0, 1, 0]) {
        Some(image::ImageFormat::Ico)
    } else {
        None
    }
}

fn needs_normalizing(data: &[u8]) -> bool {
    if !is_png(data) {
        return false;
    }
    let mut pos = 8;
    let mut bit_depth = 0u8;
    let mut color_type = 0u8;
    while pos + 8 <= data.len() {
        let chunk_len = read_u32_be(data, pos) as usize;
        let chunk_type = &data[pos + 4..pos + 8];
        match chunk_type {
            b"IHDR" if pos + 18 <= data.len() => {
                bit_depth = data[pos + 16];
                color_type = data[pos + 17];
            }
            b"tRNS" => return true,
            b"IDAT" | b"IEND" => break,
            _ => {}
        }
        pos = pos.saturating_add(12).saturating_add(chunk_len);
    }
    color_type == 3 || (bit_depth == 16 && matches!(color_type, 4 | 6))
}

fn read_u32_be(data: &[u8], offset: usize) -> u32 {
    ((data[offset] as u32) << 24)
        | ((data[offset + 1] as u32) << 16)
        | ((data[offset + 2] as u32) << 8)
        | (data[offset + 3] as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_signature_detection() {
        assert!(is_png(&PNG_SIGNATURE));
        assert!(is_png(&[137, 80, 78, 71, 13, 10, 26, 10, 0, 0]));
        assert!(!is_png(&[0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(!is_png(&[0xFF, 0xD8, 0xFF])); // JPEG header
        assert!(!is_png(&[]));
        assert!(!is_png(&[137, 80])); // too short
    }

    #[test]
    fn parse_invalid_data() {
        assert!(parse_png(&[]).is_none());
        assert!(parse_png(&[0, 1, 2, 3]).is_none());
        // Valid signature but no chunks
        assert!(parse_png(&PNG_SIGNATURE).is_none());
    }

    #[test]
    fn parse_minimal_valid_png() {
        // Build a minimal valid PNG with IHDR and one IDAT chunk
        let png = build_test_png(
            1,
            1,
            8,
            2,
            &[
                0x78, 0x01, 0x62, 0x60, 0x60, 0x60, 0x00, 0x00, 0x00, 0x04, 0x00, 0x01,
            ],
        );
        let info = parse_png(&png).unwrap();
        assert_eq!(info.width, 1);
        assert_eq!(info.height, 1);
        assert_eq!(info.bit_depth, 8);
        assert_eq!(info.color_type, 2);
        assert_eq!(info.channels, 3);
        assert!(!info.idat_data.is_empty());
    }

    #[test]
    fn parse_grayscale_png() {
        let png = build_test_png(2, 2, 8, 0, &[0x78, 0x01, 0x01, 0x00, 0x00]);
        let info = parse_png(&png).unwrap();
        assert_eq!(info.color_type, 0);
        assert_eq!(info.channels, 1);
    }

    #[test]
    fn parse_rgba_png() {
        let png = build_test_png(2, 2, 8, 6, &[0x78, 0x01, 0x01, 0x00, 0x00]);
        let info = parse_png(&png).unwrap();
        assert_eq!(info.color_type, 6);
        assert_eq!(info.channels, 4);
    }

    #[test]
    fn parse_gray_alpha_png() {
        let png = build_test_png(2, 2, 8, 4, &[0x78, 0x01, 0x01, 0x00, 0x00]);
        let info = parse_png(&png).unwrap();
        assert_eq!(info.color_type, 4);
        assert_eq!(info.channels, 2);
    }

    #[test]
    fn parse_unsupported_color_type() {
        // Color type 3 (indexed) is not supported
        let png = build_test_png(1, 1, 8, 3, &[0x78, 0x01, 0x01, 0x00, 0x00]);
        assert!(parse_png(&png).is_none());
    }

    #[test]
    fn parse_multiple_idat_chunks() {
        // Build PNG with two IDAT chunks
        let mut png = Vec::new();
        png.extend_from_slice(&PNG_SIGNATURE);

        // IHDR chunk
        let ihdr_data = build_ihdr(4, 4, 8, 2);
        append_chunk(&mut png, b"IHDR", &ihdr_data);

        // First IDAT
        let idat1 = [0x78, 0x01, 0x62];
        append_chunk(&mut png, b"IDAT", &idat1);

        // Second IDAT
        let idat2 = [0x60, 0x60, 0x00];
        append_chunk(&mut png, b"IDAT", &idat2);

        // IEND
        append_chunk(&mut png, b"IEND", &[]);

        let info = parse_png(&png).unwrap();
        assert_eq!(info.width, 4);
        assert_eq!(info.height, 4);
        // IDAT data should be concatenated
        assert_eq!(info.idat_data, [0x78, 0x01, 0x62, 0x60, 0x60, 0x00]);
    }

    #[test]
    fn parse_ihdr_too_short() {
        let mut png = Vec::new();
        png.extend_from_slice(&PNG_SIGNATURE);
        // IHDR with only 5 bytes of data (needs 13)
        append_chunk(&mut png, b"IHDR", &[0; 5]);
        append_chunk(&mut png, b"IEND", &[]);
        assert!(parse_png(&png).is_none());
    }

    /// Helper: build a minimal test PNG with given parameters.
    fn build_test_png(
        width: u32,
        height: u32,
        bit_depth: u8,
        color_type: u8,
        idat_data: &[u8],
    ) -> Vec<u8> {
        let mut png = Vec::new();
        png.extend_from_slice(&PNG_SIGNATURE);

        let ihdr_data = build_ihdr(width, height, bit_depth, color_type);
        append_chunk(&mut png, b"IHDR", &ihdr_data);
        append_chunk(&mut png, b"IDAT", idat_data);
        append_chunk(&mut png, b"IEND", &[]);

        png
    }

    fn build_ihdr(width: u32, height: u32, bit_depth: u8, color_type: u8) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.push(bit_depth);
        data.push(color_type);
        data.push(0); // compression method
        data.push(0); // filter method
        data.push(0); // interlace method
        data
    }

    fn append_chunk(buf: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
        buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
        buf.extend_from_slice(chunk_type);
        buf.extend_from_slice(data);
        // CRC (we just write zeros; our parser doesn't verify CRC)
        buf.extend_from_slice(&[0, 0, 0, 0]);
    }

    #[test]
    fn parse_png_idat_within_limit() {
        // A normal small PNG should parse fine with a small limit
        let idat_payload = vec![0x78; 32]; // 32 bytes of IDAT data
        let mut png = Vec::new();
        png.extend_from_slice(&PNG_SIGNATURE);
        let ihdr_data = build_ihdr(1, 1, 8, 2);
        append_chunk(&mut png, b"IHDR", &ihdr_data);
        append_chunk(&mut png, b"IDAT", &idat_payload);
        append_chunk(&mut png, b"IEND", &[]);

        // Limit of 64 bytes is well above 32 bytes of IDAT
        let info = parse_png_with_limit(&png, 64).unwrap();
        assert_eq!(info.idat_data.len(), 32);
    }

    #[test]
    fn parse_png_idat_exceeds_limit() {
        // Build a PNG whose accumulated IDAT data exceeds a small limit
        let mut png = Vec::new();
        png.extend_from_slice(&PNG_SIGNATURE);
        let ihdr_data = build_ihdr(1, 1, 8, 2);
        append_chunk(&mut png, b"IHDR", &ihdr_data);

        // Add several IDAT chunks that together exceed the limit
        let chunk = vec![0xAA; 20];
        append_chunk(&mut png, b"IDAT", &chunk); // 20 bytes
        append_chunk(&mut png, b"IDAT", &chunk); // 40 bytes total
        append_chunk(&mut png, b"IDAT", &chunk); // 60 bytes total — over limit
        append_chunk(&mut png, b"IEND", &[]);

        // Limit of 50 bytes: third chunk pushes total to 60, should fail
        assert!(parse_png_with_limit(&png, 50).is_none());
    }

    fn encode(image: image::DynamicImage) -> Vec<u8> {
        let mut png = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        png
    }

    #[test]
    fn normalize_for_pdf_makes_16_bit_alpha_8_bit() {
        let image =
            image::ImageBuffer::from_pixel(3, 2, image::Rgba([0x1e1e_u16, 0x3a3a, 0x5f5f, 0xffff]));
        let png = encode(image::DynamicImage::ImageRgba16(image));
        assert_eq!(parse_png(&png).unwrap().bit_depth, 16);

        let normalized = normalize_for_pdf(png);
        let info = parse_png(&normalized).unwrap();
        assert_eq!(
            (info.width, info.height, info.bit_depth, info.channels),
            (3, 2, 8, 4)
        );
        let pixels = image::load_from_memory(&normalized).unwrap().to_rgba8();
        assert_eq!(pixels.get_pixel(2, 1).0, [0x1e, 0x3a, 0x5f, 0xff]);

        let gray = image::ImageBuffer::from_pixel(2, 2, image::LumaA([0x8080_u16, 0x4040]));
        let normalized = normalize_for_pdf(encode(image::DynamicImage::ImageLumaA16(gray)));
        let info = parse_png(&normalized).unwrap();
        assert_eq!((info.bit_depth, info.channels), (8, 4));
    }

    #[test]
    fn normalize_for_pdf_expands_a_palette_and_its_transparency() {
        let indexed = |trns: Option<Vec<u8>>| {
            let mut png = Vec::new();
            let mut encoder = png_decoder::Encoder::new(&mut png, 2, 1);
            encoder.set_color(png_decoder::ColorType::Indexed);
            encoder.set_depth(png_decoder::BitDepth::Eight);
            encoder.set_palette(vec![255, 0, 0, 0, 0, 255]);
            if let Some(trns) = trns {
                encoder.set_trns(trns);
            }
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[0, 1])
                .unwrap();
            png
        };
        assert!(parse_png(&indexed(None)).is_none());

        // The first colour is transparent
        let normalized = normalize_for_pdf(indexed(Some(vec![0])));
        assert_eq!(parse_png(&normalized).unwrap().channels, 4);
        let pixels = image::load_from_memory(&normalized).unwrap().to_rgba8();
        assert_eq!(pixels.get_pixel(0, 0)[3], 0);
        assert_eq!(pixels.get_pixel(1, 0).0, [0, 0, 255, 255]);

        let opaque = normalize_for_pdf(indexed(None));
        let info = parse_png(&opaque).unwrap();
        assert_eq!((info.bit_depth, info.channels), (8, 3));
        let pixels = image::load_from_memory(&opaque).unwrap().to_rgb8();
        assert_eq!(pixels.get_pixel(0, 0).0, [255, 0, 0]);
    }

    #[test]
    fn normalize_for_pdf_makes_gif_webp_bmp_and_ico_pngs() {
        let mut pixels = image::RgbaImage::from_pixel(3, 2, image::Rgba([200, 30, 60, 255]));
        pixels.put_pixel(0, 0, image::Rgba([0, 0, 0, 0]));
        let source = image::DynamicImage::ImageRgba8(pixels);
        for format in [
            image::ImageFormat::Gif,
            image::ImageFormat::WebP,
            image::ImageFormat::Bmp,
            image::ImageFormat::Ico,
        ] {
            let mut bytes = Vec::new();
            source
                .write_to(&mut std::io::Cursor::new(&mut bytes), format)
                .unwrap();
            assert_eq!(other_raster_format(&bytes), Some(format));
            let normalized = normalize_for_pdf(bytes);
            let info = parse_png(&normalized).unwrap_or_else(|| panic!("{format:?}"));
            assert_eq!((info.width, info.height, info.bit_depth), (3, 2, 8));
            let decoded = image::load_from_memory(&normalized).unwrap().to_rgba8();
            assert_eq!(decoded.get_pixel(2, 1).0, [200, 30, 60, 255], "{format:?}");
            assert_eq!(decoded.get_pixel(0, 0)[3], 0, "{format:?}");
        }
    }

    #[test]
    fn normalize_for_pdf_takes_an_animated_gifs_first_frame() {
        let frame = |colour: [u8; 4]| {
            image::Frame::new(image::RgbaImage::from_pixel(2, 2, image::Rgba(colour)))
        };
        let mut gif = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif);
            encoder
                .encode_frames([frame([0, 128, 0, 255]), frame([0, 0, 255, 255])])
                .unwrap();
        }
        let normalized = normalize_for_pdf(gif);
        let decoded = image::load_from_memory(&normalized).unwrap().to_rgba8();
        assert_eq!(decoded.get_pixel(1, 1).0, [0, 128, 0, 255]);
    }

    #[test]
    fn normalize_for_pdf_leaves_bytes_that_only_start_like_an_image() {
        // A stylesheet or font that starts with a BMP's or a GIF's first bytes
        let cases: [&[u8]; 3] = [b"BM { color: red }", b"GIF89a not really", b"\0\0\x01\0x"];
        for bytes in cases {
            assert_eq!(normalize_for_pdf(bytes.to_vec()), bytes.to_vec());
        }
    }

    #[test]
    fn normalize_for_pdf_leaves_pngs_it_can_embed_and_other_bytes() {
        let rgb = encode(image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            2,
            2,
            image::Rgb([1, 2, 3]),
        )));
        let rgba = encode(image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 4])),
        ));
        let rgb16 = encode(image::DynamicImage::ImageRgb16(
            image::ImageBuffer::from_pixel(2, 2, image::Rgb([1_u16, 2, 3])),
        ));
        for png in [rgb, rgba, rgb16] {
            assert_eq!(normalize_for_pdf(png.clone()), png);
        }
        assert_eq!(normalize_for_pdf(b"<svg/>".to_vec()), b"<svg/>".to_vec());
        // A PNG it can't decode stays as it was
        let mut broken = PNG_SIGNATURE.to_vec();
        broken.extend_from_slice(&[
            0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0, 1, 0, 0, 0, 1, 16, 6,
        ]);
        assert_eq!(normalize_for_pdf(broken.clone()), broken);
    }
}
