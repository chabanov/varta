use super::*;
use crate::execution::Execution;
use varta_common::http::{self, HttpResponse};

pub(super) fn execute(
    shared: &Shared,
    request: Vec<u8>,
) -> Result<(HttpResponse, Vec<String>), RuntimeError> {
    let mut execution = Execution::with_files(
        &shared.engine,
        &shared.requested,
        shared.ceiling.as_deref(),
        shared.files.clone(),
    )?;
    execution.store.data_mut().http_control =
        Some((shared.contract.bind.clone(), shared.stop.clone()));
    let instance = execution.instantiate_prepared(&shared.module)?;
    let memory = instance
        .get_memory(&mut execution.store, "memory")
        .ok_or_else(|| HttpError::Message("missing ABI memory".into()))?;
    if request.len() > abi::MAX_INPUT_BYTES {
        return Err(HttpError::Message("request envelope limit".into()).into());
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
    let result = instance
        .get_typed_func::<i64, i64>(&mut execution.store, &shared.contract.export)?
        .call(&mut execution.store, input)? as u64;
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
    let response = serde_json::from_slice::<HttpResponse>(bytes)
        .map_err(|_| HttpError::Message("response envelope schema or UTF-8".into()))?;
    Ok((
        response,
        std::mem::take(&mut execution.store.data_mut().output.lines),
    ))
}
