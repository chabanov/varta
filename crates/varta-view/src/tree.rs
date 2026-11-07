//! The live node tree and the semantic rules of every operation. A batch is
//! applied to a copy of the tree; only a batch whose every operation and
//! final invariant holds replaces the live tree (fail closed, atomic).
use crate::style::{self, Environment};
use crate::wire::{Operation, Value};
use crate::{ViewError, Violation};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use varta_common::view::attributes as values;
use varta_common::view::{
    ANY_KEY, Attribute, AttributeValue, Command, CssProperty, DRAFT_MAX_LIVE_LISTENERS,
    DRAFT_MAX_LIVE_NODES, DRAFT_MAX_PENDING_COMMANDS, EventKind, ListenOption, MODIFIER_MASK,
    NO_NODE, Opcode, ROOT_NODE, Tag, Tags,
};

/// One `listen` registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Listener {
    pub kind: EventKind,
    pub handler: u32,
    pub filter: crate::Filter,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Preedit {
    pub text: String,
    pub cursor: Option<(usize, usize)>,
}
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct EditorState {
    /// The control's user validity (HTML §4.10.18.4.1): set by a trusted
    /// `change` and by its form's submission (`:user-valid`).
    pub user_validity: bool,
    pub anchor: usize,
    pub focus: usize,
    pub preedit: Option<Preedit>,
    pub history_epoch: Arc<()>,
    /// Host text-range generation; allocated on the first content/type change.
    pub text_epoch: Option<Arc<()>>,
    pub manual_text_scroll: bool,
    /// Assistive display panning; ordinary caret navigation restores following.
    pub horizontal_scroll: Option<f32>,
    pub preferred_x: Option<f32>,
    pub upstream: bool,
    pub option_selected: bool,
    pub option_dirty: bool,
    pub select_popup: Option<crate::select::SelectPopup>,
    /// Sticky across type changes: a local/deferred edit must not alter capacity.
    pub value_reserved: bool,
}
impl EditorState {
    pub fn reset_navigation(&mut self) {
        self.horizontal_scroll = None;
        self.preferred_x = None;
        self.upstream = false;
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct OutlineStyle {
    pub values: BTreeMap<CssProperty, (CssProperty, String)>,
}

/// Last inline writer for each border color, including shorthand resets.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct BorderColorStyle {
    pub values: BTreeMap<u8, (CssProperty, String)>,
}

/// An element's custom data attributes (`set_data`), by name.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct DataAttributes {
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Element {
    pub tag: Tag,
    pub attributes: BTreeMap<Attribute, String>,
    /// Custom data attributes (`set_data`), by name.
    pub data: Box<DataAttributes>,
    pub class: String,
    pub style: BTreeMap<CssProperty, String>,
    /// CSSOM-like last writer for each geometry longhand, kept separately so
    /// shorthand updates and removals do not depend on enum sorting.
    pub layout_inline: BTreeMap<CssProperty, (CssProperty, String, usize)>,
    /// Last inline writer for each border side and component (width/style).
    pub border_inline: BTreeMap<(u8, bool), (CssProperty, String)>,
    pub border_color_inline: Box<BorderColorStyle>,
    pub outline_inline: Box<OutlineStyle>,
    pub listeners: Vec<Listener>,
    /// The live value of `input`, `textarea` and `select` (`set_value`).
    pub value: String,
    /// Selection endpoints are UTF-8 byte offsets in the current value.
    pub editor: Box<EditorState>,
    pub checked: bool,
    /// The field edit counter (`seq`).
    pub seq: u32,
    pub scroll_top: f64,
    /// `dialog`: shown modally.
    pub open: bool,
}
impl Element {
    pub(crate) fn new(tag: Tag) -> Self {
        Self {
            tag,
            attributes: BTreeMap::new(),
            data: Box::default(),
            class: String::new(),
            style: BTreeMap::new(),
            layout_inline: BTreeMap::new(),
            border_inline: BTreeMap::new(),
            border_color_inline: Box::default(),
            outline_inline: Box::default(),
            listeners: Vec::new(),
            value: String::new(),
            editor: Box::default(),
            checked: false,
            seq: 0,
            scroll_top: 0.0,
            open: false,
        }
    }
    pub(crate) fn input_type(&self) -> &str {
        self.attributes
            .get(&Attribute::Type)
            .map_or("text", String::as_str)
    }
    pub(crate) fn checkable(&self) -> bool {
        self.tag == Tag::Input && matches!(self.input_type(), "checkbox" | "radio")
    }
    /// A field with a live text value (`set_value`).
    pub(crate) fn text_field(&self) -> bool {
        (self.tag == Tag::Input && !self.checkable()) || self.tag == Tag::Textarea
    }
    pub(crate) fn disabled(&self) -> bool {
        self.attributes.contains_key(&Attribute::Disabled)
    }
    pub(crate) fn readonly(&self) -> bool {
        self.attributes.contains_key(&Attribute::Readonly)
    }
    pub(crate) fn focusable(&self) -> bool {
        matches!(
            self.tag,
            Tag::Button | Tag::Input | Tag::Select | Tag::Textarea | Tag::A
        ) || self.attributes.contains_key(&Attribute::Tabindex)
    }
}

pub(crate) fn geometry_sides(property: CssProperty) -> Option<&'static [CssProperty]> {
    use CssProperty as P;
    Some(match property {
        P::Margin => &[P::MarginTop, P::MarginRight, P::MarginBottom, P::MarginLeft],
        P::Padding => &[
            P::PaddingTop,
            P::PaddingRight,
            P::PaddingBottom,
            P::PaddingLeft,
        ],
        P::Gap => &[P::RowGap, P::ColumnGap],
        P::MarginTop => &[P::MarginTop],
        P::MarginRight => &[P::MarginRight],
        P::MarginBottom => &[P::MarginBottom],
        P::MarginLeft => &[P::MarginLeft],
        P::PaddingTop => &[P::PaddingTop],
        P::PaddingRight => &[P::PaddingRight],
        P::PaddingBottom => &[P::PaddingBottom],
        P::PaddingLeft => &[P::PaddingLeft],
        P::RowGap => &[P::RowGap],
        P::ColumnGap => &[P::ColumnGap],
        P::Overflow => &[P::OverflowX, P::OverflowY],
        P::OverflowX => &[P::OverflowX],
        P::OverflowY => &[P::OverflowY],
        _ => return None,
    })
}

pub(crate) fn border_components(property: CssProperty) -> Vec<(u8, bool)> {
    use CssProperty as P;
    let (sides, width, style): (&[u8], bool, bool) = match property {
        P::Border => (&[0, 1, 2, 3], true, true),
        P::BorderTop => (&[0], true, true),
        P::BorderRight => (&[1], true, true),
        P::BorderBottom => (&[2], true, true),
        P::BorderLeft => (&[3], true, true),
        P::BorderWidth => (&[0, 1, 2, 3], true, false),
        P::BorderStyle => (&[0, 1, 2, 3], false, true),
        P::BorderTopWidth => (&[0], true, false),
        P::BorderRightWidth => (&[1], true, false),
        P::BorderBottomWidth => (&[2], true, false),
        P::BorderLeftWidth => (&[3], true, false),
        P::BorderTopStyle => (&[0], false, true),
        P::BorderRightStyle => (&[1], false, true),
        P::BorderBottomStyle => (&[2], false, true),
        P::BorderLeftStyle => (&[3], false, true),
        _ => return Vec::new(),
    };
    sides
        .iter()
        .flat_map(|side| [(width, (*side, false)), (style, (*side, true))])
        .filter_map(|(enabled, key)| enabled.then_some(key))
        .collect()
}

pub(crate) fn border_color_sides(property: CssProperty) -> &'static [u8] {
    use CssProperty as P;
    match property {
        P::Border | P::BorderColor => &[0, 1, 2, 3],
        P::BorderTop | P::BorderTopColor => &[0],
        P::BorderRight | P::BorderRightColor => &[1],
        P::BorderBottom | P::BorderBottomColor => &[2],
        P::BorderLeft | P::BorderLeftColor => &[3],
        _ => &[],
    }
}

pub(crate) fn outline_components(property: CssProperty) -> &'static [CssProperty] {
    use CssProperty as P;
    match property {
        P::Outline => &[P::OutlineWidth, P::OutlineStyle, P::OutlineColor],
        P::OutlineWidth => &[P::OutlineWidth],
        P::OutlineStyle => &[P::OutlineStyle],
        P::OutlineColor => &[P::OutlineColor],
        P::OutlineOffset => &[P::OutlineOffset],
        _ => &[],
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Content {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
    pub identity: Arc<()>,
    pub content: Content,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    /// A box the renderer generated for a pseudo-element
    /// ([`crate::generated`]); `None` for the guest's nodes.
    pub generated: Option<crate::generated::Generated>,
}

/// A host-side consequence of an applied batch, performed after the swap.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Effect {
    Copy {
        request: u32,
        text: String,
    },
    /// A record caused by a guest command (`focusin`, `focusout`, `close`).
    Record(crate::EventRecord),
    /// A queued scroll belongs to this particular element, even if its ID is reused.
    ScrollRecord {
        record: crate::EventRecord,
        identity: Arc<()>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tree {
    pub render_reads: crate::render::ReadCache,
    pub nodes: BTreeMap<u32, Node>,
    /// Live handler ids and the node holding each.
    pub handlers: BTreeMap<u32, u32>,
    /// The stylesheets and the cascade (the shared style engine).
    pub styles: varta_style::Styles,
    pub title: String,
    pub focus: Option<u32>,
    pub hovered: Option<crate::NodeIdentity>,
    pub active: Option<crate::NodeIdentity>,
    /// `copy_text` requests whose `command_result` is not yet delivered.
    pub pending_requests: BTreeSet<u32>,
    /// The top layer: modal dialogs in the order they were shown, the
    /// topmost last.
    pub modals: Vec<Modal>,
    /// The animation clock and running CSS transitions.
    pub animations: crate::transition::Animations,
    /// The host's packaged images: what an `img` may name and draws. With
    /// none, no `img` is laid out.
    pub images: crate::image::Images,
    /// Shown overlay scrollbars on the same clock.
    pub scrollbars: crate::overlay_scrollbar::OverlayScrollbars,
}

/// A modal dialog in the top layer and the element focused before it was
/// shown, which closing it focuses again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Modal {
    pub dialog: u32,
    pub restore: Option<u32>,
}

impl Default for Tree {
    fn default() -> Self {
        let root = Node {
            identity: Arc::new(()),
            content: Content::Element(Element::new(Tag::Div)),
            parent: None,
            children: Vec::new(),
            generated: None,
        };
        Self {
            render_reads: crate::render::ReadCache::default(),
            nodes: BTreeMap::from([(ROOT_NODE, root)]),
            handlers: BTreeMap::new(),
            styles: varta_style::Styles::default(),
            title: String::new(),
            focus: None,
            hovered: None,
            active: None,
            pending_requests: BTreeSet::new(),
            modals: Vec::new(),
            animations: crate::transition::Animations::default(),
            images: crate::image::Images::default(),
            scrollbars: crate::overlay_scrollbar::OverlayScrollbars::default(),
        }
    }
}

/// What a commit needs from its host: the patch-time external-link check
/// against the package request.
pub(crate) struct Context<'a> {
    pub link_requested: &'a dyn Fn(&str) -> bool,
    pub environment: Environment,
    pub layout: Option<LayoutContext<'a>>,
}

pub(crate) struct LayoutContext<'a> {
    pub width: f32,
    pub height: f32,
    pub font: Option<(&'a mut crate::TextMeasurer, f32)>,
}

struct Apply<'tree, 'context, 'input> {
    tree: &'tree mut Tree,
    context: &'context mut Context<'input>,
    effects: Vec<Effect>,
    index: usize,
    layout_commands: usize,
    select_work: usize,
    radio_work: usize,
    stylesheet_work: usize,
    cascade_work: usize,
    expanded_value_bytes: usize,
    last_select_value: Option<(u32, u32, String)>,
}

/// A per-navigation partition; group members retain document order. Building it
/// once avoids one complete tree scan per distinct radio name during Tab.
#[derive(Default)]
pub(crate) struct RadioGroups {
    by_node: BTreeMap<u32, usize>,
    members: Vec<Vec<u32>>,
}
impl RadioGroups {
    pub(crate) fn group(&self, id: u32) -> &[u32] {
        self.by_node
            .get(&id)
            .map_or(&[], |index| self.members[*index].as_slice())
    }
}

impl Tree {
    fn validate_retained_text(&mut self) -> Result<(), Violation> {
        use varta_common::view::{
            DRAFT_FIELD_VALUE_RESERVATION_BYTES, DRAFT_MAX_RETAINED_TEXT_BYTES,
        };
        let mut total = 0usize;
        let mut charge = |bytes: usize| {
            total = total.saturating_add(bytes);
            if total > DRAFT_MAX_RETAINED_TEXT_BYTES {
                Err(Violation::RetainedTextLimit)
            } else {
                Ok(())
            }
        };
        charge(self.title.len())?;
        for sheet in self.styles.sheets().values() {
            charge(sheet.text.len())?;
        }
        for node in self.nodes.values_mut() {
            match &mut node.content {
                Content::Text(text) => charge(text.len())?,
                Content::Element(element) => {
                    let value_field = element.text_field() || element.tag == Tag::Select;
                    element.editor.value_reserved |= value_field;
                    charge(element.class.len())?;
                    for text in element.attributes.values().chain(element.style.values()) {
                        charge(text.len())?;
                    }
                    for (name, text) in &element.data.values {
                        charge(name.len() + text.len())?;
                    }
                    if element.editor.value_reserved {
                        charge(DRAFT_FIELD_VALUE_RESERVATION_BYTES)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// One conservative full-inventory read. Candidate anchors ignore media and
    /// local field/focus state; var() scopes include the ancestor inventory cost.
    fn potential_cascade_work(&self, allowance: usize) -> Result<usize, Violation> {
        self.styles
            .potential_cascade_work(self, allowance)
            .map_err(|varta_style::BudgetExceeded| Violation::CascadeWorkLimit)
    }

    fn potential_expanded_values(&self, allowance: usize) -> Result<usize, Violation> {
        self.styles
            .potential_expanded_values(self, allowance)
            .map_err(|varta_style::BudgetExceeded| Violation::ExpandedValueLimit)
    }

    pub(crate) fn radio_groups(&self) -> RadioGroups {
        let mut groups = RadioGroups::default();
        let mut named = BTreeMap::new();
        let mut stack = vec![(ROOT_NODE, ROOT_NODE)];
        while let Some((id, inherited_owner)) = stack.pop() {
            let node = &self.nodes[&id];
            let owner = if self
                .element(id)
                .is_some_and(|element| element.tag == Tag::Form)
            {
                id
            } else {
                inherited_owner
            };
            if let Some(element) = self.element(id)
                && element.checkable()
                && element.input_type() == "radio"
            {
                let name = element
                    .attributes
                    .get(&Attribute::Name)
                    .filter(|name| !name.is_empty());
                let index = match name {
                    Some(name) => *named.entry((owner, name.as_str())).or_insert_with(|| {
                        groups.members.push(Vec::new());
                        groups.members.len() - 1
                    }),
                    None => {
                        groups.members.push(Vec::new());
                        groups.members.len() - 1
                    }
                };
                groups.members[index].push(id);
                groups.by_node.insert(id, index);
            }
            stack.extend(node.children.iter().rev().map(|child| (*child, owner)));
        }
        groups
    }

    pub(crate) fn element(&self, id: u32) -> Option<&Element> {
        match self.nodes.get(&id).map(|node| &node.content) {
            Some(Content::Element(element)) => Some(element),
            _ => None,
        }
    }
    /// Set an element's scroll offset; a change shows its overlay scrollbar
    /// as an explicit (user, programmatic or assistive) scroll does in
    /// Chrome. Returns the element, or `None` for no element.
    pub(crate) fn scroll_to(&mut self, id: u32, top: f64) -> Option<&mut Element> {
        let node = self.nodes.get(&id)?;
        let identity = crate::NodeIdentity {
            id,
            token: node.identity.clone(),
        };
        let element = self.element_mut(id)?;
        let before = std::mem::replace(&mut element.scroll_top, top);
        if before != top {
            self.scrollbars.scrolled(identity);
        }
        self.element_mut(id)
    }
    pub(crate) fn element_mut(&mut self, id: u32) -> Option<&mut Element> {
        match self.nodes.get_mut(&id).map(|node| &mut node.content) {
            Some(Content::Element(element)) => Some(element),
            _ => None,
        }
    }
    /// Whether `id` is attached under the root (or is the root).
    pub(crate) fn attached(&self, mut id: u32) -> bool {
        loop {
            if id == ROOT_NODE {
                return true;
            }
            match self.nodes.get(&id).and_then(|node| node.parent) {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    }
    pub(crate) fn displayed(&self, id: u32, environment: &Environment) -> bool {
        self.attached(id)
            && !self.path(id).into_iter().any(|ancestor| {
                self.closed_dialog(ancestor)
                    || self
                        .computed_property(ancestor, environment, CssProperty::Display)
                        .is_some_and(|value| value.eq_ignore_ascii_case("none"))
            })
    }
    /// A `dialog` that is not open renders nothing (the user agent's
    /// `dialog:not([open]) { display: none }`).
    pub(crate) fn closed_dialog(&self, id: u32) -> bool {
        self.element(id)
            .is_some_and(|element| element.tag == Tag::Dialog && !element.open)
    }
    /// The topmost modal dialog still in the document, if any.
    pub(crate) fn top_modal(&self) -> Option<u32> {
        self.modals
            .iter()
            .rev()
            .map(|modal| modal.dialog)
            .find(|dialog| self.attached(*dialog) && self.element(*dialog).is_some_and(|e| e.open))
    }
    /// Whether a modal dialog blocks `id`: everything outside the topmost
    /// modal dialog is inert (no hits, focus or accessibility).
    pub(crate) fn inert(&self, id: u32) -> bool {
        self.top_modal()
            .is_some_and(|dialog| !self.path(id).contains(&dialog))
    }
    /// Whether a modal dialog removes `id` from the accessibility tree: it is
    /// inert and not an ancestor of the topmost modal dialog (which keeps its
    /// ancestors as the path to it).
    pub(crate) fn blocked(&self, id: u32) -> bool {
        self.top_modal().is_some_and(|dialog| {
            !self.path(id).contains(&dialog) && !self.path(dialog).contains(&id)
        })
    }
    /// The dialog focusing steps: the first displayed, enabled, focusable
    /// descendant with `autofocus`, else the first displayed, enabled,
    /// focusable descendant in tree order, else the dialog itself.
    pub(crate) fn focus_delegate(&self, dialog: u32, environment: &Environment) -> u32 {
        let mut candidates = Vec::new();
        let mut stack: Vec<u32> = self.nodes[&dialog].children.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            if self.closed_dialog(id)
                || self
                    .computed_property(id, environment, CssProperty::Display)
                    .is_some_and(|display| display.eq_ignore_ascii_case("none"))
            {
                continue;
            }
            if self
                .element(id)
                .is_some_and(|element| element.focusable() && !element.disabled())
                && crate::style::visible(self, environment, id)
            {
                candidates.push(id);
            }
            stack.extend(self.nodes[&id].children.iter().rev());
        }
        candidates
            .iter()
            .copied()
            .find(|id| {
                self.element(*id)
                    .is_some_and(|element| element.attributes.contains_key(&Attribute::Autofocus))
            })
            .or_else(|| candidates.first().copied())
            .unwrap_or(dialog)
    }
    /// Close an open dialog: it leaves the top layer, and focus returns to
    /// the element focused before it was shown when that element can take
    /// focus again; focus inside the closed dialog is otherwise dropped.
    /// Returns the focus records; the caller fires `close` after them.
    pub(crate) fn close_dialog(
        &mut self,
        dialog: u32,
        environment: &Environment,
    ) -> Vec<crate::EventRecord> {
        let Some(element) = self.element_mut(dialog) else {
            return Vec::new();
        };
        element.open = false;
        let restore = self
            .modals
            .iter()
            .position(|modal| modal.dialog == dialog)
            .map(|at| self.modals.remove(at))
            .and_then(|modal| modal.restore);
        let restorable = restore.filter(|id| {
            self.displayed(*id, environment)
                && crate::style::visible(self, environment, *id)
                && !self.inert(*id)
                && self
                    .element(*id)
                    .is_some_and(|element| element.focusable() && !element.disabled())
        });
        match restorable {
            Some(id) => focus_records(self, Some(id)),
            None if self
                .focus
                .is_some_and(|focus| self.path(focus).contains(&dialog)) =>
            {
                focus_records(self, None)
            }
            None => Vec::new(),
        }
    }
    /// The node and its ancestors, target first.
    pub(crate) fn path(&self, id: u32) -> Vec<u32> {
        let mut path = vec![id];
        let mut current = id;
        while let Some(parent) = self.nodes.get(&current).and_then(|node| node.parent) {
            path.push(parent);
            current = parent;
        }
        path
    }
    /// Concatenated text of a subtree.
    pub(crate) fn text_content(&self, id: u32) -> String {
        let mut out = String::new();
        for id in self.subtree_order(id) {
            if let Some(Node {
                content: Content::Text(text),
                ..
            }) = self.nodes.get(&id)
            {
                out.push_str(text);
            }
        }
        out
    }
    /// Live attached elements in document order.
    pub(crate) fn document_order(&self) -> Vec<u32> {
        self.subtree_order(ROOT_NODE)
    }
    pub(crate) fn subtree_order(&self, root: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            out.push(id);
            if let Some(node) = self.nodes.get(&id) {
                stack.extend(node.children.iter().rev());
            }
        }
        out
    }
    /// A nonempty-name radio group shares its tree and closest form owner.
    /// Walk once from that owner, skipping nested forms, rather than resolving
    /// every candidate's ancestor path separately.
    pub(crate) fn radio_group(&self, id: u32) -> Vec<u32> {
        self.radio_group_with_work(id, usize::MAX)
            .expect("one group cannot overflow usize")
            .0
    }
    fn radio_group_with_work(
        &self,
        id: u32,
        allowance: usize,
    ) -> Result<(Vec<u32>, usize), Violation> {
        let mut work = 0usize;
        let mut charge = |amount: usize| {
            work = work.saturating_add(amount);
            if work > allowance {
                Err(Violation::RadioWorkLimit)
            } else {
                Ok(())
            }
        };
        let Some(element) = self.element(id) else {
            return Ok((Vec::new(), 0));
        };
        if !element.checkable() || element.input_type() != "radio" {
            return Ok((Vec::new(), 0));
        }
        let Some(name) = element
            .attributes
            .get(&Attribute::Name)
            .filter(|name| !name.is_empty())
        else {
            charge(varta_common::view::DRAFT_RADIO_NODE_WORK)?;
            return Ok((vec![id], work));
        };
        let mut owner = id;
        let mut at = Some(id);
        while let Some(current) = at {
            charge(varta_common::view::DRAFT_RADIO_NODE_WORK)?;
            owner = current;
            let node = &self.nodes[&current];
            if self
                .element(current)
                .is_some_and(|element| element.tag == Tag::Form)
            {
                break;
            }
            at = node.parent;
        }
        let mut group = Vec::new();
        let mut stack = vec![owner];
        while let Some(current) = stack.pop() {
            charge(varta_common::view::DRAFT_RADIO_NODE_WORK)?;
            let node = &self.nodes[&current];
            if let Content::Element(candidate) = &node.content {
                if current != owner && candidate.tag == Tag::Form {
                    continue;
                }
                if candidate.checkable() && candidate.input_type() == "radio" {
                    let candidate_name = candidate.attributes.get(&Attribute::Name);
                    charge(candidate_name.map_or(0, String::len))?;
                    if candidate_name == Some(name) {
                        group.push(current);
                    }
                }
            }
            stack.extend(node.children.iter().rev());
        }
        Ok((group, work))
    }
    pub(crate) fn by_ident(&self, ident: &str) -> Option<u32> {
        self.nodes
            .iter()
            .find_map(|(id, node)| match &node.content {
                Content::Element(element)
                    if element.attributes.get(&Attribute::Id).map(String::as_str)
                        == Some(ident) =>
                {
                    Some(*id)
                }
                _ => None,
            })
    }
    /// Resolve custom declarations at each ancestor before inheritance.
    pub(crate) fn custom_properties<'a>(
        &'a self,
        id: u32,
        environment: &Environment,
    ) -> varta_style::Scope<'a> {
        self.styles.scope(self, id, &environment.style())
    }
    pub(crate) fn pointer_contains(&self, target: Option<&crate::NodeIdentity>, id: u32) -> bool {
        target.is_some_and(|target| {
            self.nodes
                .get(&target.id)
                .is_some_and(|node| Arc::ptr_eq(&node.identity, &target.token))
                && self.attached(target.id)
                && self.path(target.id).contains(&id)
        })
    }
    /// The matching rule declarations of `id` in cascade order (Cascade 5
    /// §6.1): normal ones, which inline values override, and important
    /// ones, which override inline values.
    pub(crate) fn cascade_refs(
        &self,
        id: u32,
        environment: &Environment,
    ) -> varta_style::Cascade<'_> {
        self.styles.cascade(self, id, &environment.media())
    }
    /// Select one winning property without resolving unrelated values/variables.
    pub(crate) fn computed_property(
        &self,
        id: u32,
        environment: &Environment,
        property: CssProperty,
    ) -> Option<String> {
        if let Some(values) = self.render_reads.get(id, environment) {
            return values.get(&property).cloned();
        }
        if let Some(value) = self.animations.overlay_property(id, property) {
            return Some(value);
        }
        self.computed_static(id, environment).remove(&property)
    }
    /// The computed values layout and paint read: the cascade with the
    /// current value of each running transition.
    pub(crate) fn computed(
        &self,
        id: u32,
        environment: &Environment,
    ) -> BTreeMap<CssProperty, String> {
        if let Some(values) = self.render_reads.get(id, environment) {
            return values.clone();
        }
        let mut values = self.computed_static(id, environment);
        self.animations.overlay(id, &mut values);
        values
    }
    /// The computed values without transitions: what a style change compares
    /// and snapshots show. The engine resolves the cascade and `var()`; the
    /// CSS-wide keywords it leaves resolve against the native user-agent
    /// origin ([`Tree::keyword`]).
    pub(crate) fn computed_static(
        &self,
        id: u32,
        environment: &Environment,
    ) -> BTreeMap<CssProperty, String> {
        self.styles
            .computed(self, id, &environment.style(), |property, value| {
                self.render_reads.validate(environment, property, value)
            })
            .into_iter()
            .filter_map(|(property, computed)| {
                self.keyword(id, environment, property, computed)
                    .map(|value| (property, value))
            })
            .collect()
    }
    /// A computed value's native text: a value as is; `initial` the
    /// property's initial value where the native user-agent origin sets one
    /// for this element, else no value (which is the initial value); and
    /// `inherit` the parent's computed value, or for an inherited property
    /// the marker [`style::INHERIT`] its consumers follow (Cascade 5 §7.3).
    pub(crate) fn keyword(
        &self,
        id: u32,
        environment: &Environment,
        property: CssProperty,
        computed: varta_style::Computed,
    ) -> Option<String> {
        match computed {
            varta_style::Computed::Value(value) => Some(value),
            varta_style::Computed::Initial => style::initial_override(self, id, property),
            varta_style::Computed::Inherit if varta_style::inherited(property) => {
                Some(style::INHERIT.into())
            }
            varta_style::Computed::Inherit => {
                let parent = self.nodes.get(&id)?.parent?;
                self.computed_property(parent, environment, property)
                    .or_else(|| style::ua_value(self, parent, property))
            }
        }
    }
    pub(crate) fn computed_read<'a>(
        &'a self,
        id: u32,
        env: &Environment,
    ) -> std::borrow::Cow<'a, BTreeMap<CssProperty, String>> {
        match self.render_reads.get(id, env) {
            Some(values) => std::borrow::Cow::Borrowed(values),
            None => std::borrow::Cow::Owned(self.computed(id, env)),
        }
    }
    /// Whether any stylesheet rule or inline style declares a transition
    /// duration or delay, without which no transition has a positive
    /// combined duration and none can start.
    pub(crate) fn uses_transitions(&self) -> bool {
        let declares = |property: &CssProperty| {
            matches!(
                property,
                CssProperty::TransitionDuration | CssProperty::TransitionDelay
            )
        };
        self.styles.declares_transitions()
            || self.nodes.keys().any(|id| {
                self.element(*id)
                    .is_some_and(|element| element.style.keys().any(declares))
            })
    }
    /// Document order with hidden subtrees skipped once, rather than checking
    /// the complete ancestor path for every prospective focus stop.
    pub(crate) fn displayed_order(&self, environment: &Environment) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack = vec![ROOT_NODE];
        while let Some(id) = stack.pop() {
            if self.closed_dialog(id)
                || self
                    .computed_property(id, environment, CssProperty::Display)
                    .is_some_and(|display| display.eq_ignore_ascii_case("none"))
            {
                continue;
            }
            out.push(id);
            stack.extend(self.nodes[&id].children.iter().rev());
        }
        out
    }

    /// Apply a decoded batch to this (copied) tree: to the guest's nodes
    /// alone, without the generated boxes, which the next style change
    /// event regenerates.
    pub(crate) fn apply(
        &mut self,
        operations: &[Operation],
        context: &mut Context<'_>,
    ) -> Result<Vec<Effect>, ViewError> {
        self.strip_generated();
        let mut apply = Apply {
            tree: self,
            context,
            effects: Vec::new(),
            index: 0,
            layout_commands: 0,
            select_work: 0,
            radio_work: 0,
            stylesheet_work: 0,
            cascade_work: 0,
            expanded_value_bytes: 0,
            last_select_value: None,
        };
        for (index, operation) in operations.iter().enumerate() {
            apply.index = index;
            apply.operation(operation)?;
        }
        apply.finish()?;
        Ok(apply.effects)
    }
}

impl Apply<'_, '_, '_> {
    fn fail(&self, violation: Violation) -> ViewError {
        ViewError::Invalid {
            operation: Some(self.index),
            violation,
        }
    }
    fn live(&self, id: u32) -> Result<&Node, ViewError> {
        self.tree
            .nodes
            .get(&id)
            .ok_or_else(|| self.fail(Violation::UnknownNode(id)))
    }
    fn element(&mut self, id: u32) -> Result<&mut Element, ViewError> {
        let operation = Some(self.index);
        match self.tree.nodes.get_mut(&id).map(|node| &mut node.content) {
            Some(Content::Element(element)) => Ok(element),
            Some(Content::Text(_)) => Err(ViewError::Invalid {
                operation,
                violation: Violation::NotAnElement(id),
            }),
            None => Err(ViewError::Invalid {
                operation,
                violation: Violation::UnknownNode(id),
            }),
        }
    }
    fn tag(&mut self, id: u32) -> Result<Tag, ViewError> {
        Ok(self.element(id)?.tag)
    }

    fn charge_select_work(
        &mut self,
        work: usize,
        operation: Option<usize>,
    ) -> Result<(), ViewError> {
        self.select_work = self.select_work.saturating_add(work);
        if self.select_work > varta_common::view::DRAFT_MAX_SELECT_WORK {
            return Err(ViewError::Invalid {
                operation,
                violation: Violation::SelectWorkLimit,
            });
        }
        Ok(())
    }

    fn charge_cascade_read(&mut self, operation: Option<usize>) -> Result<(), ViewError> {
        let work = self
            .tree
            .potential_cascade_work(
                varta_common::view::DRAFT_MAX_CASCADE_WORK.saturating_sub(self.cascade_work),
            )
            .map_err(|violation| ViewError::Invalid {
                operation,
                violation,
            })?;
        self.cascade_work = self.cascade_work.saturating_add(work);
        let bytes = self
            .tree
            .potential_expanded_values(
                varta_common::view::DRAFT_MAX_EXPANDED_VALUE_BYTES
                    .saturating_sub(self.expanded_value_bytes),
            )
            .map_err(|violation| ViewError::Invalid {
                operation,
                violation,
            })?;
        self.expanded_value_bytes = self.expanded_value_bytes.saturating_add(bytes);
        Ok(())
    }

    fn charge_radio_work(&mut self, work: usize) -> Result<(), ViewError> {
        self.radio_work = self.radio_work.saturating_add(work);
        if self.radio_work > varta_common::view::DRAFT_MAX_RADIO_WORK {
            return Err(self.fail(Violation::RadioWorkLimit));
        }
        Ok(())
    }

    // Meter independently of checked/seq: a browser edit can still be queued
    // for Worker delivery. Group membership is authoritative in either state.
    fn reconcile_radio(&mut self, id: u32, apply: bool) -> Result<(), ViewError> {
        let (group, work) = self
            .tree
            .radio_group_with_work(
                id,
                varta_common::view::DRAFT_MAX_RADIO_WORK.saturating_sub(self.radio_work),
            )
            .map_err(|violation| self.fail(violation))?;
        self.charge_radio_work(work)?;
        if apply {
            for peer in group.into_iter().filter(|peer| *peer != id) {
                if let Some(element) = self.tree.element_mut(peer) {
                    element.checked = false;
                }
            }
        }
        Ok(())
    }

    fn operation(&mut self, operation: &Operation) -> Result<(), ViewError> {
        let node = operation.id(0);
        let select_value = operation.opcode == Opcode::SetValue
            && self
                .tree
                .element(node)
                .is_some_and(|element| element.tag == Tag::Select);
        // The counter must not depend on locally edited value/seq: a browser
        // Worker can still be waiting to deliver that edit. Consecutive identical
        // writes are idempotent even when their unmatched value becomes "".
        if select_value
            && self
                .last_select_value
                .as_ref()
                .is_some_and(|(id, seq, value)| {
                    *id == node && *seq == operation.id(2) && value.as_str() == operation.text()
                })
        {
            return Ok(());
        }
        self.last_select_value = None;
        self.charge_select_work(self.tree.select_operation_work(operation), Some(self.index))?;
        if select_value {
            self.last_select_value = Some((node, operation.id(2), operation.text().to_owned()));
        }
        match operation.opcode {
            Opcode::CreateElement => {
                let Some(Value::Tag(tag)) = operation.operands.get(1) else {
                    return Err(self.fail(Violation::UnknownTag));
                };
                self.create(node, Content::Element(Element::new(*tag)))
            }
            Opcode::CreateText => self.create(node, Content::Text(operation.text().into())),
            Opcode::SetText => {
                let text = operation.text().to_owned();
                match self.tree.nodes.get_mut(&node).map(|node| &mut node.content) {
                    Some(Content::Text(current)) => {
                        *current = text;
                        self.tree.sync_select_for(node);
                        Ok(())
                    }
                    Some(Content::Element(_)) => Err(self.fail(Violation::NotAText(node))),
                    None => Err(self.fail(Violation::UnknownNode(node))),
                }
            }
            Opcode::SetAttribute => {
                let Some(Value::Attribute(attribute)) = operation.operands.get(1) else {
                    return Err(self.fail(Violation::UnknownAttribute));
                };
                self.set_attribute(node, *attribute, operation.text())
            }
            Opcode::RemoveAttribute => {
                let Some(Value::Attribute(attribute)) = operation.operands.get(1) else {
                    return Err(self.fail(Violation::UnknownAttribute));
                };
                let tag = self.tag(node)?;
                self.attribute_admitted(*attribute, tag)?;
                if self.element(node)?.attributes.remove(attribute).is_some() {
                    if *attribute == Attribute::Type {
                        self.element(node)?.editor.text_epoch = Some(Arc::new(()));
                        self.element(node)?.editor.horizontal_scroll = None;
                    }
                    self.tree.select_attribute(node, *attribute, false);
                }
                Ok(())
            }
            Opcode::SetClass => {
                let class = operation.text();
                // The HTML class model: whitespace-separated tokens without
                // controls, each bounded.
                if !varta_common::view::css::class_list(class) {
                    return Err(self.fail(Violation::InvalidClass));
                }
                self.element(node)?.class = class.into();
                Ok(())
            }
            Opcode::SetStyle => {
                let Some(Value::Property(property)) = operation.operands.get(1) else {
                    return Err(self.fail(Violation::UnknownProperty));
                };
                let tag = self.tag(node)?;
                if matches!(property, CssProperty::FieldSizing | CssProperty::Resize)
                    && tag != Tag::Textarea
                {
                    return Err(self.fail(Violation::PropertyNotAllowed));
                }
                if !varta_common::view::css::property_value(*property, operation.text()) {
                    let keyword = (*property == CssProperty::FontFamily)
                        .then(|| {
                            varta_common::view::css::font_family_css_wide_keyword(operation.text())
                        })
                        .flatten();
                    return Err(self.fail(
                        keyword.map_or(Violation::InvalidStyleValue, Violation::FontFamilyKeyword),
                    ));
                }
                // Comments and escapes are stored serialized (CSS Syntax 3 §9),
                // so that later `var()` scans never see either.
                let stored: String =
                    if varta_common::view::syntax::needs_serialization(operation.text()) {
                        varta_common::view::css::normalize(operation.text())
                    } else {
                        operation.text().into()
                    };
                let element = self.element(node)?;
                element.style.insert(*property, stored.clone());
                if let Some(sides) = geometry_sides(*property) {
                    for (index, side) in sides.iter().enumerate() {
                        element
                            .layout_inline
                            .insert(*side, (*property, stored.clone(), index));
                    }
                }
                for component in border_components(*property) {
                    element
                        .border_inline
                        .insert(component, (*property, stored.clone()));
                }
                for side in border_color_sides(*property) {
                    element
                        .border_color_inline
                        .values
                        .insert(*side, (*property, stored.clone()));
                }
                for component in outline_components(*property) {
                    element
                        .outline_inline
                        .values
                        .insert(*component, (*property, stored.clone()));
                }
                Ok(())
            }
            Opcode::RemoveStyle => {
                let Some(Value::Property(property)) = operation.operands.get(1) else {
                    return Err(self.fail(Violation::UnknownProperty));
                };
                let element = self.element(node)?;
                element.style.remove(property);
                if let Some(sides) = geometry_sides(*property) {
                    for side in sides {
                        element.layout_inline.remove(side);
                    }
                }
                for component in border_components(*property) {
                    element.border_inline.remove(&component);
                }
                for side in border_color_sides(*property) {
                    element.border_color_inline.values.remove(side);
                }
                for component in outline_components(*property) {
                    element.outline_inline.values.remove(component);
                }
                Ok(())
            }
            Opcode::Insert => self.insert(node, operation.id(1), operation.id(2)),
            Opcode::Remove => self.remove(node),
            Opcode::Listen => self.listen(node, operation),
            Opcode::Unlisten => {
                let (Some(Value::Event(kind)), handler) =
                    (operation.operands.get(1), operation.id(2))
                else {
                    return Err(self.fail(Violation::UnknownEvent));
                };
                let kind = *kind;
                let element = self.element(node)?;
                let Some(position) = element
                    .listeners
                    .iter()
                    .position(|listener| listener.handler == handler && listener.kind == kind)
                else {
                    return Err(self.fail(Violation::NotListening(handler)));
                };
                element.listeners.remove(position);
                self.tree.handlers.remove(&handler);
                Ok(())
            }
            Opcode::SetValue => self.set_value(node, operation),
            Opcode::SetChecked => self.set_checked(node, operation),
            Opcode::Command => self.command(node, operation),
            Opcode::Stylesheet => self.stylesheet(node, operation.text()),
            Opcode::SetData => {
                let name = operation.data_name();
                if !varta_common::view::data_attribute_name(name) {
                    return Err(self.fail(Violation::InvalidDataName));
                }
                let (name, value) = (name.to_owned(), operation.text().to_owned());
                let element = self.element(node)?;
                if !element.data.values.contains_key(&name)
                    && element.data.values.len() >= varta_common::view::DRAFT_MAX_DATA_ATTRIBUTES
                {
                    return Err(self.fail(Violation::TooManyDataAttributes));
                }
                element.data.values.insert(name, value);
                Ok(())
            }
            Opcode::RemoveData => {
                let name = operation.data_name();
                if !varta_common::view::data_attribute_name(name) {
                    return Err(self.fail(Violation::InvalidDataName));
                }
                let name = name.to_owned();
                self.element(node)?.data.values.remove(&name);
                Ok(())
            }
            Opcode::SetTitle => {
                self.tree.title = operation.text().into();
                Ok(())
            }
            Opcode::CopyText => {
                let request = node;
                if request == 0 || self.tree.pending_requests.contains(&request) {
                    return Err(self.fail(Violation::DuplicateRequest(request)));
                }
                if self.tree.pending_requests.len() >= DRAFT_MAX_PENDING_COMMANDS {
                    return Err(self.fail(Violation::TooManyPendingCommands));
                }
                self.tree.pending_requests.insert(request);
                self.effects.push(Effect::Copy {
                    request,
                    text: operation.text().into(),
                });
                Ok(())
            }
        }
    }

    fn create(&mut self, id: u32, content: Content) -> Result<(), ViewError> {
        if id == NO_NODE || self.tree.nodes.contains_key(&id) {
            return Err(self.fail(Violation::DuplicateNode(id)));
        }
        if crate::generated::generated_id(id) {
            return Err(self.fail(Violation::GeneratedNode(id)));
        }
        if self.tree.nodes.len() >= DRAFT_MAX_LIVE_NODES {
            return Err(self.fail(Violation::TooManyNodes));
        }
        self.tree.nodes.insert(
            id,
            Node {
                identity: Arc::new(()),
                content,
                parent: None,
                children: Vec::new(),
                generated: None,
            },
        );
        Ok(())
    }

    fn attribute_admitted(&self, attribute: Attribute, tag: Tag) -> Result<(), ViewError> {
        let admitted = match attribute.tags() {
            Tags::All => true,
            Tags::Focusable => false,
            Tags::Only(tags) => tags.contains(&tag),
        };
        if admitted {
            Ok(())
        } else {
            Err(self.fail(Violation::AttributeNotAllowed(attribute.name())))
        }
    }

    fn set_attribute(
        &mut self,
        node: u32,
        attribute: Attribute,
        value: &str,
    ) -> Result<(), ViewError> {
        let tag = self.tag(node)?;
        self.attribute_admitted(attribute, tag)?;
        let grammar = attribute.value();
        let valid = if grammar == AttributeValue::Href && values::external(value) {
            (self.context.link_requested)(value)
        } else {
            values::valid(grammar, tag, value)
        };
        if !valid {
            return Err(self.fail(Violation::InvalidAttributeValue(attribute.name())));
        }
        if matches!(attribute, Attribute::Src | Attribute::Srcset)
            && let Some(images) = &self.tree.images.set
        {
            let names = match attribute {
                Attribute::Src => crate::image::select::named(Some(value), None),
                _ => crate::image::select::named(None, Some(value)),
            };
            if let Some(missing) = names.into_iter().find(|name| !images.contains(name)) {
                return Err(self.fail(Violation::UnpackagedImage(missing.into())));
            }
        }
        let element = self.element(node)?;
        let changed = element.attributes.get(&attribute).map(String::as_str) != Some(value);
        // A new image source restarts the element's animation timeline.
        if changed && matches!(attribute, Attribute::Src | Attribute::Srcset) {
            let now = self.tree.images.now;
            self.tree.images.started.insert(node, now);
        }
        let element = self.element(node)?;
        element.attributes.insert(attribute, value.into());
        if changed && attribute == Attribute::Type {
            element.editor.text_epoch = Some(Arc::new(()));
            element.editor.horizontal_scroll = None;
        }
        if matches!(attribute, Attribute::Disabled | Attribute::Readonly) {
            element.editor.preedit = None;
            element.editor.select_popup = None;
        }
        let reconcile = matches!(attribute, Attribute::Name | Attribute::Type)
            && element.checkable()
            && element.input_type() == "radio";
        let checked = element.checked;
        if reconcile {
            self.reconcile_radio(node, checked)?;
        }
        if changed {
            self.tree.select_attribute(node, attribute, true);
        }
        Ok(())
    }

    fn insert(&mut self, parent: u32, node: u32, before: u32) -> Result<(), ViewError> {
        if node == ROOT_NODE {
            return Err(self.fail(Violation::RootNotMovable));
        }
        let parent_tag = self.tag(parent)?;
        let child = self.live(node)?;
        let child_tag = match &child.content {
            Content::Element(element) => Some(element.tag),
            Content::Text(_) => None,
        };
        if !parent_tag.admits_child(child_tag) {
            return Err(self.fail(Violation::ChildNotAllowed));
        }
        let parent_path = self.tree.path(parent);
        if parent_path.contains(&node) {
            return Err(self.fail(Violation::Cycle));
        }
        if before != NO_NODE
            && (before == node
                || self.tree.nodes.get(&before).and_then(|n| n.parent) != Some(parent))
        {
            return Err(self.fail(Violation::InvalidBefore(before)));
        }
        let old_select = self.tree.select_owner(node);
        let moved_option = self
            .tree
            .element(node)
            .is_some_and(|element| element.tag == Tag::Option);
        self.detach(node);
        let children = &mut self
            .tree
            .nodes
            .get_mut(&parent)
            .ok_or(ViewError::Invalid {
                operation: Some(self.index),
                violation: Violation::UnknownNode(parent),
            })?
            .children;
        let at = if before == NO_NODE {
            children.len()
        } else {
            children
                .iter()
                .position(|child| *child == before)
                .unwrap_or(children.len())
        };
        children.insert(at, node);
        if let Some(child) = self.tree.nodes.get_mut(&node) {
            child.parent = Some(parent);
        }
        // The existing moved-subtree walk checks every descendant, including
        // detached components and text leaves, before any host effect escapes.
        let mut stack = vec![(node, parent_path.len())];
        while let Some((radio, depth)) = stack.pop() {
            if depth > varta_common::view::DRAFT_MAX_TREE_DEPTH {
                return Err(self.fail(Violation::TreeDepthLimit));
            }
            self.charge_radio_work(varta_common::view::DRAFT_RADIO_NODE_WORK)?;
            if let Some(element) = self.tree.element(radio)
                && element.checkable()
                && element.input_type() == "radio"
            {
                let checked = element.checked;
                self.reconcile_radio(radio, checked)?;
            }
            stack.extend(
                self.tree.nodes[&radio]
                    .children
                    .iter()
                    .rev()
                    .map(|child| (*child, depth + 1)),
            );
        }
        if let Some(select) = old_select {
            if moved_option {
                self.tree.normalize_select(select, None);
            } else {
                self.tree.sync_select_value(select);
            }
        }
        if let Some(select) = self.tree.select_owner(node) {
            let preferred = self
                .tree
                .element(node)
                .filter(|element| element.tag == Tag::Option && element.editor.option_selected)
                .map(|_| node);
            if self
                .tree
                .element(node)
                .is_some_and(|element| element.tag == Tag::Option)
            {
                self.tree.normalize_select(select, preferred);
            } else {
                self.tree.sync_select_for(node);
            }
        }
        Ok(())
    }

    fn detach(&mut self, node: u32) {
        let Some(parent) = self.tree.nodes.get(&node).and_then(|n| n.parent) else {
            return;
        };
        // A dialog removed from the document, or moved, leaves the top layer
        // (the HTML removing steps); it stays open but is no longer modal.
        let tree = &*self.tree;
        let leaving: Vec<u32> = tree
            .modals
            .iter()
            .map(|modal| modal.dialog)
            .filter(|dialog| tree.path(*dialog).contains(&node))
            .collect();
        self.tree
            .modals
            .retain(|modal| !leaving.contains(&modal.dialog));
        if let Some(parent) = self.tree.nodes.get_mut(&parent) {
            parent.children.retain(|child| *child != node);
        }
        if let Some(child) = self.tree.nodes.get_mut(&node) {
            child.parent = None;
        }
    }

    fn remove(&mut self, node: u32) -> Result<(), ViewError> {
        if node == ROOT_NODE {
            return Err(self.fail(Violation::RootNotMovable));
        }
        self.live(node)?;
        let select = self.tree.select_owner(node);
        let option = self
            .tree
            .element(node)
            .is_some_and(|element| element.tag == Tag::Option);
        self.detach(node);
        let mut stack = vec![node];
        while let Some(id) = stack.pop() {
            let Some(removed) = self.tree.nodes.remove(&id) else {
                continue;
            };
            if let Content::Element(element) = &removed.content {
                for listener in &element.listeners {
                    self.tree.handlers.remove(&listener.handler);
                }
            }
            if self.tree.focus == Some(id) {
                self.tree.focus = None;
            }
            stack.extend(removed.children);
        }
        if let Some(select) = select {
            if option {
                self.tree.normalize_select(select, None);
            } else {
                self.tree.sync_select_value(select);
            }
        }
        Ok(())
    }

    fn listen(&mut self, node: u32, operation: &Operation) -> Result<(), ViewError> {
        let (Some(Value::Event(kind)), handler) = (operation.operands.get(1), operation.id(2))
        else {
            return Err(self.fail(Violation::UnknownEvent));
        };
        let kind = *kind;
        let Some(Value::Key(key)) = operation.operands.get(4) else {
            return Err(self.fail(Violation::UnknownKey));
        };
        let filter = crate::Filter {
            options: operation.id(3),
            key: *key,
            required: operation.id(5),
            forbidden: operation.id(6),
        };
        let option_bits = ListenOption::ALL
            .iter()
            .fold(0, |bits, option| bits | *option as u32);
        if kind == EventKind::CommandResult {
            return Err(self.fail(Violation::NotListenable));
        }
        if filter.options & !option_bits != 0
            || filter.required & !MODIFIER_MASK != 0
            || filter.forbidden & !MODIFIER_MASK != 0
        {
            return Err(self.fail(Violation::InvalidFilter));
        }
        let key_filter = filter.key != ANY_KEY
            || filter.required != 0
            || filter.forbidden != 0
            || filter.options & ListenOption::NotComposing as u32 != 0;
        if kind != EventKind::Keydown && key_filter {
            return Err(self.fail(Violation::InvalidFilter));
        }
        if handler == 0 || self.tree.handlers.contains_key(&handler) {
            return Err(self.fail(Violation::DuplicateHandler(handler)));
        }
        if self.tree.handlers.len() >= DRAFT_MAX_LIVE_LISTENERS {
            return Err(self.fail(Violation::TooManyListeners));
        }
        let same_filter = |listener: &Listener| {
            listener.kind == kind
                && listener.filter.key == filter.key
                && listener.filter.required == filter.required
                && listener.filter.forbidden == filter.forbidden
                && (listener.filter.options & ListenOption::NotComposing as u32)
                    == (filter.options & ListenOption::NotComposing as u32)
        };
        let element = self.element(node)?;
        if element.listeners.iter().any(same_filter) {
            return Err(self.fail(Violation::DuplicateFilter));
        }
        element.listeners.push(Listener {
            kind,
            handler,
            filter,
        });
        self.tree.handlers.insert(handler, node);
        Ok(())
    }

    fn set_value(&mut self, node: u32, operation: &Operation) -> Result<(), ViewError> {
        let base = operation.id(2);
        let value = operation.text().to_owned();
        let element = self.element(node)?;
        if !(element.text_field() || element.tag == Tag::Select) {
            return Err(self.fail(Violation::NotAField(node)));
        }
        let element = self.element(node)?;
        element.editor.value_reserved = true;
        // A stale patch (newer local edits exist) is dropped, not an error.
        if element.seq == base {
            if element.tag == Tag::Select {
                if element.value != value {
                    self.tree.set_select_value(node, &value);
                }
                return Ok(());
            }
            if element.value != value {
                element.editor.anchor = value.len();
                element.editor.focus = value.len();
                element.editor.preedit = None;
                element.editor.history_epoch = Arc::new(());
                element.editor.text_epoch = Some(Arc::new(()));
                element.editor.manual_text_scroll = false;
                element.editor.reset_navigation();
            }
            element.value = value;
        }
        Ok(())
    }

    fn set_checked(&mut self, node: u32, operation: &Operation) -> Result<(), ViewError> {
        let (checked, base) = (operation.id(1), operation.id(2));
        if checked > 1 {
            return Err(self.fail(Violation::InvalidChecked));
        }
        let element = self.element(node)?;
        if !element.checkable() {
            return Err(self.fail(Violation::NotAField(node)));
        }
        let accepted = element.seq == base;
        let radio = element.input_type() == "radio";
        if accepted {
            element.checked = checked == 1;
        }
        if checked == 1 && radio {
            self.reconcile_radio(node, accepted)?;
        }
        Ok(())
    }

    fn command(&mut self, node: u32, operation: &Operation) -> Result<(), ViewError> {
        let (Some(Value::Command(command)), Some(Value::Number(argument))) =
            (operation.operands.get(1), operation.operands.get(2))
        else {
            return Err(self.fail(Violation::UnknownCommand));
        };
        let (command, argument) = (*command, *argument);
        let element = self.element(node)?;
        let admitted = match command.tags() {
            Tags::All => true,
            Tags::Focusable => element.focusable(),
            Tags::Only(tags) => tags.contains(&element.tag),
        };
        if !admitted {
            return Err(self.fail(Violation::CommandNotAllowed(command.name())));
        }
        let valid_argument = if command.takes_argument() {
            argument.is_finite() && argument >= 0.0
        } else {
            argument.to_bits() == 0
        };
        if !valid_argument {
            return Err(self.fail(Violation::InvalidArgument));
        }
        if matches!(
            command,
            Command::Focus | Command::Select | Command::ScrollIntoView | Command::ScrollTo
        ) {
            self.charge_cascade_read(Some(self.index))?;
        }
        match command {
            Command::Focus => {
                let element = self.element(node)?;
                if !element.disabled()
                    && self.tree.displayed(node, &self.context.environment)
                    && crate::style::visible(self.tree, &self.context.environment, node)
                    && !self.tree.inert(node)
                {
                    let records = focus_records(self.tree, Some(node));
                    self.effects.extend(records.into_iter().map(Effect::Record));
                }
            }
            Command::Blur => {
                if self.tree.focus == Some(node) {
                    let records = focus_records(self.tree, None);
                    self.effects.extend(records.into_iter().map(Effect::Record));
                }
            }
            Command::Select => {
                let focus = !self.element(node)?.disabled()
                    && self.tree.displayed(node, &self.context.environment)
                    && !self.tree.inert(node);
                if focus {
                    let records = focus_records(self.tree, Some(node));
                    self.effects.extend(records.into_iter().map(Effect::Record));
                }
                let element = self.element(node)?;
                if element.text_field()
                    && (element.tag == Tag::Textarea || element.input_type() != "number")
                {
                    element.editor.anchor = 0;
                    element.editor.focus = element.value.len();
                    element.editor.preedit = None;
                    element.editor.manual_text_scroll = false;
                    element.editor.reset_navigation();
                }
            }
            Command::ScrollIntoView => {
                if self.context.layout.is_some() {
                    self.layout_commands += 1;
                    if self.layout_commands > DRAFT_MAX_PENDING_COMMANDS {
                        return Err(self.fail(Violation::TooManyPendingCommands));
                    }
                }
                if let Some(layout) = self.context.layout.as_mut() {
                    let geometry = match layout.font.as_mut() {
                        Some((font, size)) => crate::layout::geometry_with_text(
                            self.tree,
                            &self.context.environment,
                            layout.width,
                            layout.height,
                            font,
                            *size,
                        ),
                        None => crate::layout::geometry(
                            self.tree,
                            &self.context.environment,
                            layout.width,
                            layout.height,
                        ),
                    }
                    .map_err(|_| self.fail(Violation::ScrollLayout))?;
                    let plan = crate::layout::scroll_into_view_plan(
                        self.tree,
                        &self.context.environment,
                        &geometry,
                        node,
                    )
                    .map_err(|_| self.fail(Violation::ScrollLayout))?;
                    for (id, top) in plan {
                        self.element(id)?;
                        self.tree.scroll_to(id, top);
                        let scroll = geometry[&id]
                            .programmatic_scroll
                            .ok_or_else(|| self.fail(Violation::ScrollLayout))?;
                        let mut record = crate::EventRecord::occurrence(EventKind::Scroll, id);
                        record.scroll_top = top;
                        record.client_height = f64::from(scroll.viewport.height);
                        record.scroll_height = f64::from(scroll.viewport.height + scroll.max);
                        let identity = self.tree.nodes[&id].identity.clone();
                        self.effects.push(Effect::ScrollRecord { record, identity });
                    }
                }
            }
            Command::ScrollTo => {
                if self.context.layout.is_some() {
                    self.layout_commands += 1;
                    if self.layout_commands > DRAFT_MAX_PENDING_COMMANDS {
                        return Err(self.fail(Violation::TooManyPendingCommands));
                    }
                }
                if let Some(layout) = self.context.layout.as_mut() {
                    let geometry = match layout.font.as_mut() {
                        Some((font, size)) => crate::layout::geometry_with_text(
                            self.tree,
                            &self.context.environment,
                            layout.width,
                            layout.height,
                            font,
                            *size,
                        ),
                        None => crate::layout::geometry(
                            self.tree,
                            &self.context.environment,
                            layout.width,
                            layout.height,
                        ),
                    }
                    .map_err(|_| self.fail(Violation::ScrollLayout))?;
                    if let Some(scroll) = geometry
                        .get(&node)
                        .and_then(|item| item.programmatic_scroll)
                    {
                        let top = argument.clamp(0.0, f64::from(scroll.max));
                        if top != f64::from(scroll.top) {
                            self.element(node)?;
                            self.tree.scroll_to(node, top);
                            let mut record =
                                crate::EventRecord::occurrence(EventKind::Scroll, node);
                            record.scroll_top = top;
                            record.client_height = f64::from(scroll.viewport.height);
                            record.scroll_height = f64::from(scroll.viewport.height + scroll.max);
                            let identity = self.tree.nodes[&node].identity.clone();
                            self.effects.push(Effect::ScrollRecord { record, identity });
                        }
                    }
                    return Ok(());
                }
                let scrolls =
                    crate::style::overflow(&self.tree.computed(node, &self.context.environment))
                        .is_some_and(|axes| {
                            matches!(
                                axes[1],
                                crate::style::Overflow::Auto | crate::style::Overflow::Scroll
                            )
                        });
                if scrolls {
                    self.element(node)?;
                    self.tree.scroll_to(node, argument);
                }
            }
            Command::ShowModal => {
                if !self.tree.attached(node) {
                    return Err(self.fail(Violation::Detached(node)));
                }
                let element = self.element(node)?;
                // An open dialog stays where it is in the top layer.
                if !element.open {
                    element.open = true;
                    let restore = self.tree.focus;
                    self.tree.modals.push(Modal {
                        dialog: node,
                        restore,
                    });
                    let delegate = self.tree.focus_delegate(node, &self.context.environment);
                    let records = focus_records(self.tree, Some(delegate));
                    self.effects.extend(records.into_iter().map(Effect::Record));
                }
            }
            Command::Close => {
                if self.element(node)?.open {
                    let records = self.tree.close_dialog(node, &self.context.environment);
                    self.effects.extend(records.into_iter().map(Effect::Record));
                    self.effects
                        .push(Effect::Record(crate::EventRecord::occurrence(
                            EventKind::Close,
                            node,
                        )));
                }
            }
        }
        Ok(())
    }

    fn stylesheet(&mut self, sheet: u32, text: &str) -> Result<(), ViewError> {
        self.tree
            .styles
            .set(sheet, text, &mut self.stylesheet_work)
            .map_err(|error| {
                self.fail(match error {
                    varta_style::SheetError::InvalidId => Violation::InvalidSheet,
                    varta_style::SheetError::Invalid(_) => Violation::InvalidStylesheet,
                    varta_style::SheetError::WorkLimit => Violation::StylesheetWorkLimit,
                    varta_style::SheetError::TooLarge => Violation::StylesheetsTooLarge,
                })
            })
    }

    /// Invariants of the whole batch: `id` values unique among live elements.
    fn finish(&mut self) -> Result<(), ViewError> {
        self.charge_cascade_read(None)?;
        self.charge_select_work(self.tree.select_inventory_work(), None)?;
        self.tree
            .validate_retained_text()
            .map_err(|violation| ViewError::Invalid {
                operation: None,
                violation,
            })?;
        let mut seen = BTreeSet::new();
        for (id, node) in &self.tree.nodes {
            if self
                .tree
                .element(*id)
                .is_some_and(|element| element.tag == Tag::Option)
                && self
                    .tree
                    .option_value(*id)
                    .encode_utf16()
                    .take(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2 + 1)
                    .count()
                    > varta_common::view::DRAFT_MAX_VALUE_BYTES / 2
            {
                return Err(ViewError::Invalid {
                    operation: None,
                    violation: Violation::StringTooLong,
                });
            }
            if let Content::Element(element) = &node.content
                && element.tag == Tag::Select
                && element
                    .value
                    .encode_utf16()
                    .take(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2 + 1)
                    .count()
                    > varta_common::view::DRAFT_MAX_VALUE_BYTES / 2
            {
                return Err(ViewError::Invalid {
                    operation: None,
                    violation: Violation::StringTooLong,
                });
            }
            if let Content::Element(element) = &node.content
                && let Some(ident) = element.attributes.get(&Attribute::Id)
                && !seen.insert(ident.as_str())
            {
                return Err(ViewError::Invalid {
                    operation: None,
                    violation: Violation::DuplicateIdent(ident.clone()),
                });
            }
        }
        if crate::naming_work::potential_work(self.tree) > varta_common::view::DRAFT_MAX_NAMING_WORK
        {
            return Err(ViewError::Invalid {
                operation: None,
                violation: Violation::NamingWorkLimit,
            });
        }
        Ok(())
    }
}

/// Move focus to `next` and return the `focusout`/`focusin` records of the
/// change (the records bubble through the scheduler like any other).
pub(crate) fn focus_records(tree: &mut Tree, next: Option<u32>) -> Vec<crate::EventRecord> {
    let previous = tree.focus;
    if previous == next {
        return Vec::new();
    }
    if let Some(previous) = previous
        && let Some(element) = tree.element_mut(previous)
    {
        element.editor.preedit = None;
        element.editor.select_popup = None;
    }
    tree.focus = next;
    let mut records = Vec::new();
    if let Some(previous) = previous {
        records.push(crate::EventRecord::occurrence(
            EventKind::Focusout,
            previous,
        ));
    }
    if let Some(next) = next {
        records.push(crate::EventRecord::occurrence(EventKind::Focusin, next));
    }
    records
}

/// The native document of the shared style engine.
impl varta_style::Document for Tree {
    fn element(&self, id: u32) -> Option<varta_style::Element<'_>> {
        Tree::element(self, id).map(|element| varta_style::Element {
            tag: element.tag,
            class: &element.class,
            style: &element.style,
            disabled: element.disabled(),
            checked: element.checked,
        })
    }
    fn parent(&self, id: u32) -> Option<u32> {
        self.nodes.get(&id).and_then(|node| node.parent)
    }
    fn children(&self, id: u32) -> &[u32] {
        self.document_children(id)
    }
    fn top_level(&self) -> Vec<u32> {
        self.nodes
            .iter()
            .filter_map(|(id, node)| {
                (node.parent.is_none() && node.generated.is_none()).then_some(*id)
            })
            .collect()
    }
    fn pseudo(&self, id: u32) -> Option<(u32, varta_style::PseudoElement)> {
        let node = self.nodes.get(&id)?;
        Some((node.parent?, node.generated.as_ref()?.kind))
    }
    fn focused(&self, id: u32) -> bool {
        self.focus == Some(id)
    }
    fn hovered(&self, id: u32) -> bool {
        self.pointer_contains(self.hovered.as_ref(), id)
    }
    fn active(&self, id: u32) -> bool {
        self.pointer_contains(self.active.as_ref(), id)
    }
    fn attribute(&self, id: u32, name: &str) -> Option<&str> {
        if name.starts_with(varta_common::view::DATA_ATTRIBUTE_PREFIX) {
            return Tree::element(self, id)?
                .data
                .values
                .get(name)
                .map(String::as_str);
        }
        let attribute = Attribute::ALL
            .iter()
            .copied()
            .find(|attribute| attribute.name() == name)?;
        Tree::element(self, id)?
            .attributes
            .get(&attribute)
            .map(String::as_str)
    }
    fn value(&self, id: u32) -> Option<&str> {
        let element = Tree::element(self, id)?;
        (element.text_field() || element.tag == Tag::Select).then_some(element.value.as_str())
    }
    fn text_data(&self, id: u32) -> Option<bool> {
        match &self.nodes.get(&id)?.content {
            Content::Text(text) => Some(!text.is_empty()),
            Content::Element(_) => None,
        }
    }
    fn state(&self, id: u32) -> varta_style::ElementState {
        let Some(element) = Tree::element(self, id) else {
            return varta_style::ElementState::default();
        };
        // A number field's text that is no valid floating-point number is
        // the user's bad input (HTML §4.10.5.1.12).
        let bad_input = element.tag == Tag::Input
            && element.input_type() == "number"
            && !element.value.is_empty()
            && !varta_style::valid_float(&element.value);
        varta_style::ElementState {
            // The view shows dialogs only modally (`show_modal`).
            open: element.open,
            modal: element.open,
            bad_input,
            user_validity: element.editor.user_validity,
        }
    }
}
