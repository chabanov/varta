//! Flat isolated groups and border color fields use small reusable pixel tiles.
//! The source list remains under the existing scene cache limits. No recursive
//! replay or viewport allocation for each depth. Native GPU limits stay fixed;
//! group scenes use the established raster preparation path for now.
#[path = "group_raster.rs"]
mod raster;
use super::*;
use raster::Raster;
use tiny_skia::{BlendMode, PixmapPaint};

const TILE_EDGE: u32 = 128;
const MAX_GROUP_DEPTH: usize = crate::layout::MAX_LAYOUT_DEPTH + 1;

pub(super) fn present(commands: &[Draw]) -> bool {
    commands.iter().any(|d| {
        matches!(
            d.primitive,
            Primitive::BeginGroup { .. } | Primitive::EndGroup | Primitive::Border(_)
        )
    })
}
fn depth(commands: &[Draw]) -> Result<usize, PaintError> {
    let mut stack = Vec::new();
    let mut maximum = 0;
    for (index, draw) in commands.iter().enumerate() {
        match draw.primitive {
            Primitive::BeginGroup { opacity, end, .. } => {
                if !opacity.is_finite()
                    || !(0.0..=1.0).contains(&opacity)
                    || end <= index
                    || end >= commands.len()
                    || !matches!(commands[end].primitive, Primitive::EndGroup)
                    || stack.last().is_some_and(|parent| end >= *parent)
                    || stack.len() >= MAX_GROUP_DEPTH
                {
                    return Err(PaintError::Unsupported(varta_common::view::ROOT_NODE));
                }
                stack.push(end);
                maximum = maximum.max(stack.len());
            }
            Primitive::EndGroup if stack.pop() != Some(index) => {
                return Err(PaintError::Unsupported(varta_common::view::ROOT_NODE));
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(PaintError::Unsupported(varta_common::view::ROOT_NODE));
    }
    Ok(maximum)
}
fn blend(commands: &[Draw], stack: &[usize]) -> BlendMode {
    if stack.last().is_some_and(|index| {
        matches!(
            commands[*index].primitive,
            Primitive::BeginGroup { additive: true, .. }
        )
    }) {
        BlendMode::Plus
    } else {
        BlendMode::SourceOver
    }
}
fn tile(
    layers: &mut [Pixmap],
    commands: &[Draw],
    raster: &mut Raster,
    stack: &mut Vec<usize>,
    x: u32,
    y: u32,
) {
    stack.clear();
    let mut index = 0;
    while index < commands.len() {
        let draw = &commands[index];
        match draw.primitive {
            Primitive::BeginGroup { opacity, end, .. } => {
                if opacity == 0.0 || !raster.masks.visible(draw) {
                    index = end + 1;
                    continue;
                }
                stack.push(index);
                layers[stack.len()].fill(Color::TRANSPARENT);
            }
            Primitive::EndGroup => {
                let level = stack.len();
                let entry = stack.pop().expect("balanced group program was preflighted");
                let group = &commands[entry];
                let Primitive::BeginGroup { opacity, .. } = group.primitive else {
                    unreachable!()
                };
                let paint = PixmapPaint {
                    opacity,
                    blend_mode: blend(commands, stack),
                    ..PixmapPaint::default()
                };
                let (parents, source) = layers.split_at_mut(level);
                raster.composite(&mut parents[level - 1], &source[0], group, &paint, x, y);
            }
            _ => raster.draw(
                &mut layers[stack.len()],
                index,
                draw,
                blend(commands, stack),
                x,
                y,
            ),
        }
        index += 1;
    }
}
pub(super) fn replay(image: &mut Pixmap, commands: &[Draw], dirty: Rect) -> Result<(), PaintError> {
    let depth = depth(commands)?;
    // Allocate every pixel layer and mask before changing any destination pixel.
    // At native maximum depth this is less than9MiB, independent of viewport.
    let mut layers = (0..=depth)
        .map(|_| Pixmap::new(TILE_EDGE, TILE_EDGE).ok_or(PaintError::Dimensions))
        .collect::<Result<Vec<_>, _>>()?;
    let mut raster = Raster::new(commands, dirty, image.width(), image.height())?;
    let mut stack = Vec::with_capacity(depth);
    let left = dirty.left() as u32;
    let top = dirty.top() as u32;
    let right = (dirty.right() as u32).min(image.width());
    let bottom = (dirty.bottom() as u32).min(image.height());
    let width = image.width() as usize;
    for y in (top / TILE_EDGE * TILE_EDGE..bottom).step_by(TILE_EDGE as usize) {
        for x in (left / TILE_EDGE * TILE_EDGE..right).step_by(TILE_EDGE as usize) {
            let w = TILE_EDGE.min(image.width() - x) as usize;
            let h = TILE_EDGE.min(image.height() - y) as usize;
            layers[0].fill(Color::TRANSPARENT);
            for row in 0..h {
                let source = ((y as usize + row) * width + x as usize) * 4;
                let destination = row * TILE_EDGE as usize * 4;
                layers[0].data_mut()[destination..destination + w * 4]
                    .copy_from_slice(&image.data()[source..source + w * 4]);
            }
            let extent = Rect::from_xywh(x as f32, y as f32, w as f32, h as f32)
                .ok_or(PaintError::Dimensions)?;
            raster.masks.dirty = extent.intersect(&dirty).ok_or(PaintError::Dimensions)?;
            tile(&mut layers, commands, &mut raster, &mut stack, x, y);
            let d = raster.masks.dirty;
            for row in d.top() as usize..d.bottom() as usize {
                let destination = (row * width + d.left() as usize) * 4;
                let source =
                    ((row - y as usize) * TILE_EDGE as usize + d.left() as usize - x as usize) * 4;
                let bytes = d.width() as usize * 4;
                image.data_mut()[destination..destination + bytes]
                    .copy_from_slice(&layers[0].data()[source..source + bytes]);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "groups_tests.rs"]
mod tests;
