use super::*;
use crate::execution::Execution;
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

/// A streaming body that ended without its end record.
#[derive(Debug)]
pub(super) struct StreamAborted;
impl std::fmt::Display for StreamAborted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("streaming response aborted")
    }
}
impl std::error::Error for StreamAborted {}

pub(super) fn execute(
    shared: &Shared,
    request: Vec<u8>,
    cancellation: Option<crate::cancellation::Cancellation>,
    streaming: Option<Streaming>,
) -> Result<HttpResponse, RuntimeError> {
    let mut evidence = None;
    let response = execute_owned(shared, request, cancellation, streaming, &mut evidence);
    if shared.contract.version == http::UTF16_VERSION {
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
    }
    if let Err(error) = &response {
        shared.sink.diagnostic(error.to_string());
    }
    response
}

fn execute_owned(
    shared: &Shared,
    request: Vec<u8>,
    cancellation: Option<crate::cancellation::Cancellation>,
    streaming: Option<Streaming>,
    evidence: &mut Option<(crate::BudgetReport, Vec<crate::AuditEntry>, u64)>,
) -> Result<HttpResponse, RuntimeError> {
    let setup = &shared.request_setup;
    // A streaming response lives at most its lifetime; each body chunk is a
    // turn of its own (`stream`).
    let mode = match shared.contract.limits.stream {
        Some(limits) if shared.contract.streaming => crate::execution::Mode::Application {
            lifetime: Some(std::time::Duration::from_millis(limits.lifetime_ms)),
        },
        _ => crate::execution::Mode::Bounded,
    };
    let mut execution = Execution::with_setup(
        &shared.engine,
        crate::execution::Setup {
            requested: &shared.requested,
            ceiling: shared.ceiling.as_deref(),
            files: shared.files.clone(),
            budgets: setup.budgets,
            capped: setup.capped.clone(),
            renew: false,
            mode,
            broker: setup.broker.setup(),
        },
    )?;
    execution.store.data_mut().cancellation = cancellation;
    execution.store.data_mut().http_control =
        Some((shared.contract.bind.clone(), shared.stop.clone()));
    let mut instance = None;
    let response = execution
        .store
        .data()
        .check_cancelled()
        .map_err(RuntimeError::from)
        .and_then(|()| invoke(shared, &mut execution, request, streaming, &mut instance));
    if shared.contract.version == http::UTF16_VERSION {
        crate::turns::close(instance.as_ref(), &mut execution.store);
        let context = execution.store.data();
        let budget = context.turns.report(context.budgets, context.output_bytes);
        let (audit, dropped) = context.broker.audit_log();
        *evidence = Some((budget, audit, dropped));
    }
    let logs = std::mem::take(&mut execution.store.data_mut().output.lines);
    // Response/error and lines own their Rust data. Release guest resources
    // before calling a trusted observer, which may apply backpressure.
    drop(execution);
    // V2 preserves output preceding failure even if the connection's service
    // future has gone away. The request worker owns delivery and its bounds.
    if (response.is_ok() || shared.contract.version == http::UTF16_VERSION)
        && let Err(error) = shared.sink.lines(logs)
    {
        shared.stop.shutdown();
        return Err(error);
    }
    response
}

fn invoke(
    shared: &Shared,
    execution: &mut Execution,
    request: Vec<u8>,
    streaming: Option<Streaming>,
    started: &mut Option<wasmtime::Instance>,
) -> Result<HttpResponse, RuntimeError> {
    let instance = execution.instantiate_prepared(&shared.module)?;
    *started = Some(instance);
    let memory = instance
        .get_memory(&mut execution.store, varta_common::abi::MEMORY_EXPORT)
        .ok_or_else(|| HttpError::Message("missing ABI memory".into()))?;
    if request.len() > abi::MAX_INPUT_BYTES {
        return Err(HttpError::Message("request envelope limit".into()).into());
    }
    // Bound the UTF-8 wire envelope first, then transcode only for v2.
    // At most two guest bytes per input byte fit the reserved result region.
    let request = if shared.contract.version == http::UTF16_VERSION {
        std::str::from_utf8(&request)
            .map_err(|_| HttpError::Message("request envelope UTF-8".into()))?
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    } else {
        request
    };
    if request.len() > abi::MAX_RESULT_BYTES {
        return Err(HttpError::Message("encoded request envelope limit".into()).into());
    }
    memory
        .write(&mut execution.store, abi::RESULT_BASE, &request)
        .map_err(wasmtime::Error::new)?;
    let copy = instance.get_typed_func::<(i32, i32), i64>(&mut execution.store, http::COPY)?;
    let input = copy.call(
        &mut execution.store,
        (abi::RESULT_BASE as i32, request.len() as i32),
    )?;
    let packed = input as u64;
    let ptr = packed as u32 as usize;
    let len = (packed >> i32::BITS) as usize;
    if len != request.len()
        || ptr
            .checked_add(len)
            .and_then(|end| memory.data(&execution.store).get(ptr..end))
            != Some(request.as_slice())
    {
        return Err(HttpError::Message("input copy changed length".into()).into());
    }
    let retained = instance
        .get_typed_func::<i64, i64>(&mut execution.store, http::RETAIN)?
        .call(&mut execution.store, input)?;
    if retained != input {
        return Err(HttpError::Message("input retention changed value".into()).into());
    }
    let mut result = instance
        .get_typed_func::<i64, i64>(&mut execution.store, &shared.contract.export)?
        .call(&mut execution.store, input)? as u64;
    if shared.contract.version == http::UTF16_VERSION {
        crate::exceptions::finish(&instance, &mut execution.store)?;
    }
    if shared.contract.asynchronous {
        let promise = result as i64;
        let retained = instance
            .get_typed_func::<i64, i64>(&mut execution.store, http::RETAIN)?
            .call(&mut execution.store, promise)?;
        if retained != promise {
            return Err(HttpError::Message("HTTP promise retention changed value".into()).into());
        }
        let state = instance.get_typed_func::<i64, f64>(&mut execution.store, http::STATE)?;
        let completed =
            crate::scheduler::checkpoint_until(&instance, &mut execution.store, |_, store| {
                let state = state.call(store, promise)?;
                use varta_common::scheduler::PromiseState;
                if state == PromiseState::Fulfilled as i32 as f64 {
                    Ok(true)
                } else if state == PromiseState::Pending as i32 as f64 {
                    Ok(false)
                } else {
                    Err(HttpError::Message(
                        "async HTTP handler rejected or invalid promise state".into(),
                    )
                    .into())
                }
            })?;
        if !completed {
            return Err(HttpError::Message(
                "async HTTP handler is pending without runnable work".into(),
            )
            .into());
        }
        result = instance
            .get_typed_func::<i64, i64>(&mut execution.store, http::VALUE)?
            .call(&mut execution.store, promise)? as u64;
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
            );
        }
    }
    if !shared.contract.asynchronous && shared.contract.version == http::UTF16_VERSION {
        let retained = instance
            .get_typed_func::<i64, i64>(&mut execution.store, http::RETAIN)?
            .call(&mut execution.store, result as i64)?;
        if retained as u64 != result {
            return Err(HttpError::Message("HTTP response retention changed value".into()).into());
        }
        crate::scheduler::checkpoint_until(&instance, &mut execution.store, |_, _| Ok(true))?;
    }
    execution.check_deadline()?;
    let encoded;
    let bytes = if shared.contract.version == http::UTF16_VERSION {
        encoded = String::from_utf16(&guest_units(&memory, &execution.store, result)?)
            .map_err(|_| HttpError::Message("response envelope UTF-16".into()))?;
        if encoded.len() > abi::MAX_RESULT_BYTES {
            return Err(HttpError::Message("response envelope limit".into()).into());
        }
        encoded.as_bytes()
    } else {
        guest_bytes(&memory, &execution.store, result)?
    };
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
fn stream(
    shared: &Shared,
    execution: &mut Execution,
    (instance, memory): (wasmtime::Instance, wasmtime::Memory),
    body: i64,
    streaming: Streaming,
) -> Result<HttpResponse, RuntimeError> {
    let limits = shared
        .contract
        .limits
        .stream
        .ok_or_else(|| HttpError::Message("streaming contract without stream limits".into()))?;
    let retained = instance
        .get_typed_func::<i64, i64>(&mut execution.store, http::RETAIN)?
        .call(&mut execution.store, body)?;
    if retained != body {
        return Err(HttpError::Message("HTTP body retention changed value".into()).into());
    }
    let Streaming {
        head,
        chunks,
        bodyless,
    } = streaming;
    let first = pull(execution, (&instance, &memory), body)?;
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
    let result = (|| -> Result<(), RuntimeError> {
        loop {
            if shared.stop.stopped() {
                return Ok(());
            }
            execution.store.data_mut().budgets.deadline_per_turn =
                std::time::Duration::from_millis(limits.idle_ms);
            crate::turns::close(Some(&instance), &mut execution.store);
            crate::turns::begin(&mut execution.store, crate::turns::TurnKind::Stream)?;
            let record = pull(execution, (&instance, &memory), body)?;
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
                    if !bytes.is_empty() && chunks.blocking_send(Ok(Bytes::from(bytes))).is_err() {
                        return Ok(());
                    }
                }
                _ => return Err(HttpError::Message("stream record".into()).into()),
            }
        }
    })();
    if result.is_err() {
        let _ = chunks.blocking_send(Err(StreamAborted));
    }
    result.map(|()| response)
}

/// One pull: the code units of the record its promise fulfils with.
fn pull(
    execution: &mut Execution,
    (instance, memory): (&wasmtime::Instance, &wasmtime::Memory),
    body: i64,
) -> Result<Vec<u16>, RuntimeError> {
    let promise = instance
        .get_typed_func::<i64, i64>(&mut execution.store, http::PULL)?
        .call(&mut execution.store, body)?;
    crate::exceptions::finish(instance, &mut execution.store)?;
    let state = instance.get_typed_func::<i64, f64>(&mut execution.store, http::CHUNK_STATE)?;
    let completed =
        crate::scheduler::checkpoint_until(instance, &mut execution.store, |_, store| {
            use varta_common::scheduler::PromiseState;
            let state = state.call(store, promise)?;
            if state == PromiseState::Fulfilled as i32 as f64 {
                Ok(true)
            } else if state == PromiseState::Pending as i32 as f64 {
                Ok(false)
            } else {
                Err(HttpError::Message("streaming response body rejected".into()).into())
            }
        })?;
    if !completed {
        return Err(HttpError::Message(
            "streaming response body is pending without runnable work".into(),
        )
        .into());
    }
    let value = instance
        .get_typed_func::<i64, i64>(&mut execution.store, http::CHUNK_VALUE)?
        .call(&mut execution.store, promise)? as u64;
    crate::exceptions::finish(instance, &mut execution.store)?;
    execution.check_deadline()?;
    guest_units(memory, &execution.store, value)
}
