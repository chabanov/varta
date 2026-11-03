//! Host-owned, bounded drawing data shared by software and GPU consumers.
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
    /// Recording overflow preserves the original software admission behavior.
    Raster(RasterImage),
}
impl SegmentCache {
    pub fn clear(&mut self) {
        self.entries.clear();
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
#[derive(Default)]
pub(crate) struct SegmentCache {
    entries: BTreeMap<usize, CachedPath>,
    pub hits: usize,
}
impl DisplayList {
    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }
    pub(crate) fn from_commands(draws: &[Draw], cache: &mut SegmentCache) -> Option<Self> {
        let mut commands = Vec::new();
        let mut bytes = 0usize;
        let mut next = BTreeMap::new();
        let mut cache_bytes = 0usize;
        for (index, draw) in draws.iter().enumerate() {
            bytes = bytes.checked_add(std::mem::size_of::<DrawCommand>())?;
            let primitive = match &draw.primitive {
                Primitive::Rect(r) => DrawPrimitive::Rect(rect(*r)),
                Primitive::Path(path, rule) => {
                    // Charge the full output before allocating its segment array.
                    bytes = bytes.checked_add(
                        path.verbs()
                            .len()
                            .checked_mul(std::mem::size_of::<PathSegment>())?,
                    )?;
                    if bytes > scene::MAX_SCENE_BYTES {
                        return None;
                    }
                    let point = |p: tiny_skia::Point| [p.x, p.y];
                    let segments: Arc<[PathSegment]> = if let Some(old) = cache
                        .entries
                        .get(&index)
                        .filter(|old| old.path.as_ref() == path.as_ref())
                    {
                        cache.hits = cache.hits.saturating_add(1);
                        old.segments.clone()
                    } else {
                        path.segments()
                            .map(|s| match s {
                                tiny_skia::PathSegment::MoveTo(p) => PathSegment::Move(point(p)),
                                tiny_skia::PathSegment::LineTo(p) => PathSegment::Line(point(p)),
                                tiny_skia::PathSegment::QuadTo(a, b) => {
                                    PathSegment::Quad(point(a), point(b))
                                }
                                tiny_skia::PathSegment::CubicTo(a, b, c) => {
                                    PathSegment::Cubic(point(a), point(b), point(c))
                                }
                                tiny_skia::PathSegment::Close => PathSegment::Close,
                            })
                            .collect::<Vec<_>>()
                            .into()
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
            });
        }
        cache.entries = next;
        Some(Self { commands })
    }
    /// Software consumer for conformance and fallback, with the same clipping.
    pub fn rasterize(&self, metadata: FrameMetadata) -> Result<RasterImage, PaintError> {
        crate::paint::validate_dimensions(metadata.width, metadata.height, metadata.scale())?;
        let mut draws = Vec::with_capacity(self.commands.len());
        for draw in &self.commands {
            let primitive = match &draw.primitive {
                DrawPrimitive::Rect(r) => Primitive::Rect(tiny_rect(*r)?),
                DrawPrimitive::Path { segments, even_odd } => {
                    let mut path = tiny_skia::PathBuilder::new();
                    for s in segments.iter() {
                        match *s {
                            PathSegment::Move(p) => path.move_to(p[0], p[1]),
                            PathSegment::Line(p) => path.line_to(p[0], p[1]),
                            PathSegment::Quad(a, b) => path.quad_to(a[0], a[1], b[0], b[1]),
                            PathSegment::Cubic(a, b, c) => {
                                path.cubic_to(a[0], a[1], b[0], b[1], c[0], c[1])
                            }
                            PathSegment::Close => path.close(),
                        }
                    }
                    Primitive::Path(
                        Arc::new(path.finish().ok_or(PaintError::Dimensions)?),
                        if *even_odd {
                            tiny_skia::FillRule::EvenOdd
                        } else {
                            tiny_skia::FillRule::Winding
                        },
                    )
                }
            };
            let [xx, yx, xy, yy, dx, dy] = draw.transform;
            let bounds = match &primitive {
                Primitive::Rect(r) => *r,
                Primitive::Path(p, _) => p.bounds(),
            };
            let transform = tiny_skia::Transform::from_row(xx, yx, xy, yy, dx, dy);
            draws.push(Draw {
                primitive,
                color: tiny_skia::Color::from_rgba(
                    draw.color[0],
                    draw.color[1],
                    draw.color[2],
                    draw.color[3],
                )
                .ok_or(PaintError::Dimensions)?,
                transform,
                clip: draw.clip.map(tiny_rect).transpose()?,
                bounds: bounds.transform(transform).ok_or(PaintError::Dimensions)?,
            });
        }
        let pixels = software_pixels(&draws, metadata.width, metadata.height)?;
        Ok(RasterImage::from_parts(metadata, pixels))
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
