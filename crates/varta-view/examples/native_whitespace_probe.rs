//! Native preserved white space (tab stops, preserved, hanging and
//! conditionally hanging spaces, mixed modes) for the Chrome inline oracle,
//! `scripts/test-native-inline.mjs`. Arguments: an optional device scale.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter, Broker, TextMeasurer, View};

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

const FAMILY: &str = "Varta Inline Fixture";
const VIEWPORT: [u32; 2] = [200, 100];
/// Hit-test grid, in CSS pixels: step and extent of each axis.
const HIT_STEP: [usize; 2] = [4, 5];
const HIT_EXTENT: [usize; 2] = [200, 100];
/// Every case's root text style; cases add or override declarations.
const ROOT_STYLE: [(P, &str); 4] = [
    (P::FontFamily, "\"Varta Inline Fixture\""),
    (P::FontSize, "20px"),
    (P::LineHeight, "20px"),
    (P::Color, "#000"),
];

enum Node {
    Text(&'static str),
    Element(Tag, Vec<(P, &'static str)>, Vec<Node>),
}
fn text(value: &'static str) -> Node {
    Node::Text(value)
}
fn span(style: &[(P, &'static str)], children: Vec<Node>) -> Node {
    Node::Element(Tag::Span, style.to_vec(), children)
}
fn block(tag: Tag, style: &[(P, &'static str)], children: Vec<Node>) -> Node {
    Node::Element(tag, style.to_vec(), children)
}

/// The probe cases: the root element and its descendants.
fn fixtures() -> Vec<(&'static str, Node)> {
    let pre_wrap = (P::WhiteSpace, "pre-wrap");
    vec![
        // Stops 64px apart from the line start: after one glyph, directly
        // at a stop, and twice in a row.
        (
            "tab-stops",
            block(
                Tag::Div,
                &[pre_wrap, (P::Width, "200px")],
                vec![text("A\t"), span(&[], vec![text("A")]), text("\t\tA")],
            ),
        ),
        // After preserved indentation, a ligature ends 2px before the first
        // stop: closer than half a space, so the tab advances to the
        // following stop. Glyph origins stay on whole device pixels at every
        // probed scale.
        (
            "tab-offsets",
            block(
                Tag::Div,
                &[pre_wrap, (P::Width, "200px")],
                vec![text("      AA\t"), span(&[], vec![text("A")]), text("\tA")],
            ),
        ),
        // Leading and trailing preserved spaces keep their advance.
        (
            "preserved-spaces",
            block(
                Tag::Div,
                &[pre_wrap, (P::Width, "100px")],
                vec![text("  A  "), span(&[], vec![text("A")]), text("   ")],
            ),
        ),
        // The wrap falls inside a run of spaces: all of them hang.
        (
            "hanging-spaces",
            block(
                Tag::Div,
                &[pre_wrap, (P::Width, "100px")],
                vec![text("A A A A A A    "), span(&[], vec![text("A")])],
            ),
        ),
        // Two tabs past the right edge hang together.
        (
            "hanging-tabs",
            block(
                Tag::Div,
                &[pre_wrap, (P::Width, "100px")],
                vec![text("A A A A A A\t\t"), span(&[], vec![text("A")])],
            ),
        ),
        // Wrapping at the right edge, then a tab measured from the new line.
        (
            "wrap-right-edge",
            block(
                Tag::Div,
                &[pre_wrap, (P::Width, "60px")],
                vec![
                    text("A A A\tA A\t"),
                    span(&[], vec![text("A")]),
                    text(" A A A A"),
                ],
            ),
        ),
        // Preserved spans in a collapsing flow: their spaces stay, and a
        // following collapsible space is not collapsed into them.
        (
            "mixed-normal-root",
            block(
                Tag::Div,
                &[(P::Width, "200px")],
                vec![
                    text("A "),
                    span(&[pre_wrap], vec![text("  A  ")]),
                    text(" A"),
                    span(&[pre_wrap], vec![text("\tA")]),
                ],
            ),
        ),
        // A collapsing span in a preserving flow.
        (
            "mixed-pre-wrap-root",
            block(
                Tag::Div,
                &[pre_wrap, (P::Width, "200px")],
                vec![
                    text("A"),
                    span(&[(P::WhiteSpace, "normal")], vec![text("   A   ")]),
                    span(&[], vec![text("  A")]),
                ],
            ),
        ),
        // `pre` keeps trailing spaces in its shrink-to-fit width.
        (
            "pre-shrink",
            block(
                Tag::Div,
                &[(P::Display, "flex"), (P::Width, "200px")],
                vec![block(
                    Tag::Pre,
                    &[
                        (P::Margin, "0"),
                        (P::FontFamily, "\"Varta Inline Fixture\""),
                    ],
                    vec![text("A\tA   \n\tA "), span(&[], vec![text("A")])],
                )],
            ),
        ),
        // Spaces before a forced break and at the end count in max-content.
        (
            "pre-wrap-shrink",
            block(
                Tag::Div,
                &[(P::Display, "flex"), (P::Width, "200px")],
                vec![block(
                    Tag::Div,
                    &[pre_wrap],
                    vec![text("A   \nA  "), span(&[], vec![text("A")])],
                )],
            ),
        ),
        // A flex item narrower than its content shrinks to min-content,
        // where hanging spaces and tabs do not count.
        (
            "pre-wrap-min-content",
            block(
                Tag::Div,
                &[(P::Display, "flex"), (P::Width, "4px")],
                vec![block(
                    Tag::Div,
                    &[pre_wrap],
                    vec![text("A\tA   A\n  "), span(&[], vec![text("A")])],
                )],
            ),
        ),
        // Center alignment counts preserved spaces before a forced break and
        // hangs those at a soft wrap.
        (
            "pre-wrap-center",
            block(
                Tag::Div,
                &[pre_wrap, (P::Width, "100px"), (P::TextAlign, "center")],
                vec![
                    span(&[], vec![text("A")]),
                    text("  \nA A A A A A    "),
                    span(&[], vec![text("A")]),
                ],
            ),
        ),
        // Tab stops follow the root's 8px space whatever the tab's own
        // font: a 10px span's tab still reaches the 64px stop.
        (
            "tab-small-span",
            block(
                Tag::Div,
                &[pre_wrap, (P::Width, "200px")],
                vec![text("A"), span(&[(P::FontSize, "10px")], vec![text("\tA")])],
            ),
        ),
        // A 30px span's tab also stops on the root grid, and so does a
        // following 10px span's tab.
        (
            "tab-mixed-spans",
            block(
                Tag::Div,
                &[pre_wrap, (P::Width, "200px")],
                vec![
                    text("A"),
                    span(&[(P::FontSize, "30px")], vec![text("A\t")]),
                    span(&[(P::FontSize, "10px")], vec![text("\tA")]),
                ],
            ),
        ),
        // A wrapping span inside `pre` wraps at min-content, though the
        // flow needs no preserved-space model. The span's last space keeps
        // the fixture's `AA` ligature from joining the two text nodes.
        (
            "pre-normal-span-min-content",
            block(
                Tag::Div,
                &[(P::Display, "flex"), (P::Width, "4px")],
                vec![block(
                    Tag::Pre,
                    &[
                        (P::Margin, "0"),
                        (P::FontFamily, "\"Varta Inline Fixture\""),
                    ],
                    vec![
                        span(&[(P::WhiteSpace, "normal")], vec![text("A A A ")]),
                        text("A"),
                    ],
                )],
            ),
        ),
        // The same inside a `nowrap` root.
        (
            "nowrap-normal-span-min-content",
            block(
                Tag::Div,
                &[(P::Display, "flex"), (P::Width, "4px")],
                vec![block(
                    Tag::Div,
                    &[(P::WhiteSpace, "nowrap")],
                    vec![
                        span(&[(P::WhiteSpace, "normal")], vec![text("A A A ")]),
                        text("A"),
                    ],
                )],
            ),
        ),
        // The underline spans trailing `pre` spaces, before a forced break
        // and at the end.
        (
            "pre-underline-trailing",
            block(
                Tag::Div,
                &[(P::Width, "200px")],
                vec![block(
                    Tag::Pre,
                    &[
                        (P::Margin, "0"),
                        (P::FontFamily, "\"Varta Inline Fixture\""),
                        (P::TextDecoration, "underline"),
                    ],
                    vec![text("A   \nA    ")],
                )],
            ),
        ),
    ]
}

/// Inserting before node 0 appends.
const APPEND: u32 = 0;

/// Creates `node` as the last child of `parent`; returns the next free id.
fn build(batch: &mut BatchWriter, parent: u32, node: &Node, id: u32) -> u32 {
    match node {
        Node::Text(value) => {
            batch.create_text(id, value).insert(parent, id, APPEND);
            id + 1
        }
        Node::Element(tag, style, children) => {
            batch.create_element(id, *tag).insert(parent, id, APPEND);
            for &(property, value) in style {
                batch.set_style(id, property, value);
            }
            children
                .iter()
                .fold(id + 1, |next, child| build(batch, id, child, next))
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let scale = args
        .next()
        .map(|value| value.parse::<f32>())
        .transpose()?
        .unwrap_or(1.0);
    if !scale.is_finite() || !(0.25..=8.0).contains(&scale) {
        return Err("scale must be within 0.25..=8".into());
    }
    if args.next().is_some() {
        return Err("unexpected probe argument".into());
    }
    let font_bytes = include_bytes!("../tests/fixtures/inline.ttf");
    let mut font = TextMeasurer::new(font_bytes.to_vec(), FAMILY)?;
    let [width, height] = VIEWPORT;
    let raster = [
        (width as f32 * scale).round() as u32,
        (height as f32 * scale).round() as u32,
    ];
    let mut cases = Vec::new();
    for (name, root) in fixtures() {
        let mut batch = BatchWriter::new();
        build(&mut batch, 1, &root, 2);
        for (property, value) in ROOT_STYLE {
            // A case's own declaration of the same property wins.
            let declared = matches!(&root, Node::Element(_, style, _)
                if style.iter().any(|(own, _)| *own == property));
            if !declared {
                batch.set_style(2, property, value);
            }
        }
        let mut view = View::default();
        view.commit(&batch.finish(), false, &mut Denied)?;
        let geometry = view.layout_boxes_with_text(width as f32, height as f32, &mut font, 20.0)?;
        let image = view.rasterize_scaled(raster[0], raster[1], scale, Some((&mut font, 20.0)))?;
        let boxes = geometry
            .iter()
            .filter(|(id, _)| **id != 1)
            .map(|(&id, r)| {
                (
                    id.to_string(),
                    serde_json::json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height}),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let hit = |x: f32, y: f32| image.hit_test(x * scale, y * scale);
        let hits = (0..HIT_EXTENT[1])
            .step_by(HIT_STEP[1])
            .flat_map(|y| {
                (0..HIT_EXTENT[0])
                    .step_by(HIT_STEP[0])
                    .map(move |x| (x as f32, y as f32))
            })
            .map(|(x, y)| {
                // The oracle tests the same quarter-pixel offset.
                let (px, py) = (x + 0.25, y + 0.25);
                let target = hit(px, py);
                let boundary = [-1.0, 1.0].into_iter().any(|dx| {
                    [-1.0, 1.0]
                        .into_iter()
                        .any(|dy| hit(px + dx, py + dy) != target)
                });
                serde_json::json!({"x":x,"y":y,"id":target,"boundary":boundary})
            })
            .collect::<Vec<_>>();
        let root_box = geometry[&2];
        cases.push(serde_json::json!({
            "name": name,
            "tree": view.snapshot()["tree"],
            "boxes": boxes,
            "hits": hits,
            "samples": [],
            "raster": image.pixels,
            "raster_size": [image.width, image.height],
            "roi": {"x": 0, "y": 0, "width": raster[0], "height": (root_box.y + root_box.height) * scale},
        }));
    }
    let output = serde_json::to_vec(&serde_json::json!({
        "family": FAMILY, "font": font_bytes.as_slice(), "scale": scale,
        "viewport": {"width": width, "height": height}, "cases": cases,
    }))?;
    use std::io::Write;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}
