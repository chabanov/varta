use super::*;

/// Parent, children, class and inline style.
type Node = (Option<u32>, Vec<u32>, String, BTreeMap<CssProperty, String>);

/// A minimal document: a root with one child chain.
#[derive(Default)]
struct Tree {
    nodes: BTreeMap<u32, Node>,
}
impl Tree {
    fn chain(classes: &[&str]) -> Self {
        let mut tree = Self::default();
        let mut parent = None;
        for (index, class) in classes.iter().enumerate() {
            let id = ROOT_NODE + index as u32;
            tree.nodes
                .insert(id, (parent, Vec::new(), (*class).into(), BTreeMap::new()));
            if let Some(parent) = parent {
                tree.nodes.get_mut(&parent).unwrap().1.push(id);
            }
            parent = Some(id);
        }
        tree
    }
}
impl Document for Tree {
    fn element(&self, id: u32) -> Option<Element<'_>> {
        self.nodes.get(&id).map(|(_, _, class, style)| Element {
            class,
            style,
            disabled: false,
            checked: false,
        })
    }
    fn parent(&self, id: u32) -> Option<u32> {
        self.nodes.get(&id).and_then(|node| node.0)
    }
    fn children(&self, id: u32) -> &[u32] {
        self.nodes.get(&id).map_or(&[], |node| &node.1)
    }
    fn top_level(&self) -> Vec<u32> {
        vec![ROOT_NODE]
    }
    fn focused(&self, _: u32) -> bool {
        false
    }
    fn hovered(&self, _: u32) -> bool {
        false
    }
    fn active(&self, _: u32) -> bool {
        false
    }
}

fn environment(width: f64) -> Environment {
    Environment {
        media: Media {
            width,
            height: 600.0,
            dark: false,
            preferences: stylesheet::media::Preferences::default(),
        },
        context: Context::NOMINAL,
    }
}

fn computed(sheet: &str, classes: &[&str], width: f64) -> BTreeMap<CssProperty, Computed> {
    let mut styles = Styles::default();
    styles.set(1, sheet, &mut 0).unwrap();
    let tree = Tree::chain(classes);
    let leaf = ROOT_NODE + classes.len() as u32 - 1;
    styles.computed(&tree, leaf, &environment(width), |property, value| {
        css::property_value(property, value)
    })
}
fn color(sheet: &str) -> Option<Computed> {
    computed(sheet, &["", "t"], 800.0).remove(&CssProperty::Color)
}
fn value(text: &str) -> Option<Computed> {
    Some(Computed::Value(text.into()))
}

#[test]
fn importance_reverses_layers_and_beats_inline_values() {
    // Cascade 5 §6.1, §6.4.3.
    assert_eq!(
        color(
            "@layer a, b; @layer a { .t { color: #111 !important } } @layer b { .t { color: #222 !important } } .t { color: #333 !important }"
        ),
        value("#111")
    );
    assert_eq!(
        color(
            "@layer a, b; @layer a { .t { color: #111 } } @layer b { .t { color: #222 } } .t { color: #333 }"
        ),
        value("#333")
    );
    let mut styles = Styles::default();
    styles
        .set(
            1,
            ".t { color: #111 !important; background-color: #222 }",
            &mut 0,
        )
        .unwrap();
    let mut tree = Tree::chain(&["", "t"]);
    let node = tree.nodes.get_mut(&2).unwrap();
    node.3.insert(CssProperty::Color, "#999".into());
    node.3.insert(CssProperty::BackgroundColor, "#888".into());
    let out = styles.computed(&tree, 2, &environment(800.0), |p, v| {
        css::property_value(p, v)
    });
    assert_eq!(out[&CssProperty::Color], Computed::Value("#111".into()));
    assert_eq!(
        out[&CssProperty::BackgroundColor],
        Computed::Value("#888".into())
    );
}

#[test]
fn css_wide_keywords_follow_cascade_5() {
    assert_eq!(color(".t { color: inherit }"), Some(Computed::Inherit));
    assert_eq!(color(".t { color: initial }"), Some(Computed::Initial));
    assert_eq!(color(".t { color: unset }"), Some(Computed::Inherit));
    assert_eq!(
        computed(".t { display: unset }", &["", "t"], 800.0).remove(&CssProperty::Display),
        Some(Computed::Initial)
    );
    assert_eq!(color(".t { color: #111 } .t.t { color: revert }"), None);
    // revert-layer rolls back to earlier layers, then unlayered to layers.
    assert_eq!(
        color(
            "@layer a, b; @layer a { .t { color: #111 } } @layer b { .t { color: revert-layer } }"
        ),
        value("#111")
    );
    assert_eq!(
        color("@layer a { .t { color: #111 } } .t { color: #222 } .t.t { color: revert-layer }"),
        value("#111")
    );
    assert_eq!(color(".t { color: revert-layer }"), None);
}

#[test]
fn conditional_layers_count_only_while_their_condition_holds() {
    // Cascade 5 §6.4.3: `b` is declared first only in wide viewports.
    let sheet = "@media (width >= 500px) { @layer b; } @layer a { .t { color: #111 } } @layer b { .t { color: #222 } }";
    assert_eq!(
        computed(sheet, &["", "t"], 800.0).remove(&CssProperty::Color),
        value("#111")
    );
    assert_eq!(
        computed(sheet, &["", "t"], 300.0).remove(&CssProperty::Color),
        value("#222")
    );
}

#[test]
fn unrelated_and_duplicate_anchors_do_not_expand_candidate_lists() {
    let mut index = CascadeIndex::default();
    for sheet in 1..1001 {
        let parsed = stylesheet::parse_with_work_limit(
            &format!(".unrelated{sheet} {{ color:#abc }}"),
            usize::MAX,
        )
        .unwrap();
        index.insert(sheet, &RuleIndex::new(&parsed.rules));
    }
    let rules = stylesheet::parse_with_work_limit(".a, .a.b, .b, :root { color:#fff }", usize::MAX)
        .unwrap()
        .rules;
    let anchors = RuleIndex::new(&rules);
    index.insert(2000, &anchors);
    assert_eq!(
        index.candidates(false, &BTreeSet::from(["a", "b"])),
        vec![(2000, 0)]
    );
    assert_eq!(index.candidates(true, &BTreeSet::new()), vec![(2000, 0)]);
    index.remove(2000, &anchors);
    assert!(
        index
            .candidates(false, &BTreeSet::from(["a", "b"]))
            .is_empty()
    );
    assert!(index.candidates(true, &BTreeSet::new()).is_empty());
}
