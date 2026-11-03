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
        if let Some(clip) = draw.clip {
            scene.push_clip_rect(&rect(clip));
        }
        scene.set_transform(Affine::new(draw.transform.map(f64::from)));
        let source = Color::new(draw.color);
        // Vello emits the paint's encoded components. Tag the window explicitly
        // and convert primaries before emission, rather than inherit a display
        // profile from an untagged CAMetalLayer on wide-gamut macOS displays.
        let components = if display_p3 {
            source.convert::<vello_gpu::color::DisplayP3>().components
        } else {
            draw.color
        };
        scene.set_paint(Color::new(components));
        scene.set_fill_rule(Fill::NonZero);
        match &draw.primitive {
            DrawPrimitive::Rect(r) => scene.fill_rect(&rect(*r)),
            DrawPrimitive::Path { segments, even_odd } => {
                scene.set_fill_rule(if *even_odd {
                    Fill::EvenOdd
                } else {
                    Fill::NonZero
                });
                let mut path = BezPath::new();
                let point = |p: [f32; 2]| (p[0] as f64, p[1] as f64);
                for s in segments.iter() {
                    match *s {
                        PathSegment::Move(p) => path.move_to(point(p)),
                        PathSegment::Line(p) => path.line_to(point(p)),
                        PathSegment::Quad(a, b) => path.quad_to(point(a), point(b)),
                        PathSegment::Cubic(a, b, c) => path.curve_to(point(a), point(b), point(c)),
                        PathSegment::Close => path.close_path(),
                    }
                }
                scene.fill_path(&path);
            }
        }
        if draw.clip.is_some() {
            scene.pop_clip();
        }
    }
}
