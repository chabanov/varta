//! Packaged images of the native renderer: PNG files decoded once, within
//! the shared view image limits (`varta_common::view::images`), into
//! premultiplied sRGB pixels and the downscaling levels Chrome samples.
//!
//! Chrome (Skia) draws an `img` with bilinear filtering between texels and,
//! when the image is drawn smaller than half its size, linear filtering
//! between the two nearest levels of a box-filtered level chain whose level
//! of detail is `log2(1 / scale) - 0.5` for the larger of the two axis
//! scales. Each level halves the axes longer than one pixel with Skia's
//! integer kernels: a two-tap box on an even axis and a 1-2-1 kernel on an
//! odd one. Both were measured against Chrome (see
//! `examples/native_image_probe.rs`).
mod draw;
mod levels;

pub(crate) use draw::{ImageDraw, draw_image};
use std::collections::BTreeMap;
use std::sync::Arc;
use varta_common::view::images::{
    DECODED_PIXEL_BYTES, MAX_IMAGE_BYTES, MAX_IMAGE_CACHE_BYTES, MAX_IMAGE_DECODED_BYTES,
    MAX_IMAGE_NAME_BYTES, MAX_IMAGE_TOTAL_BYTES, MAX_IMAGES, PngRefusal, inspect_png,
};
use varta_common::view::{AttributeValue, Tag, attributes};

/// The PNG decoder's own allocation ceiling for one image: its decoded
/// output (at most 16-bit channels before they are stripped) and its
/// working rows.
const DECODER_LIMIT_FACTOR: usize = 4;
const RGB_CHANNELS: usize = 3;
const GRAY_ALPHA_CHANNELS: usize = 2;
const OPAQUE: u8 = u8::MAX;

/// Why a set of packaged images was refused. Every image is checked and
/// decoded before any view is laid out.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("{0} images exceed the limit of {MAX_IMAGES}")]
    Count(usize),
    #[error("image name {0:?} is not a bounded view resource path")]
    Name(String),
    #[error("duplicate image name {0:?}")]
    Duplicate(String),
    #[error("encoded images exceed {MAX_IMAGE_TOTAL_BYTES} bytes")]
    TotalBytes,
    #[error("decoded images exceed {MAX_IMAGE_DECODED_BYTES} bytes")]
    Decoded,
    #[error("decoded images and their levels exceed {MAX_IMAGE_CACHE_BYTES} bytes")]
    Cache,
    #[error("image {name:?}: {source}")]
    Refused {
        name: String,
        #[source]
        source: PngRefusal,
    },
    #[error("image {name:?}: invalid PNG data (checksum, compressed stream or pixel format)")]
    Malformed { name: String },
}

/// One decoded image level: premultiplied 8-bit RGBA rows.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Level {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A decoded image and its downscaling levels, the first being the image.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Image {
    pub levels: Vec<Level>,
}
impl Image {
    pub fn width(&self) -> u32 {
        self.levels[0].width
    }
    pub fn height(&self) -> u32 {
        self.levels[0].height
    }
}

/// The decoded images of an application, by their resource names: what an
/// `img` element's `src` may name. Decoding happens once, here; renderers
/// share the set.
#[derive(Default, PartialEq, Eq)]
pub struct ImageSet {
    images: BTreeMap<String, Arc<Image>>,
    bytes: usize,
}
impl std::fmt::Debug for ImageSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageSet")
            .field("names", &self.images.keys().collect::<Vec<_>>())
            .field("bytes", &self.bytes)
            .finish()
    }
}
impl ImageSet {
    /// Check and decode `(name, PNG bytes)` pairs. Names follow the view
    /// resource grammar and are unique; the count, every image's bytes,
    /// dimensions and structure ([`inspect_png`]), the encoded and decoded
    /// totals and the retained levels are bounded before or as they are
    /// allocated, and a checksum or stream error refuses the whole set.
    pub fn decode<'a>(
        sources: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    ) -> Result<Self, ImageError> {
        let sources: Vec<_> = sources.into_iter().take(MAX_IMAGES + 1).collect();
        if sources.len() > MAX_IMAGES {
            return Err(ImageError::Count(sources.len()));
        }
        let mut encoded = 0usize;
        let mut decoded = 0usize;
        let mut headers = Vec::with_capacity(sources.len());
        // Every bound that needs no decoding holds before anything decodes.
        for &(name, bytes) in &sources {
            if name.len() > MAX_IMAGE_NAME_BYTES
                || !attributes::valid(AttributeValue::Resource, Tag::Img, name)
            {
                return Err(ImageError::Name(name.to_owned()));
            }
            encoded = encoded
                .checked_add(bytes.len())
                .filter(|total| *total <= MAX_IMAGE_TOTAL_BYTES)
                .ok_or(ImageError::TotalBytes)?;
            let header = inspect_png(bytes).map_err(|source| ImageError::Refused {
                name: name.to_owned(),
                source,
            })?;
            decoded = decoded
                .checked_add(header.decoded_bytes)
                .filter(|total| *total <= MAX_IMAGE_DECODED_BYTES)
                .ok_or(ImageError::Decoded)?;
            headers.push(header);
        }
        let mut set = Self::default();
        for (&(name, bytes), header) in sources.iter().zip(headers) {
            if set.images.contains_key(name) {
                return Err(ImageError::Duplicate(name.to_owned()));
            }
            let malformed = || ImageError::Malformed {
                name: name.to_owned(),
            };
            let pixels = decode_png(bytes, header.decoded_bytes).ok_or_else(malformed)?;
            let image = levels::build(header.width, header.height, pixels, |bytes| {
                set.bytes = set
                    .bytes
                    .checked_add(bytes)
                    .filter(|total| *total <= MAX_IMAGE_CACHE_BYTES)
                    .ok_or(ImageError::Cache)?;
                Ok(())
            })?;
            set.images.insert(name.to_owned(), Arc::new(image));
        }
        Ok(set)
    }

    /// The number of images.
    #[must_use]
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Whether the set has no images.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Whether `name` is an image of the set.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.images.contains_key(name)
    }

    /// The natural size of image `name`, in pixels (one CSS pixel each, as
    /// Chrome sizes a PNG; `pHYs` density is ignored, as Chrome ignores it).
    #[must_use]
    pub fn size(&self, name: &str) -> Option<(u32, u32)> {
        self.images
            .get(name)
            .map(|image| (image.width(), image.height()))
    }

    /// The decoded memory the set retains (pixels of every level).
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Arc<Image>> {
        self.images.get(name)
    }
}

/// The images a view's `img` elements draw, shared by its renderers;
/// compared by identity.
#[derive(Debug, Clone, Default)]
pub(crate) struct Images(pub Option<Arc<ImageSet>>);
impl PartialEq for Images {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// Decode one admitted PNG to premultiplied RGBA: palette, low bit depths
/// and `tRNS` expand, 16-bit channels keep their high byte, and gray
/// replicates into the color channels. `None` for any decoder error, or
/// output that does not match the inspected header.
fn decode_png(bytes: &[u8], decoded_bytes: usize) -> Option<Vec<u8>> {
    let limits = png::Limits {
        bytes: decoded_bytes
            .saturating_mul(DECODER_LIMIT_FACTOR)
            .max(MAX_IMAGE_BYTES),
    };
    let mut decoder = png::Decoder::new_with_limits(bytes, limits);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(reader.output_buffer_size()).ok()?;
    buffer.resize(reader.output_buffer_size(), 0);
    let frame = reader.next_frame(&mut buffer).ok()?;
    if frame.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let pixels = frame.width as usize * frame.height as usize;
    if pixels * DECODED_PIXEL_BYTES != decoded_bytes {
        return None;
    }
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(decoded_bytes).ok()?;
    let rows = buffer
        .chunks_exact(frame.line_size)
        .take(frame.height as usize);
    for row in rows {
        let row = &row[..frame.width as usize * frame.color_type.samples()];
        match frame.color_type {
            png::ColorType::Rgba => {
                for [red, green, blue, alpha] in row.as_chunks::<DECODED_PIXEL_BYTES>().0 {
                    push_premultiplied(&mut rgba, [*red, *green, *blue], *alpha);
                }
            }
            png::ColorType::Rgb => {
                for color in row.as_chunks::<RGB_CHANNELS>().0 {
                    push_premultiplied(&mut rgba, *color, OPAQUE);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for [gray, alpha] in row.as_chunks::<GRAY_ALPHA_CHANNELS>().0 {
                    push_premultiplied(&mut rgba, [*gray; 3], *alpha);
                }
            }
            png::ColorType::Grayscale => {
                for &gray in row {
                    push_premultiplied(&mut rgba, [gray; 3], OPAQUE);
                }
            }
            png::ColorType::Indexed => return None,
        }
    }
    (rgba.len() == decoded_bytes).then_some(rgba)
}

/// Skia's premultiplication of one 8-bit channel (`SkMulDiv255Round`).
fn premultiply(channel: u8, alpha: u8) -> u8 {
    let product = u32::from(channel) * u32::from(alpha) + 128;
    // At most (255 * 255 + 128 + 254) >> 8 = 255.
    ((product + (product >> 8)) >> 8) as u8
}

fn push_premultiplied(out: &mut Vec<u8>, color: [u8; 3], alpha: u8) {
    out.extend(color.map(|channel| premultiply(channel, alpha)));
    out.push(alpha);
}

#[cfg(test)]
pub(crate) mod tests;
