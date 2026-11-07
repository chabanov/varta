use super::*;

/// Parent, children, class and inline style.
type Node = (Option<u32>, Vec<u32>, String, BTreeMap<CssProperty, String>);

/// A minimal document: a root with one child chain.
#[derive(Default)]
struct Tree {
    nodes: BTreeMap<u32, Node>,
    /// Tags other than `div`.
    tags: BTreeMap<u32, varta_common::view::Tag>,
    /// The focused element.
    focus: Option<u32>,
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
    fn focused(&self, id: u32) -> bool {
        self.focus == Some(id)
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

/// CSS Values 4 §6.1.1 with the first available font: a packaged face read
/// from its tables, the user-agent weight rows choosing its bold face, the
/// parent's font in `font-size`, `normal` line heights from the font's
/// rounded metrics, and the root's font for the root forms.
#[test]
fn font_relative_units_take_the_first_available_font() {
    let mut styles = Styles::default();
    styles.set_fonts(alloc::sync::Arc::new(fonts::tests::set()));
    styles
        .set(
            1,
            ".p { font-family: \"Varta Metrics\" } .a { width: 2ex; height: 1lh; margin-left: 1ch; padding-left: 1cap; padding-top: 1ic; font-size: 2ex } .b { width: 1rex; height: 1rlh; line-height: 2ex; margin-top: 1lh }",
            &mut 0,
        )
        .unwrap();
    let mut tree = Tree::chain(&["", "p", "a"]);
    let environment = environment(800.0);
    let leaf = ROOT_NODE + 2;
    let px = |computed: &BTreeMap<CssProperty, Computed>, property| match computed.get(&property) {
        Some(Computed::Value(value)) => value.strip_suffix("px").unwrap().parse::<f64>().unwrap(),
        other => panic!("{property:?}: {other:?}"),
    };
    let close = |a: f64, b: f64| assert!((a - b).abs() < 1e-9, "{a} != {b}");
    let values = styles.computed(&tree, leaf, &environment, |_, _| true);
    // `font-size: 2ex` of the parent's 16px face (x-height 520 units) is
    // 16.64px; the element's own units follow from that size.
    close(px(&values, CssProperty::FontSize), 16.64);
    close(px(&values, CssProperty::Width), 17.3056);
    // `normal`: 830, 170 and 90 units at 16.64px round to 14 + 3 + 1.
    close(px(&values, CssProperty::Height), 18.0);
    close(px(&values, CssProperty::MarginLeft), 9.2352);
    // `strong` is `bolder`: the bold face (x-height 540 units).
    tree.tags.insert(leaf, varta_common::view::Tag::Strong);
    let bold = styles.computed(&tree, leaf, &environment, |_, _| true);
    close(px(&bold, CssProperty::FontSize), 16.64);
    close(px(&bold, CssProperty::Width), 17.9712);
    // The root forms take the default family's face at the root size (the
    // set's default is "Varta Metrics"); `line-height: 2ex` is the
    // element's own x-height, and `lh` is then that length.
    let mut tree = Tree::chain(&["", "p", "b"]);
    tree.tags.clear();
    let values = styles.computed(&tree, leaf, &environment, |_, _| true);
    close(px(&values, CssProperty::Width), 8.32);
    close(px(&values, CssProperty::Height), 17.0);
    close(px(&values, CssProperty::LineHeight), 16.64);
    close(px(&values, CssProperty::MarginTop), 16.64);
    // Without any font: the fallbacks, and `normal` `lh` unresolved.
    let mut bare = styles.clone();
    bare.set_fonts(alloc::sync::Arc::new(fonts::FontSet::default()));
    let values = bare.computed(&Tree::chain(&["", "p", "a"]), leaf, &environment, |_, _| {
        true
    });
    close(px(&values, CssProperty::FontSize), 16.0);
    assert_eq!(values.get(&CssProperty::Height), None);
    assert_eq!(values.get(&CssProperty::PaddingLeft), None);
    close(px(&values, CssProperty::Width), 16.0);
}

/// A root with children `classes` (ids from `ROOT_NODE + 1`), and the last
/// child with one child of class `inner` (id `ROOT_NODE + 100`).
fn siblings(classes: &[&str], inner: &str) -> Tree {
    let mut tree = Tree::chain(&[""]);
    let mut last = ROOT_NODE;
    for (index, class) in classes.iter().enumerate() {
        let id = ROOT_NODE + 1 + index as u32;
        tree.nodes.insert(
            id,
            (
                Some(ROOT_NODE),
                Vec::new(),
                (*class).into(),
                BTreeMap::new(),
            ),
        );
        tree.nodes.get_mut(&ROOT_NODE).unwrap().1.push(id);
        last = id;
    }
    let child = ROOT_NODE + 100;
    tree.nodes.insert(
        child,
        (Some(last), Vec::new(), inner.into(), BTreeMap::new()),
    );
    tree.nodes.get_mut(&last).unwrap().1.push(child);
    tree
}

fn colour_of(sheet: &str, tree: &Tree, id: u32) -> Option<Computed> {
    let mut styles = Styles::default();
    styles.set(1, sheet, &mut 0).unwrap();
    styles
        .computed(tree, id, &environment(800.0), |property, value| {
            css::property_value(property, value)
        })
        .remove(&CssProperty::Color)
}

/// Selectors 4 §16 combinators, §14 structural and §4 logical
/// pseudo-classes, `:root, :host` and specificity, as Tailwind emits them.
#[test]
fn complex_selectors_match_by_the_tree() {
    let tree = siblings(&["a", "b item", "c item"], "d");
    let (a, b, c, d) = (ROOT_NODE + 1, ROOT_NODE + 2, ROOT_NODE + 3, ROOT_NODE + 100);
    let theme = ":root, :host { --c: #111 } .item { color: var(--c) }";
    assert_eq!(colour_of(theme, &tree, b), value("#111"));
    // Tailwind's `space-y-*` and `divide-*`: zero specificity, so a later
    // single class wins.
    let space = ":where(.list > :not(:last-child)) { color: #222 } .a { color: #333 }";
    let mut list = siblings(&["a", "b", "c"], "d");
    list.nodes.get_mut(&ROOT_NODE).unwrap().2 = "list".into();
    assert_eq!(colour_of(space, &list, a), value("#333"));
    assert_eq!(colour_of(space, &list, b), value("#222"));
    assert_eq!(colour_of(space, &list, c), None);
    // Structural pseudo-classes count element siblings; `of S` is outside
    // the subset.
    assert!(
        Styles::default()
            .set(1, ":nth-child(2 of .x) { color: #f00 }", &mut 0)
            .is_err()
    );
    let structural = ".item:first-child { color: #000 } :nth-child(odd) { color: #111 } .item:last-child { color: #444 } :only-child { color: #555 }";
    assert_eq!(colour_of(structural, &tree, a), value("#111"));
    assert_eq!(colour_of(structural, &tree, b), None);
    assert_eq!(colour_of(structural, &tree, c), value("#444"));
    assert_eq!(colour_of(structural, &tree, d), value("#555"));
    // Combinators: descendant, child, next and subsequent sibling.
    let combinators = ".a ~ .c { color: #010 } .a + .b { color: #020 } :root > .c .d { color: #030 } .a .d { color: #040 }";
    assert_eq!(colour_of(combinators, &tree, c), value("#010"));
    assert_eq!(colour_of(combinators, &tree, b), value("#020"));
    assert_eq!(colour_of(combinators, &tree, d), value("#030"));
    // `:is()` takes its most specific argument, `:where()` nothing, and
    // `:not()` negates; `.group:hover` needs a hovered ancestor.
    let logical = ":is(.c, .zz.yy.xx) { color: #100 } .c { color: #200 } :where(.c.item) { color: #300 } div:not(.a):not(.b) .d { color: #400 } :is(:where(.group):hover .d) { color: #500 }";
    assert_eq!(colour_of(logical, &tree, c), value("#100"));
    assert_eq!(colour_of(logical, &tree, d), value("#400"));
    // Type selectors match the element's tag; `:enabled` form controls.
    let mut typed = siblings(&["a"], "d");
    typed
        .tags
        .insert(ROOT_NODE + 100, varta_common::view::Tag::Button);
    assert_eq!(
        // Equal specificity: the later rule wins; `span.d` does not match.
        colour_of(
            "button.d { color: #600 } div .d { color: #700 } span.d { color: #900 }",
            &typed,
            d
        ),
        value("#700")
    );
    assert_eq!(
        colour_of(":enabled { color: #800 }", &typed, d),
        value("#800")
    );
    assert_eq!(colour_of(":enabled { color: #800 }", &typed, a), None);
    // `:focus` and `:focus-within` (Selectors 4 §9.2, §9.4).
    typed.focus = Some(d);
    let focus =
        ".a:focus-within { color: #a00 } .d:focus { color: #b00 } :root:focus { color: #c00 }";
    assert_eq!(colour_of(focus, &typed, a), value("#a00"));
    assert_eq!(colour_of(focus, &typed, d), value("#b00"));
    assert_eq!(colour_of(focus, &typed, ROOT_NODE), None);
}

/// Selectors that search the tree charge that search before values are
/// computed (`selectors::selector_work`).
#[test]
fn searching_selectors_charge_their_search() {
    let tree = siblings(&["a", "b", "c"], "d");
    let work = |sheet: &str| {
        let mut styles = Styles::default();
        styles.set(1, sheet, &mut 0).unwrap();
        styles.potential_cascade_work(&tree, usize::MAX).unwrap()
    };
    assert!(
        work(":is(.d) { color: #fff } * .d { color: #fff }")
            > work(".d { color: #fff } .d { color: #fff }")
    );
}
