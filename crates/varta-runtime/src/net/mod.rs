//! Asynchronous network requests of profile v37 (`net_request`), behind the
//! `fetch` prelude. See `varta_common::net` for the shared contract.
//!
//! A request is authorized against the exact `net.request` grant (and the
//! host ceiling) inside `net_request_start`, before anything is resolved or
//! connected; then its transfer becomes a task on a current-thread Tokio
//! runtime that only the event loop drives. The runtime keeps request ids,
//! completions and the promise serial each armed operation wakes, never a
//! guest reference: the guest's waiting frame roots everything it needs.
mod client;

use crate::scheduler::{self, Scheduler};
use crate::{RuntimeContext, host};
use client::ReadMode;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use varta_common::HostOperation;
use varta_common::net::{
    CompletionKind, DESCRIPTOR_BODY, DESCRIPTOR_BYTES, DESCRIPTOR_NO_BODY, DESCRIPTOR_SEPARATOR,
    DESCRIPTOR_VERSION, DESCRIPTOR_VERSION_BYTES, MAX_PENDING_REQUESTS, MAX_REQUEST_BYTES,
    MAX_REQUEST_HEADER_BYTES, MAX_REQUEST_HEADERS, NetError, REQUEST_CAPABILITY, RUNTIME_HEADERS,
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
}

/// A running transfer: its task and the channel that asks for the next chunk
/// (as text or as bytes).
struct Transfer {
    task: tokio::task::JoinHandle<()>,
    pull: mpsc::Sender<ReadMode>,
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
    /// A new request: a slot, then either an immediate completion (a denial,
    /// an invalid URL or request) or a transfer task.
    fn start(&mut self, outcome: Result<client::Plan, Completion>) -> Result<f64, wasmtime::Error> {
        if self.requests.len() >= MAX_PENDING_REQUESTS {
            return Err(wasmtime::Error::new(NetError::Budget));
        }
        self.next_id += 1;
        let id = self.next_id;
        let (phase, transfer) = match outcome {
            Err(completion) => (Phase::Ready(completion), None),
            Ok(plan) => (Phase::Armed, Some(self.spawn(id, plan)?)),
        };
        self.requests.insert(
            id,
            Request {
                phase,
                watcher: None,
                transfer,
            },
        );
        Ok(id as f64)
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
        let task = runtime.spawn(client::run(id, plan, events, pulls));
        Ok(Transfer { task, pull })
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
    /// were aborted or released are discarded.
    pub(crate) fn deliver(
        &mut self,
        scheduler: &mut Scheduler,
        event: Event,
    ) -> Result<(), wasmtime::Error> {
        let Some(request) = self.requests.get_mut(&event.id) else {
            return Ok(());
        };
        if !matches!(request.phase, Phase::Armed) {
            return Ok(());
        }
        if event.completion.kind.terminal() {
            request.transfer = None;
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
    fn read(&mut self, id: f64, mode: ReadMode) -> Result<(), NetError> {
        let (_, request) = self.request(id)?;
        if !matches!(request.phase, Phase::Idle) || request.watcher.is_some() {
            return Err(NetError::State);
        }
        let Some(transfer) = &request.transfer else {
            return Err(NetError::State);
        };
        // The capacity-1 channel is empty: every earlier pull was answered.
        transfer.pull.try_send(mode).map_err(|_| NetError::State)?;
        request.phase = Phase::Armed;
        Ok(())
    }
    /// Cancel the transfer. An armed (or completed but untaken) operation
    /// completes as aborted and wakes its waiter; an idle request is released.
    fn abort(&mut self, scheduler: &mut Scheduler, id: f64) -> Result<(), wasmtime::Error> {
        let Some(id) = request_id(id) else {
            return Ok(());
        };
        let Some(request) = self.requests.get_mut(&id) else {
            return Ok(());
        };
        request.transfer = None;
        if matches!(request.phase, Phase::Idle) {
            self.requests.remove(&id);
            return Ok(());
        }
        request.phase = Phase::Ready(Completion {
            kind: CompletionKind::Aborted,
            text: String::new(),
        });
        if let Some(serial) = request.watcher.take() {
            scheduler.wake(serial).map_err(wasmtime::Error::new)?;
        }
        Ok(())
    }
    /// A completion that already arrived, without driving the transfers.
    pub(crate) fn poll(&mut self) -> Option<Event> {
        self.events.as_mut()?.1.try_recv().ok()
    }
    /// Drive the transfers until one reports a completion or `until` passes.
    pub(crate) fn wait(&mut self, until: Instant) -> Option<Event> {
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

fn failed(text: impl Into<String>) -> Completion {
    Completion {
        kind: CompletionKind::Failed,
        text: text.into(),
    }
}

/// The parsed fields of a request descriptor (`varta_common::net`).
struct Descriptor<'a> {
    method: &'a str,
    url: &'a str,
    headers: Vec<(&'a str, &'a str)>,
    body: Option<Vec<u8>>,
}

/// The descriptor form an import accepts: version 1 (`net_request_start`,
/// text bodies) or version 2 (`net_request_start_v2`, v38, text or bytes).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Text,
    Bytes,
}
impl Form {
    fn version(self) -> &'static str {
        match self {
            Self::Text => DESCRIPTOR_VERSION,
            Self::Bytes => DESCRIPTOR_VERSION_BYTES,
        }
    }
}

/// Split a descriptor. A malformed structure is fatal: only a tampered or
/// miscompiled module can produce one.
fn descriptor(text: &str, form: Form) -> Result<Descriptor<'_>, NetError> {
    let mut fields = text.splitn(4, DESCRIPTOR_SEPARATOR);
    let mut field = || fields.next().ok_or(NetError::Descriptor);
    if field()? != form.version() {
        return Err(NetError::Descriptor);
    }
    let method = field()?;
    let url = field()?;
    let mut rest = field()?;
    let count: usize = split(&mut rest)?
        .parse()
        .map_err(|_| NetError::Descriptor)?;
    if count > MAX_REQUEST_HEADERS {
        return Err(NetError::Descriptor);
    }
    let mut headers = Vec::with_capacity(count);
    for _ in 0..count {
        let name = split(&mut rest)?;
        let value = split(&mut rest)?;
        headers.push((name, value));
    }
    let body = match rest.split_once(DESCRIPTOR_SEPARATOR) {
        Some((DESCRIPTOR_NO_BODY, "")) => None,
        Some((DESCRIPTOR_BODY, body)) => Some(body.as_bytes().to_vec()),
        // Version 2 bytes: each code unit is one byte.
        Some((DESCRIPTOR_BYTES, body)) if form == Form::Bytes => Some(
            body.chars()
                .map(|unit| u8::try_from(u32::from(unit)).ok())
                .collect::<Option<Vec<u8>>>()
                .ok_or(NetError::Descriptor)?,
        ),
        _ => return Err(NetError::Descriptor),
    };
    Ok(Descriptor {
        method,
        url,
        headers,
        body,
    })
}
fn split<'a>(rest: &mut &'a str) -> Result<&'a str, NetError> {
    let (field, tail) = rest
        .split_once(DESCRIPTOR_SEPARATOR)
        .ok_or(NetError::Descriptor)?;
    *rest = tail;
    Ok(field)
}

/// Check a request against the grants and build its transfer plan; every
/// refusal is an immediate completion, so nothing is resolved or connected.
fn plan(
    context: &mut RuntimeContext,
    request: &Descriptor<'_>,
) -> Result<client::Plan, Completion> {
    let method = hyper::Method::from_bytes(request.method.as_bytes())
        .ok()
        .filter(|_| varta_common::net::valid_method(request.method))
        .ok_or_else(|| failed(format!("invalid method: {}", request.method)))?;
    if url::Url::parse(request.url).is_err() {
        return Err(failed(format!("Invalid URL: {}", request.url)));
    }
    let url = match context
        .broker
        .request(&context.policy, request.url, request.method)
    {
        crate::permission::Decision::Granted(url) => url,
        crate::permission::Decision::Denied => {
            let text = format!(
                "<capability denied: {REQUEST_CAPABILITY}> ({} {})",
                request.method, request.url
            );
            // The denial stays observable in the host diagnostics, like every
            // other capability denial, while the guest gets a catchable
            // `NotAllowedError`.
            if context.emit(format!("varta log: {text}")).is_err() {
                return Err(failed("output budget exhausted"));
            }
            return Err(Completion {
                kind: CompletionKind::Denied,
                text,
            });
        }
    };
    if request.body.is_some() && matches!(method, hyper::Method::GET | hyper::Method::HEAD) {
        return Err(failed("a GET or HEAD request cannot have a body"));
    }
    let mut bytes = 0usize;
    let mut headers = Vec::with_capacity(request.headers.len());
    for (name, value) in &request.headers {
        bytes = bytes.saturating_add(name.len() + value.len());
        if bytes > MAX_REQUEST_HEADER_BYTES {
            return Err(failed("request headers are too large"));
        }
        if RUNTIME_HEADERS
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(name))
        {
            return Err(failed(format!("header {name} is set by the runtime")));
        }
        let name = hyper::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| failed(format!("invalid header name: {name}")))?;
        // Header values are ByteStrings: one byte per code unit.
        let value = value
            .chars()
            .map(|unit| u8::try_from(u32::from(unit)).ok())
            .collect::<Option<Vec<u8>>>()
            .and_then(|bytes| hyper::header::HeaderValue::from_bytes(&bytes).ok())
            .ok_or_else(|| failed(format!("invalid header value for {name}")))?;
        headers.push((name, value));
    }
    // Plain HTTP requests never build the TLS configuration.
    let tls = match url.scheme() {
        client::HTTPS => Some(context.net.tls().map_err(failed)?),
        _ => None,
    };
    Ok(client::Plan {
        url,
        method,
        headers,
        body: request.body.clone().map(hyper::body::Bytes::from),
        deadline: crate::turns::operation_deadline(context),
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
    form: Form,
) -> Result<f64, wasmtime::Error> {
    caller.data_mut().tick()?;
    let text = read_descriptor(caller, ptr, len).map_err(fatal)?;
    let request = descriptor(&text, form).map_err(fatal)?;
    let context = caller.data_mut();
    let outcome = plan(context, &request);
    context.net.start(outcome)
}

pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        HostOperation::NetRequestStart.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            start(&mut caller, ptr, len, Form::Text)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::NetRequestStartV2.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            start(&mut caller, ptr, len, Form::Bytes)
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
                .map_err(|error| wasmtime::Error::msg(error.message))
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::NetBodyRead.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| {
            deadline(caller.data())?;
            caller
                .data_mut()
                .net
                .read(id, ReadMode::Text)
                .map_err(fatal)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::NetBodyReadBytes.name(),
        |mut caller: Caller<'_, RuntimeContext>, id: f64| {
            deadline(caller.data())?;
            caller
                .data_mut()
                .net
                .read(id, ReadMode::Bytes)
                .map_err(fatal)
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
    let Some(event) = context.net.wait(until) else {
        return Ok(false);
    };
    context.net.deliver(&mut context.scheduler, event)?;
    scheduler::observed_wall_clock(context);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_split_into_bounded_fields() {
        let text = format!(
            "{DESCRIPTOR_VERSION}\nPOST\nhttp://127.0.0.1:1/x\n2\naccept\n*/*\nx-a\nb\n1\nline1\nline2"
        );
        let parsed = descriptor(&text, Form::Text).unwrap();
        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.url, "http://127.0.0.1:1/x");
        assert_eq!(parsed.headers, [("accept", "*/*"), ("x-a", "b")]);
        assert_eq!(parsed.body.as_deref(), Some(&b"line1\nline2"[..]));
        let get = format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\n0\n0\n");
        assert_eq!(descriptor(&get, Form::Text).unwrap().body, None);
        // Version 1 never carries bytes, and each import accepts only its
        // own version.
        let bytes = format!("{DESCRIPTOR_VERSION}\nPOST\nhttp://a.test/\n0\n2\nab");
        assert_eq!(
            descriptor(&bytes, Form::Text).err(),
            Some(NetError::Descriptor)
        );
        assert_eq!(
            descriptor(&text, Form::Bytes).err(),
            Some(NetError::Descriptor)
        );
        for bad in [
            String::new(),
            "varta-net-request-0\nGET\nhttp://a.test/\n0\n0\n".into(),
            format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\n1\nx\n0\n"),
            format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\nx\n0\n"),
            format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\n0\n2\n"),
            format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\n0\n0\nextra"),
            format!("{DESCRIPTOR_VERSION}\nGET\nhttp://a.test/\n65\n0\n"),
        ] {
            assert_eq!(
                descriptor(&bad, Form::Text).err(),
                Some(NetError::Descriptor),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn version_two_descriptors_carry_text_or_bytes() {
        let head = format!("{DESCRIPTOR_VERSION_BYTES}\nPOST\nhttp://a.test/\n0\n");
        let text = format!("{head}1\n\u{e9}");
        assert_eq!(
            descriptor(&text, Form::Bytes).unwrap().body.as_deref(),
            Some("\u{e9}".as_bytes())
        );
        let bytes = format!("{head}2\n\u{0}\u{e9}\u{ff}");
        assert_eq!(
            descriptor(&bytes, Form::Bytes).unwrap().body.as_deref(),
            Some(&[0x00, 0xE9, 0xFF][..])
        );
        // A code unit above 0xFF is no byte.
        let wide = format!("{head}2\n\u{100}");
        assert_eq!(
            descriptor(&wide, Form::Bytes).err(),
            Some(NetError::Descriptor)
        );
        let none = format!("{DESCRIPTOR_VERSION_BYTES}\nGET\nhttp://a.test/\n0\n0\n");
        assert_eq!(descriptor(&none, Form::Bytes).unwrap().body, None);
    }

    #[test]
    fn request_ids_are_positive_integers() {
        assert_eq!(request_id(1.0), Some(1));
        for bad in [0.0, -1.0, 1.5, f64::NAN, f64::INFINITY] {
            assert_eq!(request_id(bad), None);
        }
    }
}
