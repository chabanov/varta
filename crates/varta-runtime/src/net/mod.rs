//! Asynchronous network requests of profile v37 (`net_request`), behind the
//! `fetch` prelude. See `varta_common::net` for the shared contract.
//!
//! A request is authorized against the exact `net.request` grant (and the
//! host ceiling) inside `net_request_start`, before anything is resolved or
//! connected; then its transfer becomes a task on a current-thread Tokio
//! runtime that only the event loop drives. The runtime keeps request ids,
//! completions and the promise serial each armed operation wakes, never a
//! guest reference: the guest's waiting frame roots everything it needs.
//!
//! An application instance may record its transfers or replay a recorded
//! log instead of a transport ([`crate::replay`]); requests are planned and
//! authorized the same way in every mode.
mod client;
mod dns;
pub(crate) use client::fetch_cancellable;

use crate::replay::{NetworkMode, Recorder, Replayer, recorded_step};
use crate::scheduler::{self, Scheduler};
use crate::{RuntimeContext, host};
use client::ReadMode;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use varta_common::HostOperation;
use varta_common::net::replay::{LoggedEvent, LoggedRead, NetworkLog, ReplayError};
use varta_common::net::{
    BodyBound, CompletionKind, DescriptorVersion, HeadForm, MAX_PENDING_REQUESTS,
    MAX_REQUEST_BYTES, MAX_REQUEST_HEADER_BYTES, NATIVE_ACCEPT_ENCODING, NetError, RUNTIME_HEADERS,
    RequestDescriptor, RequestFailure, host_request_header,
};
use wasmtime::{Caller, Extern, Linker};

/// One completed operation.
pub(crate) struct Completion {
    kind: CompletionKind,
    text: String,
}
/// A completion reported by a transfer task.
pub(crate) struct Event {
    id: u64,
    completion: Completion,
    /// When the transfer task reported it (v38 application instances order
    /// it by this arrival among host tasks and due timers).
    arrived: Instant,
}
impl Event {
    pub(crate) fn arrived(&self) -> Instant {
        self.arrived
    }
}

enum Phase {
    /// An operation is armed and has not completed.
    Armed,
    /// The armed operation completed and was not taken yet.
    Ready(Completion),
    /// No operation is armed (a head or a chunk was taken).
    Idle,
}

struct Request {
    phase: Phase,
    /// The serial whose one reaction the armed operation wakes.
    watcher: Option<u64>,
    transfer: Option<Transfer>,
    /// The request's place among the logged requests (recording or replay).
    logged: Option<u32>,
    /// The mode of the most recently armed body read.
    read: Option<ReadMode>,
    /// The open transfer a replay serves in place of `transfer`.
    replay: Option<Replayed>,
}

/// A replayed transfer: what its head needs and whether a logged event was
/// released to its armed operation and not delivered yet.
struct Replayed {
    url: String,
    visible: BTreeSet<String>,
    released: bool,
}

/// Where completions come from.
#[derive(Default)]
enum Session {
    #[default]
    Live,
    Record(Recorder),
    Replay(Replayer),
}

impl ReadMode {
    fn logged(self) -> LoggedRead {
        match self {
            Self::Text => LoggedRead::Text,
            Self::Bytes => LoggedRead::Bytes,
        }
    }
}

fn replay_fatal(error: ReplayError) -> wasmtime::Error {
    wasmtime::Error::new(error)
}

/// A running transfer: its task, the channel that asks for the next chunk
/// (as text or as bytes), and its connection task once there is one.
struct Transfer {
    task: tokio::task::JoinHandle<()>,
    pull: mpsc::Sender<ReadMode>,
    connection: client::ConnectionSlot,
}
impl Transfer {
    /// Cancel the transfer and run `runtime` until its request and connection
    /// tasks are dropped, so its socket is closed when this returns instead
    /// of when the event loop next drives the runtime (which a guest waiting
    /// only for timers never does). Cancelling a task needs no network
    /// progress: an aborted task is dropped at its next poll, which this
    /// `block_on` performs. Other transfers may make progress meanwhile; their
    /// completions wait in the event queue like any that arrive during a turn.
    ///
    /// A transfer that ended on its own (its end of body or a failure) is
    /// closed the same way before its terminal completion is delivered: its
    /// request task has finished, and its cancelled connection task would
    /// otherwise wait for the next drive as well. Without a runtime there is
    /// no task to wait for: a transfer exists only on the runtime that
    /// spawned it.
    fn close(mut self, runtime: Option<&tokio::runtime::Runtime>) {
        let Some(runtime) = runtime else {
            return;
        };
        self.task.abort();
        let connection = self.connection.take();
        if let Some(connection) = &connection {
            connection.abort();
        }
        runtime.block_on(async {
            // Both end as cancelled (or already finished): the outcome is
            // irrelevant, only that each future has been dropped.
            let _ = (&mut self.task).await;
            if let Some(connection) = connection {
                let _ = connection.await;
            }
        });
    }
}
impl Drop for Transfer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The network state of one execution. Its Tokio runtime is created by the
/// first request, so programs without requests start no runtime.
#[derive(Default)]
pub(crate) struct Net {
    runtime: Option<tokio::runtime::Runtime>,
    events: Option<(mpsc::UnboundedSender<Event>, mpsc::UnboundedReceiver<Event>)>,
    tls: Option<Arc<rustls::ClientConfig>>,
    requests: BTreeMap<u64, Request>,
    next_id: u64,
    /// The text of the most recently taken completion and its request id.
    taken: Option<(u64, String)>,
    session: Session,
    /// The scripted-session step being performed (`None` outside a
    /// scripted session): recorded with each event, and the bound up to
    /// which a replay releases logged events.
    step: Option<u32>,
}

#[cfg(test)]
thread_local! {
    /// Test-only trust anchors for the loopback HTTPS server of the runtime
    /// tests. Production builds trust the compiled-in roots alone.
    static TEST_ROOTS: std::cell::RefCell<Vec<rustls::pki_types::CertificateDer<'static>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}
/// Test-only: run `body` trusting `root` in addition to the compiled-in roots.
#[cfg(test)]
pub(crate) fn with_test_root<T>(
    root: rustls::pki_types::CertificateDer<'static>,
    body: impl FnOnce() -> T,
) -> T {
    TEST_ROOTS.with(|roots| roots.borrow_mut().push(root));
    let result = body();
    TEST_ROOTS.with(|roots| roots.borrow_mut().clear());
    result
}
#[cfg(test)]
fn test_roots() -> Vec<rustls::pki_types::CertificateDer<'static>> {
    TEST_ROOTS.with(|roots| roots.borrow().clone())
}

impl Net {
    fn request(&mut self, id: f64) -> Result<(u64, &mut Request), NetError> {
        let id = request_id(id).ok_or(NetError::State)?;
        let request = self.requests.get_mut(&id).ok_or(NetError::State)?;
        Ok((id, request))
    }
    /// Whether a waiting frame keeps the event loop alive.
    pub(crate) fn alive(&self) -> bool {
        self.requests
            .values()
            .any(|request| request.watcher.is_some() && matches!(request.phase, Phase::Armed))
    }
    /// The pending-request limit: one more request is fatal (`Budget`).
    fn full(&self) -> Result<(), wasmtime::Error> {
        if self.requests.len() >= MAX_PENDING_REQUESTS {
            return Err(wasmtime::Error::new(NetError::Budget));
        }
        Ok(())
    }
    /// A new request: a slot, then either an immediate completion (a denial,
    /// an invalid URL or request) or a transfer task.
    ///
    /// A recording logs a planned request before its transfer starts; a
    /// replay serves it from the log instead, once it claimed the next
    /// logged request. Both serve version 2 descriptors only.
    fn start(&mut self, outcome: Result<client::Plan, Completion>) -> Result<f64, wasmtime::Error> {
        self.full()?;
        self.next_id += 1;
        let id = self.next_id;
        let mut request = Request {
            phase: Phase::Armed,
            watcher: None,
            transfer: None,
            logged: None,
            read: None,
            replay: None,
        };
        match outcome {
            Err(completion) => request.phase = Phase::Ready(completion),
            Ok(plan) => {
                let visible = match (&self.session, &plan.head) {
                    (Session::Live, _) => None,
                    (_, HeadForm::Canonical(visible)) => Some(visible.clone()),
                    (_, HeadForm::Arrival) => return Err(replay_fatal(ReplayError::Descriptor)),
                };
                let (method, url) = (plan.method.as_str(), plan.url.as_str());
                match &mut self.session {
                    Session::Live => request.transfer = Some(self.spawn(id, plan)?),
                    Session::Record(recorder) => {
                        request.logged = Some(recorder.request(method, url).map_err(replay_fatal)?);
                        request.transfer = Some(self.spawn(id, plan)?);
                    }
                    Session::Replay(replayer) => {
                        request.logged = Some(replayer.claim(method, url).map_err(replay_fatal)?);
                        request.replay = Some(Replayed {
                            url: url.to_owned(),
                            visible: visible.unwrap_or_default(),
                            released: false,
                        });
                    }
                }
            }
        }
        self.requests.insert(id, request);
        self.release().map_err(replay_fatal)?;
        Ok(id as f64)
    }
    /// Set how completions reach this execution's requests (before any).
    pub(crate) fn configure(&mut self, mode: NetworkMode) {
        self.session = match mode {
            NetworkMode::Live => Session::Live,
            NetworkMode::Record => Session::Record(Recorder::new()),
            NetworkMode::Replay(log) => Session::Replay(Replayer::new(log)),
        };
    }
    /// A scripted session performs `step` (1-based): later events are
    /// recorded with it, and a replay releases the events logged up to it.
    pub(crate) fn set_step(&mut self, step: u32) -> Result<(), ReplayError> {
        self.step = Some(step);
        self.release()
    }
    /// Whether completions come from a replayed log.
    pub(crate) fn replaying(&self) -> bool {
        matches!(self.session, Session::Replay(_))
    }
    /// Whether waiting can deliver a completion: a transfer is armed and
    /// watched, or a replay released a completion that has not arrived.
    pub(crate) fn busy(&self) -> bool {
        match &self.session {
            Session::Replay(replayer) => replayer.waiting(),
            _ => self.alive(),
        }
    }
    /// A replay's next logged event and the events not released yet.
    pub(crate) fn replay_position(&self) -> Option<(usize, usize)> {
        match &self.session {
            Session::Replay(replayer) => Some((replayer.cursor(), replayer.remaining())),
            _ => None,
        }
    }
    /// The log a recording produced; the execution keeps no recording after.
    pub(crate) fn take_log(&mut self) -> Option<NetworkLog> {
        match std::mem::take(&mut self.session) {
            Session::Record(recorder) => Some(recorder.finish()),
            other => {
                self.session = other;
                None
            }
        }
    }
    /// Release logged events in log order: each once its request has the
    /// matching operation armed (and nothing released to it is pending),
    /// up to the first event that cannot be released yet. An abort waits
    /// for the guest's own abort ([`Self::abort`]).
    fn release(&mut self) -> Result<(), ReplayError> {
        let Self {
            session: Session::Replay(replayer),
            requests,
            step,
            ..
        } = self
        else {
            return Ok(());
        };
        while let Some(event) = replayer.next(*step).cloned() {
            let logged = event.request();
            let Some((&id, request)) = requests
                .iter_mut()
                .find(|(_, request)| request.logged == Some(logged))
            else {
                break;
            };
            let Some(replayed) = request.replay.as_mut() else {
                break;
            };
            if replayed.released || !matches!(request.phase, Phase::Armed) {
                break;
            }
            let completion = match &event {
                LoggedEvent::Head { .. } => Completion {
                    kind: CompletionKind::Head,
                    text: event
                        .head_text(&replayed.url, &replayed.visible)
                        .unwrap_or_default(),
                },
                LoggedEvent::Chunk { read, text, .. } => {
                    let Some(actual) = request.read.map(ReadMode::logged) else {
                        break;
                    };
                    if actual != *read {
                        return Err(ReplayError::Read {
                            request: logged,
                            actual,
                            logged: *read,
                        });
                    }
                    Completion {
                        kind: CompletionKind::Chunk,
                        text: text.clone(),
                    }
                }
                LoggedEvent::End { .. } => Completion {
                    kind: CompletionKind::End,
                    text: String::new(),
                },
                LoggedEvent::Failed { text, .. } => Completion {
                    kind: CompletionKind::Failed,
                    text: text.clone(),
                },
                LoggedEvent::Abort { .. } => break,
            };
            if event.terminal() {
                request.replay = None;
            } else {
                replayed.released = true;
            }
            replayer.release(Event {
                id,
                completion,
                arrived: Instant::now(),
            });
        }
        Ok(())
    }
    fn spawn(&mut self, id: u64, plan: client::Plan) -> Result<Transfer, wasmtime::Error> {
        if self.runtime.is_none() {
            self.runtime = Some(
                tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .enable_time()
                    .build()
                    .map_err(wasmtime::Error::new)?,
            );
        }
        let (events, _) = self.events.get_or_insert_with(mpsc::unbounded_channel);
        let events = events.clone();
        // One pull may be outstanding: the guest arms one chunk at a time.
        let (pull, pulls) = mpsc::channel(1);
        let Some(runtime) = &self.runtime else {
            return Err(wasmtime::Error::new(NetError::State));
        };
        let connection = client::ConnectionSlot::default();
        let task = runtime.spawn(client::run(id, plan, events, pulls, connection.clone()));
        Ok(Transfer {
            task,
            pull,
            connection,
        })
    }
    fn tls(&mut self) -> Result<Arc<rustls::ClientConfig>, String> {
        if let Some(config) = &self.tls {
            return Ok(config.clone());
        }
        let config = client::tls_config()?;
        self.tls = Some(config.clone());
        Ok(config)
    }
    /// Register `serial` (one pending reaction) as the waiter of `id`'s armed
    /// operation; a completion that is already there wakes it at once.
    fn watch(
        &mut self,
        scheduler: &mut Scheduler,
        serial: f64,
        id: f64,
    ) -> Result<(), wasmtime::Error> {
        let serial = scheduler.watchable(serial).map_err(wasmtime::Error::new)?;
        let (_, request) = self.request(id).map_err(wasmtime::Error::new)?;
        if request.watcher.is_some() {
            return Err(wasmtime::Error::new(NetError::State));
        }
        match request.phase {
            Phase::Armed => request.watcher = Some(serial),
            Phase::Ready(_) => scheduler.wake(serial).map_err(wasmtime::Error::new)?,
            Phase::Idle => return Err(wasmtime::Error::new(NetError::State)),
        }
        Ok(())
    }
    /// A transfer completed its armed operation. Completions of requests that
    /// were aborted or released are discarded. A terminal completion first
    /// closes the transfer's connection, so its socket is gone before the
    /// guest can observe the end, as after an abort. A recording logs each
    /// completion the guest receives here, and only those.
    pub(crate) fn deliver(
        &mut self,
        scheduler: &mut Scheduler,
        event: Event,
    ) -> Result<(), wasmtime::Error> {
        let Some(request) = self.requests.get_mut(&event.id) else {
            return Ok(());
        };
        if event.completion.kind.terminal()
            && let Some(transfer) = request.transfer.take()
        {
            transfer.close(self.runtime.as_ref());
        }
        if !matches!(request.phase, Phase::Armed) {
            return Ok(());
        }
        if let (Session::Record(recorder), Some(logged)) = (&mut self.session, request.logged) {
            let step = recorded_step(self.step);
            logged_event(step, logged, request.read, &event.completion)
                .and_then(|logged| recorder.event(logged))
                .map_err(replay_fatal)?;
        }
        if let Some(replayed) = request.replay.as_mut() {
            replayed.released = false;
        }
        request.phase = Phase::Ready(event.completion);
        if let Some(serial) = request.watcher.take() {
            scheduler.wake(serial).map_err(wasmtime::Error::new)?;
        }
        Ok(())
    }
    fn take(&mut self, id: f64) -> Result<i32, NetError> {
        let (id, request) = self.request(id)?;
        let Phase::Ready(_) = request.phase else {
            return Err(NetError::State);
        };
        let Phase::Ready(completion) = std::mem::replace(&mut request.phase, Phase::Idle) else {
            return Err(NetError::State);
        };
        if completion.kind.terminal() {
            self.requests.remove(&id);
        }
        self.taken = Some((id, completion.text));
        Ok(completion.kind as i32)
    }
    fn text(&mut self, id: f64) -> Result<String, NetError> {
        let id = request_id(id).ok_or(NetError::State)?;
        match self.taken.take() {
            Some((taken, text)) if taken == id => Ok(text),
            _ => Err(NetError::State),
        }
    }
    fn read(&mut self, id: f64, mode: ReadMode) -> Result<(), wasmtime::Error> {
        let (_, request) = self.request(id).map_err(fatal)?;
        if !matches!(request.phase, Phase::Idle) || request.watcher.is_some() {
            return Err(fatal(NetError::State));
        }
        if let Some(replayed) = request.replay.as_mut() {
            replayed.released = false;
        } else {
            let Some(transfer) = &request.transfer else {
                return Err(fatal(NetError::State));
            };
            // The capacity-1 channel is empty: every earlier pull was answered.
            transfer
                .pull
                .try_send(mode)
                .map_err(|_| fatal(NetError::State))?;
        }
        request.read = Some(mode);
        request.phase = Phase::Armed;
        self.release().map_err(replay_fatal)
    }
    /// Cancel the transfer, closing its connection before this returns. An
    /// armed (or completed but untaken) operation completes as aborted and
    /// wakes its waiter; an idle request is released. A recording logs the
    /// abort of an open transfer; a replay requires the log's next event to
    /// be that abort.
    fn abort(&mut self, scheduler: &mut Scheduler, id: f64) -> Result<(), wasmtime::Error> {
        let Some(id) = request_id(id) else {
            return Ok(());
        };
        let Some(request) = self.requests.get_mut(&id) else {
            return Ok(());
        };
        let step = self.step;
        if request.replay.take().is_some()
            && let (Session::Replay(replayer), Some(logged)) = (&mut self.session, request.logged)
        {
            replayer.abort(logged, step).map_err(replay_fatal)?;
        }
        if let Some(transfer) = request.transfer.take() {
            if let (Session::Record(recorder), Some(logged)) = (&mut self.session, request.logged) {
                recorder
                    .event(LoggedEvent::Abort {
                        step: recorded_step(step),
                        request: logged,
                    })
                    .map_err(replay_fatal)?;
            }
            transfer.close(self.runtime.as_ref());
        }
        if matches!(request.phase, Phase::Idle) {
            self.requests.remove(&id);
        } else {
            request.phase = Phase::Ready(Completion {
                kind: CompletionKind::Aborted,
                text: String::new(),
            });
            if let Some(serial) = request.watcher.take() {
                scheduler.wake(serial).map_err(wasmtime::Error::new)?;
            }
        }
        self.release().map_err(replay_fatal)
    }
    /// Let the transfers make the progress they can without waiting, so
    /// completions arrive while a long-lived instance stays busy with other
    /// tasks (v38).
    pub(crate) fn drive(&mut self) {
        if let Some(runtime) = &self.runtime {
            runtime.block_on(tokio::task::yield_now());
        }
    }
    /// A completion that already arrived, without driving the transfers.
    pub(crate) fn poll(&mut self) -> Option<Event> {
        if let Session::Replay(replayer) = &mut self.session {
            return replayer.poll();
        }
        self.events.as_mut()?.1.try_recv().ok()
    }
    /// Drive the transfers until one reports a completion or `until` passes.
    /// A replay never waits for the wall clock: what it released arrives
    /// now, and nothing else can.
    pub(crate) fn wait(&mut self, until: Instant) -> Option<Event> {
        if let Session::Replay(replayer) = &mut self.session {
            return replayer.wait();
        }
        let (Some(runtime), Some((_, events))) = (&self.runtime, &mut self.events) else {
            std::thread::sleep(until.saturating_duration_since(Instant::now()));
            return None;
        };
        if let Ok(event) = events.try_recv() {
            return Some(event);
        }
        let until = tokio::time::Instant::from_std(until);
        // The timer must be created inside the runtime's context.
        runtime
            .block_on(async { tokio::time::timeout_at(until, events.recv()).await })
            .ok()
            .flatten()
    }
}

/// A request id the guest passed back: a positive integer.
fn request_id(value: f64) -> Option<u64> {
    (value >= 1.0 && value <= u64::MAX as f64 && value.fract() == 0.0).then_some(value as u64)
}

/// The logged form of a completion a transfer delivered to logged request
/// `request`: a head keeps the fields of its record except the URL (a
/// replaying host supplies its own), a chunk the mode of the read that
/// armed it.
fn logged_event(
    step: u32,
    request: u32,
    read: Option<ReadMode>,
    completion: &Completion,
) -> Result<LoggedEvent, ReplayError> {
    let unrecordable = || ReplayError::Unrecordable { request };
    Ok(match completion.kind {
        CompletionKind::Head => {
            let [status, status_text, _, location, headers] = varta_common::net::HEAD_FIELDS;
            let record: serde_json::Value =
                serde_json::from_str(&completion.text).map_err(|_| unrecordable())?;
            let text = |name: &str| {
                record
                    .get(name)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(unrecordable)
            };
            LoggedEvent::Head {
                step,
                request,
                status: record
                    .get(status)
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|status| u16::try_from(status).ok())
                    .ok_or_else(unrecordable)?,
                status_text: text(status_text)?,
                location: text(location)?,
                headers: record
                    .get(headers)
                    .and_then(serde_json::Value::as_array)
                    .and_then(|values| {
                        values
                            .iter()
                            .map(|value| value.as_str().map(str::to_owned))
                            .collect::<Option<Vec<_>>>()
                    })
                    .ok_or_else(unrecordable)?,
            }
        }
        CompletionKind::Chunk => LoggedEvent::Chunk {
            step,
            request,
            read: read.ok_or_else(unrecordable)?.logged(),
            text: completion.text.clone(),
        },
        CompletionKind::End => LoggedEvent::End { step, request },
        CompletionKind::Failed => LoggedEvent::Failed {
            step,
            request,
            text: completion.text.clone(),
        },
        CompletionKind::Aborted | CompletionKind::Denied => return Err(unrecordable()),
    })
}

fn failed(failure: RequestFailure) -> Completion {
    Completion {
        kind: CompletionKind::Failed,
        text: failure.to_string(),
    }
}

/// Check a request against the grants and build its transfer plan; every
/// refusal is an immediate completion, so nothing is resolved or connected.
/// A version 2 descriptor (v38) gets the canonical head with the response
/// headers the grants show; version 1 (v37) keeps every field as received.
fn plan(
    context: &mut RuntimeContext,
    request: &RequestDescriptor<'_>,
    version: DescriptorVersion,
) -> Result<client::Plan, Completion> {
    let method = hyper::Method::from_bytes(request.method.as_bytes())
        .ok()
        .filter(|_| varta_common::net::valid_method(request.method))
        .ok_or_else(|| failed(RequestFailure::Method(request.method.to_owned())))?;
    if url::Url::parse(request.url).is_err() {
        return Err(failed(RequestFailure::Url(request.url.to_owned())));
    }
    let url = match context
        .broker
        .request(&context.policy, request.url, request.method)
    {
        crate::permission::Decision::Granted(url) => url,
        crate::permission::Decision::Denied => {
            let text = varta_common::net::denial_text(request.method, request.url);
            // The denial stays observable in the host diagnostics, like every
            // other capability denial, while the guest gets a catchable
            // `NotAllowedError`.
            if context.emit(format!("varta log: {text}")).is_err() {
                return Err(failed(RequestFailure::OutputBudget));
            }
            return Err(Completion {
                kind: CompletionKind::Denied,
                text,
            });
        }
    };
    if request.body.is_some() && matches!(method, hyper::Method::GET | hyper::Method::HEAD) {
        return Err(failed(RequestFailure::BodyNotAllowed));
    }
    let mut bytes = 0usize;
    let mut headers = Vec::with_capacity(request.headers.len() + 1);
    for (name, value) in &request.headers {
        bytes = bytes.saturating_add(name.len() + value.len());
        if bytes > MAX_REQUEST_HEADER_BYTES {
            return Err(failed(RequestFailure::HeaderBytes));
        }
        if RUNTIME_HEADERS
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(name))
        {
            return Err(failed(RequestFailure::RuntimeHeader((*name).to_owned())));
        }
        let name = hyper::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| failed(RequestFailure::HeaderName((*name).to_owned())))?;
        // Header values are ByteStrings: one byte per code unit.
        let value = value
            .chars()
            .map(|unit| u8::try_from(u32::from(unit)).ok())
            .collect::<Option<Vec<u8>>>()
            .and_then(|bytes| hyper::header::HeaderValue::from_bytes(&bytes).ok())
            .ok_or_else(|| failed(RequestFailure::HeaderValue(name.to_string())))?;
        // V38: a host-owned header is checked, then ignored on every host.
        if version == DescriptorVersion::V2 && host_request_header(name.as_str()) {
            continue;
        }
        headers.push((name, value));
    }
    if version == DescriptorVersion::V2 {
        headers.push((
            hyper::header::ACCEPT_ENCODING,
            hyper::header::HeaderValue::from_static(NATIVE_ACCEPT_ENCODING),
        ));
    }
    // Plain HTTP requests never build the TLS configuration.
    let tls = match url.scheme() {
        client::HTTPS => Some(
            context
                .net
                .tls()
                .map_err(|error| failed(RequestFailure::TlsConfig(error)))?,
        ),
        _ => None,
    };
    // V37 keeps one end-to-end deadline; a v38 transfer is bounded by
    // inactivity, so a long-lived stream stays open while it is active.
    let (head, timing) = match version {
        DescriptorVersion::V1 => (
            HeadForm::Arrival,
            client::Timing::Transfer(crate::turns::operation_deadline(context)),
        ),
        DescriptorVersion::V2 => (
            HeadForm::Canonical(context.policy.response_headers()),
            client::Timing::Inactivity(crate::turns::inactivity(context)),
        ),
    };
    Ok(client::Plan {
        url,
        method,
        headers,
        head,
        body: request.body.clone().map(hyper::body::Bytes::from),
        deadline: timing.until(),
        timing,
        bound: BodyBound::of(version),
        identity: version == DescriptorVersion::V2,
        tls,
    })
}

/// Read the descriptor as UTF-16 from guest memory. Lone surrogates (only a
/// body can carry them) become U+FFFD, as for a USVString.
fn read_descriptor(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<String, NetError> {
    let (Ok(start), Ok(length)) = (usize::try_from(ptr), usize::try_from(len)) else {
        return Err(NetError::Descriptor);
    };
    if length > MAX_REQUEST_BYTES || length % 2 != 0 {
        return Err(NetError::Descriptor);
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(NetError::Descriptor);
    };
    let bytes = memory
        .data(&*caller)
        .get(start..start.saturating_add(length))
        .ok_or(NetError::Descriptor)?;
    let units = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair));
    Ok(char::decode_utf16(units)
        .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect())
}

fn fatal(error: NetError) -> wasmtime::Error {
    wasmtime::Error::new(error)
}

/// Request bookkeeping is no host effect: it observes the execution deadline
/// but is not charged to the host-call budget, like the timer bookkeeping.
fn deadline(context: &RuntimeContext) -> Result<(), wasmtime::Error> {
    context.check_deadline()
}

/// `net_request_start` and `net_request_start_v2`: one host call, the
/// descriptor read and split, then the grant check before any effect.
fn start(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
    version: DescriptorVersion,
) -> Result<f64, wasmtime::Error> {
    caller.data_mut().tick()?;
    let text = read_descriptor(caller, ptr, len).map_err(fatal)?;
    let request = RequestDescriptor::parse(&text, version).map_err(fatal)?;
    let context = caller.data_mut();
    // V38: the pending-request limit fails before the permission broker
    // decides (no prompt, decision or audit entry for a request that cannot
    // start); v37 keeps its order.
    if context.turns.renew {
        context.net.full()?;
    }
    let outcome = plan(context, &request, version);
    context.net.start(outcome)
}

pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        HostOperation::NetRequestStart.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            start(&mut caller, ptr, len, DescriptorVersion::V1)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::NetRequestStartV2.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            start(&mut caller, ptr, len, DescriptorVersion::V2)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::NetRequestWatch.name(),
        |mut caller: Caller<'_, RuntimeContext>, serial: f64, id: f64| {
            deadline(caller.data())?;
            let context = caller.data_mut();
            context.net.watch(&mut context.scheduler, serial, id)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::NetRequestTake.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| {
            deadline(caller.data())?;
            caller.data_mut().net.take(id).map_err(fatal)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::NetRequestText.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| -> Result<i32, wasmtime::Error> {
            deadline(caller.data())?;
            let text = caller.data_mut().net.text(id).map_err(fatal)?;
            host::result16(&mut caller, text.as_bytes())
                .map_err(|error| error.into_trap(wasmtime::Error::msg))
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::NetBodyRead.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| {
            deadline(caller.data())?;
            caller.data_mut().net.read(id, ReadMode::Text)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::NetBodyReadBytes.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| {
            deadline(caller.data())?;
            caller.data_mut().net.read(id, ReadMode::Bytes)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::NetRequestAbort.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| {
            deadline(caller.data())?;
            let context = caller.data_mut();
            context.net.abort(&mut context.scheduler, id)
        },
    )?;
    Ok(())
}

/// The event loop's wait while requests are in flight: deliver the next
/// completion, or report that `until` passed without one.
pub(crate) fn wait(context: &mut RuntimeContext, until: Instant) -> Result<bool, wasmtime::Error> {
    if context.net.replaying() {
        // A released completion arrives at once; nothing else can.
        let Some(event) = context.net.wait(until) else {
            return Ok(false);
        };
        context.net.deliver(&mut context.scheduler, event)?;
        return Ok(true);
    }
    let event = loop {
        context.check_cancelled()?;
        let slice = context
            .cancellation
            .as_ref()
            .map_or(until, |c| c.wait_until(until));
        let event = context.net.wait(slice);
        context.check_cancelled()?;
        if let Some(event) = event {
            break event;
        }
        if slice == until || Instant::now() >= until {
            return Ok(false);
        }
    };
    context.net.deliver(&mut context.scheduler, event)?;
    scheduler::observed_wall_clock(context);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The head record of one GET of `path` on a loopback server, as the
    /// transfer task reports it.
    fn head_record(server: &crate::net_loopback::Loopback, path: &str, head: HeadForm) -> String {
        let completion = first_completion(server, path, head, false);
        assert_eq!(completion.kind, CompletionKind::Head);
        completion.text
    }
    /// The first completion of one GET of `path` (`identity`: a v38 request
    /// that asked for the identity coding only).
    fn first_completion(
        server: &crate::net_loopback::Loopback,
        path: &str,
        head: HeadForm,
        identity: bool,
    ) -> Completion {
        const DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap();
        let (events, mut received) = mpsc::unbounded_channel();
        // No body read follows: the closed pull channel ends the task.
        let (_, pulls) = mpsc::channel(1);
        let deadline = Instant::now() + DEADLINE;
        let plan = client::Plan {
            url: url::Url::parse(&format!("{}{path}", server.base)).unwrap(),
            method: hyper::Method::GET,
            headers: Vec::new(),
            head,
            body: None,
            deadline,
            timing: client::Timing::Transfer(deadline),
            bound: BodyBound::Response,
            identity,
            tls: None,
        };
        runtime.block_on(client::run(
            1,
            plan,
            events,
            pulls,
            client::ConnectionSlot::default(),
        ));
        received.try_recv().unwrap().completion
    }

    /// A v38 request asks for the identity coding only, so a response coded
    /// otherwise fails before its head reaches the guest; v37 shows it.
    #[test]
    fn identity_requests_refuse_coded_responses() {
        let server = crate::net_loopback::Loopback::start();
        let visible = varta_common::net::visible_response_headers(None);
        let canonical = HeadForm::Canonical(visible);
        let refused = first_completion(&server, "/coded", canonical.clone(), true);
        assert_eq!(refused.kind, CompletionKind::Failed);
        assert_eq!(
            refused.text,
            RequestFailure::Encoding("gzip".into()).to_string()
        );
        assert_eq!(
            refused.text,
            "response content-encoding gzip was not requested"
        );
        let shown = first_completion(&server, "/coded", HeadForm::Arrival, false);
        assert_eq!(shown.kind, CompletionKind::Head);
        assert!(shown.text.contains(r#""content-encoding","gzip""#));
        let plain = first_completion(&server, "/hello", canonical, true);
        assert_eq!(plain.kind, CompletionKind::Head);
    }

    /// V38 heads are canonical whatever order, case and repetition the raw
    /// response has; set-cookie and undeclared names never appear. V37 keeps
    /// its frozen record.
    #[test]
    fn raw_out_of_order_heads_become_canonical_records() {
        let server = crate::net_loopback::Loopback::start();
        let url = format!("{}/headers", server.base);
        let tail = format!(r#""location":"","status":200,"statusText":"OK","url":"{url}"}}"#);
        assert_eq!(
            head_record(&server, "/headers", HeadForm::Arrival),
            format!(
                "{}{tail}",
                concat!(
                    r#"{"headers":["x-b","1","x-b","2","content-type","text/plain","#,
                    r#""set-cookie","a=1","set-cookie","b=2","x-a","z","#,
                    r#""cache-control","no-store","x-hidden","secret","#,
                    r#""content-length","1","connection","close"],"#
                )
            )
        );
        let declared = ["x-b", "x-a", "content-type", "x-absent"]
            .map(str::to_owned)
            .into_iter()
            .collect();
        assert_eq!(
            head_record(&server, "/headers", HeadForm::Canonical(declared)),
            format!(r#"{{"headers":["content-type","text/plain","x-a","z","x-b","1, 2"],{tail}"#)
        );
        let safelisted = varta_common::net::visible_response_headers(None);
        assert_eq!(
            head_record(&server, "/headers", HeadForm::Canonical(safelisted)),
            format!(
                r#"{{"headers":["cache-control","no-store","content-length","1","content-type","text/plain"],{tail}"#
            )
        );
    }

    #[test]
    fn request_ids_are_positive_integers() {
        assert_eq!(request_id(1.0), Some(1));
        for bad in [0.0, -1.0, 1.5, f64::NAN, f64::INFINITY] {
            assert_eq!(request_id(bad), None);
        }
    }
}
