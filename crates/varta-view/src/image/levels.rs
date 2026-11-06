//! Skia's downscaling level chain (`SkMipmap`): each level halves every axis
//! longer than one pixel (rounding down) until one pixel remains, filtering
//! premultiplied channels with integer kernels that truncate: a two-tap box
//! on an axis of even length, a 1-2-1 kernel on an odd one, and a single tap
//! on an axis of one pixel.
use super::{Image, ImageError, Level};
use varta_common::view::images::DECODED_PIXEL_BYTES;

/// Source taps (offsets from twice the output index) and weights along an
/// axis of `length`.
fn taps(length: usize) -> &'static [(usize, u32)] {
    const SINGLE: &[(usize, u32)] = &[(0, 1)];
    const PAIR: &[(usize, u32)] = &[(0, 1), (1, 1)];
    const TRIPLE: &[(usize, u32)] = &[(0, 1), (1, 2), (2, 1)];
    match length {
        1 => SINGLE,
        length if length % 2 == 1 => TRIPLE,
        _ => PAIR,
    }
}

/// The level after `level`.
fn halve(level: &Level) -> Level {
    let (width, height) = (level.width as usize, level.height as usize);
    let next_width = (width / 2).max(1);
    let next_height = (height / 2).max(1);
    let mut pixels = vec![0; next_width * next_height * DECODED_PIXEL_BYTES];
    for y in 0..next_height {
        // A one-pixel axis keeps its single row or column.
        let row_origin = if height == 1 { 0 } else { 2 * y };
        for x in 0..next_width {
            let column_origin = if width == 1 { 0 } else { 2 * x };
            let mut sum = [0u32; DECODED_PIXEL_BYTES];
            let mut weight = 0u32;
            for &(dy, wy) in taps(height) {
                for &(dx, wx) in taps(width) {
                    let at = ((row_origin + dy) * width + column_origin + dx) * DECODED_PIXEL_BYTES;
                    for (channel, total) in sum.iter_mut().enumerate() {
                        *total += u32::from(level.pixels[at + channel]) * wx * wy;
                    }
                    weight += wx * wy;
                }
            }
            let out = (y * next_width + x) * DECODED_PIXEL_BYTES;
            for (channel, total) in sum.into_iter().enumerate() {
                // Weights sum to a power of two; Skia shifts (truncates).
                pixels[out + channel] = (total / weight) as u8;
            }
        }
    }
    Level {
        width: next_width as u32,
        height: next_height as u32,
        pixels,
    }
}

/// The image and its level chain, charging every level's bytes to `charge`
/// before it is allocated.
pub(super) fn build(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    mut charge: impl FnMut(usize) -> Result<(), ImageError>,
) -> Result<Image, ImageError> {
    charge(pixels.len())?;
    let mut levels = vec![Level {
        width,
        height,
        pixels,
    }];
    loop {
        let last = levels.last().expect("the image is the first level");
        if last.width == 1 && last.height == 1 {
            break;
        }
        let bytes = (last.width as usize / 2).max(1)
            * (last.height as usize / 2).max(1)
            * DECODED_PIXEL_BYTES;
        charge(bytes)?;
        let next = halve(last);
        levels.push(next);
    }
    Ok(Image { levels })
}
