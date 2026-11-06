//! Packaged image formats and limits shared by the artifact's resource
//! section, packaging and the native view renderer. Packaging decodes every
//! image with the renderer's decoder, the artifact admits a file by its
//! format signature and bounds, and the renderer (`varta-view`) decodes at
//! most these images, frames and pixels, so a packaged artifact always loads
//! in the shell. The per-image byte and dimension limits equal the browser
//! bundle's resource limits, so both distributions admit the same images.
//!
//! These values are conservative initial choices pending an owner decision.

/// Image limits: owner decision 2026-11-06 (the "extended" set; a host may
/// only lower them).
///
/// At most this many images in one artifact.
pub const MAX_IMAGES: usize = 256;
/// The largest single encoded image (bytes).
pub const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
/// The largest total of encoded image bytes in one artifact.
pub const MAX_IMAGE_TOTAL_BYTES: usize = 128 * 1024 * 1024;
/// The largest image width or height (pixels).
pub const MAX_IMAGE_DIMENSION: u32 = 16384;
/// At most this many frames in one animated image.
pub const MAX_IMAGE_FRAMES: usize = 4096;
/// Bytes per decoded pixel: premultiplied 8-bit RGBA.
pub const DECODED_PIXEL_BYTES: usize = 4;
/// The largest total of decoded full-resolution pixels of one artifact's
/// images, every frame of an animated image counted, in bytes of
/// premultiplied RGBA.
pub const MAX_IMAGE_DECODED_BYTES: usize = 256 * 1024 * 1024;
/// The largest total of decoded image memory a renderer retains: every
/// frame's full-resolution pixels and its downscaling levels. A level halves
/// each axis longer than one pixel, so it holds at most half the pixels of
/// the level before it and all levels together at most twice the frame.
pub const MAX_IMAGE_CACHE_BYTES: usize = 2 * MAX_IMAGE_DECODED_BYTES;
/// The longest image name (bytes); names follow the view resource grammar.
pub const MAX_IMAGE_NAME_BYTES: usize = 1024;

/// An image format an `img` may show: the raster formats of the WHATWG MIME
/// Sniffing image type pattern table (§6.1) that the renderer decodes. The
/// code is the resource section's format field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImageFormat {
    /// PNG, including animated PNG (APNG).
    Png = 1,
    Jpeg = 2,
    /// GIF87a and GIF89a, animated or not.
    Gif = 3,
    /// WebP, lossy or lossless, animated or not.
    WebP = 4,
    Bmp = 5,
    /// Windows icon (the largest entry is shown).
    Ico = 6,
}

impl ImageFormat {
    pub const ALL: [Self; 6] = [
        Self::Png,
        Self::Jpeg,
        Self::Gif,
        Self::WebP,
        Self::Bmp,
        Self::Ico,
    ];
    /// The section code.
    #[must_use]
    pub fn code(self) -> u16 {
        self as u16
    }
    /// The format of a section code.
    #[must_use]
    pub fn from_code(code: u16) -> Option<Self> {
        Self::ALL.into_iter().find(|format| format.code() == code)
    }
    /// The MIME type the browser bundle serves the file with.
    #[must_use]
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
            Self::Bmp => "image/bmp",
            Self::Ico => "image/x-icon",
        }
    }
    /// The file extension the browser bundle names the file with.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Gif => "gif",
            Self::WebP => "webp",
            Self::Bmp => "bmp",
            Self::Ico => "ico",
        }
    }
}

/// Why the bytes of a packaged image were refused before decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImageRefusal {
    #[error("the image exceeds {MAX_IMAGE_BYTES} bytes or is empty")]
    Size,
    #[error("not an image of a known format")]
    Unknown,
    #[error(
        "AVIF images are not admitted: no decoder in safe Rust is available for the native renderer"
    )]
    Avif,
    #[error(
        "SVG images are not admitted: an `img` showing SVG needs an SVG renderer, which the native renderer does not have"
    )]
    Svg,
}

/// The PNG file signature.
pub const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
/// MIME Sniffing §6.1 patterns: (pattern, mask, format). `RIFF` WebP masks
/// its length field.
const PATTERNS: [(&[u8], &[u8], ImageFormat); 7] = [
    (b"\x00\x00\x01\x00", b"\xff\xff\xff\xff", ImageFormat::Ico),
    (b"BM", b"\xff\xff", ImageFormat::Bmp),
    (b"GIF87a", b"\xff\xff\xff\xff\xff\xff", ImageFormat::Gif),
    (b"GIF89a", b"\xff\xff\xff\xff\xff\xff", ImageFormat::Gif),
    (
        b"RIFF\x00\x00\x00\x00WEBPVP",
        b"\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff\xff\xff",
        ImageFormat::WebP,
    ),
    (
        PNG_SIGNATURE,
        b"\xff\xff\xff\xff\xff\xff\xff\xff",
        ImageFormat::Png,
    ),
    (b"\xff\xd8\xff", b"\xff\xff\xff", ImageFormat::Jpeg),
];
/// The ISO BMFF `ftyp` box brands of AVIF.
const AVIF_BRANDS: [&[u8; 4]; 2] = [b"avif", b"avis"];
const FTYP_AT: usize = 4;
const BRAND_AT: usize = 8;
/// How far an SVG document's root element may be looked for.
const SVG_PREFIX_BYTES: usize = 512;

/// The format of a packaged image by its leading bytes (WHATWG MIME
/// Sniffing §6.1, with AVIF's `ftyp` brand and an SVG root element named so
/// they are refused with their own reason), within [`MAX_IMAGE_BYTES`].
pub fn sniff(bytes: &[u8]) -> Result<ImageFormat, ImageRefusal> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err(ImageRefusal::Size);
    }
    for (pattern, mask, format) in PATTERNS {
        if bytes.len() >= pattern.len()
            && pattern
                .iter()
                .zip(mask)
                .zip(bytes)
                .all(|((p, m), b)| b & m == *p)
        {
            return Ok(format);
        }
    }
    if bytes.get(FTYP_AT..BRAND_AT) == Some(b"ftyp")
        && AVIF_BRANDS
            .iter()
            .any(|brand| bytes.get(BRAND_AT..BRAND_AT + brand.len()) == Some(&brand[..]))
    {
        return Err(ImageRefusal::Avif);
    }
    let prefix = &bytes[..bytes.len().min(SVG_PREFIX_BYTES)];
    if prefix.windows(4).any(|window| window == b"<svg") {
        return Err(ImageRefusal::Svg);
    }
    Err(ImageRefusal::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_sniffed_by_their_signatures() {
        let cases: [(&[u8], ImageFormat); 7] = [
            (b"\x89PNG\r\n\x1a\n....", ImageFormat::Png),
            (b"\xff\xd8\xff\xe0", ImageFormat::Jpeg),
            (b"GIF89a....", ImageFormat::Gif),
            (b"GIF87a....", ImageFormat::Gif),
            (b"RIFF\x10\x00\x00\x00WEBPVP8 ", ImageFormat::WebP),
            (b"BM....", ImageFormat::Bmp),
            (b"\x00\x00\x01\x00\x01\x00", ImageFormat::Ico),
        ];
        for (bytes, format) in cases {
            assert_eq!(sniff(bytes), Ok(format));
            assert_eq!(ImageFormat::from_code(format.code()), Some(format));
        }
        assert_eq!(sniff(b""), Err(ImageRefusal::Size));
        assert_eq!(
            sniff(&vec![0xff; MAX_IMAGE_BYTES + 1]),
            Err(ImageRefusal::Size)
        );
        assert_eq!(
            sniff(b"\x00\x00\x00\x1cftypavif\x00\x00"),
            Err(ImageRefusal::Avif)
        );
        assert_eq!(
            sniff(b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            Err(ImageRefusal::Svg)
        );
        assert_eq!(
            sniff(b"RIFF\x10\x00\x00\x00WAVE"),
            Err(ImageRefusal::Unknown)
        );
        assert_eq!(ImageFormat::from_code(0), None);
    }
}
