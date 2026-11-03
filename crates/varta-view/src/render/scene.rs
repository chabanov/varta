//! Bounded display-list retention and damage replay, in physical pixels.
use crate::PaintError;
use std::sync::Arc;
use tiny_skia::{Color, FillRule, Mask, Paint, Path, PathBuilder, Pixmap, Rect, Transform};

pub(crate) fn fill_rect(
    image: &mut Pixmap,
    rect: Rect,
    paint: &Paint<'_>,
    transform: Transform,
    mask: Option<&Mask>,
) {
    // tiny-skia's identity rectangle scan converter asserts on a fractional
    // one-pixel caret spanning two columns with no full interior column.
    // Its path converter handles this valid geometry in both build profiles.
    if transform.is_identity() && rect.width() < 2.0 {
        image.fill_path(
            &PathBuilder::from_rect(rect),
            paint,
            FillRule::Winding,
            transform,
            mask,
        );
    } else {
        image.fill_rect(rect, paint, transform, mask);
    }
}

pub(crate) const MAX_SCENE_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_SCENE_COMMANDS: usize = 65_536;

#[derive(Clone, PartialEq)]
pub(crate) enum Primitive {
    Rect(Rect),
    Path(Arc<Path>, FillRule),
}
#[derive(Clone, PartialEq)]
pub(crate) struct Draw {
    pub primitive: Primitive,
    pub color: Color,
    pub transform: Transform,
    pub clip: Option<Rect>,
    pub bounds: Rect,
}
impl Draw {
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match &self.primitive {
                Primitive::Rect(_) => 0,
                Primitive::Path(path, _) => {
                    std::mem::size_of_val(path.points()) + path.verbs().len()
                }
            }
    }
}
#[derive(Default)]
pub(crate) struct Recording {
    pub commands: Vec<Draw>,
    pub bytes: usize,
}

#[derive(Default)]
pub(crate) struct Scene {
    pub record_only: bool,
    image: Option<Pixmap>,
    scratch: Option<Pixmap>,
    pub commands: Vec<Draw>,
    pub dirty_pixels: usize,
    pub damage: Option<Rect>,
}
fn union(a: Option<Rect>, b: Rect) -> Option<Rect> {
    Some(match a {
        None => b,
        Some(a) => Rect::from_ltrb(
            a.left().min(b.left()),
            a.top().min(b.top()),
            a.right().max(b.right()),
            a.bottom().max(b.bottom()),
        )?,
    })
}
fn dirty(commands: &[Draw], previous: &[Draw], viewport: Rect) -> Option<Rect> {
    let prefix = commands
        .iter()
        .zip(previous)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = commands[prefix..]
        .iter()
        .rev()
        .zip(previous[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut rect = None;
    for draw in commands[prefix..commands.len() - suffix]
        .iter()
        .chain(&previous[prefix..previous.len() - suffix])
    {
        rect = union(rect, draw.bounds);
    }
    let rect = rect?.outset(2.0, 2.0)?.intersect(&viewport)?.round_out()?;
    Rect::from_xywh(
        rect.x() as f32,
        rect.y() as f32,
        rect.width() as f32,
        rect.height() as f32,
    )
}
fn key(rect: Rect) -> [u32; 4] {
    [
        rect.left().to_bits(),
        rect.top().to_bits(),
        rect.right().to_bits(),
        rect.bottom().to_bits(),
    ]
}
struct Masks {
    dirty: Rect,
    mask: Option<Mask>,
    last: Option<[u32; 4]>,
}
impl Masks {
    fn clip(&self, draw: &Draw) -> Option<Rect> {
        draw.clip
            .filter(|clip| clip.intersect(&self.dirty).is_some())
    }
    fn new(commands: &[Draw], dirty: Rect, width: u32, height: u32) -> Result<Self, PaintError> {
        let mut masks = Self {
            dirty,
            mask: None,
            last: None,
        };
        let needed = commands
            .iter()
            .any(|draw| draw.bounds.intersect(&dirty).is_some() && masks.clip(draw).is_some());
        if needed {
            masks.mask = Some(Mask::new(width, height).ok_or(PaintError::Dimensions)?);
        }
        Ok(masks)
    }
    fn draw(&mut self, image: &mut Pixmap, draw: &Draw) {
        if draw.bounds.intersect(&self.dirty).is_none() {
            return;
        }
        if draw
            .clip
            .is_some_and(|clip| clip.intersect(&self.dirty).is_none())
        {
            return;
        }
        let mask = if let Some(clip) = draw.clip {
            // new() reserves one scratch mask for every nontrivial clip.
            let mask = self.mask.as_mut().expect("clip scratch was prepared");
            if self.last != Some(key(clip)) {
                mask.clear();
                mask.fill_path(
                    &PathBuilder::from_rect(clip),
                    FillRule::Winding,
                    true,
                    Transform::identity(),
                );
                self.last = Some(key(clip));
            }
            Some(&*mask)
        } else {
            None
        };
        let mut paint = Paint::default();
        paint.set_color(draw.color);
        match &draw.primitive {
            Primitive::Rect(rect) => {
                // Intersect before rasterization: a dirty mask alone would still
                // scan a viewport-sized background in an unoptimized build.
                if let Some(rect) = rect
                    .transform(draw.transform)
                    .and_then(|r| r.intersect(&self.dirty))
                {
                    let opaque_integer_rect = draw.color.alpha() == 1.0
                        && [rect.left(), rect.top(), rect.right(), rect.bottom()]
                            .iter()
                            .all(|v| v.fract() == 0.0);
                    if draw.transform.is_identity() || opaque_integer_rect {
                        fill_rect(image, rect, &paint, Transform::identity(), mask);
                    } else {
                        // tiny-skia routes transformed rectangles through its
                        // path rasterizer, whose AA rounding is distinct.
                        image.fill_path(
                            &PathBuilder::from_rect(rect),
                            &paint,
                            FillRule::Winding,
                            Transform::identity(),
                            mask,
                        );
                    }
                }
            }
            Primitive::Path(path, rule) => {
                image.fill_path(path, &paint, *rule, draw.transform, mask)
            }
        }
    }
}
pub(crate) fn replay(image: &mut Pixmap, commands: &[Draw]) -> Result<(), PaintError> {
    let viewport = Rect::from_xywh(0.0, 0.0, image.width() as f32, image.height() as f32)
        .ok_or(PaintError::Dimensions)?;
    let mut masks = Masks::new(commands, viewport, image.width(), image.height())?;
    for draw in commands {
        masks.draw(image, draw);
    }
    Ok(())
}
impl Scene {
    pub fn record(&mut self, commands: Vec<Draw>) {
        // Software pixels belong to the previous scene. They cannot survive a
        // vector-only frame and then be reused after switching back to CPU.
        self.image = None;
        self.scratch = None;
        self.damage = None;
        self.dirty_pixels = 0;
        self.commands = commands;
    }
    pub fn reset(&mut self, width: u32, height: u32) {
        *self = Self {
            damage: Rect::from_xywh(0.0, 0.0, width as f32, height as f32),
            dirty_pixels: width as usize * height as usize,
            ..Self::default()
        };
    }
    pub fn update(
        &mut self,
        commands: Vec<Draw>,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, PaintError> {
        let viewport =
            Rect::from_xywh(0.0, 0.0, width as f32, height as f32).ok_or(PaintError::Dimensions)?;
        let reusable = self
            .image
            .as_ref()
            .is_some_and(|image| image.width() == width && image.height() == height);
        let dirty = if reusable {
            dirty(&commands, &self.commands, viewport)
        } else {
            Some(viewport)
        };
        // Finish every fallible preparation before changing the retained image.
        let masks = dirty
            .map(|d| Masks::new(&commands, d, width, height))
            .transpose()?;
        self.dirty_pixels = 0;
        if let Some(mut masks) = masks {
            let mut image = if self
                .scratch
                .as_ref()
                .is_some_and(|s| s.width() == width && s.height() == height)
            {
                self.scratch.take().ok_or(PaintError::Dimensions)?
            } else {
                Pixmap::new(width, height).ok_or(PaintError::Dimensions)?
            };
            let d = masks.dirty;
            let x = d.left() as usize;
            let y = d.top() as usize;
            let w = d.width() as usize;
            let h = d.height() as usize;
            self.dirty_pixels = w * h;
            for row in y..y + h {
                image.data_mut()
                    [(row * width as usize + x) * 4..(row * width as usize + x + w) * 4]
                    .fill(0);
            }
            for draw in &commands {
                masks.draw(&mut image, draw);
            }
            if reusable {
                let retained = self.image.as_mut().ok_or(PaintError::Dimensions)?;
                for row in y..y + h {
                    let range = (row * width as usize + x) * 4..(row * width as usize + x + w) * 4;
                    retained.data_mut()[range.clone()].copy_from_slice(&image.data()[range]);
                }
                self.scratch = Some(image);
            } else {
                self.image = Some(image);
                self.scratch = None;
            }
        }
        self.damage = dirty;
        self.commands = commands;
        Ok(self
            .image
            .as_ref()
            .ok_or(PaintError::Dimensions)?
            .data()
            .to_vec())
    }
}
