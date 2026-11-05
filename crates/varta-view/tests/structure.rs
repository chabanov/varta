//! Native L0 structure: user-agent defaults of headings, lists, `pre` and
//! `label`, `text-align`, one-line ellipsis and their accessibility geometry.
use varta_common::view::{Attribute as A, CssProperty as P, Tag};
use varta_view::{
    Access, BatchWriter as B, Broker, Input, LayoutError, PaintError, Target, TextFont,
    TextMeasurer, View,
};

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

const FAMILY: &str = "Varta Structure Fixture";
const REGULAR: &[u8] = include_bytes!("fixtures/structure-400.ttf");
const BOLD: &[u8] = include_bytes!("fixtures/structure-700.ttf");
const HOST_SIZE: f32 = 16.0;
const SIDE: u32 = 200;
/// A premultiplied red channel below this, with coverage, is dark ink.
const INK_THRESHOLD: u8 = 128;

fn fonts(faces: &[&[u8]], aliases: &[&str]) -> TextMeasurer {
    TextMeasurer::new_with_fonts(
        faces
            .iter()
            .map(|data| TextFont {
                data: data.to_vec(),
                family: FAMILY.into(),
                aliases: aliases.iter().map(|alias| (*alias).into()).collect(),
            })
            .collect(),
    )
    .unwrap()
}
fn font() -> TextMeasurer {
    fonts(&[REGULAR, BOLD], &[])
}
/// Root block 2 with the fixture font at 10px, 10px lines, black text.
fn base(width: &str) -> B {
    let mut b = B::new();
    b.create_element(2, Tag::Div).insert(1, 2, 0);
    for (property, value) in [
        (P::Width, width),
        (P::FontFamily, "\"Varta Structure Fixture\""),
        (P::FontSize, "10px"),
        (P::LineHeight, "10px"),
        (P::Color, "#000000"),
    ] {
        b.set_style(2, property, value);
    }
    b
}
/// The explicit control box and paint styles native fields require.
fn control(b: &mut B, id: u32, width: &str) {
    for (property, value) in [
        (P::Display, "block"),
        (P::Width, width),
        (P::Height, "12px"),
        (P::MinWidth, "0"),
        (P::MinHeight, "0"),
        (P::Padding, "0"),
        (P::Border, "1px solid #000"),
        (P::BorderRadius, "0"),
        (P::Outline, "none"),
        (P::BackgroundColor, "#fff"),
        (P::Color, "#000"),
        (P::FontFamily, "\"Varta Structure Fixture\""),
        (P::FontSize, "10px"),
        (P::FontWeight, "normal"),
        (P::FontStyle, "normal"),
        (P::LineHeight, "10px"),
    ] {
        b.set_style(id, property, value);
    }
}
fn add(b: &mut B, parent: u32, id: u32, tag: Tag, text: &str) {
    b.create_element(id, tag)
        .insert(parent, id, 0)
        .create_text(id + 1, text)
        .insert(id, id + 1, 0);
}
fn view(b: &B) -> View {
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    v
}
fn boxes(v: &View, f: &mut TextMeasurer) -> std::collections::BTreeMap<u32, varta_view::BoxRect> {
    v.layout_boxes_with_text(SIDE as f32, SIDE as f32, f, HOST_SIZE)
        .unwrap()
}
fn ink(image: &varta_view::RasterImage, x0: u32, y0: u32, x1: u32, y1: u32) -> u32 {
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            // Black ink: covered and darker than a white background.
            let at = ((y * image.width + x) * 4) as usize;
            let pixel = &image.pixels[at..at + 4];
            count += u32::from(pixel[3] > 0 && pixel[0] < INK_THRESHOLD);
        }
    }
    count
}

#[test]
fn headings_use_user_agent_sizes_margins_and_the_bold_face() {
    let mut b = base("200px");
    add(&mut b, 2, 3, Tag::H1, "A");
    add(&mut b, 2, 5, Tag::H2, "A");
    add(&mut b, 2, 7, Tag::H3, "A");
    add(&mut b, 2, 9, Tag::H2, "A");
    b.set_style(9, P::FontWeight, "400")
        .set_style(9, P::FontSize, "10px")
        .set_style(9, P::Margin, "0");
    let v = view(&b);
    let mut f = font();
    let g = boxes(&v, &mut f);
    // h1: 2em = 20px, margins 0.67em = 13.4px (collapsing through the root).
    assert!((g[&3].y - 13.4).abs() < 1e-4);
    assert_eq!(g[&3].height, 10.0);
    // h2 margin 0.83 * 15px collapses with h1's 13.4px bottom margin.
    assert!((g[&5].y - (13.4 + 10.0 + 13.4)).abs() < 1e-4);
    // h3: 1.17em font, 1em margin.
    assert!((g[&7].y - (g[&5].y + 10.0 + 12.45)).abs() < 1e-3);
    // Author weight, size and margins replace every default.
    assert!((g[&9].y - (g[&7].y + 10.0 + 11.7)).abs() < 1e-3);
    v.rasterize_with_text(SIDE, SIDE, &mut f, HOST_SIZE)
        .unwrap();
}

#[test]
fn bold_headings_without_a_bold_face_refuse_synthesized_bold() {
    let mut b = base("200px");
    add(&mut b, 2, 3, Tag::H2, "A");
    let v = view(&b);
    let mut regular_only = fonts(&[REGULAR], &[]);
    assert!(matches!(
        v.rasterize_with_text(SIDE, SIDE, &mut regular_only, HOST_SIZE),
        Err(PaintError::Unsupported(_))
    ));
    v.rasterize_with_text(SIDE, SIDE, &mut font(), HOST_SIZE)
        .unwrap();
}

#[test]
fn lists_indent_items_paint_markers_and_markers_hit_their_item() {
    let mut b = base("200px");
    b.create_element(3, Tag::Ul).insert(2, 3, 0);
    add(&mut b, 3, 4, Tag::Li, "A");
    b.create_element(6, Tag::Ol).insert(2, 6, 0);
    add(&mut b, 6, 7, Tag::Li, "A");
    add(&mut b, 6, 9, Tag::Li, "B");
    b.create_element(11, Tag::Li).insert(6, 11, 0);
    b.create_element(12, Tag::Ul).insert(11, 12, 0);
    add(&mut b, 12, 13, Tag::Li, "A");
    let v = view(&b);
    let mut f = font();
    let g = boxes(&v, &mut f);
    // ul/ol: 40px inline-start padding and 1em block margins.
    assert_eq!((g[&4].x, g[&4].width), (40.0, 160.0));
    assert_eq!(g[&3].y, 10.0);
    assert_eq!(g[&6].y, g[&3].y + 10.0 + 10.0);
    // A list nested in a list has no block margins.
    assert_eq!(g[&12].y, g[&11].y);
    assert_eq!(g[&13].x, 80.0);
    let image = v
        .rasterize_with_text(SIDE, SIDE, &mut f, HOST_SIZE)
        .unwrap();
    // The disc of item 4 (ascent 8: offset 5, bullet 3) at x 28..31.
    let top = g[&4].y as u32;
    assert!(ink(&image, 28, top, 31, top + 10) > 0);
    assert_eq!(ink(&image, 0, top, 28, top + 10), 0);
    assert_eq!(image.hit_test(29.5, g[&4].y + 5.0), Some(4));
    assert_eq!(image.hit_test(20.0, g[&4].y + 5.0), Some(3));
    // "1. " ends at the content edge: the decimal marker hits its item.
    assert_eq!(image.hit_test(39.0, g[&7].y + 5.0), Some(7));
    assert!(ink(&image, 25, g[&7].y as u32, 40, g[&7].y as u32 + 10) > 0);
    // The nested item's circle and the outer item's "3. " share one line.
    assert!(ink(&image, 25, g[&13].y as u32, 40, g[&13].y as u32 + 10) > 0);
    assert!(ink(&image, 66, g[&13].y as u32, 72, g[&13].y as u32 + 10) > 0);
}

#[test]
fn list_items_without_a_line_or_with_a_clip_are_refused() {
    let mut b = base("200px");
    b.create_element(3, Tag::Ul).insert(2, 3, 0);
    b.create_element(4, Tag::Li).insert(3, 4, 0);
    let v = view(&b);
    assert!(matches!(
        v.rasterize_with_text(SIDE, SIDE, &mut font(), HOST_SIZE),
        Err(PaintError::Layout(LayoutError::Unsupported(4)))
    ));
    let mut b = base("200px");
    b.create_element(3, Tag::Ul).insert(2, 3, 0);
    add(&mut b, 3, 4, Tag::Li, "A");
    b.set_style(4, P::Overflow, "hidden");
    assert!(matches!(
        view(&b).rasterize_with_text(SIDE, SIDE, &mut font(), HOST_SIZE),
        Err(PaintError::Layout(LayoutError::Unsupported(4)))
    ));
    // An author display removes the list-item marker.
    let mut b = base("200px");
    b.create_element(3, Tag::Ul).insert(2, 3, 0);
    b.create_element(4, Tag::Li).insert(3, 4, 0);
    b.set_style(4, P::Display, "block");
    view(&b)
        .rasterize_with_text(SIDE, SIDE, &mut font(), HOST_SIZE)
        .unwrap();
}

#[test]
fn pre_preserves_lines_and_resolves_the_monospace_face() {
    let pre = |root_size: Option<&str>| {
        let mut b = B::new();
        b.create_element(2, Tag::Div).insert(1, 2, 0);
        if let Some(size) = root_size {
            b.set_style(2, P::FontSize, size);
        }
        add(&mut b, 2, 3, Tag::Pre, "A\n  AB");
        view(&b)
    };
    // The user-agent family is the generic monospace: without a face mapped
    // to it the item is refused.
    assert!(matches!(
        pre(None).layout_boxes_with_text(200.0, 200.0, &mut font(), HOST_SIZE),
        Err(LayoutError::Unsupported(3))
    ));
    let mut mono = fonts(&[REGULAR, BOLD], &["monospace"]);
    // Medium-sized generic monospace is 13/16 of the host size (13px); the
    // margins are 1em of it. Two preserved lines of ascent+descent.
    let g = pre(None)
        .layout_boxes_with_text(200.0, 200.0, &mut mono, HOST_SIZE)
        .unwrap();
    assert_eq!(g[&3].y, 13.0);
    assert!((g[&3].height - 26.0).abs() < 1e-4);
    // Any explicit size disables the keyword quirk.
    let g = pre(Some("10px"))
        .layout_boxes_with_text(200.0, 200.0, &mut mono, HOST_SIZE)
        .unwrap();
    assert_eq!((g[&3].y, g[&3].height), (10.0, 20.0));
    // Preserved leading spaces are measured; no soft wrap occurs.
    let mut b = base("20px");
    add(&mut b, 2, 3, Tag::Pre, "AAAA AAAA");
    b.set_style(3, P::FontFamily, "\"Varta Structure Fixture\"");
    let g = boxes(&view(&b), &mut font());
    assert_eq!(g[&3].height, 10.0);
    // Tabs and trailing spaces are laid out natively (`text::whitespace`):
    // neither adds a line.
    for (text, lines) in [("A\tB", 1.0), ("A \nB", 2.0)] {
        let mut b = base("200px");
        add(&mut b, 2, 3, Tag::Pre, text);
        b.set_style(3, P::FontFamily, "\"Varta Structure Fixture\"");
        let g = view(&b)
            .layout_boxes_with_text(200.0, 200.0, &mut font(), HOST_SIZE)
            .unwrap();
        assert_eq!(g[&3].height, lines * 10.0, "{text:?}");
    }
}

/// Chrome 156 sizes where the generic family switches to or from monospace:
/// each case's flex element records its size in a `100%` (or `150%`) line
/// height; its fixture-font item returns to the proportional size.
#[test]
fn monospace_sizes_switch_at_the_family_change_like_chrome() {
    struct Case {
        outer: (Tag, &'static str),
        /// Inner tag and its extra author declarations.
        inner: (Tag, &'static [(P, &'static str)]),
        line_height: &'static str,
        /// Expected inner line height and the item's text box height.
        expected: (f32, f32),
    }
    const NONE: &[(P, &str)] = &[];
    // The item text box is the rounded ascent plus the rounded descent
    // (18.72px gives 15 + 4).
    let cases = [
        Case {
            outer: (Tag::H1, ""),
            inner: (Tag::Code, NONE),
            line_height: "100%",
            expected: (26.0, 32.0),
        },
        Case {
            outer: (Tag::H3, ""),
            inner: (Tag::Pre, NONE),
            line_height: "100%",
            expected: (15.21, 19.0),
        },
        Case {
            outer: (Tag::P, "150%"),
            inner: (Tag::Code, NONE),
            line_height: "100%",
            expected: (19.5, 24.0),
        },
        Case {
            outer: (Tag::Pre, ""),
            inner: (Tag::Span, &[(P::FontSize, "200%")]),
            line_height: "100%",
            expected: (26.0, 32.0),
        },
        Case {
            outer: (Tag::Div, ""),
            inner: (Tag::H2, &[(P::FontFamily, "monospace")]),
            line_height: "100%",
            expected: (19.5, 24.0),
        },
        Case {
            outer: (Tag::Div, ""),
            inner: (Tag::Pre, NONE),
            line_height: "150%",
            expected: (19.5, 16.0),
        },
    ];
    let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
    let mut mono = fonts(&[REGULAR, BOLD], &["monospace"]);
    for case in cases {
        let mut b = B::new();
        b.create_element(2, Tag::Div).insert(1, 2, 0);
        b.set_style(2, P::FontFamily, "\"Varta Structure Fixture\"");
        b.create_element(3, case.outer.0).insert(2, 3, 0);
        if !case.outer.1.is_empty() {
            b.set_style(3, P::FontSize, case.outer.1);
        }
        b.set_style(3, P::Display, "flex");
        b.create_element(4, case.inner.0).insert(3, 4, 0);
        for &(property, value) in case.inner.1 {
            b.set_style(4, property, value);
        }
        b.set_style(4, P::Display, "flex")
            .set_style(4, P::LineHeight, case.line_height);
        add(&mut b, 4, 5, Tag::Span, "A");
        b.set_style(5, P::FontFamily, "\"Varta Structure Fixture\"");
        let g = boxes(&view(&b), &mut mono);
        let inner = case.inner.0;
        assert!(
            close(g[&4].height, case.expected.0),
            "{inner:?}: {}",
            g[&4].height
        );
        assert!(
            close(g[&6].height, case.expected.1),
            "{inner:?}: {}",
            g[&6].height
        );
    }
    // An absolute size (`px`, `rem`) is never re-derived.
    for size in ["1rem", "16px"] {
        let mut b = B::new();
        add(&mut b, 1, 2, Tag::Pre, "A");
        b.set_style(2, P::FontSize, size);
        let g = boxes(&view(&b), &mut mono);
        assert!(close(g[&3].height, HOST_SIZE), "{size}");
    }
}

/// A packaged face that declares `monospace` (`new_with_packaged_fonts`)
/// makes the generic name its family, as the official browser host rewrites
/// it before Chrome sees it: neither the user-agent family of `pre`/`code`
/// nor a guest-set `monospace` takes the fixed-font size ratio, with or
/// without an explicit or inherited size.
#[test]
fn packaged_monospace_is_sized_as_its_named_family() {
    let packaged = || {
        TextMeasurer::new_with_packaged_fonts(
            [REGULAR, BOLD]
                .iter()
                .map(|data| TextFont {
                    data: data.to_vec(),
                    family: FAMILY.into(),
                    aliases: vec!["monospace".into()],
                })
                .collect(),
        )
        .unwrap()
    };
    let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
    // `pre` at the medium keyword: 16px (not 13px), margins 1em of it, two
    // preserved lines of ascent+descent.
    let mut b = B::new();
    b.create_element(2, Tag::Div).insert(1, 2, 0);
    add(&mut b, 2, 3, Tag::Pre, "A\n  AB");
    let g = view(&b)
        .layout_boxes_with_text(200.0, 200.0, &mut packaged(), HOST_SIZE)
        .unwrap();
    assert_eq!((g[&3].y, g[&3].height), (HOST_SIZE, 2.0 * HOST_SIZE));
    // Switching into and out of the generic keeps the inherited size: each
    // flex element records its size in a 100% line height.
    let cases = [
        // h1 (32px) > code stays 32px.
        (Tag::H1, None, Tag::Code, None, 32.0),
        // p { font-size: 150% } > code stays 24px.
        (Tag::P, Some("150%"), Tag::Code, None, 24.0),
        // pre > span { font-size: 200% } is 32px.
        (Tag::Pre, None, Tag::Span, Some("200%"), 32.0),
        // h2 { font-family: monospace } stays 24px.
        (Tag::Div, None, Tag::H2, Some("monospace"), 24.0),
    ];
    for (outer, outer_size, inner, inner_value, expected) in cases {
        let mut b = B::new();
        b.create_element(2, Tag::Div).insert(1, 2, 0);
        b.set_style(2, P::FontFamily, "\"Varta Structure Fixture\"");
        b.create_element(3, outer).insert(2, 3, 0);
        if let Some(size) = outer_size {
            b.set_style(3, P::FontSize, size);
        }
        b.set_style(3, P::Display, "flex");
        b.create_element(4, inner).insert(3, 4, 0);
        match (inner, inner_value) {
            (Tag::Span, Some(size)) => {
                b.set_style(4, P::FontSize, size);
            }
            (_, Some(family)) => {
                b.set_style(4, P::FontFamily, family);
            }
            _ => {}
        }
        b.set_style(4, P::Display, "flex")
            .set_style(4, P::LineHeight, "100%");
        add(&mut b, 4, 5, Tag::Span, "A");
        let g = boxes(&view(&b), &mut packaged());
        assert!(close(g[&4].height, expected), "{inner:?}: {}", g[&4].height);
    }
    // With the same faces loaded as stand-ins for the browser's generic
    // font, `pre` keeps the ratio (13px).
    let mut b = B::new();
    b.create_element(2, Tag::Div).insert(1, 2, 0);
    add(&mut b, 2, 3, Tag::Pre, "A");
    let g = view(&b)
        .layout_boxes_with_text(
            200.0,
            200.0,
            &mut fonts(&[REGULAR, BOLD], &["monospace"]),
            HOST_SIZE,
        )
        .unwrap();
    assert_eq!(g[&3].y, 13.0);
}

#[test]
fn a_label_click_activates_its_associated_control() {
    let mut b = base("200px");
    add(&mut b, 2, 3, Tag::Label, "AB");
    b.set_attribute(3, A::For, "agree");
    b.create_element(5, Tag::Input)
        .set_attribute(5, A::Type, "checkbox")
        .set_attribute(5, A::Id, "agree")
        .insert(1, 5, 0);
    control(&mut b, 5, "12px");
    let mut v = view(&b);
    let mut f = font();
    let g = boxes(&v, &mut f);
    // The label is an inline box of the root's flow.
    assert_eq!((g[&3].x, g[&3].width, g[&3].height), (0.0, 8.0, 10.0));
    let image = v
        .rasterize_with_text(SIDE, SIDE, &mut f, HOST_SIZE)
        .unwrap();
    let target = image.hit_test(4.0, 5.0).unwrap();
    assert_eq!(target, 3);
    v.input(
        &Input::Click {
            target: Target::Node(target),
        },
        &mut Denied,
    )
    .unwrap();
    assert_eq!(v.snapshot()["focus"], 5);
    let checked = |v: &View| {
        v.snapshot()["tree"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["id"] == 5)
            .unwrap()["checked"]
            .as_bool()
            .unwrap()
    };
    assert!(checked(&v));
}

#[test]
fn text_align_offsets_wrapped_and_decorated_lines_but_not_fields() {
    let mut b = base("50px");
    b.set_style(2, P::TextAlign, "center");
    b.create_text(3, "A AB ").insert(2, 3, 0);
    add(&mut b, 2, 4, Tag::Span, "AB");
    b.set_style(4, P::Padding, "0 2px");
    let v = view(&b);
    let mut f = font();
    let g = v
        .layout_geometry_with_text(SIDE as f32, SIDE as f32, &mut f, HOST_SIZE)
        .unwrap();
    // The 30px line ("A AB " and the padded 12px "AB") is centered in 50px.
    assert_eq!(g[&4].rect.x, 28.0);
    assert_eq!(g[&4].rect.width, 12.0);
    let image = v
        .rasterize_with_text(SIDE, SIDE, &mut f, HOST_SIZE)
        .unwrap();
    assert_eq!(ink(&image, 0, 0, 10, 10), 0);
    assert!(ink(&image, 10, 0, 40, 10) > 0);
    assert_eq!(ink(&image, 41, 0, 50, 10), 0);
    assert_eq!(image.hit_test(34.0, 5.0), Some(4));
    // Wrapped lines align individually.
    let mut b = base("12px");
    b.set_style(2, P::TextAlign, "right");
    b.create_text(3, "AB A").insert(2, 3, 0);
    let image = view(&b)
        .rasterize_with_text(SIDE, SIDE, &mut font(), HOST_SIZE)
        .unwrap();
    assert_eq!(ink(&image, 0, 0, 4, 10), 0);
    assert!(ink(&image, 4, 0, 12, 10) > 0);
    assert_eq!(ink(&image, 0, 10, 8, 20), 0);
    assert!(ink(&image, 8, 10, 12, 20) > 0);
    // A right-aligned single text node.
    let mut b = base("50px");
    add(&mut b, 2, 3, Tag::P, "AB");
    b.set_style(3, P::TextAlign, "right")
        .set_style(3, P::Margin, "0");
    let image = view(&b)
        .rasterize_with_text(SIDE, SIDE, &mut font(), HOST_SIZE)
        .unwrap();
    assert_eq!(ink(&image, 0, 0, 42, 10), 0);
    assert!(ink(&image, 42, 0, 50, 10) > 0);
    // Inherited alignment stops at a field; an authored field alignment is refused.
    let mut b = base("200px");
    b.set_style(2, P::TextAlign, "right");
    b.create_element(3, Tag::Input).insert(2, 3, 0);
    control(&mut b, 3, "100px");
    b.set_value(3, "A", 0);
    let image = view(&b)
        .rasterize_with_text(SIDE, SIDE, &mut font(), HOST_SIZE)
        .unwrap();
    // The value starts at the field's left content edge (after a 1px border).
    assert!(ink(&image, 1, 1, 6, 11) > 0);
    assert_eq!(ink(&image, 20, 1, 99, 11), 0);
    b.set_style(3, P::TextAlign, "center");
    assert!(
        view(&b)
            .rasterize_with_text(SIDE, SIDE, &mut font(), HOST_SIZE)
            .is_err()
    );
    // A right-to-left base paragraph cannot honor CSS `direction: ltr`.
    let mut b = base("100px");
    add(&mut b, 2, 3, Tag::P, "\u{05d0}\u{05d1} A");
    b.set_style(3, P::TextAlign, "center");
    assert!(matches!(
        view(&b).layout_boxes_with_text(200.0, 200.0, &mut font(), HOST_SIZE),
        Err(LayoutError::Unsupported(3))
    ));
}

#[test]
fn ellipsis_hides_overflowing_glyphs_but_keeps_geometry_hits_and_name() {
    let mut b = base("200px");
    b.create_element(3, Tag::Div).insert(2, 3, 0);
    for (property, value) in [
        (P::Width, "40px"),
        (P::WhiteSpace, "nowrap"),
        (P::Overflow, "hidden"),
        (P::TextOverflow, "ellipsis"),
    ] {
        b.set_style(3, property, value);
    }
    b.create_text(4, "AA ").insert(3, 4, 0);
    add(&mut b, 3, 5, Tag::Span, "AAAAAAAAAA");
    let v = view(&b);
    let mut f = font();
    let g = boxes(&v, &mut f);
    // The span keeps its untruncated geometry (11px .. 51px).
    assert_eq!((g[&5].x, g[&5].width), (11.0, 40.0));
    let image = v
        .rasterize_with_text(SIDE, SIDE, &mut f, HOST_SIZE)
        .unwrap();
    // The 6px ellipsis leaves 34px: "AA " (11px) and five whole "A" (20px)
    // fit; the ellipsis dots follow at 31..37px and nothing paints after them.
    assert!(ink(&image, 31, 7, 37, 9) > 0);
    assert_eq!(ink(&image, 37, 0, 40, 10), 0);
    // Hidden glyph positions still hit the span, as in Chrome.
    assert_eq!(image.hit_test(38.0, 5.0), Some(5));
    // Nothing paints beyond the box.
    assert_eq!(ink(&image, 40, 0, 200, 10), 0);
    let tree = v.accessibility_tree().unwrap();
    let names: Vec<_> = tree.nodes.values().map(|n| n.name.clone()).collect();
    // The truncated text keeps its whole accessible name.
    assert!(names.iter().any(|name| name == "AAAAAAAAAA"));
    // Wrapping text, decorated inline content and scroll containers refuse.
    for (property, value) in [(P::WhiteSpace, "normal"), (P::Overflow, "auto")] {
        let mut c = b.clone();
        c.set_style(3, property, value);
        assert!(
            view(&c)
                .rasterize_with_text(SIDE, SIDE, &mut font(), HOST_SIZE)
                .is_err()
        );
    }
    let mut c = b.clone();
    c.set_style(5, P::BackgroundColor, "#ff0000");
    assert!(matches!(
        view(&c).layout_boxes_with_text(200.0, 200.0, &mut font(), HOST_SIZE),
        Err(LayoutError::Unsupported(3))
    ));
    // `clip` and a visible overflow do not truncate.
    let mut c = b.clone();
    c.set_style(3, P::TextOverflow, "clip");
    view(&c)
        .rasterize_with_text(SIDE, SIDE, &mut font(), HOST_SIZE)
        .unwrap();
}

#[test]
fn accessibility_exposes_heading_levels_list_roles_and_frame_geometry() {
    let mut b = base("200px");
    add(&mut b, 2, 3, Tag::H2, "A");
    b.create_element(5, Tag::Ul).insert(2, 5, 0);
    add(&mut b, 5, 6, Tag::Li, "B");
    let v = view(&b);
    let mut f = font();
    let g = boxes(&v, &mut f);
    let tree = v.accessibility_tree().unwrap();
    assert_eq!(tree.nodes[&3].role, "heading");
    assert_eq!(tree.nodes[&3].level, Some(2));
    assert_eq!(tree.nodes[&5].role, "list");
    assert_eq!(tree.nodes[&6].role, "listitem");
    let metadata = v
        .rasterize_with_text(SIDE, SIDE, &mut f, HOST_SIZE)
        .unwrap()
        .into_metadata();
    let bounds = metadata.node_bounds_map();
    // Frame bounds cover the heading's line fragments (content areas).
    let heading = bounds[&3];
    assert_eq!((heading.x, heading.width), (g[&3].x, g[&3].width));
    assert!(heading.y <= g[&3].y && heading.y + heading.height >= g[&3].y + g[&3].height);
    // A list item's frame covers its box and its outside marker.
    let item = bounds[&6];
    assert!(item.x < g[&6].x);
    assert_eq!(item.x + item.width, g[&6].x + g[&6].width);
    assert_eq!(bounds[&5].y, g[&5].y);
}
