//! Bounded display-list retention and damage replay, in physical pixels.
#[path = "groups.rs"]
mod groups;
use super::clips::{Clip, MaskCache};
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
    Border(Arc<crate::paint::border::Border>),
    BeginGroup {
        opacity: f32,
        additive: bool,
        end: usize,
    },
    EndGroup,
}
#[derive(Clone, PartialEq)]
pub(crate) struct Draw {
    pub primitive: Primitive,
    pub color: Color,
    pub transform: Transform,
    pub clip: Option<Rect>,
    pub clips: Option<Arc<Clip>>,
    pub bounds: Rect,
}
impl Draw {
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.clips.as_ref().map_or(0, |clip| clip.bytes)
            + match &self.primitive {
                Primitive::Rect(_) | Primitive::BeginGroup { .. } | Primitive::EndGroup => 0,
                Primitive::Path(path, _) => {
                    std::mem::size_of_val(path.points()) + path.verbs().len()
                }
                Primitive::Border(border) => {
                    std::mem::size_of_val(border.as_ref())
                        + std::mem::size_of_val(border.path.points())
                        + border.path.verbs().len()
                }
            }
    }
}
// A nonretained isolated-frame program is bounded by the unchanged source
// preparation ceilings: glyphs/selection/preedit rectangles, inline fragments,
// fixed per-box paint operations and enter/exit markers. This is not a larger
// display-list cache; overflow programs are rasterized once and then dropped.
const TEXT_DRAWS_PER_WORK_UNIT: usize = 4;
const BOX_DRAWS_AND_MARKERS: usize = 16;
const FRAME_AUXILIARY_DRAWS: usize = 1;
pub(crate) const MAX_FRAME_COMMANDS: usize = TEXT_DRAWS_PER_WORK_UNIT
    * crate::paint::MAX_TEXT_PAINT_WORK
    + crate::inline::MAX_FRAGMENT_COUNT
    + BOX_DRAWS_AND_MARKERS * crate::layout::MAX_LAYOUT_NODES
    + FRAME_AUXILIARY_DRAWS;
pub(crate) struct Recording {
    pub commands: Vec<Draw>,
    pub bytes: usize,
    pub cacheable: bool,
}
impl Default for Recording {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
            bytes: 0,
            cacheable: true,
        }
    }
}
impl Recording {
    pub fn fits(&self, draw: &Draw) -> bool {
        self.cacheable
            && self.commands.len() < MAX_SCENE_COMMANDS
            && self.bytes.saturating_add(draw.bytes()) <= MAX_SCENE_BYTES
    }
    pub fn push(&mut self, draw: Draw) -> Result<(), PaintError> {
        if self.commands.len() >= MAX_FRAME_COMMANDS {
            return Err(PaintError::Unsupported(varta_common::view::ROOT_NODE));
        }
        if !self.fits(&draw) {
            self.cacheable = false;
        }
        if self.commands.len() == self.commands.capacity() {
            if self.cacheable {
                self.commands
                    .try_reserve(1)
                    .map_err(|_| PaintError::Dimensions)?;
            } else {
                let extra = MAX_SCENE_COMMANDS.min(MAX_FRAME_COMMANDS - self.commands.len());
                self.commands
                    .try_reserve_exact(extra)
                    .map_err(|_| PaintError::Dimensions)?;
            }
        }
        self.bytes = self.bytes.saturating_add(draw.bytes());
        self.commands.push(draw);
        Ok(())
    }
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
fn needs_complex(draw: &Draw, dirty: Rect) -> bool {
    draw.clips.as_ref().is_some_and(|clip| {
        clip.bounds.is_some()
            && !draw
                .bounds
                .intersect(&dirty)
                .is_some_and(|bounds| clip.contains_rect(bounds))
    })
}
struct Masks {
    dirty: Rect,
    mask: Option<Mask>,
    last: Option<[u32; 4]>,
    complex: Option<MaskCache>,
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
            complex: None,
        };
        let needed = commands.iter().any(|draw| {
            !needs_complex(draw, dirty)
                && draw.clips.as_ref().is_none_or(|clip| clip.bounds.is_some())
                && draw.bounds.intersect(&dirty).is_some()
                && masks.clip(draw).is_some()
        });
        if needed {
            masks.mask = Some(Mask::new(width, height).ok_or(PaintError::Dimensions)?);
        }
        if commands.iter().any(|draw| {
            draw.bounds.intersect(&dirty).is_some()
                && draw
                    .clip
                    .is_none_or(|clip| clip.intersect(&dirty).is_some())
                && needs_complex(draw, dirty)
        }) {
            masks.complex = Some(MaskCache::new(width, height)?);
        }
        Ok(masks)
    }
    fn visible(&self, draw: &Draw) -> bool {
        if draw.bounds.intersect(&self.dirty).is_none() {
            return false;
        }
        if draw
            .clip
            .is_some_and(|clip| clip.intersect(&self.dirty).is_none())
        {
            return false;
        }
        if draw
            .clips
            .as_ref()
            .is_some_and(|clip| clip.bounds.is_none())
        {
            return false;
        }
        true
    }
    fn mask(&mut self, draw: &Draw) -> Option<&Mask> {
        if needs_complex(draw, self.dirty) {
            let clips = draw.clips.as_ref().expect("complex clip was checked");
            Some(
                self.complex
                    .as_mut()
                    .expect("complex clip scratch was prepared")
                    .mask(clips, draw.clip),
            )
        } else if let Some(clip) = draw.clip {
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
        }
    }
    fn draw(&mut self, image: &mut Pixmap, draw: &Draw) {
        self.draw_blend(image, draw, tiny_skia::BlendMode::SourceOver);
    }
    fn draw_blend(&mut self, image: &mut Pixmap, draw: &Draw, blend: tiny_skia::BlendMode) {
        if !self.visible(draw) {
            return;
        }
        let dirty = self.dirty;
        let mask = self.mask(draw);
        let mut paint = Paint::default();
        paint.set_color(draw.color);
        paint.blend_mode = blend;
        match &draw.primitive {
            Primitive::Rect(rect) => {
                // Intersect before rasterization: a dirty mask alone would still
                // scan a viewport-sized background in an unoptimized build.
                if let Some(rect) = rect
                    .transform(draw.transform)
                    .and_then(|r| r.intersect(&dirty))
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
            Primitive::BeginGroup { .. } | Primitive::EndGroup | Primitive::Border(_) => {
                unreachable!("groups use the tiled compositor")
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
    if groups::present(commands) {
        return groups::replay(image, commands, viewport);
    }
    let mut masks = Masks::new(commands, viewport, image.width(), image.height())?;
    for draw in commands {
        masks.draw(image, draw);
    }
    Ok(())
}
impl Scene {
    pub fn prepare(
        &mut self,
        commands: Vec<Draw>,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, PaintError> {
        // Group/color-field commands cannot enter the current vector contract. Preserve
        // their software scene so repeated prepare() calls still reuse pixels
        // and local changes replay only their damage, like ordinary CPU frames.
        if self.record_only && !groups::present(&commands) {
            self.record(commands);
            Ok(Vec::new())
        } else {
            self.update(commands, width, height)
        }
    }
    fn record(&mut self, commands: Vec<Draw>) {
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
        let grouped = groups::present(&commands);
        let masks = dirty
            .filter(|_| !grouped)
            .map(|d| Masks::new(&commands, d, width, height))
            .transpose()?;
        self.dirty_pixels = 0;
        if let Some(d) = dirty {
            let mut masks = masks;
            let mut image = if self
                .scratch
                .as_ref()
                .is_some_and(|s| s.width() == width && s.height() == height)
            {
                self.scratch.take().ok_or(PaintError::Dimensions)?
            } else {
                Pixmap::new(width, height).ok_or(PaintError::Dimensions)?
            };
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
            if grouped {
                groups::replay(&mut image, &commands, d)?;
            } else if let Some(masks) = masks.as_mut() {
                for draw in &commands {
                    masks.draw(&mut image, draw);
                }
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

#[cfg(test)]
mod clip_replay_tests {
    use super::*;
    use crate::{BoxRect, paint::corners::Corners};
    #[test]
    fn interior_damage_skips_complex_masks_but_corner_damage_keeps_them() {
        let rect = Rect::from_xywh(0.0, 0.0, 40.0, 40.0).unwrap();
        let bounds = BoxRect {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 40.0,
        };
        let corners = Corners([[20.0; 2]; 4]);
        let clip = Clip::new(None, corners.path(bounds), bounds, corners);
        let draw = Draw {
            primitive: Primitive::Rect(rect),
            color: Color::from_rgba8(255, 0, 0, 255),
            transform: Transform::identity(),
            clip: None,
            clips: Some(clip),
            bounds: rect,
        };
        let dirty = Rect::from_xywh(18.0, 18.0, 4.0, 4.0).unwrap();
        let mut masks = Masks::new(std::slice::from_ref(&draw), dirty, 40, 40).unwrap();
        assert!(masks.complex.is_none());
        assert!(masks.mask.is_none());
        let mut image = Pixmap::new(40, 40).unwrap();
        masks.draw(&mut image, &draw);
        assert_eq!(image.pixel(20, 20).unwrap().alpha(), 255);
        assert_eq!(image.pixel(0, 0).unwrap().alpha(), 0);
        let dirty = Rect::from_xywh(0.0, 0.0, 8.0, 8.0).unwrap();
        let mut masks = Masks::new(std::slice::from_ref(&draw), dirty, 40, 40).unwrap();
        assert!(masks.complex.is_some());
        let mut image = Pixmap::new(40, 40).unwrap();
        masks.draw(&mut image, &draw);
        assert_eq!(image.pixel(1, 1).unwrap().alpha(), 0);
        assert_eq!(image.pixel(7, 7).unwrap().alpha(), 255);
    }
}
