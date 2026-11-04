//! Tile-independent AA for paths crossing a tile edge. One reusable global
//! coverage mask, independent of group depth; only its previous bounds clear.
//! Interior paths/rectangles render directly to the small layer pixels.
use super::*;

fn inside(draw: &Draw, x: u32, y: u32) -> bool {
    let b = draw.bounds;
    let extent = [
        b.left().abs(),
        b.top().abs(),
        b.right().abs(),
        b.bottom().abs(),
    ]
    .into_iter()
    .fold(0.0_f32, f32::max);
    let margin = 2.0 + extent * f32::EPSILON * 4.0;
    b.left() >= x as f32 + margin
        && b.top() >= y as f32 + margin
        && b.right() <= x as f32 + TILE_EDGE as f32 - margin
        && b.bottom() <= y as f32 + TILE_EDGE as f32 - margin
}
pub(super) struct Raster {
    pub masks: Masks,
    coverage: Option<Mask>,
    previous: Option<Rect>,
    path: Option<usize>,
    slice: Mask,
    border_pixels: Option<Pixmap>,
    width: u32,
    height: u32,
}
impl Raster {
    pub fn new(
        commands: &[Draw],
        dirty: Rect,
        width: u32,
        height: u32,
    ) -> Result<Self, PaintError> {
        let path = commands.iter().any(|draw| {
            matches!(draw.primitive, Primitive::Path(..) | Primitive::Border(_))
                && draw.bounds.intersect(&dirty).is_some()
        });
        Ok(Self {
            masks: Masks {
                dirty,
                last: None,
                mask: if commands.iter().any(|d| d.clip.is_some()) {
                    Some(Mask::new(width, height).ok_or(PaintError::Dimensions)?)
                } else {
                    None
                },
                complex: if commands.iter().any(|d| d.clips.is_some()) {
                    Some(MaskCache::new(width, height)?)
                } else {
                    None
                },
            },
            coverage: if path {
                Some(Mask::new(width, height).ok_or(PaintError::Dimensions)?)
            } else {
                None
            },
            previous: None,
            path: None,
            slice: Mask::new(TILE_EDGE, TILE_EDGE).ok_or(PaintError::Dimensions)?,
            border_pixels: if commands
                .iter()
                .any(|draw| matches!(draw.primitive, Primitive::Border(_)))
            {
                Some(Pixmap::new(TILE_EDGE, TILE_EDGE).ok_or(PaintError::Dimensions)?)
            } else {
                None
            },
            width,
            height,
        })
    }
    fn cover(&mut self, index: usize, draw: &Draw) {
        if self.path == Some(index) {
            return;
        }
        let coverage = self.coverage.as_mut().expect("path mask was preallocated");
        if let Some(previous) = self.previous {
            for y in previous.top() as usize..previous.bottom() as usize {
                let start = y * self.width as usize + previous.left() as usize;
                coverage.data_mut()[start..start + previous.width() as usize].fill(0);
            }
        }
        let viewport = Rect::from_xywh(0.0, 0.0, self.width as f32, self.height as f32)
            .expect("validated raster viewport");
        let extent = [
            draw.bounds.left().abs(),
            draw.bounds.top().abs(),
            draw.bounds.right().abs(),
            draw.bounds.bottom().abs(),
        ]
        .into_iter()
        .fold(0.0_f32, f32::max);
        self.previous = draw
            .bounds
            .outset(
                2.0 + extent * f32::EPSILON * 4.0,
                2.0 + extent * f32::EPSILON * 4.0,
            )
            .and_then(|b| b.intersect(&viewport))
            .and_then(|b| b.round_out())
            .and_then(|b| {
                Rect::from_xywh(
                    b.x() as f32,
                    b.y() as f32,
                    b.width() as f32,
                    b.height() as f32,
                )
            });
        let (path, rule) = match &draw.primitive {
            Primitive::Path(path, rule) => (path.as_ref(), *rule),
            Primitive::Border(border) => (&border.path, FillRule::EvenOdd),
            _ => unreachable!(),
        };
        coverage.fill_path(path, rule, true, draw.transform);
        self.path = Some(index);
    }
    fn slice_mask(&mut self, draw: &Draw, x: u32, y: u32, coverage: bool) -> Option<&Mask> {
        let clip = self.masks.mask(draw);
        if clip.is_none() && !coverage {
            return None;
        }
        self.slice.clear();
        let w = TILE_EDGE.min(self.width - x) as usize;
        let h = TILE_EDGE.min(self.height - y) as usize;
        for row in 0..h {
            for column in 0..w {
                let source = (y as usize + row) * self.width as usize + x as usize + column;
                let alpha = if coverage {
                    self.coverage.as_ref().expect("path coverage exists").data()[source]
                } else {
                    255
                };
                let clip = clip.map_or(255, |mask| mask.data()[source]);
                self.slice.data_mut()[row * TILE_EDGE as usize + column] =
                    ((u16::from(alpha) * u16::from(clip) + 127) / 255) as u8;
            }
        }
        Some(&self.slice)
    }
    pub fn draw(
        &mut self,
        image: &mut Pixmap,
        index: usize,
        draw: &Draw,
        blend: BlendMode,
        x: u32,
        y: u32,
    ) {
        if !self.masks.visible(draw) {
            return;
        }
        if matches!(draw.primitive, Primitive::Border(_)) {
            self.border(image, index, draw, blend, x, y);
            return;
        }
        let origin = Transform::from_translate(-(x as f32), -(y as f32));
        let covered = matches!(draw.primitive, Primitive::Path(..)) && !inside(draw, x, y);
        if covered {
            self.cover(index, draw);
        }
        let dirty = self.masks.dirty;
        let mask = self.slice_mask(draw, x, y, covered);
        let mut paint = Paint::default();
        paint.set_color(draw.color);
        paint.blend_mode = blend;
        match &draw.primitive {
            Primitive::Rect(rect) => {
                if let Some(rect) = rect
                    .transform(draw.transform)
                    .and_then(|b| b.intersect(&dirty))
                {
                    let integer = draw.color.alpha() == 1.0
                        && [rect.left(), rect.top(), rect.right(), rect.bottom()]
                            .into_iter()
                            .all(|v| v.fract() == 0.0);
                    if draw.transform.is_identity() || integer {
                        if let Some(rect) = rect.transform(origin) {
                            fill_rect(image, rect, &paint, Transform::identity(), mask);
                        }
                    } else {
                        image.fill_path(
                            &PathBuilder::from_rect(rect),
                            &paint,
                            FillRule::Winding,
                            origin,
                            mask,
                        );
                    }
                }
            }
            Primitive::Path(_, _) if covered => fill_rect(
                image,
                Rect::from_xywh(0.0, 0.0, TILE_EDGE as f32, TILE_EDGE as f32)
                    .expect("nonempty tile"),
                &paint,
                Transform::identity(),
                mask,
            ),
            Primitive::Path(path, rule) => image.fill_path(
                path,
                &paint,
                *rule,
                draw.transform.post_concat(origin),
                mask,
            ),
            _ => unreachable!("only drawable operations reach tile rasterization"),
        }
    }
    fn border(
        &mut self,
        image: &mut Pixmap,
        index: usize,
        draw: &Draw,
        blend: BlendMode,
        x: u32,
        y: u32,
    ) {
        let Primitive::Border(border) = &draw.primitive else {
            unreachable!()
        };
        self.cover(index, draw);
        let pixels = self
            .border_pixels
            .as_mut()
            .expect("border tile was preallocated");
        pixels.fill(Color::TRANSPARENT);
        let dirty = self.masks.dirty;
        let Some(bounds) = draw.bounds.intersect(&dirty) else {
            return;
        };
        // Border commands are emitted only with the validated uniform physical
        // scale. Keep coordinates global so a tile boundary cannot shift color AA.
        let scale = f64::from(draw.transform.sx);
        let step = 1.0 / scale;
        const COLOR_BLOCK: u32 = 8;
        // Rect::round_out floors the origin and ceils the size, which need not
        // enclose the far fractional edge. Round the four edges independently.
        let left = bounds.left().floor().max(x as f32) as u32;
        let top = bounds.top().floor().max(y as f32) as u32;
        let right = (bounds.right().ceil() as u32).min(x + TILE_EDGE);
        let bottom = (bounds.bottom().ceil() as u32).min(y + TILE_EDGE);
        for by in (top..bottom).step_by(COLOR_BLOCK as usize) {
            for bx in (left..right).step_by(COLOR_BLOCK as usize) {
                let w = COLOR_BLOCK.min(right - bx);
                let h = COLOR_BLOCK.min(bottom - by);
                let css_x = (f64::from(bx) - f64::from(draw.transform.tx)) * step;
                let css_y = (f64::from(by) - f64::from(draw.transform.ty)) * step;
                let uniform =
                    border.uniform(css_x, css_y, f64::from(w) * step, f64::from(h) * step);
                for py in by..by + h {
                    if let Some(color) = uniform {
                        let start = ((py - y) * TILE_EDGE + bx - x) as usize * 4;
                        for pixel in pixels.data_mut()[start..start + w as usize * 4]
                            .as_chunks_mut::<4>()
                            .0
                        {
                            pixel.copy_from_slice(&color);
                        }
                        continue;
                    }
                    for px in bx..bx + w {
                        let source = py as usize * self.width as usize + px as usize;
                        if self
                            .coverage
                            .as_ref()
                            .expect("border coverage exists")
                            .data()[source]
                            == 0
                        {
                            continue;
                        }
                        let color = border.pixel(
                            (f64::from(px) - f64::from(draw.transform.tx)) * step,
                            (f64::from(py) - f64::from(draw.transform.ty)) * step,
                            step,
                        );
                        let target = ((py - y) * TILE_EDGE + px - x) as usize * 4;
                        pixels.data_mut()[target..target + 4].copy_from_slice(&color);
                    }
                }
            }
        }
        self.slice_mask(draw, x, y, true);
        image.draw_pixmap(
            0,
            0,
            self.border_pixels
                .as_ref()
                .expect("border tile exists")
                .as_ref(),
            &PixmapPaint {
                blend_mode: blend,
                ..PixmapPaint::default()
            },
            Transform::identity(),
            Some(&self.slice),
        );
    }
    pub fn composite(
        &mut self,
        destination: &mut Pixmap,
        source: &Pixmap,
        group: &Draw,
        paint: &PixmapPaint,
        x: u32,
        y: u32,
    ) {
        let mask = self.slice_mask(group, x, y, false);
        destination.draw_pixmap(0, 0, source.as_ref(), paint, Transform::identity(), mask);
    }
}
