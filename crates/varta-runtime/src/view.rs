//! The v38 L0 view protocol (`view_protocol()`): the `view_commit` import,
//! the view capabilities decided by permission broker v1 and the delivery of
//! view event records.
//!
//! [`Session`] owns the instance's headless renderer and the
//! commit-per-turn rule: each turn may commit at most one batch. The records
//! the renderer produces (from a committed batch or from a trusted platform
//! input) leave it at once and become host tasks of the instance
//! (`events::HostEvents::push_view`): every record runs as one event turn of
//! an application instance with fresh per-turn budgets, its gesture flag
//! makes a user-gesture turn (the broker's `gesture`), and coalescible kinds
//! follow the task queue's coalescing rule. The record reaches the guest in
//! its binary form through the synthesized export [`EVENT_EXPORT`]. Platform
//! inputs are host work between turns ([`input`]), never a turn themselves.
use crate::events::HostEvents;
use crate::permission::{Broker as PermissionBroker, ViewPrompts};
use crate::{RuntimeContext, RuntimeError, policy::Policy};
use varta_common::{HostOperation, abi, view};
use varta_view::{
    Access, Broker, Environment, EventRecord, Input, InputError, Platform, View, ViewError,
};
use wasmtime::{Caller, Extern, Instance, Linker, Store};

pub(crate) use view::EVENT_EXPORT;

/// One instance's view state.
#[derive(Default)]
pub(crate) struct Session {
    view: View,
    /// A batch was committed in the current turn.
    committed: bool,
    /// The platform is simulated (a scripted headless session): it answers
    /// the prompts the broker asks for when the instance has no prompt seam.
    simulated: bool,
    /// An application instance hosts the view and delivers its records; a
    /// bounded execution renders but has no event delivery, so its records
    /// are discarded.
    hosted: bool,
}

impl Session {
    /// The view of an instance whose embedder renders it headlessly with a
    /// simulated `platform`.
    pub(crate) fn simulated(environment: Environment, platform: Platform) -> Self {
        Self {
            view: View::new(environment, platform),
            simulated: true,
            hosted: true,
            ..Self::default()
        }
    }
    /// The view of an application instance on a host with its own prompt
    /// seam (or none).
    pub(crate) fn hosted() -> Self {
        Self {
            hosted: true,
            ..Self::default()
        }
    }
    /// A new turn starts: its commit allowance.
    pub(crate) fn begin_turn(&mut self) {
        self.committed = false;
    }
    pub(crate) fn snapshot(&self) -> serde_json::Value {
        self.view.snapshot()
    }
    pub(crate) fn commits(&self) -> usize {
        self.view.commits()
    }
    /// Whether a committed view can still receive platform inputs (and so
    /// records): it keeps an application instance alive like an open event
    /// source.
    pub(crate) fn attached(&self) -> bool {
        self.view.commits() > 0
    }
    /// Move the renderer's records into the instance's task queue.
    fn queue(&mut self, events: &mut HostEvents) -> Result<(), ViewError> {
        for delivery in self.view.drain() {
            if self.hosted {
                events.push_view(delivery)?;
            } else {
                // Undelivered: the record leaves the renderer as delivered.
                self.view.delivered(&delivery.record);
            }
        }
        Ok(())
    }
}

/// The view capabilities, decided by permission broker v1: the package
/// request within the host ceiling, broker states (revocation included),
/// the audit log and, for `ui.open_url`, the prompt rule.
struct ViewBroker<'a> {
    broker: &'a mut PermissionBroker,
    policy: &'a Policy,
    prompts: ViewPrompts,
}
impl Broker for ViewBroker<'_> {
    fn link_requested(&self, url: &str) -> bool {
        self.policy.link_requested(url)
    }
    fn open_url(&mut self, url: &str) -> Access {
        self.broker.open_url(self.policy, url, self.prompts)
    }
    fn clipboard_write(&mut self) -> Access {
        self.broker.clipboard_write(self.policy)
    }
    fn prompted(&mut self, operation: view::ResultOperation, resource: &str, allowed: bool) {
        let id = match operation {
            view::ResultOperation::OpenUrl => view::OPEN_URL_CAPABILITY,
            view::ResultOperation::CopyText => view::CLIPBOARD_WRITE_CAPABILITY,
        };
        self.broker.prompted(id, resource, allowed);
    }
}

fn fatal(error: ViewError) -> wasmtime::Error {
    wasmtime::Error::new(error)
}

pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        HostOperation::ViewCommit.name(),
        |mut caller: Caller<'_, RuntimeContext>,
         ptr: i32,
         len: i32|
         -> Result<i32, wasmtime::Error> {
            caller.data_mut().tick()?;
            let bytes = batch(&mut caller, ptr, len)?;
            let RuntimeContext {
                view,
                broker,
                policy,
                events,
                ..
            } = caller.data_mut();
            if view.committed {
                return Err(fatal(ViewError::SecondCommit));
            }
            let gesture = broker.gesture;
            // Guest code is running: nothing may prompt now.
            let simulated = view.simulated;
            let mut decisions = ViewBroker {
                broker,
                policy,
                prompts: ViewPrompts {
                    seam: false,
                    simulated,
                },
            };
            view.view
                .commit(&bytes, gesture, &mut decisions)
                .map_err(fatal)?;
            view.committed = true;
            view.queue(events).map_err(fatal)?;
            Ok(abi::HostStatus::Ok as i32)
        },
    )?;
    Ok(())
}

/// Copy the batch out of guest memory: bounds-checked and length-capped
/// before any decoding.
fn batch(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<Vec<u8>, wasmtime::Error> {
    let invalid = |violation| {
        fatal(ViewError::Invalid {
            operation: None,
            violation,
        })
    };
    let (Ok(start), Ok(length)) = (usize::try_from(ptr), usize::try_from(len)) else {
        return Err(invalid(varta_view::Violation::Truncated));
    };
    if length > view::DRAFT_MAX_BATCH_BYTES {
        return Err(invalid(varta_view::Violation::BatchTooLarge));
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(wasmtime::Error::msg(
            "view_commit requires an exported memory",
        ));
    };
    memory
        .data(&*caller)
        .get(start..start.saturating_add(length))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| invalid(varta_view::Violation::Truncated))
}

/// Perform one trusted platform input between turns (host work, no guest
/// code): the renderer updates host-owned state and its records become host
/// tasks. A prompt it needs may use the instance's prompt seam.
pub(crate) fn input(context: &mut RuntimeContext, input: &Input) -> Result<(), RuntimeError> {
    let RuntimeContext {
        view,
        broker,
        policy,
        events,
        ..
    } = context;
    let simulated = view.simulated;
    let mut decisions = ViewBroker {
        broker,
        policy,
        prompts: ViewPrompts {
            seam: true,
            simulated,
        },
    };
    view.view
        .input(input, &mut decisions)
        .map_err(RuntimeError::ViewInput)?;
    view.queue(events).map_err(RuntimeError::View)
}

/// Run a view record's turn: the renderer learns it was delivered, the
/// record is written at the host result buffer and the guest export runs.
pub(crate) fn deliver(
    instance: &Instance,
    store: &mut Store<RuntimeContext>,
    record: &EventRecord,
) -> Result<(), RuntimeError> {
    store.data_mut().view.view.delivered(record);
    let record = record.encode();
    let Some(export) = instance.get_func(&mut *store, EVENT_EXPORT) else {
        return Err(
            wasmtime::Error::msg("view event record without the guest event export").into(),
        );
    };
    let export = export.typed::<i32, ()>(&*store)?;
    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| wasmtime::Error::msg("view events require an exported memory"))?;
    let desired = abi::RESULT_BASE + record.len();
    if desired > memory.data_size(&*store) {
        let pages = (desired - memory.data_size(&*store)).div_ceil(abi::WASM_PAGE_BYTES);
        memory.grow(&mut *store, pages as u64)?;
    }
    memory
        .write(&mut *store, abi::RESULT_BASE, &record)
        .map_err(wasmtime::Error::new)?;
    store.data().check_budget()?;
    export.call(&mut *store, record.len() as i32)?;
    Ok(())
}

/// Map view failures inside guest calls to their typed errors.
pub(crate) fn typed(source: wasmtime::Error) -> Result<RuntimeError, wasmtime::Error> {
    match source.downcast::<ViewError>() {
        Ok(error) => Ok(RuntimeError::View(error)),
        Err(source) => Err(source),
    }
}

impl From<InputError> for RuntimeError {
    fn from(error: InputError) -> Self {
        Self::ViewInput(error)
    }
}
