//! Backdrop-root prefix capture and a bounded three-box Gaussian approximation.
//! Prefixes are clipped to the element border box and mirrored at its edges
//! (Filter Effects 2). They are resolved before tiled composition, so no tile seam can become
//! an artificial filter edge. The ordinary preparation/cache budgets are shared.
use super::*;
use std::sync::Arc;
use tiny_skia::{FilterQuality, Pattern, SpreadMode};
use varta_common::view::CssProperty as P;

#[derive(Clone, PartialEq)]
pub(crate) struct Backdrop {
    pub bounds: Rect,
    path: tiny_skia::Path,
    pixels: Pixmap,
    origin: [f32; 2],
    scale: f32,
}
impl Backdrop {
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.pixels.data().len()
            + std::mem::size_of_val(self.path.points())
            + self.path.verbs().len()
    }
    pub fn draw(&self, image: &mut Pixmap, raster: shadow::Raster<'_>) {
        let shadow::Raster {
            transform,
            origin,
            color,
            blend,
            mask,
            ..
        } = raster;
        let paint = Paint {
            shader: Pattern::new(
                self.pixels.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bilinear,
                color.alpha(),
                Transform::from_scale(self.scale.recip(), self.scale.recip())
                    .post_translate(self.origin[0], self.origin[1]),
            ),
            blend_mode: blend,
            anti_alias: true,
            ..Paint::default()
        };
        image.fill_path(
            &self.path,
            &paint,
            FillRule::Winding,
            transform.post_translate(-(origin.0 as f32), -(origin.1 as f32)),
            mask,
        );
    }
}
pub(crate) fn establishes(properties: &BTreeMap<P, String>) -> bool {
    properties
        .get(&P::BackdropFilter)
        .is_some_and(|v| v != "none")
}
fn charge(work: &mut usize, amount: usize, id: u32) -> Result<(), PaintError> {
    *work = work.saturating_add(amount);
    if *work > MAX_TEXT_PAINT_WORK {
        return Err(PaintError::Unsupported(id));
    }
    Ok(())
}
fn shifted(
    source: &[Draw],
    start: usize,
    dx: f32,
    dy: f32,
    work: &mut usize,
    id: u32,
) -> Result<Vec<Draw>, PaintError> {
    charge(work, source.len().saturating_mul(2), id)?;
    let mut clips = BTreeMap::<usize, Arc<Clip>>::new();
    let mut draws = Vec::new();
    draws
        .try_reserve_exact(source.len())
        .map_err(|_| PaintError::Dimensions)?;
    let translate = Transform::from_translate(dx, dy);
    for draw in source {
        let mut draw = draw.clone();
        draw.transform = draw.transform.post_concat(translate);
        draw.bounds = draw
            .bounds
            .transform(translate)
            .ok_or(PaintError::Dimensions)?;
        draw.clip = draw.clip.and_then(|b| b.transform(translate));
        if let Primitive::BeginGroup { end, .. } = &mut draw.primitive {
            *end = end.checked_sub(start).ok_or(PaintError::Unsupported(id))?;
        }
        let mut missing = Vec::new();
        let mut current = draw.clips.clone();
        let mut parent = None;
        while let Some(clip) = current {
            let key = Arc::as_ptr(&clip) as usize;
            if let Some(found) = clips.get(&key) {
                parent = Some(found.clone());
                break;
            }
            charge(
                work,
                1 + clip.path.as_ref().map_or(0, |p| p.verbs().len()),
                id,
            )?;
            current = clip.parent.clone();
            missing.push(clip);
        }
        for clip in missing.into_iter().rev() {
            let key = Arc::as_ptr(&clip) as usize;
            let translated = Clip::translated(&clip, dx, dy, parent)?;
            clips.insert(key, translated.clone());
            parent = Some(translated);
        }
        draw.clips = parent;
        draws.push(draw);
    }
    Ok(draws)
}
/// Constant work per pixel, even for a blur much wider than the image.
/// The initial sum counts complete mirror periods without a radius-sized loop.
fn box_pass(
    source: &[u8],
    target: &mut [u8],
    width: usize,
    height: usize,
    radius: f64,
    vertical: bool,
) {
    let radius = radius.clamp(0.0, 1e12) as usize;
    let divisor = (2 * radius + 1) as f64;
    let (length, lines, stride) = if vertical {
        (height, width, width)
    } else {
        (width, height, 1)
    };
    for line in 0..lines {
        let base = if vertical { line } else { line * width };
        for channel in 0..4 {
            let at = |i: usize| (base + i * stride) * 4 + channel;
            let period = (2 * length) as i64;
            let reflect = |index: i64| {
                let n = index.rem_euclid(period);
                if n >= length as i64 {
                    (period - 1 - n) as usize
                } else {
                    n as usize
                }
            };
            let window = 2 * radius + 1;
            // Each sample occurs twice in one mirror period. Count complete
            // periods analytically; the remainder is shorter than two lines.
            let period_sum = 2.0 * (0..length).map(|i| f64::from(source[at(i)])).sum::<f64>();
            let mut sum = period_sum * (window / (2 * length)) as f64;
            for i in 0..window % (2 * length) {
                sum += f64::from(source[at(reflect(-(radius as i64) + i as i64))]);
            }
            for i in 0..length {
                target[at(i)] = (sum / divisor).round().clamp(0.0, 255.0) as u8;
                let entering = reflect(i as i64 + radius as i64 + 1);
                let leaving = reflect(i as i64 - radius as i64);
                sum += f64::from(source[at(entering)]) - f64::from(source[at(leaving)]);
            }
        }
    }
}
fn blur(image: &mut Pixmap, sigma: f64) -> Result<(), PaintError> {
    if sigma <= 0.0 {
        return Ok(());
    }
    // Variance-matched three box filters, including fractional Gaussian widths.
    let ideal = (4.0 * sigma * sigma + 1.0).sqrt();
    let mut low = ideal.floor();
    if low % 2.0 == 0.0 {
        low -= 1.0;
    }
    low = low.max(1.0);
    let nlow = ((12.0 * sigma * sigma - 3.0 * low * low - 12.0 * low - 9.0) / (-4.0 * low - 4.0))
        .round()
        .clamp(0.0, 3.0) as usize;
    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(image.data().len())
        .map_err(|_| PaintError::Dimensions)?;
    scratch.resize(image.data().len(), 0);
    let (w, h) = (image.width() as usize, image.height() as usize);
    for i in 0..3 {
        let radius = (low + if i < nlow { 0.0 } else { 2.0 } - 1.0) * 0.5;
        box_pass(image.data(), &mut scratch, w, h, radius, false);
        box_pass(&scratch, image.data_mut(), w, h, radius, true);
    }
    Ok(())
}
pub(super) fn paint(
    surface: &mut Surface<'_>,
    properties: &BTreeMap<P, String>,
    id: u32,
    rect: BoxRect,
    corners: Corners,
    units: Units,
    work: &mut usize,
) -> Result<(), PaintError> {
    if surface.suppressed || surface.groups.last().is_some_and(|s| s.hidden) {
        return Ok(());
    }
    // Transforming the backdrop-root image requires a separate inverse sampling
    // boundary. Refuse it rather than blur a differently located prefix.
    if !surface.affines.is_empty() || surface.capture.is_some() {
        return Err(PaintError::Unsupported(id));
    }
    let raw = properties[&P::BackdropFilter].trim();
    let arg = raw
        .strip_prefix("blur(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or(PaintError::Unsupported(id))?
        .trim();
    let sigma = if arg.is_empty() {
        0.0
    } else {
        f64::from(
            units
                .pixels(arg, false)
                .ok_or(PaintError::Unsupported(id))?,
        )
    };
    if sigma < 0.0 || !sigma.is_finite() {
        return Err(PaintError::Unsupported(id));
    }
    let device = scaled_rect(rect, surface.scale);
    let left = (f64::from(device.x))
        .floor()
        .max(0.0)
        .min(f64::from(surface.width)) as u32;
    let top = (f64::from(device.y))
        .floor()
        .max(0.0)
        .min(f64::from(surface.height)) as u32;
    let right = (f64::from(device.x + device.width))
        .ceil()
        .max(0.0)
        .min(f64::from(surface.width)) as u32;
    let bottom = (f64::from(device.y + device.height))
        .ceil()
        .max(0.0)
        .min(f64::from(surface.height)) as u32;
    if right <= left || bottom <= top {
        return Ok(());
    }
    let (w, h) = (right - left, bottom - top);
    charge(
        work,
        (w as usize).saturating_mul(h as usize).saturating_mul(7),
        id,
    )?;
    let mut image = Pixmap::new(w, h).ok_or(PaintError::Dimensions)?;
    // Current element group starts before the filter; the nearest ancestor group
    // is the backdrop root and its own opacity/clip is applied only afterwards.
    let own = surface
        .groups
        .last()
        .filter(|s| s.node == id)
        .and_then(|s| s.start);
    let start = surface
        .groups
        .iter()
        .rev()
        .find(|s| s.node != id)
        .and_then(|s| s.start)
        .map_or(0, |s| s + 1);
    let end = own.unwrap_or_else(|| surface.commands.as_ref().map_or(0, |r| r.commands.len()));
    if start == 0
        && let Some(prefix) = &surface.pixels
    {
        for y in 0..h as usize {
            let from = ((top as usize + y) * surface.width as usize + left as usize) * 4;
            image.data_mut()[y * w as usize * 4..(y + 1) * w as usize * 4]
                .copy_from_slice(&prefix.data()[from..from + w as usize * 4]);
        }
    }
    if let Some(recording) = &surface.commands {
        let draws = shifted(
            &recording.commands[start..end],
            start,
            -(left as f32),
            -(top as f32),
            work,
            id,
        )?;
        scene::replay(&mut image, &draws)?;
    }
    blur(&mut image, sigma * f64::from(surface.scale))?;
    let path = corners.clipped_path(rect).ok_or(PaintError::Dimensions)?;
    let bounds = path.bounds();
    let source = Backdrop {
        bounds,
        path,
        pixels: image,
        origin: [left as f32 / surface.scale, top as f32 / surface.scale],
        scale: surface.scale,
    };
    let mut paint = Paint::default();
    paint.set_color(Color::WHITE);
    surface.record(
        Primitive::Backdrop(Arc::new(source)),
        &paint,
        Transform::identity(),
        None,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::box_pass;
    #[test]
    fn sliding_mirror_filter_matches_direct_convolution() {
        for width in 1..12 {
            for height in 1..7 {
                for radius in 0..18 {
                    let source: Vec<u8> = (0..width * height * 4)
                        .map(|i| ((i * 37 + 11) % 256) as u8)
                        .collect();
                    for vertical in [false, true] {
                        let mut out = vec![0; source.len()];
                        box_pass(&source, &mut out, width, height, radius as f64, vertical);
                        for y in 0..height {
                            for x in 0..width {
                                for c in 0..4 {
                                    let mut total = 0.0;
                                    for delta in -(radius as i64)..=radius as i64 {
                                        let length = if vertical { height } else { width };
                                        let position = if vertical { y } else { x };
                                        let n = (position as i64 + delta)
                                            .rem_euclid((2 * length) as i64)
                                            as usize;
                                        let i = if n < length { n } else { 2 * length - 1 - n };
                                        let (xx, yy) = if vertical { (x, i) } else { (i, y) };
                                        total += source[(yy * width + xx) * 4 + c] as f64;
                                    }
                                    let expected = (total / (2 * radius + 1) as f64).round() as u8;
                                    assert_eq!(
                                        out[(y * width + x) * 4 + c],
                                        expected,
                                        "{width}x{height} radius{radius}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
