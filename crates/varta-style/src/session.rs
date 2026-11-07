//! A host-neutral style session: the engine's own mirror of a view's node
//! tree, admission of the style-relevant operations inside a transaction,
//! the cascade budgets, and computed values for the committed tree. The
//! browser host drives it through the wasm32 build (`varta-style-wasm`),
//! and the cross-host tests drive the native build the same way, so both
//! hosts decide every style input with this code.
use crate::prelude::*;
use crate::{BudgetExceeded, Computed, Document, Element, Environment, SheetError, Styles};
use varta_common::view::{
    CssProperty, DRAFT_MAX_CASCADE_WORK, DRAFT_MAX_EXPANDED_VALUE_BYTES, ROOT_NODE, Tag, css,
};

/// The status of one session call (`0` is success).
pub mod status {
    pub const OK: u32 = 0;
    /// A class or style value outside the grammar.
    pub const INVALID: u32 = 1;
    /// A style value valid in CSS that the renderers cannot render yet.
    pub const UNRENDERABLE: u32 = 2;
    /// `font-family` naming a CSS-wide keyword the subset refuses.
    pub const FONT_FAMILY_KEYWORD: u32 = 3;
    /// Stylesheet refusals.
    pub const SHEET_ID: u32 = 4;
    pub const SHEET_INVALID: u32 = 5;
    pub const SHEET_WORK: u32 = 6;
    pub const SHEET_BYTES: u32 = 7;
    /// Cascade budgets.
    pub const CASCADE_WORK: u32 = 8;
    pub const EXPANDED_VALUES: u32 = 9;
    /// A call outside a transaction, an unknown node or property.
    pub const MISUSE: u32 = 10;
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Node {
    parent: Option<u32>,
    children: Vec<u32>,
    /// The element's tag; `None` for a text node.
    tag: Option<Tag>,
    class: String,
    style: BTreeMap<CssProperty, String>,
    disabled: bool,
    checked: bool,
}

/// The mirrored tree and the selector state.
#[derive(Debug, Clone, PartialEq)]
struct Mirror {
    nodes: BTreeMap<u32, Node>,
    styles: Styles,
    focus: u32,
    hovered: u32,
    active: u32,
}
impl Default for Mirror {
    fn default() -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            ROOT_NODE,
            Node {
                // The view root is a `div` (the L0 root container).
                tag: Some(Tag::Div),
                ..Node::default()
            },
        );
        Self {
            nodes,
            styles: Styles::default(),
            focus: 0,
            hovered: 0,
            active: 0,
        }
    }
}
impl Mirror {
    /// Whether `id` is `target` or one of its ancestors.
    fn contains(&self, target: u32, id: u32) -> bool {
        let mut at = Some(target).filter(|target| *target != 0);
        while let Some(node) = at {
            if node == id {
                return true;
            }
            at = self.nodes.get(&node).and_then(|node| node.parent);
        }
        false
    }
}
impl Document for Mirror {
    fn element(&self, id: u32) -> Option<Element<'_>> {
        self.nodes
            .get(&id)
            .and_then(|node| Some((node, node.tag?)))
            .map(|(node, tag)| Element {
                tag,
                class: &node.class,
                style: &node.style,
                disabled: node.disabled,
                checked: node.checked,
            })
    }
    fn parent(&self, id: u32) -> Option<u32> {
        self.nodes.get(&id).and_then(|node| node.parent)
    }
    fn children(&self, id: u32) -> &[u32] {
        self.nodes
            .get(&id)
            .map_or(&[], |node| node.children.as_slice())
    }
    fn top_level(&self) -> Vec<u32> {
        self.nodes
            .iter()
            .filter_map(|(id, node)| node.parent.is_none().then_some(*id))
            .collect()
    }
    fn focused(&self, id: u32) -> bool {
        self.focus == id
    }
    fn hovered(&self, id: u32) -> bool {
        self.contains(self.hovered, id)
    }
    fn active(&self, id: u32) -> bool {
        self.contains(self.active, id)
    }
}

/// Per-transaction allowances, as a native batch counts them.
#[derive(Debug, Clone, Copy, Default)]
struct Counters {
    stylesheet_work: usize,
    cascade_work: usize,
    expanded_value_bytes: usize,
}

/// One view's style session.
#[derive(Debug, Clone)]
pub struct Session {
    committed: Mirror,
    working: Option<(Mirror, Counters)>,
    environment: Environment,
}

impl Session {
    #[must_use]
    pub fn new(environment: Environment) -> Self {
        Self {
            committed: Mirror::default(),
            working: None,
            environment,
        }
    }
    fn tree(&mut self) -> Option<&mut Mirror> {
        self.working.as_mut().map(|(mirror, _)| mirror)
    }
    /// Start a transaction over a copy of the committed tree.
    pub fn begin(&mut self) {
        self.working = Some((self.committed.clone(), Counters::default()));
    }
    /// Publish the transaction.
    pub fn commit(&mut self) -> u32 {
        match self.working.take() {
            Some((mirror, _)) => {
                self.committed = mirror;
                status::OK
            }
            None => status::MISUSE,
        }
    }
    /// Drop the transaction; nothing of it remains.
    pub fn abort(&mut self) {
        self.working = None;
    }
    /// `create_element` (with its tag) / `create_text` (`None`).
    pub fn create(&mut self, id: u32, tag: Option<Tag>) -> u32 {
        let Some(tree) = self.tree() else {
            return status::MISUSE;
        };
        tree.nodes.insert(
            id,
            Node {
                tag,
                ..Node::default()
            },
        );
        status::OK
    }
    fn detach(tree: &mut Mirror, id: u32) {
        if let Some(parent) = tree.nodes.get_mut(&id).and_then(|node| node.parent.take())
            && let Some(parent) = tree.nodes.get_mut(&parent)
        {
            parent.children.retain(|child| *child != id);
        }
    }
    /// `insert`: `child` before `before` (0 appends) in `parent`.
    pub fn insert(&mut self, parent: u32, child: u32, before: u32) -> u32 {
        let Some(tree) = self.tree() else {
            return status::MISUSE;
        };
        if !tree.nodes.contains_key(&parent) || !tree.nodes.contains_key(&child) {
            return status::MISUSE;
        }
        Self::detach(tree, child);
        if let Some(node) = tree.nodes.get_mut(&child) {
            node.parent = Some(parent);
        }
        let Some(node) = tree.nodes.get_mut(&parent) else {
            return status::MISUSE;
        };
        let at = node
            .children
            .iter()
            .position(|sibling| *sibling == before)
            .unwrap_or(node.children.len());
        node.children.insert(at, child);
        status::OK
    }
    /// `remove`: the node and its subtree.
    pub fn remove(&mut self, id: u32) -> u32 {
        let Some(tree) = self.tree() else {
            return status::MISUSE;
        };
        Self::detach(tree, id);
        let mut pending = vec![id];
        while let Some(node) = pending.pop() {
            if let Some(node) = tree.nodes.remove(&node) {
                pending.extend(node.children);
            }
        }
        status::OK
    }
    /// `set_class`: the HTML class model.
    pub fn set_class(&mut self, id: u32, text: &str) -> u32 {
        let Some(tree) = self.tree() else {
            return status::MISUSE;
        };
        if !css::class_list(text) {
            return status::INVALID;
        }
        match tree.nodes.get_mut(&id) {
            Some(node) => {
                node.class = text.to_owned();
                status::OK
            }
            None => status::MISUSE,
        }
    }
    /// `set_style`: a value of the property with code `code`, stored as
    /// [`css::normalize`] leaves it.
    pub fn set_style(&mut self, id: u32, code: u16, value: &str) -> u32 {
        let Some(property) = CssProperty::from_code(code) else {
            return status::MISUSE;
        };
        let Some(tree) = self.tree() else {
            return status::MISUSE;
        };
        if let Err(refusal) = css::check(property, value) {
            return match refusal {
                css::Refusal::Unrenderable => status::UNRENDERABLE,
                css::Refusal::Invalid
                    if property == CssProperty::FontFamily
                        && css::font_family_css_wide_keyword(value).is_some() =>
                {
                    status::FONT_FAMILY_KEYWORD
                }
                css::Refusal::Invalid => status::INVALID,
            };
        }
        let stored = if varta_common::view::syntax::needs_serialization(value) {
            css::normalize(value)
        } else {
            value.to_owned()
        };
        match tree.nodes.get_mut(&id) {
            Some(node) => {
                node.style.insert(property, stored);
                status::OK
            }
            None => status::MISUSE,
        }
    }
    /// `remove_style`.
    pub fn remove_style(&mut self, id: u32, code: u16) -> u32 {
        let Some(property) = CssProperty::from_code(code) else {
            return status::MISUSE;
        };
        let Some(tree) = self.tree() else {
            return status::MISUSE;
        };
        if let Some(node) = tree.nodes.get_mut(&id) {
            node.style.remove(&property);
        }
        status::OK
    }
    /// The `:disabled`/`:checked` state of an element, in the transaction
    /// or, outside one, in the committed tree (a user's toggle).
    pub fn set_flags(&mut self, id: u32, disabled: bool, checked: bool) -> u32 {
        let tree = match self.working.as_mut() {
            Some((mirror, _)) => mirror,
            None => &mut self.committed,
        };
        match tree.nodes.get_mut(&id) {
            Some(node) => {
                node.disabled = disabled;
                node.checked = checked;
                status::OK
            }
            None => status::MISUSE,
        }
    }
    /// `stylesheet`.
    pub fn stylesheet(&mut self, sheet: u32, text: &str) -> u32 {
        let Some((tree, counters)) = self.working.as_mut() else {
            return status::MISUSE;
        };
        match tree.styles.set(sheet, text, &mut counters.stylesheet_work) {
            Ok(()) => status::OK,
            Err(SheetError::InvalidId) => status::SHEET_ID,
            Err(SheetError::Invalid(_)) => status::SHEET_INVALID,
            Err(SheetError::WorkLimit) => status::SHEET_WORK,
            Err(SheetError::TooLarge) => status::SHEET_BYTES,
        }
    }
    /// Charge the cascade budgets of the transaction's tree, as a native
    /// batch does before a style-reading command and at its end.
    pub fn charge(&mut self) -> u32 {
        let Some((tree, counters)) = self.working.as_mut() else {
            return status::MISUSE;
        };
        let work = match tree.styles.potential_cascade_work(
            &*tree,
            DRAFT_MAX_CASCADE_WORK.saturating_sub(counters.cascade_work),
        ) {
            Ok(work) => work,
            Err(BudgetExceeded) => return status::CASCADE_WORK,
        };
        counters.cascade_work = counters.cascade_work.saturating_add(work);
        let bytes = match tree.styles.potential_expanded_values(
            &*tree,
            DRAFT_MAX_EXPANDED_VALUE_BYTES.saturating_sub(counters.expanded_value_bytes),
        ) {
            Ok(bytes) => bytes,
            Err(BudgetExceeded) => return status::EXPANDED_VALUES,
        };
        counters.expanded_value_bytes = counters.expanded_value_bytes.saturating_add(bytes);
        status::OK
    }
    /// The recorded media and lowering inputs.
    pub fn set_environment(&mut self, environment: Environment) {
        self.environment = environment;
    }
    /// The recorded selector state of the committed tree (0 is none).
    pub fn set_state(&mut self, focus: u32, hovered: u32, active: u32) {
        self.committed.focus = focus;
        self.committed.hovered = hovered;
        self.committed.active = active;
    }
    /// The committed computed values of element `id` (empty for a text
    /// node or an unknown id).
    #[must_use]
    pub fn computed(&self, id: u32) -> BTreeMap<CssProperty, Computed> {
        if self.committed.element(id).is_none() {
            return BTreeMap::new();
        }
        self.committed
            .styles
            .computed(&self.committed, id, &self.environment, css::property_value)
    }
    /// Every committed element in document order from the root.
    #[must_use]
    pub fn attached(&self) -> Vec<u32> {
        let mut out = Vec::new();
        let mut pending = vec![ROOT_NODE];
        while let Some(id) = pending.pop() {
            if self.committed.element(id).is_some() {
                out.push(id);
            }
            pending.extend(self.committed.children(id).iter().rev());
        }
        out
    }
    /// The committed sheets' texts, for a host that installs `@property`
    /// registrations or inspects them.
    #[must_use]
    pub fn styles(&self) -> &Styles {
        &self.committed.styles
    }
}

/// The wire form of computed values: per property its `u16` code, a `u32`
/// byte length and the UTF-8 text (`inherit`/`initial` for the keywords),
/// little-endian.
#[must_use]
pub fn encode_computed(values: &BTreeMap<CssProperty, Computed>) -> Vec<u8> {
    let mut out = Vec::new();
    for (property, value) in values {
        let text = value.text();
        out.extend_from_slice(&property.code().to_le_bytes());
        out.extend_from_slice(&u32::try_from(text.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(text.as_bytes());
    }
    out
}

/// The recorded media inputs as one bit set, the form a host passes them
/// across the Wasm boundary.
pub mod inputs {
    pub const DARK: u32 = 1;
    pub const REDUCED_MOTION: u32 = 1 << 1;
    pub const FORCED_COLORS: u32 = 1 << 2;
    /// `prefers-contrast`: 0 no preference, 1 more, 2 less, 3 custom.
    pub const CONTRAST_SHIFT: u32 = 3;
    /// `hover: none` / `any-hover: none` (else `hover`).
    pub const NO_HOVER: u32 = 1 << 5;
    pub const NO_ANY_HOVER: u32 = 1 << 6;
    /// `pointer` / `any-pointer`: 0 fine, 1 coarse, 2 none.
    pub const POINTER_SHIFT: u32 = 7;
    pub const ANY_POINTER_SHIFT: u32 = 9;
    /// Two-bit fields.
    pub const FIELD: u32 = 0b11;
    /// The generic `monospace` names a packaged face, so it does not keep
    /// the fixed-font size ([`crate::font::FontBase`]).
    pub const PACKAGED_MONOSPACE: u32 = 1 << 11;
}

/// The environment of recorded inputs: the viewport, the root and medium
/// font sizes in CSS pixels and the [`inputs`] bit set.
#[must_use]
pub fn environment(
    (width, height): (f64, f64),
    root_font_size: f64,
    medium_font_size: f64,
    flags: u32,
) -> Environment {
    use varta_common::view::stylesheet::media::{Contrast, Hover, Pointer, Preferences};
    let pointer = |shift: u32| match (flags >> shift) & inputs::FIELD {
        1 => Pointer::Coarse,
        2 => Pointer::None,
        _ => Pointer::Fine,
    };
    let hover = |bit: u32| {
        if flags & bit != 0 {
            Hover::None
        } else {
            Hover::Hover
        }
    };
    Environment {
        media: varta_common::view::stylesheet::Media {
            width,
            height,
            dark: flags & inputs::DARK != 0,
            preferences: Preferences {
                reduced_motion: flags & inputs::REDUCED_MOTION != 0,
                contrast: match (flags >> inputs::CONTRAST_SHIFT) & inputs::FIELD {
                    1 => Contrast::More,
                    2 => Contrast::Less,
                    3 => Contrast::Custom,
                    _ => Contrast::NoPreference,
                },
                forced_colors: flags & inputs::FORCED_COLORS != 0,
                hover: hover(inputs::NO_HOVER),
                any_hover: hover(inputs::NO_ANY_HOVER),
                pointer: pointer(inputs::POINTER_SHIFT),
                any_pointer: pointer(inputs::ANY_POINTER_SHIFT),
            },
        },
        context: css::Context {
            root_font_size,
            viewport_width: width,
            viewport_height: height,
            font: None,
        },
        font: crate::font::FontBase {
            medium: medium_font_size,
            fixed_monospace: flags & inputs::PACKAGED_MONOSPACE == 0,
        },
    }
}
