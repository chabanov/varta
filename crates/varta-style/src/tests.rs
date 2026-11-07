use super::*;

/// Parent, children, class and inline style.
type Node = (Option<u32>, Vec<u32>, String, BTreeMap<CssProperty, String>);

/// A minimal document: a root with one child chain.
#[derive(Default)]
struct Tree {
    nodes: BTreeMap<u32, Node>,
    /// Tags other than `div`.
    tags: BTreeMap<u32, varta_common::view::Tag>,
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
            tag: self
                .tags
                .get(&id)
                .copied()
                .unwrap_or(varta_common::view::Tag::Div),
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
        context: Context {
            font: None,
            ..Context::NOMINAL
        },
        font: font::FontBase::default(),
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
    // CSS Images 3 §5.1: `image-orientation` is inherited, so `unset`
    // inherits; `object-fit` is not, so it resets.
    let mut out = computed(
        ".t { image-orientation: unset; object-fit: unset }",
        &["", "t"],
        800.0,
    );
    assert_eq!(
        out.remove(&CssProperty::ImageOrientation),
        Some(Computed::Inherit)
    );
    assert_eq!(out.remove(&CssProperty::ObjectFit), Some(Computed::Initial));
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

/// The computed values of the leaf of a chain whose elements have `tags`.
fn computed_tagged(
    sheet: &str,
    classes: &[&str],
    tags: &[(u32, varta_common::view::Tag)],
) -> BTreeMap<CssProperty, Computed> {
    let mut styles = Styles::default();
    styles.set(1, sheet, &mut 0).unwrap();
    let mut tree = Tree::chain(classes);
    tree.tags.extend(tags.iter().copied());
    let leaf = ROOT_NODE + classes.len() as u32 - 1;
    styles.computed(&tree, leaf, &environment(800.0), |property, value| {
        css::property_value(property, value)
    })
}

#[test]
fn font_relative_units_resolve_against_the_computed_font() {
    use varta_common::view::Tag;
    let width = |sheet: &str, tags: &[(u32, Tag)]| {
        computed_tagged(sheet, &["", "p", "t"], tags).remove(&CssProperty::Width)
    };
    // Values 4 §6.1.1: `em` is the element's computed font size.
    assert_eq!(
        width(".p { font-size: 20px } .t { width: 2em }", &[]),
        value("40px")
    );
    assert_eq!(
        width(".p { font-size: 2em } .t { width: calc(1em + 1px) }", &[]),
        value("33px")
    );
    // The user-agent rows: `h1` is 2em, `code` switches to the fixed
    // monospace size, a control has the small-control font.
    assert_eq!(width(".t { width: 1em }", &[(3, Tag::H1)]), value("32px"));
    assert_eq!(width(".t { width: 1em }", &[(3, Tag::Code)]), value("13px"));
    assert_eq!(
        width(".t { width: 3em }", &[(3, Tag::Input)]),
        value("40px")
    );
    // `em` in `font-size` is the parent's size; a sum is absolute.
    assert_eq!(
        width(
            ".p { font-size: 10px } .t { font-size: calc(1em + 2px); width: 1em }",
            &[]
        ),
        value("12px")
    );
    // `lh` is the computed line height, a number times the font size; in
    // `line-height` itself, `lh` is the parent's.
    let heights = computed_tagged(
        ".p { font-size: 10px; line-height: 2 } .t { font-size: 20px; line-height: calc(1lh + 5px); height: 2lh; width: 1lh }",
        &["", "p", "t"],
        &[],
    );
    assert_eq!(
        heights[&CssProperty::LineHeight],
        Computed::Value("25px".into())
    );
    assert_eq!(
        heights[&CssProperty::Height],
        Computed::Value("50px".into())
    );
    // `lh` with `line-height: normal` needs the font's metrics.
    assert_eq!(width(".t { width: 1lh }", &[]), None);
    // `em` in `line-height` is the element's own size.
    assert_eq!(
        computed_tagged(
            ".p { font-size: 10px } .t { font-size: 20px; line-height: 1.5em }",
            &["", "p", "t"],
            &[]
        )
        .remove(&CssProperty::LineHeight),
        value("30px")
    );
}

#[test]
fn registered_lengths_compute_to_absolute_lengths_before_inheritance() {
    // Properties and Values API 1 §2.4: `2em` computes at the declaring
    // element (10px font), so the child inherits 20px, not 2 × its 20px.
    let sheet = "@property --gap { syntax: \"<length>\"; inherits: true; initial-value: 0px }
        @property --pair { syntax: \"<length>+\"; inherits: true; initial-value: 0px }
        @property --raw { syntax: \"*\"; inherits: true }
        .p { font-size: 10px; --gap: 2em; --pair: 1em calc(1em + 1rem); --raw: 2em }
        .t { font-size: 20px; margin-top: var(--gap); padding: var(--pair); margin-bottom: var(--raw) }";
    let out = computed_tagged(sheet, &["", "p", "t"], &[]);
    assert_eq!(out[&CssProperty::MarginTop], Computed::Value("20px".into()));
    assert_eq!(
        out[&CssProperty::Padding],
        Computed::Value("10px 26px".into())
    );
    // An unregistered value is a token sequence: `em` resolves where used.
    assert_eq!(
        out[&CssProperty::MarginBottom],
        Computed::Value("40px".into())
    );
}
