//! One premultiplied color field over the existing even-odd border contour.
//! Outer/hole coverage is applied once, avoiding seams between colored pieces.
use super::{BoxRect, Corners, border_partition::Partition};
use tiny_skia::{Color, Path};

const COLOR_GRID: usize = 2;
// A rotated pattern, closed under quarter turns. No sample lies on the common
// 45-degree corner seam; a regular grid would bias half the diagonal samples.
const COLOR_OFFSETS: [[f64; 2]; 4] = [
    [0.375, 0.125],
    [0.875, 0.375],
    [0.125, 0.625],
    [0.625, 0.875],
];
const SAMPLE_COUNT: u32 = (COLOR_GRID * COLOR_GRID * COLOR_OFFSETS.len()) as u32;

#[derive(Clone, PartialEq)]
pub(crate) struct Border {
    pub path: Path,
    partition: Partition,
    colors: [[u8; 4]; 4],
}

impl Border {
    pub(super) fn new(
        path: Path,
        bounds: BoxRect,
        widths: [f32; 4],
        radii: Corners,
        colors: [Color; 4],
    ) -> Self {
        Self {
            path,
            partition: Partition::new(bounds, widths, radii),
            colors: colors.map(|color| {
                let c = color.to_color_u8().premultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            }),
        }
    }
    pub fn pixel(&self, x: f64, y: f64, step: f64) -> [u8; 4] {
        let center = self.colors[self.partition.owner(x + step * 0.5, y + step * 0.5)];
        let corners = [[x, y], [x + step, y], [x, y + step], [x + step, y + step]];
        if corners
            .into_iter()
            .all(|[x, y]| self.colors[self.partition.owner(x, y)] == center)
        {
            return center;
        }
        let mut sum = [0_u32; 4];
        let mut count = 0;
        for row in 0..COLOR_GRID {
            for column in 0..COLOR_GRID {
                for [ox, oy] in COLOR_OFFSETS {
                    let sx = x + step * (column as f64 + ox) / COLOR_GRID as f64;
                    let sy = y + step * (row as f64 + oy) / COLOR_GRID as f64;
                    if self.partition.in_ring(sx, sy) {
                        let color = self.colors[self.partition.owner(sx, sy)];
                        for (sum, value) in sum.iter_mut().zip(color) {
                            *sum += u32::from(value);
                        }
                        count += 1;
                    }
                }
            }
        }
        debug_assert!(count <= SAMPLE_COUNT);
        if count == 0 {
            return center;
        }
        sum.map(|value| ((value + count / 2) / count) as u8)
    }
    pub fn uniform(&self, x: f64, y: f64, w: f64, h: f64) -> Option<[u8; 4]> {
        self.partition
            .uniform(x, y, w, h)
            .map(|side| self.colors[side])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{FillRule, Mask, Paint, PathBuilder, Pixmap, Rect, Transform};
    #[test]
    fn fractional_ring_coverage_and_color_keep_the_last_partial_row() {
        let bounds = BoxRect {
            x: 10.25,
            y: 10.25,
            width: 176.0,
            height: 116.0,
        };
        let mut p = PathBuilder::new();
        p.push_rect(Rect::from_xywh(10.25, 10.25, 176.0, 116.0).unwrap());
        p.push_rect(Rect::from_xywh(18.25, 18.25, 160.0, 100.0).unwrap());
        let path = p.finish().unwrap();
        let mut mask = Mask::new(210, 150).unwrap();
        mask.fill_path(&path, FillRule::EvenOdd, true, Transform::identity());
        let at = (126 * 210 + 10) as usize;
        let border = Border::new(
            path.clone(),
            bounds,
            [8.0; 4],
            Corners::default(),
            [
                Color::from_rgba8(255, 0, 0, 128),
                Color::from_rgba8(0, 0, 255, 128),
                Color::from_rgba8(0, 255, 0, 128),
                Color::from_rgba8(255, 255, 0, 128),
            ],
        );
        assert_eq!(border.pixel(10.0, 126.0, 1.0)[3], 128);
        let mut reference = Pixmap::new(210, 150).unwrap();
        let mut paint = Paint::default();
        paint.set_color_rgba8(255, 0, 0, 128);
        reference.fill_path(
            &path,
            &paint,
            FillRule::EvenOdd,
            Transform::identity(),
            None,
        );
        assert!(
            mask.data()[at] > 0,
            "mask lost edge; painted alpha{}",
            reference.data()[at * 4 + 3]
        );
        let draw = crate::render::scene::Draw {
            primitive: crate::render::scene::Primitive::Border(std::sync::Arc::new(border)),
            color: Color::TRANSPARENT,
            transform: Transform::identity(),
            clip: None,
            clips: None,
            bounds: path.bounds(),
        };
        let mut actual = Pixmap::new(210, 150).unwrap();
        crate::render::scene::replay(&mut actual, &[draw]).unwrap();
        assert_eq!(actual.data()[at * 4 + 3], reference.data()[at * 4 + 3]);
    }
}
