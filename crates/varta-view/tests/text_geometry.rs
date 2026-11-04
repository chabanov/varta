use varta_common::view::{Attribute as A, CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Input, Target, TextMeasurer, View};

struct Deny;
impl Broker for Deny {
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
fn commit(view: &mut View, b: &B) {
    view.commit(&b.finish(), false, &mut Deny).unwrap();
}
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/password.ttf").to_vec(),
        "Varta Password Fixture",
    )
    .unwrap()
}
fn field(tag: Tag, value: &str) -> View {
    let mut b = B::new();
    b.stylesheet(1, ".field {display:block; width:60px; height:60px; min-width:0; min-height:0; padding:4px; border:1px solid #000; border-radius:0; outline:none; background-color:#fff; color:#000; font-family:\"Varta Password Fixture\"; font-size:20px; font-weight:400; font-style:normal; line-height:20px}")
        .create_element(2, tag).set_class(2,"field");
    if matches!(tag, Tag::Input | Tag::Textarea) {
        b.set_value(2, value, 0);
    }
    b.insert(1, 2, 0);
    let mut v = View::default();
    commit(&mut v, &b);
    v
}

#[test]
fn geometry_tracks_soft_lines_bidi_source_ranges_and_physical_scale() {
    let value = "AA אבג BBBB e\u{301}😀";
    let view = field(Tag::Textarea, value);
    let semantics = view.accessibility_tree().unwrap();
    let source = &semantics.nodes[&2];
    let revision = &source.text.as_ref().unwrap().revision;
    let mut previous = None;
    for scale in [1.0, 2.0] {
        let image = view
            .rasterize_scaled(
                (140.0 * scale) as u32,
                (120.0 * scale) as u32,
                scale,
                Some((&mut font(), 20.0)),
            )
            .unwrap();
        let geometry = image
            .field_text_geometry(&source.identity, revision)
            .unwrap();
        assert!(geometry.lines.len() > 1);
        assert!(geometry.clusters.iter().any(|cluster| cluster.rtl));
        let mut ranges = geometry
            .clusters
            .iter()
            .map(|c| c.source.clone())
            .collect::<Vec<_>>();
        ranges.sort_by_key(|r| r.start);
        assert_eq!(ranges.first().unwrap().start, 0);
        assert_eq!(ranges.last().unwrap().end, value.len());
        assert!(ranges.windows(2).all(|pair| pair[0].end == pair[1].start));
        assert!(
            ranges
                .iter()
                .all(|r| value.is_char_boundary(r.start) && value.is_char_boundary(r.end))
        );
        for line in geometry.lines {
            let clusters = &geometry.clusters[line.clusters.clone()];
            assert!(clusters.iter().all(|c| c.bounds.y == line.bounds.y));
            assert!(
                clusters
                    .windows(2)
                    .all(|pair| pair[0].bounds.x <= pair[1].bounds.x)
            );
            for cluster in clusters
                .iter()
                .filter(|c| !c.line_break && c.bounds.width > 0.0)
            {
                let y = cluster.bounds.y + cluster.bounds.height / 2.0;
                let x = cluster.bounds.x + cluster.bounds.width / 4.0;
                assert_eq!(
                    image.text_position_at(2, x, y),
                    Some(if cluster.rtl {
                        cluster.source.end
                    } else {
                        cluster.source.start
                    })
                );
            }
        }
        if let Some((old_clip, old)) = previous.take() {
            let old_clip: varta_view::BoxRect = old_clip;
            assert_eq!(geometry.clip.x, old_clip.x * 2.0);
            assert_eq!(geometry.clip.height, old_clip.height * 2.0);
            for (new, old) in geometry.clusters.iter().zip(old) {
                let old: varta_view::FieldTextCluster = old;
                assert_eq!(new.source, old.source);
                assert_eq!(new.bounds.x, old.bounds.x * 2.0);
                assert_eq!(new.bounds.width, old.bounds.width * 2.0);
            }
        } else {
            previous = Some((geometry.clip, geometry.clusters.to_vec()));
        }
    }
}

#[test]
fn geometry_rejects_changed_or_replaced_content_and_transient_composition() {
    let mut view = field(Tag::Input, "abc");
    let source = view.accessibility_tree().unwrap().nodes[&2].clone();
    let revision = &source.text.as_ref().unwrap().revision;
    let before = view
        .rasterize_with_text(140, 100, &mut font(), 20.0)
        .unwrap();
    assert!(
        before
            .field_text_geometry(&source.identity, revision)
            .is_some()
    );
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 0,
            focus: 1,
            activate: false,
        },
        &mut Deny,
    )
    .unwrap();
    let selected = view
        .rasterize_with_text(140, 100, &mut font(), 20.0)
        .unwrap();
    assert!(
        selected
            .field_text_geometry(&source.identity, revision)
            .is_some()
    );
    view.input(
        &Input::ImePreedit {
            target: Target::Node(2),
            text: "e".into(),
            cursor: None,
        },
        &mut Deny,
    )
    .unwrap();
    let composing = view
        .rasterize_with_text(140, 100, &mut font(), 20.0)
        .unwrap();
    assert!(
        composing
            .field_text_geometry(&source.identity, revision)
            .is_none()
    );
    let mut b = B::new();
    b.set_value(2, "changed", 0);
    commit(&mut view, &b);
    let changed = view.accessibility_tree().unwrap().nodes[&2].clone();
    assert!(
        before
            .field_text_geometry(&changed.identity, &changed.text.as_ref().unwrap().revision)
            .is_none()
    );
    let mut b = B::new();
    b.remove(2)
        .create_element(2, Tag::Input)
        .set_class(2, "field")
        .set_value(2, "abc", 0)
        .insert(1, 2, 0);
    commit(&mut view, &b);
    let replacement = view.accessibility_tree().unwrap().nodes[&2].clone();
    assert!(
        before
            .field_text_geometry(
                &replacement.identity,
                &replacement.text.as_ref().unwrap().revision
            )
            .is_none()
    );
}

#[test]
fn crlf_and_trailing_empty_line_keep_byte_spans_and_end_before_the_pair() {
    let mut view = field(Tag::Textarea, "AA\r\nB\n");
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 0,
            focus: 0,
            activate: false,
        },
        &mut Deny,
    )
    .unwrap();
    let source = view.accessibility_tree().unwrap().nodes[&2].clone();
    let image = view
        .rasterize_with_text(140, 100, &mut font(), 20.0)
        .unwrap();
    let g = image
        .field_text_geometry(&source.identity, &source.text.as_ref().unwrap().revision)
        .unwrap();
    assert_eq!(g.lines.len(), 3);
    assert_eq!(g.lines[0].source, 0..4);
    assert_eq!(g.lines[0].content_end, 2);
    assert_eq!(g.lines[1].source, 4..6);
    assert_eq!(g.lines[2].source, 6..6);
    assert!(
        g.clusters
            .iter()
            .any(|c| c.line_break && c.source == (2..4))
    );
    assert_eq!(image.text_navigation(2, "End").unwrap().position, 2);
    assert_eq!(
        image.text_position_at(2, 100.0, g.lines[0].bounds.y + 1.0),
        Some(2)
    );
}

#[test]
fn crlf_content_sizing_and_pixels_match_lf_without_rewriting_guest_values() {
    let mut crlf = field(Tag::Textarea, "AA\r\nB\r\n");
    let mut lf = field(Tag::Textarea, "AA\nB\n");
    for view in [&mut crlf, &mut lf] {
        let mut b = B::new();
        b.set_style(2, P::FieldSizing, "content")
            .set_style(2, P::Height, "auto");
        commit(view, &b);
    }
    let before = crlf.snapshot();
    assert_eq!(
        crlf.layout_geometry_with_text(140.0, 140.0, &mut font(), 20.0)
            .unwrap(),
        lf.layout_geometry_with_text(140.0, 140.0, &mut font(), 20.0)
            .unwrap()
    );
    assert_eq!(
        crlf.rasterize_with_text(140, 140, &mut font(), 20.0)
            .unwrap()
            .pixels,
        lf.rasterize_with_text(140, 140, &mut font(), 20.0)
            .unwrap()
            .pixels
    );
    assert_eq!(crlf.snapshot(), before);
    assert_eq!(before["tree"]["children"][0]["value"], "AA\r\nB\r\n");
}

#[test]
fn password_geometry_matches_masks_even_for_combining_and_joined_emoji() {
    let value = "Ae\u{301}👨‍👩‍👧B";
    let mut view = field(Tag::Input, value);
    let mut b = B::new();
    b.set_attribute(2, A::Type, "password");
    commit(&mut view, &b);
    let source = view.accessibility_tree().unwrap().nodes[&2].clone();
    assert_eq!(source.value.as_deref(), Some("••••"));
    let text = source.text.as_ref().unwrap();
    let image = view
        .rasterize_with_text(140, 100, &mut font(), 20.0)
        .unwrap();
    let g = image
        .field_text_geometry(&source.identity, &text.revision)
        .unwrap();
    assert_eq!(g.clusters.len(), 4);
    for (cluster, pair) in g.clusters.iter().zip(text.source_offsets.windows(2)) {
        assert_eq!(cluster.source, pair[0]..pair[1]);
    }
    let mut b = B::new();
    b.set_style(2, P::Width, "120px");
    commit(&mut view, &b);
    let wider = view
        .rasterize_with_text(150, 100, &mut font(), 20.0)
        .unwrap();
    assert!(
        wider
            .field_text_geometry(&source.identity, &text.revision)
            .is_some()
    );
}

#[test]
fn curved_ancestor_visibility_preserves_full_advances_and_clips_ime_in_physical_pixels() {
    let mut view = field(Tag::Textarea, "AAAA");
    let mut batch = B::new();
    batch
        .create_element(3, Tag::Div)
        .set_style(3, P::Display, "block")
        .set_style(3, P::Width, "80px")
        .set_style(3, P::Height, "80px")
        .set_style(3, P::Overflow, "hidden")
        .set_style(3, P::BorderRadius, "50%")
        .insert(1, 3, 0)
        .insert(3, 2, 0)
        .set_style(2, P::Width, "80px")
        .set_style(2, P::Height, "80px")
        .set_style(2, P::Padding, "0")
        .set_style(2, P::Border, "0 solid #000");
    commit(&mut view, &batch);
    view.input(
        &Input::TextSelect {
            target: Target::Node(2),
            anchor: 0,
            focus: 0,
            activate: false,
        },
        &mut Deny,
    )
    .unwrap();
    let source = view.accessibility_tree().unwrap();
    let field = &source.nodes[&2];
    let revision = &field.text.as_ref().unwrap().revision;
    let before = view.snapshot();
    for scale in [1.0, 2.0] {
        let image = view
            .rasterize_scaled(
                (140.0 * scale) as u32,
                (120.0 * scale) as u32,
                scale,
                Some((&mut font(), 20.0)),
            )
            .unwrap();
        let geometry = image
            .field_text_geometry(&field.identity, revision)
            .unwrap();
        let mut work = source.observer_work();
        let full = geometry.clusters[0].bounds;
        let visible = geometry.visible_bounds(full, &mut work).unwrap().unwrap();
        assert_eq!(full.x, 0.0);
        assert!(visible.x > full.x && visible.y > full.y);
        assert!(visible.width < full.width && visible.height < full.height);
        assert!(
            geometry
                .visible_bounds(
                    varta_view::BoxRect {
                        x: 0.0,
                        y: 0.0,
                        width: 2.0 * scale,
                        height: 2.0 * scale
                    },
                    &mut work
                )
                .unwrap()
                .is_none()
        );
        assert!(
            image.ime_cursor_area().is_none(),
            "left edge caret is fully cut by the curved ancestor"
        );
        assert_eq!(geometry.clusters[0].bounds, full);
    }
    assert_eq!(view.snapshot(), before);
}

#[test]
fn rounded_native_controls_share_content_clips_across_reference_retained_and_vector_frames() {
    for (tag, kind) in [
        (Tag::Input, "text"),
        (Tag::Textarea, ""),
        (Tag::Input, "checkbox"),
        (Tag::Input, "radio"),
        (Tag::Select, ""),
        (Tag::Button, ""),
    ] {
        let mut view = field(tag, "AAAA");
        let mut batch = B::new();
        batch
            .set_style(1, P::BackgroundColor, "#00ff00")
            .set_style(2, P::BorderRadius, "50%");
        if !kind.is_empty() {
            batch.set_attribute(2, A::Type, kind);
        }
        if matches!(kind, "checkbox" | "radio") {
            batch.set_checked(2, 1, 0);
        }
        if tag == Tag::Button {
            batch
                .set_style(2, P::Display, "flex")
                .set_style(2, P::JustifyContent, "flex-start")
                .set_style(2, P::AlignItems, "flex-start")
                .create_text(3, "A")
                .insert(2, 3, 0);
        }
        if tag == Tag::Select {
            batch
                .create_element(3, Tag::Option)
                .create_text(4, "A")
                .insert(3, 4, 0)
                .insert(2, 3, 0);
        }
        commit(&mut view, &batch);
        let before = view.snapshot();
        let mut renderer = varta_view::Renderer::new(Some(font()), 20.0).unwrap();
        for scale in [1.0, 2.0] {
            let (width, height) = ((140.0 * scale) as u32, (120.0 * scale) as u32);
            let expected = view
                .rasterize_scaled(width, height, scale, Some((&mut font(), 20.0)))
                .unwrap_or_else(|error| panic!("{tag:?}/{kind} at {scale}: {error:?}"));
            assert_eq!(&expected.pixels[0..4], &[0, 255, 0, 255]);
            assert_eq!(expected.hit_test(scale, scale), None);
            assert_eq!(
                renderer.render(&view, width, height, scale).unwrap(),
                expected
            );
            let varta_view::PreparedFrame::Vector { metadata, scene } =
                renderer.prepare(&view, width, height, scale).unwrap()
            else {
                panic!("bounded rounded control vector scene")
            };
            assert_eq!(scene.rasterize(metadata).unwrap(), expected);
            if tag == Tag::Textarea {
                let source = view.accessibility_tree().unwrap();
                let source = &source.nodes[&2];
                let geometry = expected
                    .field_text_geometry(&source.identity, &source.text.as_ref().unwrap().revision)
                    .unwrap();
                let full = geometry.clusters[0].bounds;
                let visible = geometry
                    .visible_bounds(full, &mut varta_view::AccessibilityWork::default())
                    .unwrap()
                    .unwrap();
                assert!(visible.x > full.x && visible.y > full.y);
            }
        }
        assert_eq!(view.snapshot(), before);
    }
}
