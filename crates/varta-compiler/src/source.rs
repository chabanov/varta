use crate::CompilerError;
use std::path::Path;
use swc_common::{BytePos, FileName, SourceFile, SourceMap, Span, source_map::SmallPos, sync::Lrc};
use varta_common::{Diagnostic, DiagnosticCode, SourcePosition, SourceRange, SourceSnippet};

#[derive(Clone)]
pub(crate) struct Source {
    map: Lrc<SourceMap>,
    bom_bytes: usize,
    pub(crate) file: Lrc<SourceFile>,
}

impl Source {
    pub(crate) fn new(path: &Path, text: &str) -> Self {
        let map: Lrc<SourceMap> = Default::default();
        let file = map.new_source_file(FileName::Real(path.to_owned()).into(), text.to_owned());
        Self {
            map,
            file,
            bom_bytes: usize::from(text.starts_with('\u{feff}')) * '\u{feff}'.len_utf8(),
        }
    }

    /// The source text of `span`, for messages that quote a construct.
    pub(crate) fn text(&self, span: Span) -> Option<String> {
        swc_common::SourceMapper::span_to_snippet(&*self.map, span).ok()
    }

    pub(crate) fn locate(&self, error: CompilerError, span: Span) -> CompilerError {
        if error.diagnostic().is_some() {
            return error;
        }
        let diagnostic = self.diagnostic(error.code(), error.to_string(), span);
        CompilerError::Located {
            diagnostic: Box::new(diagnostic),
            source: Box::new(error),
        }
    }

    pub(crate) fn diagnostic(
        &self,
        code: DiagnosticCode,
        message: String,
        span: Span,
    ) -> Diagnostic {
        let start = self.clamp(span.lo());
        let end = self.clamp(span.hi()).max(start);
        Diagnostic {
            code,
            message,
            file: self.file.name.to_string(),
            range: SourceRange {
                start: self.position(start),
                end: self.position(end),
            },
            snippet: self.snippet(start, end),
        }
    }

    fn clamp(&self, pos: BytePos) -> BytePos {
        let mut offset = pos.0.saturating_sub(self.file.start_pos.0) as usize;
        offset = offset.min(self.file.src.len());
        while !self.file.src.is_char_boundary(offset) {
            offset -= 1;
        }
        BytePos(self.file.start_pos.0 + offset as u32)
    }

    fn position(&self, pos: BytePos) -> SourcePosition {
        let location = self.map.lookup_char_pos(pos);
        SourcePosition {
            line: location.line,
            column: location.col.to_usize() + 1,
            byte_offset: (pos.0 - self.file.start_pos.0) as usize + self.bom_bytes,
        }
    }

    fn snippet(&self, start: BytePos, end: BytePos) -> Option<SourceSnippet> {
        const CONTEXT: usize = 60;
        const WIDTH: usize = 160;
        let location = self.map.lookup_char_pos(start);
        let text = self.file.get_line(location.line.saturating_sub(1))?;
        let text = text.trim_end_matches('\r');
        let column = location.col.to_usize().min(text.chars().count());
        let skip = column.saturating_sub(CONTEXT);
        let left = text
            .char_indices()
            .nth(skip)
            .map_or(text.len(), |(offset, _)| offset);
        let right = text
            .char_indices()
            .nth(skip + WIDTH)
            .map_or(text.len(), |(offset, _)| offset);
        let line_start = self.file.line_begin_pos(start);
        let window_start = BytePos(line_start.0 + left as u32);
        let window_end = BytePos(line_start.0 + right as u32);
        let prefix = if left > 0 { "…" } else { "" };
        let suffix = if right < text.len() { "…" } else { "" };
        let mut display = String::from(prefix);
        for c in text[left..right].chars() {
            if c == '\t' {
                display.push_str("    ");
            } else {
                display.push(if c.is_control() { ' ' } else { c });
            }
        }
        display.push_str(suffix);
        let start_column = location.col_display;
        let window_column = self.map.lookup_char_pos(window_start).col_display;
        let end_location = self.map.lookup_char_pos(end.min(window_end));
        let highlight_width = if end_location.line == location.line {
            end_location.col_display.saturating_sub(start_column).max(1)
        } else {
            1
        };
        Some(SourceSnippet {
            text: display,
            highlight_start: start_column.saturating_sub(window_column) + usize::from(left > 0),
            highlight_width,
        })
    }
}

/// A source-wide limit can be reported without copying an oversized source into SWC.
pub(crate) fn limit_error(path: &Path, error: CompilerError) -> CompilerError {
    let position = SourcePosition {
        line: 1,
        column: 1,
        byte_offset: 0,
    };
    let diagnostic = Diagnostic {
        code: error.code(),
        message: error.to_string(),
        file: path.to_string_lossy().into_owned(),
        range: SourceRange {
            start: position,
            end: position,
        },
        snippet: None,
    };
    CompilerError::Located {
        diagnostic: Box::new(diagnostic),
        source: Box::new(error),
    }
}
