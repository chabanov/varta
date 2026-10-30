//! The v38 L0 view protocol (`view_protocol()`): the `view_commit` import,
//! the permission broker of the view capabilities and the delivery of view
//! event records as scheduler tasks.
//!
//! **Interim turn seam.** The long-lived application instance and the
//! host→guest task API belong to the turn-budgets stream. Until it lands,
//! one execution hosts the view: `main` (with its microtask checkpoint) is
//! the first turn, and the scheduler's event loop then runs one task per
//! turn in this order: a pending view record (delivered through the guest
//! export [`EVENT_EXPORT`]), else the next scripted platform input (a
//! `varta dev` headless script; an input is host work, not a turn), else
//! the existing timer and network tasks. Each turn may commit at most one
//! batch. The integrator replaces [`step`] with the instance's task queue;
//! [`Session`] (renderer, queue, gesture flag and commit-per-turn rule) is
//! the part to keep.
use crate::{RuntimeContext, RuntimeError, policy::Policy};
use std::collections::VecDeque;
use varta_common::{HostOperation, abi, view};
use varta_view::{Access, Broker, Input, InputError, View, ViewError};
use wasmtime::{Caller, Extern, Instance, Linker, Store};

pub(crate) use view::EVENT_EXPORT;

/// One execution's view state.
#[derive(Default)]
pub(crate) struct Session {
    view: View,
    /// Scripted platform inputs not yet performed.
    inputs: VecDeque<Input>,
    /// A batch was committed in the current turn.
    committed: bool,
    /// The current turn's task is a trusted user-gesture record.
    gesture: bool,
}

impl Session {
    pub(crate) fn scripted(script: &varta_view::Script) -> Self {
        Self {
            view: View::new(script.environment(), script.platform.clone()),
            inputs: script.inputs.iter().cloned().collect(),
            ..Self::default()
        }
    }
    /// A new turn starts: its commit allowance and its gesture flag.
    pub(crate) fn begin_turn(&mut self, gesture: bool) {
        self.committed = false;
        self.gesture = gesture;
    }
    pub(crate) fn snapshot(&self) -> serde_json::Value {
        self.view.snapshot()
    }
    pub(crate) fn commits(&self) -> usize {
        self.view.commits()
    }
}

/// The view capabilities decided from the package request and the host
/// ceiling; the headless renderer has no stored decisions.
struct PolicyBroker<'a> {
    policy: &'a Policy,
}
impl Broker for PolicyBroker<'_> {
    fn link_requested(&self, url: &str) -> bool {
        self.policy.link_requested(url)
    }
    fn open_url(&mut self, url: &str) -> Access {
        self.policy.open_url(url)
    }
    fn clipboard_write(&mut self) -> Access {
        if self.policy.enabled(view::CLIPBOARD_WRITE_CAPABILITY) {
            Access::Granted
        } else {
            Access::Denied
        }
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
            let context = caller.data_mut();
            if context.view.committed {
                return Err(fatal(ViewError::SecondCommit));
            }
            let mut broker = PolicyBroker {
                policy: &context.policy,
            };
            let gesture = context.view.gesture;
            context
                .view
                .view
                .commit(&bytes, gesture, &mut broker)
                .map_err(fatal)?;
            context.view.committed = true;
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

/// Run one view task if one is ready: deliver the next pending record as a
/// turn, or perform the next scripted input. Returns whether it did anything.
pub(crate) fn step(
    instance: &Instance,
    store: &mut Store<RuntimeContext>,
) -> Result<bool, RuntimeError> {
    let context = store.data_mut();
    if let Some(delivery) = context.view.view.next_delivery() {
        context.view.begin_turn(delivery.gesture);
        deliver(instance, store, &delivery.record.encode())?;
        crate::exceptions::finish(instance, store)?;
        return Ok(true);
    }
    let Some(input) = context.view.inputs.pop_front() else {
        return Ok(false);
    };
    let mut broker = PolicyBroker {
        policy: &context.policy,
    };
    context
        .view
        .view
        .input(&input, &mut broker)
        .map_err(RuntimeError::ViewInput)?;
    Ok(true)
}

/// Write the record at the host result buffer and call the guest export.
fn deliver(
    instance: &Instance,
    store: &mut Store<RuntimeContext>,
    record: &[u8],
) -> Result<(), RuntimeError> {
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
        .write(&mut *store, abi::RESULT_BASE, record)
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
