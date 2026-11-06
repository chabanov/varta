//! Packaged image limits shared by the artifact's resource section and the
//! native view renderer: packaging (`varta-artifact`) refuses a section that
//! exceeds them, and the renderer (`varta-view`) decodes at most these images
//! and pixels, so a packaged artifact always loads in the shell. The per-image
//! byte and dimension limits equal the browser bundle's resource limits, so
//! both distributions admit the same images.
//!
//! These values are conservative initial choices pending an owner decision.

/// At most this many images in one artifact.
pub const MAX_IMAGES: usize = 64;
/// The largest single encoded image (bytes).
pub const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;
/// The largest total of encoded image bytes in one artifact.
pub const MAX_IMAGE_TOTAL_BYTES: usize = 16 * 1024 * 1024;
/// The largest image width or height (pixels).
pub const MAX_IMAGE_DIMENSION: u32 = 4096;
/// Bytes per decoded pixel: premultiplied 8-bit RGBA.
pub const DECODED_PIXEL_BYTES: usize = 4;
/// The largest total of decoded full-resolution pixels of one artifact's
/// images, in bytes of premultiplied RGBA.
pub const MAX_IMAGE_DECODED_BYTES: usize = 16 * 1024 * 1024;
/// The largest total of decoded image memory a renderer retains: every
/// image's full-resolution pixels and its downscaling levels. A level halves
/// each axis longer than one pixel, so it holds at most half the pixels of
/// the level before it and all levels together at most twice the image.
pub const MAX_IMAGE_CACHE_BYTES: usize = 2 * MAX_IMAGE_DECODED_BYTES;
/// The longest image name (bytes); names follow the view resource grammar.
pub const MAX_IMAGE_NAME_BYTES: usize = 1024;

/// The PNG file signature.
pub const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
/// The `gAMA` value of the sRGB transfer curve (1/2.2 in units of 1e-5),
/// which Chrome treats as untagged sRGB.
pub const SRGB_GAMMA: u32 = 45_455;
const IHDR_BYTES: u32 = 13;
const CHUNK_OVERHEAD: usize = 12;
const GAMMA_BYTES: usize = 4;
/// Chunks that change how Chrome draws a PNG beyond its sRGB pixels (an
/// embedded profile, HDR signalling, EXIF orientation) or make it animated.
const REFUSED_CHUNKS: [(&[u8; 4], PngRefusal); 7] = [
    (b"acTL", PngRefusal::Animated),
    (b"fcTL", PngRefusal::Animated),
    (b"fdAT", PngRefusal::Animated),
    (b"iCCP", PngRefusal::ColorManaged),
    (b"cICP", PngRefusal::ColorManaged),
    (b"mDCV", PngRefusal::ColorManaged),
    (b"eXIf", PngRefusal::Orientation),
];

/// Why the bytes of a packaged image were refused before decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PngRefusal {
    #[error("not a PNG file")]
    Signature,
    #[error("the PNG header is missing, malformed or repeated")]
    Header,
    #[error("PNG dimensions {width}x{height} are outside 1..={MAX_IMAGE_DIMENSION} pixels")]
    Dimensions { width: u32, height: u32 },
    #[error("the decoded PNG exceeds {MAX_IMAGE_DECODED_BYTES} bytes")]
    Decoded,
    #[error("a PNG chunk is truncated or the file does not end with IEND")]
    Structure,
    #[error("animated PNG is not admitted")]
    Animated,
    #[error(
        "color-managed PNG (an embedded profile, chromaticities or a gamma other than sRGB's) is not admitted; convert it to sRGB"
    )]
    ColorManaged,
    #[error("PNG with EXIF orientation is not admitted")]
    Orientation,
}

/// The header of an admitted PNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PngHeader {
    pub width: u32,
    pub height: u32,
    /// Bytes of its decoded premultiplied RGBA pixels.
    pub decoded_bytes: usize,
}

/// Admit a PNG file by its structure alone, before any decoding: the
/// signature, one leading `IHDR` with dimensions within the limits, bounded
/// chunk lengths and a final empty `IEND` with nothing after it. Animation
/// and every chunk that would make Chrome draw other colors or another
/// orientation than the plain sRGB pixels are refused; an `sRGB` chunk
/// declares the pixels sRGB, so with it `gAMA` and `cHRM` are ignored, as
/// Chrome ignores them. Without it, `cHRM` is refused and `gAMA` admitted
/// only with the sRGB value. CRCs and the compressed stream are checked by
/// the decoder.
pub fn inspect_png(bytes: &[u8]) -> Result<PngHeader, PngRefusal> {
    if bytes.len() > MAX_IMAGE_BYTES || !bytes.starts_with(PNG_SIGNATURE) {
        return Err(PngRefusal::Signature);
    }
    let word = |at: usize| -> Option<u32> {
        bytes
            .get(at..at.checked_add(4)?)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let start = PNG_SIGNATURE.len();
    if word(start) != Some(IHDR_BYTES) || bytes.get(start + 4..start + 8) != Some(b"IHDR") {
        return Err(PngRefusal::Header);
    }
    let width = word(start + 8).ok_or(PngRefusal::Header)?;
    let height = word(start + 12).ok_or(PngRefusal::Header)?;
    if !(1..=MAX_IMAGE_DIMENSION).contains(&width) || !(1..=MAX_IMAGE_DIMENSION).contains(&height) {
        return Err(PngRefusal::Dimensions { width, height });
    }
    // Both dimensions are at most MAX_IMAGE_DIMENSION, so this cannot overflow.
    let decoded_bytes = width as usize * height as usize * DECODED_PIXEL_BYTES;
    if decoded_bytes > MAX_IMAGE_DECODED_BYTES {
        return Err(PngRefusal::Decoded);
    }
    let mut at = start;
    let mut srgb = false;
    let mut chromaticities = false;
    let mut gamma = None;
    loop {
        let length = word(at).ok_or(PngRefusal::Structure)? as usize;
        let kind = bytes.get(at + 4..at + 8).ok_or(PngRefusal::Structure)?;
        let next = at
            .checked_add(CHUNK_OVERHEAD)
            .and_then(|end| end.checked_add(length))
            .filter(|end| *end <= bytes.len())
            .ok_or(PngRefusal::Structure)?;
        if kind == b"IHDR" && at != start {
            return Err(PngRefusal::Header);
        }
        if let Some((_, refusal)) = REFUSED_CHUNKS.iter().find(|(name, _)| kind == *name) {
            return Err(*refusal);
        }
        match kind {
            b"sRGB" => srgb = true,
            b"cHRM" => chromaticities = true,
            b"gAMA" if length == GAMMA_BYTES => gamma = word(at + 8),
            b"gAMA" => return Err(PngRefusal::ColorManaged),
            b"IEND" if length == 0 && next == bytes.len() => break,
            b"IEND" => return Err(PngRefusal::Structure),
            _ => {}
        }
        at = next;
    }
    if !srgb && (chromaticities || gamma.is_some_and(|gamma| gamma != SRGB_GAMMA)) {
        return Err(PngRefusal::ColorManaged);
    }
    Ok(PngHeader {
        width,
        height,
        decoded_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG of `chunks` after a `width` x `height` RGBA header (CRCs are
    /// not checked here).
    fn png(width: u32, height: u32, chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        let mut chunk = |kind: &[u8; 4], data: &[u8]| {
            bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
            bytes.extend_from_slice(kind);
            bytes.extend_from_slice(data);
            bytes.extend_from_slice(&[0; 4]);
        };
        let mut header = width.to_be_bytes().to_vec();
        header.extend_from_slice(&height.to_be_bytes());
        header.extend_from_slice(&[8, 6, 0, 0, 0]);
        chunk(b"IHDR", &header);
        for (kind, data) in chunks {
            chunk(kind, data);
        }
        chunk(b"IEND", &[]);
        bytes
    }

    #[test]
    fn structure_dimensions_and_color_chunks_are_checked() {
        let idat: (&[u8; 4], &[u8]) = (b"IDAT", &[0]);
        assert_eq!(
            inspect_png(&png(3, 2, &[idat])),
            Ok(PngHeader {
                width: 3,
                height: 2,
                decoded_bytes: 24
            })
        );
        assert_eq!(inspect_png(b"GIF89a"), Err(PngRefusal::Signature));
        assert_eq!(
            inspect_png(&png(0, 2, &[idat])),
            Err(PngRefusal::Dimensions {
                width: 0,
                height: 2
            })
        );
        assert_eq!(
            inspect_png(&png(MAX_IMAGE_DIMENSION + 1, 1, &[idat])),
            Err(PngRefusal::Dimensions {
                width: MAX_IMAGE_DIMENSION + 1,
                height: 1
            })
        );
        assert_eq!(
            inspect_png(&png(MAX_IMAGE_DIMENSION, MAX_IMAGE_DIMENSION, &[idat])),
            Err(PngRefusal::Decoded)
        );
        let mut truncated = png(3, 2, &[idat]);
        truncated.pop();
        assert_eq!(inspect_png(&truncated), Err(PngRefusal::Structure));
        let mut trailing = png(3, 2, &[idat]);
        trailing.push(0);
        assert_eq!(inspect_png(&trailing), Err(PngRefusal::Structure));
        let header = png(1, 1, &[])[8..33].to_vec();
        let mut repeated = png(3, 2, &[idat]);
        let end = repeated.len() - CHUNK_OVERHEAD;
        repeated.splice(end..end, header);
        assert_eq!(inspect_png(&repeated), Err(PngRefusal::Header));
        assert_eq!(
            inspect_png(&png(3, 2, &[(b"acTL", &[0; 8]), idat])),
            Err(PngRefusal::Animated)
        );
        assert_eq!(
            inspect_png(&png(3, 2, &[(b"eXIf", &[0; 8]), idat])),
            Err(PngRefusal::Orientation)
        );
        let gamma = |value: u32| value.to_be_bytes();
        for refused in [
            vec![(b"iCCP", &[0u8; 8][..])],
            vec![(b"iCCP", &[0u8; 8][..]), (b"sRGB", &[0][..])],
            vec![(b"cHRM", &[0u8; 32][..])],
            vec![(b"gAMA", &gamma(40_000)[..])],
            vec![(b"gAMA", &[0u8; 3][..])],
        ] {
            let mut chunks = refused;
            chunks.push(idat);
            assert_eq!(
                inspect_png(&png(3, 2, &chunks)),
                Err(PngRefusal::ColorManaged)
            );
        }
        for admitted in [
            vec![(b"sRGB", &[0u8][..])],
            vec![(b"gAMA", &gamma(SRGB_GAMMA)[..])],
            vec![
                (b"sRGB", &[0u8][..]),
                (b"gAMA", &gamma(40_000)[..]),
                (b"cHRM", &[0u8; 32][..]),
            ],
        ] {
            let mut chunks = admitted;
            chunks.push(idat);
            assert!(inspect_png(&png(3, 2, &chunks)).is_ok());
        }
    }
}
