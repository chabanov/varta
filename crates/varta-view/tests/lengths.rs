//! Native length units resolve against an explicit host reference font.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter, Broker, Environment, LayoutError, TextMeasurer, View};

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

fn boxes(parent: &[(P, &str)], child: &[(P, &str)]) -> View {
    let mut view = View::default();
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .create_element(3, Tag::Div)
        .insert(1, 2, 0)
        .insert(2, 3, 0);
    for (id, styles) in [(2, parent), (3, child)] {
        for &(property, value) in styles {
            batch.set_style(id, property, value);
        }
    }
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    view
}

#[test]
fn rem_lengths_share_box_and_paint_geometry() {
    let rem = boxes(
        &[
            (P::Display, "flex"),
            (P::Width, "12rem"),
            (P::Height, "4rem"),
            (P::Padding, ".5rem .25rem"),
            (P::Margin, ".25rem .5rem"),
            (P::Border, ".125rem solid #00f"),
            (P::Outline, ".125rem solid #f00"),
            (P::OutlineOffset, "-.0625rem"),
            (P::Gap, ".25rem"),
            (P::BackgroundColor, "#fff"),
        ],
        &[
            (P::Width, "1rem"),
            (P::Height, "1rem"),
            (P::FlexBasis, "2rem"),
            (P::MinWidth, "1rem"),
            (P::MaxWidth, "3rem"),
            (P::BackgroundColor, "#080"),
        ],
    );
    let px = boxes(
        &[
            (P::Display, "flex"),
            (P::Width, "192px"),
            (P::Height, "64px"),
            (P::Padding, "8px 4px"),
            (P::Margin, "4px 8px"),
            (P::Border, "2px solid #00f"),
            (P::Outline, "2px solid #f00"),
            (P::OutlineOffset, "-1px"),
            (P::Gap, "4px"),
            (P::BackgroundColor, "#fff"),
        ],
        &[
            (P::Width, "16px"),
            (P::Height, "16px"),
            (P::FlexBasis, "32px"),
            (P::MinWidth, "16px"),
            (P::MaxWidth, "48px"),
            (P::BackgroundColor, "#080"),
        ],
    );
    assert_eq!(
        rem.layout_geometry(400.0, 200.0).unwrap(),
        px.layout_geometry(400.0, 200.0).unwrap()
    );
    assert_eq!(
        rem.rasterize_boxes(400, 200).unwrap().pixels,
        px.rasterize_boxes(400, 200).unwrap().pixels
    );
    assert_eq!(
        rem.rasterize_scaled(800, 400, 2.0, None).unwrap().pixels,
        px.rasterize_scaled(800, 400, 2.0, None).unwrap().pixels
    );
}

#[test]
fn rem_grid_tracks_and_positioned_edges_resolve_before_layout() {
    for (relative, pixels) in [
        (
            "repeat(2, minmax(1rem, 2rem)) 25%",
            "repeat(2, minmax(16px, 32px)) 25%",
        ),
        ("minmax(0, 2rem) 1fr", "minmax(0, 32px) 1fr"),
    ] {
        let rem = boxes(
            &[
                (P::Display, "grid"),
                (P::Width, "12rem"),
                (P::GridTemplateColumns, relative),
                (P::GridTemplateRows, "1rem minmax(0, 2rem)"),
                (P::Gap, ".25rem .5rem"),
            ],
            &[(P::GridColumn, "2"), (P::GridRow, "2"), (P::Height, "1rem")],
        );
        let px = boxes(
            &[
                (P::Display, "grid"),
                (P::Width, "192px"),
                (P::GridTemplateColumns, pixels),
                (P::GridTemplateRows, "16px minmax(0, 32px)"),
                (P::Gap, "4px 8px"),
            ],
            &[(P::GridColumn, "2"), (P::GridRow, "2"), (P::Height, "16px")],
        );
        assert_eq!(
            rem.layout_geometry(400.0, 200.0).unwrap(),
            px.layout_geometry(400.0, 200.0).unwrap()
        );
    }
    let rem = boxes(
        &[
            (P::Width, "12rem"),
            (P::Height, "4rem"),
            (P::Position, "relative"),
        ],
        &[
            (P::Position, "absolute"),
            (P::Top, "-.5rem"),
            (P::Left, "1rem"),
            (P::Right, "2rem"),
            (P::Bottom, ".5rem"),
        ],
    );
    let px = boxes(
        &[
            (P::Width, "192px"),
            (P::Height, "64px"),
            (P::Position, "relative"),
        ],
        &[
            (P::Position, "absolute"),
            (P::Top, "-8px"),
            (P::Left, "16px"),
            (P::Right, "32px"),
            (P::Bottom, "8px"),
        ],
    );
    assert_eq!(
        rem.layout_geometry(400.0, 200.0).unwrap(),
        px.layout_geometry(400.0, 200.0).unwrap()
    );
    let thread = boxes(&[(P::MaxWidth, "48rem"), (P::Margin, "0 auto")], &[]);
    let rect = thread.layout_boxes(1024.0, 768.0).unwrap()[&2];
    assert_eq!((rect.x, rect.width), (128.0, 768.0));
}

#[test]
fn rem_font_size_and_line_height_use_the_reference_font() {
    let make = |size, line| {
        let mut view = boxes(
            &[
                (P::FontFamily, "\"Varta Unicode Fixture\""),
                (P::FontSize, size),
                (P::LineHeight, line),
                (P::Color, "#00f"),
                (P::Width, "120px"),
            ],
            &[],
        );
        let mut batch = BatchWriter::new();
        batch.remove(3).create_text(4, "AAA").insert(2, 4, 0);
        view.commit(&batch.finish(), false, &mut Denied).unwrap();
        view
    };
    let rem = make("1rem", "1.5rem");
    let px = make("16px", "24px");
    let mut font = TextMeasurer::new(
        include_bytes!("fixtures/unicode.ttf").to_vec(),
        "Varta Unicode Fixture",
    )
    .unwrap();
    assert_eq!(
        rem.layout_boxes_with_text(160.0, 80.0, &mut font, 20.0)
            .unwrap(),
        px.layout_boxes_with_text(160.0, 80.0, &mut font, 20.0)
            .unwrap()
    );
    assert_eq!(
        rem.rasterize_with_text(160, 80, &mut font, 20.0)
            .unwrap()
            .pixels,
        px.rasterize_with_text(160, 80, &mut font, 20.0)
            .unwrap()
            .pixels
    );
}

#[test]
fn rem_reference_changes_preserve_raw_styles_and_reject_invalid_host_metrics() {
    let mut view = boxes(&[(P::Width, "4rem"), (P::Height, "1rem")], &[]);
    let snapshot = view.snapshot();
    for (reference, width) in [(16.0, 64.0), (24.0, 96.0), (0.0, 0.0)] {
        view.set_environment(view.environment().with_root_font_size(reference).unwrap())
            .unwrap();
        assert_eq!(view.layout_boxes(400.0, 200.0).unwrap()[&2].width, width);
        view.rasterize_scaled(800, 400, 2.0, None).unwrap();
        assert_eq!(view.snapshot(), snapshot);
    }
    let environment = view.environment();
    for root_font_size in [f64::NAN, f64::INFINITY, -1.0, 1_000_001.0] {
        let invalid = Environment {
            root_font_size,
            ..environment
        };
        assert!(matches!(
            view.set_environment(invalid),
            Err(LayoutError::RootFont)
        ));
        assert_eq!(view.environment(), environment);
        assert!(matches!(
            Environment::default().with_root_font_size(root_font_size),
            Err(LayoutError::RootFont)
        ));
        let invalid_view = View::new(invalid, Default::default());
        assert!(matches!(
            invalid_view.layout_boxes(400.0, 200.0),
            Err(LayoutError::RootFont)
        ));
        assert!(matches!(
            invalid_view.rasterize_boxes(400, 200),
            Err(varta_view::PaintError::Layout(LayoutError::RootFont))
        ));
    }
    assert_eq!(view.snapshot(), snapshot);
}

#[test]
fn rem_reference_is_distinct_from_view_root_font_and_media_initial_font() {
    let make = |relative: bool| {
        let mut view = View::new(
            Environment::default().with_root_font_size(20.0).unwrap(),
            Default::default(),
        );
        let mut batch = BatchWriter::new();
        batch.stylesheet(1,if relative {
            ":root { font-size:2rem; line-height:1.5rem; --w:6rem } .p { width:var(--w); font-size:25%; color:#f00 } @media (min-width:50rem) { .p { color:#00f } }"
        } else {
            ":root { font-size:40px; line-height:30px; --w:120px } .p { width:var(--w); font-size:25%; color:#f00 } @media (min-width:800px) { .p { color:#00f } }"
        }).create_element(2,Tag::Div).set_class(2,"p").create_text(3,"AAA").insert(1,2,0).insert(2,3,0);
        view.commit(&batch.finish(), false, &mut Denied).unwrap();
        view
    };
    let rem = make(true);
    let px = make(false);
    let mut font = TextMeasurer::new(
        include_bytes!("fixtures/unicode.ttf").to_vec(),
        "Varta Unicode Fixture",
    )
    .unwrap();
    let geometry = rem
        .layout_boxes_with_text(900.0, 80.0, &mut font, 16.0)
        .unwrap();
    assert_eq!((geometry[&2].width, geometry[&2].height), (120.0, 30.0));
    assert_eq!(
        geometry,
        px.layout_boxes_with_text(900.0, 80.0, &mut font, 16.0)
            .unwrap()
    );
    let image = rem.rasterize_with_text(900, 80, &mut font, 16.0).unwrap();
    assert_eq!(
        image.pixels,
        px.rasterize_with_text(900, 80, &mut font, 16.0)
            .unwrap()
            .pixels
    );
    assert!(
        image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel == &[0, 0, 255, 255])
    );
}

#[test]
fn rem_conversion_keeps_resolved_limits_and_layout_transaction_atomicity() {
    let mut view = boxes(
        &[
            (P::Width, "4rem"),
            (P::Height, "2rem"),
            (P::Overflow, "auto"),
        ],
        &[(P::Height, "6rem")],
    );
    view.set_environment(view.environment().with_root_font_size(20.0).unwrap())
        .unwrap();
    let mut limit = BatchWriter::new();
    limit.set_style(3, P::Width, "50000rem");
    view.commit(&limit.finish(), false, &mut Denied).unwrap();
    assert_eq!(
        view.layout_boxes(400.0, 200.0).unwrap()[&3].width,
        1_000_000.0
    );
    let snapshot = view.snapshot();
    #[derive(Default)]
    struct Clipboard(usize);
    impl Broker for Clipboard {
        fn link_requested(&self, _: &str) -> bool {
            false
        }
        fn open_url(&mut self, _: &str) -> Access {
            Access::Denied
        }
        fn clipboard_write(&mut self) -> Access {
            self.0 += 1;
            Access::Granted
        }
    }
    let mut broker = Clipboard::default();
    let mut overflow = BatchWriter::new();
    overflow
        .set_title("unpublished")
        .copy_text(7, "deferred")
        .set_style(3, P::Width, "50000.05rem")
        .command(2, varta_common::view::Command::ScrollTo, 1000.0);
    assert!(matches!(
        view.commit_with_layout(&overflow.finish(), true, &mut broker, 400.0, 200.0, None),
        Err(varta_view::ViewError::Invalid {
            violation: varta_view::Violation::ScrollLayout,
            ..
        })
    ));
    assert_eq!(broker.0, 0);
    assert_eq!(view.snapshot(), snapshot);
    let mut recover = BatchWriter::new();
    recover
        .copy_text(7, "deferred")
        .set_style(3, P::Width, "2rem")
        .command(2, varta_common::view::Command::ScrollTo, 1000.0);
    view.commit_with_layout(&recover.finish(), true, &mut broker, 400.0, 200.0, None)
        .unwrap();
    assert_eq!(broker.0, 1);
    assert_eq!(view.layout_boxes(400.0, 200.0).unwrap()[&3].width, 40.0);
    let mut negative = BatchWriter::new();
    negative.set_style(3, P::Width, "-1rem");
    view.commit(&negative.finish(), false, &mut Denied).unwrap();
    assert!(matches!(
        view.layout_boxes(400.0, 200.0),
        Err(LayoutError::Unsupported(3))
    ));
}
