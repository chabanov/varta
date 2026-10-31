//! Host event sources and the host task queue of profile v38 (`turn_budgets`,
//! increment P5). See `varta_common::events` for the shared contract.
//!
//! The embedder of a long-lived application instance delivers records to a
//! named source; each delivery is queued as one **host task** and runs as one
//! event turn. A permission prompt's answer is a host task on a one-shot
//! source. Ordering is FIFO by arrival across all sources (host tasks, due
//! timers and network completions); high-frequency records marked
//! coalescible keep only the newest pending record per source, so a burst
//! costs one turn. Every task carries its source id, which is never reused,
//! so a task for a closed source is discarded before it reaches the guest.
//! The runtime keeps ids and serials, never a guest reference.
//!
//! The records of an attached view (v38 `view_protocol`) are host tasks of
//! the same queue: each one runs as one turn through the synthesized
//! `varta.view_abi.event` export with its binary L0 record, coalesces by the
//! same rule (one pending record per listener for coalescible kinds) and is
//! bounded by the view registry's pending-event limit as well.
use crate::scheduler::Scheduler;
use crate::{RuntimeContext, host};
use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;
use varta_common::HostOperation;
use varta_common::events::{
    EventError, INVALID_SOURCE, MAX_EVENT_RECORD_BYTES, MAX_EVENT_SOURCES, MAX_PENDING_HOST_TASKS,
    TakeKind, valid_source_name,
};
use varta_common::permission::{INVALID_DESCRIPTOR, MAX_DESCRIPTOR_BYTES, PermissionState};
use wasmtime::{Caller, Linker};

/// What a source holds for its frame.
enum Phase {
    /// Waiting for the next task.
    Armed,
    /// A delivered record (or the close) not taken yet.
    Ready(Option<String>),
}

struct Source {
    /// `None` for the one-shot source of a permission answer.
    name: Option<String>,
    phase: Phase,
    /// The serial whose one reaction the next delivery wakes.
    watcher: Option<u64>,
}

/// Why a host task exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskKind {
    HostEvent,
    Permission,
    /// A view event record (v38 `view_protocol`).
    View,
}

/// What a host task delivers.
pub(crate) enum Payload {
    /// The UTF-8 record of an event source.
    Text(String),
    /// A binary L0 view event record for `varta.view_abi.event`, delivered
    /// without a text detour.
    View(varta_view::EventRecord),
}

pub(crate) struct Task {
    /// The event source; view records have none ([`TaskKind::View`]).
    pub source: u64,
    pub payload: Payload,
    pub kind: TaskKind,
    pub gesture: bool,
    pub coalesce: bool,
    pub arrived: Instant,
    /// The task's place in the arrival order of the instance's host tasks
    /// and network completions ([`HostEvents::stamp`]).
    pub order: u64,
}
impl Task {
    /// Whether `self` is a pending task the coalescible `next` replaces: the
    /// same source, or for view records the same listener and event kind.
    fn replaced_by(&self, next: &Task) -> bool {
        self.coalesce
            && self.kind == next.kind
            && self.source == next.source
            && match (&self.payload, &next.payload) {
                (Payload::View(old), Payload::View(new)) => {
                    old.kind == new.kind && old.handler == new.handler
                }
                (Payload::Text(_), Payload::Text(_)) => true,
                _ => false,
            }
    }
}

/// What became of a delivered host event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Queued as a new host task.
    Queued,
    /// Merged into a pending coalescible task of the same source.
    Coalesced,
    /// Dropped: the queue was full and the record was coalescible.
    Dropped,
}

/// A host event the embedder delivers to an application instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEvent {
    /// The name of the open source (`varta.events.listen(name, ...)`).
    pub source: String,
    /// The record the guest's listener receives (UTF-8 text).
    pub record: String,
    /// The turn is a user-gesture turn (a click or key press), in which
    /// `varta.permissions.request` may prompt.
    pub gesture: bool,
    /// High-frequency records (`input`, `scroll`, `pointermove`, `resize`):
    /// only the newest pending record of the source is kept.
    pub coalesce: bool,
}

#[derive(Default)]
pub(crate) struct HostEvents {
    sources: BTreeMap<u64, Source>,
    by_name: BTreeMap<String, u64>,
    queue: VecDeque<Task>,
    next_id: u64,
    taken: Option<(u64, String)>,
    /// The next arrival order number (v38 application instances).
    next_order: u64,
}

impl HostEvents {
    /// The next number of the arrival order shared by host tasks and network
    /// completions: deterministic, in the order the runtime receives them.
    pub fn stamp(&mut self) -> u64 {
        self.next_order += 1;
        self.next_order
    }
    fn insert(&mut self, name: Option<String>, phase: Phase) -> Result<u64, EventError> {
        if self.sources.len() >= MAX_EVENT_SOURCES {
            return Err(EventError::QueueFull);
        }
        self.next_id += 1;
        let id = self.next_id;
        if let Some(name) = &name {
            self.by_name.insert(name.clone(), id);
        }
        self.sources.insert(
            id,
            Source {
                name,
                phase,
                watcher: None,
            },
        );
        Ok(id)
    }
    /// Open a named source; an invalid or already open name answers
    /// [`INVALID_SOURCE`].
    fn open(&mut self, name: &str) -> Result<f64, EventError> {
        if !valid_source_name(name) || self.by_name.contains_key(name) {
            return Ok(INVALID_SOURCE);
        }
        Ok(self.insert(Some(name.to_owned()), Phase::Armed)? as f64)
    }
    /// A one-shot source for a permission answer, ready now or later.
    pub fn answer(&mut self, ready: Option<PermissionState>) -> Result<u64, EventError> {
        let phase = match ready {
            Some(state) => Phase::Ready(Some(state.name().to_owned())),
            None => Phase::Armed,
        };
        self.insert(None, phase)
    }
    fn source(&mut self, id: f64) -> Result<(u64, &mut Source), EventError> {
        let id = source_id(id).ok_or(EventError::State)?;
        let source = self.sources.get_mut(&id).ok_or(EventError::State)?;
        Ok((id, source))
    }
    fn watch(
        &mut self,
        scheduler: &mut Scheduler,
        serial: f64,
        id: f64,
    ) -> Result<(), wasmtime::Error> {
        let serial = scheduler.watchable(serial).map_err(wasmtime::Error::new)?;
        let (_, source) = self.source(id).map_err(wasmtime::Error::new)?;
        if source.watcher.is_some() {
            return Err(wasmtime::Error::new(EventError::State));
        }
        match source.phase {
            Phase::Armed => source.watcher = Some(serial),
            Phase::Ready(_) => scheduler.wake(serial).map_err(wasmtime::Error::new)?,
        }
        Ok(())
    }
    fn take(&mut self, id: f64) -> Result<i32, EventError> {
        let (id, source) = self.source(id)?;
        let Phase::Ready(record) = std::mem::replace(&mut source.phase, Phase::Armed) else {
            return Err(EventError::State);
        };
        let one_shot = source.name.is_none();
        let kind = match record {
            Some(record) => {
                self.taken = Some((id, record));
                TakeKind::Event
            }
            None => {
                self.taken = Some((id, String::new()));
                TakeKind::Closed
            }
        };
        if one_shot || kind == TakeKind::Closed {
            self.sources.remove(&id);
        }
        Ok(kind as i32)
    }
    fn text(&mut self, id: f64) -> Result<String, EventError> {
        let id = source_id(id).ok_or(EventError::State)?;
        match self.taken.take() {
            Some((taken, text)) if taken == id => Ok(text),
            _ => Err(EventError::State),
        }
    }
    /// Close a named source: its frame takes `Closed`, and queued tasks for
    /// it are discarded. Unknown ids are a no-op.
    fn close(&mut self, scheduler: &mut Scheduler, id: f64) -> Result<(), wasmtime::Error> {
        let Some(id) = source_id(id) else {
            return Ok(());
        };
        let Some(source) = self.sources.get_mut(&id) else {
            return Ok(());
        };
        let Some(name) = source.name.take() else {
            return Ok(());
        };
        self.by_name.remove(&name);
        source.phase = Phase::Ready(None);
        if let Some(serial) = source.watcher.take() {
            scheduler.wake(serial).map_err(wasmtime::Error::new)?;
        }
        Ok(())
    }
    /// Queue a host event for its named source.
    pub fn push(&mut self, event: HostEvent) -> Result<Delivery, EventError> {
        if event.record.len() > MAX_EVENT_RECORD_BYTES {
            return Err(EventError::RecordTooLarge);
        }
        let id = *self
            .by_name
            .get(&event.source)
            .ok_or_else(|| EventError::NoListener(event.source.clone()))?;
        let task = Task {
            source: id,
            payload: Payload::Text(event.record),
            kind: TaskKind::HostEvent,
            gesture: event.gesture,
            coalesce: event.coalesce,
            arrived: Instant::now(),
            order: self.stamp(),
        };
        let full = self.queue.len() >= MAX_PENDING_HOST_TASKS;
        self.enqueue(task, full).ok_or(EventError::QueueFull)
    }
    /// Queue `task`, first coalescing it into a pending task it replaces;
    /// `full` drops a coalescible task and refuses (`None`) any other.
    fn enqueue(&mut self, task: Task, full: bool) -> Option<Delivery> {
        if task.coalesce
            && let Some(pending) = self
                .queue
                .iter_mut()
                .find(|pending| pending.replaced_by(&task))
        {
            pending.payload = task.payload;
            pending.gesture |= task.gesture;
            return Some(Delivery::Coalesced);
        }
        if full {
            return task.coalesce.then_some(Delivery::Dropped);
        }
        self.queue.push_back(task);
        Some(Delivery::Queued)
    }
    /// Queue a view event record (v38 `view_protocol`) as a host task: a
    /// user-gesture record makes a gesture turn, and coalescible kinds
    /// (`input`, `scroll`) keep one pending record per listener. A full queue
    /// drops a coalescible record and is fatal for any other.
    pub fn push_view(
        &mut self,
        delivery: varta_view::Delivery,
    ) -> Result<Delivery, varta_view::ViewError> {
        let views = self
            .queue
            .iter()
            .filter(|task| task.kind == TaskKind::View)
            .count();
        let full = views >= varta_common::view::DRAFT_MAX_PENDING_EVENTS
            || self.queue.len() >= MAX_PENDING_HOST_TASKS;
        let task = Task {
            source: 0,
            coalesce: delivery.record.kind.coalescible(),
            payload: Payload::View(delivery.record),
            kind: TaskKind::View,
            gesture: delivery.gesture,
            arrived: Instant::now(),
            order: self.stamp(),
        };
        self.enqueue(task, full)
            .ok_or(varta_view::ViewError::QueueOverflow)
    }
    /// Queue the answer of a prompt on its one-shot source.
    pub fn complete(&mut self, source: u64, state: PermissionState) -> Result<(), EventError> {
        if self.queue.len() >= MAX_PENDING_HOST_TASKS {
            return Err(EventError::QueueFull);
        }
        let order = self.stamp();
        self.queue.push_back(Task {
            source,
            payload: Payload::Text(state.name().to_owned()),
            kind: TaskKind::Permission,
            gesture: false,
            coalesce: false,
            arrived: Instant::now(),
            order,
        });
        Ok(())
    }
    /// The arrival time of the next task whose source is still open.
    pub fn next_arrival(&mut self) -> Option<Instant> {
        self.next_task().map(|task| task.arrived)
    }
    /// The arrival order of the next task whose source is still open.
    pub fn next_order(&mut self) -> Option<u64> {
        self.next_task().map(|task| task.order)
    }
    fn next_task(&mut self) -> Option<&Task> {
        while let Some(task) = self.queue.front() {
            if task.kind == TaskKind::View || self.sources.contains_key(&task.source) {
                break;
            }
            self.queue.pop_front();
        }
        self.queue.front()
    }
    pub fn pop(&mut self) -> Option<Task> {
        self.next_arrival()?;
        self.queue.pop_front()
    }
    /// Run a task: its record becomes ready and wakes the source's frame.
    pub fn deliver(
        &mut self,
        scheduler: &mut Scheduler,
        task: Task,
    ) -> Result<(), wasmtime::Error> {
        let Payload::Text(record) = task.payload else {
            return Err(wasmtime::Error::new(EventError::State));
        };
        let Some(source) = self.sources.get_mut(&task.source) else {
            return Ok(());
        };
        if !matches!(source.phase, Phase::Armed) {
            return Err(wasmtime::Error::new(EventError::State));
        }
        source.phase = Phase::Ready(Some(record));
        if let Some(serial) = source.watcher.take() {
            scheduler.wake(serial).map_err(wasmtime::Error::new)?;
        }
        Ok(())
    }
    /// Whether an open named source has a waiting frame.
    pub fn listening(&self) -> bool {
        self.sources
            .values()
            .any(|source| source.name.is_some() && source.watcher.is_some())
    }
    pub fn queued(&self) -> usize {
        self.queue.len()
    }
}

/// A source id the guest passed back: a positive integer.
fn source_id(value: f64) -> Option<u64> {
    (value >= 1.0 && value <= u64::MAX as f64 && value.fract() == 0.0).then_some(value as u64)
}

fn fatal(error: EventError) -> wasmtime::Error {
    wasmtime::Error::new(error)
}

/// Read a bounded UTF-16 name or descriptor exactly (no lone surrogates and
/// no NUL); a malformed one is fatal, like a malformed network descriptor.
fn read_text(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<String, wasmtime::Error> {
    host::input(caller, ptr, len, MAX_DESCRIPTOR_BYTES, true)
        .map_err(|error| error.into_trap(|_| fatal(EventError::State)))
}

pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::EventSourceOpen.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            caller.data().check_deadline()?;
            let name = read_text(&mut caller, ptr, len)?;
            caller.data_mut().events.open(&name).map_err(fatal)
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::EventWatch.name(),
        |mut caller: Caller<'_, RuntimeContext>, serial: f64, id: f64| {
            caller.data().check_deadline()?;
            let context = caller.data_mut();
            context.events.watch(&mut context.scheduler, serial, id)
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::EventTake.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| {
            caller.data().check_deadline()?;
            caller.data_mut().events.take(id).map_err(fatal)
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::EventText.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| -> Result<i32, wasmtime::Error> {
            caller.data().check_deadline()?;
            let text = caller.data_mut().events.text(id).map_err(fatal)?;
            host::result16(&mut caller, text.as_bytes())
                .map_err(|error| error.into_trap(wasmtime::Error::msg))
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::EventSourceClose.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| {
            caller.data().check_deadline()?;
            let context = caller.data_mut();
            context.events.close(&mut context.scheduler, id)
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::PermissionQuery.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            caller.data().check_deadline()?;
            let text = read_text(&mut caller, ptr, len)?;
            let descriptor = crate::permission::descriptor(&text).map_err(fatal)?;
            let context = caller.data();
            Ok(context
                .broker
                .state(&context.policy, &descriptor)
                .map_or(INVALID_DESCRIPTOR, PermissionState::code))
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::PermissionRequest.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            caller.data().check_deadline()?;
            let text = read_text(&mut caller, ptr, len)?;
            let descriptor = crate::permission::descriptor(&text).map_err(fatal)?;
            let context = caller.data_mut();
            let Some(state) = context.broker.state(&context.policy, &descriptor) else {
                return Ok(f64::from(INVALID_DESCRIPTOR));
            };
            // Only a user-gesture turn of an instance with a prompt seam asks;
            // every other request answers the current state at once.
            if state == PermissionState::Prompt && context.broker.may_prompt(&descriptor.name) {
                let source = context.events.answer(None).map_err(fatal)?;
                context.broker.open_prompt(source, &descriptor.name);
                return Ok(source as f64);
            }
            Ok(context.events.answer(Some(state)).map_err(fatal)? as f64)
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(source: &str, record: &str, coalesce: bool) -> HostEvent {
        HostEvent {
            source: source.into(),
            record: record.into(),
            gesture: false,
            coalesce,
        }
    }

    #[test]
    fn deliveries_are_fifo_coalesced_bounded_and_need_a_listener() {
        let mut events = HostEvents::default();
        assert_eq!(events.open("9bad").unwrap(), INVALID_SOURCE);
        let ui = events.open("ui").unwrap();
        assert_eq!(events.open("ui").unwrap(), INVALID_SOURCE);
        let input = events.open("input").unwrap();
        assert!(ui >= 1.0 && input > ui);
        assert_eq!(
            events.push(event("missing", "x", false)),
            Err(EventError::NoListener("missing".into()))
        );
        assert_eq!(events.push(event("ui", "a", false)), Ok(Delivery::Queued));
        assert_eq!(events.push(event("input", "1", true)), Ok(Delivery::Queued));
        assert_eq!(
            events.push(event("input", "2", true)),
            Ok(Delivery::Coalesced)
        );
        assert_eq!(events.push(event("ui", "b", false)), Ok(Delivery::Queued));
        let records: Vec<String> = std::iter::from_fn(|| {
            events.pop().map(|t| match t.payload {
                Payload::Text(record) => record,
                Payload::View(_) => unreachable!("no view task queued"),
            })
        })
        .collect();
        assert_eq!(records, ["a", "2", "b"]);
        assert_eq!(
            events.push(event("ui", &"x".repeat(MAX_EVENT_RECORD_BYTES + 1), false)),
            Err(EventError::RecordTooLarge)
        );
        for _ in 0..MAX_PENDING_HOST_TASKS {
            events.push(event("ui", "r", false)).unwrap();
        }
        assert_eq!(
            events.push(event("ui", "r", false)),
            Err(EventError::QueueFull)
        );
        assert_eq!(
            events.push(event("input", "r", true)),
            Ok(Delivery::Dropped)
        );
    }

    #[test]
    fn closed_sources_discard_queued_tasks_and_ids_are_never_reused() {
        let mut scheduler = Scheduler::default();
        let mut events = HostEvents::default();
        let ui = events.open("ui").unwrap();
        events.push(event("ui", "late", false)).unwrap();
        events.close(&mut scheduler, ui).unwrap();
        // The frame takes the close; the queued task is discarded.
        assert_eq!(events.take(ui), Ok(TakeKind::Closed as i32));
        assert!(events.pop().is_none());
        let again = events.open("ui").unwrap();
        assert!(again > ui);
        // A one-shot answer is ready at once and released when taken.
        let answer = events.answer(Some(PermissionState::Granted)).unwrap();
        assert_eq!(events.take(answer as f64), Ok(TakeKind::Event as i32));
        assert_eq!(events.text(answer as f64).as_deref(), Ok("granted"));
        assert_eq!(events.take(answer as f64), Err(EventError::State));
    }
}
