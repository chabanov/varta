use super::*;
use crate::execution::Execution;
use crate::scheduler::{Ended, Until};
use hyper::body::Bytes;
use varta_common::http::{self, HttpResponse};

/// Where a streaming response goes: its head once, then its body chunks. A
/// failure after the head ends the body with [`StreamAborted`].
pub(super) struct Streaming {
    pub head: tokio::sync::oneshot::Sender<HttpResponse>,
    pub chunks: tokio::sync::mpsc::Sender<Result<Bytes, StreamAborted>>,
    /// A HEAD request: the head only, no pull of the body.
    pub bodyless: bool,
}

/// What a worker hands its adapter.
pub(super) enum Envelope {
    /// The whole request as the `VartaHttpRequest` JSON envelope.
    Text(Vec<u8>),
    /// A Web `Request` service's request head and its body still unread
    /// (`None` without one); the worker registers the body as a transfer
    /// and sets `head.body` to its id.
    Web {
        head: http::WebRequestHead,
        body: Option<hyper::body::Incoming>,
    },
}

/// A streaming body that ended without its end record.
#[derive(Debug)]
pub(super) struct StreamAborted;
impl std::fmt::Display for StreamAborted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("streaming response aborted")
    }
}
impl std::error::Error for StreamAborted {}

/// One contract v2 request on a task of its own (K7 H2): a fresh Store
/// instantiated from the server's linked module, its guest calls and waits
/// asynchronous, then its report.
pub(super) async fn execute(
    shared: &Shared,
    request: Envelope,
    cancellation: Option<crate::cancellation::Cancellation>,
    streaming: Option<Streaming>,
) -> Result<HttpResponse, RuntimeError> {
    let mut evidence = None;
    let response = execute_owned(shared, request, cancellation, streaming, &mut evidence).await;
    let outcome = match &response {
        Ok(response) => HttpRequestOutcome::Returned {
            status: response.status,
        },
        Err(RuntimeError::Execution(error))
            if error
                .downcast_ref::<crate::cancellation::Cancelled>()
                .is_some() =>
        {
            HttpRequestOutcome::Cancelled
        }
        Err(_) => HttpRequestOutcome::Failed,
    };
    let (budget, audit, audit_dropped) = evidence
        .map_or((None, Vec::new(), 0), |(budget, audit, dropped)| {
            (Some(budget), audit, dropped)
        });
    shared.sink.request(HttpRequestReport {
        sequence: 0,
        outcome,
        budget,
        audit,
        audit_dropped,
    });
    if let Err(error) = &response {
        shared.sink.diagnostic(error.to_string());
    }
    response
}

async fn execute_owned(
    shared: &Shared,
    request: Envelope,
    cancellation: Option<crate::cancellation::Cancellation>,
    streaming: Option<Streaming>,
    evidence: &mut Option<(crate::BudgetReport, Vec<crate::AuditEntry>, u64)>,
) -> Result<HttpResponse, RuntimeError> {
    let setup = &shared.request_setup;
    let stream = shared
        .limits
        .stream
        .filter(|_| shared.contract.streaming)
        .map(|limits| std::time::Duration::from_millis(limits.lifetime_ms));
    // A Web `Request` service's request is a sequence of turns, each with
    // fresh budgets and the per-turn deadline, which waits between turns
    // (for its body, timers or transfers) do not use: it lives at most the
    // request lifetime (plus a streaming response's). Otherwise a
    // streaming response lives at most its lifetime, each body chunk a turn
    // of its own (`stream`), and every other request is one bounded turn.
    let web = shared
        .limits
        .transport
        .filter(|_| shared.contract.web_request)
        .map(|transport| std::time::Duration::from_millis(transport.request_lifetime_ms));
    let mode = match (web, stream) {
        (Some(lifetime), stream) => crate::execution::Mode::Application {
            lifetime: Some(lifetime + stream.unwrap_or_default()),
        },
        (None, Some(lifetime)) => crate::execution::Mode::Application {
            lifetime: Some(lifetime),
        },
        (None, None) => crate::execution::Mode::Bounded,
    };
    let mut execution = Execution::with_setup(
        &shared.engine,
        crate::execution::Setup {
            requested: &shared.requested,
            ceiling: shared.ceiling.as_deref(),
            files: shared.files.clone(),
            budgets: setup.budgets,
            capped: setup.capped.clone(),
            renew: web.is_some(),
            mode,
            broker: setup.broker.setup(),
            task: true,
        },
    )?;
    let context = execution.store.data_mut();
    context.cancellation = cancellation;
    context.http_control = Some((shared.contract.bind.clone(), shared.stop.clone()));
    // Its transfers run on the server's runtime, progressing while it awaits.
    context.net.share_runtime(tokio::runtime::Handle::current());
    if let Some(lifetime) = web {
        context.idle_bound = Some(context.started + lifetime);
    }
    let mut instance = None;
    let response = match execution.store.data().check_cancelled() {
        Ok(()) => invoke(shared, &mut execution, request, streaming, &mut instance).await,
        Err(error) => Err(error.into()),
    }
    // A guest's resource trap reads as it does in an application.
    .map_err(|error| match instance.as_ref() {
        Some(instance) => crate::resources::classify(instance, &mut execution.store, error),
        None => error,
    });
    crate::turns::close(instance.as_ref(), &mut execution.store);
    let context = execution.store.data();
    let budget = context.turns.report(context.budgets, context.output_bytes);
    let (audit, dropped) = context.broker.audit_log();
    *evidence = Some((budget, audit, dropped));
    let logs = std::mem::take(&mut execution.store.data_mut().output.lines);
    // Response/error and lines own their Rust data. Release guest resources
    // before calling a trusted observer, which may apply backpressure.
    drop(execution);
    // V2 preserves output preceding failure even if the connection's service
    // future has gone away. The request worker owns delivery and its bounds.
    if let Err(error) = shared.sink.lines(logs) {
        shared.stop.shutdown();
        return Err(error);
    }
    response
}

/// The wire envelope of `request`. A Web `Request`'s body becomes a
/// transfer of the request's Store first, whose id the envelope carries.
fn envelope(
    shared: &Shared,
    execution: &mut Execution,
    request: Envelope,
) -> Result<Vec<u8>, RuntimeError> {
    let (mut head, body) = match request {
        Envelope::Text(bytes) => return Ok(bytes),
        Envelope::Web { head, body } => (head, body),
    };
    if let (Some(body), Some(transport)) = (body, shared.limits.transport) {
        let context = execution.store.data_mut();
        let deadline = context
            .idle_bound
            .ok_or_else(|| HttpError::Message("Web request without its lifetime".into()))?;
        head.body = context.net.inbound(crate::net::InboundBody {
            body,
            limit: transport.request_body_bytes,
            idle: std::time::Duration::from_millis(transport.body_idle_ms),
            deadline,
        })?;
    }
    serde_json::to_vec(&head).map_err(|_| HttpError::Message("request envelope".into()).into())
}

async fn invoke(
    shared: &Shared,
    execution: &mut Execution,
    request: Envelope,
    streaming: Option<Streaming>,
    started: &mut Option<wasmtime::Instance>,
) -> Result<HttpResponse, RuntimeError> {
    let instance = execution
        .instantiate_pre_async(&shared.module, &shared.linked)
        .await?;
    *started = Some(instance);
    let request = envelope(shared, execution, request)?;
    let memory = instance
        .get_memory(&mut execution.store, varta_common::abi::MEMORY_EXPORT)
        .ok_or_else(|| HttpError::Message("missing ABI memory".into()))?;
    if request.len() > abi::MAX_INPUT_BYTES {
        return Err(HttpError::Message("request envelope limit".into()).into());
    }
    // Bound the UTF-8 wire envelope first, then transcode for the v2 guest.
    // At most two guest bytes per input byte fit the reserved result region.
    let request = std::str::from_utf8(&request)
        .map_err(|_| HttpError::Message("request envelope UTF-8".into()))?
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    write_input(execution, &memory, &request)?;
    let copy = instance.get_typed_func::<(i32, i32), i64>(&mut execution.store, http::COPY)?;
    let input = copy
        .call_async(
            &mut execution.store,
            (abi::RESULT_BASE as i32, request.len() as i32),
        )
        .await?;
    copied(&memory, &execution.store, &request, input)?;
    let retain = instance.get_typed_func::<i64, i64>(&mut execution.store, http::RETAIN)?;
    let retained = retain.call_async(&mut execution.store, input).await?;
    if retained != input {
        return Err(HttpError::Message("input retention changed value".into()).into());
    }
    let mut result = instance
        .get_typed_func::<i64, i64>(&mut execution.store, &shared.contract.export)?
        .call_async(&mut execution.store, input)
        .await? as u64;
    crate::exceptions::finish(&instance, &mut execution.store)?;
    if shared.contract.asynchronous {
        let promise = result as i64;
        let retained = retain.call_async(&mut execution.store, promise).await?;
        if retained != promise {
            return Err(HttpError::Message("HTTP promise retention changed value".into()).into());
        }
        let state = instance.get_typed_func::<i64, f64>(&mut execution.store, http::STATE)?;
        settled(
            crate::scheduler::checkpoint_until_async(
                &instance,
                &mut execution.store,
                Until::Settled { state, promise },
            )
            .await?,
            "async HTTP handler rejected or invalid promise state",
            "async HTTP handler is pending without runnable work",
        )?;
        result = instance
            .get_typed_func::<i64, i64>(&mut execution.store, http::VALUE)?
            .call_async(&mut execution.store, promise)
            .await? as u64;
        crate::exceptions::finish(&instance, &mut execution.store)?;
        if shared.contract.streaming {
            let streaming = streaming
                .ok_or_else(|| HttpError::Message("streaming response without a sink".into()))?;
            return stream(
                shared,
                execution,
                (instance, memory),
                result as i64,
                streaming,
            )
            .await;
        }
    }
    if !shared.contract.asynchronous {
        let retained = retain
            .call_async(&mut execution.store, result as i64)
            .await?;
        if retained as u64 != result {
            return Err(HttpError::Message("HTTP response retention changed value".into()).into());
        }
        crate::scheduler::checkpoint_until_async(&instance, &mut execution.store, Until::Drained)
            .await?;
    }
    execution.check_deadline()?;
    let encoded = String::from_utf16(&guest_units(&memory, &execution.store, result)?)
        .map_err(|_| HttpError::Message("response envelope UTF-16".into()))?;
    if encoded.len() > abi::MAX_RESULT_BYTES {
        return Err(HttpError::Message("response envelope limit".into()).into());
    }
    parse_response(execution, encoded.as_bytes())
}

/// Contract v1 (frozen): one synchronous worker per request, never
/// cancelled. Its checked JSON parsing calls back into the guest from a host
/// import, which an asynchronous call cannot, so it keeps a blocking
/// thread; it instantiates from the same linked module. V1 publishes no
/// request reports and delivers output only with a response.
pub(super) fn execute_legacy(
    shared: &Shared,
    request: Vec<u8>,
) -> Result<HttpResponse, RuntimeError> {
    let response = legacy_owned(shared, request);
    if let Err(error) = &response {
        shared.sink.diagnostic(error.to_string());
    }
    response
}

fn legacy_owned(shared: &Shared, request: Vec<u8>) -> Result<HttpResponse, RuntimeError> {
    let setup = &shared.request_setup;
    let mut execution = Execution::with_setup(
        &shared.engine,
        crate::execution::Setup {
            requested: &shared.requested,
            ceiling: shared.ceiling.as_deref(),
            files: shared.files.clone(),
            budgets: setup.budgets,
            capped: setup.capped.clone(),
            renew: false,
            mode: crate::execution::Mode::Bounded,
            broker: setup.broker.setup(),
            task: false,
        },
    )?;
    execution.store.data_mut().http_control =
        Some((shared.contract.bind.clone(), shared.stop.clone()));
    let response = invoke_legacy(shared, &mut execution, &request);
    let logs = std::mem::take(&mut execution.store.data_mut().output.lines);
    drop(execution);
    if response.is_ok()
        && let Err(error) = shared.sink.lines(logs)
    {
        shared.stop.shutdown();
        return Err(error);
    }
    response
}

/// The v1 adapter: the UTF-8 envelope copied in, retained, handled.
fn invoke_legacy(
    shared: &Shared,
    execution: &mut Execution,
    request: &[u8],
) -> Result<HttpResponse, RuntimeError> {
    let instance = execution.instantiate_linked(&shared.module, &shared.linked)?;
    let memory = instance
        .get_memory(&mut execution.store, varta_common::abi::MEMORY_EXPORT)
        .ok_or_else(|| HttpError::Message("missing ABI memory".into()))?;
    if request.len() > abi::MAX_INPUT_BYTES {
        return Err(HttpError::Message("request envelope limit".into()).into());
    }
    write_input(execution, &memory, request)?;
    let input = instance
        .get_typed_func::<(i32, i32), i64>(&mut execution.store, http::COPY)?
        .call(
            &mut execution.store,
            (abi::RESULT_BASE as i32, request.len() as i32),
        )?;
    copied(&memory, &execution.store, request, input)?;
    let retained = instance
        .get_typed_func::<i64, i64>(&mut execution.store, http::RETAIN)?
        .call(&mut execution.store, input)?;
    if retained != input {
        return Err(HttpError::Message("input retention changed value".into()).into());
    }
    let result = instance
        .get_typed_func::<i64, i64>(&mut execution.store, &shared.contract.export)?
        .call(&mut execution.store, input)? as u64;
    execution.check_deadline()?;
    let bytes = guest_bytes(&memory, &execution.store, result)?.to_vec();
    parse_response(execution, &bytes)
}

/// Write the encoded request envelope into the reserved result region.
fn write_input(
    execution: &mut Execution,
    memory: &wasmtime::Memory,
    request: &[u8],
) -> Result<(), RuntimeError> {
    if request.len() > abi::MAX_RESULT_BYTES {
        return Err(HttpError::Message("encoded request envelope limit".into()).into());
    }
    memory
        .write(&mut execution.store, abi::RESULT_BASE, request)
        .map_err(wasmtime::Error::new)?;
    Ok(())
}

/// The adapter's copy (`input`, packed) must hold exactly `request`.
fn copied(
    memory: &wasmtime::Memory,
    store: &wasmtime::Store<crate::RuntimeContext>,
    request: &[u8],
    input: i64,
) -> Result<(), RuntimeError> {
    let packed = input as u64;
    let ptr = packed as u32 as usize;
    let len = (packed >> i32::BITS) as usize;
    if len != request.len()
        || ptr
            .checked_add(len)
            .and_then(|end| memory.data(store).get(ptr..end))
            != Some(request)
    {
        return Err(HttpError::Message("input copy changed length".into()).into());
    }
    Ok(())
}

/// The response envelope's schema, within the request deadline.
fn parse_response(execution: &Execution, bytes: &[u8]) -> Result<HttpResponse, RuntimeError> {
    let response = serde_json::from_slice::<HttpResponse>(bytes)
        .map_err(|_| HttpError::Message("response envelope schema or UTF-8".into()))?;
    execution.check_deadline()?;
    Ok(response)
}

/// The bytes of a packed guest string (`len << 32 | ptr`), bounded by
/// `abi::MAX_RESULT_BYTES`.
fn guest_bytes<'a>(
    memory: &wasmtime::Memory,
    store: &'a wasmtime::Store<crate::RuntimeContext>,
    packed: u64,
) -> Result<&'a [u8], RuntimeError> {
    let len = (packed >> i32::BITS) as usize;
    let ptr = packed as u32 as usize;
    if len > abi::MAX_RESULT_BYTES {
        return Err(HttpError::Message("response envelope limit".into()).into());
    }
    ptr.checked_add(len)
        .and_then(|end| memory.data(store).get(ptr..end))
        .ok_or_else(|| HttpError::Message("response range".into()).into())
}

/// The UTF-16 code units of a packed guest string.
fn guest_units(
    memory: &wasmtime::Memory,
    store: &wasmtime::Store<crate::RuntimeContext>,
    packed: u64,
) -> Result<Vec<u16>, RuntimeError> {
    let bytes = guest_bytes(memory, store, packed)?;
    if !bytes.len().is_multiple_of(size_of::<u16>()) {
        return Err(HttpError::Message("response UTF-16 length".into()).into());
    }
    Ok(bytes
        .as_chunks::<{ size_of::<u16>() }>()
        .0
        .iter()
        .map(|unit| u16::from_le_bytes(*unit))
        .collect())
}

/// Pull a streaming response: its head, then its body records, each pull
/// after the head a `stream` turn with fresh fuel, host calls and microtasks
/// and the idle bound as its deadline (within the response's lifetime).
/// `body` is the adapter's rooted record of the response's pull function.
/// The body ends at the end record, when the client is gone or the service
/// stops; a failure after the head aborts it.
async fn stream(
    shared: &Shared,
    execution: &mut Execution,
    (instance, memory): (wasmtime::Instance, wasmtime::Memory),
    body: i64,
    streaming: Streaming,
) -> Result<HttpResponse, RuntimeError> {
    let limits = shared
        .limits
        .stream
        .ok_or_else(|| HttpError::Message("streaming contract without stream limits".into()))?;
    let retained = instance
        .get_typed_func::<i64, i64>(&mut execution.store, http::RETAIN)?
        .call_async(&mut execution.store, body)
        .await?;
    if retained != body {
        return Err(HttpError::Message("HTTP body retention changed value".into()).into());
    }
    let Streaming {
        head,
        chunks,
        bodyless,
    } = streaming;
    let first = pull(execution, (&instance, &memory), body).await?;
    let response = serde_json::from_str::<HttpResponse>(
        &String::from_utf16(&first)
            .map_err(|_| HttpError::Message("response head UTF-16".into()))?,
    )
    .map_err(|_| HttpError::Message("response head schema".into()))?;
    if head.send(response.clone()).is_err()
        || bodyless
        || http::BODYLESS_STATUSES.contains(&response.status)
    {
        return Ok(response);
    }
    let result: Result<(), RuntimeError> = async {
        loop {
            if shared.stop.stopped() {
                return Ok(());
            }
            let idle = std::time::Duration::from_millis(limits.idle_ms);
            let context = execution.store.data_mut();
            context.budgets.deadline_per_turn = idle;
            // A Web `Request` service's pull waits at most the idle bound too.
            if context.idle_bound.is_some() {
                let bound = std::time::Instant::now() + idle;
                context.idle_bound = Some(context.lifetime.map_or(bound, |end| end.min(bound)));
            }
            crate::turns::close(Some(&instance), &mut execution.store);
            crate::turns::begin(&mut execution.store, crate::turns::TurnKind::Stream)?;
            let record = pull(execution, (&instance, &memory), body).await?;
            match record.split_first() {
                Some((&tag, _)) if tag == http::STREAM_END as u16 && record.len() == 1 => {
                    return Ok(());
                }
                Some((&tag, data)) if tag == http::STREAM_DATA as u16 => {
                    if data.len() > limits.chunk_bytes {
                        return Err(HttpError::Message("stream chunk limit".into()).into());
                    }
                    let bytes = data
                        .iter()
                        .map(|unit| u8::try_from(*unit))
                        .collect::<Result<Vec<u8>, _>>()
                        .map_err(|_| HttpError::Message("stream chunk byte".into()))?;
                    // The client is gone once its body is dropped.
                    if !bytes.is_empty() && chunks.send(Ok(Bytes::from(bytes))).await.is_err() {
                        return Ok(());
                    }
                }
                _ => return Err(HttpError::Message("stream record".into()).into()),
            }
        }
    }
    .await;
    if result.is_err() {
        let _ = chunks.send(Err(StreamAborted)).await;
    }
    result.map(|()| response)
}

/// An adapter promise's wait: fulfilled, or the error of a rejection or of
/// a loop that became idle while the promise was pending.
fn settled(ended: Ended, rejected: &str, pending: &str) -> Result<(), RuntimeError> {
    match ended {
        Ended::Complete => Ok(()),
        Ended::Rejected => Err(HttpError::Message(rejected.into()).into()),
        Ended::Idle => Err(HttpError::Message(pending.into()).into()),
    }
}

/// One pull: the code units of the record its promise fulfils with.
async fn pull(
    execution: &mut Execution,
    (instance, memory): (&wasmtime::Instance, &wasmtime::Memory),
    body: i64,
) -> Result<Vec<u16>, RuntimeError> {
    let promise = instance
        .get_typed_func::<i64, i64>(&mut execution.store, http::PULL)?
        .call_async(&mut execution.store, body)
        .await?;
    crate::exceptions::finish(instance, &mut execution.store)?;
    let state = instance.get_typed_func::<i64, f64>(&mut execution.store, http::CHUNK_STATE)?;
    settled(
        crate::scheduler::checkpoint_until_async(
            instance,
            &mut execution.store,
            Until::Settled { state, promise },
        )
        .await?,
        "streaming response body rejected",
        "streaming response body is pending without runnable work",
    )?;
    let value = instance
        .get_typed_func::<i64, i64>(&mut execution.store, http::CHUNK_VALUE)?
        .call_async(&mut execution.store, promise)
        .await? as u64;
    crate::exceptions::finish(instance, &mut execution.store)?;
    execution.check_deadline()?;
    guest_units(memory, &execution.store, value)
}
