//! Owned decoration boundaries and atomic inlines. Source text and byte
//! offsets remain unchanged.
use super::{MAX_INLINE_WORK, TextMeasureError};
#[cfg(test)]
mod tests;
pub(crate) const MAX_EDGES: usize = crate::layout::MAX_LAYOUT_NODES * 2;

/// What an edge stands for in the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EdgeKind {
    /// The opening margin, border and padding of a non-replaced inline box.
    Opening,
    /// Its closing margin, border and padding.
    Closing,
    /// An atomic inline (an inline image's margin box): CSS Text 3 §5.1
    /// puts a soft wrap opportunity before and after it.
    Atom,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextEdge {
    pub owner: u32,
    pub index: usize,
    pub kind: EdgeKind,
    pub advance: f32,
}
impl TextEdge {
    pub(super) fn inline_box(self) -> parley::InlineBox {
        parley::InlineBox {
            id: u64::from(self.owner),
            index: self.index,
            kind: match self.kind {
                EdgeKind::Opening => parley::InlineBoxKind::InlineStart,
                EdgeKind::Closing => parley::InlineBoxKind::InlineEnd,
                EdgeKind::Atom => parley::InlineBoxKind::InFlow,
            },
            width: self.advance,
            height: 0.0,
        }
    }
}
pub(super) fn validate(text: &str, edges: &[TextEdge]) -> Result<usize, TextMeasureError> {
    if edges.len() > MAX_EDGES
        || edges.iter().any(|e| {
            e.index > text.len()
                || !text.is_char_boundary(e.index)
                || !e.advance.is_finite()
                || e.advance.abs() > crate::units::MAX_LENGTH
        })
        || edges.windows(2).any(|pair| pair[0].index > pair[1].index)
    {
        return Err(TextMeasureError::Limit);
    }
    let work = edges.len().saturating_mul(if edges.len() > 1 {
        edges.len().ilog2() as usize + 2
    } else {
        1
    });
    if work > MAX_INLINE_WORK {
        return Err(TextMeasureError::Limit);
    }
    Ok(work)
}
