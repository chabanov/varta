//! Backend-independent drawing commands lowered to Vello's 2D scene.
use varta_view::{BoxRect, DisplayList, DrawPrimitive, PathSegment};
use vello_gpu::kurbo::{Affine, BezPath, Rect};
use vello_gpu::peniko::{Color, Fill};
fn rect(r: BoxRect) -> Rect {
    Rect::new(
        r.x as f64,
        r.y as f64,
        (r.x + r.width) as f64,
        (r.y + r.height) as f64,
    )
}
pub(crate) fn populate(scene: &mut vello_gpu::Scene, list: &DisplayList, display_p3: bool) {
    for draw in list.commands() {
        scene.reset_transform();
        // Physical clip paths use the CPU mask winding rule, independently of
        // the preceding primitive's even-odd border fill.
        scene.set_fill_rule(Fill::NonZero);
        let mut clips = Vec::new();
        let mut current = draw.clips.as_deref();
        while let Some(clip) = current {
            clips.push(clip);
            current = clip.parent();
        }
        for clip in clips.iter().rev() {
            scene.push_clip_path(&path(clip.segments()));
        }
        if let Some(clip) = draw.clip {
            scene.push_clip_rect(&rect(clip));
        }
        scene.set_transform(Affine::new(draw.transform.map(f64::from)));
        scene.set_paint(paint(draw.color, display_p3));
        scene.set_fill_rule(Fill::NonZero);
        match &draw.primitive {
            DrawPrimitive::Rect(r) => scene.fill_rect(&rect(*r)),
            DrawPrimitive::Path { segments, even_odd } => {
                scene.set_fill_rule(if *even_odd {
                    Fill::EvenOdd
                } else {
                    Fill::NonZero
                });
                scene.fill_path(&path(segments));
            }
            DrawPrimitive::ColorField { contour, cells } => {
                // The contour's coverage applies once, as a clip. Disjoint
                // whole-pixel cells then take full coverage inside it, so an
                // internal color seam is never composited twice. No layer or
                // intermediate texture is needed.
                scene.set_fill_rule(Fill::EvenOdd);
                scene.push_clip_path(&path(contour));
                scene.reset_transform();
                scene.set_fill_rule(Fill::NonZero);
                for cell in cells.iter() {
                    scene.set_paint(paint(cell.color, display_p3));
                    scene.fill_rect(&rect(cell.rect));
                }
                scene.pop_clip();
            }
        }
        if draw.clip.is_some() {
            scene.pop_clip();
        }
        for _ in &clips {
            scene.pop_clip();
        }
    }
}

fn paint(color: [f32; 4], display_p3: bool) -> Color {
    // Vello emits the paint's encoded components. Tag the window explicitly
    // and convert primaries before emission, rather than inherit a display
    // profile from an untagged CAMetalLayer on wide-gamut macOS displays.
    let source = Color::new(color);
    if display_p3 {
        Color::new(source.convert::<vello_gpu::color::DisplayP3>().components)
    } else {
        source
    }
}

fn path(segments: &[PathSegment]) -> BezPath {
    let mut path = BezPath::new();
    let point = |p: [f32; 2]| (f64::from(p[0]), f64::from(p[1]));
    for segment in segments {
        match *segment {
            PathSegment::Move(p) => path.move_to(point(p)),
            PathSegment::Line(p) => path.line_to(point(p)),
            PathSegment::Quad(a, b) => path.quad_to(point(a), point(b)),
            PathSegment::Cubic(a, b, c) => path.curve_to(point(a), point(b), point(c)),
            PathSegment::Close => path.close_path(),
        }
    }
    path
}
