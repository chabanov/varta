//! Visual native spans in source order, without reshaping painted text.
use accesskit::Rect;
use std::ops::Range;
use varta_view::{
    AccessibilityError, AccessibilityNode, AccessibilityText, FieldTextGeometry, FrameMetadata,
};

pub(super) struct Metrics {
    pub bounds: Rect,
    pub positions: Vec<f32>,
    pub widths: Vec<f32>,
    pub rtl: bool,
}
pub(super) struct Run {
    pub characters: Range<usize>,
    pub line: usize,
    pub metrics: Option<Metrics>,
}
#[derive(Clone, Copy)]
struct Unit {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    rtl: bool,
    line: usize,
}
impl Unit {
    fn leading(self) -> f32 {
        if self.rtl { self.right } else { self.left }
    }
    fn trailing(self) -> f32 {
        if self.rtl { self.left } else { self.right }
    }
    fn clipped(mut self, geometry: &FieldTextGeometry<'_>) -> Self {
        let clip = geometry.clip;
        self.left = self.left.clamp(clip.x, clip.x + clip.width);
        self.right = self.right.clamp(clip.x, clip.x + clip.width);
        self.top = self.top.clamp(clip.y, clip.y + clip.height);
        self.bottom = self.bottom.clamp(clip.y, clip.y + clip.height);
        self
    }
}
fn push(runs: &mut Vec<Run>, run: Run, remaining: usize) -> Result<(), AccessibilityError> {
    if runs.len() >= remaining {
        return Err(AccessibilityError::TextRunLimit {
            limit: varta_view::ACCESSIBILITY_TEXT_RUN_LIMIT,
        });
    }
    runs.push(run);
    Ok(())
}

fn units(text: &AccessibilityText, geometry: &FieldTextGeometry<'_>) -> Vec<Unit> {
    let count = text.character_lengths.len();
    let mut units = vec![None; count];
    for (line_index, line) in geometry.lines.iter().enumerate() {
        for cluster in &geometry.clusters[line.clusters.clone()] {
            let first = text
                .source_offsets
                .partition_point(|byte| *byte <= cluster.source.start)
                .saturating_sub(1);
            let end = text
                .source_offsets
                .partition_point(|byte| *byte < cluster.source.end)
                .min(count);
            let origin = if cluster.rtl {
                cluster.bounds.x + cluster.bounds.width
            } else {
                cluster.bounds.x
            };
            for (index, unit) in units.iter_mut().enumerate().take(end).skip(first) {
                // Interior scalar boundaries share the shaped cluster's leading
                // caret. Its final unit advances to the trailing caret, matching
                // Parley's byte-index cursor snapping for combining sequences.
                let width = if index + 1 == end {
                    cluster.bounds.width
                } else {
                    0.0
                };
                *unit = Some(Unit {
                    left: if cluster.rtl { origin - width } else { origin },
                    right: if cluster.rtl { origin } else { origin + width },
                    top: line.bounds.y,
                    bottom: line.bounds.y + line.bounds.height,
                    rtl: cluster.rtl,
                    line: line_index,
                });
            }
        }
    }
    // Nonprinting source units may have no glyph cluster. Keep their caret at
    // the next logical visible cluster, or at the preceding line-end caret.
    let mut line_ends = vec![None; geometry.lines.len()];
    for unit in units.iter().flatten() {
        line_ends[unit.line] = Some(*unit);
    }
    let mut next = None;
    for (index, slot) in units.iter_mut().enumerate().rev() {
        let start = text.source_offsets[index];
        let line = geometry
            .lines
            .partition_point(|line| line.source.end <= start)
            .min(geometry.lines.len() - 1);
        if let Some(unit) = *slot {
            next = Some(unit);
            continue;
        }
        let line_geometry = &geometry.lines[line];
        let neighbor = next.filter(|unit: &Unit| unit.line == line);
        let previous = line_ends[line];
        let x = neighbor
            .map(Unit::leading)
            .or_else(|| previous.map(Unit::trailing))
            .unwrap_or(line_geometry.bounds.x);
        *slot = Some(Unit {
            left: x,
            right: x,
            top: line_geometry.bounds.y,
            bottom: line_geometry.bounds.y + line_geometry.bounds.height,
            rtl: neighbor.is_some_and(|unit| unit.rtl),
            line,
        });
    }
    units
        .into_iter()
        .map(|unit| {
            unit.expect("every source unit has a caret")
                .clipped(geometry)
        })
        .collect()
}

pub(super) fn plan(
    source: &AccessibilityNode,
    metadata: &FrameMetadata,
    remaining: usize,
) -> Result<Vec<Run>, AccessibilityError> {
    let text = source.text.as_ref().expect("text field projection");
    let Some(geometry) = metadata.field_text_geometry(&source.identity, &text.revision) else {
        let mut runs = Vec::new();
        for run in &text.runs {
            push(
                &mut runs,
                Run {
                    characters: run.characters.clone(),
                    line: run.line,
                    metrics: None,
                },
                remaining,
            )?;
        }
        return Ok(runs);
    };
    let mut runs = Vec::new();
    if geometry.lines.len() > remaining {
        return Err(AccessibilityError::TextRunLimit {
            limit: varta_view::ACCESSIBILITY_TEXT_RUN_LIMIT,
        });
    }
    if text.character_lengths.is_empty() {
        let line = &geometry.lines[0];
        let x = line
            .bounds
            .x
            .clamp(geometry.clip.x, geometry.clip.x + geometry.clip.width);
        let top = line
            .bounds
            .y
            .clamp(geometry.clip.y, geometry.clip.y + geometry.clip.height);
        let bottom = (line.bounds.y + line.bounds.height)
            .clamp(geometry.clip.y, geometry.clip.y + geometry.clip.height);
        push(
            &mut runs,
            Run {
                characters: 0..0,
                line: 0,
                metrics: Some(Metrics {
                    bounds: Rect::new(x.into(), top.into(), x.into(), bottom.into()),
                    positions: Vec::new(),
                    widths: Vec::new(),
                    rtl: false,
                }),
            },
            remaining,
        )?;
        return Ok(runs);
    }
    let units = units(text, &geometry);
    let mut start = 0;
    while start < units.len() {
        let first = units[start];
        let mut end = start + 1;
        while end < units.len() && end - start < varta_view::ACCESSIBILITY_TEXT_RUN_CHARACTERS {
            let unit = units[end];
            let previous = units[end - 1];
            if unit.line != first.line
                || unit.rtl != first.rtl
                || if unit.rtl {
                    unit.leading() > previous.leading()
                } else {
                    unit.leading() < previous.leading()
                }
            {
                break;
            }
            // AccessKit intentionally treats password metrics as uniform within
            // a run. Clipped and fully visible masks must not share that run.
            if source.password
                && ((unit.right - unit.left) != (first.right - first.left)
                    || (unit.leading() - previous.leading()).abs() != (first.right - first.left))
            {
                break;
            }
            end += 1;
        }
        let span = &units[start..end];
        let left = span.iter().map(|unit| unit.left).fold(first.left, f32::min);
        let right = span
            .iter()
            .map(|unit| unit.right)
            .fold(first.right, f32::max);
        let positions = span
            .iter()
            .map(|unit| {
                if first.rtl {
                    right - unit.right
                } else {
                    unit.left - left
                }
            })
            .collect();
        let widths = span.iter().map(|unit| unit.right - unit.left).collect();
        push(
            &mut runs,
            Run {
                characters: start..end,
                line: first.line,
                metrics: Some(Metrics {
                    bounds: Rect::new(
                        left.into(),
                        first.top.into(),
                        right.into(),
                        first.bottom.into(),
                    ),
                    positions,
                    widths,
                    rtl: first.rtl,
                }),
            },
            remaining,
        )?;
        start = end;
    }
    for (line_index, line) in geometry
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.source.is_empty())
    {
        let index = text
            .source_offsets
            .partition_point(|byte| *byte < line.source.start);
        let x = line
            .bounds
            .x
            .clamp(geometry.clip.x, geometry.clip.x + geometry.clip.width);
        let top = line
            .bounds
            .y
            .clamp(geometry.clip.y, geometry.clip.y + geometry.clip.height);
        let bottom = (line.bounds.y + line.bounds.height)
            .clamp(geometry.clip.y, geometry.clip.y + geometry.clip.height);
        push(
            &mut runs,
            Run {
                characters: index..index,
                line: line_index,
                metrics: Some(Metrics {
                    bounds: Rect::new(x.into(), top.into(), x.into(), bottom.into()),
                    positions: Vec::new(),
                    widths: Vec::new(),
                    rtl: false,
                }),
            },
            remaining,
        )?;
    }
    runs.sort_by_key(|run| (run.characters.start, run.line));
    Ok(runs)
}
