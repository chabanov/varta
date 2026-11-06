//! Image files of the native image probe, encoded at run time with the
//! image-rs encoders (no binary fixtures): every admitted format, colour
//! chunks and profiles, EXIF orientation and animations.
use image::codecs::{bmp, gif, ico, jpeg, png as png_codec, webp};
use image::{ExtendedColorType, ImageEncoder};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// JPEG quality of the probe's JPEG files.
const JPEG_QUALITY: u8 = 90;
/// The EXIF Orientation tag and the TIFF SHORT type.
const ORIENTATION_TAG: u16 = 0x0112;
const TIFF_SHORT: u16 = 3;
/// PNG `cHRM`/`gAMA` scale.
const PNG_SCALE: f64 = 100_000.0;

/// The test pattern: a two-pixel checker in red, a horizontal green ramp, a
/// vertical blue ramp and, for `alpha`, a repeating alpha ramp.
pub fn pattern(width: u32, height: u32, alpha: bool) -> Vec<u8> {
    let ramp = |value: u32, length: u32| (255 * value / (length - 1).max(1)) as u8;
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let checker = ((x >> 1) + (y >> 1)) & 1 == 1;
            pixels.extend([
                if checker { 230 } else { 20 },
                ramp(x, width),
                ramp(y, height),
                if alpha {
                    (255 * ((x + y) % 5) / 4) as u8
                } else {
                    255
                },
            ]);
        }
    }
    pixels
}

/// A solid `width` x `height` RGBA image.
pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    (0..width * height).flat_map(|_| rgba).collect()
}

/// An asymmetric image whose orientation shows: a red top-left quadrant,
/// green top right, blue bottom left and the pattern elsewhere.
pub fn quadrants(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = pattern(width, height, false);
    for y in 0..height {
        for x in 0..width {
            let color = match (x < width / 2, y < height / 2) {
                (true, true) => [230, 20, 20, 255],
                (false, true) => [20, 200, 20, 255],
                (true, false) => [20, 20, 230, 255],
                (false, false) => continue,
            };
            let at = ((y * width + x) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&color);
        }
    }
    pixels
}

fn rgb(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|[red, green, blue, _]| [*red, *green, *blue])
        .collect()
}

/// A PNG of RGBA `pixels` with extra chunks written before `IDAT`.
pub fn png(
    width: u32,
    height: u32,
    pixels: &[u8],
    chunks: &[(&[u8; 4], Vec<u8>)],
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        for (kind, data) in chunks {
            writer.write_chunk(png::chunk::ChunkType(**kind), data)?;
        }
        writer.write_image_data(pixels)?;
    }
    Ok(out)
}

/// A PNG with an embedded ICC profile.
pub fn png_icc(width: u32, height: u32, pixels: &[u8], icc: Vec<u8>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png_codec::PngEncoder::new(&mut out);
    encoder.set_icc_profile(icc).map_err(|e| e.to_string())?;
    encoder.write_image(pixels, width, height, ExtendedColorType::Rgba8)?;
    Ok(out)
}

/// A `gAMA` chunk of file gamma `gamma`.
pub fn gama(gamma: f64) -> Vec<u8> {
    ((gamma * PNG_SCALE).round() as u32).to_be_bytes().to_vec()
}

/// A `cHRM` chunk of white, red, green and blue chromaticities.
pub fn chrm(points: [(f64, f64); 4]) -> Vec<u8> {
    points
        .iter()
        .flat_map(|(x, y)| [*x, *y])
        .flat_map(|value| ((value * PNG_SCALE).round() as u32).to_be_bytes())
        .collect()
}

/// A TIFF structure with one Orientation entry (little-endian), the body
/// of a PNG `eXIf` chunk and of a JPEG `Exif` segment.
pub fn exif(orientation: u16) -> Vec<u8> {
    let mut tiff = b"II*\0".to_vec();
    tiff.extend(8u32.to_le_bytes());
    tiff.extend(1u16.to_le_bytes());
    tiff.extend(ORIENTATION_TAG.to_le_bytes());
    tiff.extend(TIFF_SHORT.to_le_bytes());
    tiff.extend(1u32.to_le_bytes());
    tiff.extend(u32::from(orientation).to_le_bytes());
    tiff.extend(0u32.to_le_bytes());
    tiff
}

/// A JPEG of `pixels` (RGB), with an ICC profile and an EXIF orientation
/// when given.
pub fn jpeg(
    width: u32,
    height: u32,
    pixels: &[u8],
    icc: Option<Vec<u8>>,
    orientation: Option<u16>,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
    if let Some(icc) = icc {
        encoder.set_icc_profile(icc).map_err(|e| e.to_string())?;
    }
    encoder.write_image(&rgb(pixels), width, height, ExtendedColorType::Rgb8)?;
    if let Some(orientation) = orientation {
        // APP1 `Exif\0\0` right after SOI.
        let mut body = b"Exif\0\0".to_vec();
        body.extend(exif(orientation));
        let mut segment = vec![0xff, 0xe1];
        segment.extend(((body.len() + 2) as u16).to_be_bytes());
        segment.extend(body);
        out.splice(2..2, segment);
    }
    Ok(out)
}

/// A lossless WebP of RGBA `pixels`.
pub fn webp(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    webp::WebPEncoder::new_lossless(&mut out).encode(
        pixels,
        width,
        height,
        ExtendedColorType::Rgba8,
    )?;
    Ok(out)
}

/// A lossless WebP of RGBA `pixels` with an ICC profile.
pub fn webp_icc(width: u32, height: u32, pixels: &[u8], icc: Vec<u8>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = webp::WebPEncoder::new_lossless(&mut out);
    encoder.set_icc_profile(icc).map_err(|e| e.to_string())?;
    encoder.write_image(pixels, width, height, ExtendedColorType::Rgba8)?;
    Ok(out)
}

/// A BMP of RGBA `pixels`.
pub fn bmp(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    bmp::BmpEncoder::new(&mut out).encode(pixels, width, height, ExtendedColorType::Rgba8)?;
    Ok(out)
}

/// An ICO with one PNG entry of RGBA `pixels`.
pub fn ico(width: u32, height: u32, pixels: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let frame = ico::IcoFrame::as_png(pixels, width, height, ExtendedColorType::Rgba8)?;
    ico::IcoEncoder::new(&mut out).encode_images(&[frame])?;
    Ok(out)
}

/// An animated GIF of RGBA `frames` with delays in milliseconds (multiples
/// of 10) and a NETSCAPE loop count (`None` for none).
pub fn gif(
    (width, height): (u32, u32),
    frames: &[(Vec<u8>, u32)],
    repeat: Option<u16>,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = gif::GifEncoder::new(&mut out);
        if let Some(repeat) = repeat {
            encoder.set_repeat(if repeat == 0 {
                gif::Repeat::Infinite
            } else {
                gif::Repeat::Finite(repeat)
            })?;
        }
        for (pixels, delay) in frames {
            let buffer =
                image::RgbaImage::from_raw(width, height, pixels.clone()).ok_or("frame size")?;
            encoder.encode_frame(image::Frame::from_parts(
                buffer,
                0,
                0,
                image::Delay::from_numer_denom_ms(*delay, 1),
            ))?;
        }
    }
    Ok(out)
}

/// An APNG of RGBA `frames` with delays in milliseconds and `plays` (0 for
/// forever).
pub fn apng((width, height): (u32, u32), frames: &[(Vec<u8>, u16)], plays: u32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(frames.len() as u32, plays)?;
        let mut writer = encoder.write_header()?;
        for (pixels, delay) in frames {
            writer.set_frame_delay(*delay, 1000)?;
            writer.write_image_data(pixels)?;
        }
        writer.finish()?;
    }
    Ok(out)
}

fn riff_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = kind.to_vec();
    chunk.extend((data.len() as u32).to_le_bytes());
    chunk.extend(data);
    if data.len() % 2 == 1 {
        chunk.push(0);
    }
    chunk
}

fn u24(value: u32) -> [u8; 3] {
    let [a, b, c, _] = value.to_le_bytes();
    [a, b, c]
}

/// An animated WebP (RFC 9649 §2.7) of lossless RGBA `frames` with
/// durations in milliseconds and a loop count (0 for forever): `VP8X`,
/// `ANIM` and one `ANMF` per frame wrapping its `VP8L` bitstream.
pub fn webp_animation(
    (width, height): (u32, u32),
    frames: &[(Vec<u8>, u32)],
    loops: u16,
) -> Result<Vec<u8>> {
    const ANIMATION: u8 = 0x02;
    const ALPHA: u8 = 0x10;
    let mut body = b"WEBP".to_vec();
    let mut vp8x = vec![ANIMATION | ALPHA, 0, 0, 0];
    vp8x.extend(u24(width - 1));
    vp8x.extend(u24(height - 1));
    body.extend(riff_chunk(b"VP8X", &vp8x));
    let mut anim = vec![0, 0, 0, 0];
    anim.extend(loops.to_le_bytes());
    body.extend(riff_chunk(b"ANIM", &anim));
    for (pixels, duration) in frames {
        let still = webp(width, height, pixels)?;
        // The still file is `RIFF size WEBP VP8L ...`: keep the VP8L chunk.
        let vp8l = &still[12..];
        let mut anmf = Vec::new();
        anmf.extend(u24(0));
        anmf.extend(u24(0));
        anmf.extend(u24(width - 1));
        anmf.extend(u24(height - 1));
        anmf.extend(u24(*duration));
        // Blend with alpha (bit 1 clear), no disposal.
        anmf.push(0);
        anmf.extend(vp8l);
        body.extend(riff_chunk(b"ANMF", &anmf));
    }
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    Ok(out)
}
