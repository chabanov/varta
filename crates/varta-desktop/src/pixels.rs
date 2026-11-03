//! Retained white-composited RGB presentation buffer.
use varta_view::RasterImage;

pub(crate) const WHITE_RGB: u32 = 0x00ff_ffff;

// Keep the host's RGB buffer between frames. A dirty frame converts only the
// changed physical pixels; presenting copies packed pixels without per-pixel
// conversion on the window thread. Opaque pixels use a direct byte shuffle.
pub(crate) fn update_rgb(
    image: &RasterImage,
    rgb: &mut Vec<u32>,
    damage: Option<varta_view::BoxRect>,
    reset: bool,
) -> Result<(), String> {
    let width = image.width as usize;
    let height = image.height as usize;
    let count = width.checked_mul(height).ok_or("invalid frame size")?;
    if image.pixels.len() != count.checked_mul(4).ok_or("invalid frame size")? {
        return Err("invalid frame pixels".into());
    }
    let full = reset || rgb.len() != count;
    let rect = if full {
        rgb.resize(count, WHITE_RGB);
        varta_view::BoxRect {
            x: 0.0,
            y: 0.0,
            width: image.width as f32,
            height: image.height as f32,
        }
    } else if let Some(rect) = damage {
        rect
    } else {
        return Ok(());
    };
    if ![rect.x, rect.y, rect.width, rect.height]
        .iter()
        .all(|v| v.is_finite())
        || rect.width < 0.0
        || rect.height < 0.0
    {
        return Err("invalid frame damage".into());
    }
    let left = rect.x.floor().clamp(0.0, width as f32) as usize;
    let right = (rect.x + rect.width).ceil().clamp(0.0, width as f32) as usize;
    let top = rect.y.floor().clamp(0.0, height as f32) as usize;
    let bottom = (rect.y + rect.height).ceil().clamp(0.0, height as f32) as usize;
    let pixels = image.pixels.as_chunks::<4>().0;
    for row in top..bottom {
        let range = row * width + left..row * width + right;
        for (out, pixel) in rgb[range.clone()].iter_mut().zip(&pixels[range]) {
            *out = if pixel[3] == 255 {
                u32::from_be_bytes(*pixel) >> 8
            } else {
                let white = 255 - u32::from(pixel[3]);
                ((u32::from(pixel[0]) + white).min(255) << 16)
                    | ((u32::from(pixel[1]) + white).min(255) << 8)
                    | (u32::from(pixel[2]) + white).min(255)
            };
        }
    }
    Ok(())
}
