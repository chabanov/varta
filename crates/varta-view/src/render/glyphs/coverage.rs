//! Coverage cells merge into disjoint physical rectangles. Existing CPU/GPU
//! primitives can draw these with no image resource or new scene-cache budget.
use super::*;
use crate::GlyphRasterMask;
#[derive(Clone, Copy)]
pub(crate) struct Span {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub alpha: u8,
}
pub(crate) struct Coverage {
    pub bounds: GlyphRasterBounds,
    pub spans: Vec<Span>,
}
pub(crate) enum Outcome {
    Missing,
    Culled,
    Ready(Arc<Coverage>),
}
impl Coverage {
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.spans.capacity() * std::mem::size_of::<Span>()
    }
    pub fn new(mask: GlyphRasterMask) -> Result<Self, PaintError> {
        let mut spans = Vec::<Span>::new();
        let mut previous = BTreeMap::<(usize, usize, u8), usize>::new();
        let w = mask.bounds.width as usize;
        for (y, row) in mask.coverage.chunks(w.max(1)).enumerate() {
            let mut current = BTreeMap::new();
            let mut x = 0;
            while x < row.len() {
                let alpha = row[x];
                let start = x;
                x += 1;
                while x < row.len() && row[x] == alpha {
                    x += 1;
                }
                if alpha == 0 {
                    continue;
                }
                let key = (start, x - start, alpha);
                if let Some(&index) = previous.get(&key) {
                    spans[index].height += 1;
                    current.insert(key, index);
                } else {
                    spans.try_reserve(1).map_err(|_| PaintError::Dimensions)?;
                    current.insert(key, spans.len());
                    spans.push(Span {
                        x: start as u32,
                        y: y as u32,
                        width: (x - start) as u32,
                        height: 1,
                        alpha,
                    });
                }
            }
            previous = current;
        }
        Ok(Self {
            bounds: mask.bounds,
            spans,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merged_rectangles_preserve_every_coverage_cell_without_overlap() {
        for width in 1..=12 {
            for height in 1..=9 {
                let values = (0..width * height)
                    .map(|i| [((i / width) % 3) * 40, ((i * 17) % 256), 0, 255][i % 4] as u8)
                    .collect::<Vec<_>>();
                let mask = GlyphRasterMask::new(
                    GlyphRasterBounds {
                        left: -3,
                        top: -7,
                        width: width as u32,
                        height: height as u32,
                    },
                    values.clone(),
                )
                .unwrap();
                let coverage = Coverage::new(mask).unwrap();
                let mut actual = vec![0; values.len()];
                let mut visits = vec![0; values.len()];
                for span in coverage.spans {
                    for y in span.y..span.y + span.height {
                        for x in span.x..span.x + span.width {
                            let i = y as usize * width + x as usize;
                            actual[i] = span.alpha;
                            visits[i] += 1;
                        }
                    }
                }
                assert_eq!(actual, values);
                assert!(visits.into_iter().all(|n| n <= 1));
            }
        }
    }
    #[test]
    fn repeated_rows_merge_without_coalescing_different_alpha_values() {
        let mask = GlyphRasterMask::new(
            GlyphRasterBounds {
                left: 0,
                top: 0,
                width: 4,
                height: 20,
            },
            [0, 50, 200, 255].repeat(20),
        )
        .unwrap();
        let coverage = Coverage::new(mask).unwrap();
        assert_eq!(coverage.spans.len(), 3);
        assert!(coverage.spans.iter().all(|span| span.height == 20));
    }
}
