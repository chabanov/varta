//! Opt-in layout rectangles for the headless snapshot (`geometry` section).
//!
//! The ordinary snapshot carries the tree, cascaded styles and accessibility
//! tree only. A host that supplies explicit fonts may request this section
//! too: one border-box rectangle per displayed element, in document order,
//! computed by the same experimental layout as
//! [`View::layout_geometry_with_text`](crate::View::layout_geometry_with_text).
//! Unsupported content or CSS fails the whole request; no partial section is
//! published.
use crate::layout::{BoxRect, LayoutError};
use crate::style::Environment;
use crate::text::TextMeasurer;
use crate::tree::{Content, Tree};
use serde_json::{Map, Number, Value, json};
use std::collections::BTreeMap;
use varta_common::view::{CssProperty, ROOT_NODE, Tag};

/// Default headless viewport height in CSS pixels; with
/// [`DEFAULT_VIEWPORT_WIDTH`](crate::DEFAULT_VIEWPORT_WIDTH) it is the
/// 1024×768 headless frame.
pub const DEFAULT_VIEWPORT_HEIGHT: f32 = 768.0;

/// The snapshot key of the geometry section.
pub const GEOMETRY_KEY: &str = "geometry";

/// What a host supplies for a geometry section: the explicit fonts (the first
/// family is the default when CSS names none) and the viewport height. The
/// viewport width and base text size come from the view's [`Environment`].
pub struct GeometryRequest {
    measurer: TextMeasurer,
    height: f32,
}

impl GeometryRequest {
    /// Geometry with these explicit fonts at [`DEFAULT_VIEWPORT_HEIGHT`].
    #[must_use]
    pub fn new(measurer: TextMeasurer) -> Self {
        Self {
            measurer,
            height: DEFAULT_VIEWPORT_HEIGHT,
        }
    }

    /// The viewport height used for layout and reported in the section.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.height
    }

    /// Admit `environment` for this request before any session runs: its
    /// viewport width and base text size must have exact single-precision
    /// values ([`LayoutError::InexactViewportWidth`], [`LayoutError::RootFont`]).
    /// An environment the view itself admits, such as a width of `300.1`, may
    /// still be refused here; layout validates the viewport range.
    pub fn admit(&self, environment: &Environment) -> Result<(), LayoutError> {
        viewport_width(environment)?;
        base_font_size(environment)?;
        Ok(())
    }
}

/// The outer display type layout used for an element. It differs from the
/// cascaded `style.display` of the tree when a flex or grid container
/// blockifies an inline child, or when the element declares no `display`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UsedDisplay {
    Block,
    Inline,
    Flex,
    Grid,
}

impl UsedDisplay {
    fn name(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Inline => "inline",
            Self::Flex => "flex",
            Self::Grid => "grid",
        }
    }
}

/// The used display of a displayed element whose layout succeeded.
pub(crate) fn used_display(tree: &Tree, environment: &Environment, id: u32) -> UsedDisplay {
    if id == ROOT_NODE {
        return UsedDisplay::Block;
    }
    let cascaded = tree.computed_property(id, environment, CssProperty::Display);
    let inline = match cascaded.as_deref() {
        Some(value) if value.eq_ignore_ascii_case("flex") => return UsedDisplay::Flex,
        Some(value) if value.eq_ignore_ascii_case("grid") => return UsedDisplay::Grid,
        Some(value) => value.eq_ignore_ascii_case("inline"),
        None => tree
            .element(id)
            .is_some_and(|element| crate::inline::inline_tag(element.tag)),
    };
    if inline && !crate::inline::blockified(tree, environment, id) {
        UsedDisplay::Inline
    } else {
        UsedDisplay::Block
    }
}

/// Compute the geometry section of `tree` for `request`.
pub(crate) fn section(
    tree: &Tree,
    environment: &Environment,
    request: &mut GeometryRequest,
) -> Result<Value, LayoutError> {
    let width = viewport_width(environment)?;
    let font_size = base_font_size(environment)?;
    let boxes = crate::layout::geometry_with_text(
        tree,
        environment,
        width,
        request.height,
        &mut request.measurer,
        font_size,
    )?;
    let rects: BTreeMap<u32, BoxRect> = boxes.into_iter().map(|(id, b)| (id, b.rect)).collect();
    let mut entries = Vec::new();
    for id in tree.displayed_order(environment) {
        let Some(Content::Element(element)) = tree.nodes.get(&id).map(|node| &node.content) else {
            continue;
        };
        // A line break is a text-level control, not a box of its own.
        if element.tag == Tag::Br {
            continue;
        }
        let rect = rects.get(&id).ok_or(LayoutError::Unsupported(id))?;
        let mut entry = Map::new();
        entry.insert("id".into(), json!(id));
        entry.insert(
            "display".into(),
            json!(used_display(tree, environment, id).name()),
        );
        for (key, value) in [
            ("x", rect.x),
            ("y", rect.y),
            ("width", rect.width),
            ("height", rect.height),
        ] {
            entry.insert(key.into(), number(value)?);
        }
        entries.push(Value::Object(entry));
    }
    Ok(json!({
        "viewport": {"width": number(width)?, "height": number(request.height)?},
        "fontSize": number(font_size)?,
        "boxes": entries,
    }))
}

/// The single-precision layout width. The view admits any finite width in its
/// range, but layout runs in `f32`; a width with no exact `f32` value (such as
/// `300.1`) is refused rather than rounded, so the rectangles and the snapshot
/// cascade always see the same media width. Layout validates the range.
fn viewport_width(environment: &Environment) -> Result<f32, LayoutError> {
    let width = environment.width as f32;
    if f64::from(width) == environment.width {
        Ok(width)
    } else {
        Err(LayoutError::InexactViewportWidth(environment.width))
    }
}

fn base_font_size(environment: &Environment) -> Result<f32, LayoutError> {
    let size = environment.root_font_size as f32;
    if size.is_finite() && size > 0.0 && f64::from(size) == environment.root_font_size {
        Ok(size)
    } else {
        Err(LayoutError::RootFont)
    }
}

/// The shortest decimal that round-trips the single-precision layout value,
/// so `33.333332` is not widened to `33.33333206176758`; `-0` becomes `0`.
fn number(value: f32) -> Result<Value, LayoutError> {
    let value = if value == 0.0 { 0.0 } else { value };
    value
        .to_string()
        .parse::<f64>()
        .ok()
        .and_then(Number::from_f64)
        .map(Value::Number)
        .ok_or_else(|| LayoutError::Engine("non-finite layout geometry".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Access, BatchWriter, Broker, Platform, View};
    use varta_common::view::CssProperty as P;

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

    const FAMILY: &str = "Varta Fixture 400";

    fn font() -> TextMeasurer {
        TextMeasurer::new(
            include_bytes!("../tests/fixtures/advance-400.ttf").to_vec(),
            FAMILY,
        )
        .unwrap()
    }

    fn view(environment: Environment, batch: BatchWriter) -> View {
        let mut view = View::new(environment, Platform::default());
        view.commit(&batch.finish(), false, &mut Denied).unwrap();
        view
    }

    /// A flex row of text-level children, a paragraph with an inline child
    /// and a line break, a hidden subtree and a grid with an explicitly
    /// inline child.
    fn mixed() -> BatchWriter {
        let mut b = BatchWriter::new();
        b.create_element(2, Tag::Div).insert(1, 2, 0);
        b.set_style(2, P::Display, "flex");
        b.create_element(3, Tag::Span).insert(2, 3, 0);
        b.create_text(4, "A A").insert(3, 4, 0);
        b.create_element(5, Tag::P).insert(1, 5, 0);
        b.create_text(6, "A ").insert(5, 6, 0);
        b.create_element(7, Tag::Strong).insert(5, 7, 0);
        b.create_text(8, "A").insert(7, 8, 0);
        b.create_element(9, Tag::Br).insert(5, 9, 0);
        b.create_element(10, Tag::Div).insert(1, 10, 0);
        b.set_style(10, P::Display, "none");
        b.create_element(11, Tag::Div).insert(10, 11, 0);
        b.create_element(12, Tag::Div).insert(1, 12, 0);
        b.set_style(12, P::Display, "grid");
        b.create_element(13, Tag::Span).insert(12, 13, 0);
        b.set_style(13, P::Display, "inline");
        b.create_text(14, "A").insert(13, 14, 0);
        b
    }

    fn entry(section: &Value, id: u32) -> &Value {
        section["boxes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == id)
            .unwrap_or_else(|| panic!("no box {id}"))
    }

    #[test]
    fn section_lists_displayed_elements_with_used_display_and_layout_rectangles() {
        let environment = Environment {
            width: 375.0,
            ..Environment::default()
        };
        let view = view(environment, mixed());
        let mut request = GeometryRequest::new(font());
        let section = section(&view.tree, &view.environment, &mut request).unwrap();
        let ids: Vec<u32> = section["boxes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| u32::try_from(entry["id"].as_u64().unwrap()).unwrap())
            .collect();
        // Document order; no text nodes, no line break, no hidden subtree.
        assert_eq!(ids, [1, 2, 3, 5, 7, 12, 13]);
        for (id, display) in [
            (1, "block"),
            (2, "flex"),
            (3, "block"),
            (5, "block"),
            (7, "inline"),
            (12, "grid"),
            (13, "block"),
        ] {
            assert_eq!(entry(&section, id)["display"], display, "node {id}");
        }
        assert_eq!(
            section["viewport"],
            json!({"width": 375.0, "height": DEFAULT_VIEWPORT_HEIGHT})
        );
        assert_eq!(section["fontSize"], json!(environment.root_font_size));
        // The rectangles are the public layout API's border boxes.
        let mut font = font();
        let layout = view
            .layout_geometry_with_text(
                375.0,
                request.height(),
                &mut font,
                base_font_size(&environment).unwrap(),
            )
            .unwrap();
        for &id in &ids {
            let rect = layout[&id].rect;
            let reported = entry(&section, id);
            for (key, value) in [
                ("x", rect.x),
                ("y", rect.y),
                ("width", rect.width),
                ("height", rect.height),
            ] {
                assert_eq!(reported[key], number(value).unwrap(), "{id} {key}");
            }
        }
        // The blockified grid item keeps its cascaded `inline` in the tree.
        let snapshot = view.snapshot();
        let grid_item = &snapshot["tree"]["children"][3]["children"][0];
        assert_eq!(grid_item["id"], 13);
        assert_eq!(grid_item["style"]["display"], "inline");
        assert_eq!(
            section,
            super::section(
                &view.tree,
                &view.environment,
                &mut GeometryRequest::new(font)
            )
            .unwrap(),
            "the section is a function of the view and its fonts"
        );
    }

    #[test]
    fn snapshot_with_geometry_only_adds_the_section() {
        let view = view(Environment::default(), mixed());
        let mut measured = view
            .snapshot_with_geometry(&mut GeometryRequest::new(font()))
            .unwrap();
        let section = measured
            .as_object_mut()
            .unwrap()
            .remove(GEOMETRY_KEY)
            .unwrap();
        assert_eq!(measured, view.snapshot());
        assert!(view.snapshot().get(GEOMETRY_KEY).is_none());
        assert_eq!(section["viewport"]["width"], json!(1024.0));
    }

    #[test]
    fn unsupported_layout_fails_the_whole_request() {
        let mut b = mixed();
        // A text-level child the native layout does not admit as a flex item.
        b.create_element(15, Tag::Code).insert(2, 15, 0);
        b.create_text(16, "A").insert(15, 16, 0);
        let view = view(Environment::default(), b);
        assert!(matches!(
            view.snapshot_with_geometry(&mut GeometryRequest::new(font())),
            Err(LayoutError::Unsupported(15))
        ));
        let mut environment = view.environment();
        for size in [0.0, 0.1, f64::NAN] {
            environment.root_font_size = size;
            assert!(matches!(
                base_font_size(&environment),
                Err(LayoutError::RootFont)
            ));
        }
    }

    #[test]
    fn inexact_viewport_widths_are_refused_before_layout() {
        let request = GeometryRequest::new(font());
        assert!(request.admit(&Environment::default()).is_ok());
        for width in [0.1, 300.1, 1_024.000_000_1] {
            let environment = Environment {
                width,
                ..Environment::default()
            };
            assert!(
                matches!(
                    request.admit(&environment),
                    Err(LayoutError::InexactViewportWidth(refused)) if refused == width
                ),
                "{width}"
            );
            // The view itself admits the width; only the geometry refuses it.
            let mut view = view(Environment::default(), mixed());
            view.set_environment(environment).unwrap();
            assert!(matches!(
                view.snapshot_with_geometry(&mut GeometryRequest::new(font())),
                Err(LayoutError::InexactViewportWidth(_))
            ));
        }
        // Widths with an exact single-precision value, fractional ones too.
        for width in [300.0, 300.5, 375.25] {
            let environment = Environment {
                width,
                ..Environment::default()
            };
            assert!(request.admit(&environment).is_ok(), "{width}");
        }
        let environment = Environment {
            root_font_size: 0.1,
            ..Environment::default()
        };
        assert!(matches!(
            request.admit(&environment),
            Err(LayoutError::RootFont)
        ));
    }

    #[test]
    fn numbers_are_the_shortest_single_precision_decimals() {
        for (value, text) in [(19.2, "19.2"), (70.880_005, "70.880005"), (-0.0, "0.0")] {
            assert_eq!(
                serde_json::to_string(&number(value).unwrap()).unwrap(),
                text
            );
        }
        for value in [f32::NAN, f32::INFINITY] {
            assert!(matches!(number(value), Err(LayoutError::Engine(_))));
        }
    }
}
