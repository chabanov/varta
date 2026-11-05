//! Platform accessibility bounds from one presented frame, in physical pixels.
use super::AccessibilityTree;
use crate::FrameMetadata;
use crate::layout::{BoxRect, intersect};
use std::collections::{BTreeMap, BTreeSet};

/// Bounds and clipping of the platform accessibility nodes of one frame.
///
/// A platform hit test returns the deepest node whose bounds contain the point
/// and searches a clipping node's descendants only inside that node's bounds.
/// Under that rule a point never reaches a node where it is not presented:
/// clipped and scrolled-out nodes keep their layout boxes only where the hit
/// test cannot reach them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AccessibilityGeometry {
    /// Physical bounds by source node.
    pub bounds: BTreeMap<u32, BoxRect>,
    /// Source nodes whose platform node clips its descendants to its bounds.
    pub clips: BTreeSet<u32>,
}

/// Half-open physical edges, as a platform rectangle built from a
/// [`BoxRect`] holds them: `x0 <= x < x1`, `y0 <= y < y1`.
#[derive(Debug, Clone, Copy)]
struct Edges {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}
impl Edges {
    fn of(rect: BoxRect) -> Self {
        Self {
            x0: rect.x,
            y0: rect.y,
            x1: rect.x + rect.width,
            y1: rect.y + rect.height,
        }
    }
    fn meet(self, other: Self) -> Self {
        Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        }
    }
    /// Some point lies in both.
    fn overlaps(self, other: Self) -> bool {
        let both = self.meet(other);
        both.x0 < both.x1 && both.y0 < both.y1
    }
    /// Every point of `self` lies in `outer`.
    fn within(self, outer: Self) -> bool {
        self.x0 >= outer.x0 && self.y0 >= outer.y0 && self.x1 <= outer.x1 && self.y1 <= outer.y1
    }
    fn union(self, other: Self) -> Self {
        Self {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }
}

fn empty(rect: BoxRect) -> bool {
    rect.width <= 0.0 || rect.height <= 0.0
}

impl FrameMetadata {
    /// Bounds and clipping for the platform nodes of `tree`, all from this
    /// frame:
    ///
    /// - a node with visible element fragments keeps their union
    ///   ([`Self::node_bounds_map`]);
    /// - a presented row of the open select list keeps that row;
    /// - another laid-out node that is partly presented (such as a text node,
    ///   whose layout box is its anonymous block or inline fragment box) gets
    ///   the presented part of its layout box;
    /// - a node that is not presented at all (scrolled out or clipped away, or
    ///   a row scrolled out of the open list) gets its unclipped layout box at
    ///   the presented scroll offsets, but only if the hit test cannot reach
    ///   that box inside the frame and its clipping ancestors. Otherwise it
    ///   stays unbounded, as do options of a closed select (no box).
    ///
    /// A clipping container (`overflow` hidden or scroll) clips when it has
    /// bounds and all presented descendant bounds lie inside them. A
    /// descendant that escapes it (an absolute box whose containing block is
    /// outside, or an open list drawn over it) keeps it from clipping.
    #[must_use]
    pub fn accessibility_geometry(&self, tree: &AccessibilityTree) -> AccessibilityGeometry {
        let mut bounds = self.node_bounds_map();
        let mut hidden = BTreeMap::new();
        for (option, rect, shown) in self.select_option_bounds() {
            if shown {
                bounds.entry(option).or_insert(rect);
            } else if !bounds.contains_key(&option) {
                hidden.insert(option, rect);
            }
        }
        let scale = self.scale();
        let scaled = |rect: BoxRect| BoxRect {
            x: rect.x * scale,
            y: rect.y * scale,
            width: rect.width * scale,
            height: rect.height * scale,
        };
        let mut containers = BTreeSet::new();
        for (&id, geometry) in self.layout_for_scroll() {
            if geometry.programmatic_scroll.is_some() {
                containers.insert(id);
            }
            if bounds.contains_key(&id) || (empty(geometry.rect) && empty(geometry.clip)) {
                continue;
            }
            let (rect, clip) = (scaled(geometry.rect), scaled(geometry.clip));
            let presented = intersect(rect, clip);
            if empty(presented) {
                hidden.entry(id).or_insert(rect);
            } else {
                bounds.insert(id, presented);
            }
        }
        let order = preorder(tree);
        let clips = clipping(tree, &order, &bounds, &containers);
        // Region the hit test can still reach below each node: the frame, cut
        // by every clipping ancestor.
        let frame = Edges {
            x0: 0.0,
            y0: 0.0,
            x1: self.width as f32,
            y1: self.height as f32,
        };
        let mut reach = BTreeMap::from([(tree.root, frame)]);
        for &id in &order {
            let Some(&region) = reach.get(&id) else {
                continue;
            };
            if let Some(&rect) = hidden.get(&id)
                && !Edges::of(rect).overlaps(region)
            {
                bounds.insert(id, rect);
            }
            let below = match bounds.get(&id) {
                Some(&own) if clips.contains(&id) => region.meet(Edges::of(own)),
                _ => region,
            };
            for child in tree.nodes.get(&id).map_or(&[][..], |node| &node.children) {
                reach.entry(*child).or_insert(below);
            }
        }
        AccessibilityGeometry { bounds, clips }
    }
}

/// Source nodes reachable from the root, parents before children, each once.
fn preorder(tree: &AccessibilityTree) -> Vec<u32> {
    let mut order = Vec::with_capacity(tree.nodes.len());
    let mut seen = BTreeSet::new();
    let mut pending = vec![tree.root];
    while let Some(id) = pending.pop() {
        let Some(node) = tree.nodes.get(&id) else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        order.push(id);
        pending.extend(node.children.iter().rev());
    }
    order
}

/// Clipping containers whose presented descendants all lie inside them.
fn clipping(
    tree: &AccessibilityTree,
    order: &[u32],
    bounds: &BTreeMap<u32, BoxRect>,
    containers: &BTreeSet<u32>,
) -> BTreeSet<u32> {
    // Union of presented bounds strictly below each node, children first.
    let mut below: BTreeMap<u32, Edges> = BTreeMap::new();
    let mut clips = BTreeSet::new();
    for &id in order.iter().rev() {
        let mut union: Option<Edges> = None;
        for child in tree.nodes.get(&id).map_or(&[][..], |node| &node.children) {
            let own = bounds.get(child).map(|rect| Edges::of(*rect));
            for part in [own, below.get(child).copied()].into_iter().flatten() {
                union = Some(union.map_or(part, |all| all.union(part)));
            }
        }
        if containers.contains(&id)
            && let Some(own) = bounds.get(&id)
            && union.is_none_or(|all| all.within(Edges::of(*own)))
        {
            clips.insert(id);
        }
        if let Some(union) = union {
            below.insert(id, union);
        }
    }
    clips
}

#[cfg(test)]
mod tests {
    use crate::{Access, BatchWriter, Broker, TextMeasurer, View};
    use varta_common::view::{Attribute as A, CssProperty as P, Tag};
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
    fn fixture_font() -> TextMeasurer {
        TextMeasurer::new(
            include_bytes!("../../tests/fixtures/advance-400.ttf").to_vec(),
            "Varta Fixture 400",
        )
        .unwrap()
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
        let frame = view
            .rasterize_with_text(200, 100, &mut fixture_font(), 20.0)
            .unwrap();
        let bounds = frame
            .accessibility_geometry(&view.accessibility_tree().unwrap())
            .bounds;
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

    /// A 20px heading, then a 50px pane of four 40px items; `pane` styles it.
    fn scroller(pane: &str) -> View {
        let mut b = BatchWriter::new();
        b.stylesheet(1, &format!(".pane {{{pane}}} .item {{height:40px}}"))
            .create_element(2, Tag::H1)
            .set_style(2, P::Height, "20px")
            .set_style(2, P::Margin, "0")
            .insert(1, 2, 0)
            .create_element(3, Tag::Div)
            .set_class(3, "pane")
            .insert(1, 3, 0);
        for index in 0..4 {
            let id = 10 + index;
            b.create_element(id, Tag::Section)
                .set_class(id, "item")
                .set_attribute(id, A::AriaLabel, &format!("Item {index}"))
                .insert(3, id, 0);
        }
        let mut view = View::default();
        view.commit(&b.finish(), false, &mut Denied).unwrap();
        view
    }

    #[test]
    fn hidden_boxes_are_published_only_where_clipping_ancestors_exclude_them() {
        let view = scroller("height:50px;overflow:hidden");
        let frame = view
            .rasterize_with_text(200, 200, &mut fixture_font(), 20.0)
            .unwrap();
        let geometry = frame.accessibility_geometry(&view.accessibility_tree().unwrap());
        assert!(geometry.clips.contains(&3));
        // The third item starts at 20 + 80, below the 20..70 pane.
        assert_eq!(geometry.bounds[&12].y, 100.0);
        assert!(!frame.node_bounds_map().contains_key(&12));
        // A border is inside the pane's bounds but outside its scrollport: a
        // hidden box under it could be hit there, so it stays unbounded.
        let view = scroller("height:50px;overflow:hidden;border-bottom:40px solid #000");
        let frame = view
            .rasterize_with_text(200, 200, &mut fixture_font(), 20.0)
            .unwrap();
        let geometry = frame.accessibility_geometry(&view.accessibility_tree().unwrap());
        assert!(geometry.clips.contains(&3));
        assert!(!geometry.bounds.contains_key(&12));
        // The fourth item lies wholly below the border.
        assert_eq!(geometry.bounds[&13].y, 140.0);
    }

    #[test]
    fn an_escaping_descendant_keeps_its_container_from_clipping() {
        let mut view = scroller("height:50px;overflow:hidden");
        let mut b = BatchWriter::new();
        // Absolute against the root's initial containing block: the pane
        // does not clip it, and it is presented below the pane.
        b.set_style(10, P::Position, "absolute")
            .set_style(10, P::Top, "120px")
            .set_style(10, P::Left, "0")
            .set_style(10, P::Width, "50px");
        view.commit(&b.finish(), false, &mut Denied).unwrap();
        let frame = view
            .rasterize_with_text(200, 200, &mut fixture_font(), 20.0)
            .unwrap();
        let geometry = frame.accessibility_geometry(&view.accessibility_tree().unwrap());
        assert!(!geometry.clips.contains(&3));
        assert_eq!(geometry.bounds[&10].y, 120.0);
        // Without the pane's clip, hidden items over the frame stay unbounded.
        assert!(!geometry.bounds.contains_key(&13));
    }
}
