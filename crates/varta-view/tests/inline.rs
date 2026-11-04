use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, TextMeasurer, View};

struct Denied;
impl Broker for Denied {
    fn link_requested(&self, _: &str) -> bool {
        false
    }
    fn open_url(&mut self, _: &str) -> Access {
        Access::Denied
    }
    fn clipboard_write(&mut self) -> Access {
        Access::Denied
    }
}
fn base(width: &str) -> B {
    let mut b = B::new();
    b.create_element(2, Tag::Div).insert(1, 2, 0);
    for (p, v) in [
        (P::Width, width),
        (P::FontFamily, "\"Varta Fixture 400\""),
        (P::FontSize, "20px"),
        (P::LineHeight, "20px"),
        (P::Color, "#00f"),
    ] {
        b.set_style(2, p, v);
    }
    b
}
fn view(b: B) -> View {
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    v
}
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap()
}
fn plain(text: &str, width: &str) -> View {
    let mut b = base(width);
    b.create_text(3, text).insert(2, 3, 0);
    view(b)
}

#[test]
fn inline_spans_share_wrapping_and_hits_use_fragments() {
    let mut b = base("24px");
    b.create_text(3, "A ")
        .create_element(4, Tag::Span)
        .create_text(5, "A A")
        .create_text(6, " A")
        .insert(2, 3, 0)
        .insert(2, 4, 0)
        .insert(4, 5, 0)
        .insert(2, 6, 0);
    let v = view(b);
    let snapshot = v.snapshot();
    let mut f = font();
    let geometry = v
        .layout_boxes_with_text(100.0, 100.0, &mut f, 20.0)
        .unwrap();
    assert_eq!(geometry[&2].height, 40.0);
    assert_eq!((geometry[&4].width, geometry[&4].height), (24.0, 40.0));
    let image = v.rasterize_with_text(100, 100, &mut f, 20.0).unwrap();
    assert_eq!(
        image.pixels,
        plain("A A A A", "24px")
            .rasterize_with_text(100, 100, &mut f, 20.0)
            .unwrap()
            .pixels
    );
    assert_eq!(image.hit_test(17.0, 5.0), Some(4));
    assert_eq!(image.hit_test(2.0, 25.0), Some(4));
    assert_eq!(
        image.hit_test(12.0, 25.0),
        Some(2),
        "a wrapped span's bounding-box gap is not a hit"
    );
    assert_eq!(v.snapshot(), snapshot);
}

#[test]
fn inline_whitespace_collapses_across_nodes_and_hidden_subtrees() {
    let mut b = base("24px");
    b.create_text(3, " A ")
        .create_element(4, Tag::Span)
        .create_text(5, " \tA")
        .create_element(6, Tag::Span)
        .create_text(7, "A")
        .create_element(8, Tag::Span)
        .set_style(8, P::Display, "none")
        .create_text(9, "hidden")
        .insert(2, 3, 0)
        .insert(2, 4, 0)
        .insert(4, 5, 0)
        .insert(2, 8, 0)
        .insert(8, 9, 0)
        .insert(2, 6, 0)
        .insert(6, 7, 0);
    let v = view(b);
    let mut f = font();
    let geometry = v
        .layout_boxes_with_text(100.0, 100.0, &mut f, 20.0)
        .unwrap();
    assert_eq!((geometry[&8].width, geometry[&9].height), (0.0, 0.0));
    assert_eq!(
        v.rasterize_with_text(100, 100, &mut f, 20.0)
            .unwrap()
            .pixels,
        plain("A AA", "24px")
            .rasterize_with_text(100, 100, &mut f, 20.0)
            .unwrap()
            .pixels
    );
}

#[test]
fn inline_colors_and_parent_strut_preserve_baselines() {
    let mut b = base("100px");
    b.set_style(2, P::LineHeight, "30px")
        .create_element(3, Tag::Span)
        .set_style(3, P::FontSize, "10px")
        .set_style(3, P::LineHeight, "10px")
        .set_style(3, P::Color, "#f00")
        .create_text(4, "A")
        .insert(2, 3, 0)
        .insert(3, 4, 0);
    let v = view(b);
    let mut f = font();
    let g = v
        .layout_boxes_with_text(100.0, 100.0, &mut f, 20.0)
        .unwrap();
    assert_eq!(
        g[&2].height, 30.0,
        "the parent contributes a line strut even without its own glyphs"
    );
    assert_eq!((g[&3].y, g[&3].width, g[&3].height), (13.0, 4.0, 10.0));
    let image = v.rasterize_with_text(100, 100, &mut f, 20.0).unwrap();
    assert!(
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p == &[255, 0, 0, 255])
    );
    assert!(
        !image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p == &[0, 0, 255, 255])
    );
}

#[test]
fn inline_breaks_and_flex_item_blockification_keep_css_flow() {
    let mut b = base("100px");
    b.create_text(3, "A")
        .create_element(4, Tag::Span)
        .create_text(5, "A")
        .create_element(6, Tag::Br)
        .create_text(7, "A")
        .insert(2, 3, 0)
        .insert(2, 4, 0)
        .insert(4, 5, 0)
        .insert(2, 6, 0)
        .insert(2, 7, 0);
    let v = view(b);
    let mut f = font();
    let mut expected = base("100px");
    expected
        .set_style(2, P::WhiteSpace, "pre-wrap")
        .create_text(3, "AA\nA")
        .insert(2, 3, 0);
    assert_eq!(
        v.rasterize_with_text(100, 100, &mut f, 20.0)
            .unwrap()
            .pixels,
        view(expected)
            .rasterize_with_text(100, 100, &mut f, 20.0)
            .unwrap()
            .pixels
    );
    let mut b = base("100px");
    b.set_style(2, P::Display, "flex")
        .set_style(2, P::Gap, "4px")
        .create_element(3, Tag::Span)
        .set_style(3, P::Display, "inline")
        .create_text(4, "AA")
        .insert(2, 3, 0)
        .insert(3, 4, 0)
        .create_element(5, Tag::Span)
        .create_text(6, "A")
        .insert(2, 5, 0)
        .insert(5, 6, 0);
    let g = view(b)
        .layout_boxes_with_text(100.0, 100.0, &mut f, 20.0)
        .unwrap();
    assert_eq!(
        (g[&3].x, g[&3].width, g[&5].x, g[&5].width),
        (0.0, 16.0, 20.0, 8.0)
    );
}

fn joined(left: &str, right: &str, different_color: bool) -> View {
    let mut b = base("100px");
    b.set_style(2, P::FontFamily, "\"Varta Inline Fixture\"")
        .create_text(3, left)
        .create_element(4, Tag::Span)
        .create_text(5, right)
        .insert(2, 3, 0)
        .insert(2, 4, 0)
        .insert(4, 5, 0);
    if different_color {
        b.set_style(4, P::Color, "#f00");
    }
    view(b)
}
fn inline_font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/inline.ttf").to_vec(),
        "Varta Inline Fixture",
    )
    .unwrap()
}

#[test]
fn inline_boundaries_preserve_ligatures_combining_marks_and_arabic_joining() {
    let mut f = inline_font();
    for (left, right) in [("A", "A"), ("A", "\u{035d}"), ("ل", "ا")] {
        let v = joined(left, right, false);
        let mut b = base("100px");
        b.set_style(2, P::FontFamily, "\"Varta Inline Fixture\"")
            .create_text(3, &format!("{left}{right}"))
            .insert(2, 3, 0);
        assert_eq!(
            v.rasterize_with_text(100, 100, &mut f, 20.0)
                .unwrap()
                .pixels,
            view(b)
                .rasterize_with_text(100, 100, &mut f, 20.0)
                .unwrap()
                .pixels,
            "shaping context across {left:?}/{right:?}"
        );
    }
    assert_eq!(f.measure("AA", 20.0, None).unwrap().width, 14.0);
    // The fixture's joining forms have distinct outlines; this must not pass
    // merely because the font has no Arabic contextual substitutions.
    assert_ne!(
        f.measure("لا", 20.0, None).unwrap(),
        f.measure("ل ا", 20.0, None).unwrap()
    );
}

#[test]
fn colored_ligature_uses_first_source_node_for_ink_and_fragments() {
    let v = joined("A", "A", true);
    let mut f = inline_font();
    let g = v
        .layout_boxes_with_text(100.0, 100.0, &mut f, 20.0)
        .unwrap();
    assert_eq!((g[&4].x, g[&4].width), (14.0, 0.0));
    let image = v.rasterize_with_text(100, 100, &mut f, 20.0).unwrap();
    let sample = |x: usize, y: usize| &image.pixels[(y * 100 + x) * 4..(y * 100 + x + 1) * 4];
    assert_eq!(sample(3, 6), &[0, 0, 255, 255]);
    assert_eq!(sample(10, 6), &[0, 0, 255, 255]);
    assert_eq!(image.hit_test(3.0, 6.0), Some(2));
    assert_eq!(image.hit_test(10.0, 6.0), Some(2));
    let scaled = v
        .rasterize_scaled(200, 200, 2.0, Some((&mut f, 20.0)))
        .unwrap();
    assert_eq!(
        &scaled.pixels[(12 * 200 + 20) * 4..(12 * 200 + 21) * 4],
        &[0, 0, 255, 255]
    );
}

#[test]
fn inline_semantic_font_faces_and_explicit_overrides_are_resolved() {
    use varta_view::TextFont;
    let mut f = TextMeasurer::new_with_fonts(
        [
            include_bytes!("fixtures/advance-400.ttf").as_slice(),
            include_bytes!("fixtures/bold-900.ttf").as_slice(),
            include_bytes!("fixtures/italic-900.ttf").as_slice(),
        ]
        .into_iter()
        .map(|data| TextFont {
            data: data.to_vec(),
            family: "Varta Fixture 400".into(),
            aliases: vec!["monospace".into()],
        })
        .collect(),
    )
    .unwrap();
    let mut b = base("100px");
    for (id, tag) in [
        (3, Tag::Strong),
        (5, Tag::Em),
        (7, Tag::Code),
        (9, Tag::Strong),
    ] {
        b.create_element(id, tag)
            .create_text(id + 1, "A")
            .insert(2, id, 0)
            .insert(id, id + 1, 0);
    }
    b.set_style(9, P::FontWeight, "400");
    let v = view(b);
    let g = v
        .layout_boxes_with_text(100.0, 100.0, &mut f, 20.0)
        .unwrap();
    assert_eq!(
        (g[&3].width, g[&5].width, g[&7].width, g[&9].width),
        (18.0, 18.0, 8.0, 8.0)
    );
    v.rasterize_with_text(100, 100, &mut f, 20.0).unwrap();
}

#[test]
fn overflowing_bare_text_keeps_its_container_hit_target() {
    let mut b = base("24px");
    b.set_style(2, P::WhiteSpace, "nowrap")
        .create_text(3, "A ")
        .insert(2, 3, 0)
        .create_element(4, Tag::Span)
        .create_text(5, "A A")
        .insert(2, 4, 0)
        .insert(4, 5, 0)
        .create_text(6, " A")
        .insert(2, 6, 0);
    let image = view(b)
        .rasterize_with_text(100, 100, &mut font(), 20.0)
        .unwrap();
    assert_eq!(image.hit_test(43.0, 5.0), Some(2));
    assert_eq!(image.hit_test(30.0, 5.0), Some(4));
}

#[test]
fn inline_scroll_fragments_move_and_clip_without_mutating_during_paint() {
    use varta_common::view::Command;
    let mut b = base("24px");
    b.set_style(2, P::Height, "20px")
        .set_style(2, P::Overflow, "hidden")
        .create_text(3, "A ")
        .insert(2, 3, 0)
        .create_element(4, Tag::Span)
        .create_text(5, "A A")
        .insert(2, 4, 0)
        .insert(4, 5, 0)
        .create_text(6, " A")
        .insert(2, 6, 0);
    let mut v = view(b);
    let mut f = font();
    let mut command = B::new();
    command.command(2, Command::ScrollTo, 20.0);
    v.commit_with_layout(
        &command.finish(),
        false,
        &mut Denied,
        100.0,
        100.0,
        Some((&mut f, 20.0)),
    )
    .unwrap();
    let snapshot = v.snapshot();
    let g = v
        .layout_boxes_with_text(100.0, 100.0, &mut f, 20.0)
        .unwrap();
    assert_eq!(g[&4].y, -20.0);
    let frame = v.rasterize_with_text(100, 100, &mut f, 20.0).unwrap();
    assert_eq!(frame.hit_test(2.0, 5.0), Some(4));
    assert_eq!(frame.hit_test(17.0, 5.0), Some(2));
    assert_eq!(frame.hit_test(2.0, 25.0), None);
    assert_eq!(v.snapshot(), snapshot);
}

#[test]
fn empty_inline_anchors_follow_ligatures_and_contribute_line_metrics() {
    let mut b = base("100px");
    b.set_style(2, P::FontFamily, "\"Varta Inline Fixture\"")
        .create_text(3, "A")
        .insert(2, 3, 0)
        .create_element(4, Tag::Span)
        .insert(2, 4, 0)
        .create_text(5, "A")
        .insert(2, 5, 0);
    let g = view(b)
        .layout_boxes_with_text(100.0, 100.0, &mut inline_font(), 20.0)
        .unwrap();
    assert_eq!((g[&4].x, g[&4].width, g[&4].height), (14.0, 0.0, 20.0));
    let mut b = base("100px");
    b.create_element(3, Tag::Span)
        .set_style(3, P::LineHeight, "40px")
        .insert(2, 3, 0)
        .create_text(4, "A")
        .insert(2, 4, 0);
    let g = view(b)
        .layout_boxes_with_text(100.0, 100.0, &mut font(), 20.0)
        .unwrap();
    assert_eq!((g[&2].height, g[&3].y, g[&3].height), (40.0, 10.0, 20.0));
}

#[test]
fn unsupported_inline_box_styles_fail_without_changing_the_view() {
    for (p, value) in [(P::Padding, "1px"), (P::Border, "1px solid #f00")] {
        let mut b = base("100px");
        b.create_element(3, Tag::Span)
            .insert(2, 3, 0)
            .set_style(3, p, value)
            .create_text(4, "A")
            .insert(3, 4, 0);
        let v = view(b);
        let snapshot = v.snapshot();
        assert!(
            v.rasterize_with_text(100, 100, &mut font(), 20.0).is_err(),
            "{p:?}: {value}"
        );
        assert_eq!(v.snapshot(), snapshot);
    }
}
