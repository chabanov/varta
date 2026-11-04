//! Immutable logical text runs and checked native-position to source-byte mapping.
use super::{text_layout, tree::ProjectionError};
use accesskit::{Node, NodeId, Role, TextDirection, TextPosition, TextSelection};
use std::{collections::BTreeMap, ops::Range, sync::Arc};
use varta_view::{AccessibilityNode, AccessibilityText, AccessibilityTextRevision, FrameMetadata};

pub(super) struct TextSlot {
    pub revision: AccessibilityTextRevision,
    pub runs: Vec<NodeId>,
    pub ranges: Vec<Range<usize>>,
}
pub(super) struct RunBinding {
    pub owner: NodeId,
    pub revision: AccessibilityTextRevision,
    offsets: Arc<[usize]>,
    range: Range<usize>,
}
impl RunBinding {
    pub fn source_range(&self) -> Option<Range<usize>> {
        Some(self.source_offset(0)?..self.source_offset(self.range.len())?)
    }
    pub fn source_offset(&self, index: usize) -> Option<usize> {
        if index > self.range.len() {
            return None;
        }
        self.offsets
            .get(self.range.start.checked_add(index)?)
            .copied()
    }
}
pub(super) struct TextPublication {
    pub slot: TextSlot,
    pub nodes: Vec<(NodeId, Node)>,
    pub bindings: BTreeMap<NodeId, RunBinding>,
    pub selection: Option<TextSelection>,
}
pub(super) fn text_role(role: Role) -> bool {
    matches!(
        role,
        Role::TextInput
            | Role::MultilineTextInput
            | Role::SearchInput
            | Role::NumberInput
            | Role::PasswordInput
    )
}

fn position(
    text: &AccessibilityText,
    plans: &[text_layout::Run],
    runs: &[NodeId],
    source: usize,
    preferred: Option<(f64, f64)>,
) -> Option<TextPosition> {
    let index = text.source_offsets.binary_search(&source).ok()?;
    let canonical = || {
        plans
            .iter()
            .enumerate()
            .find(|(line, range)| index < range.characters.end || *line + 1 == plans.len())
    };
    let visual = preferred.and_then(|(x, y)| {
        plans
            .iter()
            .enumerate()
            .filter_map(|(run_index, run)| {
                if index < run.characters.start || index > run.characters.end {
                    return None;
                }
                let metrics = run.metrics.as_ref()?;
                let local = index - run.characters.start;
                let offset = if local < run.characters.len() {
                    *metrics.positions.get(local)?
                } else if local == 0 {
                    0.0
                } else {
                    *metrics.positions.get(local - 1)? + *metrics.widths.get(local - 1)?
                };
                let actual_x = if metrics.rtl {
                    metrics.bounds.x1 - f64::from(offset)
                } else {
                    metrics.bounds.x0 + f64::from(offset)
                };
                let actual_y = (metrics.bounds.y0 + metrics.bounds.y1) / 2.0;
                let dx = x - actual_x;
                let dy = y - actual_y;
                Some((run_index, run, dx.mul_add(dx, dy * dy)))
            })
            .min_by(|left, right| left.2.total_cmp(&right.2))
            .map(|(index, run, _)| (index, run))
    });
    let (line, range) = visual.or_else(canonical)?;
    Some(TextPosition {
        node: runs[line],
        character_index: index.checked_sub(range.characters.start)?,
    })
}

pub(super) fn project(
    source: &AccessibilityNode,
    owner: NodeId,
    old: Option<&TextSlot>,
    next: &mut u64,
    metadata: &FrameMetadata,
    remaining: usize,
) -> Result<TextPublication, ProjectionError> {
    let text = source.text.as_ref().expect("text field projection");
    let value = source.value.as_deref().expect("text field value");
    let plans =
        text_layout::plan(source, metadata, remaining).map_err(ProjectionError::Observer)?;
    let reusable = old
        .filter(|old| old.revision == text.revision)
        .map(|old| {
            old.ranges
                .iter()
                .zip(&old.runs)
                .map(|(range, id)| ((range.start, range.end), *id))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut runs = Vec::with_capacity(plans.len());
    for run in &plans {
        let id = if let Some(id) = reusable.get(&(run.characters.start, run.characters.end)) {
            *id
        } else {
            let id = NodeId(*next);
            *next = next
                .checked_add(1)
                .ok_or(ProjectionError::IdentityExhausted)?;
            id
        };
        runs.push(id);
    }
    let offsets: Arc<[usize]> = text.source_offsets.clone().into();
    let mut nodes = Vec::with_capacity(runs.len());
    let mut bindings = BTreeMap::new();
    let mut byte = 0;
    let reveal = metadata.text_reveal_supported(&source.identity, &text.revision);
    for (index, (id, run)) in runs.iter().zip(&plans).enumerate() {
        let range = &run.characters;
        let lengths = &text.character_lengths[range.clone()];
        let end = byte
            + lengths
                .iter()
                .map(|length| usize::from(*length))
                .sum::<usize>();
        let mut node = Node::new(Role::TextRun);
        if reveal {
            node.add_action(accesskit::Action::ScrollIntoView);
        }
        node.set_value(&value[byte..end]);
        node.set_character_lengths(lengths.to_vec());
        if let Some(metrics) = &run.metrics {
            node.set_bounds(metrics.bounds);
            node.set_character_positions(metrics.positions.clone());
            node.set_character_widths(metrics.widths.clone());
            node.set_text_direction(if metrics.rtl {
                TextDirection::RightToLeft
            } else {
                TextDirection::LeftToRight
            });
        }
        let word_start = text.word_starts.partition_point(|i| *i < range.start);
        let word_end = text.word_starts.partition_point(|i| *i < range.end);
        // Runs and their end positions fit u8 for native word navigation.
        node.set_word_starts(
            text.word_starts[word_start..word_end]
                .iter()
                .map(|i| (i - range.start) as u8)
                .collect::<Vec<_>>(),
        );
        if index > 0 && plans[index - 1].line == run.line {
            node.set_previous_on_line(runs[index - 1]);
        }
        if plans
            .get(index + 1)
            .is_some_and(|next| next.line == run.line)
        {
            node.set_next_on_line(runs[index + 1]);
        }
        nodes.push((*id, node));
        bindings.insert(
            *id,
            RunBinding {
                owner,
                revision: text.revision.clone(),
                offsets: offsets.clone(),
                range: range.clone(),
            },
        );
        byte = end;
    }
    let selection = text.selection.and_then(|selection| {
        let preferred = metadata
            .field_text_geometry(&source.identity, &text.revision)
            .map(|geometry| {
                geometry.selection_carets.map(|caret| {
                    let clip = geometry.clip;
                    let x = caret.x.clamp(clip.x, clip.x + clip.width);
                    let top = caret.y.clamp(clip.y, clip.y + clip.height);
                    let bottom = (caret.y + caret.height).clamp(clip.y, clip.y + clip.height);
                    (f64::from(x), f64::from((top + bottom) / 2.0))
                })
            });
        let focus = position(
            text,
            &plans,
            &runs,
            selection.focus,
            preferred.map(|p| p[1]),
        )?;
        let anchor = if selection.anchor == selection.focus {
            focus
        } else {
            position(
                text,
                &plans,
                &runs,
                selection.anchor,
                preferred.map(|p| p[0]),
            )?
        };
        Some(TextSelection { anchor, focus })
    });
    Ok(TextPublication {
        slot: TextSlot {
            revision: text.revision.clone(),
            runs,
            ranges: plans.into_iter().map(|run| run.characters).collect(),
        },
        nodes,
        bindings,
        selection,
    })
}
