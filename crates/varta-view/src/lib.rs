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
//! Layout rectangles are not computed: the Taffy/Parley stack of the freeze
//! criterion is not a dependency yet, so snapshots carry the tree, the
//! cascaded styles and the accessibility tree only.
mod events;
mod snapshot;
mod style;
mod tree;

mod wire;

pub use events::{Input, Platform, PromptAnswer, Script, ScriptError, Target};
pub use style::{DEFAULT_VIEWPORT_WIDTH, Environment};
pub use wire::{BatchWriter, EventRecord};

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
    #[error("too many live nodes")]
    TooManyNodes,
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
    #[error("view input key {0} is not in the key table")]
    Key(String),
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

    /// Validate `bytes` as one batch against the live tree and apply it, or
    /// leave the tree unchanged and fail. `gesture` is the scheduler's flag
    /// for the committing turn; it gates `copy_text`.
    pub fn commit(
        &mut self,
        bytes: &[u8],
        gesture: bool,
        broker: &mut dyn Broker,
    ) -> Result<(), ViewError> {
        let operations = wire::decode(bytes)?;
        let mut next = self.tree.clone();
        let effects = {
            let link = |url: &str| broker.link_requested(url);
            let context = tree::Context {
                link_requested: &link,
                environment: self.environment,
            };
            next.apply(&operations, &context)?
        };
        self.tree = next;
        self.commits += 1;
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
    #[must_use]
    pub fn snapshot(&self) -> serde_json::Value {
        snapshot::snapshot(&self.tree, &self.environment, &self.effects)
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
        id.filter(|id| self.tree.element(*id).is_some() && self.tree.attached(*id))
            .ok_or_else(|| InputError::Target(target.to_string()))
    }

    /// The root id (the host-created container).
    #[must_use]
    pub fn root() -> u32 {
        ROOT_NODE
    }
}
