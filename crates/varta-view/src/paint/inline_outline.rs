//! Outline pixels keep their original clips and join the context's outline phase.
use super::*;

pub(super) struct Painter<'a> {
    pub tree: &'a Tree,
    pub env: &'a Environment,
    pub node: u32,
    pub pieces: &'a [BoxRect],
    pub clip: BoxRect,
    pub units: Units,
}
impl Painter<'_> {
    pub fn paint(&self, surface: &mut Surface, work: &mut usize) -> Result<(), PaintError> {
        let properties = self.tree.computed_read(self.node, self.env);
        if [CssProperty::Outline, CssProperty::OutlineStyle]
            .into_iter()
            .all(|p| properties.get(&p).is_none_or(|v| v == "none"))
        {
            return Ok(());
        }
        let Some((width, offset, color)) = outline(self.tree, self.env, self.node)? else {
            return Ok(());
        };
        if color.alpha() == 0.0
            || self.pieces.is_empty()
            || surface.groups.last().is_some_and(|g| g.hidden)
            || surface
                .capture
                .as_ref()
                .is_some_and(|c| c.hidden.contains(&self.node))
        {
            return Ok(());
        }
        let radius = crate::style::border_radius(&self.tree.computed_read(self.node, self.env));
        let raw = radius.as_deref().unwrap_or("0");
        let first = self.pieces[0];
        let snapped = snap::rect(first);
        let radii = Corners::resolve_paint(raw, first, snapped, self.units)
            .ok_or(PaintError::Unsupported(self.node))?;
        if self.pieces.len() == 1 {
            *work = work.saturating_add(inline_decor::SLICE_PAINT_WORK);
            if *work > MAX_TEXT_PAINT_WORK {
                return Err(PaintError::Unsupported(self.node));
            }
            let ex = (width + offset).max(width - snapped.width * 0.5);
            let ey = (width + offset).max(width - snapped.height * 0.5);
            let outer = BoxRect {
                x: snapped.x - ex,
                y: snapped.y - ey,
                width: snapped.width + 2.0 * ex,
                height: snapped.height + 2.0 * ey,
            };
            return paint_border(
                surface,
                outer,
                self.clip,
                [width; 4],
                radii.outset(ex, ey),
                [color; 4],
            );
        }
        *work = work.saturating_add(self.pieces.len());
        if *work > MAX_TEXT_PAINT_WORK {
            return Err(PaintError::Unsupported(self.node));
        }
        let rects: Vec<_> = self
            .pieces
            .iter()
            .map(|&p| {
                let p = snap::rect(p);
                let x = f64::from(p.x);
                let y = f64::from(p.y);
                [x, y, x + f64::from(p.width), y + f64::from(p.height)]
            })
            .collect();
        let clip = surface.local_clip(self.clip);
        let path = if radii.is_square() {
            outline_union::ring(
                &rects,
                f64::from(width),
                f64::from(offset),
                clip,
                work,
                self.node,
            )?
        } else {
            outline_curve::ring(
                &rects,
                (f64::from(width), f64::from(offset), radii),
                clip,
                work,
                self.node,
            )?
        };
        let Some(path) = path else { return Ok(()) };
        let Some(rect) = Rect::from_xywh(clip.x, clip.y, clip.width, clip.height) else {
            return Ok(());
        };
        let mask = surface.mask(rect)?;
        let mut paint = Paint::default();
        paint.set_color(color);
        surface.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            Some(&mask),
        );
        Ok(())
    }
}
