//! Host-owned, bounded drawing data shared by software and GPU consumers.
use crate::paint::border::{Border, FieldCell};
use crate::render::clips::Clip;
use crate::render::scene::{self, Draw, Primitive};
use crate::{BoxRect, FrameMetadata, PaintError, RasterImage};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSegment {
    Move([f32; 2]),
    Line([f32; 2]),
    Quad([f32; 2], [f32; 2]),
    Cubic([f32; 2], [f32; 2], [f32; 2]),
    Close,
}
#[derive(Debug, Clone, PartialEq)]
pub enum DrawPrimitive {
    Rect(BoxRect),
    Path {
        segments: Arc<[PathSegment]>,
        even_odd: bool,
    },
    /// A border with distinct side colors: one closed even-odd `contour`,
    /// transformed like a path, whose coverage applies once to a piecewise
    /// uniform color field. A consumer clips to the contour and fills each
    /// cell; cells never overlap, so no internal color seam is composited
    /// twice. The command color is unused.
    ColorField {
        contour: Arc<[PathSegment]>,
        cells: Arc<[ColorCell]>,
    },
}
/// One rectangle of a color field with integral physical-pixel edges. The
/// command transform does not apply to it; the command's clips do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorCell {
    pub rect: BoxRect,
    /// Unpremultiplied sRGB components, including alpha.
    pub color: [f32; 4],
}
/// One closed, host-validated physical clipping path. Parent paths intersect
/// in tree order; the primitive transform does not transform these paths.
#[derive(Debug, Clone)]
pub struct DrawClip {
    segments: Arc<[PathSegment]>,
    parent: Option<Arc<Self>>,
    source: Arc<Clip>,
}
impl PartialEq for DrawClip {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.source, &other.source) || self.source == other.source)
            && (Arc::ptr_eq(&self.segments, &other.segments) || self.segments == other.segments)
    }
}
impl DrawClip {
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }
    pub fn parent(&self) -> Option<&Self> {
        self.parent.as_deref()
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct DrawCommand {
    pub primitive: DrawPrimitive,
    /// Unpremultiplied sRGB components, including alpha.
    pub color: [f32; 4],
    /// Affine coefficients [xx, yx, xy, yy, dx, dy].
    pub transform: [f32; 6],
    /// Physical-pixel clip, applied after the primitive transform.
    pub clip: Option<BoxRect>,
    pub clips: Option<Arc<DrawClip>>,
}
#[derive(Debug, Clone)]
pub struct DisplayList {
    commands: Vec<DrawCommand>,
}
pub enum PreparedFrame {
    Vector {
        metadata: FrameMetadata,
        scene: DisplayList,
    },
    /// Software-only operations and recording overflow preserve pixel/metadata behavior.
    Raster(RasterImage),
}
impl SegmentCache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.fields.clear();
    }
}
fn rect(r: tiny_skia::Rect) -> BoxRect {
    BoxRect {
        x: r.x(),
        y: r.y(),
        width: r.width(),
        height: r.height(),
    }
}
fn tiny_rect(r: BoxRect) -> Result<tiny_skia::Rect, PaintError> {
    tiny_skia::Rect::from_xywh(r.x, r.y, r.width, r.height).ok_or(PaintError::Dimensions)
}
const MAX_CACHE_BYTES: usize = 8 * 1024 * 1024;
struct CachedPath {
    path: Arc<tiny_skia::Path>,
    segments: Arc<[PathSegment]>,
}
/// A lowered color field, reusable for an equal border at the same placement.
struct CachedField {
    border: Arc<Border>,
    transform: tiny_skia::Transform,
    bounds: tiny_skia::Rect,
    contour: Arc<[PathSegment]>,
    cells: Arc<[ColorCell]>,
}
#[derive(Default)]
pub(crate) struct SegmentCache {
    entries: BTreeMap<usize, CachedPath>,
    fields: BTreeMap<usize, CachedField>,
    pub hits: usize,
}
fn color_cell(cell: &FieldCell) -> ColorCell {
    let [red, green, blue, alpha] = cell.color;
    // `Border::cells` emits no transparent cell: those pixels paint nothing.
    debug_assert_ne!(alpha, 0, "lowered cells have nonzero alpha");
    // Premultiplied channels never exceed alpha, so these stay within [0, 1].
    let alpha = f32::from(alpha);
    let unpremultiply = |channel: u8| f32::from(channel) / alpha;
    ColorCell {
        rect: BoxRect {
            x: cell.x as f32,
            y: cell.y as f32,
            width: cell.width as f32,
            height: cell.height as f32,
        },
        color: [
            unpremultiply(red),
            unpremultiply(green),
            unpremultiply(blue),
            alpha / f32::from(u8::MAX),
        ],
    }
}
fn path_bytes(path: &tiny_skia::Path) -> Option<usize> {
    path.verbs()
        .len()
        .checked_mul(std::mem::size_of::<PathSegment>())
}
fn tiny_path(segments: &[PathSegment]) -> Result<tiny_skia::Path, PaintError> {
    let mut path = tiny_skia::PathBuilder::new();
    for s in segments {
        match *s {
            PathSegment::Move(p) => path.move_to(p[0], p[1]),
            PathSegment::Line(p) => path.line_to(p[0], p[1]),
            PathSegment::Quad(a, b) => path.quad_to(a[0], a[1], b[0], b[1]),
            PathSegment::Cubic(a, b, c) => path.cubic_to(a[0], a[1], b[0], b[1], c[0], c[1]),
            PathSegment::Close => path.close(),
        }
    }
    path.finish().ok_or(PaintError::Dimensions)
}
/// Lower one border to its contour and cells. The contour segments are charged
/// before allocation, like every path. The lowering's whole transient peak
/// (scratch, cell lists by capacity and the converted cells) must fit the
/// remaining scene budget; afterwards only the retained cells stay charged.
/// `None` sends the whole frame to the explicit software fallback. An
/// unchanged border reuses the previous lowering.
fn color_field(
    index: usize,
    draw: &Draw,
    border: &Arc<Border>,
    cache: &mut SegmentCache,
    bytes: &mut usize,
) -> Option<CachedField> {
    *bytes = bytes.checked_add(path_bytes(&border.path)?)?;
    if *bytes > scene::MAX_SCENE_BYTES {
        return None;
    }
    let cell_bytes = std::mem::size_of::<ColorCell>();
    let reused = cache.fields.get(&index).filter(|old| {
        (Arc::ptr_eq(&old.border, border) || old.border == *border)
            && old.transform == draw.transform
            && old.bounds == draw.bounds
    });
    let (contour, cells) = if let Some(old) = reused {
        cache.hits = cache.hits.saturating_add(1);
        (old.contour.clone(), old.cells.clone())
    } else {
        let remaining = scene::MAX_SCENE_BYTES - *bytes;
        let lowered = border.cells(draw.transform, draw.bounds, remaining, cell_bytes)?;
        // An exact-length iterator allocates the shared slice once, with no
        // intermediate vector, while `lowered` is still alive (charged above).
        let cells: Arc<[ColorCell]> = lowered.iter().map(color_cell).collect();
        drop(lowered);
        (segments(&border.path), cells)
    };
    *bytes = bytes.checked_add(cells.len().checked_mul(cell_bytes)?)?;
    Some(CachedField {
        border: border.clone(),
        transform: draw.transform,
        bounds: draw.bounds,
        contour,
        cells,
    })
}
fn segments(path: &tiny_skia::Path) -> Arc<[PathSegment]> {
    let point = |p: tiny_skia::Point| [p.x, p.y];
    path.segments()
        .map(|segment| match segment {
            tiny_skia::PathSegment::MoveTo(p) => PathSegment::Move(point(p)),
            tiny_skia::PathSegment::LineTo(p) => PathSegment::Line(point(p)),
            tiny_skia::PathSegment::QuadTo(a, b) => PathSegment::Quad(point(a), point(b)),
            tiny_skia::PathSegment::CubicTo(a, b, c) => {
                PathSegment::Cubic(point(a), point(b), point(c))
            }
            tiny_skia::PathSegment::Close => PathSegment::Close,
        })
        .collect::<Vec<_>>()
        .into()
}
fn convert_clip(
    source: &Arc<Clip>,
    cache: &mut BTreeMap<usize, Arc<DrawClip>>,
    bytes: &mut usize,
) -> Option<Arc<DrawClip>> {
    let key = |clip: &Arc<Clip>| Arc::as_ptr(clip) as usize;
    let mut missing = Vec::new();
    let mut current = Some(source);
    while let Some(clip) = current {
        if cache.contains_key(&key(clip)) {
            break;
        }
        missing.push(clip.clone());
        current = clip.parent.as_ref();
    }
    for clip in missing.into_iter().rev() {
        let path = clip.path.as_ref()?;
        *bytes = bytes
            .checked_add(std::mem::size_of::<DrawClip>())?
            .checked_add(
                path.verbs()
                    .len()
                    .checked_mul(std::mem::size_of::<PathSegment>())?,
            )?;
        if *bytes > scene::MAX_SCENE_BYTES {
            return None;
        }
        let parent = clip
            .parent
            .as_ref()
            .and_then(|parent| cache.get(&key(parent)).cloned());
        let neutral = Arc::new(DrawClip {
            segments: segments(path),
            parent,
            source: clip.clone(),
        });
        // Holding source Arcs prevents address reuse for this conversion cache.
        cache.insert(key(&clip), neutral);
    }
    cache.get(&key(source)).cloned()
}
impl DisplayList {
    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }
    pub(crate) fn from_commands(draws: &[Draw], cache: &mut SegmentCache) -> Option<Self> {
        let mut commands = Vec::new();
        let mut bytes = 0usize;
        let mut next = BTreeMap::new();
        let mut fields = BTreeMap::new();
        let mut cache_bytes = 0usize;
        let mut clips = BTreeMap::new();
        for (index, draw) in draws.iter().enumerate() {
            bytes = bytes.checked_add(std::mem::size_of::<DrawCommand>())?;
            if draw
                .clips
                .as_ref()
                .is_some_and(|clip| clip.bounds.is_none())
            {
                continue;
            }
            let clip = match &draw.clips {
                Some(clip) => Some(convert_clip(clip, &mut clips, &mut bytes)?),
                None => None,
            };
            let primitive = match &draw.primitive {
                // Groups and images have no vector form: the frame takes
                // the explicit software fallback (`PreparedFrame::Raster`).
                Primitive::BeginGroup { .. } | Primitive::EndGroup | Primitive::Image(_) => {
                    return None;
                }
                Primitive::Border(border) => {
                    let field = color_field(index, draw, border, cache, &mut bytes)?;
                    let primitive = DrawPrimitive::ColorField {
                        contour: field.contour.clone(),
                        cells: field.cells.clone(),
                    };
                    let entry_bytes = std::mem::size_of::<CachedField>()
                        + std::mem::size_of::<usize>()
                        + field.contour.len() * std::mem::size_of::<PathSegment>()
                        + field.cells.len() * std::mem::size_of::<ColorCell>();
                    if cache_bytes.saturating_add(entry_bytes) <= MAX_CACHE_BYTES {
                        cache_bytes += entry_bytes;
                        fields.insert(index, field);
                    }
                    primitive
                }
                Primitive::Rect(r) => DrawPrimitive::Rect(rect(*r)),
                Primitive::Path(path, rule) => {
                    // Charge the full output before allocating its segment array.
                    bytes = bytes.checked_add(path_bytes(path)?)?;
                    if bytes > scene::MAX_SCENE_BYTES {
                        return None;
                    }
                    let segments: Arc<[PathSegment]> = if let Some(old) = cache
                        .entries
                        .get(&index)
                        .filter(|old| old.path.as_ref() == path.as_ref())
                    {
                        cache.hits = cache.hits.saturating_add(1);
                        old.segments.clone()
                    } else {
                        segments(path)
                    };
                    let entry_bytes = std::mem::size_of::<CachedPath>()
                        + std::mem::size_of::<usize>()
                        + segments.len() * std::mem::size_of::<PathSegment>()
                        + std::mem::size_of_val(path.points())
                        + path.verbs().len();
                    if cache_bytes.saturating_add(entry_bytes) <= MAX_CACHE_BYTES {
                        cache_bytes += entry_bytes;
                        next.insert(
                            index,
                            CachedPath {
                                path: path.clone(),
                                segments: segments.clone(),
                            },
                        );
                    }
                    DrawPrimitive::Path {
                        segments,
                        even_odd: *rule == tiny_skia::FillRule::EvenOdd,
                    }
                }
            };
            if bytes > scene::MAX_SCENE_BYTES || commands.len() >= scene::MAX_SCENE_COMMANDS {
                return None;
            }
            let t = draw.transform;
            commands.push(DrawCommand {
                primitive,
                color: [
                    draw.color.red(),
                    draw.color.green(),
                    draw.color.blue(),
                    draw.color.alpha(),
                ],
                transform: [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty],
                clip: draw.clip.map(rect),
                clips: clip,
            });
        }
        cache.entries = next;
        cache.fields = fields;
        Some(Self { commands })
    }
    /// Software consumer for conformance and fallback, with the same clipping.
    /// A color field costs one viewport coverage mask plus its cells' area.
    pub fn rasterize(&self, metadata: FrameMetadata) -> Result<RasterImage, PaintError> {
        crate::paint::validate_dimensions(metadata.width, metadata.height, metadata.scale())?;
        let mut image = tiny_skia::Pixmap::new(metadata.width, metadata.height)
            .ok_or(PaintError::Dimensions)?;
        let mut draws = Vec::with_capacity(self.commands.len());
        let color = |c: [f32; 4]| {
            tiny_skia::Color::from_rgba(c[0], c[1], c[2], c[3]).ok_or(PaintError::Dimensions)
        };
        for draw in &self.commands {
            let [xx, yx, xy, yy, dx, dy] = draw.transform;
            let transform = tiny_skia::Transform::from_row(xx, yx, xy, yy, dx, dy);
            let clip = draw.clip.map(tiny_rect).transpose()?;
            let clips = draw.clips.as_ref().map(|clip| clip.source.clone());
            let primitive = match &draw.primitive {
                DrawPrimitive::Rect(r) => Primitive::Rect(tiny_rect(*r)?),
                DrawPrimitive::Path { segments, even_odd } => Primitive::Path(
                    Arc::new(tiny_path(segments)?),
                    if *even_odd {
                        tiny_skia::FillRule::EvenOdd
                    } else {
                        tiny_skia::FillRule::Winding
                    },
                ),
                DrawPrimitive::ColorField { contour, cells } => {
                    // Earlier commands paint first, preserving source order.
                    scene::replay(&mut image, &draws)?;
                    draws.clear();
                    // The contour's coverage applies once; cells never overlap.
                    let contour = Arc::new(tiny_path(contour)?);
                    let bounds = contour
                        .bounds()
                        .transform(transform)
                        .ok_or(PaintError::Dimensions)?;
                    let field = Draw {
                        primitive: Primitive::Path(contour, tiny_skia::FillRule::EvenOdd),
                        color: tiny_skia::Color::TRANSPARENT,
                        transform,
                        clip,
                        clips,
                        bounds,
                    };
                    let cells = cells
                        .iter()
                        .map(|cell| Ok((tiny_rect(cell.rect)?, color(cell.color)?)));
                    scene::replay_field(&mut image, &field, cells)?;
                    continue;
                }
            };
            let bounds = match &primitive {
                Primitive::Rect(r) => *r,
                Primitive::Path(p, _) => p.bounds(),
                Primitive::BeginGroup { .. }
                | Primitive::EndGroup
                | Primitive::Border(_)
                | Primitive::Image(_) => {
                    unreachable!("public drawing primitives exclude groups and images")
                }
            };
            draws.push(Draw {
                primitive,
                color: color(draw.color)?,
                transform,
                clip,
                clips,
                bounds: bounds.transform(transform).ok_or(PaintError::Dimensions)?,
            });
        }
        scene::replay(&mut image, &draws)?;
        Ok(RasterImage::from_parts(metadata, image.take()))
    }
}
pub(crate) fn software_pixels(
    draws: &[Draw],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, PaintError> {
    let mut image = tiny_skia::Pixmap::new(width, height).ok_or(PaintError::Dimensions)?;
    scene::replay(&mut image, draws)?;
    Ok(image.take())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::border::tests::fixtures;
    use tiny_skia::{Color, FillRule, PathBuilder, Transform};

    fn field_draw() -> Draw {
        let border = fixtures().swap_remove(1);
        let bounds = border.path.bounds();
        Draw {
            primitive: Primitive::Border(Arc::new(border)),
            color: Color::TRANSPARENT,
            transform: Transform::identity(),
            clip: None,
            clips: None,
            bounds,
        }
    }
    /// An ordinary path command that leaves between `remaining` and one
    /// segment more of the scene budget after the following color-field
    /// command and its `contour` segment bytes.
    fn filler(remaining: usize, contour: usize) -> Draw {
        const ROW: usize = 64;
        let fixed = 2 * std::mem::size_of::<DrawCommand>() + contour + remaining;
        let verbs = (scene::MAX_SCENE_BYTES - fixed) / std::mem::size_of::<PathSegment>();
        let mut path = PathBuilder::new();
        path.move_to(0.0, 0.0);
        for index in 1..verbs {
            path.line_to((index % ROW) as f32, (index / ROW) as f32);
        }
        let path = path.finish().unwrap();
        let bounds = path.bounds();
        Draw {
            primitive: Primitive::Path(Arc::new(path), FillRule::Winding),
            color: Color::BLACK,
            transform: Transform::identity(),
            clip: None,
            clips: None,
            bounds,
        }
    }

    #[test]
    fn a_color_field_over_the_remaining_scene_budget_refuses_the_vector_frame() {
        let field = field_draw();
        let Primitive::Border(border) = &field.primitive else {
            unreachable!("the fixture is a color field")
        };
        let contour = path_bytes(&border.path).unwrap();
        let alone =
            DisplayList::from_commands(std::slice::from_ref(&field), &mut Default::default())
                .expect("an unconstrained field lowers");
        let [
            DrawCommand {
                primitive: DrawPrimitive::ColorField { cells, .. },
                ..
            },
        ] = alone.commands()
        else {
            panic!("one color-field command")
        };
        let retained = cells.len() * std::mem::size_of::<ColorCell>();
        // Room for the retained cells, but not for the lowering's peak: the
        // frame takes the explicit software fallback (`Renderer::prepare`).
        let tight = [filler(retained, contour), field.clone()];
        assert!(DisplayList::from_commands(&tight, &mut Default::default()).is_none());
        const ROOM: usize = 1024 * 1024;
        let roomy = [filler(ROOM, contour), field];
        let list = DisplayList::from_commands(&roomy, &mut Default::default())
            .expect("the field fits the remaining budget");
        assert!(matches!(
            &list.commands()[1].primitive,
            DrawPrimitive::ColorField { cells: lowered, .. } if lowered == cells
        ));
    }
}
