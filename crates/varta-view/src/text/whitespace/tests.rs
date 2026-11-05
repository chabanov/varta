//! Native white-space model regressions. The inline fixture's space and `A`
//! advance 8px at 20px, so a tab stop falls every 64px.
use super::*;
use crate::text::{TextAlign, TextMeasurer, TextOptions, TextSource, TextSpan, WhiteSpaceMode};
use parley::Layout;

const SIZE: f32 = 20.0;
const SPACE: f32 = 8.0;
const STOP: f32 = TAB_SIZE * SPACE;

fn measurer() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("../../../tests/fixtures/inline.ttf").to_vec(),
        "Varta Inline Fixture",
    )
    .unwrap()
}

fn options(white_space: WhiteSpaceMode) -> TextOptions {
    TextOptions {
        size: SIZE,
        line_height: None,
        weight: 400.0,
        italic: false,
        white_space,
        break_anywhere: false,
        source: TextSource::General,
        align: TextAlign::Start,
    }
}

fn shape(
    text: &str,
    width: Option<f32>,
    options: TextOptions,
) -> Result<(TextMetrics, Layout<u32>), TextMeasureError> {
    let mut font = measurer();
    let families = vec!["Varta Inline Fixture".to_owned()];
    font.shape_styled(text, width, &families, options)
}

/// Each cluster's source byte and advance, in logical order.
fn advances(layout: &Layout<u32>) -> Vec<(usize, f32)> {
    layout
        .lines()
        .flat_map(|line| line.runs().collect::<Vec<_>>())
        .flat_map(|run| {
            run.clusters()
                .map(|cluster| (cluster.text_range().start, cluster.advance()))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn advance_at(layout: &Layout<u32>, byte: usize) -> f32 {
    advances(layout)
        .into_iter()
        .find(|&(start, _)| start == byte)
        .map(|(_, advance)| advance)
        .unwrap()
}

fn line_ranges(layout: &Layout<u32>) -> Vec<std::ops::Range<usize>> {
    layout.lines().map(|line| line.text_range()).collect()
}

#[test]
fn tab_stops_follow_blink_tab_width() {
    assert_eq!(tab_advance(0.0, SPACE), STOP);
    assert_eq!(tab_advance(SPACE, SPACE), STOP - SPACE);
    // Exactly half a space before a stop still reaches it; closer skips it.
    assert_eq!(tab_advance(STOP - SPACE / 2.0, SPACE), SPACE / 2.0);
    assert_eq!(tab_advance(STOP - 2.0, SPACE), STOP + 2.0);
    assert_eq!(tab_advance(STOP, SPACE), STOP);
    // A font without a space advance has no stops.
    assert_eq!(tab_advance(10.0, 0.0), 0.0);
}

#[test]
fn only_preserved_text_that_needs_it_takes_the_native_model() {
    let analyze = |text: &str, mode| Flow::analyze(text, options(mode), &[]).unwrap();
    for mode in [WhiteSpaceMode::Normal, WhiteSpaceMode::NoWrap] {
        assert!(analyze("A\t  B ", mode).is_none());
    }
    for mode in [WhiteSpaceMode::PreWrap, WhiteSpaceMode::Pre] {
        // A single inner or leading space breaks exactly as Parley does.
        assert!(analyze("A B", mode).is_none());
        assert!(analyze(" A", mode).is_none());
        for text in ["A  B", "A\tB", "A ", "A \nB"] {
            assert!(analyze(text, mode).is_some(), "{text:?} {mode:?}");
        }
    }
    assert_eq!(analyze("\tA\t", WhiteSpaceMode::Pre).unwrap().tabs, [0, 2]);
    // Field display keeps its own editing model.
    let mut field = options(WhiteSpaceMode::PreWrap);
    field.source = TextSource::Field;
    assert!(Flow::analyze("A\tB  ", field, &[]).unwrap().is_none());
    // A preserving span in a collapsing flow.
    let span = |range, white_space| TextSpan {
        range,
        owner: 1,
        options: options(white_space),
        families: vec!["Varta Inline Fixture".into()],
    };
    let spans = [
        span(0..2, WhiteSpaceMode::Normal),
        span(2..4, WhiteSpaceMode::PreWrap),
    ];
    let flow = Flow::analyze("A   ", options(WhiteSpaceMode::Normal), &spans).unwrap();
    assert!(flow.is_some());
    // The preserved tab bound is checked before shaping.
    let tabs = "\t".repeat(MAX_PRESERVED_TABS + 1);
    assert!(tabs_admitted(&tabs[1..]));
    assert!(!tabs_admitted(&tabs));
    assert!(matches!(
        Flow::analyze(&tabs, options(WhiteSpaceMode::Pre), &[]),
        Err(TextMeasureError::Limit)
    ));
}

#[test]
fn tabs_advance_to_stops_from_their_line_start() {
    for mode in [WhiteSpaceMode::PreWrap, WhiteSpaceMode::Pre] {
        let (metrics, layout) = shape("A\tA\t\tA", None, options(mode)).unwrap();
        assert_eq!(advance_at(&layout, 1), STOP - SPACE);
        assert_eq!(advance_at(&layout, 3), STOP - SPACE);
        assert_eq!(advance_at(&layout, 4), STOP);
        assert_eq!(metrics.width, 3.0 * STOP + SPACE);
        // A tab after a forced break starts from the new line.
        let (_, layout) = shape("AAAA\n\tA", None, options(mode)).unwrap();
        assert_eq!(advance_at(&layout, 5), STOP);
    }
    // A wrapped line measures its tab from where it starts.
    let (_, layout) = shape(
        "A A A\tA A\tA",
        Some(60.0),
        options(WhiteSpaceMode::PreWrap),
    )
    .unwrap();
    let lines = line_ranges(&layout);
    assert_eq!(lines[0], 0..6, "the first tab hangs");
    assert_eq!(advance_at(&layout, 5), STOP - 5.0 * SPACE);
    assert_eq!(advance_at(&layout, 9), STOP - 3.0 * SPACE);
}

/// One span over `range` of the flow, with its own white space and size.
fn sized_span(range: std::ops::Range<usize>, white_space: WhiteSpaceMode, size: f32) -> TextSpan {
    TextSpan {
        range,
        owner: 1,
        options: TextOptions {
            size,
            ..options(white_space)
        },
        families: vec!["Varta Inline Fixture".into()],
    }
}

#[test]
fn tab_stops_use_the_root_font_space_in_every_span() {
    let families = vec!["Varta Inline Fixture".to_owned()];
    let pre_wrap = options(WhiteSpaceMode::PreWrap);
    // A tab in a half-size or a larger span still stops on the root's
    // 64px grid, not on the 32px or 96px grid of its own space.
    for size in [SIZE / 2.0, SIZE * 1.5] {
        let spans = [sized_span(1..3, WhiteSpaceMode::PreWrap, size)];
        let (metrics, layout) = measurer()
            .with_view_shaping(|font| {
                font.shape_with_spans("A\tA", None, &families, pre_wrap, &spans)
            })
            .unwrap();
        assert_eq!(advance_at(&layout, 1), STOP - SPACE, "{size}");
        assert_eq!(metrics.width, STOP + SPACE * size / SIZE, "{size}");
    }
    // Two tabs of different sizes share the grid: the second starts on a
    // stop and advances a whole interval.
    let spans = [
        sized_span(1..3, WhiteSpaceMode::PreWrap, SIZE * 1.5),
        sized_span(3..5, WhiteSpaceMode::PreWrap, SIZE / 2.0),
    ];
    let (_, layout) = measurer()
        .with_view_shaping(|font| {
            font.shape_with_spans("AA\t\tA", None, &families, pre_wrap, &spans)
        })
        .unwrap();
    assert_eq!(advance_at(&layout, 2), STOP - SPACE - 1.5 * SPACE);
    assert_eq!(advance_at(&layout, 3), STOP);
}

#[test]
fn a_wrapping_span_in_a_non_wrapping_root_wraps_at_min_content() {
    let families = vec!["Varta Inline Fixture".to_owned()];
    for root in [WhiteSpaceMode::Pre, WhiteSpaceMode::NoWrap] {
        // `A` keeps the root's mode; ` A A` wraps after each space. No
        // preserved space needs the native model, so Parley measures it.
        let text = "A A A";
        let spans = [sized_span(1..text.len(), WhiteSpaceMode::Normal, SIZE)];
        assert!(
            Flow::analyze(text, options(root), &spans)
                .unwrap()
                .is_none()
        );
        let metrics = measurer()
            .with_view_shaping(|font| {
                font.shape_with_spans(text, None, &families, options(root), &spans)
            })
            .unwrap()
            .0;
        assert_eq!((metrics.min_width, metrics.width), (SPACE, 5.0 * SPACE));
        // Without a wrapping span the root's mode keeps one line.
        let metrics = measurer()
            .shape_styled(text, None, &families, options(root))
            .unwrap()
            .0;
        assert_eq!(metrics.min_width, 5.0 * SPACE, "{root:?}");
    }
}

#[test]
fn a_run_of_preserved_spaces_or_tabs_hangs_as_a_whole() {
    let pre_wrap = options(WhiteSpaceMode::PreWrap);
    // Eleven clusters fill 88px; the fourth space overflows 100px.
    let (metrics, layout) = shape("A A A A A A    A", Some(100.0), pre_wrap).unwrap();
    assert_eq!(line_ranges(&layout), [0..15, 15..16]);
    // Hanging spaces do not count in the line's width.
    assert_eq!(metrics.width, 11.0 * SPACE);
    let (_, layout) = shape("A A A A A A\t\tA", Some(100.0), pre_wrap).unwrap();
    assert_eq!(line_ranges(&layout), [0..13, 13..14]);
    assert_eq!(advance_at(&layout, 11), 2.0 * STOP - 11.0 * SPACE);
    assert_eq!(advance_at(&layout, 12), STOP);
    // A hanging run before a forced break keeps the break on its line.
    let (_, layout) = shape("A A A A A A    \nA", Some(100.0), pre_wrap).unwrap();
    assert_eq!(line_ranges(&layout), [0..16, 16..17]);
}

#[test]
fn preserved_spaces_conditionally_hang_before_forced_breaks() {
    let pre_wrap = options(WhiteSpaceMode::PreWrap);
    // Max-content keeps spaces before a forced break and at the end;
    // min-content hangs them, and tabs, at every opportunity.
    let (metrics, _) = shape("A   \nA  ", None, pre_wrap).unwrap();
    assert_eq!((metrics.width, metrics.min_width), (4.0 * SPACE, SPACE));
    let (metrics, _) = shape("A\tA A A", None, pre_wrap).unwrap();
    assert_eq!(
        (metrics.width, metrics.min_width),
        (STOP + 5.0 * SPACE, SPACE)
    );
    let (metrics, _) = shape("  A", None, pre_wrap).unwrap();
    assert_eq!((metrics.width, metrics.min_width), (3.0 * SPACE, SPACE));
    // At a definite width they count only while they fit.
    let (metrics, _) = shape("A      \nA", Some(30.0), pre_wrap).unwrap();
    assert_eq!(metrics.width, 30.0);
    // `pre` spaces never hang: they are part of both intrinsic sizes.
    let (metrics, _) = shape("A\tA   ", None, options(WhiteSpaceMode::Pre)).unwrap();
    assert_eq!(
        (metrics.width, metrics.min_width),
        (STOP + 4.0 * SPACE, STOP + 4.0 * SPACE)
    );
}

#[test]
fn center_alignment_counts_conditionally_hanging_spaces() {
    let mut centered = options(WhiteSpaceMode::PreWrap);
    centered.align = TextAlign::Center;
    let (_, layout) = shape("A  \nA A A A A A    A", Some(100.0), centered).unwrap();
    let offsets: Vec<f32> = layout.lines().map(|line| line.metrics().offset).collect();
    // "A  " is 24px wide; a soft-wrapped line hangs all four of its spaces.
    assert_eq!(offsets, [38.0, 6.0, 46.0]);
}

#[test]
fn native_white_space_work_is_charged_and_bidi_is_refused() {
    let mut font = measurer();
    let families = vec!["Varta Inline Fixture".to_owned()];
    let pre = options(WhiteSpaceMode::Pre);
    font.with_view_shaping(|font| font.measure_styled("A\tA  ", None, &families, pre))
        .unwrap();
    let charged = font.last_view_shaping_operations();
    font.with_view_shaping(|font| font.measure_styled("A A", None, &families, pre))
        .unwrap();
    let single = font.last_view_shaping_operations();
    assert!(charged > single, "{charged} > {single}");
    assert!(matches!(
        font.with_view_shaping_limit(charged - 1, |font| font
            .measure_styled("A\tA  ", None, &families, pre)),
        Err(TextMeasureError::Limit)
    ));
    // Line-relative tab stops and hanging are modeled in logical order only.
    assert!(matches!(
        font.measure_styled("\u{0644}  \u{0627}", None, &families, pre),
        Err(TextMeasureError::Unsupported)
    ));
}

struct Denied;
impl crate::Broker for Denied {
    fn link_requested(&self, _: &str) -> bool {
        false
    }
    fn open_url(&mut self, _: &str) -> crate::Access {
        crate::Access::Denied
    }
    fn clipboard_write(&mut self) -> crate::Access {
        crate::Access::Denied
    }
}

/// A root `div` (2) holding `children`: text, or a span with one white-space
/// declaration around its text. Returns each node's box.
fn flow(
    root: &[(varta_common::view::CssProperty, &str)],
    children: &[(Option<&str>, &str)],
) -> Result<std::collections::BTreeMap<u32, crate::BoxRect>, crate::LayoutError> {
    use varta_common::view::{CssProperty as P, Tag};
    const APPEND: u32 = 0;
    let mut batch = crate::BatchWriter::new();
    batch.create_element(2, Tag::Div).insert(1, 2, APPEND);
    for (property, value) in [
        (P::FontFamily, "\"Varta Inline Fixture\""),
        (P::FontSize, "20px"),
        (P::LineHeight, "20px"),
    ]
    .into_iter()
    .chain(root.iter().copied())
    {
        batch.set_style(2, property, value);
    }
    let mut id = 3;
    for &(white_space, text) in children {
        let parent = if let Some(value) = white_space {
            batch
                .create_element(id, Tag::Span)
                .insert(2, id, APPEND)
                .set_style(id, P::WhiteSpace, value);
            id += 1;
            id - 1
        } else {
            2
        };
        batch.create_text(id, text).insert(parent, id, APPEND);
        id += 1;
    }
    let mut view = crate::View::default();
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    view.layout_boxes_with_text(200.0, 100.0, &mut measurer(), SIZE)
}

#[test]
fn collapsing_and_preserving_text_nodes_share_one_flow() {
    use varta_common::view::CssProperty as P;
    // A preserving span in a collapsing flow keeps its spaces; the
    // collapsible space after them is not collapsed into them.
    let boxes = flow(
        &[(P::Width, "200px")],
        &[(None, "A "), (Some("pre-wrap"), "  A  "), (None, " A")],
    )
    .unwrap();
    assert_eq!((boxes[&4].x, boxes[&4].width), (2.0 * SPACE, 5.0 * SPACE));
    assert_eq!((boxes[&6].x, boxes[&6].width), (7.0 * SPACE, 2.0 * SPACE));
    // A collapsing span in a preserving flow collapses only its own runs.
    let boxes = flow(
        &[(P::Width, "200px"), (P::WhiteSpace, "pre-wrap")],
        &[
            (None, "A"),
            (Some("normal"), "   A   "),
            (Some("pre-wrap"), "  A"),
        ],
    )
    .unwrap();
    assert_eq!((boxes[&4].x, boxes[&4].width), (SPACE, 3.0 * SPACE));
    assert_eq!((boxes[&6].x, boxes[&6].width), (4.0 * SPACE, 3.0 * SPACE));
}

#[test]
fn native_white_space_refusals_name_their_node() {
    use varta_common::view::CssProperty as P;
    // A plain text node (3) and a rich flow's root (2): right-to-left text
    // the model does not reproduce is refused, not reported as a failure.
    let rtl = "\u{0644}  \u{0627}";
    assert!(matches!(
        flow(&[(P::WhiteSpace, "pre-wrap")], &[(None, rtl)]),
        Err(crate::LayoutError::Unsupported(3))
    ));
    assert!(matches!(
        flow(&[], &[(None, "A"), (Some("pre-wrap"), rtl)]),
        Err(crate::LayoutError::Unsupported(2))
    ));
}

#[test]
fn ellipsis_refuses_text_that_needs_native_white_space() {
    use varta_common::view::CssProperty as P;
    let ellipsis = [
        (P::Width, "40px"),
        (P::WhiteSpace, "nowrap"),
        (P::Overflow, "hidden"),
        (P::TextOverflow, "ellipsis"),
    ];
    assert!(flow(&ellipsis, &[(None, "A A A A A A")]).is_ok());
    assert!(matches!(
        flow(&ellipsis, &[(None, "A A"), (Some("pre-wrap"), "  A A")]),
        Err(crate::LayoutError::Unsupported(2))
    ));
}
