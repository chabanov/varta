use super::*;
use crate::text::{TextMeasurer, TextOptions, TextSource, WhiteSpaceMode};
use parley::{Layout, LineHeight};

fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("../../../tests/fixtures/inline.ttf").to_vec(),
        "Varta Inline Fixture",
    )
    .unwrap()
}
fn options() -> TextOptions {
    TextOptions {
        size: 20.0,
        line_height: Some(LineHeight::Absolute(24.0)),
        weight: 400.0,
        italic: false,
        letter_spacing: 0.0,
        white_space: WhiteSpaceMode::Normal,
        break_anywhere: false,
        source: TextSource::General,
        align: crate::text::TextAlign::Start,
    }
}
fn edge(index: usize, opening: bool, advance: f32) -> TextEdge {
    TextEdge {
        index,
        kind: if opening {
            EdgeKind::Opening
        } else {
            EdgeKind::Closing
        },
        advance,
        owner: 3,
    }
}
fn atom(index: usize, advance: f32) -> TextEdge {
    TextEdge {
        index,
        kind: EdgeKind::Atom,
        advance,
        owner: 4,
    }
}
fn shape(
    f: &mut TextMeasurer,
    text: &str,
    width: f32,
    edges: &[TextEdge],
) -> Result<(crate::TextMetrics, Layout<u32>), TextMeasureError> {
    f.with_view_shaping(|f| {
        f.shape_with_edges(
            text,
            Some(width),
            &["Varta Inline Fixture".into()],
            options(),
            &[],
            edges,
        )
    })
}
fn glyphs(layout: &Layout<u32>) -> Vec<u32> {
    let mut glyphs = Vec::new();
    for line in layout.lines() {
        for run in line.runs() {
            for cluster in run.visual_clusters() {
                glyphs.extend(cluster.glyphs().map(|g| g.id));
            }
        }
    }
    glyphs
}
#[test]
fn decoration_edges_preserve_unbreakable_words_and_intrinsic_widths() {
    // Independent Chrome baseline: A<span padding:0 3px>AAAA</span>A.
    let mut f = font();
    let edges = [edge(1, true, 3.0), edge(5, false, 3.0)];
    for width in [12.0, 24.0, 32.0, 48.0, 80.0] {
        let (m, l) = shape(&mut f, "AAAAAA", width, &edges).unwrap();
        assert_eq!((m.width, m.min_width, m.lines), (50.0, 50.0, 1));
        assert_eq!(l.calculate_content_widths().max, 50.0);
    }
}
#[test]
fn opening_edges_follow_the_next_word_and_ends_stay_with_the_prior_word() {
    // Independent Chrome: A<span padding:0 5px 0 3px>A A A</span>A.
    let mut f = font();
    let edges = [edge(1, true, 3.0), edge(6, false, 5.0)];
    for (width, lines) in [(12.0, 3), (24.0, 3), (32.0, 3), (48.0, 2), (80.0, 1)] {
        let (m, l) = shape(&mut f, "AA A AA", width, &edges).unwrap();
        assert_eq!(
            (m.min_width, l.calculate_content_widths().max, m.lines),
            (21.0, 64.0, lines),
            "{width}"
        );
        assert!(l.lines().all(|line| !line.text_range().is_empty()));
    }
}
#[test]
fn negative_margins_participate_in_width_and_last_cluster_fitting() {
    let mut f = font();
    let edges = [edge(1, true, -3.0), edge(6, false, -3.0)];
    for (width, lines) in [(12.0, 3), (24.0, 3), (32.0, 2), (48.0, 2), (80.0, 1)] {
        let (m, l) = shape(&mut f, "AA A AA", width, &edges).unwrap();
        assert_eq!(
            (m.min_width, l.calculate_content_widths().max, m.lines),
            (13.0, 50.0, lines),
            "{width}"
        );
    }
    let (m, _) = shape(&mut f, "A A", 21.0, &[edge(3, false, -3.0)]).unwrap();
    assert_eq!(
        (m.width, m.lines),
        (21.0, 1),
        "closing negative margin saves the last word"
    );
    let (m, _) = shape(&mut f, "A A", 24.0, &[edge(3, false, 4.0)]).unwrap();
    assert_eq!(
        (m.width, m.lines),
        (12.0, 2),
        "closing width moves with its word"
    );
}
#[test]
fn zero_edges_keep_ligatures_and_real_separating_edges_split_shaping() {
    let mut f = font();
    let (a, plain) = shape(&mut f, "AA", 12.0, &[]).unwrap();
    let (b, separated) = shape(&mut f, "AA", 12.0, &[edge(1, true, 3.0)]).unwrap();
    assert_eq!((a.width, a.min_width, a.lines), (14.0, 14.0, 1));
    assert_eq!((b.width, b.min_width, b.lines), (19.0, 19.0, 1));
    assert_eq!(glyphs(&plain).len(), 1);
    assert_eq!(glyphs(&separated).len(), 2);
    let (a, joined) = shape(&mut f, "لا", 12.0, &[]).unwrap();
    let (b, separated) = shape(&mut f, "لا", 12.0, &[edge(2, true, 3.0)]).unwrap();
    assert_eq!((a.width, b.width), (16.0, 19.0));
    assert_ne!(glyphs(&joined), glyphs(&separated));
}
#[test]
fn empty_inline_edges_keep_source_ranges_and_do_not_become_atomic_breaks() {
    let mut f = font();
    let edges = [edge(1, true, 5.0), edge(1, false, 5.0)];
    for width in [12.0, 24.0, 32.0, 80.0] {
        let (m, l) = shape(&mut f, "AA", width, &edges).unwrap();
        assert_eq!((m.width, m.min_width, m.lines), (26.0, 26.0, 1));
        assert_eq!(l.lines().next().unwrap().text_range(), 0..2);
    }
    let (m, l) = shape(&mut f, "", 1.0, &[edge(0, true, 5.0), edge(0, false, 5.0)]).unwrap();
    assert_eq!((m.width, m.min_width, m.lines), (10.0, 10.0, 1));
    assert_eq!(l.lines().next().unwrap().text_range(), 0..0);
}
#[test]
fn changed_edges_invalidate_shape_memos_and_hits_retain_full_operation_charges() {
    let mut f = font();
    f.cache_shapes();
    let edges = [edge(1, true, 3.0)];
    let expected = shape(&mut f, "AA", 12.0, &edges).unwrap().0;
    let used = f.last_view_shaping_operations();
    let builds = f.shaping_builds;
    assert_eq!(shape(&mut f, "AA", 12.0, &edges).unwrap().0, expected);
    assert_eq!(f.shaping_builds, builds);
    assert_eq!(f.last_view_shaping_operations(), used);
    let changed = shape(&mut f, "AA", 12.0, &[edge(1, true, 5.0)]).unwrap().0;
    assert_eq!(changed.width, 21.0);
    assert_eq!(f.shaping_builds, builds + 1);
    assert!(
        f.with_view_shaping_limit(used - 1, |f| f.shape_with_edges(
            "AA",
            Some(12.0),
            &["Varta Inline Fixture".into()],
            options(),
            &[],
            &edges
        ))
        .is_err()
    );
}
#[test]
fn invalid_programs_fail_before_shaping_and_output_slots_share_existing_limits() {
    let mut f = font();
    let builds = f.shaping_builds;
    for edges in [
        vec![edge(1, true, 3.0)],
        vec![edge(4, false, f32::NAN)],
        vec![edge(4, false, f32::INFINITY)],
        vec![edge(4, false, 1_000_001.0)],
        vec![edge(4, false, 1.0), edge(0, true, 1.0)],
        vec![edge(0, true, 1.0); MAX_EDGES + 1],
    ] {
        assert!(shape(&mut f, "لا", 12.0, &edges).is_err());
    }
    assert_eq!(f.shaping_builds, builds);
    let edges = [edge(0, true, 1.0), edge(1, false, 1.0)];
    f.layout.set_shaping_limits(
        2,
        super::super::MAX_SHAPING_BUFFER_SLOTS,
        super::super::MAX_SHAPING_OPERATIONS,
        super::super::MAX_CACHED_SHAPING_SLOTS,
    );
    assert!(shape(&mut f, "A", 12.0, &edges).is_err());
    f.layout.set_shaping_limits(
        3,
        super::super::MAX_SHAPING_BUFFER_SLOTS,
        super::super::MAX_SHAPING_OPERATIONS,
        super::super::MAX_CACHED_SHAPING_SLOTS,
    );
    assert_eq!(shape(&mut f, "A", 12.0, &edges).unwrap().0.width, 10.0);
}
/// Each line's advance, from its clusters and inline boxes.
fn line_widths(layout: &Layout<u32>) -> Vec<f32> {
    layout.lines().map(|line| line.metrics().advance).collect()
}
#[test]
fn atomic_inlines_break_on_both_sides_and_keep_their_adjacent_edges() {
    // CSS Text 3 §5.1: soft wrap opportunities before and after an atomic
    // inline, even between letters (A<img>A, ligature split).
    let mut f = font();
    let atoms = [atom(1, 10.0)];
    let (m, l) = shape(&mut f, "AA", 80.0, &atoms).unwrap();
    assert_eq!((m.width, m.min_width, m.lines), (26.0, 10.0, 1));
    let (m, l2) = shape(&mut f, "AA", 20.0, &atoms).unwrap();
    assert_eq!((m.lines, line_widths(&l2)), (2, vec![18.0, 8.0]));
    assert_eq!(glyphs(&l).len(), 2);
    // A<span padding:0 3px><img></span>A: the opening edge moves with the
    // image to the next line and the closing edge stays with it.
    let program = [edge(1, true, 3.0), atom(1, 10.0), edge(1, false, 3.0)];
    let (m, l) = shape(&mut f, "AA", 22.0, &program).unwrap();
    assert_eq!(
        (m.min_width, m.lines, line_widths(&l)),
        (16.0, 3, vec![8.0, 16.0, 8.0])
    );
    let (m, l) = shape(&mut f, "AA", 30.0, &program).unwrap();
    assert_eq!((m.lines, line_widths(&l)), (2, vec![24.0, 8.0]));
    // A line holding only an image keeps the image's source position.
    let (m, l) = shape(&mut f, "AA", 9.0, &atoms).unwrap();
    assert_eq!(m.lines, 3);
    assert_eq!(l.lines().nth(1).unwrap().text_range(), 1..1);
    // An image wider than the line overflows it alone.
    let (m, l) = shape(&mut f, "AA", 9.0, &[atom(1, 30.0)]).unwrap();
    assert_eq!((m.min_width, line_widths(&l)), (30.0, vec![8.0, 30.0, 8.0]));
}
