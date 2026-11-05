//! One premultiplied color field over the existing even-odd border contour.
//! Outer/hole coverage is applied once, avoiding seams between colored pieces.
use super::{BoxRect, Corners, border_partition::Partition};
use tiny_skia::{Color, FillRule, Mask, Path, Rect, Stroke, Transform};

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
/// Uniform-ownership proof granularity, shared by the tiled software painter
/// and vector lowering. Proofs are interval bounds over closed rectangles, so
/// correctness does not depend on the value; one constant keeps both equal.
pub(crate) const COLOR_BLOCK: u32 = 8;
// Any pixel a contour edge crosses lies within half this physical width of
// that edge, so the stroked outline marks it with clearly nonzero coverage.
const EDGE_OUTLINE_WIDTH: f32 = 2.0;
/// Lowering stops at the largest validated viewport edge: vector consumers
/// never present a pixel at or beyond it.
pub(crate) const FIELD_EDGE: u32 = super::MAX_VIEWPORT_EDGE;
// The first allocation of a cell list; later growth doubles within the limit.
const FIRST_CELL_CAPACITY: usize = 4;

/// Disjoint whole physical pixels sharing one premultiplied field color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FieldCell {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub color: [u8; 4],
}

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
        if self.colors[self.partition.owner(x, y)] == center
            && self.colors[self.partition.owner(x + step, y)] == center
            && self.colors[self.partition.owner(x, y + step)] == center
            && self.colors[self.partition.owner(x + step, y + step)] == center
        {
            return center;
        }
        let mut sum = [0_u32; 4];
        let mut count = 0;
        for row in 0..COLOR_GRID {
            for column in 0..COLOR_GRID {
                for &[ox, oy] in &COLOR_OFFSETS {
                    let sx = x + step * (column as f64 + ox) / COLOR_GRID as f64;
                    let sy = y + step * (row as f64 + oy) / COLOR_GRID as f64;
                    if self.partition.in_ring(sx, sy) {
                        let color = self.colors[self.partition.owner(sx, sy)];
                        sum[0] += u32::from(color[0]);
                        sum[1] += u32::from(color[1]);
                        sum[2] += u32::from(color[2]);
                        sum[3] += u32::from(color[3]);
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
    /// Lower the field to disjoint whole-pixel cells for a vector consumer
    /// that applies this contour's even-odd coverage once (as a clip) and
    /// fills each cell with its color. Every pixel inside the contour or
    /// crossed by one of its edges receives exactly the software painter's
    /// `pixel` color, or no cell when that color is transparent (it paints
    /// nothing), so every cell has nonzero alpha. Other pixels are
    /// unconstrained, so a run may bridge a hole between equal colors.
    /// `transform` is the draw's uniform physical scale and translation;
    /// `bounds` are its physical bounds.
    ///
    /// `max_bytes` bounds this lowering's peak memory. It covers the scratch
    /// (one band mask and color row of `COLOR_BLOCK` rows, at most
    /// `FIELD_EDGE` wide, plus the transformed contour and its stroked
    /// outline, charged once stroked; both are proportional to the contour the
    /// recording budget already bounds), the cell lists counted by allocated
    /// capacity (a growing list's old buffer counts until it is released), and
    /// `retained_bytes_per_cell` for every cell the caller converts while the
    /// returned vector is still alive. `None` means the budget is exceeded or
    /// an allocation failed; the caller keeps the software fallback.
    pub(crate) fn cells(
        &self,
        transform: Transform,
        bounds: Rect,
        max_bytes: usize,
        retained_bytes_per_cell: usize,
    ) -> Option<Vec<FieldCell>> {
        let edge = FIELD_EDGE as f32;
        let left = bounds.left().floor().max(0.0);
        let top = bounds.top().floor().max(0.0);
        let right = bounds.right().ceil().min(edge);
        let bottom = bounds.bottom().ceil().min(edge);
        // Also rejects NaN; an offscreen field lowers to no cells.
        if !(left < right && top < bottom) {
            return Some(Vec::new());
        }
        let (left, top, right, bottom) = (left as u32, top as u32, right as u32, bottom as u32);
        let width = right - left;
        // Band-local AA is not exactly invariant under the band's integer
        // offset: a thin taper can read zero here yet nonzero in the software
        // painter's global mask. The fill marks interiors and the stroked
        // outline marks every pixel an edge crosses, independently of AA noise.
        let outline = self.path.clone().transform(transform)?.stroke(
            &Stroke {
                width: EDGE_OUTLINE_WIDTH,
                ..Stroke::default()
            },
            1.0,
        )?;
        let area = (width * COLOR_BLOCK) as usize;
        // The band mask holds one byte per pixel; the color row one slot.
        let scratch = area
            .checked_mul(1 + std::mem::size_of::<Option<[u8; 4]>>())?
            .checked_add(path_bytes(&self.path))?
            .checked_add(path_bytes(&outline))?;
        let per_cell = std::mem::size_of::<FieldCell>().checked_add(retained_bytes_per_cell)?;
        let mut builder = CellBuilder::new(max_bytes.checked_sub(scratch)? / per_cell);
        let mut mask = Mask::new(width, COLOR_BLOCK)?;
        let mut band = Vec::new();
        band.try_reserve_exact(area).ok()?;
        band.resize(area, None);
        // Identical coordinates to the software painter's global probes.
        let step = 1.0 / f64::from(transform.sx);
        let css_x = |px: u32| (f64::from(px) - f64::from(transform.tx)) * step;
        let css_y = |py: u32| (f64::from(py) - f64::from(transform.ty)) * step;
        for band_top in (top..bottom).step_by(COLOR_BLOCK as usize) {
            let rows = COLOR_BLOCK.min(bottom - band_top);
            let origin = Transform::from_translate(-(left as f32), -(band_top as f32));
            mask.clear();
            mask.fill_path(
                &self.path,
                FillRule::EvenOdd,
                true,
                transform.post_concat(origin),
            );
            mask.fill_path(&outline, FillRule::Winding, true, origin);
            band.fill(None);
            for bx in (0..width).step_by(COLOR_BLOCK as usize) {
                let w = COLOR_BLOCK.min(width - bx);
                let Some([cx, cy, cw, ch]) = covered_block(&mask, width, bx, 0, w, rows) else {
                    continue;
                };
                let uniform = self.uniform(
                    css_x(left + cx),
                    css_y(band_top + cy),
                    f64::from(cw) * step,
                    f64::from(ch) * step,
                );
                for row in cy..cy + ch {
                    for column in cx..cx + cw {
                        let index = (row * width + column) as usize;
                        if mask.data()[index] == 0 {
                            continue;
                        }
                        // A transparent color stays marked so no run covers it.
                        band[index] = Some(uniform.unwrap_or_else(|| {
                            self.pixel(css_x(left + column), css_y(band_top + row), step)
                        }));
                    }
                }
            }
            for row in 0..rows {
                let start = (row * width) as usize;
                builder.row(left, band_top + row, &band[start..start + width as usize])?;
            }
        }
        builder.finish()
    }
}

/// The whole-pixel rectangle of marked pixels inside one block, so a uniform
/// proof covers every color sample of a contributing pixel.
pub(crate) fn covered_block(
    mask: &Mask,
    stride: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Option<[u32; 4]> {
    let mut left = x + w;
    let mut right = x;
    let mut top = y + h;
    let mut bottom = y;
    for row in y..y + h {
        let start = (row * stride + x) as usize;
        let pixels = &mask.data()[start..start + w as usize];
        let Some(first) = pixels.iter().position(|alpha| *alpha != 0) else {
            continue;
        };
        let last = pixels.iter().rposition(|alpha| *alpha != 0)?;
        left = left.min(x + first as u32);
        right = right.max(x + last as u32 + 1);
        top = top.min(row);
        bottom = row + 1;
    }
    (left < right).then(|| [left, top, right - left, bottom - top])
}

fn path_bytes(path: &Path) -> usize {
    std::mem::size_of_val(path.points()) + path.verbs().len()
}

/// Row runs of equal color merged into rectangles when consecutive rows repeat
/// the same span and color. The four lists share one storage limit in cells,
/// counted by allocated capacity; a list's old buffer counts while it grows.
struct CellBuilder {
    limit: usize,
    storage: usize,
    cells: Vec<FieldCell>,
    open: Vec<FieldCell>,
    next: Vec<FieldCell>,
    runs: Vec<FieldCell>,
}
impl CellBuilder {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            storage: 0,
            cells: Vec::new(),
            open: Vec::new(),
            next: Vec::new(),
            runs: Vec::new(),
        }
    }
    fn push(
        target: &mut Vec<FieldCell>,
        storage: &mut usize,
        limit: usize,
        cell: FieldCell,
    ) -> Option<()> {
        if target.len() == target.capacity() {
            let old = target.capacity();
            // Doubling keeps the work linear; the old buffer is still live
            // while its contents move, so it stays charged during growth.
            let room = (limit - *storage).checked_sub(old)?;
            let grow = old.max(FIRST_CELL_CAPACITY).min(room);
            if grow == 0 {
                return None;
            }
            target.try_reserve_exact(grow).ok()?;
            if *storage + target.capacity() > limit {
                return None;
            }
            *storage = *storage - old + target.capacity();
        }
        target.push(cell);
        Some(())
    }
    fn row(&mut self, left: u32, y: u32, colors: &[Option<[u8; 4]>]) -> Option<()> {
        self.runs.clear();
        // Only uncovered pixels (`None`) may be bridged by a run; a covered
        // transparent pixel ends it and receives no cell.
        let mut extendable = false;
        for (offset, color) in colors.iter().enumerate() {
            let Some(color) = *color else {
                continue;
            };
            if color[3] == 0 {
                extendable = false;
                continue;
            }
            let x = left + offset as u32;
            match self.runs.last_mut() {
                Some(run) if extendable && run.color == color => run.width = x + 1 - run.x,
                _ => {
                    let cell = FieldCell {
                        x,
                        y,
                        width: 1,
                        height: 1,
                        color,
                    };
                    Self::push(&mut self.runs, &mut self.storage, self.limit, cell)?;
                    extendable = true;
                }
            }
        }
        // Both lists are sorted by x and disjoint; extend exact repeats. The
        // lists keep their buffers across rows, so storage stays counted.
        self.next.clear();
        let mut previous = 0;
        for &run in &self.runs {
            while let Some(&cell) = self.open.get(previous).filter(|cell| cell.x < run.x) {
                Self::push(&mut self.cells, &mut self.storage, self.limit, cell)?;
                previous += 1;
            }
            let repeated = self.open.get(previous).filter(|cell| {
                cell.x == run.x && cell.width == run.width && cell.color == run.color
            });
            let cell = match repeated {
                Some(&cell) => {
                    previous += 1;
                    FieldCell {
                        height: cell.height + 1,
                        ..cell
                    }
                }
                None => run,
            };
            Self::push(&mut self.next, &mut self.storage, self.limit, cell)?;
        }
        for &cell in &self.open[previous..] {
            Self::push(&mut self.cells, &mut self.storage, self.limit, cell)?;
        }
        std::mem::swap(&mut self.open, &mut self.next);
        Some(())
    }
    fn finish(mut self) -> Option<Vec<FieldCell>> {
        for &cell in &self.open {
            Self::push(&mut self.cells, &mut self.storage, self.limit, cell)?;
        }
        Some(self.cells)
    }
}

#[cfg(test)]
pub(crate) mod tests {
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

    const VIEWPORT: u32 = 240;
    const COLORS: [[u8; 4]; 4] = [
        [255, 0, 0, 128],
        [0, 0, 255, 128],
        [0, 255, 0, 128],
        [255, 255, 0, 128],
    ];
    fn radii(raw: &str, bounds: BoxRect) -> Corners {
        let env = crate::Environment::default();
        Corners::resolve(raw, bounds, crate::units::Units::new(&env).unwrap()).unwrap()
    }
    /// The even-odd ring exactly as `paint_border` builds a rounded contour.
    fn border(bounds: BoxRect, widths: [f32; 4], raw: &str) -> Border {
        let corners = radii(raw, bounds);
        let mut path = PathBuilder::new();
        path.push_path(&corners.clipped_path(bounds).unwrap());
        let (inner, inner_radii) = corners.inset(bounds, widths);
        if let Some(hole) = inner_radii.clipped_path(inner) {
            path.push_path(&hole);
        }
        let colors = COLORS.map(|[r, g, b, a]| Color::from_rgba8(r, g, b, a));
        Border::new(path.finish().unwrap(), bounds, widths, corners, colors)
    }
    /// Border contours with distinct side colors, shared with lowering tests.
    pub(crate) fn fixtures() -> Vec<Border> {
        let square = BoxRect {
            x: 10.25,
            y: 12.5,
            width: 160.0,
            height: 100.0,
        };
        let collision = BoxRect {
            x: 20.0,
            y: 20.0,
            width: 100.0,
            height: 100.0,
        };
        vec![
            border(square, [8.0, 8.0, 8.0, 8.0], "20px"),
            border(
                square,
                [3.5, 12.0, 0.0, 7.25],
                "7.25px 32.75px 18.5px 44.5px",
            ),
            border(collision, [30.0, 30.0, 30.0, 30.0], "90px 0 90px 0"),
            border(collision, [30.0, 0.0, 30.0, 0.0], "90px 0 0 0"),
        ]
    }

    #[test]
    fn cells_give_every_covered_pixel_the_software_color_exactly_once() {
        for border in fixtures() {
            for (scale, dx) in [(1.0, 0.0), (1.25, -20.0), (2.0, 0.0)] {
                let transform = Transform::from_scale(scale, scale).post_translate(dx, 0.0);
                let bounds = border.path.bounds().transform(transform).unwrap();
                let cells = border.cells(transform, bounds, usize::MAX, 0).unwrap();
                // A global viewport mask, as the software tile painter uses.
                let mut mask = Mask::new(VIEWPORT, VIEWPORT).unwrap();
                mask.fill_path(&border.path, FillRule::EvenOdd, true, transform);
                let mut owner = vec![None; (VIEWPORT * VIEWPORT) as usize];
                for (index, cell) in cells.iter().enumerate() {
                    assert!(cell.width > 0 && cell.height > 0 && cell.color[3] > 0);
                    for y in cell.y..(cell.y + cell.height).min(VIEWPORT) {
                        for x in cell.x..(cell.x + cell.width).min(VIEWPORT) {
                            let slot = &mut owner[(y * VIEWPORT + x) as usize];
                            assert_eq!(*slot, None, "overlapping cells at{x},{y}");
                            *slot = Some(index);
                        }
                    }
                }
                let step = 1.0 / f64::from(scale);
                for y in 0..VIEWPORT {
                    for x in 0..VIEWPORT {
                        let at = (y * VIEWPORT + x) as usize;
                        if mask.data()[at] == 0 {
                            continue;
                        }
                        let expected = border.pixel(
                            (f64::from(x) - f64::from(dx)) * step,
                            f64::from(y) * step,
                            step,
                        );
                        let Some(cell) = owner[at] else {
                            assert_eq!(expected[3], 0, "covered pixel has a cell at{x},{y}");
                            continue;
                        };
                        assert_eq!(cells[cell].color, expected, "{scale} at{x},{y}");
                    }
                }
            }
        }
    }

    #[test]
    fn cells_merge_uniform_sides_and_charge_their_peak_to_the_byte_budget() {
        // Any per-cell retained size; the caller's converted cell, for example.
        const RETAINED: usize = 32;
        let border = &fixtures()[0];
        let bounds = border.path.bounds();
        let lower = |max_bytes| border.cells(Transform::identity(), bounds, max_bytes, RETAINED);
        let cells = lower(usize::MAX).unwrap();
        let area: u64 = cells
            .iter()
            .map(|c| u64::from(c.width) * u64::from(c.height))
            .sum();
        // Runs and repeated rows merge: far fewer cells than covered pixels.
        const MIN_PIXELS_PER_CELL: usize = 16;
        assert!(
            cells.len() * MIN_PIXELS_PER_CELL < area as usize,
            "{} cells",
            cells.len()
        );
        // The output list alone, plus the retained conversion, exceeds this.
        let output = cells.len() * (std::mem::size_of::<FieldCell>() + RETAINED);
        assert_eq!(lower(output - 1), None);
        assert_eq!(lower(0), None);
        // Doubling growth with the old buffer charged, plus per-row lists and
        // the band scratch, fits four output lists and one mebibyte.
        const GROWTH_FACTOR: usize = 4;
        const SCRATCH_ALLOWANCE: usize = 1024 * 1024;
        assert_eq!(
            lower(GROWTH_FACTOR * output + SCRATCH_ALLOWANCE),
            Some(cells.clone())
        );
    }

    #[test]
    fn transparent_covered_pixels_get_no_cell_and_split_runs() {
        let mut builder = CellBuilder::new(usize::MAX);
        let red = [255, 0, 0, 255];
        let row = [Some(red), None, Some(red), Some([0; 4]), Some(red)];
        builder.row(0, 0, &row).unwrap();
        let cells = builder.finish().unwrap();
        let spans: Vec<_> = cells.iter().map(|c| (c.x, c.width)).collect();
        assert_eq!(spans, [(0, 3), (4, 1)], "an uncovered pixel may be bridged");
        assert!(cells.iter().all(|cell| cell.color == red));
    }

    #[test]
    fn cells_stop_at_the_validated_viewport_edges() {
        assert!(crate::paint::validate_dimensions(FIELD_EDGE, 1, 1.0).is_ok());
        assert!(crate::paint::validate_dimensions(FIELD_EDGE + 1, 1, 1.0).is_err());
        let border = &fixtures()[0];
        let edge = FIELD_EDGE as f32;
        for transform in [
            Transform::from_translate(-200.0, -200.0),
            Transform::from_translate(edge, 0.0),
            Transform::from_translate(edge - 40.0, edge - 40.0),
        ] {
            let bounds = border.path.bounds().transform(transform).unwrap();
            let cells = border.cells(transform, bounds, usize::MAX, 0).unwrap();
            for cell in cells {
                assert!(cell.x + cell.width <= FIELD_EDGE && cell.y + cell.height <= FIELD_EDGE);
            }
        }
        let offscreen = Transform::from_translate(-200.0, -200.0);
        let bounds = border.path.bounds().transform(offscreen).unwrap();
        assert_eq!(border.cells(offscreen, bounds, 0, 0), Some(Vec::new()));
    }
}
