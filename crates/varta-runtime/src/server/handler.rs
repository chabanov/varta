use super::*;
use crate::execution::Execution;
use varta_common::http::{self, HttpResponse};

pub(super) fn execute(
    shared: &Shared,
    request: Vec<u8>,
    cancellation: Option<crate::cancellation::Cancellation>,
) -> Result<HttpResponse, RuntimeError> {
    let mut evidence = None;
    let response = execute_owned(shared, request, cancellation, &mut evidence);
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
    evidence: &mut Option<(crate::BudgetReport, Vec<crate::AuditEntry>, u64)>,
) -> Result<HttpResponse, RuntimeError> {
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
        .and_then(|()| invoke(shared, &mut execution, request, &mut instance));
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
    let len = (result >> i32::BITS) as usize;
    let ptr = result as u32 as usize;
    if len > abi::MAX_RESULT_BYTES {
        return Err(HttpError::Message("response envelope limit".into()).into());
    }
    let bytes = ptr
        .checked_add(len)
        .and_then(|end| memory.data(&execution.store).get(ptr..end))
        .ok_or_else(|| HttpError::Message("response range".into()))?;
    let encoded;
    let bytes = if shared.contract.version == http::UTF16_VERSION {
        if !bytes.len().is_multiple_of(size_of::<u16>()) {
            return Err(HttpError::Message("response UTF-16 length".into()).into());
        }
        let units = bytes
            .as_chunks::<{ size_of::<u16>() }>()
            .0
            .iter()
            .map(|unit| u16::from_le_bytes(*unit))
            .collect::<Vec<_>>();
        encoded = String::from_utf16(&units)
            .map_err(|_| HttpError::Message("response envelope UTF-16".into()))?;
        if encoded.len() > abi::MAX_RESULT_BYTES {
            return Err(HttpError::Message("response envelope limit".into()).into());
        }
        encoded.as_bytes()
    } else {
        bytes
    };
    let response = serde_json::from_slice::<HttpResponse>(bytes)
        .map_err(|_| HttpError::Message("response envelope schema or UTF-8".into()))?;
    execution.check_deadline()?;
    Ok(response)
}
