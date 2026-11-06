#![forbid(unsafe_code)]
//! The headless renderer of the L0 view protocol (draft v0, profile
//! `checked-f64-utf16-v38`, `view_protocol()`).
//!
//! It is the reference applier: it decodes and validates each committed
//! batch as a whole against the `varta_common::view` registry before
//! applying any operation (fail closed, atomic), keeps the node tree,
//! stylesheets and field state, simulates the platform side of the
//! protocol (trusted inputs, bubbling, key filters, default actions, the
//! clipboard and the system browser) and serializes a deterministic tree and
//! accessibility snapshot. It has no Wasm, file, network or clock access:
//! the embedding runtime owns the guest, the scheduler and the permission
//! broker, and supplies broker decisions through [`Broker`].
//!
//! Snapshots carry the tree, cascaded styles and accessibility tree; a host
//! with explicit fonts may add the opt-in `geometry` section of layout
//! rectangles ([`View::snapshot_with_geometry`]). The separate experimental
//! layout and CPU raster methods support bounded box and explicit-font text
//! subsets; full native rendering is not implemented yet.
mod accessibility;
mod drawing;
mod events;
mod geometry;
mod glyph_raster;
mod inline;
mod layout;
mod list;
mod name;
mod naming_work;
mod paint;
mod platform_effects;
mod render;
mod select;
mod snapshot;
mod snapshot_json;
mod style;
mod text;
mod transform;
mod transition;
mod tree;
mod units;

mod variables;
mod wire;

pub use accessibility::{
    ACCESSIBILITY_TEXT_RUN_CHARACTERS, ACCESSIBILITY_TEXT_RUN_LIMIT, ACCESSIBILITY_TEXT_WORK_LIMIT,
    AccessibilityAction, AccessibilityError, AccessibilityGeometry, AccessibilityNode,
    AccessibilityScrollAction, AccessibilityScrollHint, AccessibilityScrollOffset,
    AccessibilityScrollUnit, AccessibilitySelection, AccessibilityState, AccessibilityText,
    AccessibilityTextRevision, AccessibilityTextRun, AccessibilityTree, AccessibilityWork,
};
pub use drawing::{
    ColorCell, DisplayList, DrawClip, DrawCommand, DrawPrimitive, PathSegment, PreparedFrame,
};
pub use events::{
    ClipboardAction, ClipboardData, ClipboardError, ClipboardKey, ClipboardTarget, ClipboardTask,
    ClipboardToken, HistoryAction, Input, Platform, PromptAnswer, Script, ScriptError, ScriptStep,
    Target,
};
pub use geometry::{DEFAULT_VIEWPORT_HEIGHT, GEOMETRY_KEY, GeometryRequest};
pub use glyph_raster::{
    GlyphRasterBounds, GlyphRasterError, GlyphRasterMask, GlyphRasterRequest, GlyphRasterizer,
};
pub use layout::{
    BoxGeometry, BoxRect, CLASSIC_SCROLLBAR_THICKNESS, LayoutError, ScrollMetrics,
    ScrollbarGeometry,
};
pub use paint::{
    FieldTextCluster, FieldTextGeometry, FieldTextLine, FrameMetadata, PaintError, PointerCursor,
    RasterImage, SelectHit, SelectMenuMetrics, TextNavigation,
};
pub use platform_effects::{PlatformEffects, PlatformRefusal};
pub use render::{RenderStats, Renderer};
pub use snapshot::project_flex_blockification;
pub use snapshot_json::{SnapshotError, SnapshotFormat, encode_snapshot};
pub use style::{DEFAULT_VIEWPORT_WIDTH, Environment, NATIVE_UA_VERSION, Scrollbars};
pub use text::fallback::{
    FallbackFace, FallbackRefusal, FallbackRequest, FontFallback, MAX_FALLBACK_BYTES,
    MAX_FALLBACK_FACE_BYTES, MAX_FALLBACK_FACES, MAX_FALLBACK_PASSES,
};
pub use text::{ENLARGED_ASCENT_FRACTION, TextFont, TextMeasureError, TextMeasurer, TextMetrics};
pub use wire::{BatchWriter, EventRecord};

/// Whether a number field's value is valid under Chrome's number
/// sanitization, the grammar native Up/Down stepping reads (an invalid or
/// empty value steps from 0).
#[must_use]
pub fn number_value_is_valid(value: &str) -> bool {
    events::number_step::is_valid(value)
}

use std::collections::VecDeque;
use varta_common::view::{
    CommandStatus, DRAFT_MAX_PENDING_EVENTS, EventKind, ROOT_NODE, ResultOperation,
};

/// A `listen` key filter and options (wire operands after the handler).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Filter {
    pub options: u32,
    /// A key code, or `view::ANY_KEY`.
    pub key: u16,
    pub required: u32,
    pub forbidden: u32,
}

/// Why a batch is invalid; every violation rejects the whole batch.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Violation {
    #[error("batch exceeds the batch byte limit")]
    BatchTooLarge,
    #[error("batch is truncated")]
    Truncated,
    #[error("unsupported batch version")]
    Version,
    #[error("nonzero batch flags")]
    Flags,
    #[error("too many operations")]
    TooManyOperations,
    #[error("unknown opcode")]
    UnknownOpcode,
    #[error("nonzero padding")]
    Padding,
    #[error("bytes after the last operation")]
    TrailingBytes,
    #[error("string exceeds its byte limit")]
    StringTooLong,
    #[error("single-select work exceeds its transaction limit")]
    SelectWorkLimit,
    #[error("radio reconciliation work exceeds its transaction limit")]
    RadioWorkLimit,
    #[error("retained text and field reservations exceed the view limit")]
    RetainedTextLimit,
    #[error("string byte length is odd")]
    OddStringLength,
    #[error("string contains a lone surrogate")]
    LoneSurrogate,
    #[error("string contains U+0000")]
    NulCharacter,
    #[error("unknown tag code")]
    UnknownTag,
    #[error("unknown attribute code")]
    UnknownAttribute,
    #[error("unknown CSS property code")]
    UnknownProperty,
    #[error("unknown event code")]
    UnknownEvent,
    #[error("unknown command code")]
    UnknownCommand,
    #[error("unknown key code")]
    UnknownKey,
    #[error("unknown or dead node {0}")]
    UnknownNode(u32),
    #[error("node id {0} is zero or already live")]
    DuplicateNode(u32),
    #[error("node {0} is a text node")]
    NotAnElement(u32),
    #[error("node {0} is not a text node")]
    NotAText(u32),
    #[error("node {0} is not a field of this operation")]
    NotAField(u32),
    #[error("node {0} is not attached")]
    Detached(u32),
    #[error("attribute `{0}` is not allowed on this element")]
    AttributeNotAllowed(&'static str),
    #[error("invalid value for attribute `{0}`")]
    InvalidAttributeValue(&'static str),
    #[error("duplicate `id` value {0}")]
    DuplicateIdent(String),
    #[error("invalid class list")]
    InvalidClass,
    #[error("invalid style value")]
    InvalidStyleValue,
    #[error("`font-family` names the CSS-wide keyword `{0}`, which L0 does not admit")]
    FontFamilyKeyword(&'static str),
    #[error("CSS property not allowed on this element")]
    PropertyNotAllowed,
    #[error("the root cannot be inserted or removed")]
    RootNotMovable,
    #[error("element does not admit this child")]
    ChildNotAllowed,
    #[error("insertion would create a cycle")]
    Cycle,
    #[error("`before` node {0} is not a child of the parent")]
    InvalidBefore(u32),
    #[error("handler {0} is zero or already live")]
    DuplicateHandler(u32),
    #[error("handler {0} is not listening there")]
    NotListening(u32),
    #[error("identical listener filter twice")]
    DuplicateFilter,
    #[error("invalid listener options or key filter")]
    InvalidFilter,
    #[error("event kind cannot be listened to")]
    NotListenable,
    #[error("view tree exceeds its component depth limit")]
    TreeDepthLimit,
    #[error("too many live nodes")]
    TooManyNodes,
    #[error("cannot compute geometry for scroll_into_view")]
    ScrollLayout,
    #[error("too many live listeners")]
    TooManyListeners,
    #[error("checked must be 0 or 1")]
    InvalidChecked,
    #[error("command `{0}` is not allowed on this element")]
    CommandNotAllowed(&'static str),
    #[error("invalid command argument")]
    InvalidArgument,
    #[error("stylesheet id must be nonzero")]
    InvalidSheet,
    #[error("invalid stylesheet")]
    InvalidStylesheet,
    #[error("stylesheets exceed the stylesheet byte limit")]
    StylesheetsTooLarge,
    #[error("potential cascade reads exceed their transaction limit")]
    CascadeWorkLimit,
    #[error("potential expanded component values exceed their transaction limit")]
    ExpandedValueLimit,
    #[error("potential accessible-name work exceeds its inventory limit")]
    NamingWorkLimit,
    #[error("stylesheet parsing or inventory work exceeds its limit")]
    StylesheetWorkLimit,
    #[error("request id {0} is zero or already pending")]
    DuplicateRequest(u32),
    #[error("too many pending commands")]
    TooManyPendingCommands,
}

/// A fatal view error of the instance.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ViewError {
    /// An invalid batch: nothing of it was applied.
    #[error("invalid view batch{}: {violation}", operation.map(|index| format!(" (operation {index})")).unwrap_or_default())]
    Invalid {
        operation: Option<usize>,
        violation: Violation,
    },
    /// A second `view_commit` in one turn.
    #[error("view_commit called twice in one turn")]
    SecondCommit,
    /// A non-coalescible event record found the pending-event queue full.
    #[error("pending view events exceed the limit")]
    QueueOverflow,
}

/// A failure of a scripted platform input (a tooling error, not a guest one).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum InputError {
    #[error("view input target {0} is not an attached element")]
    Target(String),
    /// A modal dialog blocks the target: a platform input cannot reach it
    /// (a pointer lands on the dialog's backdrop, focus stays in the
    /// dialog), so a scripted input naming it is refused.
    #[error("view input target {0} is inert under a modal dialog")]
    Inert(String),
    #[error("view input key {0} is not in the key table")]
    Key(String),
    #[error("view text selection is invalid for the current field value")]
    Selection,
    #[error("view scroll geometry is invalid")]
    ScrollGeometry,
    #[error("view select choice is invalid")]
    Choice,
    #[error(transparent)]
    View(#[from] ViewError),
}

/// A failure while applying a layout-dependent scroll request.
#[derive(Debug, thiserror::Error)]
pub enum ScrollIntoViewError {
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error(transparent)]
    View(#[from] ViewError),
}

/// A permission broker answer for a view capability (`ui.open_url`,
/// `clipboard.write`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Package request ∩ host ceiling allow it.
    Granted,
    /// Outside the request or the ceiling: `not_allowed`, no prompt.
    Denied,
    /// Allowed only by the any-HTTPS form: every activation prompts, and no
    /// decision is stored.
    Prompt,
}

/// The embedder's side of the view capabilities. The renderer never learns
/// grants from the guest.
pub trait Broker {
    /// Patch time: whether an external `href` matches a `ui.open_url`
    /// pattern of the package request.
    fn link_requested(&self, url: &str) -> bool;
    /// Activation time: the broker decision for opening `url`.
    fn open_url(&mut self, url: &str) -> Access;
    /// The broker decision for one `copy_text` in a user-gesture turn.
    fn clipboard_write(&mut self) -> Access;
    /// The simulated platform answered a prompt the broker asked for
    /// ([`Access::Prompt`]) for `operation` on `resource` (the full URL of an
    /// `open_url`); the embedder records the outcome. Nothing is stored.
    fn prompted(&mut self, _operation: ResultOperation, _resource: &str, _allowed: bool) {}
}

/// A record waiting in the host's pending-event queue.
#[derive(Debug, Clone, PartialEq)]
struct Pending {
    record: EventRecord,
    /// Produced by the host from a real (scripted) platform input.
    trusted: bool,
}

/// One delivery: the record and whether its turn is a user-gesture turn.
#[derive(Debug, Clone, PartialEq)]
pub struct Delivery {
    pub record: EventRecord,
    pub gesture: bool,
}

/// Opaque identity of one source node, distinct across ID reuse.
/// Element-action APIs still validate attachment and element type.
#[derive(Debug, Clone)]
pub struct NodeIdentity {
    id: u32,
    token: std::sync::Arc<()>,
}
impl PartialEq for NodeIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && std::sync::Arc::ptr_eq(&self.token, &other.token)
    }
}
impl Eq for NodeIdentity {}

/// One instance's view: the live tree, the pending-event queue and the
/// simulated platform.
#[derive(Debug, Clone, Default)]
pub struct View {
    tree: tree::Tree,
    queue: VecDeque<Pending>,
    platform: Platform,
    effects: events::Effects,
    environment: Environment,
    /// Batches applied so far (for diagnostics and snapshots).
    commits: usize,
    history: VecDeque<events::HistoryEntry>,
    pending_clipboard: Option<ClipboardToken>,
    /// The embedder's real clipboard and system browser, when it has them.
    platform_effects: platform_effects::Sink,
}

impl View {
    #[must_use]
    pub fn new(environment: Environment, platform: Platform) -> Self {
        Self {
            environment,
            platform,
            ..Self::default()
        }
    }

    /// Perform granted `copy_text` and external link effects on the
    /// embedder's real platform instead of simulating them
    /// ([`PlatformEffects`]).
    #[must_use]
    pub fn with_platform_effects(mut self, effects: std::sync::Arc<dyn PlatformEffects>) -> Self {
        self.platform_effects = Some(effects);
        self
    }

    /// Capture an attached element for a deferred host action.
    #[must_use]
    pub fn node_identity(&self, id: u32) -> Option<NodeIdentity> {
        let node = self.tree.nodes.get(&id)?;
        if self.tree.element(id).is_none() || !self.tree.attached(id) {
            return None;
        }
        Some(NodeIdentity {
            id,
            token: node.identity.clone(),
        })
    }

    /// Resolve a deferred identity only while that same element is attached.
    #[must_use]
    pub fn resolve_node_identity(&self, identity: &NodeIdentity) -> Option<u32> {
        (self.node_identity(identity.id).as_ref() == Some(identity)).then_some(identity.id)
    }

    /// Update trusted pointing-device CSS states between turns. Targets and
    /// their ancestors match; identity guards prevent transfer across ID reuse.
    /// This neither activates a control nor dispatches guest events.
    pub fn set_pointer_state(&mut self, hovered: Option<u32>, active: Option<u32>) -> bool {
        let hovered = hovered.and_then(|id| self.node_identity(id));
        let active = active.and_then(|id| self.node_identity(id));
        if self.tree.hovered == hovered && self.tree.active == active {
            return false;
        }
        self.tree.hovered = hovered;
        self.tree.active = active;
        self.restyle();
        true
    }

    /// Move the animation clock to `ms` milliseconds on the host's time line
    /// (it never moves back; a scripted session uses its virtual loop time).
    /// Returns whether a running CSS transition's value may have changed, so
    /// a new frame should be presented.
    pub fn set_animation_time(&mut self, ms: f64) -> bool {
        self.tree.animations.advance(ms)
    }

    /// Whether a CSS transition is running: the host keeps presenting frames
    /// at its display rate, advancing the clock, until none is.
    #[must_use]
    pub fn animating(&self) -> bool {
        self.tree.animations.active()
    }

    /// One style change event at the current animation clock: start, keep,
    /// reverse or cancel transitions ([`transition`]).
    fn restyle(&mut self) {
        transition::Animations::restyle(&mut self.tree, &self.environment);
    }

    /// Update host viewport conditions used by the cascade and platform input.
    pub fn set_environment(&mut self, environment: Environment) -> Result<(), LayoutError> {
        if !environment.width.is_finite() || !(0.01..=1_000_000.0).contains(&environment.width) {
            return Err(LayoutError::Viewport);
        }
        units::Units::new(&environment).ok_or(LayoutError::RootFont)?;
        let changed = self.environment != environment;
        self.environment = environment;
        if changed {
            self.restyle();
        }
        Ok(())
    }

    /// The environment currently used for style evaluation.
    #[must_use]
    pub fn environment(&self) -> Environment {
        self.environment
    }

    /// Cancel transient IME state on the currently focused field, if any.
    pub fn cancel_preedit(&mut self) {
        if let Some(id) = self.tree.focus
            && let Some(element) = self.tree.element_mut(id)
        {
            element.editor.preedit = None;
        }
    }

    /// Validate `bytes` as one batch against the live tree and apply it, or
    /// leave the tree unchanged and fail. `gesture` is the scheduler's flag
    /// for the committing turn; it gates `copy_text`.
    pub fn commit(
        &mut self,
        bytes: &[u8],
        gesture: bool,
        broker: &mut dyn Broker,
    ) -> Result<(), ViewError> {
        self.commit_impl(bytes, gesture, broker, None)
    }

    /// Apply a batch with geometry available to layout-dependent commands.
    /// The geometry is evaluated at the command's position in the batch.
    pub fn commit_with_layout(
        &mut self,
        bytes: &[u8],
        gesture: bool,
        broker: &mut dyn Broker,
        width: f32,
        height: f32,
        font: Option<(&mut TextMeasurer, f32)>,
    ) -> Result<(), ViewError> {
        match font {
            Some((measurer, size)) => measurer.with_view_shaping(|measurer| {
                self.commit_impl(
                    bytes,
                    gesture,
                    broker,
                    Some(tree::LayoutContext {
                        width,
                        height,
                        font: Some((measurer, size)),
                    }),
                )
            }),
            None => self.commit_impl(
                bytes,
                gesture,
                broker,
                Some(tree::LayoutContext {
                    width,
                    height,
                    font: None,
                }),
            ),
        }
    }

    fn commit_impl(
        &mut self,
        bytes: &[u8],
        gesture: bool,
        broker: &mut dyn Broker,
        mut layout: Option<tree::LayoutContext<'_>>,
    ) -> Result<(), ViewError> {
        let operations = wire::decode(bytes)?;
        let mut next = self.tree.clone();
        let effects = {
            let link = |url: &str| broker.link_requested(url);
            let mut context = tree::Context {
                link_requested: &link,
                environment: self.environment,
                layout: layout.as_mut().map(|layout| tree::LayoutContext {
                    width: layout.width,
                    height: layout.height,
                    font: layout
                        .font
                        .as_mut()
                        .map(|(font, size)| (&mut **font, *size)),
                }),
            };
            next.apply(&operations, &mut context)?
        };
        self.tree = next;
        self.prune_history();
        self.commits += 1;
        self.restyle();
        for effect in effects {
            match effect {
                tree::Effect::Copy { request, text } => {
                    let status = self.copy(gesture, &text, broker);
                    let mut record = EventRecord::new(EventKind::CommandResult, 0, 0);
                    record.seq = request;
                    record.status = status.code();
                    record.key = ResultOperation::CopyText.name().into();
                    self.enqueue(record, true)?;
                }
                tree::Effect::Record(occurrence) => {
                    self.dispatch(occurrence, false)?;
                }
                tree::Effect::ScrollRecord { record, identity } => {
                    if self
                        .tree
                        .nodes
                        .get(&record.target)
                        .is_some_and(|node| std::sync::Arc::ptr_eq(&node.identity, &identity))
                    {
                        self.dispatch(record, false)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn copy(&mut self, gesture: bool, text: &str, broker: &mut dyn Broker) -> CommandStatus {
        if !gesture {
            return CommandStatus::InvalidContext;
        }
        let granted = match broker.clipboard_write() {
            Access::Granted => true,
            Access::Prompt => {
                let allowed = self.prompt_allows();
                broker.prompted(ResultOperation::CopyText, "", allowed);
                allowed
            }
            Access::Denied => false,
        };
        if !granted {
            return CommandStatus::NotAllowed;
        }
        self.write_clipboard(text)
    }

    /// Queue a record, coalescing a pending record of a coalescible kind for
    /// the same listener; a full queue drops a coalescible record and is
    /// fatal for any other.
    fn enqueue(&mut self, record: EventRecord, trusted: bool) -> Result<(), ViewError> {
        if record.kind.coalescible()
            && let Some(pending) = self.queue.iter_mut().find(|pending| {
                pending.record.kind == record.kind && pending.record.handler == record.handler
            })
        {
            *pending = Pending { record, trusted };
            return Ok(());
        }
        if self.queue.len() >= DRAFT_MAX_PENDING_EVENTS {
            if record.kind.coalescible() {
                return Ok(());
            }
            return Err(ViewError::QueueOverflow);
        }
        self.queue.push_back(Pending { record, trusted });
        Ok(())
    }

    /// Take the next pending record (one per turn), in FIFO order, as
    /// delivered.
    pub fn next_delivery(&mut self) -> Option<Delivery> {
        let delivery = Self::delivery(self.queue.pop_front()?);
        self.delivered(&delivery.record);
        Some(delivery)
    }

    /// Move every pending record out, in FIFO order, to an embedder whose
    /// own task queue orders, coalesces and bounds them; it reports each one
    /// with [`Self::delivered`] when its turn runs.
    pub fn drain(&mut self) -> Vec<Delivery> {
        self.queue.drain(..).map(Self::delivery).collect()
    }

    fn delivery(pending: Pending) -> Delivery {
        Delivery {
            gesture: pending.trusted && pending.record.kind.user_gesture(),
            record: pending.record,
        }
    }

    /// A record's turn runs: a delivered `copy_text` result releases its
    /// request id.
    pub fn delivered(&mut self, record: &EventRecord) {
        if record.kind == EventKind::CommandResult && record.key == ResultOperation::CopyText.name()
        {
            self.tree.pending_requests.remove(&record.seq);
        }
    }

    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Batches applied so far.
    #[must_use]
    pub fn commits(&self) -> usize {
        self.commits
    }

    /// The deterministic headless snapshot: the node tree with attributes,
    /// text, field state and computed styles; the accessibility tree; the
    /// title, focus, stylesheets and the simulated platform's effects.
    /// This is structured inspection; use [`Self::snapshot_json`] or
    /// [`encode_snapshot`] for bounded JSON export. Caller-owned copies and
    /// independently chosen serialization remain the embedder's responsibility.
    #[must_use]
    pub fn snapshot(&self) -> serde_json::Value {
        snapshot::snapshot(&self.tree, &self.environment, &self.effects)
    }

    /// The headless snapshot with its opt-in `geometry` section: the
    /// viewport, the base text size and one border-box rectangle with its
    /// used display per displayed element, in document order, laid out with
    /// the request's explicit fonts. Every other key equals [`Self::snapshot`].
    /// Unsupported content or CSS fails the whole request.
    pub fn snapshot_with_geometry(
        &self,
        request: &mut GeometryRequest,
    ) -> Result<serde_json::Value, LayoutError> {
        let section = geometry::section(&self.tree, &self.environment, request)?;
        let mut value = self.snapshot();
        value[GEOMETRY_KEY] = section;
        Ok(value)
    }

    /// Export diagnostic JSON under its separate host byte allowance. A refused
    /// export does not alter the tree, event queue or simulated platform effects.
    pub fn snapshot_json(&self, format: SnapshotFormat) -> Result<Vec<u8>, SnapshotError> {
        encode_snapshot(&self.snapshot(), format)
    }

    /// Current guest title without constructing a complete view snapshot.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.tree.title
    }

    /// Compute experimental box-only geometry; unsupported content and CSS fail closed.
    pub fn layout_boxes(
        &self,
        width: f32,
        height: f32,
    ) -> Result<std::collections::BTreeMap<u32, BoxRect>, LayoutError> {
        layout::boxes(&self.tree, &self.environment, width, height)
    }

    /// Compute box rectangles and rectangular ancestor clips within the viewport.
    /// Rounded clipping, transforms and actual painting are not implemented.
    pub fn layout_geometry(
        &self,
        width: f32,
        height: f32,
    ) -> Result<std::collections::BTreeMap<u32, BoxGeometry>, LayoutError> {
        layout::geometry(&self.tree, &self.environment, width, height)
    }

    /// Paint supported, unpositioned box backgrounds into a transparent CPU image.
    /// Unsupported content or visual effects return an error instead of being omitted.
    pub fn rasterize_boxes(&self, width: u32, height: u32) -> Result<RasterImage, PaintError> {
        paint::rasterize(&self.tree, &self.environment, width, height)
    }

    /// Paint at physical resolution while measuring layout and fonts in CSS pixels.
    pub fn rasterize_scaled(
        &self,
        width: u32,
        height: u32,
        scale: f32,
        font: Option<(&mut TextMeasurer, f32)>,
    ) -> Result<RasterImage, PaintError> {
        paint::rasterize_scaled(&self.tree, &self.environment, width, height, scale, font)
    }

    /// Paint supported boxes and explicitly supplied-font text into a CPU image.
    pub fn rasterize_with_text(
        &self,
        width: u32,
        height: u32,
        measurer: &mut TextMeasurer,
        font_size: f32,
    ) -> Result<RasterImage, PaintError> {
        paint::rasterize_with_text(
            &self.tree,
            &self.environment,
            width,
            height,
            measurer,
            font_size,
        )
    }

    /// Compute rectangular clips and boxes with the experimental text measurer.
    pub fn layout_geometry_with_text(
        &self,
        width: f32,
        height: f32,
        measurer: &mut TextMeasurer,
        font_size: f32,
    ) -> Result<std::collections::BTreeMap<u32, BoxGeometry>, LayoutError> {
        layout::geometry_with_text(
            &self.tree,
            &self.environment,
            width,
            height,
            measurer,
            font_size,
        )
    }

    /// Plan nearest-edge vertical scrolling using the current experimental geometry.
    /// Changes are ordered from inner to outer scrollports and are not applied.
    /// Positioned targets scroll only the scrollports of their containing-block chain.
    pub fn scroll_into_view_plan(
        &self,
        target: u32,
        width: f32,
        height: f32,
        font: Option<(&mut TextMeasurer, f32)>,
    ) -> Result<Vec<(u32, f64)>, LayoutError> {
        let geometry = match font {
            Some((measurer, size)) => {
                self.layout_geometry_with_text(width, height, measurer, size)?
            }
            None => self.layout_geometry(width, height)?,
        };
        layout::scroll_into_view_plan(&self.tree, &self.environment, &geometry, target)
    }

    /// Apply the current nearest-edge vertical scroll plan atomically.
    /// Scroll records are delivered without a user-gesture grant.
    pub fn scroll_into_view(
        &mut self,
        target: u32,
        width: f32,
        height: f32,
        font: Option<(&mut TextMeasurer, f32)>,
    ) -> Result<usize, ScrollIntoViewError> {
        let geometry = match font {
            Some((measurer, size)) => {
                self.layout_geometry_with_text(width, height, measurer, size)?
            }
            None => self.layout_geometry(width, height)?,
        };
        let plan = layout::scroll_into_view_plan(&self.tree, &self.environment, &geometry, target)?;
        let mut next = self.clone();
        for &(id, top) in &plan {
            let Some(element) = next.tree.element_mut(id) else {
                return Err(LayoutError::Unsupported(id).into());
            };
            element.scroll_top = top;
            let scroll = geometry[&id]
                .programmatic_scroll
                .ok_or(LayoutError::Unsupported(id))?;
            let mut record = EventRecord::occurrence(EventKind::Scroll, id);
            record.scroll_top = top;
            record.client_height = f64::from(scroll.viewport.height);
            record.scroll_height = f64::from(scroll.viewport.height + scroll.max);
            next.dispatch(record, false)?;
        }
        *self = next;
        Ok(plan.len())
    }

    /// Apply a deferred scroll only if its original target is still attached.
    pub fn scroll_into_view_identity(
        &mut self,
        identity: &NodeIdentity,
        width: f32,
        height: f32,
        font: Option<(&mut TextMeasurer, f32)>,
    ) -> Result<usize, ScrollIntoViewError> {
        match self.resolve_node_identity(identity) {
            Some(target) => self.scroll_into_view(target, width, height, font),
            None => Ok(0),
        }
    }

    /// Experimental layout for isolated text in flex/grid or single-text-child blocks.
    /// The caller supplies a covering font and base size; CSS font size and line height inherit.
    /// General inline formatting is not supported.
    pub fn layout_boxes_with_text(
        &self,
        width: f32,
        height: f32,
        measurer: &mut TextMeasurer,
        font_size: f32,
    ) -> Result<std::collections::BTreeMap<u32, BoxRect>, LayoutError> {
        layout::boxes_with_text(
            &self.tree,
            &self.environment,
            width,
            height,
            measurer,
            font_size,
        )
    }

    /// Project explicit flex-item `display` values for layout comparison.
    /// Other computed-style and geometry rules are not modeled. The ordinary
    /// cascade snapshot remains available unchanged via [`Self::snapshot`].
    #[must_use]
    pub fn snapshot_flex_blockified(&self) -> serde_json::Value {
        let mut value = self.snapshot();
        project_flex_blockification(&mut value);
        value
    }

    /// The node an input target names, if it is an attached element.
    fn resolve(&self, target: &Target) -> Result<u32, InputError> {
        let id = match target {
            Target::Node(id) => Some(*id),
            Target::Ident(ident) => ident
                .strip_prefix('#')
                .and_then(|ident| self.tree.by_ident(ident)),
        };
        // A pointer on a text node targets its parent element, as in the DOM.
        let id = id.map(|id| match self.tree.element(id) {
            Some(_) => id,
            None => self
                .tree
                .nodes
                .get(&id)
                .and_then(|node| node.parent)
                .unwrap_or(id),
        });
        let id = id
            .filter(|id| self.tree.element(*id).is_some() && self.tree.attached(*id))
            .ok_or_else(|| InputError::Target(target.to_string()))?;
        if self.tree.inert(id) {
            return Err(InputError::Inert(target.to_string()));
        }
        Ok(id)
    }

    /// The root id (the host-created container).
    #[must_use]
    pub fn root() -> u32 {
        ROOT_NODE
    }
}
