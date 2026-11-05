//! Platform accessibility bounds from one presented frame, in physical pixels.
use crate::FrameMetadata;
use crate::layout::BoxRect;
use std::collections::BTreeMap;

fn empty(rect: BoxRect) -> bool {
    rect.width <= 0.0 || rect.height <= 0.0
}

impl FrameMetadata {
    /// Bounds for platform accessibility nodes, all from this frame:
    ///
    /// - a node with visible element fragments keeps their union
    ///   ([`Self::node_bounds_map`]);
    /// - an option of the open select list gets its list row, including a row
    ///   scrolled out of the list at the position the list offset gives it;
    /// - any other laid-out node (scrolled out, clipped away, or a text node,
    ///   whose layout box is its anonymous block or its inline fragment box)
    ///   gets its unclipped layout box at the presented scroll offsets.
    ///
    /// Options of a closed select stay absent: their layout has no box.
    #[must_use]
    pub fn accessibility_bounds_map(&self) -> BTreeMap<u32, BoxRect> {
        let mut bounds = self.node_bounds_map();
        for (option, rect) in self.select_option_bounds() {
            bounds.entry(option).or_insert(rect);
        }
        let scale = self.scale();
        for (&id, geometry) in self.layout_for_scroll() {
            if empty(geometry.rect) && empty(geometry.clip) {
                continue;
            }
            bounds.entry(id).or_insert(BoxRect {
                x: geometry.rect.x * scale,
                y: geometry.rect.y * scale,
                width: geometry.rect.width * scale,
                height: geometry.rect.height * scale,
            });
        }
        bounds
    }
}

#[cfg(test)]
mod tests {
    use crate::{Access, BatchWriter, Broker, TextMeasurer, View};
    use varta_common::view::{CssProperty as P, Tag};
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

    #[test]
    fn text_nodes_get_their_own_layout_boxes_in_plain_and_mixed_flows() {
        let mut b = BatchWriter::new();
        b.create_element(2, Tag::Div)
            .insert(1, 2, 0)
            .create_text(3, "Alone")
            .insert(2, 3, 0)
            .create_element(4, Tag::Div)
            .insert(1, 4, 0)
            .create_text(5, "Mixed ")
            .insert(4, 5, 0)
            .create_element(6, Tag::Span)
            .insert(4, 6, 0)
            .create_text(7, "flow")
            .insert(6, 7, 0);
        for id in [2, 4] {
            b.set_style(id, P::FontFamily, "\"Varta Fixture 400\"")
                .set_style(id, P::FontSize, "20px")
                .set_style(id, P::LineHeight, "20px");
        }
        let mut view = View::default();
        view.commit(&b.finish(), false, &mut Denied).unwrap();
        let mut font = TextMeasurer::new(
            include_bytes!("../../tests/fixtures/advance-400.ttf").to_vec(),
            "Varta Fixture 400",
        )
        .unwrap();
        let frame = view.rasterize_with_text(200, 100, &mut font, 20.0).unwrap();
        let bounds = frame.accessibility_bounds_map();
        let visible = frame.node_bounds_map();
        assert!(!visible.contains_key(&3));
        let alone = bounds[&3];
        assert!(alone.width > 0.0 && alone.height > 0.0);
        // Text in a mixed inline flow gets its own fragment box: six 8px
        // advances for "Mixed ", then the span's text over the span's box.
        assert!(!visible.contains_key(&5) && !visible.contains_key(&7));
        let mixed = bounds[&5];
        assert_eq!((mixed.x, mixed.width), (0.0, 48.0));
        assert_eq!(mixed.y, visible[&4].y);
        assert_eq!(bounds[&7], visible[&6]);
        for (id, rect) in &visible {
            assert_eq!(bounds[id], *rect);
        }
    }
}
