//! Native CSS Text 3 white space for preserved spaces and tabs.
//!
//! `white-space: pre-wrap` and `pre` keep spaces, tabs and newlines, and one
//! inline flow may mix them with collapsing modes. Parley shapes preserved
//! white space as ordinary clusters; this module supplies what its line model
//! lacks, for the flows that need it:
//!
//! - Tab stops. Each preserved tab shapes as a space in its own run, and its
//!   advance reaches the next stop at `tab-size: 8` from the start of its
//!   line. As in Chrome, stops are measured in space advances of the flow
//!   root's (block container's) font, whatever the tab's own font: one flow
//!   has one grid of stops. A stop closer than half of that space is
//!   skipped. Stops depend on where lines break, so the flow is reshaped
//!   until every tab's advance matches the stop computed from its final
//!   line.
//! - Hanging. A sequence of preserved spaces and tabs at a soft wrap hangs as a
//!   whole. Parley hangs only the first overflowing space and starts the next
//!   line with the rest, so such a line is broken again with an advance that
//!   admits the whole sequence; Parley's own break opportunities then end it.
//! - Widths. Preserved spaces before a forced break or the end of the text
//!   conditionally hang: they count while they fit, fully in max-content and
//!   not in min-content. `pre` spaces never hang, and collapsible spaces at a
//!   line end are removed. Center/right alignment uses the same extents.
//!
//! The line breaker is driven through Parley's public API plus one hidden
//! hook, `BreakLines::set_prior_line_width` (`#[doc(hidden)]` in the vendored
//! Parley), which alignment relies on. Every realized line is checked
//! against the line this module expects; any disagreement refuses the layout
//! rather than painting a different one.
//! Field display keeps its own editing model and never takes this path.
use super::{TextMeasureError, TextMeasurer, TextMetrics, TextOptions, TextSource, TextSpan};
use super::{WhiteSpaceMode, edges::TextEdge};
use parley::{AlignmentOptions, BreakLines, BreakReason, Layout, LineBreakData, LineItem};
#[cfg(test)]
mod tests;

/// CSS `tab-size` initial value in space advances. The L0 grammar has no
/// `tab-size` property, so every preserved tab uses it.
pub(crate) const TAB_SIZE: f32 = 8.0;
/// A tab stop closer than this fraction of a space advance is skipped.
const MIN_TAB_STOP_FRACTION: f32 = 0.5;
/// Preserved tabs per shaping request. Each tab is its own styled run, and
/// Parley numbers styles with 16 bits, shared with the other style ranges.
pub(crate) const MAX_PRESERVED_TABS: usize = 16_384;
/// Shaping passes before tab stops must agree with the lines they produce.
/// Each pass fixes at least the first disagreeing tab; most flows need two.
const MAX_TAB_PASSES: usize = 8;
/// Tolerance, in CSS pixels, between a tab's shaped advance and its stop.
const TAB_EPSILON: f32 = 1.0 / 1024.0;
/// Tolerance, in CSS pixels, when locating a broken line by its advance.
const LINE_EPSILON: f32 = 1.0 / 1024.0;
/// A feature that is already on by default. Giving it to a tab's range keeps
/// that tab in its own shaping run whatever its spacing, so every pass shapes
/// the surrounding text identically.
const TAB_RUN_FEATURES: &str = "\"kern\" 1";

/// The style of one preserved tab: its byte offset and the spacing added to
/// the space it shapes as.
#[derive(Clone, Copy)]
pub(super) struct TabStyle {
    pub index: usize,
    pub spacing: f32,
}
impl TabStyle {
    pub(super) fn range(self) -> std::ops::Range<usize> {
        self.index..self.index + 1
    }
    pub(super) fn features() -> parley::FontFeatures<'static> {
        TAB_RUN_FEATURES.into()
    }
}

/// Whether the number of tabs in a flow's preserved text stays within the
/// per-request bound. Collapsible tabs never reach shaping.
pub(crate) fn tabs_admitted(text: &str) -> bool {
    text.bytes().filter(|&byte| byte == b'\t').count() <= MAX_PRESERVED_TABS
}

/// Whether shaping this flow takes the native white-space model.
pub(crate) fn native(text: &str, options: TextOptions, spans: &[TextSpan]) -> bool {
    !matches!(Flow::analyze(text, options, spans), Ok(None))
}

/// The white-space classification of one shaping request.
pub(super) struct Flow {
    /// Mode of each source byte; only character starts are read.
    modes: Vec<WhiteSpaceMode>,
    /// Byte offsets of preserved tabs, in order.
    tabs: Vec<usize>,
}
impl Flow {
    /// The flow's classification when its preserved text needs this model:
    /// a preserved tab, or a preserved space next to other white space or
    /// before a newline or the end. Other flows break exactly as Parley does.
    pub(super) fn analyze(
        text: &str,
        options: TextOptions,
        spans: &[TextSpan],
    ) -> Result<Option<Self>, TextMeasureError> {
        if options.source != TextSource::General
            || (!options.white_space.preserves()
                && spans
                    .iter()
                    .all(|span| !span.options.white_space.preserves()))
        {
            return Ok(None);
        }
        let mut modes = vec![options.white_space; text.len()];
        for span in spans {
            let target = modes
                .get_mut(span.range.clone())
                .ok_or(TextMeasureError::Limit)?;
            target.fill(span.options.white_space);
        }
        let mut tabs = Vec::new();
        let mut needed = false;
        let mut previous = None;
        let mut chars = text.char_indices().peekable();
        while let Some((index, ch)) = chars.next() {
            let next = chars.peek().map(|&(_, next)| next);
            if modes[index].preserves() {
                if ch == '\t' {
                    tabs.push(index);
                    needed = true;
                } else if ch == ' '
                    && (matches!(previous, Some(' ' | '\t'))
                        || next.is_none_or(|next| matches!(next, ' ' | '\t' | '\n')))
                {
                    needed = true;
                }
            }
            previous = Some(ch);
        }
        if tabs.len() > MAX_PRESERVED_TABS {
            return Err(TextMeasureError::Limit);
        }
        Ok(needed.then_some(Self { modes, tabs }))
    }

    /// Shaping text: preserved tabs become spaces of the same byte length.
    fn shaping_text(&self, text: &str) -> Result<String, TextMeasureError> {
        let mut bytes = text.as_bytes().to_vec();
        for &tab in &self.tabs {
            bytes[tab] = b' ';
        }
        String::from_utf8(bytes).map_err(|_| TextMeasureError::Limit)
    }
}

/// White space class of a cluster's first character, as CSS treats it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Blank {
    None,
    Space,
    Tab,
    /// No-break space: Parley counts it as trailing white space, CSS does not
    /// hang it.
    NoBreak,
    Newline,
}
impl Blank {
    /// Spaces and tabs: the characters that hang or collapse at a line end.
    fn hangs(self) -> bool {
        matches!(self, Self::Space | Self::Tab)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ligature {
    None,
    Start,
    Continuation,
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// A decoration edge (inline start or end) with its signed advance.
    Edge,
    Cluster {
        space: Blank,
        mode: WhiteSpaceMode,
        ligature: Ligature,
        /// Parley run index, which bounds its trailing white space.
        run: usize,
        /// Index into [`Flow::tabs`] for a preserved tab.
        tab: Option<usize>,
    },
}

/// One logical item of the flow: a cluster or a decoration edge.
#[derive(Clone, Copy, Debug)]
struct Unit {
    advance: f32,
    /// A cluster's source range end, or an edge's index.
    end: usize,
    kind: Kind,
}
impl Unit {
    fn space(self) -> Blank {
        match self.kind {
            Kind::Cluster { space, .. } => space,
            Kind::Edge => Blank::None,
        }
    }
}

/// One realized line: its units, why it ended, its advance and the maximum
/// advance it was broken with.
#[derive(Clone, Copy, Debug)]
struct Line {
    start: usize,
    end: usize,
    reason: BreakReason,
    advance: f32,
    max: f32,
}

/// Shapes and breaks `text` with native white space. Returns the result and
/// the charged operations a cached copy must pay again.
#[expect(clippy::too_many_arguments, reason = "one shaping request")]
pub(super) fn shape(
    measurer: &mut TextMeasurer,
    flow: &Flow,
    text: &str,
    width: Option<f32>,
    families: &[String],
    options: TextOptions,
    spans: &[TextSpan],
    edges: &[TextEdge],
) -> Result<((TextMetrics, Layout<u32>), usize), TextMeasureError> {
    let shaping = flow.shaping_text(text)?;
    let mut styles: Vec<TabStyle> = flow
        .tabs
        .iter()
        .map(|&index| TabStyle {
            index,
            spacing: 0.0,
        })
        .collect();
    // The shaped advance of each tab's space in its own font, before any
    // spacing, and the root font's space advance that sets the stops.
    let mut spaces = Vec::new();
    let mut operations = 0usize;
    let interval = if flow.tabs.is_empty() {
        0.0
    } else {
        let (space, built) = measurer.root_space_advance(families, options)?;
        operations = operations.saturating_add(built);
        space
    };
    let breaking = width.unwrap_or(f32::MAX);
    for pass in 0..MAX_TAB_PASSES {
        let (mut layout, built) =
            measurer.build_layout(&shaping, families, options, spans, edges, &styles)?;
        operations = operations.saturating_add(built);
        let units = units(&mut layout, text, flow)?;
        operations = operations.saturating_add(measurer.debit_shaping(units.len())?);
        if pass == 0 {
            spaces = units
                .iter()
                .filter(|unit| matches!(unit.kind, Kind::Cluster { tab: Some(_), .. }))
                .map(|unit| unit.advance)
                .collect();
            if spaces.len() != flow.tabs.len() {
                return Err(TextMeasureError::Unsupported);
            }
        }
        let lines = realize(&mut layout, &units, breaking, measurer, &mut operations)?;
        if flow.tabs.is_empty() || stops_agree(&units, &lines, (&spaces, interval), &mut styles) {
            return finish(measurer, (layout, &units), width, options, &mut operations)
                .map(|result| (result, operations));
        }
    }
    Err(TextMeasureError::Limit)
}

/// Intrinsic and final breaking, metrics and alignment of a converged layout.
fn finish(
    measurer: &mut TextMeasurer,
    shaped: (Layout<u32>, &[Unit]),
    width: Option<f32>,
    options: TextOptions,
    operations: &mut usize,
) -> Result<(TextMetrics, Layout<u32>), TextMeasureError> {
    let (mut layout, units) = shaped;
    // Min-content takes every soft wrap opportunity: each line's content
    // without the white space that hangs or is removed at its end. Tabs keep
    // the advances of the final lines; one that cannot end a min-content line
    // (a tab before a character that admits no break) is not re-stopped.
    let narrowest = realize(&mut layout, units, 0.0, measurer, operations)?;
    let min_width = narrowest
        .iter()
        .map(|line| extent(units, line, Some(0.0)))
        .fold(0.0, f32::max);
    let breaking = width.unwrap_or(f32::MAX);
    let lines = realize(&mut layout, units, breaking, measurer, operations)?;
    let text_width = lines
        .iter()
        .map(|line| extent(units, line, width))
        .fold(0.0, f32::max);
    if let (Some(alignment), Some(available)) = (options.align.parley(), width) {
        *operations = operations.saturating_add(measurer.debit_shaping(units.len())?);
        align(&mut layout, units, &lines, available)?;
        layout.align(alignment, AlignmentOptions::default());
    }
    Ok((
        TextMetrics {
            width: text_width,
            min_width,
            height: layout.height(),
            lines: layout.len(),
        },
        layout,
    ))
}

/// The flow's units in logical order, from forced-break lines.
fn units(layout: &mut Layout<u32>, text: &str, flow: &Flow) -> Result<Vec<Unit>, TextMeasureError> {
    layout.break_all_lines(None);
    // Tab stops, hanging and trailing white space are line-relative in
    // logical order; bidirectional reordering is not modeled here.
    if layout.is_rtl() {
        return Err(TextMeasureError::Unsupported);
    }
    let mut units = Vec::new();
    for line in layout.lines() {
        for item in line.items_unpositioned() {
            match item {
                LineItem::Run(run) => {
                    if run.is_rtl() {
                        return Err(TextMeasureError::Unsupported);
                    }
                    for cluster in run.clusters() {
                        let range = cluster.text_range();
                        let first = text.get(range.start..).and_then(|rest| rest.chars().next());
                        let space = if cluster.is_hard_line_break() {
                            Blank::Newline
                        } else {
                            match first {
                                Some('\t') => Blank::Tab,
                                Some(' ') => Blank::Space,
                                Some('\u{00a0}') => Blank::NoBreak,
                                _ => Blank::None,
                            }
                        };
                        let mode = *flow.modes.get(range.start).ok_or(TextMeasureError::Limit)?;
                        let tab = (space == Blank::Tab && mode.preserves())
                            .then(|| flow.tabs.binary_search(&range.start).ok())
                            .flatten();
                        units.push(Unit {
                            advance: cluster.advance(),
                            end: range.end,
                            kind: Kind::Cluster {
                                space,
                                mode,
                                ligature: if cluster.is_ligature_start() {
                                    Ligature::Start
                                } else if cluster.is_ligature_continuation() {
                                    Ligature::Continuation
                                } else {
                                    Ligature::None
                                },
                                run: run.index(),
                                tab,
                            },
                        });
                    }
                }
                LineItem::InlineBox(edge) => units.push(Unit {
                    advance: edge.width,
                    end: edge.index,
                    kind: Kind::Edge,
                }),
            }
        }
    }
    Ok(units)
}

/// The advance one breaker step adds for the unit group starting at
/// `index`, and the index after it. A ligature start and its continuations
/// advance together, and a newline adds nothing, as in Parley's breaker.
fn step(units: &[Unit], index: usize) -> (f32, usize) {
    let unit = units[index];
    match unit.kind {
        Kind::Cluster {
            space: Blank::Newline,
            ..
        } => (0.0, index + 1),
        Kind::Cluster {
            ligature: Ligature::Start,
            ..
        } => {
            let mut advance = unit.advance;
            let mut next = index + 1;
            while let Some(Kind::Cluster {
                ligature: Ligature::Continuation,
                ..
            }) = units.get(next).map(|unit| unit.kind)
            {
                advance += units[next].advance;
                next += 1;
            }
            (advance, next)
        }
        _ => (unit.advance, index + 1),
    }
}

/// Sets the advance Parley breaks the next line with. The layout maximum
/// follows it, so a line may be broken wider than the container.
fn limit(breaker: &mut BreakLines<'_, u32>, advance: f32) {
    let state = breaker.state_mut();
    state.set_layout_max_advance(advance);
    state.set_line_max_advance(advance);
}

fn next_line(breaker: &mut BreakLines<'_, u32>) -> Result<Option<LineBreakData>, TextMeasureError> {
    match breaker.break_next() {
        None => Ok(None),
        Some(parley::YieldData::LineBreak(line)) => Ok(Some(line)),
        // No maximum height or custom boxes are configured.
        Some(_) => Err(TextMeasureError::Unsupported),
    }
}

/// Breaks every line at `width`, letting a line that hangs a preserved
/// sequence keep all of it, and checks the realized lines.
fn realize(
    layout: &mut Layout<u32>,
    units: &[Unit],
    width: f32,
    measurer: &mut TextMeasurer,
    operations: &mut usize,
) -> Result<Vec<Line>, TextMeasureError> {
    *operations = operations.saturating_add(measurer.debit_shaping(units.len())?);
    let mut lines = Vec::new();
    let mut breaker = layout.break_lines();
    let mut start = 0;
    loop {
        let checkpoint = breaker.state().clone();
        limit(&mut breaker, width);
        let Some(data) = next_line(&mut breaker)? else {
            break;
        };
        let mut line = locate(units, start, &data, width)?;
        if let Some(extended) = hanging_advance(units, &line, width) {
            *operations = operations.saturating_add(measurer.debit_shaping(line.end - line.start)?);
            breaker.revert_to(checkpoint);
            limit(&mut breaker, extended);
            let data = next_line(&mut breaker)?.ok_or(TextMeasureError::Unsupported)?;
            line = locate(units, start, &data, extended)?;
            if hanging_advance(units, &line, extended).is_some() {
                return Err(TextMeasureError::Unsupported);
            }
        }
        start = line.end;
        lines.push(line);
    }
    limit(&mut breaker, width);
    breaker.finish();
    verify(layout, units, &lines)?;
    Ok(lines)
}

/// Locates the line Parley just broke from `start`: through its newline,
/// to the end, or where the units' running advance reaches its advance.
fn locate(
    units: &[Unit],
    start: usize,
    data: &LineBreakData,
    max: f32,
) -> Result<Line, TextMeasureError> {
    let end = match data.reason {
        BreakReason::None => units.len(),
        BreakReason::Explicit => {
            start
                + units[start..]
                    .iter()
                    .position(|unit| unit.space() == Blank::Newline)
                    .ok_or(TextMeasureError::Unsupported)?
                + 1
        }
        BreakReason::Regular | BreakReason::Emergency => {
            let mut x = 0.0f32;
            let mut index = start;
            loop {
                if index >= units.len() {
                    return Err(TextMeasureError::Unsupported);
                }
                let (advance, next) = step(units, index);
                x += advance;
                index = next;
                if (x - data.advance).abs() <= LINE_EPSILON {
                    // Zero-advance clusters (such as a zero-width space) do
                    // not admit a break before them; they end this line.
                    while units.get(index).is_some_and(|unit| {
                        matches!(unit.kind, Kind::Cluster { .. })
                            && unit.space() != Blank::Newline
                            && unit.advance == 0.0
                    }) {
                        index += 1;
                    }
                    break index;
                }
            }
        }
    };
    Ok(Line {
        start,
        end,
        reason: data.reason,
        advance: data.advance,
        max,
    })
}

/// The advance that keeps a whole hanging sequence on `line`, when Parley
/// hung only its first overflowing space or tab and more follow.
fn hanging_advance(units: &[Unit], line: &Line, width: f32) -> Option<f32> {
    if line.reason != BreakReason::Regular
        || line.advance <= width
        || line.end <= line.start
        || !units[line.end - 1].space().hangs()
    {
        return None;
    }
    let mut x = line.advance;
    let mut index = line.end;
    while let Some(unit) = units.get(index).filter(|unit| unit.space().hangs()) {
        x += unit.advance;
        index += 1;
    }
    (index > line.end).then_some(x)
}

/// Checks that Parley's lines end where this module located them.
fn verify(layout: &Layout<u32>, units: &[Unit], lines: &[Line]) -> Result<(), TextMeasureError> {
    if layout.len() != lines.len() {
        return Err(TextMeasureError::Unsupported);
    }
    for (realized, line) in layout.lines().zip(lines) {
        let range = realized.text_range();
        let matches = match units[line.start..line.end]
            .iter()
            .map(|unit| unit.end)
            .max()
        {
            Some(end) => range.end == end,
            None => range.is_empty(),
        };
        if !matches {
            return Err(TextMeasureError::Unsupported);
        }
    }
    Ok(())
}

/// The CSS width of `line`: its advance without the white space that hangs
/// or is removed at its end. Before a forced break or the end of the text,
/// preserved wrapping spaces conditionally hang, counting while they fit in
/// `width` (all of them in an intrinsic size); `pre` spaces never hang.
fn extent(units: &[Unit], line: &Line, width: Option<f32>) -> f32 {
    let mut hanging = 0.0f32;
    let mut removed = 0.0f32;
    for unit in units[line.start..line.end].iter().rev() {
        match unit.kind {
            Kind::Cluster {
                space: Blank::Newline,
                ..
            } => {}
            Kind::Cluster { space, mode, .. } if space.hangs() => {
                if !mode.preserves() {
                    removed += unit.advance;
                } else if mode.wraps() {
                    hanging += unit.advance;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    let content = line.advance - hanging - removed;
    match line.reason {
        BreakReason::Regular | BreakReason::Emergency => content,
        BreakReason::Explicit | BreakReason::None => {
            content + width.map_or(hanging, |width| hanging.min((width - content).max(0.0)))
        }
    }
}

/// The trailing white space Parley's alignment hangs: the white-space
/// clusters at the end of the line's last run.
fn parley_trailing(units: &[Unit], line: &Line) -> f32 {
    let mut trailing = 0.0f32;
    let mut last_run = None;
    for unit in units[line.start..line.end].iter().rev() {
        match unit.kind {
            Kind::Cluster { space, run, .. } if last_run.is_none_or(|last| last == run) => {
                last_run = Some(run);
                if space == Blank::None {
                    break;
                }
                trailing += unit.advance;
            }
            _ => break,
        }
    }
    trailing
}

/// Rebreaks the same lines with each line's alignment width, so Parley's
/// alignment, which hangs every trailing white-space cluster, centers or
/// right-aligns the CSS extent instead.
fn align(
    layout: &mut Layout<u32>,
    units: &[Unit],
    lines: &[Line],
    width: f32,
) -> Result<(), TextMeasureError> {
    let mut breaker = layout.break_lines();
    for line in lines {
        limit(&mut breaker, line.max);
        next_line(&mut breaker)?.ok_or(TextMeasureError::Unsupported)?;
        let hung = line.advance - extent(units, line, Some(width));
        let correction = hung - parley_trailing(units, line);
        breaker.set_prior_line_width(width + correction);
    }
    if next_line(&mut breaker)?.is_some() {
        return Err(TextMeasureError::Unsupported);
    }
    limit(&mut breaker, width);
    breaker.finish();
    verify(layout, units, lines)
}

/// The advance of a tab whose line position is `x`, for a space advance of
/// `space`: Blink's `Font::TabWidth`.
pub(crate) fn tab_advance(x: f32, space: f32) -> f32 {
    let stop = TAB_SIZE * space;
    if stop <= 0.0 {
        return 0.0;
    }
    let mut distance = stop - x % stop;
    if distance < space * MIN_TAB_STOP_FRACTION {
        distance += stop;
    }
    distance
}

/// Recomputes every tab's stop from the lines it was broken into, on the
/// grid of `interval` (the root font's space advance). `spaces` holds each
/// tab's space advance in its own font, which its spacing extends. Returns
/// whether all shaped advances already match; otherwise updates `styles`
/// for the next pass.
fn stops_agree(
    units: &[Unit],
    lines: &[Line],
    (spaces, interval): (&[f32], f32),
    styles: &mut [TabStyle],
) -> bool {
    let mut agree = true;
    for line in lines {
        let mut x = 0.0f32;
        let mut index = line.start;
        while index < line.end {
            let (advance, next) = step(units, index);
            if let Kind::Cluster { tab: Some(tab), .. } = units[index].kind {
                let stop = tab_advance(x, interval);
                if (stop - advance).abs() > TAB_EPSILON {
                    agree = false;
                }
                styles[tab].spacing = stop - spaces[tab];
                x += stop;
            } else {
                x += advance;
            }
            index = next;
        }
    }
    agree
}
