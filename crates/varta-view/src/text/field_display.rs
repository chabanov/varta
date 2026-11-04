//! Display-only transformations, retaining original field byte boundaries.
use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) const PASSWORD_MASK: &str = "•";

pub(crate) struct SourceMap {
    boundaries: Vec<(usize, usize)>,
    display_step: Option<usize>,
}
impl SourceMap {
    pub fn display_floor(&self, source: usize) -> usize {
        let index = self
            .boundaries
            .partition_point(|(_, offset)| *offset <= source)
            .saturating_sub(1);
        self.boundaries[index].0
    }
    pub fn display_ceil(&self, source: usize) -> usize {
        let index = self
            .boundaries
            .partition_point(|(_, offset)| *offset < source)
            .min(self.boundaries.len() - 1);
        self.boundaries[index].0
    }
    pub fn source(&self, display: usize) -> usize {
        if let Some(step) = self.display_step {
            return self.boundaries[(display / step).min(self.boundaries.len() - 1)].1;
        }
        let index = self
            .boundaries
            .partition_point(|(offset, _)| *offset <= display)
            .saturating_sub(1);
        self.boundaries[index].1
    }
}

pub(crate) fn password(value: &str) -> (String, SourceMap) {
    let mut display = String::new();
    let mut boundaries = vec![(0, 0)];
    for (byte, grapheme) in value.grapheme_indices(true) {
        display.push_str(PASSWORD_MASK);
        boundaries.push((display.len(), byte + grapheme.len()));
    }
    (
        display,
        SourceMap {
            boundaries,
            display_step: Some(PASSWORD_MASK.len()),
        },
    )
}

pub(crate) fn normalized_breaks(value: &str) -> Cow<'_, str> {
    if value.contains("\r\n") {
        Cow::Owned(value.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(value)
    }
}

pub(crate) fn normalize_crlf(value: &str) -> Option<(String, SourceMap)> {
    let Cow::Owned(display) = normalized_breaks(value) else {
        return None;
    };
    let mut display_offset = 0;
    let mut boundaries = vec![(0, 0)];
    let mut chars = value.char_indices().peekable();
    while let Some((byte, ch)) = chars.next() {
        let end = if ch == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
            chars.next();
            display_offset += 1;
            byte + 2
        } else {
            display_offset += ch.len_utf8();
            byte + ch.len_utf8()
        };
        boundaries.push((display_offset, end));
    }
    Some((
        display,
        SourceMap {
            boundaries,
            display_step: None,
        },
    ))
}
