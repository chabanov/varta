//! Decoding one packaged image file into straight-alpha sRGB RGBA8 frames,
//! its EXIF orientation and how many times it plays.
//!
//! Formats are told apart by their signatures (`sniff`, WHATWG MIME Sniffing
//! §6.1) and decoded by the image-rs decoders: `png` (PNG and APNG), and the
//! `image` crate's JPEG (`zune-jpeg`), GIF, WebP (`image-webp`), BMP and ICO
//! decoders, which also composite animation frames. Every decoder runs with
//! dimension and allocation limits, and a frame's pixels are charged to the
//! set's decoded budget before they are decoded. Colour is converted to sRGB
//! (`color`); orientation is applied when sampling (`draw`).
use super::color::{self, ColorSource};
use image::{AnimationDecoder, ImageDecoder};
use std::io::Cursor;
use varta_common::view::images::{
    DECODED_PIXEL_BYTES, ImageFormat, MAX_IMAGE_DIMENSION, MAX_IMAGE_FRAMES,
};

/// HTML/Blink: a frame that asks for 10 ms or less is shown for 100 ms (the
/// rule Firefox and Chrome share, so a zero delay never spins).
const MIN_FRAME_MS: u32 = 10;
const SHORT_FRAME_MS: u32 = 100;
/// The NETSCAPE2.0 application extension carrying a GIF's loop count.
const NETSCAPE_LOOP: &[u8] = b"NETSCAPE2.0";
const NETSCAPE_SUBBLOCK: [u8; 2] = [3, 1];
/// A decoder's own allocation ceiling, in multiples of the frame's decoded
/// bytes (working rows, palettes and the composited canvas).
const DECODER_ALLOCATION_FACTOR: u64 = 4;
const BITS_16_MAX: u32 = u16::MAX as u32;
const BITS_8_MAX: u32 = u8::MAX as u32;

/// How many times an animation plays before it stays on its last frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Plays {
    Infinite,
    Finite(u32),
}

/// One frame: straight-alpha sRGB RGBA8 pixels of the whole canvas.
pub(super) struct DecodedFrame {
    pub rgba: Vec<u8>,
    pub duration_ms: u32,
}

/// A decoded image in its stored orientation.
pub(super) struct Decoded {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<DecodedFrame>,
    pub plays: Plays,
    /// The EXIF orientation (1..=8; 1 when there is none).
    pub orientation: u8,
    /// A colour conversion applied after sampling instead of to the frames.
    pub deferred: Option<color::SrgbTransform>,
}

/// Why one image did not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Failure {
    /// A checksum, a compressed stream or the structure is invalid.
    Malformed,
    /// Its dimensions, frames or pixels exceed the limits.
    Limits,
    /// A colour encoding Chrome tone-maps or that has no RGB meaning (HDR
    /// transfers, narrow range, non-RGB matrix coefficients).
    Unsupported,
}

impl From<image::ImageError> for Failure {
    fn from(error: image::ImageError) -> Self {
        match error {
            image::ImageError::Limits(_) => Self::Limits,
            _ => Self::Malformed,
        }
    }
}

/// The decoder limits of one image within `budget` decoded bytes.
fn limits(budget: usize) -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some((budget as u64).saturating_mul(DECODER_ALLOCATION_FACTOR));
    limits
}

/// Charge one `width` x `height` frame to `budget`.
fn charge(budget: &mut usize, (width, height): (u32, u32)) -> Result<(), Failure> {
    if width == 0 || height == 0 || width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err(Failure::Limits);
    }
    let bytes = width as usize * height as usize * DECODED_PIXEL_BYTES;
    *budget = budget.checked_sub(bytes).ok_or(Failure::Limits)?;
    Ok(())
}

/// A frame duration in milliseconds, with the short-frame rule.
fn duration(delay: image::Delay) -> u32 {
    let (numerator, denominator) = delay.numer_denom_ms();
    let ms = numerator.checked_div(denominator).unwrap_or(0);
    if ms <= MIN_FRAME_MS {
        SHORT_FRAME_MS
    } else {
        ms
    }
}

/// Decode `bytes` of `format` within `budget` decoded bytes.
pub(super) fn decode(
    bytes: &[u8],
    format: ImageFormat,
    budget: &mut usize,
) -> Result<Decoded, Failure> {
    match format {
        ImageFormat::Png => png(bytes, budget),
        ImageFormat::Jpeg => jpeg(bytes, budget),
        ImageFormat::Bmp => still(
            image::codecs::bmp::BmpDecoder::new(Cursor::new(bytes))?,
            ColorSource::Untagged,
            budget,
        ),
        ImageFormat::Ico => still(
            image::codecs::ico::IcoDecoder::new(Cursor::new(bytes))?,
            ColorSource::Untagged,
            budget,
        ),
        ImageFormat::Gif => gif(bytes, budget),
        ImageFormat::WebP => webp(bytes, budget),
    }
}

/// A still image: its pixels, embedded profile and orientation.
fn still(
    mut decoder: impl ImageDecoder,
    fallback: ColorSource,
    budget: &mut usize,
) -> Result<Decoded, Failure> {
    decoder.set_limits(limits(*budget))?;
    charge(budget, decoder.dimensions())?;
    let source = match decoder.icc_profile()? {
        Some(profile) => ColorSource::Icc(profile),
        None => fallback,
    };
    let orientation = decoder.orientation()?.to_exif();
    let (width, height) = decoder.dimensions();
    let pixels = image::DynamicImage::from_decoder(decoder)?;
    let rgba = to_rgba8(pixels);
    let frames = vec![DecodedFrame {
        rgba: color::to_srgb(&source, rgba)?,
        duration_ms: 0,
    }];
    Ok(Decoded {
        width,
        height,
        frames,
        plays: Plays::Finite(1),
        orientation,
        deferred: None,
    })
}

/// JPEG: Chrome draws a JPEG's pixels in its own colour space and converts
/// what it samples (Skia's colour space transform steps at draw), so the
/// frame keeps the decoded pixels and the conversion is deferred.
fn jpeg(bytes: &[u8], budget: &mut usize) -> Result<Decoded, Failure> {
    let mut decoder = image::codecs::jpeg::JpegDecoder::new(Cursor::new(bytes))?;
    decoder.set_limits(limits(*budget))?;
    charge(budget, decoder.dimensions())?;
    let source = match decoder.icc_profile()? {
        Some(profile) => ColorSource::Icc(profile),
        None => ColorSource::Untagged,
    };
    let orientation = decoder.orientation()?.to_exif();
    let (width, height) = decoder.dimensions();
    let pixels = image::DynamicImage::from_decoder(decoder)?;
    Ok(Decoded {
        width,
        height,
        frames: vec![DecodedFrame {
            rgba: to_rgba8(pixels),
            duration_ms: 0,
        }],
        plays: Plays::Finite(1),
        orientation,
        deferred: color::srgb_transform(&source)?,
    })
}

/// 8-bit straight RGBA of any decoded colour type; 16-bit channels round to
/// the nearest 8-bit value.
fn to_rgba8(pixels: image::DynamicImage) -> Vec<u8> {
    match pixels {
        image::DynamicImage::ImageRgba16(_)
        | image::DynamicImage::ImageRgb16(_)
        | image::DynamicImage::ImageLuma16(_)
        | image::DynamicImage::ImageLumaA16(_) => pixels
            .to_rgba16()
            .into_raw()
            .into_iter()
            .map(|value| ((u32::from(value) * BITS_8_MAX + BITS_16_MAX / 2) / BITS_16_MAX) as u8)
            .collect(),
        pixels => pixels.into_rgba8().into_raw(),
    }
}

/// The frames of an animation decoder, each charged before it is kept.
fn frames<'a>(
    decoder: impl AnimationDecoder<'a>,
    dimensions: (u32, u32),
    source: &ColorSource,
    budget: &mut usize,
) -> Result<Vec<DecodedFrame>, Failure> {
    let mut frames = Vec::new();
    for frame in decoder.into_frames() {
        if frames.len() == MAX_IMAGE_FRAMES {
            return Err(Failure::Limits);
        }
        charge(budget, dimensions)?;
        let frame = frame?;
        let duration_ms = duration(frame.delay());
        let buffer = frame.into_buffer();
        if buffer.dimensions() != dimensions {
            return Err(Failure::Malformed);
        }
        frames.push(DecodedFrame {
            rgba: color::to_srgb(source, buffer.into_raw())?,
            duration_ms,
        });
    }
    if frames.is_empty() {
        return Err(Failure::Malformed);
    }
    Ok(frames)
}

/// The total plays of a loop count read by `image`: an APNG's `num_plays`
/// and a WebP's loop count are total plays, zero meaning forever.
fn plays(count: image::metadata::LoopCount) -> Plays {
    match count {
        image::metadata::LoopCount::Infinite => Plays::Infinite,
        image::metadata::LoopCount::Finite(count) => Plays::Finite(count.get()),
    }
}

/// PNG and APNG (PNG 3rd edition): the colour chunks in their priority
/// order and the animation, when an `acTL` chunk is present.
fn png(bytes: &[u8], budget: &mut usize) -> Result<Decoded, Failure> {
    let reader = ::png::Decoder::new(Cursor::new(bytes))
        .read_info()
        .map_err(|_| Failure::Malformed)?;
    let info = reader.info();
    let source = color::png_source(info);
    let animated = info
        .animation_control
        .is_some_and(|control| control.num_frames > 1);
    drop(reader);
    let mut decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))?;
    decoder.set_limits(limits(*budget))?;
    let orientation = decoder.orientation()?.to_exif();
    let (width, height) = decoder.dimensions();
    if animated {
        let animation = decoder.apng()?;
        let plays = plays(animation.loop_count());
        let frames = frames(animation, (width, height), &source, budget)?;
        return Ok(Decoded {
            width,
            height,
            frames,
            plays,
            orientation,
            deferred: None,
        });
    }
    charge(budget, (width, height))?;
    let pixels = image::DynamicImage::from_decoder(decoder)?;
    let frames = vec![DecodedFrame {
        rgba: color::to_srgb(&source, to_rgba8(pixels))?,
        duration_ms: 0,
    }];
    Ok(Decoded {
        width,
        height,
        frames,
        plays: Plays::Finite(1),
        orientation,
        deferred: None,
    })
}

/// A GIF's total plays: its NETSCAPE2.0 loop count `n` repeats the
/// animation `n` times after the first (Blink's repetition count), `0`
/// loops forever, and without the extension it plays once.
fn gif_plays(bytes: &[u8]) -> Plays {
    let found = bytes
        .windows(NETSCAPE_LOOP.len())
        .position(|window| window == NETSCAPE_LOOP);
    let Some(at) = found else {
        return Plays::Finite(1);
    };
    let block = at + NETSCAPE_LOOP.len();
    match bytes.get(block..block + 4) {
        Some([size, id, low, high]) if [*size, *id] == NETSCAPE_SUBBLOCK => {
            match u16::from_le_bytes([*low, *high]) {
                0 => Plays::Infinite,
                count => Plays::Finite(u32::from(count) + 1),
            }
        }
        _ => Plays::Finite(1),
    }
}

/// GIF (89a): composited frames; a GIF has no colour profile.
fn gif(bytes: &[u8], budget: &mut usize) -> Result<Decoded, Failure> {
    let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?;
    decoder.set_limits(limits(*budget))?;
    let (width, height) = decoder.dimensions();
    let frames = frames(decoder, (width, height), &ColorSource::Untagged, budget)?;
    let plays = if frames.len() > 1 {
        gif_plays(bytes)
    } else {
        Plays::Finite(1)
    };
    Ok(Decoded {
        width,
        height,
        frames,
        plays,
        orientation: 1,
        deferred: None,
    })
}

/// WebP (RFC 9649): still or animated, with its ICC profile and EXIF.
fn webp(bytes: &[u8], budget: &mut usize) -> Result<Decoded, Failure> {
    let mut decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))?;
    decoder.set_limits(limits(*budget))?;
    let source = match decoder.icc_profile()? {
        Some(profile) => ColorSource::Icc(profile),
        None => ColorSource::Untagged,
    };
    let orientation = decoder.orientation()?.to_exif();
    let (width, height) = decoder.dimensions();
    if !decoder.has_animation() {
        charge(budget, (width, height))?;
        let pixels = image::DynamicImage::from_decoder(decoder)?;
        return Ok(Decoded {
            width,
            height,
            frames: vec![DecodedFrame {
                rgba: color::to_srgb(&source, to_rgba8(pixels))?,
                duration_ms: 0,
            }],
            plays: Plays::Finite(1),
            orientation,
            deferred: None,
        });
    }
    let plays = plays(decoder.loop_count());
    let frames = frames(decoder, (width, height), &source, budget)?;
    Ok(Decoded {
        width,
        height,
        frames,
        plays,
        orientation,
        deferred: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_frames_and_gif_loop_counts_follow_blink() {
        assert_eq!(
            duration(image::Delay::from_numer_denom_ms(0, 1)),
            SHORT_FRAME_MS
        );
        assert_eq!(
            duration(image::Delay::from_numer_denom_ms(10, 1)),
            SHORT_FRAME_MS
        );
        assert_eq!(duration(image::Delay::from_numer_denom_ms(20, 1)), 20);
        assert_eq!(gif_plays(b"GIF89a"), Plays::Finite(1));
        assert_eq!(
            gif_plays(b"..NETSCAPE2.0\x03\x01\x00\x00\x00"),
            Plays::Infinite
        );
        assert_eq!(
            gif_plays(b"..NETSCAPE2.0\x03\x01\x02\x00\x00"),
            Plays::Finite(3)
        );
    }
}
