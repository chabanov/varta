//! Packaged images of the native renderer: image files decoded once, within
//! the shared view image limits (`varta_common::view::images`), into
//! premultiplied sRGB frames and the downscaling levels Chrome samples.
//!
//! - Formats (`decode`): PNG and APNG, JPEG, GIF, WebP, BMP and ICO, told
//!   apart by their signatures (WHATWG MIME Sniffing §6.1). AVIF and SVG are
//!   refused with their own reasons.
//! - Colour (`color`): converted to sRGB per CSS Color 4 §10 and the PNG 3rd
//!   edition chunk priority.
//! - Orientation: the EXIF orientation is kept with the pixels and applied
//!   when drawing, so `image-orientation: none` (CSS Images 3 §5.1) shows
//!   the stored pixels and `from-image` (the initial value) the oriented
//!   ones, with the natural size swapped for quarter turns.
//! - Animation (`Playback`): frames advance on the view's animation clock
//!   from the time the element's image was set, with Blink's frame duration
//!   floor and loop counts, so frames are deterministic like transitions.
//! - Sampling (`draw`): Chrome (Skia) filters bilinearly and, below half
//!   size, linearly between two levels of a box-filtered level chain at the
//!   level of detail `log2(1 / scale) - 0.5` for the larger axis scale; each
//!   level halves the axes longer than one pixel with Skia's integer kernels.
//!   `image-rendering: pixelated` and `crisp-edges` sample the nearest texel
//!   when the image is not drawn smaller (CSS Images 3 §5.2). Measured
//!   against Chrome (see `examples/native_image_probe.rs`).
mod color;
mod decode;
mod draw;
mod levels;
pub(crate) mod select;
pub(crate) mod style;

pub(crate) use decode::Plays;
pub(crate) use draw::{Filter, ImageDraw, draw_image};
use std::collections::BTreeMap;
use std::sync::Arc;
use varta_common::view::images::{
    DECODED_PIXEL_BYTES, ImageRefusal, MAX_IMAGE_CACHE_BYTES, MAX_IMAGE_DECODED_BYTES,
    MAX_IMAGE_NAME_BYTES, MAX_IMAGE_TOTAL_BYTES, MAX_IMAGES, sniff,
};
use varta_common::view::{AttributeValue, Tag, attributes};

/// EXIF orientations that swap the axes (a quarter turn, with or without a
/// flip).
const QUARTER_TURNS: [u8; 4] = [5, 6, 7, 8];

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
    #[error(
        "image {0:?}: its dimensions, frames or decoded pixels exceed the image limits ({MAX_IMAGE_DECODED_BYTES} decoded bytes for all images)"
    )]
    Decoded(String),
    #[error("decoded images and their levels exceed {MAX_IMAGE_CACHE_BYTES} bytes")]
    Cache,
    #[error("image {name:?}: {source}")]
    Refused {
        name: String,
        #[source]
        source: ImageRefusal,
    },
    #[error("image {name:?}: invalid image data (checksum, compressed stream or structure)")]
    Malformed { name: String },
    #[error(
        "image {name:?}: HDR (PQ/HLG), narrow-range or non-RGB colour encodings are not admitted"
    )]
    Color { name: String },
}

/// One decoded image level: premultiplied 8-bit RGBA rows.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Level {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// One frame and its downscaling levels, the first being the frame.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    pub levels: Vec<Level>,
    /// How long the frame shows, in milliseconds (0 for a still image).
    pub duration_ms: u32,
}

/// A decoded image: its frames in their stored orientation, its EXIF
/// orientation and how many times it plays.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Image {
    pub frames: Vec<Frame>,
    pub plays: Plays,
    pub orientation: u8,
    /// A colour conversion to apply to sampled pixels (JPEG).
    pub deferred: Option<color::SrgbTransform>,
}
impl Image {
    /// The stored width (before orientation).
    pub fn width(&self) -> u32 {
        self.frames[0].levels[0].width
    }
    /// The stored height (before orientation).
    pub fn height(&self) -> u32 {
        self.frames[0].levels[0].height
    }
    /// The orientation drawn: the EXIF orientation for `from-image`, none
    /// for `image-orientation: none`.
    pub fn orientation(&self, from_image: bool) -> u8 {
        if from_image { self.orientation } else { 1 }
    }
    /// The natural size in image pixels as drawn with `orientation`.
    pub fn oriented_size(&self, orientation: u8) -> (u32, u32) {
        if QUARTER_TURNS.contains(&orientation) {
            (self.height(), self.width())
        } else {
            (self.width(), self.height())
        }
    }
    /// Whether the image has more than one frame.
    pub fn animated(&self) -> bool {
        self.frames.len() > 1
    }
    /// The frame shown `elapsed` milliseconds after the animation started:
    /// frames follow each other for their durations, the animation repeats
    /// for its plays and then stays on its last frame.
    pub fn frame_at(&self, elapsed: f64) -> usize {
        if !self.animated() || !elapsed.is_finite() || elapsed <= 0.0 {
            return 0;
        }
        let cycle: f64 = self.frames.iter().map(|f| f64::from(f.duration_ms)).sum();
        if cycle <= 0.0 {
            return 0;
        }
        if let Plays::Finite(plays) = self.plays
            && elapsed >= cycle * f64::from(plays)
        {
            return self.frames.len() - 1;
        }
        let mut at = elapsed % cycle;
        for (index, frame) in self.frames.iter().enumerate() {
            let duration = f64::from(frame.duration_ms);
            if at < duration {
                return index;
            }
            at -= duration;
        }
        self.frames.len() - 1
    }
    /// Whether the animation still changes frames `elapsed` ms after it
    /// started.
    pub fn playing(&self, elapsed: f64) -> bool {
        if !self.animated() {
            return false;
        }
        match self.plays {
            Plays::Infinite => true,
            Plays::Finite(plays) => {
                let cycle: f64 = self.frames.iter().map(|f| f64::from(f.duration_ms)).sum();
                elapsed < cycle * f64::from(plays)
            }
        }
    }
}

/// The decoded images of an application, by their resource names: what an
/// `img` element's `src` or `srcset` may name. Decoding happens once, here;
/// renderers share the set.
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
    /// Check and decode `(name, file)` pairs. Names follow the view resource
    /// grammar and are unique; the count, the encoded total and every file's
    /// format ([`sniff`]) are checked before anything decodes; each image's
    /// dimensions, frames and decoded pixels are charged to the set's
    /// decoded budget before they are decoded, and its levels to the
    /// retained cache. A checksum, stream or colour error refuses the whole
    /// set.
    pub fn decode<'a>(
        sources: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    ) -> Result<Self, ImageError> {
        Self::decode_within(sources, MAX_IMAGE_DECODED_BYTES)
    }

    /// [`Self::decode`] charging the images' decoded pixels to `decoded`
    /// bytes (at most [`MAX_IMAGE_DECODED_BYTES`]).
    pub(crate) fn decode_within<'a>(
        sources: impl IntoIterator<Item = (&'a str, &'a [u8])>,
        decoded: usize,
    ) -> Result<Self, ImageError> {
        let sources: Vec<_> = sources.into_iter().take(MAX_IMAGES + 1).collect();
        if sources.len() > MAX_IMAGES {
            return Err(ImageError::Count(sources.len()));
        }
        let mut encoded = 0usize;
        let mut formats = Vec::with_capacity(sources.len());
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
            formats.push(sniff(bytes).map_err(|source| ImageError::Refused {
                name: name.to_owned(),
                source,
            })?);
        }
        let mut set = Self::default();
        let mut budget = decoded.min(MAX_IMAGE_DECODED_BYTES);
        for (&(name, bytes), format) in sources.iter().zip(formats) {
            if set.images.contains_key(name) {
                return Err(ImageError::Duplicate(name.to_owned()));
            }
            let decoded = decode::decode(bytes, format, &mut budget).map_err(|failure| {
                let name = name.to_owned();
                match failure {
                    decode::Failure::Malformed => ImageError::Malformed { name },
                    decode::Failure::Limits => ImageError::Decoded(name),
                    decode::Failure::Unsupported => ImageError::Color { name },
                }
            })?;
            let mut frames = Vec::with_capacity(decoded.frames.len());
            for frame in decoded.frames {
                let pixels = premultiplied(frame.rgba);
                let levels = levels::build(decoded.width, decoded.height, pixels, |bytes| {
                    set.bytes = set
                        .bytes
                        .checked_add(bytes)
                        .filter(|total| *total <= MAX_IMAGE_CACHE_BYTES)
                        .ok_or(ImageError::Cache)?;
                    Ok(())
                })?;
                frames.push(Frame {
                    levels,
                    duration_ms: frame.duration_ms,
                });
            }
            set.images.insert(
                name.to_owned(),
                Arc::new(Image {
                    frames,
                    plays: decoded.plays,
                    orientation: decoded.orientation,
                    deferred: decoded.deferred,
                }),
            );
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

    /// The natural size of image `name` in image pixels, as drawn with its
    /// EXIF orientation (CSS Images 3 `image-orientation: from-image`).
    #[must_use]
    pub fn size(&self, name: &str) -> Option<(u32, u32)> {
        self.images
            .get(name)
            .map(|image| image.oriented_size(image.orientation))
    }

    /// The number of frames of image `name`.
    #[must_use]
    pub fn frames(&self, name: &str) -> Option<usize> {
        self.images.get(name).map(|image| image.frames.len())
    }

    /// The frame durations (ms) of image `name` and how many times it plays
    /// (`None` for forever).
    #[must_use]
    pub fn timeline(&self, name: &str) -> Option<(Vec<u32>, Option<u32>)> {
        self.images.get(name).map(|image| {
            let durations = image.frames.iter().map(|frame| frame.duration_ms).collect();
            let plays = match image.plays {
                Plays::Infinite => None,
                Plays::Finite(plays) => Some(plays),
            };
            (durations, plays)
        })
    }

    /// The decoded memory the set retains (pixels of every frame and level).
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Arc<Image>> {
        self.images.get(name)
    }
}

/// The images a view's `img` elements draw and when each element's
/// animation started on the view's animation clock.
#[derive(Debug, Clone, Default)]
pub(crate) struct Images {
    pub set: Option<Arc<ImageSet>>,
    /// The animation clock (ms), advanced with the view's.
    pub now: f64,
    /// When each element's current image was set.
    pub started: BTreeMap<u32, f64>,
}
impl PartialEq for Images {
    fn eq(&self, other: &Self) -> bool {
        let same_set = match (&self.set, &other.set) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        same_set && self.now.to_bits() == other.now.to_bits() && self.started == other.started
    }
}

/// Move the image clock to `now` (never back) and drop the timelines of
/// removed elements. Returns whether a shown frame changed.
pub(crate) fn advance(
    tree: &mut crate::tree::Tree,
    env: &crate::style::Environment,
    now: f64,
) -> bool {
    if !now.is_finite() || now <= tree.images.now {
        return false;
    }
    let before = tree.images.now;
    let nodes = &tree.nodes;
    tree.images
        .started
        .retain(|node, _| nodes.contains_key(node));
    let changed = tree.images.started.iter().any(|(&node, &start)| {
        select::current(tree, env, node).is_some_and(|current| {
            current.image.frame_at(before - start) != current.image.frame_at(now - start)
        })
    });
    tree.images.now = now;
    changed
}

/// Whether an element's animated image still changes frames.
pub(crate) fn animating(tree: &crate::tree::Tree, env: &crate::style::Environment) -> bool {
    tree.images.started.iter().any(|(&node, &start)| {
        tree.attached(node)
            && select::current(tree, env, node)
                .is_some_and(|current| current.image.playing(tree.images.now - start))
    })
}

/// Skia's premultiplication of one 8-bit channel (`SkMulDiv255Round`).
fn premultiply(channel: u8, alpha: u8) -> u8 {
    let product = u32::from(channel) * u32::from(alpha) + 128;
    // At most (255 * 255 + 128 + 254) >> 8 = 255.
    ((product + (product >> 8)) >> 8) as u8
}

/// Straight-alpha RGBA8 pixels premultiplied in place.
fn premultiplied(mut rgba: Vec<u8>) -> Vec<u8> {
    for pixel in rgba.as_chunks_mut::<DECODED_PIXEL_BYTES>().0.iter_mut() {
        let alpha = pixel[3];
        for channel in &mut pixel[..3] {
            *channel = premultiply(*channel, alpha);
        }
    }
    rgba
}

#[cfg(test)]
pub(crate) mod tests;
